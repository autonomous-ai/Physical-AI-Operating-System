//! Bounded local sockets, one durable owner and one independent Harness worker.
use crate::{
    engine::{Clock as ClockSample, Engine, Error as EngineError, LaunchJob, ObservationJob},
    harness::{Cli, CliConfig, Launch, LaunchOutcome, Listed, Observation, Target},
    progress::{Reads, Tracker},
    protocol::{MAX_LINE_BYTES, MAX_REPLY_BYTES, Request},
    store::Store,
};
use lamp_bus::{Bus, Publisher};
use lamp_msgs::{Diagnostic, Level, TaskStatus};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

pub const READ_DEADLINE: Duration = Duration::from_millis(300);
pub const REQUEST_DEADLINE: Duration = Duration::from_millis(1100);
pub const CLIENT_DEADLINE: Duration = Duration::from_millis(1200);
pub const STALE_AFTER: Duration = Duration::from_millis(1500);
pub const WORKERS: usize = 4;
pub const CLIENT_QUEUE: usize = 4;
pub const REQUEST_QUEUE: usize = 16;
const TICK: Duration = Duration::from_millis(10);

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn refusal(code: &str, message: &str) -> Value {
    json!({"ok":false,"error":code,"message":message})
}

pub trait Clock: Send + 'static {
    fn sample(&self) -> io::Result<ClockSample>;
}
pub struct SystemClock {
    boot_id: String,
}
impl SystemClock {
    pub fn new(boot_id: Option<&str>) -> io::Result<Self> {
        let boot_id = match boot_id {
            Some(value) => value.to_owned(),
            None if cfg!(target_os = "linux") => {
                fs::read_to_string("/proc/sys/kernel/random/boot_id")?
                    .trim()
                    .into()
            }
            None => return Err(invalid("supply --boot-id on this development host")),
        };
        if boot_id.is_empty()
            || boot_id.len() > 80
            || !boot_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(invalid(
                "boot identity must be 1..80 ASCII letters, digits, dash or underscore",
            ));
        }
        Ok(Self { boot_id })
    }
}
impl Clock for SystemClock {
    fn sample(&self) -> io::Result<ClockSample> {
        #[cfg(target_os = "linux")]
        let id = rustix::time::ClockId::Boottime;
        #[cfg(not(target_os = "linux"))]
        let id = rustix::time::ClockId::Monotonic;
        let now = rustix::time::clock_gettime(id);
        let seconds = u64::try_from(now.tv_sec).map_err(|_| invalid("negative boot clock"))?;
        let nanos = u64::try_from(now.tv_nsec).map_err(|_| invalid("negative boot clock"))?;
        let monotonic_ms = seconds
            .checked_mul(1000)
            .and_then(|s| s.checked_add(nanos / 1_000_000))
            .ok_or_else(|| invalid("boot clock overflow"))?;
        Ok(ClockSample {
            boot_id: self.boot_id.clone(),
            monotonic_ms,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub enabled: bool,
    pub dir: PathBuf,
    pub socket: PathBuf,
    pub voice_socket: PathBuf,
    pub bus_root: PathBuf,
    pub targets: BTreeMap<String, Target>,
    pub run_for: Option<Duration>,
    /// Look at each task handed over on its own until its agent has
    /// finished or asks something (on unless `serve --no-watch`, [`Engine::watch`]).
    pub watch: bool,
    /// Read how each task being watched is going with one `harness ps` a
    /// machine, on the watch's schedule, and say so on `tasks/status`
    /// (`serve --progress`, [`crate::progress`]). Off unless set.
    pub progress: bool,
}

/// An operator-owned file selects targets; a model supplies only an alias.
/// Configuration is read once. Existing tasks retain their original target.
pub fn read_targets(path: &Path) -> io::Result<BTreeMap<String, Target>> {
    fn unique_targets<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<String, Target>, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = BTreeMap<String, Target>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("unique target aliases")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut access: M,
            ) -> Result<Self::Value, M::Error> {
                let mut values = BTreeMap::new();
                while let Some((name, target)) = access.next_entry::<String, Target>()? {
                    if values.insert(name, target).is_some() {
                        return Err(serde::de::Error::custom("duplicate target alias"));
                    }
                    if values.len() > 8 {
                        return Err(serde::de::Error::custom("too many targets"));
                    }
                }
                Ok(values)
            }
        }
        deserializer.deserialize_map(Visitor)
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Document {
        version: u32,
        #[serde(deserialize_with = "unique_targets")]
        targets: BTreeMap<String, Target>,
    }
    if !path.is_absolute() {
        return Err(invalid("targets path must be absolute"));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o7777 != 0o600
        || metadata.len() > 16_384
    {
        return Err(invalid(
            "targets must be an owned unlinked 0600 regular file at most 16384 bytes",
        ));
    }
    let mut bytes = Vec::new();
    file.take(16_385).read_to_end(&mut bytes)?;
    if bytes.len() > 16_384 {
        return Err(invalid("targets file grew beyond its bound"));
    }
    let document: Document =
        serde_json::from_slice(&bytes).map_err(|_| invalid("invalid targets JSON"))?;
    if document.version != 1 || document.targets.is_empty() || document.targets.len() > 8 {
        return Err(invalid("targets requires version 1 and 1..8 aliases"));
    }
    for (name, target) in &document.targets {
        if name.is_empty()
            || name.len() > 40
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(invalid("invalid target alias"));
        }
        target
            .validate()
            .map_err(|_| invalid("invalid configured target"))?;
    }
    Ok(document.targets)
}

pub trait Adapter: Send + 'static {
    fn launch(&self, launch: &Launch) -> LaunchOutcome;
    fn observe(&self, machine_id: &str, agent_id: &str) -> Result<Observation, String>;
    /// Every harness on one machine (`harness ps`), for progress. Asked only
    /// with `serve --progress`; an adapter without it says so.
    fn status(&self, machine_id: &str) -> Result<Vec<Listed>, String> {
        let _ = machine_id;
        Err("unsupported".into())
    }
}
impl Adapter for Cli {
    fn launch(&self, launch: &Launch) -> LaunchOutcome {
        Cli::launch(self, launch)
    }
    fn observe(&self, machine_id: &str, agent_id: &str) -> Result<Observation, String> {
        Cli::observe(self, machine_id, agent_id)
    }
    fn status(&self, machine_id: &str) -> Result<Vec<Listed>, String> {
        Cli::status(self, machine_id)
    }
}

struct Work {
    request: Request,
    trusted: bool,
    deadline: Instant,
    response: SyncSender<Result<Value, EngineError>>,
}
#[derive(Clone)]
struct Report {
    updated: Instant,
    summary: Value,
    requests: u64,
    cli_busy: bool,
    /// With `--progress`: the newest status and its number, to publish
    /// once, and how the reads have gone.
    status: Option<(u64, Arc<TaskStatus>)>,
    reads: Option<Reads>,
}
enum Job {
    Launch(LaunchJob),
    Observe(ObservationJob),
    Status(String),
}
enum Outcome {
    Launch(u64, LaunchOutcome),
    Observe(ObservationJob, Result<Observation, String>),
    Status(String, Result<Vec<Listed>, String>),
}
struct Binding {
    path: PathBuf,
    dev: u64,
    ino: u64,
    _lock: File,
}

struct StopOnDrop(Arc<AtomicBool>);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
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

fn bind(path: &Path) -> io::Result<(UnixListener, Binding)> {
    if !path.is_absolute() || path.as_os_str().len() > lamp_bus::MOST_SOCKET_PATH_BYTES {
        return Err(invalid(
            "socket path must be absolute and at most 103 bytes",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| invalid("socket needs a parent"))?;
    if !parent.exists() {
        fs::DirBuilder::new().mode(0o700).create(parent)?;
    }
    let parent_meta = fs::symlink_metadata(parent)?;
    if !parent_meta.is_dir()
        || parent_meta.permissions().mode() & 0o077 != 0
        || parent_meta.uid() != rustix::process::geteuid().as_raw()
    {
        return Err(invalid(
            "socket parent must be a private directory owned by this user",
        ));
    }
    let lock_path = path.with_extension("lock");
    if let Ok(m) = fs::symlink_metadata(&lock_path)
        && (!m.is_file()
            || m.nlink() != 1
            || m.uid() != parent_meta.uid()
            || m.permissions().mode() & 0o077 != 0)
    {
        return Err(invalid("unsafe socket lock"));
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(lock_path)?;
    lock.try_lock().map_err(io::Error::other)?;
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_socket() && m.uid() == parent_meta.uid() => {
            // A lock protects cooperating owners; a live non-cooperating socket
            // is preserved too. Private parents exclude other users' races.
            let descriptor = rustix::net::socket(
                rustix::net::AddressFamily::UNIX,
                rustix::net::SocketType::STREAM,
                None,
            )?;
            rustix::io::fcntl_setfd(&descriptor, rustix::io::FdFlags::CLOEXEC)?;
            let probe = UnixStream::from(descriptor);
            probe.set_nonblocking(true)?;
            let address = rustix::net::SocketAddrUnix::new(path)?;
            match rustix::net::connect(&probe, &address).map_err(io::Error::from) {
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "socket already answers",
                    ));
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                    ) =>
                {
                    fs::remove_file(path)?
                }
                Err(e) => return Err(e),
            }
        }
        Ok(_) => return Err(invalid("existing socket path is not an owned socket")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let listener = UnixListener::bind(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let metadata = fs::symlink_metadata(path)?;
    Ok((
        listener,
        Binding {
            path: path.into(),
            dev: metadata.dev(),
            ino: metadata.ino(),
            _lock: lock,
        },
    ))
}

fn owner<C: Clock>(
    mut engine: Engine<Store>,
    clock: C,
    incoming: Receiver<Work>,
    (jobs, outcomes): (SyncSender<Job>, Receiver<Outcome>),
    current: Arc<Mutex<Report>>,
    stop: Arc<AtomicBool>,
    (watch, progress): (bool, bool),
) -> io::Result<()> {
    let mut requests = 0u64;
    let mut busy = false;
    // Progress lives here, beside the engine, and only in memory: what an
    // agent says while it works is not part of the durable record.
    let mut tracker = progress.then(Tracker::default);
    let mut status: Option<(u64, Arc<TaskStatus>)> = None;
    let mut status_began = Instant::now();
    while !stop.load(Ordering::Acquire) {
        if let Ok(outcome) = outcomes.try_recv() {
            let now = clock.sample()?;
            match outcome {
                Outcome::Launch(id, result) => engine.finish_launch(id, result, &now),
                Outcome::Observe(job, Ok(value)) => engine.finish_observation(&job, value, &now),
                Outcome::Observe(job, Err(code)) => engine.observation_failed(&job, &code, &now),
                Outcome::Status(machine, listed) => {
                    if let Some(tracker) = tracker.as_mut() {
                        let took = status_began.elapsed().as_millis();
                        tracker.read(
                            &machine,
                            listed,
                            &engine.watched(&now),
                            unix_ms(),
                            u64::try_from(took).unwrap_or(u64::MAX),
                        );
                    }
                    Ok(())
                }
            }
            .map_err(io::Error::other)?;
            busy = false;
        }
        match incoming.recv_timeout(TICK) {
            Ok(work) => {
                if Instant::now() >= work.deadline {
                    let _ = work.response.try_send(Err(EngineError {
                        code: "timeout",
                        message: "The queued request expired before it started.",
                    }));
                } else if let (Request::GetProgress { id }, Some(tracker)) =
                    (&work.request, tracker.as_ref())
                {
                    let answer = match tracker.words(*id) {
                        Some(progress) => engine.identified(json!({"id":id,"progress":progress})),
                        None => Err(EngineError {
                            code: "not_found",
                            message: "No task with that ID is being read for progress.",
                        }),
                    };
                    requests = requests.saturating_add(1);
                    let _ = work.response.try_send(answer);
                } else {
                    let answer = engine.handle(&work.request, &clock.sample()?, work.trusted);
                    requests = requests.saturating_add(1);
                    let _ = work.response.try_send(answer);
                    if engine.failed() {
                        return Err(io::Error::other("Tasks storage failed; restart required"));
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if !busy && !stop.load(Ordering::Acquire) {
            let now = clock.sample()?;
            let next = match engine.begin_launch(&now).map_err(io::Error::other)? {
                Some(job) => Some(Job::Launch(job)),
                None => {
                    if watch {
                        engine.watch(&now).map_err(io::Error::other)?;
                    }
                    let observe = engine
                        .begin_observation(&now)
                        .map_err(io::Error::other)?
                        .map(Job::Observe);
                    // One `ps` a machine, after launches and reads: it is a
                    // read only and takes nothing from them.
                    observe.or_else(|| {
                        let tracker = tracker.as_mut()?;
                        let machine = tracker.due(&engine.watched(&now), now.monotonic_ms)?;
                        tracker.asked(&machine, now.monotonic_ms);
                        status_began = Instant::now();
                        Some(Job::Status(machine))
                    })
                }
            };
            if let Some(job) = next {
                // The durable attempt precedes this handoff. If this queue or
                // worker fails, reopening records uncertainty without a resend.
                jobs.try_send(job).map_err(|_| {
                    io::Error::other("Tasks CLI worker unavailable after durable handoff")
                })?;
                busy = true;
            }
        }
        if let Some(tracker) = tracker.as_mut()
            && let Some(fresh) = tracker.take(engine.store_id())
        {
            let number = status.as_ref().map_or(1, |(number, _)| number + 1);
            status = Some((number, Arc::new(fresh)));
        }
        *current
            .lock()
            .map_err(|_| io::Error::other("Tasks report lock poisoned"))? = Report {
            updated: Instant::now(),
            summary: engine.summary(),
            requests,
            cli_busy: busy,
            status: status.clone(),
            reads: tracker.as_ref().map(Tracker::reads),
        };
    }
    Ok(())
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "Tasks client deadline"))
}

fn wait_ready(stream: &UnixStream, events: PollFlags, deadline: Instant) -> io::Result<()> {
    loop {
        let timeout = Timespec::try_from(remaining(deadline)?).map_err(io::Error::other)?;
        let mut descriptors = [PollFd::new(stream, events)];
        match poll(&mut descriptors, Some(&timeout)) {
            Ok(0) => continue,
            Ok(_) => {
                remaining(deadline)?;
                let returned = descriptors[0].revents();
                if returned.contains(PollFlags::NVAL) {
                    return Err(invalid("invalid client descriptor"));
                }
                if returned.intersects(events | PollFlags::HUP | PollFlags::ERR) {
                    return Ok(());
                }
            }
            Err(e) if e == rustix::io::Errno::INTR => {}
            Err(e) => return Err(e.into()),
        }
    }
}

fn read_line(stream: &mut UnixStream, bound: usize, deadline: Instant) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        remaining(deadline)?;
        match stream.read(&mut buffer) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Tasks message needs a newline",
                ));
            }
            Ok(n) => {
                let end = buffer[..n].iter().position(|b| *b == b'\n');
                let part = &buffer[..end.unwrap_or(n)];
                if bytes.len() + part.len() > bound {
                    return Err(invalid("message exceeds its byte bound"));
                }
                bytes.extend_from_slice(part);
                if let Some(end) = end {
                    if end + 1 != n {
                        return Err(invalid("one message per connection"));
                    }
                    return Ok(bytes);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                wait_ready(stream, PollFlags::IN, deadline)?
            }
            Err(e) => return Err(e),
        }
    }
}

fn write_bytes(stream: &mut UnixStream, bytes: &[u8], deadline: Instant) -> io::Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        remaining(deadline)?;
        match stream.write(&bytes[offset..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "Tasks client closed",
                ));
            }
            Ok(n) => offset += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                wait_ready(stream, PollFlags::OUT, deadline)?
            }
            Err(e) => return Err(e),
        }
    }
    remaining(deadline)?;
    Ok(())
}

fn serve_client(
    mut stream: UnixStream,
    accepted: Instant,
    trusted: bool,
    owner: &SyncSender<Work>,
) {
    if stream.set_nonblocking(true).is_err() {
        return;
    }
    let reply = match read_line(&mut stream, MAX_LINE_BYTES, accepted + READ_DEADLINE) {
        Ok(line) => match Request::parse(&line) {
            Ok(request) => {
                let (tx, rx) = mpsc::sync_channel(1);
                let deadline = accepted + REQUEST_DEADLINE;
                match owner.try_send(Work {
                    request,
                    trusted,
                    deadline,
                    response: tx,
                }) {
                    Ok(()) => match remaining(deadline)
                        .and_then(|left| rx.recv_timeout(left).map_err(io::Error::other))
                    {
                        Ok(Ok(value)) => value,
                        Ok(Err(error)) => refusal(error.code, error.message),
                        Err(_) => refusal(
                            "uncertain",
                            "No receipt arrived. Read status; retry only the exact guarded request because it may have committed.",
                        ),
                    },
                    Err(TrySendError::Full(_)) => refusal(
                        "busy",
                        "Tasks request queue is full; this request did not start.",
                    ),
                    Err(TrySendError::Disconnected(_)) => refusal(
                        "unavailable",
                        "Tasks owner is unavailable; this request did not start.",
                    ),
                }
            }
            Err(_) => refusal(
                "bad_request",
                "Send one bounded JSON object with recognized fields.",
            ),
        },
        Err(e) if e.kind() == io::ErrorKind::InvalidInput => {
            refusal("bad_request", "Send one JSON line of at most 8192 bytes.")
        }
        Err(_) => refusal(
            "incomplete",
            "A complete request did not arrive within 300 ms.",
        ),
    };
    if let Ok(mut bytes) = serde_json::to_vec(&reply) {
        if bytes.len() > MAX_REPLY_BYTES {
            bytes = b"{\"ok\":false,\"error\":\"reply_bound\"}".to_vec();
        }
        bytes.push(b'\n');
        let _ = write_bytes(&mut stream, &bytes, accepted + CLIENT_DEADLINE);
    }
}

/// Bounded reference client used by host checks and trusted local consumers.
/// A timeout is not proof that a mutation failed to commit.
pub fn request(path: &Path, value: &Value, limit: Duration) -> io::Result<Value> {
    let deadline = Instant::now()
        .checked_add(limit)
        .ok_or_else(|| invalid("client deadline overflow"))?;
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_LINE_BYTES {
        return Err(invalid("request exceeds 8192 bytes"));
    }
    bytes.push(b'\n');
    let descriptor = rustix::net::socket(
        rustix::net::AddressFamily::UNIX,
        rustix::net::SocketType::STREAM,
        None,
    )?;
    rustix::io::fcntl_setfd(&descriptor, rustix::io::FdFlags::CLOEXEC)?;
    let mut stream = UnixStream::from(descriptor);
    stream.set_nonblocking(true)?;
    let address = rustix::net::SocketAddrUnix::new(path)?;
    // Unix nonblocking connect reports a full accept queue rather than waiting.
    // Retrying connect is left to the caller and never resends a mutation here.
    rustix::net::connect(&stream, &address)?;
    write_bytes(&mut stream, &bytes, deadline)?;
    let line = read_line(&mut stream, MAX_REPLY_BYTES, deadline)?;
    serde_json::from_slice(&line).map_err(io::Error::other)
}

fn random_id() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn publish<T: serde::Serialize>(out: &mut Publisher, value: &T, errors: &mut u64) {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let at_us = (now.tv_sec.max(0) as u64)
        .saturating_mul(1_000_000)
        .saturating_add(now.tv_nsec.max(0) as u64 / 1000);
    if out.publish_json(at_us, value).is_err() {
        *errors = errors.saturating_add(1);
    }
}

/// Production adapter. CLI configuration and environment are inspected only
/// after the default-off check. Harness owns account/session provisioning.
pub fn serve<C: Clock>(
    config: Config,
    clock: C,
    executable: PathBuf,
    as_person: bool,
    stop: Arc<AtomicBool>,
) -> io::Result<()> {
    let cancelled = stop.clone();
    serve_with_adapter(config, clock, stop, move |session| {
        let mut cli = CliConfig::new(executable);
        cli.cancel = Some(cancelled);
        cli.as_person = as_person;
        Cli::new(cli, session.into())
    })
}

/// Injection boundary for scripted host tests. Implementations must bound and
/// cancel their local work; the service cannot forcibly stop a Rust thread.
pub fn serve_with_adapter<C: Clock, A: Adapter>(
    config: Config,
    clock: C,
    stop: Arc<AtomicBool>,
    make_adapter: impl FnOnce(&str) -> io::Result<A>,
) -> io::Result<()> {
    if !config.enabled {
        return Ok(());
    }
    if config.socket == config.voice_socket {
        return Err(invalid("public and trusted sockets must differ"));
    }
    let began = Instant::now();
    let sample = clock.sample()?;
    let engine = Engine::open(
        Store::open(&config.dir)?,
        config.targets,
        &random_id()?,
        &random_id()?,
        &sample,
    )
    .map_err(io::Error::other)?;
    let adapter = make_adapter(engine.store_id())?;
    let current = Arc::new(Mutex::new(Report {
        updated: Instant::now(),
        summary: engine.summary(),
        requests: 0,
        cli_busy: false,
        status: None,
        reads: None,
    }));
    let (public, _public_binding) = bind(&config.socket)?;
    let (trusted, _trusted_binding) = bind(&config.voice_socket)?;
    let bus = Bus::new(&config.bus_root)?;
    let mut diagnostic =
        bus.publisher_declared(Diagnostic::TOPIC, &Diagnostic::declaration(), "tasks")?;
    // Without `--progress` the topic is not touched at all.
    let mut status_out = if config.progress {
        Some(bus.publisher_declared(TaskStatus::TOPIC, &TaskStatus::declaration(), "tasks")?)
    } else {
        None
    };
    let mut status_published = 0u64;
    let _stopping = StopOnDrop(stop.clone());
    let (jobs_tx, jobs_rx) = mpsc::sync_channel(1);
    let (outcomes_tx, outcomes_rx) = mpsc::sync_channel(1);
    let cli_stop = stop.clone();
    let cli = thread::Builder::new()
        .name("tasks-harness".into())
        .spawn(move || {
            while !cli_stop.load(Ordering::Acquire) {
                match jobs_rx.recv_timeout(TICK) {
                    Ok(job) => {
                        let result = match job {
                            Job::Launch(job) => {
                                Outcome::Launch(job.task_id, adapter.launch(&job.launch))
                            }
                            Job::Observe(job) => {
                                let result = adapter.observe(&job.machine_id, &job.agent_id);
                                Outcome::Observe(job, result)
                            }
                            Job::Status(machine) => {
                                let result = adapter.status(&machine);
                                Outcome::Status(machine, result)
                            }
                        };
                        if outcomes_tx.try_send(result).is_err() {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })?;
    let (owner_tx, owner_rx) = mpsc::sync_channel(REQUEST_QUEUE);
    let owner_stop = stop.clone();
    let switches = (config.watch, config.progress);
    let report = current.clone();
    let owner = thread::Builder::new()
        .name("tasks-store".into())
        .spawn(move || {
            owner(
                engine,
                clock,
                owner_rx,
                (jobs_tx, outcomes_rx),
                report,
                owner_stop,
                switches,
            )
        })?;
    let (clients_tx, clients_rx) = mpsc::sync_channel::<(UnixStream, Instant, bool)>(CLIENT_QUEUE);
    let clients_rx = Arc::new(Mutex::new(clients_rx));
    let rejected = Arc::new(AtomicU64::new(0));
    let mut workers = Vec::new();
    for number in 0..WORKERS {
        let incoming = clients_rx.clone();
        let writer = owner_tx.clone();
        let stopped = stop.clone();
        workers.push(
            thread::Builder::new()
                .name(format!("tasks-client-{number}"))
                .spawn(move || {
                    while !stopped.load(Ordering::Acquire) {
                        if let Some((client, accepted, trusted)) =
                            incoming.lock().ok().and_then(|r| r.try_recv().ok())
                        {
                            serve_client(client, accepted, trusted, &writer);
                        } else {
                            thread::sleep(Duration::from_millis(5));
                        }
                    }
                })?,
        );
    }
    let mut last_publish: Option<Instant> = None;
    let mut last_ready = None;
    let mut errors = 0u64;
    let mut failure = None;
    while !stop.load(Ordering::Acquire)
        && config.run_for.is_none_or(|limit| began.elapsed() < limit)
    {
        for (listener, trusted) in [(&public, false), (&trusted, true)] {
            for _ in 0..WORKERS + CLIENT_QUEUE {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if clients_tx
                            .try_send((stream, Instant::now(), trusted))
                            .is_err()
                        {
                            rejected.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => {
                        failure = Some(e);
                        break;
                    }
                }
            }
        }
        if let Ok(locked) = current.try_lock() {
            let report = locked.clone();
            drop(locked);
            if let (Some(out), Some((number, status))) = (status_out.as_mut(), &report.status)
                && *number > status_published
            {
                publish(out, status.as_ref(), &mut errors);
                status_published = *number;
            }
            let ready = report.updated.elapsed() <= STALE_AFTER
                && !owner.is_finished()
                && !cli.is_finished();
            if last_publish.is_none_or(|last| last.elapsed() >= Duration::from_secs(1))
                || last_ready != Some(ready)
            {
                let level = if !ready {
                    Level::Error
                } else if errors > 0 {
                    Level::Warn
                } else {
                    Level::Ok
                };
                let message = if !ready {
                    "Tasks owner or CLI worker is unavailable"
                } else if errors > 0 {
                    "Tasks has bus publication failures"
                } else {
                    ""
                };
                let mut value = Diagnostic::new(level, "tasks", message)
                    .with("store_ready", ready)
                    .with("cli_busy", report.cli_busy)
                    .with("requests", report.requests)
                    .with("publish_errors", errors)
                    .with("clients_refused", rejected.load(Ordering::Relaxed))
                    .with("completion_supported", false);
                for field in [
                    "retained",
                    "queued",
                    "attempting",
                    "accepted",
                    "pending",
                    "uncertain",
                    "observations",
                    "speech_spoken",
                    "speech_uncertain",
                ] {
                    if let Some(count) = report.summary[field].as_u64() {
                        value = value.with(field, count);
                    }
                }
                // Only with `--progress`: without it the report is as it was.
                if let Some(reads) = report.reads {
                    value = value
                        .with("status_reads", reads.reads)
                        .with("status_failures", reads.failures)
                        .with("status_last_ms", reads.last_ms)
                        .with("status_most_ms", reads.most_ms)
                        .with("status_published", status_published);
                }
                publish(&mut diagnostic, &value, &mut errors);
                // The status topic promises no period: it says it is alive.
                if let Some(out) = status_out.as_mut()
                    && out.alive().is_err()
                {
                    errors = errors.saturating_add(1);
                }
                last_publish = Some(Instant::now());
                last_ready = Some(ready);
            }
        }
        if owner.is_finished() || cli.is_finished() {
            failure = Some(io::Error::other("Tasks worker stopped"));
        }
        if failure.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    stop.store(true, Ordering::Release);
    drop(clients_tx);
    drop(owner_tx);
    for worker in workers {
        let _ = worker.join();
    }
    let shutdown = Instant::now();
    while (!owner.is_finished() || !cli.is_finished())
        && shutdown.elapsed() < Duration::from_millis(300)
    {
        thread::sleep(Duration::from_millis(5));
    }
    if !owner.is_finished() || !cli.is_finished() {
        return Err(io::Error::other(
            "Tasks worker did not stop; exit this process before restarting",
        ));
    }
    cli.join()
        .map_err(|_| io::Error::other("Tasks CLI worker panicked"))?;
    owner
        .join()
        .map_err(|_| io::Error::other("Tasks store worker panicked"))??;
    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn root() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("tsk-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in("/private/tmp")
            .or_else(|_| {
                tempfile::Builder::new()
                    .prefix("tsk-")
                    .permissions(fs::Permissions::from_mode(0o700))
                    .tempdir()
            })
            .unwrap()
    }
    fn targets() -> Value {
        json!({"version":1,"targets":{"coding":{"machine_id":"11111111111111111111111111111111","project":"project","engine":"codex"}}})
    }
    fn write_targets(path: &Path, value: &Value) {
        fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn configuration_is_private_bounded_strict_and_has_explicit_targets() {
        let dir = root();
        let path = dir.path().join("targets.json");
        write_targets(&path, &targets());
        let value = read_targets(&path).unwrap();
        assert_eq!(value.len(), 1);
        assert_eq!(value["coding"].project, "project");
        for mode in [0o644, 0o660, 0o1600] {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            assert!(read_targets(&path).is_err());
        }
        write_targets(&path, &targets());
        let link = dir.path().join("linked");
        symlink(&path, &link).unwrap();
        assert!(read_targets(&link).is_err());
        let hard = dir.path().join("hard");
        fs::hard_link(&path, &hard).unwrap();
        assert!(read_targets(&path).is_err());
        fs::remove_file(hard).unwrap();
        assert!(read_targets(Path::new("targets.json")).is_err());
        fs::write(&path, vec![b' '; 16_385]).unwrap();
        assert!(read_targets(&path).is_err());
    }

    #[test]
    fn configuration_never_silently_selects_the_last_duplicate_alias() {
        let dir = root();
        let path = dir.path().join("targets.json");
        let target = targets()["targets"]["coding"].clone();
        fs::write(
            &path,
            format!(r#"{{"version":1,"targets":{{"coding":{target},"coding":{target}}}}}"#),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(read_targets(&path).is_err());
        for value in [
            json!({"version":2,"targets":{}}),
            json!({"version":1,"targets":{}}),
            json!({"version":1,"targets":{"bad alias":target}}),
            json!({"version":1,"targets":{"coding":target},"trusted":true}),
        ] {
            write_targets(&path, &value);
            assert!(read_targets(&path).is_err());
        }
    }

    #[test]
    fn binding_preserves_foreign_files_and_live_sockets() {
        let dir = root();
        let path = dir.path().join("socket");
        fs::write(&path, "preserve").unwrap();
        assert!(bind(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "preserve");
        fs::remove_file(&path).unwrap();
        let listener = UnixListener::bind(&path).unwrap();
        assert!(bind(&path).is_err());
        assert!(path.exists());
        drop(listener);
        let (listener, binding) = bind(&path).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        assert!(bind(&path).is_err());
        drop(listener);
        drop(binding);
        assert!(!path.exists());
    }

    #[test]
    fn binding_cleanup_does_not_delete_a_replacement() {
        let dir = root();
        let path = dir.path().join("socket");
        let (_listener, binding) = bind(&path).unwrap();
        fs::remove_file(&path).unwrap();
        fs::write(&path, "replacement").unwrap();
        drop(binding);
        assert_eq!(fs::read_to_string(path).unwrap(), "replacement");
    }

    #[test]
    fn disabled_service_does_not_read_clock_adapter_or_storage() {
        struct Never;
        impl Clock for Never {
            fn sample(&self) -> io::Result<ClockSample> {
                panic!("clock read while disabled")
            }
        }
        impl Adapter for Never {
            fn launch(&self, _: &Launch) -> LaunchOutcome {
                panic!("launch while disabled")
            }
            fn observe(&self, _: &str, _: &str) -> Result<Observation, String> {
                panic!("observe while disabled")
            }
        }
        let dir = root();
        let config = Config {
            enabled: false,
            dir: dir.path().join("data"),
            socket: dir.path().join("public"),
            voice_socket: dir.path().join("voice"),
            bus_root: dir.path().join("bus"),
            targets: BTreeMap::new(),
            run_for: None,
            watch: false,
            progress: false,
        };
        serve_with_adapter::<_, Never>(config, Never, Arc::new(AtomicBool::new(false)), |_| {
            panic!("adapter configured while disabled")
        })
        .unwrap();
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn line_reader_drains_buffered_bytes_after_peer_close() {
        let (mut sender, mut reader) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        let payload = vec![b'x'; 4096];
        sender.write_all(&payload).unwrap();
        sender.write_all(b"\n").unwrap();
        drop(sender);
        assert_eq!(
            read_line(&mut reader, 4096, Instant::now() + Duration::from_secs(1)).unwrap(),
            payload
        );
    }

    #[test]
    fn fragmented_line_and_absolute_silent_deadline_are_bounded() {
        let (mut sender, mut reader) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        let writer = thread::spawn(move || {
            sender.write_all(b"a").unwrap();
            thread::sleep(Duration::from_millis(10));
            sender.write_all(b"b\n").unwrap();
        });
        assert_eq!(
            read_line(&mut reader, 2, Instant::now() + Duration::from_secs(1)).unwrap(),
            b"ab"
        );
        writer.join().unwrap();
        let (_sender, mut reader) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        let start = Instant::now();
        assert_eq!(
            read_line(&mut reader, 2, start + Duration::from_millis(30))
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn newline_and_byte_bounds_precede_parsing() {
        for (bytes, bound) in [(b"abc\n".as_slice(), 2), (b"a\nb\n", 20), (b"abc", 2)] {
            let (mut sender, mut reader) = UnixStream::pair().unwrap();
            reader.set_nonblocking(true).unwrap();
            sender.write_all(bytes).unwrap();
            drop(sender);
            assert!(
                read_line(&mut reader, bound, Instant::now() + Duration::from_secs(1)).is_err()
            );
        }
    }

    #[test]
    fn output_to_a_nonreading_client_has_one_deadline() {
        let (mut sender, _reader) = UnixStream::pair().unwrap();
        sender.set_nonblocking(true).unwrap();
        let start = Instant::now();
        assert_eq!(
            write_bytes(
                &mut sender,
                &vec![b'x'; 2 * 1024 * 1024],
                start + Duration::from_millis(30)
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn request_rejects_oversized_input_before_connecting() {
        let value = json!({"text":"x".repeat(MAX_LINE_BYTES)});
        assert_eq!(
            request(
                Path::new("/no-tasks-socket"),
                &value,
                Duration::from_secs(1)
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
