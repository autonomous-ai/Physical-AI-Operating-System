//! Lamp-side relay for one finite lamp-live session.
//!
//! It creates a fresh private work directory, binds the cue socket that
//! `lamp-live directed` or `directed-fixture --cue-socket` connects to, starts the runtime,
//! and relays cues, stdout lines and ping replies as JSON lines. After the
//! runtime exits (or is killed at the hard deadline) it streams the final
//! `events.jsonl`. It never stops or starts services and never touches mixers
//! or motors; lamp-live itself refuses to run while legacy owners are active.
//!
//! Every relayed stream is framed with a fixed per-line bound; oversized,
//! non-UTF-8 or cut-off lines are explicit failures in `session_end`, never
//! truncated or lossily repaired. The runtime runs in its own process group so
//! that stopping it also stops its workers. Closing the runner transport
//! before the runtime ends stops the runtime: an unattended session must not
//! keep the microphone and speaker.
use crate::{
    Result,
    events::parse_cue,
    invalid,
    physical::{
        framing::{Frame, HashingReader, LineFramer},
        protocol::{
            MAX_LINE_BYTES, MAX_RUNNER_LINE_BYTES, MAX_STDOUT_LINE_BYTES, MAX_TRACE_BYTES,
            MAX_TRACE_LINES, MAX_TRACE_RECORD_BYTES, RunnerLine, SCHEMA, SessionLine,
        },
    },
};
use lamp_ipc::monotonic_us;
use std::{
    fs,
    io::{Read, Write},
    os::unix::{fs::DirBuilderExt, net::UnixDatagram, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Duration,
};

#[derive(Clone, Debug)]
pub enum SessionMode {
    Fixture { reply: PathBuf, sha256: String },
    Directed { provider_config: PathBuf },
}

#[derive(Clone, Debug)]
pub struct SessionOptions {
    pub runtime: PathBuf,
    /// Extra leading arguments (used to substitute the fake runtime in tests).
    pub runtime_prefix: Vec<String>,
    pub work: PathBuf,
    pub seconds: u16,
    pub mode: SessionMode,
    pub noise_suppression: String,
    pub diagnostics: bool,
    /// Startup before readiness plus shutdown; added to `seconds` for the
    /// hard kill deadline. lamp-live's own startup bound is 25 s.
    pub allowance_seconds: u16,
    /// Passed as `--ring-channel-ceiling`; without it no ring device opens.
    pub ring_channel_ceiling: Option<u16>,
}

impl SessionOptions {
    pub fn parse(arguments: &[String]) -> Result<Self> {
        let mut runtime = None;
        let mut runtime_prefix = Vec::new();
        let mut work = None;
        let mut seconds = None;
        let mut reply = None;
        let mut sha256 = None;
        let mut provider_config = None;
        let mut noise_suppression = "on".to_owned();
        let mut diagnostics = false;
        let mut allowance_seconds = 40;
        let mut ring_channel_ceiling = None;
        let mut iter = arguments.iter();
        while let Some(flag) = iter.next() {
            let mut value = || {
                iter.next()
                    .cloned()
                    .ok_or_else(|| invalid(&format!("{flag} needs a value")))
            };
            match flag.as_str() {
                "--runtime" => runtime = Some(PathBuf::from(value()?)),
                "--runtime-prefix-arg" => runtime_prefix.push(value()?),
                "--work" => work = Some(PathBuf::from(value()?)),
                "--seconds" => seconds = Some(value()?.parse()?),
                "--fixture-reply" => reply = Some(PathBuf::from(value()?)),
                "--fixture-sha256" => sha256 = Some(value()?),
                "--provider-config" => provider_config = Some(PathBuf::from(value()?)),
                "--noise-suppression" => noise_suppression = value()?,
                "--allowance-seconds" => allowance_seconds = value()?.parse()?,
                "--diagnostics" => diagnostics = true,
                "--ring-channel-ceiling" => ring_channel_ceiling = Some(value()?.parse()?),
                _ => return Err(invalid(&format!("unknown lamp-session option {flag}"))),
            }
        }
        let mode = match (reply, sha256, provider_config) {
            (Some(reply), Some(sha256), None) => SessionMode::Fixture { reply, sha256 },
            (None, None, Some(provider_config)) => SessionMode::Directed { provider_config },
            _ => {
                return Err(invalid(
                    "give either --fixture-reply with --fixture-sha256, or --provider-config",
                ));
            }
        };
        if !matches!(noise_suppression.as_str(), "on" | "off") {
            return Err(invalid("--noise-suppression must be on or off"));
        }
        let options = Self {
            runtime: runtime.ok_or_else(|| invalid("--runtime is required"))?,
            runtime_prefix,
            work: work.ok_or_else(|| invalid("--work is required"))?,
            seconds: seconds.ok_or_else(|| invalid("--seconds is required"))?,
            mode,
            noise_suppression,
            diagnostics,
            allowance_seconds,
            ring_channel_ceiling,
        };
        if !(1..=600).contains(&options.seconds) || !(5..=120).contains(&options.allowance_seconds)
        {
            return Err(invalid("seconds must be 1..600 and allowance 5..120"));
        }
        if !options.work.is_absolute() {
            return Err(invalid("--work must be absolute"));
        }
        Ok(options)
    }

    pub fn runtime_output(&self) -> PathBuf {
        self.work.join("runtime")
    }
    pub fn cue_path(&self) -> PathBuf {
        self.work.join("cue.sock")
    }

    pub fn argv(&self) -> Vec<String> {
        let text = |path: &PathBuf| path.display().to_string();
        let mut argv = vec![text(&self.runtime)];
        argv.extend(self.runtime_prefix.iter().cloned());
        let output = text(&self.runtime_output());
        match &self.mode {
            SessionMode::Fixture { reply, sha256 } => {
                // lamp-live requires --diagnostics for fixture experiments.
                argv.extend([
                    "directed-fixture".into(),
                    text(reply),
                    sha256.clone(),
                    self.seconds.to_string(),
                    output,
                    "--diagnostics".into(),
                    "--noise-suppression".into(),
                    self.noise_suppression.clone(),
                ]);
            }
            SessionMode::Directed { provider_config } => {
                argv.extend([
                    "directed".into(),
                    text(provider_config),
                    self.seconds.to_string(),
                    output,
                    "--noise-suppression".into(),
                    self.noise_suppression.clone(),
                ]);
                if self.diagnostics {
                    argv.push("--diagnostics".into());
                }
            }
        }
        // Both modes explicitly request the scheduling capability. An older
        // runtime must fail its CLI, never fall back to guessed delays.
        argv.extend(["--cue-socket".into(), text(&self.cue_path())]);
        if let Some(ceiling) = self.ring_channel_ceiling {
            argv.extend(["--ring-channel-ceiling".into(), ceiling.to_string()]);
        }
        argv
    }
}

/// Encode one relay line, refusing anything the runner's framing would reject.
fn encode(line: &SessionLine) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(line)?;
    if bytes.len() > MAX_LINE_BYTES {
        return Err(invalid(&format!(
            "encoded relay line of {} bytes exceeds the {MAX_LINE_BYTES}-byte limit",
            bytes.len()
        )));
    }
    bytes.push(b'\n');
    Ok(bytes)
}

fn emit(out: &mut impl Write, line: &SessionLine) -> Result<()> {
    out.write_all(&encode(line)?)?;
    out.flush()?;
    Ok(())
}

/// Bounded failure text retained for `session_end`.
#[derive(Default)]
struct RelayErrors {
    first: Vec<String>,
    omitted: usize,
}
impl RelayErrors {
    const KEPT: usize = 16;
    fn push(&mut self, error: String) {
        if self.first.len() < Self::KEPT {
            self.first.push(error);
        } else {
            self.omitted += 1;
        }
    }
    fn summary(&self) -> Option<String> {
        if self.first.is_empty() {
            return None;
        }
        let mut text = self.first.join("; ");
        if self.omitted > 0 {
            text.push_str(&format!("; {} further relay errors omitted", self.omitted));
        }
        Some(text)
    }
}

type Framed = std::result::Result<String, String>;

/// Frame a byte stream on its own thread. Invalid frames arrive as `Err`
/// text; the thread ends at end of input, after a read error, or when the
/// receiver is dropped.
fn line_reader<R: Read + Send + 'static>(
    source: R,
    max: usize,
    what: &'static str,
) -> Receiver<Framed> {
    let (sender, receiver) = mpsc::sync_channel(256);
    thread::spawn(move || {
        let mut framer = LineFramer::new(source, max);
        loop {
            let (item, last) = match framer.next_frame() {
                Ok(None) => break,
                Ok(Some(Frame::Line(line))) => (Ok(line), false),
                Ok(Some(frame)) => (Err(frame.failure(what, max).unwrap_or_default()), false),
                Err(error) => (Err(format!("{what} read failed: {error}")), true),
            };
            if sender.send(item).is_err() || last {
                break;
            }
        }
    });
    receiver
}

/// The relay line for one framed runtime stdout item.
fn stdout_line(lamp_received_us: u64, item: Framed, errors: &mut RelayErrors) -> SessionLine {
    let error = match item {
        Ok(line) => {
            let relayed = SessionLine::Stdout {
                lamp_received_us,
                line,
            };
            match encode(&relayed) {
                Ok(_) => return relayed,
                Err(error) => format!("runtime stdout: {error}"),
            }
        }
        Err(error) => error,
    };
    errors.push(error.clone());
    SessionLine::StdoutInvalid {
        lamp_received_us,
        error,
    }
}

fn cue_line(lamp_received_us: u64, datagram: &[u8]) -> Result<SessionLine> {
    Ok(match parse_cue(datagram) {
        Ok(_) => SessionLine::Cue {
            lamp_received_us,
            cue: serde_json::from_slice(datagram)?,
        },
        Err(error) => SessionLine::CueInvalid {
            lamp_received_us,
            error: error.to_string(),
        },
    })
}

/// Stop the runtime and every process still in its group, then reap it.
fn kill(child: &mut Child) {
    let _ = rustix::process::kill_process_group(
        rustix::process::Pid::from_child(child),
        rustix::process::Signal::KILL,
    );
    let _ = child.kill();
    let _ = child.wait();
}

/// Ensures an early error (for example a closed relay transport) never leaves
/// the runtime running or the cue socket behind.
struct Cleanup {
    child: Option<Child>,
    socket: Option<PathBuf>,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut()
            && matches!(child.try_wait(), Ok(None))
        {
            kill(child);
        }
        if let Some(path) = &self.socket {
            let _ = fs::remove_file(path);
        }
    }
}

/// The streamed trace: its identity, record count and any failure.
struct TraceStream {
    sha256: Option<String>,
    lines: usize,
    error: Option<String>,
}

/// Stream `events.jsonl` one bounded record per line. The digest always
/// covers the whole file, including anything after a failure. A cut-off final
/// record, an oversized or non-UTF-8 record, malformed JSON or too many
/// records stops streaming with an explicit error.
fn stream_trace(path: &Path, out: &mut impl Write) -> Result<TraceStream> {
    let mut stream = TraceStream {
        sha256: None,
        lines: 0,
        error: None,
    };
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) => {
            stream.error = Some(format!("no runtime trace: {error}"));
            return Ok(stream);
        }
    };
    let length = file.metadata()?.len();
    if length > MAX_TRACE_BYTES {
        stream.error = Some(format!(
            "trace of {length} bytes exceeds the {MAX_TRACE_BYTES}-byte relay bound"
        ));
        return Ok(stream);
    }
    // The runtime has exited, but never read past the checked length.
    let mut framer = LineFramer::new(
        HashingReader::new(file.take(MAX_TRACE_BYTES)),
        MAX_TRACE_RECORD_BYTES,
    );
    while stream.error.is_none() {
        let line = match framer.next_frame()? {
            None => break,
            Some(Frame::Line(line)) => line,
            Some(frame) => {
                stream.error = frame.failure("trace", MAX_TRACE_RECORD_BYTES);
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        if stream.lines >= MAX_TRACE_LINES {
            stream.error = Some(format!(
                "trace exceeds the {MAX_TRACE_LINES}-record relay bound; later records were not relayed"
            ));
            break;
        }
        let record = match serde_json::from_str(&line) {
            Ok(record) => record,
            Err(error) => {
                stream.error = Some(format!(
                    "malformed trace record {}: {error}",
                    stream.lines + 1
                ));
                break;
            }
        };
        match encode(&SessionLine::Trace { record }) {
            Ok(bytes) => {
                out.write_all(&bytes)?;
                out.flush()?;
            }
            Err(error) => {
                stream.error = Some(format!("trace record {}: {error}", stream.lines + 1));
                break;
            }
        }
        stream.lines += 1;
    }
    let (sha256, bytes) = framer.finish_hash()?;
    if bytes != length {
        stream.error.get_or_insert(format!(
            "trace changed while relaying ({length} bytes before, {bytes} read)"
        ));
    }
    stream.sha256 = Some(sha256);
    Ok(stream)
}

/// Run one session; returns the process exit code to use.
pub fn run<R: Read + Send + 'static>(
    options: &SessionOptions,
    input: R,
    out: &mut impl Write,
) -> Result<i32> {
    fs::DirBuilder::new().mode(0o700).create(&options.work)?;
    let mut cleanup = Cleanup {
        child: None,
        socket: None,
    };
    let socket = UnixDatagram::bind(options.cue_path())?;
    cleanup.socket = Some(options.cue_path());
    socket.set_nonblocking(true)?;
    let argv = options.argv();
    emit(
        out,
        &SessionLine::SessionStart {
            schema: SCHEMA,
            lamp_us: monotonic_us(),
            mode: match options.mode {
                SessionMode::Fixture { .. } => "fixture",
                SessionMode::Directed { .. } => "directed",
            }
            .into(),
            cue_socket: true,
            runtime_argv: argv.clone(),
        },
    )?;
    let child = cleanup.child.insert(
        Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            // Workers inherit this group, so one signal reaches all of them.
            .process_group(0)
            .spawn()?,
    );
    let stdout = line_reader(
        child
            .stdout
            .take()
            .ok_or_else(|| invalid("runtime stdout unavailable"))?,
        MAX_STDOUT_LINE_BYTES,
        "runtime stdout",
    );
    let input = line_reader(input, MAX_RUNNER_LINE_BYTES, "runner transport");
    let deadline = monotonic_us()
        + (u64::from(options.seconds) + u64::from(options.allowance_seconds)) * 1_000_000;
    let mut buffer = [0_u8; 1024];
    let mut errors = RelayErrors::default();
    let mut killed = false;
    let status = loop {
        for _ in 0..64 {
            match socket.recv(&mut buffer) {
                Ok(count) => emit(out, &cue_line(monotonic_us(), &buffer[..count])?)?,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error.into()),
            }
        }
        while let Ok(item) = stdout.try_recv() {
            emit(out, &stdout_line(monotonic_us(), item, &mut errors))?;
        }
        let mut runner_closed = false;
        loop {
            match input.try_recv() {
                Ok(Ok(line)) => match serde_json::from_str(&line) {
                    Ok(RunnerLine::Ping { seq }) => emit(
                        out,
                        &SessionLine::Pong {
                            seq,
                            lamp_us: monotonic_us(),
                        },
                    )?,
                    Err(error) => errors.push(format!("invalid runner line: {error}")),
                },
                Ok(Err(error)) => errors.push(error),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    runner_closed = true;
                    break;
                }
            }
        }
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if runner_closed {
            kill(child);
            killed = true;
            errors.push(
                "runner transport closed before the runtime ended; the relay stopped the runtime"
                    .into(),
            );
            break None;
        }
        if monotonic_us() >= deadline {
            kill(child);
            killed = true;
            break None;
        }
        thread::sleep(Duration::from_millis(2));
    };
    // A normal runtime exit must not leave workers behind in its group. Any
    // stopped straggler is reported as evidence; the trace itself is unaffected.
    let mut stray_processes_stopped = false;
    if status.is_some() {
        stray_processes_stopped = rustix::process::kill_process_group(
            rustix::process::Pid::from_child(child),
            rustix::process::Signal::KILL,
        )
        .is_ok();
    }
    // Late stdout/cues after exit are still relayed, within a short bound.
    let drain_until = monotonic_us() + 200_000;
    while monotonic_us() < drain_until {
        match stdout.recv_timeout(Duration::from_millis(20)) {
            Ok(item) => emit(out, &stdout_line(monotonic_us(), item, &mut errors))?,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    for _ in 0..64 {
        let count = match socket.recv(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => return Err(error.into()),
        };
        emit(out, &cue_line(monotonic_us(), &buffer[..count])?)?;
    }
    let trace = stream_trace(&options.runtime_output().join("events.jsonl"), out)?;
    if let Some(error) = trace.error {
        errors.push(error);
    }
    let exit_code = status.and_then(|s| s.code());
    emit(
        out,
        &SessionLine::SessionEnd {
            lamp_us: monotonic_us(),
            exit_code,
            killed,
            events_sha256: trace.sha256,
            events_lines: trace.lines,
            error: errors.summary(),
            stray_processes_stopped,
        },
    )?;
    drop(socket);
    drop(cleanup);
    Ok(if killed { 3 } else { 0 })
}
