//! A real-time stand-in for one long-lived `lamp-voice serve` runtime, used to
//! test the serve-session runner offline. It speaks serve's CLI shape
//! (`serve CONFIG OUT --cue-socket S`), emits cues through lamp-voice's own
//! `CueSink`, writes serve-shaped rolling metadata
//! (`OUT/metadata/events-{0..3}.jsonl` with a segment header and
//! `service_sequence`, plus `health.json`), stops on SIGTERM with a final
//! `run_end` and a `service_metadata_finished` stdout line, and models one
//! physical-mute service cycle. The conversation is the fake runtime;
//! stimuli arrive as fake room datagrams. It is a policy simulation of the
//! service shape, not lamp-voice, and it plays no audio.
use crate::{
    Result,
    events::{EventKind, RuntimeEvent, to_trace},
    fake::FakeRuntime,
    invalid,
    physical::fake_lamp::{BOOT_BYTES, FaultRelay, ROOM_MAX_BYTES},
    plan::{FakeProfile, FixedReply, Scenario},
    stimulus::SceneTiming,
};
use lamp_ipc::monotonic_us;
use lamp_turn_owner::{BootId, MonoTime};
use lamp_voice::fixture_provider::CueSink;
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt},
        net::UnixDatagram,
    },
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

/// The second cycle's runtime identity after a modelled physical mute.
const CYCLE_TWO_BOOT: [u8; 16] = [8; 16];
/// A startup provider Ready retained this long before readiness: older than
/// the cue bound, so the sink must treat it as history (production shape).
const STARTUP_READY_AGE_US: u64 = 150_000;

/// Service-level faults, each relative to a whole-turn completion of the
/// first cycle: `TURN:AFTER_MS[:DURATION_MS]`.
#[derive(Clone, Debug, Default)]
pub struct ServeFaults {
    /// Replaces the profile's V1 reopen delay.
    pub reopen_ms: Option<u32>,
    /// Provider link unavailable, then ready again after the duration.
    pub outage: Option<(u64, u32, u32)>,
    /// Provider link terminal: the service run fails.
    pub terminal: Option<(u64, u32)>,
    /// An input candidate that admits no turn.
    pub extra_candidate: Option<(u64, u32)>,
    /// Physical mute, then a new cycle after the duration.
    pub mute: Option<(u64, u32, u32)>,
    /// The runtime receives SIGTERM from outside the runner.
    pub stop: Option<(u64, u32)>,
    /// SIGTERM is ignored, so only a forced kill ends the runtime.
    pub ignore_sigterm: bool,
    /// A readiness status line printed at start, before the runtime is
    /// ready: stale stdout that must never become a readiness event.
    pub early_ready_stdout: bool,
    /// Exit with this code after a clean, acknowledged metadata finish.
    pub exit_code: Option<i32>,
}

impl ServeFaults {
    /// Take one `--fake-*` serve option; false when it is not one.
    pub fn parse(&mut self, flag: &str, value: &str) -> Result<bool> {
        let numbers = |count: usize| -> Result<Vec<u64>> {
            let parts: Vec<u64> = value
                .split(':')
                .map(str::parse)
                .collect::<std::result::Result<_, _>>()?;
            if parts.len() != count || parts[0] == 0 {
                return Err(invalid(&format!(
                    "{flag} needs {count} numbers, turn first"
                )));
            }
            Ok(parts)
        };
        let ms = |n: u64| u32::try_from(n).map_err(|_| invalid("fault delay too large"));
        match flag {
            "--fake-reopen-ms" => self.reopen_ms = Some(value.parse()?),
            "--fake-outage" => {
                let n = numbers(3)?;
                self.outage = Some((n[0], ms(n[1])?, ms(n[2])?));
            }
            "--fake-terminal" => {
                let n = numbers(2)?;
                self.terminal = Some((n[0], ms(n[1])?));
            }
            "--fake-extra-candidate" => {
                let n = numbers(2)?;
                self.extra_candidate = Some((n[0], ms(n[1])?));
            }
            "--fake-mute" => {
                let n = numbers(3)?;
                self.mute = Some((n[0], ms(n[1])?, ms(n[2])?));
            }
            "--fake-stop" => {
                let n = numbers(2)?;
                self.stop = Some((n[0], ms(n[1])?));
            }
            "--fake-ignore-sigterm" if value == "yes" => self.ignore_sigterm = true,
            "--fake-early-ready-stdout" if value == "yes" => self.early_ready_stdout = true,
            "--fake-exit-code" => self.exit_code = Some(value.parse()?),
            _ => return Ok(false),
        }
        Ok(true)
    }
    pub fn is_empty(&self) -> bool {
        self.reopen_ms.is_none()
            && self.outage.is_none()
            && self.terminal.is_none()
            && self.extra_candidate.is_none()
            && self.mute.is_none()
            && self.stop.is_none()
            && !self.ignore_sigterm
            && !self.early_ready_stdout
            && self.exit_code.is_none()
    }
}

pub struct FakeServe {
    pub scenario: Scenario,
    pub profile: FakeProfile,
    pub fixed_reply: FixedReply,
    pub seed: u64,
    pub output: PathBuf,
    pub cue_path: PathBuf,
    pub room: Option<PathBuf>,
    pub cue_fault: Option<String>,
    pub faults: ServeFaults,
}

/// Serve-shaped metadata: one segment (the fake never rotates), a header,
/// contiguous `service_sequence` values and every record kept for cues.
struct Metadata {
    segment: BufWriter<File>,
    health: PathBuf,
    sequence: u64,
    records: Vec<Value>,
}

impl Metadata {
    fn open(output: &std::path::Path) -> Result<Self> {
        let directory = output.join("metadata");
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let private = |name: String| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(directory.join(name))
        };
        let mut segment = BufWriter::new(private("events-0.jsonl".into())?);
        for slot in 1..4 {
            private(format!("events-{slot}.jsonl"))?;
        }
        private("health.json".into())?;
        writeln!(
            segment,
            "{}",
            json!({"kind":"service_segment","generation":0,"first_writer_event":0,
                "evidence":"rolling_best_effort_not_benchmark"})
        )?;
        Ok(Self {
            segment,
            health: directory.join("health.json"),
            sequence: 0,
            records: Vec::new(),
        })
    }
    fn record(&mut self, mut record: Value) -> Result<()> {
        self.sequence += 1;
        record["service_sequence"] = json!(self.sequence);
        serde_json::to_writer(&mut self.segment, &record)?;
        self.segment.write_all(b"\n")?;
        self.records.push(record);
        Ok(())
    }
    fn finish(&mut self) -> Result<Value> {
        self.segment.flush()?;
        self.segment.get_ref().sync_all()?;
        let health = json!({"acknowledged":true,"finish_attempted":true,"degraded":false,
            "writer_failed":false,"initialization_failed":false,"dropped_events":0,
            "oversized_events":0,"written_events":self.sequence,"unconfirmed_events":0});
        fs::write(
            &self.health,
            serde_json::to_vec(&json!({"writer_finished":true,
                "coordinator_finish_ack_required":true,"health":health,"latest_segment":0,
                "generation":0,"history_is_rolling":true,"complete_evidence":false}))?,
        )?;
        Ok(health)
    }
}

/// The service's runtime cycle: its identity, fake conversation and the
/// completion times its faults are scheduled from.
struct Cycle {
    number: u64,
    boot: [u8; 16],
    runtime: FakeRuntime,
    base_us: u64,
    ready: bool,
    completed: std::collections::BTreeMap<u64, u64>,
}

impl Cycle {
    fn new(number: u64, serve: &FakeServe, clock: &impl Fn() -> MonoTime) -> Self {
        let mut scenario = serve.scenario.clone();
        // A service has no session length; the relay bounds and stops it.
        scenario.session_seconds = crate::plan::MAX_SESSION_SECONDS;
        let mut runtime = FakeRuntime::new(
            serve.profile.clone(),
            &scenario,
            serve.fixed_reply.clone(),
            false,
            serve.seed + number,
        );
        if let Some(reopen_ms) = serve.faults.reopen_ms {
            runtime.set_input_reopen_ms(Some(reopen_ms));
        }
        Self {
            number,
            boot: if number == 1 {
                BOOT_BYTES
            } else {
                CYCLE_TWO_BOOT
            },
            runtime,
            base_us: clock().as_micros(),
            ready: false,
            completed: Default::default(),
        }
    }
    /// When this fault's turn completed plus its delay, if it is due.
    fn due(&self, fault: Option<(u64, u32)>, now_us: u64) -> bool {
        self.number == 1
            && fault.is_some_and(|(turn, after_ms)| {
                self.completed
                    .get(&turn)
                    .is_some_and(|at| now_us >= at + u64::from(after_ms) * 1000)
            })
    }
}

/// Runs until SIGTERM (or a modelled fault ends the service); returns the
/// process exit code: 0 after a stop, 1 after a failed run.
pub fn run(serve: FakeServe) -> Result<i32> {
    run_with_clock(serve, || MonoTime::from_micros(monotonic_us()))
}

/// Uses one read-only logical clock for service timestamps and cycle origins.
/// The caller owns time advancement; reads must not advance the clock. Signal
/// handling, socket I/O, metadata writes and the 2 ms host sleep are unchanged.
pub fn run_with_clock(serve: FakeServe, clock: impl Fn() -> MonoTime) -> Result<i32> {
    let signal = Arc::new(AtomicUsize::new(0));
    for number in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register_usize(number, Arc::clone(&signal), number as usize)?;
    }
    fs::DirBuilder::new().mode(0o700).create(&serve.output)?;
    let mut metadata = Metadata::open(&serve.output)?;
    let room = match &serve.room {
        Some(path) => {
            let socket = UnixDatagram::bind(path)?;
            socket.set_nonblocking(true)?;
            Some(socket)
        }
        None => None,
    };
    let mut fault_relay = serve
        .cue_fault
        .clone()
        .map(|fault| FaultRelay::new(&serve.output, &serve.cue_path, fault))
        .transpose()?;
    let sink_path = fault_relay
        .as_ref()
        .map_or_else(|| serve.cue_path.clone(), |relay| relay.path.clone());
    let mut cues = CueSink::connect(&sink_path)?;
    let stdout = std::io::stdout();
    if serve.faults.early_ready_stdout {
        let mut lock = stdout.lock();
        writeln!(
            lock,
            "{}",
            json!({"status":"listening_ready","seconds":null})
        )?;
        lock.flush()?;
    }
    let now = clock().as_micros();
    metadata.record(
        json!({"kind":"run_start","at_us":now,"mode":"serve","seconds":null,
        "provider_kind":"fake_gemini_like","cue_requested":true,"acoustic_score":null}),
    )?;
    let mut cycle = start_cycle(1, &serve, &mut metadata, &mut cues, &clock)?;
    let mut buffer = [0_u8; ROOM_MAX_BYTES];
    let mut failed: Option<String> = None;
    let mut outage_ends: Option<u64> = None;
    let mut fired = std::collections::BTreeSet::new();
    let mut muted_until: Option<u64> = None;
    loop {
        let now = clock().as_micros();
        if signal.load(Ordering::SeqCst) != 0 && !serve.faults.ignore_sigterm {
            metadata
                .record(json!({"kind":"service_stop_requested","at_us":now,"signal":"SIGTERM"}))?;
            cycle.runtime.shut_down("service_stopped");
            drain(&mut cycle, &mut metadata, &stdout)?;
            break;
        }
        if let Some(until) = muted_until {
            if now >= until {
                muted_until = None;
                metadata.record(json!({"kind":"service_privacy_state","at_us":now,
                    "state":"Allowed","audio_workers_open":false}))?;
                cycle = start_cycle(cycle.number + 1, &serve, &mut metadata, &mut cues, &clock)?;
            }
        } else {
            // Never stamp an event in the future: CueSink rejects such cues.
            while !cycle.runtime.ended()
                && cycle.base_us + cycle.runtime.now_us() + crate::fake::TICK_US <= now
            {
                cycle.runtime.tick();
            }
            if let Some(error) = drain(&mut cycle, &mut metadata, &stdout)? {
                failed = Some(error);
                break;
            }
            if cycle.due(serve.faults.outage.map(|(t, a, _)| (t, a)), now) && fired.insert("outage")
            {
                let (_, _, duration) = serve.faults.outage.expect("due outage exists");
                metadata.record(link("unavailable", now))?;
                outage_ends = Some(now + u64::from(duration) * 1000);
            }
            if outage_ends.is_some_and(|at| now >= at) {
                outage_ends = None;
                metadata.record(link("ready", now))?;
            }
            if cycle.due(serve.faults.extra_candidate, now) && fired.insert("candidate") {
                metadata.record(json!({"kind":"input_candidate","at_us":now,
                    "candidate":{"id":{"serial":999}},"note":"fake extra candidate"}))?;
            }
            if cycle.due(serve.faults.stop, now) && fired.insert("stop") {
                // An external stop takes the same signal path as a real one.
                rustix::process::kill_process(
                    rustix::process::getpid(),
                    rustix::process::Signal::TERM,
                )?;
            }
            if cycle.due(serve.faults.terminal, now) && fired.insert("terminal") {
                metadata.record(link("terminal", now))?;
                metadata.record(json!({"kind":"service_provider_ended","at_us":now,
                    "reason":"provider terminal (fake fault)"}))?;
                cycle.runtime.shut_down("runtime_failed");
                drain(&mut cycle, &mut metadata, &stdout)?;
                failed = Some("provider link terminal (fake fault)".into());
                break;
            }
            if cycle.due(serve.faults.mute.map(|(t, a, _)| (t, a)), now) && fired.insert("mute") {
                let (_, _, duration) = serve.faults.mute.expect("due mute exists");
                cycle.runtime.shut_down("privacy_revoked");
                drain(&mut cycle, &mut metadata, &stdout)?;
                metadata.record(json!({"kind":"service_cycle_muted","at_us":now,
                    "cycle":cycle.number,"cleanup_verified":true}))?;
                metadata.record(json!({"kind":"service_waiting_for_privacy","at_us":now,
                    "next_cycle":cycle.number + 1,"audio_workers_open":false}))?;
                muted_until = Some(now + u64::from(duration) * 1000);
            }
            // A muted cycle's runtime has ended; the service waits for privacy.
            if cycle.runtime.ended() && muted_until.is_none() {
                failed = Some("the fake runtime ended by itself".into());
                break;
            }
        }
        let _ = cues.scan(&metadata.records, clock().as_micros());
        if let Some(relay) = fault_relay.as_mut() {
            relay.tick(clock().as_micros())?;
        }
        if let Some(socket) = &room {
            while let Ok(count) = socket.recv(&mut buffer) {
                let timing: SceneTiming = serde_json::from_slice(&buffer[..count])?;
                if muted_until.is_none() {
                    let at = cycle.runtime.now_us();
                    cycle.runtime.inject(timing, None, at);
                }
            }
        }
        metadata.segment.flush()?;
        thread::sleep(Duration::from_millis(2));
    }
    let now = clock().as_micros();
    let signal_name = (signal.load(Ordering::SeqCst) != 0).then_some("SIGTERM");
    metadata.record(json!({"kind":"run_end","at_us":now,
        "status":if failed.is_some(){"failed"}else{"stopped_unscored"},
        "error":failed,"termination_signal":signal_name,"complete_evidence":false}))?;
    let _ = cues.scan(&metadata.records, clock().as_micros());
    if let Some(relay) = fault_relay.as_mut() {
        relay.tick(clock().as_micros())?;
    }
    let health = metadata.finish()?;
    let mut lock = stdout.lock();
    writeln!(
        lock,
        "{}",
        json!({"kind":"service_metadata_finished","health":health,
            "output":serve.output,"complete_evidence":false})
    )?;
    lock.flush()?;
    if let Some(path) = &serve.room {
        let _ = fs::remove_file(path);
    }
    Ok(if failed.is_some() {
        1
    } else {
        serve.faults.exit_code.unwrap_or(0)
    })
}

fn link(state: &str, at_us: u64) -> Value {
    json!({"kind":format!("provider_{state}"),"at_us":at_us,"coordinator_observed_at_us":at_us,
        "link":{"state":state},"note":"fake link change"})
}

/// Start one runtime cycle as the service supervisor does: privacy allowed,
/// a new cycle without claimed context continuity, a new runtime identity,
/// and a provider Ready retained through startup (history at readiness).
fn start_cycle(
    number: u64,
    serve: &FakeServe,
    metadata: &mut Metadata,
    cues: &mut CueSink,
    clock: &impl Fn() -> MonoTime,
) -> Result<Cycle> {
    let now = clock().as_micros();
    if number == 1 {
        metadata.record(json!({"kind":"service_waiting_for_privacy","at_us":now,
            "next_cycle":1,"audio_workers_open":false}))?;
        metadata.record(json!({"kind":"service_privacy_state","at_us":now,
            "state":"Allowed","audio_workers_open":false}))?;
    }
    metadata.record(
        json!({"kind":"service_cycle_start","at_us":now,"cycle":number,
        "fresh_runtime_required":true,"context_continuity_claimed":false}),
    )?;
    let cycle = Cycle::new(number, serve, clock);
    cues.set_boot(BootId::new(cycle.boot).map_err(|e| invalid(&format!("{e:?}")))?);
    let retained = now.saturating_sub(STARTUP_READY_AGE_US);
    metadata.record(json!({"kind":"provider_ready","at_us":retained,
        "coordinator_observed_at_us":retained,"link":{"session":number},
        "note":"retained during startup"}))?;
    Ok(cycle)
}

/// Record the cycle's new events in lamp-voice's vocabulary. The runtime's
/// own start and end are service-level here; its failure is returned.
fn drain(
    cycle: &mut Cycle,
    metadata: &mut Metadata,
    stdout: &std::io::Stdout,
) -> Result<Option<String>> {
    let mut failure = None;
    while let Some(mut event) = cycle.runtime.pop() {
        event.at_us = event.at_us.map(|at| cycle.base_us + at);
        match &event.kind {
            EventKind::RunStart { .. } => continue,
            EventKind::RunEnd { error, .. } => {
                failure = Some(error.clone().unwrap_or_else(|| "runtime ended".into()));
                continue;
            }
            EventKind::TurnCompleted { outcome, .. }
                if outcome != crate::events::PROVIDER_FAILED =>
            {
                if let (Some(turn), Some(at)) = (event.turn, event.at_us) {
                    cycle.completed.insert(turn, at);
                }
            }
            _ => {}
        }
        let record = trace_record(cycle, &event);
        if matches!(event.kind, EventKind::ListeningReady) {
            let mut lock = stdout.lock();
            writeln!(
                lock,
                "{}",
                json!({"status":"listening_ready","seconds":null})
            )?;
            lock.flush()?;
        }
        metadata.record(record)?;
    }
    Ok(failure)
}

/// One record as lamp-voice serve writes it. The link state readiness was
/// validated against is published as current state with readiness.
fn trace_record(cycle: &mut Cycle, event: &RuntimeEvent) -> Value {
    let owner = event
        .turn
        .map(|turn| json!({"boot": cycle.boot, "turn": turn, "generation": turn + 1}));
    match &event.kind {
        EventKind::ProviderLink { state } if !cycle.ready => {
            cycle.ready = true;
            json!({"kind":"provider_link_current","at_us":event.at_us,"state":state,
                "session":cycle.number,"basis":"coordinator link register at listening readiness"})
        }
        _ => to_trace(event, owner),
    }
}

#[cfg(test)]
#[path = "fake_serve_clock_tests.rs"]
mod clock_tests;
