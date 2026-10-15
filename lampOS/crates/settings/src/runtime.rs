//! Bounded socket handling and one persistence owner. Disk and Bus I/O never
//! run under the current-value lock or in the socket loop.
use crate::{
    engine::Engine,
    model::{Mutation, Snapshot},
    protocol::{MAX_REPLY_BYTES, MAX_REQUEST_BYTES, Request, refusal},
    store::Storage,
};
use lamp_bus::Bus;
use lamp_msgs::{Diagnostic, KeyValue, Level};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

pub const MAX_CLIENTS: usize = 8;
pub const MUTATION_QUEUE: usize = 4;
pub const READ_DEADLINE: Duration = Duration::from_millis(300);
pub const MUTATION_DEADLINE: Duration = Duration::from_millis(1100);
pub const HEALTHY_SAVE_TARGET: Duration = Duration::from_millis(250);
pub const SHUTDOWN_DEADLINE: Duration = Duration::from_millis(500);
const TICK: Duration = Duration::from_millis(5);

#[derive(Clone, Debug)]
pub struct Config {
    /// Must be inside the private, exclusively owned store directory.
    pub socket: PathBuf,
    /// Explicitly opt in to health reports; absence creates no Bus files.
    pub bus_root: Option<PathBuf>,
    pub run_for: Option<Duration>,
}

struct View {
    snapshot: Snapshot,
    reliable: bool,
    pending: Option<Instant>,
    completed: u64,
    last_save_ms: u64,
    frontend: Instant,
}

impl View {
    fn status(&self) -> Value {
        json!({"ok":true,"ready":self.reliable,"store_id":self.snapshot.store_id,
            "revision":self.snapshot.revision,"reliable":self.reliable,
            "application":"desired_only","pending":self.pending.is_some(),
            "pending_ms":self.pending.map(|at| millis(at.elapsed())),
            "completed":self.completed,"last_save_ms":self.last_save_ms})
    }
    fn read(&self) -> Value {
        json!({"ok":true,"state":self.snapshot,"reliable":self.reliable,
            "application":"desired_only","pending":self.pending.is_some()})
    }
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

struct Work {
    id: u64,
    mutation: Mutation,
    deadline: Instant,
}
struct Answer {
    id: u64,
    body: Value,
}

fn owner<S: Storage>(
    mut engine: Engine<S>,
    incoming: Receiver<Work>,
    outgoing: SyncSender<Answer>,
    view: Arc<Mutex<View>>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Acquire) {
        let work = match incoming.recv_timeout(TICK) {
            Ok(work) => work,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        let body = if stop.load(Ordering::Acquire) || Instant::now() >= work.deadline {
            refusal("expired_before_start", "The queued mutation did not start.")
        } else {
            let started = Instant::now();
            view.lock().unwrap().pending = Some(started);
            let result = engine.mutate(work.mutation);
            let mut current = view.lock().unwrap();
            current.snapshot = engine.snapshot();
            current.reliable = !engine.poisoned();
            current.pending = None;
            current.completed = current.completed.saturating_add(1);
            current.last_save_ms = millis(started.elapsed());
            match result {
                Ok(commit) => json!({"ok":true,"commit":commit,"application":"desired_only"}),
                Err(error) => refusal(error.code, &error.message),
            }
        };
        // A vanished or slow client cannot hold the persistence owner. If the
        // response is lost, its stable request ID and current revision resolve it.
        let _ = outgoing.try_send(Answer { id: work.id, body });
    }
}

struct Binding {
    path: PathBuf,
    dev: u64,
    ino: u64,
}
impl Drop for Binding {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|m| m.file_type().is_socket() && m.dev() == self.dev && m.ino() == self.ino)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// The FileStore lifetime lock is already held. Preserve any live socket,
/// including one made by a program that ignores that lock.
fn bind(path: &Path) -> io::Result<(UnixListener, Binding)> {
    crate::client::validate_socket(path).map_err(io::Error::other)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("socket has no parent"))?;
    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.is_dir()
        || metadata.mode() & 0o7777 != 0o700
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || parent.canonicalize()? != parent
    {
        return Err(io::Error::other(
            "socket parent must be canonical, owned and mode 0700",
        ));
    }
    match fs::symlink_metadata(path) {
        Ok(m)
            if m.file_type().is_socket()
                && m.uid() == metadata.uid()
                && m.mode() & 0o7777 == 0o600 =>
        {
            let fd = rustix::net::socket(
                rustix::net::AddressFamily::UNIX,
                rustix::net::SocketType::STREAM,
                None,
            )?;
            rustix::io::fcntl_setfd(&fd, rustix::io::FdFlags::CLOEXEC)?;
            let probe = UnixStream::from(fd);
            probe.set_nonblocking(true)?;
            let address = rustix::net::SocketAddrUnix::new(path)?;
            match rustix::net::connect(&probe, &address) {
                Err(error) if error == rustix::io::Errno::CONNREFUSED => fs::remove_file(path)?,
                Err(error) => return Err(error.into()),
                Ok(()) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "settings socket is live",
                    ));
                }
            }
        }
        Ok(_) => {
            return Err(io::Error::other(
                "settings socket path is not an owned private socket",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let listener = UnixListener::bind(path)?;
    let m = fs::symlink_metadata(path)?;
    let binding = Binding {
        path: path.into(),
        dev: m.dev(),
        ino: m.ino(),
    };
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    Ok((listener, binding))
}

struct Client {
    id: u64,
    stream: UnixStream,
    input: Vec<u8>,
    output: Option<Vec<u8>>,
    written: usize,
    waiting: bool,
    deadline: Instant,
    done: bool,
}
impl Client {
    fn respond(&mut self, body: Value) {
        let mut output = serde_json::to_vec(&body).unwrap();
        if output.len() > MAX_REPLY_BYTES {
            output = serde_json::to_vec(&refusal("too_long", "Reply exceeds its bound.")).unwrap();
        }
        output.push(b'\n');
        self.output = Some(output);
        self.written = 0;
        self.waiting = false;
        self.deadline = Instant::now() + READ_DEADLINE;
    }

    fn step(&mut self, view: &Arc<Mutex<View>>, work: &SyncSender<Work>) {
        if Instant::now() >= self.deadline {
            if self.waiting {
                self.respond(refusal("indeterminate", "No confirmed result. Read and retry the same request ID and guard; do not assume cancellation."));
            } else {
                self.done = true;
                return;
            }
        }
        if let Some(output) = &self.output {
            match self.stream.write(&output[self.written..]) {
                Ok(0) => self.done = true,
                Ok(n) => {
                    self.written += n;
                    self.done = self.written == output.len();
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => self.done = true,
            }
            return;
        }
        if self.waiting {
            return;
        }
        let mut bytes = [0u8; 1024];
        // One bounded read per client per turn; a flooding client cannot own
        // this loop. The protocol accepts one line, then closes the stream.
        match self.stream.read(&mut bytes) {
            Ok(0) => self.done = true,
            Ok(n) => {
                let end = bytes[..n].iter().position(|b| *b == b'\n');
                let chunk = &bytes[..end.unwrap_or(n)];
                if chunk.len() > MAX_REQUEST_BYTES.saturating_sub(self.input.len()) {
                    self.respond(refusal("too_long", "Request exceeds its bound."));
                    return;
                }
                self.input.extend_from_slice(chunk);
                let Some(end) = end else {
                    return;
                };
                if bytes[end + 1..n].iter().any(|b| !b.is_ascii_whitespace()) {
                    self.respond(refusal("bad_request", "Only one request per connection."));
                    return;
                }
                match Request::parse(&self.input) {
                    Err(code) => self.respond(refusal(code, "Invalid settings request.")),
                    Ok(Request::Status {}) => self.respond(view.lock().unwrap().status()),
                    Ok(Request::Read {}) => self.respond(view.lock().unwrap().read()),
                    Ok(Request::Set { mutation }) => {
                        self.deadline = Instant::now() + MUTATION_DEADLINE;
                        match work.try_send(Work {
                            id: self.id,
                            mutation,
                            deadline: self.deadline,
                        }) {
                            Ok(()) => self.waiting = true,
                            Err(TrySendError::Full(_)) => self.respond(refusal(
                                "busy",
                                "Mutation queue is full; this request was not accepted.",
                            )),
                            Err(TrySendError::Disconnected(_)) => self.respond(refusal(
                                "unavailable",
                                "Settings owner has stopped; this request was not accepted.",
                            )),
                        }
                    }
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(_) => self.done = true,
        }
    }
}

fn report(root: PathBuf, view: Arc<Mutex<View>>, stop: Arc<AtomicBool>) {
    let Ok(bus) = Bus::new(root) else {
        return;
    };
    let Ok(mut publisher) =
        bus.publisher_declared(Diagnostic::TOPIC, &Diagnostic::declaration(), "settings")
    else {
        return;
    };
    let epoch = Instant::now();
    while !stop.load(Ordering::Acquire) {
        let health = {
            let current = view.lock().unwrap();
            let (level, message) = if !current.reliable {
                (
                    Level::Error,
                    "storage result uncertain; restart and recover",
                )
            } else if current.frontend.elapsed() > Duration::from_millis(1500) {
                (
                    Level::Error,
                    "settings request loop has stopped progressing",
                )
            } else if current
                .pending
                .is_some_and(|at| at.elapsed() > HEALTHY_SAVE_TARGET)
            {
                (Level::Warn, "settings save exceeds the 250 ms target")
            } else {
                (
                    Level::Ok,
                    "desired settings available; consumer application is separate",
                )
            };
            Diagnostic {
                name: "settings".into(),
                level,
                message: message.into(),
                values: [
                    ("revision".into(), current.snapshot.revision.to_string()),
                    ("completed".into(), current.completed.to_string()),
                    ("last_save_ms".into(), current.last_save_ms.to_string()),
                ]
                .map(|(key, value)| KeyValue { key, value })
                .to_vec(),
            }
        };
        let _ = publisher.publish_json(
            epoch.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
            &health,
        );
        for _ in 0..100 {
            if stop.load(Ordering::Acquire) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

struct StopOnDrop(Arc<AtomicBool>);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Finished {
    pub enabled: bool,
    pub owner_stopped: bool,
    pub reporter_stopped: bool,
    pub reliable: bool,
    pub revision: u64,
}

/// Start only after the caller explicitly enabled Settings and recovered its
/// private store. The binary passes a signal flag; tests can pass their own.
/// No actuator, consumer setting, or global logging configuration is changed.
pub fn serve<S: Storage>(
    engine: Engine<S>,
    config: Config,
    stop: Arc<AtomicBool>,
) -> io::Result<Finished> {
    let (listener, _binding) = bind(&config.socket)?;
    let view = Arc::new(Mutex::new(View {
        snapshot: engine.snapshot(),
        reliable: !engine.poisoned(),
        pending: None,
        completed: 0,
        last_save_ms: 0,
        frontend: Instant::now(),
    }));
    let local_stop = Arc::new(AtomicBool::new(false));
    let _stop_on_exit = StopOnDrop(local_stop.clone());
    let (work_tx, work_rx) = mpsc::sync_channel(MUTATION_QUEUE);
    let (answer_tx, answer_rx) = mpsc::sync_channel(MAX_CLIENTS);
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let owner_view = view.clone();
    let owner_stop = local_stop.clone();
    let owner = thread::Builder::new()
        .name("settings-save".into())
        .spawn(move || {
            owner(engine, work_rx, answer_tx, owner_view, owner_stop);
            let _ = done_tx.try_send(());
        })?;
    let (report_tx, report_rx) = mpsc::sync_channel(1);
    let reporter = config
        .bus_root
        .map(|root| {
            let view = view.clone();
            let stop = local_stop.clone();
            thread::Builder::new()
                .name("settings-health".into())
                .spawn(move || {
                    report(root, view, stop);
                    let _ = report_tx.try_send(());
                })
        })
        .transpose()?;
    let until = config.run_for.map(|duration| Instant::now() + duration);
    let mut next_id = 0u64;
    let mut clients: Vec<Client> = Vec::new();
    while !stop.load(Ordering::Acquire) && until.is_none_or(|end| Instant::now() < end) {
        if owner.is_finished() {
            view.lock().unwrap().reliable = false;
        }
        for _ in 0..MAX_CLIENTS {
            if clients.len() == MAX_CLIENTS {
                break;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(true)?;
                    next_id = next_id
                        .checked_add(1)
                        .ok_or_else(|| io::Error::other("client identity exhausted"))?;
                    clients.push(Client {
                        id: next_id,
                        stream,
                        input: Vec::new(),
                        output: None,
                        written: 0,
                        waiting: false,
                        deadline: Instant::now() + READ_DEADLINE,
                        done: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        for _ in 0..MAX_CLIENTS {
            let Ok(answer) = answer_rx.try_recv() else {
                break;
            };
            if let Some(client) = clients.iter_mut().find(|c| c.id == answer.id && c.waiting) {
                client.respond(answer.body);
            }
        }
        for client in &mut clients {
            client.step(&view, &work_tx);
        }
        clients.retain(|client| !client.done);
        view.lock().unwrap().frontend = Instant::now();
        thread::sleep(TICK);
    }
    local_stop.store(true, Ordering::Release);
    drop(work_tx);
    // A filesystem call cannot be forcibly cancelled. A timeout here says so;
    // it never pretends a writer stopped or that its pending save rolled back.
    let owner_stopped = done_rx.recv_timeout(SHUTDOWN_DEADLINE).is_ok();
    if owner_stopped {
        let _ = owner.join();
    }
    let reporter_stopped =
        reporter.is_none() || report_rx.recv_timeout(Duration::from_millis(50)).is_ok();
    if reporter_stopped && let Some(reporter) = reporter {
        let _ = reporter.join();
    }
    let current = view.lock().unwrap();
    Ok(Finished {
        enabled: true,
        owner_stopped,
        reporter_stopped,
        reliable: current.reliable && owner_stopped,
        revision: current.snapshot.revision,
    })
}
