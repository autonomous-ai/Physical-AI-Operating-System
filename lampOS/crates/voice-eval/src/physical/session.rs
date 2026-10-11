//! Lamp-side relay for one lamp-live session: a finite `directed` or
//! `directed-fixture` run, or one long-lived `serve` runtime.
//!
//! It creates a fresh private work directory, binds the cue socket that
//! lamp-live's `--cue-socket` connects to, starts the runtime, and relays
//! cues, stdout lines and ping replies as JSON lines. After a finite runtime
//! exits (or is killed at the hard deadline) it streams the final
//! `events.jsonl`. A serve runtime runs until the runner's `stop` line, the
//! runner transport closing or the hard deadline; each takes the same path
//! (SIGTERM, a bounded grace, then a forced group kill), and the relay then
//! streams the rolling `metadata/events-*.jsonl` in generation order. It
//! never stops or starts services and never touches mixers or motors;
//! lamp-live itself refuses to run while legacy owners are active.
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
        framing::{Frame, HashingReader, LineFramer, STREAM_LIMIT},
        protocol::{
            MAX_LINE_BYTES, MAX_RUNNER_LINE_BYTES, MAX_RUNNER_TOTAL_BYTES, MAX_STDOUT_LINE_BYTES,
            MAX_STDOUT_TOTAL_BYTES, MAX_TRACE_BYTES, MAX_TRACE_LINES, MAX_TRACE_RECORD_BYTES,
            RunnerLine, SCHEMA, ServeEnd, ServeMetadata, ServeSegment, SessionLine,
        },
    },
};
use lamp_ipc::monotonic_us;
use serde_json::Value;
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
    Fixture {
        reply: PathBuf,
        sha256: String,
    },
    Directed {
        provider_config: PathBuf,
    },
    /// One long-lived `lamp-live serve` runtime; `seconds` bounds it.
    Serve {
        provider_config: PathBuf,
    },
}

/// Grace between a serve runtime's SIGTERM and the forced kill of its group.
/// lamp-live records its end and finishes its bounded metadata wait well
/// inside it; a runtime that outlives it is a failure, never a retry.
pub const SERVE_STOP_GRACE_US: u64 = 10_000_000;
/// lamp-live serve's rolling metadata files and its health record bound.
const SERVE_SEGMENTS: usize = 4;
const MAX_HEALTH_BYTES: u64 = 64 * 1024;

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
        let mut serve_config = None;
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
                "--serve-config" => serve_config = Some(PathBuf::from(value()?)),
                "--noise-suppression" => noise_suppression = value()?,
                "--allowance-seconds" => allowance_seconds = value()?.parse()?,
                "--diagnostics" => diagnostics = true,
                "--ring-channel-ceiling" => ring_channel_ceiling = Some(value()?.parse()?),
                _ => return Err(invalid(&format!("unknown lamp-session option {flag}"))),
            }
        }
        let mode = match (reply, sha256, provider_config, serve_config) {
            (Some(reply), Some(sha256), None, None) => SessionMode::Fixture { reply, sha256 },
            (None, None, Some(provider_config), None) => SessionMode::Directed { provider_config },
            (None, None, None, Some(provider_config)) => SessionMode::Serve { provider_config },
            _ => {
                return Err(invalid(
                    "give one of --fixture-reply with --fixture-sha256, --provider-config or --serve-config",
                ));
            }
        };
        if diagnostics && matches!(mode, SessionMode::Serve { .. }) {
            // lamp-live serve refuses unbounded raw audio diagnostics.
            return Err(invalid("--diagnostics needs a finite session, not serve"));
        }
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
            // serve has no length: the relay bounds it and stops it.
            SessionMode::Serve { provider_config } => {
                argv.extend([
                    "serve".into(),
                    text(provider_config),
                    output,
                    "--noise-suppression".into(),
                    self.noise_suppression.clone(),
                ]);
            }
        }
        // Every mode explicitly requests the scheduling capability. An older
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
    total: u64,
    what: &'static str,
) -> Receiver<Framed> {
    let (sender, receiver) = mpsc::sync_channel(256);
    thread::spawn(move || {
        let mut framer = LineFramer::with_total_limit(source, max, total);
        loop {
            let (item, last) = match framer.next_frame() {
                Ok(None) => break,
                Ok(Some(Frame::Line(line))) => (Ok(line), false),
                Ok(Some(frame)) => (Err(frame.failure(what, max).unwrap_or_default()), false),
                Err(error) if error.to_string() == STREAM_LIMIT => (
                    Err(format!("{what} exceeded {total} bytes; reading stopped")),
                    true,
                ),
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
/// Safe after the runtime has exited: the group outlives its leader while any
/// worker remains, and reaping an already reaped child is a no-op error.
fn kill(child: &mut Child) {
    let _ = rustix::process::kill_process_group(
        rustix::process::Pid::from_child(child),
        rustix::process::Signal::KILL,
    );
    let _ = child.kill();
    let _ = child.wait();
}

/// Ensures an early error (for example a closed relay transport) never leaves
/// the runtime, any of its workers, or the cue socket behind. The process
/// group is killed even when the runtime itself has already exited: its
/// workers may still be running in that group.
struct Cleanup {
    child: Option<Child>,
    socket: Option<PathBuf>,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
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

/// A requested stop of a serve runtime and how its SIGTERM went.
struct StopRequest {
    cause: &'static str,
    reason: Option<String>,
    requested_us: u64,
    signal_sent_us: Option<u64>,
    signal_error: Option<String>,
}

/// Coordinator-first stop, like the pilot unit's `KillMode=mixed`: SIGTERM
/// goes only to the runtime this relay started (the leader of the process
/// group it created). Workers are stopped by the coordinator itself; a
/// group-wide SIGTERM would bypass its ordered cleanup.
fn request_stop(child: &Child, cause: &'static str, reason: Option<String>) -> StopRequest {
    let requested_us = monotonic_us();
    let signal = rustix::process::kill_process(
        rustix::process::Pid::from_child(child),
        rustix::process::Signal::TERM,
    );
    StopRequest {
        cause,
        reason,
        requested_us,
        signal_sent_us: signal.is_ok().then(monotonic_us),
        signal_error: signal.err().map(|error| error.to_string()),
    }
}

/// Whether the relay's own process group is gone, polled within a bound
/// after its leader was reaped (orphaned members are reaped by init).
fn group_gone(child: &Child, within_us: u64) -> bool {
    let group = rustix::process::Pid::from_child(child);
    let until = monotonic_us() + within_us;
    loop {
        if rustix::process::test_kill_process_group(group).is_err() {
            return true;
        }
        if monotonic_us() >= until {
            return false;
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// Stream lamp-live serve's rolling metadata as `trace` lines, oldest
/// generation first. Headers are checked and summarized, not relayed. A
/// missing, overwritten, malformed or gapped history is reported, never
/// repaired: the relay pushes it as an error so the attempt is not scored.
fn stream_metadata(directory: &Path, out: &mut impl Write) -> Result<ServeMetadata> {
    let mut metadata = ServeMetadata::default();
    let mut segments = Vec::new();
    let mut total = 0_u64;
    for slot in 0..SERVE_SEGMENTS {
        let file = match fs::File::open(directory.join(format!("events-{slot}.jsonl"))) {
            Ok(file) => file,
            Err(error) => {
                metadata.error = Some(format!("metadata segment {slot}: {error}"));
                return Ok(metadata);
            }
        };
        let length = file.metadata()?.len();
        if length == 0 {
            continue;
        }
        total += length;
        if total > MAX_TRACE_BYTES {
            metadata.error = Some(format!(
                "metadata exceeds the {MAX_TRACE_BYTES}-byte relay bound"
            ));
            return Ok(metadata);
        }
        let mut framer = LineFramer::new(
            HashingReader::new(file.take(length)),
            MAX_TRACE_RECORD_BYTES,
        );
        let header = match framer.next_frame()? {
            Some(Frame::Line(line)) => serde_json::from_str::<Value>(&line).ok(),
            _ => None,
        };
        let identity = header.as_ref().and_then(|header| {
            (header["kind"] == "service_segment").then_some((
                header["generation"].as_u64()?,
                header["first_writer_event"].as_u64()?,
            ))
        });
        let Some((generation, first_writer_event)) = identity else {
            metadata.error = Some(format!("metadata segment {slot} has no valid header"));
            return Ok(metadata);
        };
        segments.push((generation, slot, first_writer_event, length, framer));
    }
    segments.sort_by_key(|segment| segment.0);
    if segments.is_empty() {
        metadata.error = Some("no serve metadata was written".into());
        return Ok(metadata);
    }
    if segments.windows(2).any(|pair| pair[1].0 != pair[0].0 + 1) {
        metadata.error = Some("metadata segment generations are not contiguous".into());
        return Ok(metadata);
    }
    // Rotation reuses the oldest file: generation 0 gone means lost history.
    metadata.history_truncated = segments[0].0 > 0;
    let mut previous: Option<u64> = None;
    for (generation, slot, first_writer_event, length, mut framer) in segments {
        while metadata.error.is_none() {
            let line = match framer.next_frame()? {
                None => break,
                Some(Frame::Line(line)) => line,
                Some(frame) => {
                    metadata.error = frame.failure("metadata", MAX_TRACE_RECORD_BYTES);
                    break;
                }
            };
            if line.trim().is_empty() {
                continue;
            }
            if metadata.records >= MAX_TRACE_LINES {
                metadata.error = Some(format!(
                    "metadata exceeds the {MAX_TRACE_LINES}-record relay bound"
                ));
                break;
            }
            let record: Value = match serde_json::from_str(&line) {
                Ok(record) => record,
                Err(error) => {
                    metadata.error = Some(format!(
                        "malformed metadata record {}: {error}",
                        metadata.records + 1
                    ));
                    break;
                }
            };
            match record["service_sequence"].as_u64() {
                Some(sequence) => {
                    if previous.is_some_and(|last| last.checked_add(1) != Some(sequence)) {
                        metadata.sequence_gaps += 1;
                    }
                    metadata.first_service_sequence.get_or_insert(sequence);
                    metadata.last_service_sequence = Some(sequence);
                    previous = Some(sequence);
                }
                None => metadata.sequence_gaps += 1,
            }
            match encode(&SessionLine::Trace { record }) {
                Ok(bytes) => {
                    out.write_all(&bytes)?;
                    out.flush()?;
                }
                Err(error) => {
                    metadata.error =
                        Some(format!("metadata record {}: {error}", metadata.records + 1));
                    break;
                }
            }
            metadata.records += 1;
        }
        if metadata.error.is_some() {
            break;
        }
        let (sha256, bytes) = framer.finish_hash()?;
        if bytes != length {
            metadata.error = Some(format!("metadata segment {slot} changed while relaying"));
            break;
        }
        metadata.segments.push(ServeSegment {
            slot,
            generation,
            first_writer_event,
            bytes,
            sha256,
        });
    }
    let mut health = Vec::new();
    if let Ok(file) = fs::File::open(directory.join("health.json")) {
        file.take(MAX_HEALTH_BYTES + 1).read_to_end(&mut health)?;
    }
    metadata.health = (health.len() as u64 <= MAX_HEALTH_BYTES)
        .then(|| serde_json::from_slice(&health).ok())
        .flatten();
    Ok(metadata)
}

/// Every reason a serve session's metadata is missing evidence: the history
/// is overwritten, gapped or incomplete, or the runtime never confirmed its
/// final metadata flush.
fn metadata_problems(metadata: &ServeMetadata, finished: Option<&Value>) -> Vec<String> {
    let mut problems = Vec::new();
    if let Some(error) = &metadata.error {
        problems.push(format!("serve metadata: {error}"));
    }
    if metadata.history_truncated {
        problems.push("serve metadata history was overwritten by rotation".into());
    }
    if metadata.sequence_gaps > 0 {
        problems.push(format!(
            "serve metadata has {} service_sequence gaps",
            metadata.sequence_gaps
        ));
    }
    if metadata
        .health
        .as_ref()
        .is_none_or(|health| health["writer_finished"] != true)
    {
        problems.push("serve metadata health does not report a finished writer".into());
    }
    let report = finished.map(|line| &line["health"]);
    let acknowledged = report.is_some_and(|health| {
        health["acknowledged"] == true
            && health["degraded"] == false
            && health["writer_failed"] == false
            && health["dropped_events"] == 0
            && health["oversized_events"] == 0
            && health["unconfirmed_events"] == 0
    });
    if !acknowledged {
        problems.push(match report {
            None => "the runtime printed no service_metadata_finished acknowledgment".into(),
            Some(health) => {
                format!("the runtime's final metadata acknowledgment is incomplete: {health}")
            }
        });
    }
    problems
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
    let serve = matches!(options.mode, SessionMode::Serve { .. });
    emit(
        out,
        &SessionLine::SessionStart {
            schema: SCHEMA,
            lamp_us: monotonic_us(),
            mode: match options.mode {
                SessionMode::Fixture { .. } => "fixture",
                SessionMode::Directed { .. } => "directed",
                SessionMode::Serve { .. } => "serve",
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
        MAX_STDOUT_TOTAL_BYTES,
        "runtime stdout",
    );
    let input = line_reader(
        input,
        MAX_RUNNER_LINE_BYTES,
        MAX_RUNNER_TOTAL_BYTES,
        "runner transport",
    );
    let deadline = monotonic_us()
        + (u64::from(options.seconds) + u64::from(options.allowance_seconds)) * 1_000_000;
    let mut buffer = [0_u8; 1024];
    let mut errors = RelayErrors::default();
    let mut killed = false;
    let mut stop: Option<StopRequest> = None;
    let mut forced_kill = false;
    let mut finished_line: Option<Value> = None;
    let relay_stdout = |out: &mut dyn Write,
                        item: Framed,
                        errors: &mut RelayErrors,
                        finished_line: &mut Option<Value>|
     -> Result<()> {
        let line = stdout_line(monotonic_us(), item, errors);
        if serve
            && let SessionLine::Stdout { line, .. } = &line
            && let Ok(value) = serde_json::from_str::<Value>(line)
            && value["kind"] == "service_metadata_finished"
        {
            *finished_line = Some(value);
        }
        let bytes = encode(&line)?;
        out.write_all(&bytes)?;
        out.flush()?;
        Ok(())
    };
    let (status, exited_us) = loop {
        for _ in 0..64 {
            match socket.recv(&mut buffer) {
                Ok(count) => emit(out, &cue_line(monotonic_us(), &buffer[..count])?)?,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error.into()),
            }
        }
        while let Ok(item) = stdout.try_recv() {
            relay_stdout(out, item, &mut errors, &mut finished_line)?;
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
                    Ok(RunnerLine::Stop { reason }) if serve => {
                        // A repeated stop changes nothing: one signal, one grace.
                        if stop.is_none() {
                            stop = Some(request_stop(child, "runner_stop", Some(reason)));
                        }
                    }
                    Ok(RunnerLine::Stop { .. }) => {
                        errors.push("stop is only defined for serve sessions".into())
                    }
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
            break (Some(status), Some(monotonic_us()));
        }
        if runner_closed && stop.is_none() {
            errors.push(
                "runner transport closed before the runtime ended; the relay stopped the runtime"
                    .into(),
            );
            if !serve {
                kill(child);
                killed = true;
                break (None, None);
            }
            stop = Some(request_stop(child, "transport_closed", None));
        }
        if monotonic_us() >= deadline && stop.is_none() {
            if !serve {
                kill(child);
                killed = true;
                break (None, None);
            }
            errors.push("the serve hard deadline passed; the relay stopped the runtime".into());
            stop = Some(request_stop(child, "deadline", None));
        }
        if let Some(request) = &stop
            && monotonic_us() >= request.requested_us + SERVE_STOP_GRACE_US
        {
            // Only the group this relay created is killed.
            kill(child);
            killed = true;
            forced_kill = true;
            errors.push(format!(
                "the runtime outlived the {} ms stop grace; its process group was killed",
                SERVE_STOP_GRACE_US / 1000
            ));
            break (None, None);
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
        if serve && stray_processes_stopped {
            // A serve coordinator reaps its own workers; a forced cleanup is
            // never a clean conversation end.
            errors.push("processes outlived the serve runtime and were killed".into());
        }
    }
    let group_reaped = !serve || group_gone(child, 2_000_000);
    if !group_reaped {
        errors.push("the runtime's process group was still present 2 s after its end".into());
    }
    // Late stdout/cues after exit are still relayed, within a short bound.
    let drain_until = monotonic_us() + 200_000;
    while monotonic_us() < drain_until {
        match stdout.recv_timeout(Duration::from_millis(20)) {
            Ok(item) => relay_stdout(out, item, &mut errors, &mut finished_line)?,
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
    let (trace, serve_end) = if serve {
        let metadata = stream_metadata(&options.runtime_output().join("metadata"), out)?;
        for problem in metadata_problems(&metadata, finished_line.as_ref()) {
            errors.push(problem);
        }
        if let Some(request) = &stop
            && let Some(error) = &request.signal_error
        {
            errors.push(format!("the stop signal failed: {error}"));
        }
        let trace = TraceStream {
            sha256: None,
            lines: metadata.records,
            error: None,
        };
        let end = ServeEnd {
            stop_cause: stop.as_ref().map_or("runtime_exited", |s| s.cause).into(),
            stop_reason: stop.as_ref().and_then(|s| s.reason.clone()),
            stop_requested_us: stop.as_ref().map(|s| s.requested_us),
            signal_sent_us: stop.as_ref().and_then(|s| s.signal_sent_us),
            signal_error: stop.as_ref().and_then(|s| s.signal_error.clone()),
            exited_us,
            forced_kill,
            group_reaped,
            metadata,
        };
        (trace, Some(Box::new(end)))
    } else {
        (
            stream_trace(&options.runtime_output().join("events.jsonl"), out)?,
            None,
        )
    };
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
            serve: serve_end,
        },
    )?;
    drop(socket);
    drop(cleanup);
    Ok(if killed { 3 } else { 0 })
}

#[cfg(test)]
mod serve_metadata_tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    /// A metadata directory: `segments` as (slot, generation, first event,
    /// service sequences), plus `health.json` when `finished` is set.
    fn directory(segments: &[(usize, u64, u64, &[u64])], finished: Option<bool>) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lve-meta-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        for slot in 0..SERVE_SEGMENTS {
            let mut text = String::new();
            if let Some((_, generation, first, sequences)) =
                segments.iter().find(|(s, ..)| *s == slot)
            {
                text.push_str(&format!(
                    "{}\n",
                    json!({"kind":"service_segment","generation":generation,
                        "first_writer_event":first,"evidence":"rolling_best_effort_not_benchmark"})
                ));
                for sequence in *sequences {
                    text.push_str(&format!(
                        "{}\n",
                        json!({"kind":"listening_ready","at_us":sequence,"service_sequence":sequence})
                    ));
                }
            }
            fs::write(path.join(format!("events-{slot}.jsonl")), text).unwrap();
        }
        if let Some(finished) = finished {
            fs::write(
                path.join("health.json"),
                json!({"writer_finished":finished}).to_string(),
            )
            .unwrap();
        }
        path
    }

    fn acknowledged(dropped: u64) -> Value {
        json!({"kind":"service_metadata_finished","health":{"acknowledged":true,
            "degraded":false,"writer_failed":false,"dropped_events":dropped,
            "oversized_events":0,"unconfirmed_events":0}})
    }

    fn relayed(out: &[u8]) -> Vec<u64> {
        String::from_utf8(out.to_vec())
            .unwrap()
            .lines()
            .map(|line| match serde_json::from_str(line).unwrap() {
                SessionLine::Trace { record } => record["service_sequence"].as_u64().unwrap(),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn rotated_segments_relay_in_generation_order_without_headers() {
        // Slot 1 holds the older generation after one rotation.
        let path = directory(&[(1, 0, 0, &[1, 2, 3]), (0, 1, 3, &[4, 5])], Some(true));
        let mut out = Vec::new();
        let metadata = stream_metadata(&path, &mut out).unwrap();
        assert_eq!(relayed(&out), [1, 2, 3, 4, 5]);
        assert_eq!(metadata.records, 5);
        assert_eq!(
            metadata
                .segments
                .iter()
                .map(|s| (s.slot, s.generation))
                .collect::<Vec<_>>(),
            [(1, 0), (0, 1)]
        );
        assert_eq!(
            (
                metadata.first_service_sequence,
                metadata.last_service_sequence
            ),
            (Some(1), Some(5))
        );
        assert!(metadata_problems(&metadata, Some(&acknowledged(0))).is_empty());
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn gapped_overwritten_or_unacknowledged_history_is_missing_evidence() {
        let check = |segments: &[(usize, u64, u64, &[u64])],
                     finished: Option<bool>,
                     line: Option<Value>,
                     expected: &str| {
            let path = directory(segments, finished);
            let metadata = stream_metadata(&path, &mut Vec::new()).unwrap();
            let problems = metadata_problems(&metadata, line.as_ref()).join("; ");
            assert!(problems.contains(expected), "{expected}: {problems}");
            fs::remove_dir_all(path).unwrap();
        };
        check(
            &[(0, 0, 0, &[1, 2, 4])],
            Some(true),
            Some(acknowledged(0)),
            "1 service_sequence gaps",
        );
        check(
            &[(0, 1, 9, &[10]), (1, 2, 10, &[11])],
            Some(true),
            Some(acknowledged(0)),
            "overwritten",
        );
        check(
            &[(0, 0, 0, &[1]), (1, 2, 5, &[6])],
            Some(true),
            Some(acknowledged(0)),
            "not contiguous",
        );
        check(
            &[(0, 0, 0, &[1])],
            Some(false),
            Some(acknowledged(0)),
            "finished writer",
        );
        check(
            &[(0, 0, 0, &[1])],
            None,
            Some(acknowledged(0)),
            "finished writer",
        );
        check(
            &[(0, 0, 0, &[1])],
            Some(true),
            None,
            "no service_metadata_finished",
        );
        check(
            &[(0, 0, 0, &[1])],
            Some(true),
            Some(acknowledged(2)),
            "incomplete",
        );
        check(&[], Some(true), Some(acknowledged(0)), "no serve metadata");
    }

    #[test]
    fn a_segment_without_a_valid_header_is_rejected() {
        let path = directory(&[(0, 0, 0, &[1])], Some(true));
        fs::write(
            path.join("events-0.jsonl"),
            "{\"kind\":\"listening_ready\",\"service_sequence\":1}\n",
        )
        .unwrap();
        let metadata = stream_metadata(&path, &mut Vec::new()).unwrap();
        assert!(
            metadata
                .error
                .as_deref()
                .unwrap()
                .contains("no valid header")
        );
        fs::remove_dir_all(path).unwrap();
    }
}
