//! A real-time stand-in for `lamp-voice` used to test the physical orchestration
//! path (relay, cue socket, clock mapping, triggered playback, trace import)
//! without hardware. It speaks lamp-voice's CLI, emits cues through lamp-voice's
//! own `CueSink`, and writes an `events.jsonl` in lamp-voice's vocabulary. The
//! conversation is the fake runtime; "playback" arrives as datagrams on a fake
//! room socket instead of sound.
use super::fake_serve::{self, FakeServe, ServeFaults, ServePollProgress, ServePollTargets};
use crate::{
    Result,
    events::{EventKind, to_trace},
    fake::FakeRuntime,
    invalid,
    plan::{FakeContinuation, LoadedPlan},
    stimulus::{SceneTiming, StimulusCatalog},
};
use lamp_ipc::monotonic_us;
use lamp_turn_owner::{BootId, MonoTime};
use lamp_voice::fixture_provider::CueSink;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::{fs::DirBuilderExt, net::UnixDatagram},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

pub const ROOM_MAX_BYTES: usize = 2048;
pub(crate) const BOOT_BYTES: [u8; 16] = [7; 16];
/// Another runtime's identity for the `foreign-completion` cue fault.
const FOREIGN_BOOT: [u8; 16] = [9; 16];

/// Cue faults the bounded local shim can inject, on the first matching cue.
const CUE_FAULTS: [&str; 7] = [
    "drop-retirement",
    "stale-retirement",
    "drop-completion",
    "stale-completion",
    "duplicate-completion",
    "reorder-retirement",
    "foreign-completion",
];

/// `fake-lamp-voice --fake-scenario ID --fake-profile ID --fake-room SOCKET
/// [--fake-plan FILE] [--fake-seed N] [--fake-session-seconds N]
/// [--fake-cue-fault KIND] [serve faults, see `fake_serve`]
/// directed-fixture WAV SHA SECONDS OUT ... | directed CONFIG SECONDS OUT ...
/// | serve CONFIG OUT ...`
pub fn run(arguments: &[String]) -> Result<i32> {
    run_inner(
        arguments,
        true,
        || MonoTime::from_micros(monotonic_us()),
        |progress| {
            if !progress.ended {
                thread::sleep(Duration::from_millis(2));
            }
            Ok(())
        },
    )
}

/// Work completed by one finite fake-runtime poll. Counts are cumulative and
/// advance only after the corresponding operation succeeds. `at` is the poll's
/// original tick sample, not another clock read at this completion boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectedPoll {
    /// Starts at one; repeated polls at the same clock value remain distinct.
    pub poll: u64,
    pub at: MonoTime,
    pub runtime_us: u64,
    /// Complete runtime stdout lines, each written and flushed.
    pub stdout_lines: u64,
    /// Room datagrams decoded and injected into the runtime, not merely sent.
    pub room_inputs: u64,
    /// Events appended to the in-memory trace; any enabled cue scan is done.
    pub trace_records: usize,
    /// The runtime has ended. The final file is not yet written or synced;
    /// neither this flag nor returning from a hook proves a subprocess exited.
    pub ended: bool,
}

/// Run a finite fake runtime with an explicitly supplied clock and poll hook.
/// `now` must only read time; advancing time belongs to the fixture driver.
/// Each hook runs after ticks, event/stdout draining, CueSink, fault relay and
/// room-input consumption, in that order. It may acknowledge that work and
/// park until the next poll. It must return promptly on `ended` so the final
/// trace can be written and synced, and provide its own bounded host watchdog.
///
/// This entrypoint has no default sleep and rejects `serve`, whose runtime has
/// a separate system clock. The CLI and `run` retain their original clock and
/// two-millisecond polling. No command-line option selects this hook.
pub fn run_with_clock_and_poll(
    arguments: &[String],
    now: impl Fn() -> MonoTime,
    on_poll: impl FnMut(DirectedPoll) -> Result<()>,
) -> Result<i32> {
    run_inner(arguments, false, now, on_poll)
}

/// Run only the explicit controlled serve fixture. Parsing and its argv line
/// are shared with the ordinary fake; only this path flushes and counts that
/// line before entering the controlled serve engine. The engine's terminal
/// callback follows final metadata and stdout, but never proves process exit.
pub fn run_serve_with_controlled_clock(
    arguments: &[String],
    now: impl Fn() -> MonoTime,
    before_poll: impl FnMut() -> Result<ServePollTargets>,
    completed_poll: impl FnMut(ServePollProgress) -> Result<()>,
) -> Result<i32> {
    run_serve_with_argument_views(arguments, arguments, now, before_poll, completed_poll)
}

/// Preserve the actual controlled command's received argv, including its
/// control pair. Only the validated suffix is parsed as fake-runtime options;
/// the existing stdout record reports the original received vector once.
pub(super) fn run_serve_with_controlled_invocation(
    received: &[String],
    now: impl Fn() -> MonoTime,
    before_poll: impl FnMut() -> Result<ServePollTargets>,
    completed_poll: impl FnMut(ServePollProgress) -> Result<()>,
) -> Result<i32> {
    let arguments = controlled_serve_arguments(received)?;
    run_serve_with_argument_views(arguments, received, now, before_poll, completed_poll)
}

fn controlled_serve_arguments(received: &[String]) -> Result<&[String]> {
    match received {
        [flag, path, arguments @ ..]
            if flag == "--fixture-control" && Path::new(path).is_absolute() =>
        {
            Ok(arguments)
        }
        _ => Err(invalid(
            "a controlled serve invocation requires leading --fixture-control SOCKET",
        )),
    }
}

fn run_serve_with_argument_views(
    arguments: &[String],
    received: &[String],
    now: impl Fn() -> MonoTime,
    before_poll: impl FnMut() -> Result<ServePollTargets>,
    mut completed_poll: impl FnMut(ServePollProgress) -> Result<()>,
) -> Result<i32> {
    let parsed = parse_arguments(arguments)?;
    if !parsed.is_serve() {
        return Err(invalid("a controlled serve fake supports only serve runs"));
    }
    let argv_frames = emit_serve_argv(&mut std::io::stdout().lock(), received)?;
    fake_serve::run_with_controlled_clock(parsed.into_serve()?, now, before_poll, |mut progress| {
        progress.stdout_lines = progress
            .stdout_lines
            .checked_add(argv_frames)
            .ok_or_else(|| invalid("serve fixture stdout counter overflow"))?;
        completed_poll(progress)
    })
}

/// The receipt is returned only after the original JSON line and its LF flush.
pub(super) fn emit_serve_argv(out: &mut impl Write, arguments: &[String]) -> Result<u64> {
    writeln!(
        out,
        "{}",
        json!({"kind": "fake_lamp_argv", "argv": arguments})
    )?;
    out.flush()?;
    Ok(1)
}

struct FakeArguments<'a> {
    rest: &'a [String],
    plan: LoadedPlan,
    serve: ServeFaults,
    scenario: Option<String>,
    profile: Option<String>,
    room: Option<PathBuf>,
    seed: u64,
    session_override: Option<u16>,
    cue_fault: Option<String>,
    two_generations: bool,
}

impl FakeArguments<'_> {
    fn is_serve(&self) -> bool {
        self.rest.first().map(String::as_str) == Some("serve")
    }

    fn into_serve(self) -> Result<FakeServe> {
        let Self {
            rest,
            plan,
            serve,
            scenario,
            profile,
            room,
            seed,
            cue_fault,
            ..
        } = self;
        let cue = rest
            .iter()
            .position(|a| a == "--cue-socket")
            .and_then(|i| rest.get(i + 1))
            .map(PathBuf::from)
            .ok_or_else(|| invalid("the fake serve runtime needs --cue-socket"))?;
        let output = rest
            .get(2)
            .map(PathBuf::from)
            .ok_or_else(|| invalid("expected serve CONFIG OUT"))?;
        let scenario = plan
            .scenario(&scenario.ok_or_else(|| invalid("--fake-scenario required"))?)?
            .clone();
        let profile = plan
            .profile(&profile.ok_or_else(|| invalid("--fake-profile required"))?)?
            .clone();
        Ok(FakeServe {
            scenario,
            profile,
            fixed_reply: plan.plan.fixed_reply.clone(),
            seed,
            output,
            cue_path: cue,
            room,
            cue_fault,
            faults: serve,
        })
    }
}

fn parse_arguments(arguments: &[String]) -> Result<FakeArguments<'_>> {
    let mut plan_path = None;
    let mut serve = ServeFaults::default();
    let mut scenario = None;
    let mut profile = None;
    let mut room = None;
    let mut seed = 1;
    let mut session_override = None;
    let mut cue_fault = None;
    let mut two_generations = false;
    let mut index = 0;
    while index < arguments.len() && arguments[index].starts_with("--fake-") {
        let value = arguments
            .get(index + 1)
            .cloned()
            .ok_or_else(|| invalid("fake option needs a value"))?;
        match arguments[index].as_str() {
            "--fake-scenario" => scenario = Some(value),
            "--fake-profile" => profile = Some(value),
            "--fake-room" => room = Some(PathBuf::from(value)),
            "--fake-seed" => seed = value.parse()?,
            "--fake-session-seconds" => session_override = Some(value.parse::<u16>()?),
            "--fake-cue-fault" if CUE_FAULTS.contains(&value.as_str()) => cue_fault = Some(value),
            "--fake-generations" if value == "two" => two_generations = true,
            "--fake-plan" => plan_path = Some(PathBuf::from(value)),
            other if serve.parse(other, &value)? => {}
            other => return Err(invalid(&format!("unknown fake option {other}"))),
        }
        index += 2;
    }
    let rest = &arguments[index..];
    let catalog = StimulusCatalog::load()?;
    let plan = match &plan_path {
        Some(path) => LoadedPlan::parse(&fs::read_to_string(path)?, &catalog)?,
        None => LoadedPlan::default_plan(&catalog)?,
    };
    Ok(FakeArguments {
        rest,
        plan,
        serve,
        scenario,
        profile,
        room,
        seed,
        session_override,
        cue_fault,
        two_generations,
    })
}

fn run_inner(
    arguments: &[String],
    allow_serve: bool,
    now: impl Fn() -> MonoTime,
    mut on_poll: impl FnMut(DirectedPoll) -> Result<()>,
) -> Result<i32> {
    let parsed = parse_arguments(arguments)?;
    if parsed.is_serve() {
        if !allow_serve {
            return Err(invalid(
                "a controlled fake clock supports only finite directed runs",
            ));
        }
        // The exact arguments this child received, for argv forwarding tests.
        println!("{}", json!({"kind": "fake_lamp_argv", "argv": arguments}));
        return fake_serve::run(parsed.into_serve()?);
    }
    let FakeArguments {
        rest,
        plan,
        serve,
        scenario,
        profile,
        room,
        seed,
        session_override,
        cue_fault,
        two_generations,
    } = parsed;
    if !serve.is_empty() {
        return Err(invalid("serve faults need the fake serve runtime"));
    }
    let (seconds, output, cue_path) = match rest.first().map(String::as_str) {
        Some("directed-fixture") if rest.len() >= 5 => {
            let cue = rest
                .iter()
                .position(|a| a == "--cue-socket")
                .and_then(|i| rest.get(i + 1))
                .map(PathBuf::from);
            (rest[3].parse::<u16>()?, PathBuf::from(&rest[4]), cue)
        }
        Some("directed") if rest.len() >= 4 => {
            let cue = rest
                .iter()
                .position(|a| a == "--cue-socket")
                .and_then(|i| rest.get(i + 1))
                .map(PathBuf::from);
            (rest[2].parse::<u16>()?, PathBuf::from(&rest[3]), cue)
        }
        _ => {
            return Err(invalid(
                "expected lamp-voice directed-fixture or directed arguments",
            ));
        }
    };
    let mut scenario = plan
        .scenario(&scenario.ok_or_else(|| invalid("--fake-scenario required"))?)?
        .clone();
    scenario.session_seconds = session_override.unwrap_or(seconds);
    if two_generations {
        let reply = scenario
            .fake_replies
            .first_mut()
            .ok_or_else(|| invalid("two generations require a scripted Gemini reply"))?;
        reply.continuations = vec![FakeContinuation {
            delay_ms: 200,
            text: reply.text.clone(),
            speech_ms: reply.speech_ms,
        }];
        reply.text = "A synthetic first segment.".into();
        reply.speech_ms = 500;
    }
    let profile = plan
        .profile(&profile.ok_or_else(|| invalid("--fake-profile required"))?)?
        .clone();
    fs::DirBuilder::new().mode(0o700).create(&output)?;
    let room_path = room.clone();
    let room = match room {
        Some(path) => {
            let socket = UnixDatagram::bind(path)?;
            socket.set_nonblocking(true)?;
            Some(socket)
        }
        None => None,
    };
    let boot = BootId::new(BOOT_BYTES).map_err(|e| invalid(&format!("{e:?}")))?;
    // Only explicit fake fault tests interpose a bounded local datagram shim.
    // Nominal tests use the production CueSink directly, including directed mode.
    let mut fault_relay = match (&cue_path, cue_fault) {
        (Some(path), Some(fault)) => Some(FaultRelay::new(&output, path, fault)?),
        (None, Some(_)) => return Err(invalid("fake cue fault requires a cue socket")),
        _ => None,
    };
    let sink_path = fault_relay.as_ref().map(|f| f.path.clone()).or(cue_path);
    let mut cues = match sink_path {
        Some(path) => {
            let mut sink = CueSink::connect(&path)?;
            sink.set_boot(boot);
            Some(sink)
        }
        None => None,
    };
    let mut runtime = FakeRuntime::new(
        profile,
        &scenario,
        plan.plan.fixed_reply.clone(),
        false,
        seed,
    );
    let base = now().as_micros();
    let mut trace: Vec<Value> = Vec::new();
    let mut buffer = [0_u8; ROOM_MAX_BYTES];
    let mut failed = false;
    let stdout = std::io::stdout();
    let mut poll = 0;
    let mut stdout_lines = 0;
    let mut room_inputs = 0;
    loop {
        // Never stamp an event in the future: CueSink rejects such cues.
        let at = now();
        while !runtime.ended() && base + runtime.now_us() + crate::fake::TICK_US <= at.as_micros() {
            runtime.tick();
        }
        while let Some(event) = runtime.pop() {
            let mut event = event;
            event.at_us = event.at_us.map(|at| base + at);
            let owner = event
                .turn
                .map(|turn| json!({"boot": BOOT_BYTES, "turn": turn, "generation": turn + 1}));
            if matches!(event.kind, EventKind::ListeningReady) {
                let mut lock = stdout.lock();
                writeln!(
                    lock,
                    "{}",
                    json!({"status": "listening_ready", "seconds": seconds})
                )?;
                lock.flush()?;
                stdout_lines += 1;
            }
            if let EventKind::RunEnd {
                status: Some(status),
                ..
            } = &event.kind
            {
                failed = status == "failed";
            }
            trace.push(to_trace(&event, owner));
        }
        if let Some(sink) = cues.as_mut() {
            let _ = sink.scan(&trace, now().as_micros());
        }
        if let Some(relay) = fault_relay.as_mut() {
            relay.tick(now().as_micros())?;
        }
        if let Some(socket) = &room {
            while let Ok(count) = socket.recv(&mut buffer) {
                let timing: SceneTiming = serde_json::from_slice(&buffer[..count])?;
                runtime.inject(timing, None, runtime.now_us());
                room_inputs += 1;
            }
        }
        poll += 1;
        on_poll(DirectedPoll {
            poll,
            at,
            runtime_us: runtime.now_us(),
            stdout_lines,
            room_inputs,
            trace_records: trace.len(),
            ended: runtime.ended(),
        })?;
        if runtime.ended() {
            break;
        }
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("events.jsonl"))?;
    for record in &trace {
        serde_json::to_writer(&mut file, record)?;
        file.write_all(b"\n")?;
    }
    file.sync_all()?;
    if let Some(path) = room_path {
        let _ = fs::remove_file(path);
    }
    Ok(if failed { 1 } else { 0 })
}

/// `fake-room-recorder --out DIR --seconds N --fail-after-ms MS`: reports
/// readiness like the room recorder, records nothing, then fails after MS
/// without `metadata.json`. A recording-failure control only.
pub fn room_recorder(arguments: &[String]) -> Result<i32> {
    room_recorder_with_clock_and_wait(arguments, lamp_ipc::monotonic_ns, |duration, _| {
        thread::sleep(duration);
        Ok(())
    })
}

/// Explicit timing adapter for the recording-failure fixture. The declaration
/// samples `now_ns` only when requested, before the real ready-file write.
/// `wait_after_ready` runs once after that write with the entire original wait
/// duration and the same pure nanosecond clock. An adapter can take its own
/// fresh sample there; the declared capture end is not the recorder's exit.
///
/// The adapter must finish its wait before returning. This function still
/// writes no recording or final metadata and returns the configured exit code;
/// its return is not evidence that an owned child process has been reaped.
pub fn room_recorder_with_clock_and_wait(
    arguments: &[String],
    now_ns: impl Fn() -> u64,
    wait_after_ready: impl FnOnce(Duration, &dyn Fn() -> u64) -> Result<()>,
) -> Result<i32> {
    let value = |flag: &str| {
        arguments
            .iter()
            .position(|a| a == flag)
            .and_then(|i| arguments.get(i + 1))
            .cloned()
            .ok_or_else(|| invalid(&format!("fake-room-recorder needs {flag}")))
    };
    let optional = |flag: &str| value(flag).ok();
    let out = PathBuf::from(value("--out")?);
    let seconds: u64 = value("--seconds")?.parse()?;
    let fail_after_ms: u64 = value("--fail-after-ms")?.parse()?;
    // Like `open -W`, a wrapper may exit 0 whatever was recorded.
    let exit_code: i32 = optional("--exit-code").map_or(Ok(1), |v| v.parse())?;
    // A declared capture end, as the observer's ready receipt states it.
    let declared_end = optional("--declare-end-after-ms")
        .map(|ms| ms.parse::<u64>())
        .transpose()?
        .map(|ms| now_ns() + ms * 1_000_000);
    fs::DirBuilder::new().mode(0o700).create(&out)?;
    fs::write(
        out.join("ready.json"),
        json!({"fake": true, "note": "fake recorder; records nothing",
            "planned_capture_end_host_monotonic_ns": declared_end})
        .to_string(),
    )?;
    wait_after_ready(
        Duration::from_millis(fail_after_ms.min(seconds * 1000)),
        &now_ns,
    )?;
    Ok(exit_code)
}

#[cfg(test)]
#[path = "recorder_clock_tests.rs"]
mod recorder_clock_tests;

/// Start `ARGV` after `--delay-ms`, as an SSH transport does while it
/// connects: runner input written meanwhile waits in the pipe. Test support;
/// no network.
pub fn slow_transport(arguments: &[String]) -> Result<i32> {
    slow_transport_with_wait(arguments, |duration| {
        thread::sleep(duration);
        Ok(())
    })
}

/// Use an explicit wait for the complete transport delay, then perform the
/// original exec with the original arguments and inherited streams. The wait
/// runs once, after parsing; an error prevents exec and is returned unchanged.
/// It must provide its own bounded host watchdog when coordinating a fixture.
/// No clock is sampled here, and this entrypoint adds no CLI selection.
pub fn slow_transport_with_wait(
    arguments: &[String],
    wait: impl FnOnce(Duration) -> Result<()>,
) -> Result<i32> {
    use std::os::unix::process::CommandExt;
    let split = arguments
        .iter()
        .position(|a| a == "--")
        .filter(|split| *split + 1 < arguments.len())
        .ok_or_else(|| invalid("fake-slow-transport --delay-ms MS -- ARGV..."))?;
    let delay_ms: u64 = match &arguments[..split] {
        [flag, ms] if flag == "--delay-ms" => ms.parse()?,
        _ => return Err(invalid("fake-slow-transport --delay-ms MS -- ARGV...")),
    };
    wait(Duration::from_millis(delay_ms))?;
    let argv = &arguments[split + 1..];
    Err(std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .exec()
        .into())
}

/// One deliberately dropped or late production cue; no audio or external I/O.
/// The 150 ms delay leaves the original event/send/100 ms expiry untouched.
pub(crate) struct FaultRelay {
    input: UnixDatagram,
    output: UnixDatagram,
    pub(crate) path: PathBuf,
    fault: String,
    used: bool,
    pending: Option<(u64, Vec<u8>)>,
    /// `reorder-retirement`: held until the next cue has been sent.
    held: Option<Vec<u8>>,
}
impl FaultRelay {
    pub(crate) fn new(directory: &Path, target: &Path, fault: String) -> Result<Self> {
        let path = directory.join("fault.sock");
        let input = UnixDatagram::bind(&path)?;
        input.set_nonblocking(true)?;
        let output = UnixDatagram::unbound()?;
        output.set_nonblocking(true)?;
        output.connect(target)?;
        Ok(Self {
            input,
            output,
            path,
            fault,
            used: false,
            pending: None,
            held: None,
        })
    }
    fn send(&self, bytes: &[u8]) -> Result<()> {
        if self.output.send(bytes)? != bytes.len() {
            return Err(invalid("partial fake cue relay send"));
        }
        Ok(())
    }
    pub(crate) fn tick(&mut self, now_us: u64) -> Result<()> {
        if self.pending.as_ref().is_some_and(|(due, _)| now_us >= *due) {
            let (_, bytes) = self.pending.take().expect("pending delay checked");
            self.send(&bytes)?;
        }
        let mut bytes = [0_u8; 513];
        for _ in 0..64 {
            let count = match self.input.recv(&mut bytes) {
                Ok(count) => count,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error.into()),
            };
            let cue = crate::events::parse_cue(&bytes[..count])?;
            let selected = if self.fault.ends_with("completion") {
                "turn_completed"
            } else {
                "speech_retired"
            };
            if !self.used && cue.kind == selected {
                self.used = true;
                match self.fault.split('-').next() {
                    Some("stale") => {
                        self.pending = Some((now_us + 150_000, bytes[..count].to_vec()))
                    }
                    Some("duplicate") => {
                        self.send(&bytes[..count])?;
                        self.send(&bytes[..count])?;
                    }
                    Some("reorder") => self.held = Some(bytes[..count].to_vec()),
                    // Same sequence and times, another runtime's identity.
                    Some("foreign") => {
                        let mut value: Value = serde_json::from_slice(&bytes[..count])?;
                        value["boot"] = json!(FOREIGN_BOOT);
                        self.send(&serde_json::to_vec(&value)?)?;
                    }
                    _ => {}
                }
            } else {
                self.send(&bytes[..count])?;
                if let Some(held) = self.held.take() {
                    self.send(&held)?;
                }
            }
        }
        Ok(())
    }
}
impl Drop for FaultRelay {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
#[path = "fake_runtime_clock_tests.rs"]
mod clock_tests;

#[cfg(test)]
#[path = "slow_transport_clock_tests.rs"]
mod slow_transport_clock_tests;

#[cfg(test)]
#[path = "serve_invocation_tests.rs"]
mod invocation_tests;
