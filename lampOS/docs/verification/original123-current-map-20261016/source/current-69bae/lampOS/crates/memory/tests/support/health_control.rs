// Explicit parent control for the real Memory fixture child. A Checked receipt
// describes the reporter boundary only; callers still read actual reports,
// readiness, log bytes and Child::try_wait themselves.
#[path = "../../src/fixture_protocol.rs"]
pub mod protocol;

use protocol::{Bootstrap, Channel, Checked, Connected, Outcome, Step};
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
};
use std::{
    fs::{self, File, Permissions},
    io::{self, Read, Write},
    os::{
        fd::AsFd,
        unix::{
            fs::{MetadataExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

const PERIOD_US: u64 = lamp_memory::health::REPORT_EVERY.as_micros() as u64;
const WINDOW_US: u64 = 8_000_000;
const FILE_POLL: Duration = Duration::from_millis(1);

/// This changes only stdin to the one-frame bootstrap pipe. The command keeps
/// its original application arguments, environment, stdout and regular-file
/// stderr. On success, the caller owns the actual Child independently of control.
pub fn start(
    command: &mut Command,
    private_root: &Path,
    log_path: &Path,
) -> io::Result<(Child, Controller)> {
    start_until(command, private_root, log_path, host_deadline()?)
}

fn start_until(
    command: &mut Command,
    private_root: &Path,
    log_path: &Path,
    deadline: Instant,
) -> io::Result<(Child, Controller)> {
    protocol::remaining(deadline)?;
    let directory = fs::symlink_metadata(private_root)?;
    if !private_root.is_absolute()
        || !directory.is_dir()
        || directory.permissions().mode() & 0o777 != 0o700
    {
        return Err(invalid(
            "memory fixture requires a private absolute directory",
        ));
    }
    let log = LogBarrier::open(log_path)?;
    let socket = SocketPath::bind(private_root.join("health-control.sock"))?;
    let mut child = StartupChild(Some(command.stdin(Stdio::piped()).spawn()?));
    let nonce = format!("P{}C{}", std::process::id(), child.get_mut()?.id());
    let bootstrap = Bootstrap {
        version: protocol::VERSION,
        control: socket.path.clone(),
        nonce: nonce.clone(),
    };
    bootstrap.validate()?;
    let mut bytes = Vec::new();
    protocol::write_frame(&mut bytes, &bootstrap)?;
    {
        let mut stdin = child
            .get_mut()?
            .stdin
            .take()
            .ok_or_else(|| invalid("missing memory fixture bootstrap pipe"))?;
        write_pipe_until(&mut stdin, &bytes, deadline)?;
    }
    // Closing stdin after exactly one frame is part of the explicit bootstrap.
    let stream = accept_until(&socket.listener, deadline)?;
    let controller = Controller::connected(Channel::new(stream)?, nonce, log, deadline)?;
    // The private listener is no longer needed. It does not represent readiness.
    drop(socket);
    Ok((child.take()?, controller))
}

/// An operation's original logical start and absolute host deadline never move.
/// A window belongs to exactly one controller and cannot be reused on another.
pub struct Window {
    owner: Rc<()>,
    started_us: u64,
    through_us: u64,
    deadline: Instant,
}

pub struct Controller {
    channel: Channel,
    nonce: String,
    log: LogBarrier,
    owner: Rc<()>,
    origin: Instant,
    cadence: Instant,
    at_us: u64,
    round: u64,
    checked: Option<Checked>,
    failure: Option<String>,
}

impl Controller {
    fn connected(
        mut channel: Channel,
        nonce: String,
        log: LogBarrier,
        deadline: Instant,
    ) -> io::Result<Self> {
        if !protocol::valid_nonce(&nonce) {
            return Err(invalid("invalid parent memory fixture nonce"));
        }
        let connected: Connected = channel.read_until(deadline)?;
        if connected.version != protocol::VERSION || connected.nonce != nonce {
            return Err(invalid("wrong memory fixture Connected version or nonce"));
        }
        protocol::remaining(deadline)?;
        let origin = Instant::now();
        Ok(Self {
            channel,
            nonce,
            log,
            owner: Rc::new(()),
            origin,
            cadence: origin,
            at_us: 0,
            round: 0,
            checked: None,
            failure: None,
        })
    }

    /// Pure read: no work, time advance, publication or control exchange.
    pub fn cadence_now(&self) -> Instant {
        self.cadence
    }

    pub fn window(&self) -> io::Result<Window> {
        self.ensure_open()?;
        Ok(Window {
            owner: self.owner.clone(),
            started_us: self.at_us,
            through_us: self
                .at_us
                .checked_add(WINDOW_US)
                .ok_or_else(|| invalid("memory fixture logical window overflow"))?,
            deadline: host_deadline()?,
        })
    }

    /// The first settle at zero requires the real registered reporter worker's
    /// positive checked wait. Connected alone cannot satisfy it.
    pub fn settle(&mut self, window: &Window) -> io::Result<()> {
        self.operation(|this| this.step(this.at_us, window))
    }

    pub fn next_due(&mut self, window: &Window) -> io::Result<Instant> {
        self.operation(|this| {
            this.check_window(window, this.at_us)?;
            if this.checked.is_none() {
                this.step(this.at_us, window)?;
            }
            let due = this
                .checked
                .as_ref()
                .ok_or_else(|| invalid("missing memory fixture checked wait"))?
                .next_due_us;
            this.check_window(window, due)?;
            this.instant_at(due)
        })
    }

    /// A longer wait is split at every original one-second reporter due instant.
    /// Each split requires both a valid Checked and its real stderr marker.
    pub fn advance_to(&mut self, target: Instant, window: &Window) -> io::Result<()> {
        self.operation(|this| this.advance_inner(target, window))
    }

    pub fn wait_cadence(&mut self, duration: Duration) -> io::Result<()> {
        self.operation(|this| {
            let window = this.window()?;
            let target = this
                .cadence
                .checked_add(duration)
                .ok_or_else(|| invalid("memory fixture cadence overflow"))?;
            this.advance_inner(target, &window)
        })
    }

    fn advance_inner(&mut self, target: Instant, window: &Window) -> io::Result<()> {
        let elapsed = target
            .checked_duration_since(self.origin)
            .ok_or_else(|| invalid("memory fixture target precedes its origin"))?;
        if !elapsed.subsec_nanos().is_multiple_of(1_000) {
            return Err(invalid("memory fixture target is not whole microseconds"));
        }
        let target_us = u64::try_from(elapsed.as_micros())
            .map_err(|_| invalid("memory fixture target overflow"))?;
        self.check_window(window, target_us)?;
        if target_us < self.at_us {
            return Err(invalid("memory fixture time regressed"));
        }
        if self.checked.is_none() || target_us == self.at_us {
            self.step(self.at_us, window)?;
        }
        while self.at_us < target_us {
            let due = self
                .checked
                .as_ref()
                .ok_or_else(|| invalid("missing memory fixture checked wait"))?
                .next_due_us;
            self.step(target_us.min(due), window)?;
        }
        Ok(())
    }

    fn step(&mut self, at_us: u64, window: &Window) -> io::Result<()> {
        self.check_window(window, at_us)?;
        if at_us < self.at_us
            || self
                .checked
                .as_ref()
                .is_some_and(|last| at_us > last.next_due_us)
            || (self.checked.is_none() && at_us != 0)
        {
            return Err(invalid("memory fixture Step skips a reporter boundary"));
        }
        let step = Step {
            nonce: self.nonce.clone(),
            round: self
                .round
                .checked_add(1)
                .ok_or_else(|| invalid("memory fixture round overflow"))?,
            at_us,
        };
        let cadence = self.instant_at(at_us)?;
        self.channel.write_until(&step, window.deadline)?;
        let checked: Checked = self.channel.read_until(window.deadline)?;
        validate_checked(&step, self.checked.as_ref(), &checked)?;
        self.log.observe(&checked.marker, window.deadline)?;
        protocol::remaining(window.deadline)?;
        // The child may already have advanced on a later failure. Commit only
        // after file visibility, and permanently close on every operation error.
        self.round = step.round;
        self.at_us = at_us;
        self.cadence = cadence;
        self.checked = Some(checked);
        Ok(())
    }

    fn instant_at(&self, at_us: u64) -> io::Result<Instant> {
        self.origin
            .checked_add(Duration::from_micros(at_us))
            .ok_or_else(|| invalid("memory fixture Instant overflow"))
    }

    fn check_window(&self, window: &Window, target_us: u64) -> io::Result<()> {
        if !Rc::ptr_eq(&window.owner, &self.owner)
            || self.at_us < window.started_us
            || self.at_us > window.through_us
            || target_us < window.started_us
            || target_us > window.through_us
        {
            return Err(invalid(
                "memory fixture operation exceeds its logical window",
            ));
        }
        protocol::remaining(window.deadline).map(|_| ())
    }

    fn ensure_open(&self) -> io::Result<()> {
        match &self.failure {
            Some(reason) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                format!("memory fixture control is closed: {reason}"),
            )),
            None => Ok(()),
        }
    }

    fn operation<T>(&mut self, body: impl FnOnce(&mut Self) -> io::Result<T>) -> io::Result<T> {
        self.ensure_open()?;
        match body(self) {
            Ok(value) => Ok(value),
            Err(error) => {
                self.failure = Some(error.to_string());
                self.channel.shutdown();
                Err(error)
            }
        }
    }
}

fn validate_checked(step: &Step, previous: Option<&Checked>, value: &Checked) -> io::Result<()> {
    if value.nonce != step.nonce
        || value.round != step.round
        || value.at_us != step.at_us
        || value.marker != protocol::marker(&step.nonce, step.round)?
    {
        return Err(invalid("memory fixture Checked does not match its Step"));
    }
    let attempted = u64::from(value.last_outcome != Outcome::NotAttempted);
    match previous {
        None => {
            if step.at_us != 0
                || value.cycles != 1
                || value.attempts != attempted
                || value.cycle_started_us != 0
                || value.next_due_us != PERIOD_US
            {
                return Err(invalid("invalid initial memory fixture checked wait"));
            }
        }
        Some(last) if step.at_us < last.next_due_us => {
            if step.at_us < last.at_us
                || value.cycles != last.cycles
                || value.attempts != last.attempts
                || value.last_outcome != last.last_outcome
                || value.cycle_started_us != last.cycle_started_us
                || value.next_due_us != last.next_due_us
            {
                return Err(invalid(
                    "memory fixture progress changed before its due time",
                ));
            }
        }
        Some(last) => {
            if step.at_us != last.next_due_us
                || last.cycles.checked_add(1) != Some(value.cycles)
                || last.attempts.checked_add(attempted) != Some(value.attempts)
                || value.cycle_started_us != step.at_us
                || step.at_us.checked_add(PERIOD_US) != Some(value.next_due_us)
            {
                return Err(invalid(
                    "memory fixture due cycle was skipped or misreported",
                ));
            }
        }
    }
    if value.cycle_started_us > value.at_us || value.next_due_us <= value.at_us {
        return Err(invalid("memory fixture Checked is not a positive wait"));
    }
    Ok(())
}

struct LogBarrier {
    path: PathBuf,
    file: File,
    device: u64,
    inode: u64,
    offset: u64,
    line: Vec<u8>,
    oversized: bool,
}

impl LogBarrier {
    fn open(path: &Path) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() {
            return Err(invalid("memory fixture stderr must be a regular file"));
        }
        let file = File::open(path)?;
        let opened = file.metadata()?;
        if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
            return Err(invalid("memory fixture stderr changed while opening"));
        }
        Ok(Self {
            path: path.to_owned(),
            file,
            device: opened.dev(),
            inode: opened.ino(),
            offset: 0,
            line: Vec::with_capacity(protocol::MAX_MARKER_BYTES),
            oversized: false,
        })
    }

    fn observe(&mut self, marker: &str, deadline: Instant) -> io::Result<()> {
        loop {
            protocol::remaining(deadline)?;
            if self.scan(marker.as_bytes(), deadline)? {
                return Ok(());
            }
            thread::sleep(FILE_POLL.min(protocol::remaining(deadline)?));
        }
    }

    fn scan(&mut self, marker: &[u8], deadline: Instant) -> io::Result<bool> {
        if marker.is_empty()
            || marker.len() > protocol::MAX_MARKER_BYTES
            || marker.last() != Some(&b'\n')
            || marker[..marker.len() - 1].contains(&b'\n')
        {
            return Err(invalid("invalid memory fixture log marker line"));
        }
        let mut found = false;
        loop {
            protocol::remaining(deadline)?;
            let metadata = fs::symlink_metadata(&self.path)?;
            if !metadata.is_file()
                || metadata.dev() != self.device
                || metadata.ino() != self.inode
                || metadata.len() < self.offset
            {
                return Err(invalid("memory fixture stderr was replaced or truncated"));
            }
            let mut bytes = [0_u8; 4096];
            let count = match self.file.read(&mut bytes) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if count == 0 {
                return Ok(found);
            }
            self.offset = self
                .offset
                .checked_add(count as u64)
                .ok_or_else(|| invalid("memory fixture log offset overflow"))?;
            for byte in &bytes[..count] {
                if !self.oversized {
                    if self.line.len() < protocol::MAX_MARKER_BYTES {
                        self.line.push(*byte);
                    } else {
                        self.line.clear();
                        self.oversized = true;
                    }
                }
                if *byte == b'\n' {
                    found |= !self.oversized && self.line == marker;
                    self.line.clear();
                    self.oversized = false;
                }
            }
            // Process the whole chunk, retaining any partial following line.
            // A receipt field or a marker substring never settles the log.
            if found {
                protocol::remaining(deadline)?;
                return Ok(true);
            }
        }
    }
}

struct SocketPath {
    listener: UnixListener,
    path: PathBuf,
}

impl SocketPath {
    fn bind(path: PathBuf) -> io::Result<Self> {
        let listener = UnixListener::bind(&path)?;
        let socket = Self { listener, path };
        fs::set_permissions(&socket.path, Permissions::from_mode(0o600))?;
        socket.listener.set_nonblocking(true)?;
        Ok(socket)
    }
}

impl Drop for SocketPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

struct StartupChild(Option<Child>);

impl StartupChild {
    fn get_mut(&mut self) -> io::Result<&mut Child> {
        self.0
            .as_mut()
            .ok_or_else(|| invalid("memory fixture startup child is no longer owned"))
    }

    fn take(&mut self) -> io::Result<Child> {
        self.0
            .take()
            .ok_or_else(|| invalid("memory fixture startup child is no longer owned"))
    }

    fn stop(&mut self) -> io::Result<ExitStatus> {
        let mut child = self.take()?;
        let _ = child.kill();
        child.wait()
    }
}

impl Drop for StartupChild {
    fn drop(&mut self) {
        if self.0.is_some() {
            let _ = self.stop();
        }
    }
}

fn accept_until(listener: &UnixListener, deadline: Instant) -> io::Result<UnixStream> {
    loop {
        protocol::remaining(deadline)?;
        match listener.accept() {
            Ok((stream, _)) => return Ok(stream),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                wait_fd(listener, PollFlags::IN, deadline)?;
            }
            Err(error) => return Err(error),
        }
    }
}

fn write_pipe_until<W: Write + AsFd>(
    writer: &mut W,
    bytes: &[u8],
    deadline: Instant,
) -> io::Result<()> {
    let flags = fcntl_getfl(&*writer)?;
    fcntl_setfl(&*writer, flags | OFlags::NONBLOCK)?;
    let mut left = bytes;
    while !left.is_empty() {
        protocol::remaining(deadline)?;
        match writer.write(left) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => left = &left[count..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                wait_fd(&*writer, PollFlags::OUT, deadline)?;
            }
            Err(error) => return Err(error),
        }
    }
    protocol::remaining(deadline).map(|_| ())
}

fn wait_fd(fd: &impl AsFd, events: PollFlags, deadline: Instant) -> io::Result<()> {
    loop {
        let left = protocol::remaining(deadline)?;
        // macOS poll ultimately accepts a signed millisecond timeout.
        let bounded = left.min(Duration::from_secs(60));
        let timeout = Timespec {
            tv_sec: bounded.as_secs() as _,
            tv_nsec: bounded.subsec_nanos() as _,
        };
        let mut descriptors = [PollFd::new(fd, events)];
        match poll(&mut descriptors, Some(&timeout)) {
            Ok(0) => continue,
            Ok(_) if descriptors[0].revents().contains(PollFlags::NVAL) => {
                return Err(io::Error::from_raw_os_error(
                    rustix::io::Errno::BADF.raw_os_error(),
                ));
            }
            // HUP/ERR return to actual I/O, preserving queued bytes and EOF.
            Ok(_) => return Ok(()),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

fn host_deadline() -> io::Result<Instant> {
    Instant::now()
        .checked_add(protocol::HOST_WAIT)
        .ok_or_else(|| invalid("memory fixture host deadline overflow"))
}

fn invalid(message: &'static str) -> io::Error {
    protocol::invalid(message)
}
