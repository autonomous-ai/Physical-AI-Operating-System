//! Runner-side physical backend: drives one `lamp-session` per attempt through
//! an operator-supplied command, maps Lamp time onto the runner clock with
//! ping/pong, and starts cached stimuli on the explicit iMac speakers.
//!
//! In serve mode one attempt is one long-lived `lamp-live serve` runtime
//! carrying a whole multi-turn conversation. Each stimulus is prepared
//! (loaded, output opened in silence) before its trigger wait and started by
//! one original expiring grant that the player enforces at its first source
//! copy. The attempt ends with the relay's stop: SIGTERM, bounded grace,
//! forced kill, each retained separately.
//!
//! Runner software times (start requests, receipts) only locate stimuli; they
//! are not acoustic onsets. Room audio comes from a separate recorder.
use crate::{
    Result,
    events::{ClockDomain, Cue, EventKind, EventSource, RuntimeEvent, from_trace, parse_cue},
    import::load_room_evidence,
    invalid,
    physical::{
        fake_lamp::ROOM_MAX_BYTES,
        framing::{Frame, LineFramer, STREAM_LIMIT},
        protocol::{MAX_LINE_BYTES, MAX_SESSION_BYTES, RunnerLine, SessionLine},
    },
    plan::{ProviderKind, Scenario, Step},
    playback::PlaybackHistory,
    record::TriggerObservation,
    record::{ClockMapping, StimulusSource, Stratum},
    runner::{
        AttemptContext, Backend, Begin, Finished, Injection, MAX_TRIGGER_LATENESS_US, Withheld,
    },
    stimulus::{AssetIndex, SceneTiming, StimulusCatalog},
};
use lamp_ipc::{monotonic_ns, monotonic_us};
use lamp_observer::playback::GrantInput;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::net::UnixDatagram,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
    time::Duration,
};

/// Starts one stimulus. Implementations must return promptly; playback runs
/// on its own thread and its delivery report is collected at finish.
pub trait Player {
    fn start(
        &mut self,
        wav: &Path,
        timing: &SceneTiming,
        report_directory: &Path,
        cancel_file: &Path,
    ) -> Result<JoinHandle<Value>>;
    fn describe(&self) -> Value;
    /// Two-phase start: load and verify the WAV and open the output in
    /// silence now, returning its ready receipt. The stimulus starts only
    /// on one original grant through `PreparedStart::grant`. `None` when
    /// this player has no prepared start.
    fn prepare(
        &mut self,
        _wav: &Path,
        _timing: &SceneTiming,
        _report_directory: &Path,
        _cancel_file: &Path,
        _max_wait_ms: u32,
    ) -> Result<Option<PreparedStart>> {
        Ok(None)
    }
}

/// An armed two-phase player: its ready receipt, its grant channel and the
/// thread that returns its delivery report.
pub struct PreparedStart {
    pub ready: Value,
    pub grant: SyncSender<GrantInput>,
    pub handle: JoinHandle<Value>,
}

/// How long a prepared player may take to publish readiness. It runs before
/// the trigger wait, never between readiness and the start.
const PREPARE_READY_BOUND: Duration = Duration::from_secs(10);

/// Wait for a prepared player's ready receipt; a player that ends first or
/// stays silent is stopped and its failure returned.
fn await_ready(
    ready: &Receiver<Value>,
    grant: SyncSender<GrantInput>,
    handle: JoinHandle<Value>,
) -> Result<PreparedStart> {
    match ready.recv_timeout(PREPARE_READY_BOUND) {
        Ok(ready) => Ok(PreparedStart {
            ready,
            grant,
            handle,
        }),
        Err(_) => {
            let _ = grant.try_send(GrantInput::Closed);
            drop(grant);
            let report = if handle.is_finished() {
                handle
                    .join()
                    .unwrap_or_else(|_| json!({"status": "panicked"}))
            } else {
                json!({"status": "no ready receipt within 10 s; detached"})
            };
            Err(invalid(&format!(
                "prepared player not ready: {} {}",
                report["status"], report["error"]
            )))
        }
    }
}

/// Cached-WAV playback through `lamp-observer` on the explicit iMac speakers.
pub struct ObserverPlayer {
    pub output_device: String,
}

impl Player for ObserverPlayer {
    fn start(
        &mut self,
        wav: &Path,
        _: &SceneTiming,
        report_directory: &Path,
        cancel_file: &Path,
    ) -> Result<JoinHandle<Value>> {
        lamp_observer::playback::validate_output_name(&self.output_device)?;
        let options = lamp_observer::playback::PlayOptions {
            output_device: self.output_device.clone(),
            wav: wav.to_path_buf(),
            output: report_directory.to_path_buf(),
            cancel_file: Some(cancel_file.to_path_buf()),
            prepared_start_ms: None,
        };
        Ok(thread::spawn(move || {
            match lamp_observer::playback::play(&options) {
                Ok(report) => report,
                Err(error) => {
                    json!({"valid": false, "status": "failed", "error": error.to_string()})
                }
            }
        }))
    }
    fn describe(&self) -> Value {
        json!({"kind": "lamp-observer playback", "output_device": self.output_device,
            "boundary": "start request and CoreAudio callback times; not acoustic onset"})
    }
    /// In-process `play_prepared`: no process is created at the grant.
    fn prepare(
        &mut self,
        wav: &Path,
        _: &SceneTiming,
        report_directory: &Path,
        cancel_file: &Path,
        max_wait_ms: u32,
    ) -> Result<Option<PreparedStart>> {
        lamp_observer::playback::validate_output_name(&self.output_device)?;
        let options = lamp_observer::playback::PlayOptions {
            output_device: self.output_device.clone(),
            wav: wav.to_path_buf(),
            output: report_directory.to_path_buf(),
            cancel_file: Some(cancel_file.to_path_buf()),
            prepared_start_ms: Some(max_wait_ms),
        };
        let (grant, inputs) = mpsc::sync_channel(lamp_observer::playback::MAX_GRANT_LINES + 1);
        let (published, ready) = mpsc::sync_channel(1);
        let handle = thread::spawn(move || {
            let mut publish = |receipt: &Value| {
                published
                    .try_send(receipt.clone())
                    .map_err(std::io::Error::other)
            };
            let io = lamp_observer::playback::PreparedIo {
                ready: &mut publish,
                inputs,
            };
            match lamp_observer::playback::play_prepared(&options, io) {
                Ok(report) => report,
                Err(error) => {
                    json!({"valid": false, "status": "failed", "error": error.to_string()})
                }
            }
        });
        await_ready(&ready, grant, handle).map(Some)
    }
}

/// Direct-human prompts: at the trigger the operator console shows the line a
/// person in the room should say now. Nothing is played and no sound is made.
/// The prompt time is the only software boundary; reaction time and the actual
/// speech onset must come from room audio.
pub struct HumanPrompter {
    pub speaker_label: String,
}

impl Player for HumanPrompter {
    fn start(
        &mut self,
        _: &Path,
        timing: &SceneTiming,
        _: &Path,
        _: &Path,
    ) -> Result<JoinHandle<Value>> {
        let shown = monotonic_us();
        let lines: Vec<String> = timing.clips.iter().map(|clip| clip.text.clone()).collect();
        eprintln!(
            "\n>>> {} SAY NOW: {}\n",
            self.speaker_label,
            lines.join(" ... ")
        );
        let label = self.speaker_label.clone();
        Ok(thread::spawn(move || {
            json!({"valid": true, "status": "prompted_direct_human", "speaker_label": label,
                "speech_delivery_verified": false,
                "prompt_shown_host_monotonic_ns": shown * 1000, "lines": lines,
                "boundary": "prompt shown; human speech onset is later and unmeasured in software"})
        }))
    }
    fn describe(&self) -> Value {
        json!({"kind": "direct human prompts", "speaker_label": self.speaker_label,
            "note": "no playback; speech timing comes only from room audio"})
    }
}

/// Loopback player for the fake lamp-live: sends the stimulus timing to the
/// fake room socket instead of producing sound.
pub struct FakeRoomPlayer {
    pub room: PathBuf,
}

impl Player for FakeRoomPlayer {
    fn start(
        &mut self,
        _: &Path,
        timing: &SceneTiming,
        _: &Path,
        _: &Path,
    ) -> Result<JoinHandle<Value>> {
        let bytes = serde_json::to_vec(timing)?;
        if bytes.len() > ROOM_MAX_BYTES {
            return Err(invalid("stimulus timing exceeds the fake room datagram"));
        }
        let requested = monotonic_us();
        UnixDatagram::unbound()?.send_to(&bytes, &self.room)?;
        Ok(thread::spawn(
            move || json!({"valid": true, "status": "fake_room_delivery", "start_requested_host_monotonic_ns": requested * 1000}),
        ))
    }
    fn describe(&self) -> Value {
        json!({"kind": "fake room datagrams", "note": "loopback test; no audio"})
    }
}

/// A deliberate fault in one prepared fake start, by 1-based prepare order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FakePlayerFault {
    /// The first copy is reached this long after the grant arrives.
    LateFirstCopy { delay_ms: u32 },
    /// The player never becomes ready.
    PrepareFails,
}

/// Loopback two-phase player for the fake lamp-live. It mirrors the
/// observer's prepared contract (ready receipt, one original grant for its
/// attempt, CLOCK_MONOTONIC expiry checked with a fresh sample immediately
/// before the first copy) and "copies" by sending the stimulus timing to the
/// fake room socket. It is not the observer and plays no audio.
pub struct FakePreparedRoomPlayer {
    pub room: PathBuf,
    pub faults: Vec<(usize, FakePlayerFault)>,
    pub prepared: usize,
}

impl Player for FakePreparedRoomPlayer {
    fn start(
        &mut self,
        _: &Path,
        _: &SceneTiming,
        _: &Path,
        _: &Path,
    ) -> Result<JoinHandle<Value>> {
        Err(invalid("the prepared fake player starts only on a grant"))
    }
    fn describe(&self) -> Value {
        json!({"kind": "fake prepared room datagrams",
            "note": "loopback test; mirrors the prepared grant contract; no audio",
            "faults": self.faults.iter().map(|(n, f)| json!({"prepare": n, "fault": format!("{f:?}")})).collect::<Vec<_>>()})
    }
    fn prepare(
        &mut self,
        wav: &Path,
        timing: &SceneTiming,
        _: &Path,
        cancel_file: &Path,
        max_wait_ms: u32,
    ) -> Result<Option<PreparedStart>> {
        self.prepared += 1;
        let number = self.prepared;
        let fault = self
            .faults
            .iter()
            .find(|(n, _)| *n == number)
            .map(|(_, f)| *f);
        let bytes = serde_json::to_vec(timing)?;
        if bytes.len() > ROOM_MAX_BYTES {
            return Err(invalid("stimulus timing exceeds the fake room datagram"));
        }
        let source_sha256 = lamp_acoustic::hash(&fs::read(wav)?);
        let room = self.room.clone();
        let cancel_file = cancel_file.to_path_buf();
        let attempt_id = format!("fake-{number}-{}", monotonic_ns());
        let (grant, inputs) = mpsc::sync_channel::<GrantInput>(9);
        let (published, ready) = mpsc::sync_channel(1);
        let handle = thread::spawn(move || {
            let ready_ns = monotonic_ns();
            let mut report = json!({"attempt_id": attempt_id, "valid": false,
                "scope": "fake prepared start; no audio", "counters": {"source_started_host_ns": 0}});
            if fault == Some(FakePlayerFault::PrepareFails) {
                report["status"] = json!("failed");
                report["error"] = json!("fake prepare failure");
                return report;
            }
            let receipt = json!({"schema": 1, "kind": "prepared_ready", "attempt_id": attempt_id,
                "ready_monotonic_ns": ready_ns, "clock": "CLOCK_MONOTONIC",
                "source_sha256": source_sha256, "max_wait_ms": max_wait_ms});
            report["prepared_start"] = json!({"ready": receipt});
            if published.try_send(receipt).is_err() {
                report["status"] = json!("ready_not_published");
                return report;
            }
            let input = inputs.recv_timeout(Duration::from_millis(u64::from(max_wait_ms)));
            let received = monotonic_ns();
            let decision = match input {
                Err(_) => Err("grant_timeout"),
                Ok(GrantInput::Closed) => Err("stopped_before_grant"),
                Ok(GrantInput::Malformed) => Err("grant_refused"),
                Ok(GrantInput::Line(line)) => {
                    let grant: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
                    let expires = grant["expires_monotonic_ns"].as_u64();
                    match expires {
                        _ if grant["schema"] != 1 || grant["kind"] != "start" => {
                            Err("grant_refused")
                        }
                        _ if grant["attempt_id"] != attempt_id => Err("grant_refused"),
                        Some(expires)
                            if expires > received
                                && expires - received
                                    <= lamp_observer::playback::MAX_GRANT_HORIZON_NS =>
                        {
                            Ok(expires)
                        }
                        _ => Err("grant_refused"),
                    }
                }
            };
            let expires = match decision {
                Ok(expires) => expires,
                Err(status) => {
                    report["status"] = json!(status);
                    return report;
                }
            };
            report["prepared_start"]["grant"] = json!({"decision": "start",
                "expires_monotonic_ns": expires, "received_monotonic_ns": received});
            if let Some(FakePlayerFault::LateFirstCopy { delay_ms }) = fault {
                thread::sleep(Duration::from_millis(u64::from(delay_ms)));
            }
            // The fresh sample at the first copy decides; equality refuses.
            let copy_ns = monotonic_ns();
            if copy_ns >= expires {
                report["status"] = json!("start_expired");
                report["first_copy_attempt_host_ns"] = json!(copy_ns);
                return report;
            }
            if cancel_file.exists() {
                report["status"] = json!("cancelled_before_first_copy");
                return report;
            }
            let sent = UnixDatagram::unbound().and_then(|socket| socket.send_to(&bytes, &room));
            report["counters"]["source_started_host_ns"] = json!(copy_ns);
            match sent {
                Ok(_) => {
                    report["valid"] = json!(true);
                    report["status"] = json!("fake_room_delivery");
                }
                Err(error) => {
                    report["status"] = json!("failed");
                    report["error"] = json!(error.to_string());
                }
            }
            report
        });
        await_ready(&ready, grant, handle).map(Some)
    }
}

#[derive(Clone, Debug)]
pub enum LampMode {
    Fixture {
        reply: String,
        sha256: String,
    },
    Directed {
        provider_config: String,
    },
    /// One `lamp-live serve` runtime per attempt, stopped by the runner.
    Serve {
        provider_config: String,
    },
}

pub struct PhysicalConfig {
    /// Argument vector that starts `lamp-voice-eval lamp-session ... --runtime
    /// LAMP_LIVE` on the Lamp, for example through the operator's SSH command.
    /// The runner appends `--work`, `--seconds` and mode arguments.
    pub lamp_command: Vec<String>,
    /// Absolute directory on the Lamp under which per-attempt work dirs are made.
    pub lamp_work_root: String,
    pub mode: LampMode,
    pub noise_suppression: String,
    pub diagnostics: bool,
    /// Optional recorder argv with `{out}` and `{seconds}` placeholders, e.g.
    /// the signed observer bundle. It must write `metadata.json` in `{out}`.
    pub room_recorder: Option<Vec<String>>,
    pub room_independent: bool,
    pub allowance_seconds: u16,
    /// Forwarded to lamp-live so ring cues are traced and checked.
    pub ring_channel_ceiling: Option<u16>,
    /// Loudspeaker replay or direct human speech; never pooled.
    pub stimulus_source: StimulusSource,
}

/// Relay lines taken in one final dispatch drain before the backend asks the
/// runner to revalidate instead of starting.
const MAX_DISPATCH_DRAIN_LINES: usize = 1024;

/// Keep refining the clock map during the session, within a bound.
const PING_INTERVAL_US: u64 = 250_000;
const MAX_PINGS: u64 = 400;

/// Scheduling identity only; this never changes Lamp conversation authority.
#[derive(Default)]
struct CueOrder {
    boot: Option<[u8; 16]>,
    sequence: u64,
    sent_us: u64,
    owners: BTreeMap<u64, CueOwner>,
    ended: bool,
}
struct CueOwner {
    generation: u64,
    endpoint: bool,
    playback: PlaybackHistory,
    terminal: bool,
}
impl CueOrder {
    fn accept(&mut self, cue: &Cue, lamp_received_us: u64) -> Result<()> {
        if lamp_received_us < cue.sent_us || lamp_received_us >= cue.expires_us {
            return Err(invalid("stale/future cue at Lamp relay"));
        }
        // A new boot ends this conversation even with a contiguous sequence:
        // cues alone do not verify a service restart, and its context is lost.
        if !self.ended
            && self.boot.is_some_and(|boot| boot != cue.boot)
            && self.sequence.checked_add(1) == Some(cue.sequence)
        {
            return Err(invalid(
                "cue boot changed (contiguous sequence): runtime context continuity lost",
            ));
        }
        if self.ended
            || self.sequence.checked_add(1) != Some(cue.sequence)
            || self.boot.is_some_and(|boot| boot != cue.boot)
            || cue.sent_us < self.sent_us
        {
            return Err(invalid("cue boot/sequence/time order changed"));
        }
        if let (Some(turn), Some(generation)) = (cue.turn, cue.generation) {
            if cue.kind == "input_admitted" {
                if self
                    .owners
                    .last_key_value()
                    .is_some_and(|(&old, state)| turn <= old || generation <= state.generation)
                {
                    return Err(invalid("cue admission did not advance its owner"));
                }
                self.owners.insert(
                    turn,
                    CueOwner {
                        generation,
                        endpoint: false,
                        playback: PlaybackHistory::default(),
                        terminal: false,
                    },
                );
            } else {
                let owner = self
                    .owners
                    .get_mut(&turn)
                    .ok_or_else(|| invalid("cue owner has no admitted input"))?;
                if owner.generation != generation || owner.terminal {
                    return Err(invalid("cue relabeled or revived a terminal owner"));
                }
                match cue.kind.as_str() {
                    "local_endpoint" if !owner.endpoint && !owner.playback.started() => {
                        owner.endpoint = true
                    }
                    "speaker_first_write" if owner.endpoint => {
                        let sequence =
                            cue.playback_sequence.filter(|n| *n > 0).ok_or_else(|| {
                                invalid("live playback cue has no occurrence identity")
                            })?;
                        owner
                            .playback
                            .start(Some(sequence), cue.event_us)
                            .map_err(invalid)?;
                    }
                    "speech_retired" => {
                        let sequence =
                            cue.playback_sequence.filter(|n| *n > 0).ok_or_else(|| {
                                invalid("live retirement cue has no occurrence identity")
                            })?;
                        owner
                            .playback
                            .retire(Some(sequence), cue.event_us)
                            .map_err(invalid)?;
                    }
                    // Retiring all current PCM is not whole-turn completion.
                    "cancelled" => owner.terminal = true,
                    // A terminal provider failure needs no normal endpoint (an
                    // abandoned request ends on the drain path) and may follow
                    // a partial answer, whose audio must already be retired.
                    "turn_completed"
                        if cue.outcome.as_deref() == Some(crate::events::PROVIDER_FAILED)
                            && !owner.playback.active() =>
                    {
                        if owner
                            .playback
                            .occurrences
                            .last()
                            .and_then(|p| p.retired_us)
                            .is_some_and(|end| cue.event_us < end)
                        {
                            return Err(invalid("provider failure predates playback retirement"));
                        }
                        owner.terminal = true;
                    }
                    "turn_completed" if owner.endpoint && !owner.playback.active() => {
                        let audio = cue.outcome.as_deref() != Some("no_audio_answer");
                        if (audio && !owner.playback.complete())
                            || (!audio && owner.playback.started())
                            || owner
                                .playback
                                .occurrences
                                .last()
                                .and_then(|p| p.retired_us)
                                .is_some_and(|end| cue.event_us < end)
                        {
                            return Err(invalid(
                                "completion disagrees with playback occurrence history",
                            ));
                        }
                        owner.terminal = true;
                    }
                    _ => return Err(invalid("cue phase out of order or duplicated")),
                }
            }
        }
        self.boot = Some(cue.boot);
        self.sequence = cue.sequence;
        self.sent_us = cue.sent_us;
        self.ended = cue.kind == "run_end";
        Ok(())
    }
}

struct Session {
    child: Child,
    next_ping_us: u64,
    stdin: Option<ChildStdin>,
    lines: Receiver<std::result::Result<SessionLine, String>>,
    pings: Vec<(u64, u64)>,
    best: Option<ClockMapping>,
    trace: Vec<Value>,
    end: Option<SessionLine>,
    /// The transport closed without a relayed `session_end`.
    end_synthesized: bool,
    ready_seen: bool,
    protocol_errors: Vec<String>,
    start: Option<Value>,
    cues: Vec<Value>,
    cue_order: CueOrder,
    cue_failed: bool,
    /// Cues observed under a new boot, kept for provenance only.
    boot_changes: Vec<Value>,
    /// Serve only: the runner's stop line was sent.
    stop_sent: bool,
    /// The runtime's own `run_end` cue was accepted: it is ending by itself.
    runtime_ended: bool,
}

struct Playback {
    step: String,
    requested_us: u64,
    cancel_file: PathBuf,
    handle: Option<JoinHandle<Value>>,
    /// Collected during the session when the player ended early.
    report: Option<Value>,
    /// Started by a prepared grant: its start is the first source copy.
    prepared: bool,
}

/// The armed prepared player for the next step.
struct Armed {
    step: String,
    prepared: PreparedStart,
    cancel_file: PathBuf,
    armed_us: u64,
}

/// The longest a prepared player may wait for its grant (the observer's
/// own bound).
const MAX_PREPARED_WAIT_MS: u32 = 120_000;
/// How long a serve runtime that sent its own `run_end` may take to exit
/// before the runner stops it.
const SELF_END_BOUND_US: u64 = 10_000_000;

pub struct PhysicalBackend<'a> {
    config: PhysicalConfig,
    assets: &'a AssetIndex,
    player: Box<dyn Player + 'a>,
    session: Option<Session>,
    room: Option<(Child, PathBuf, u64)>,
    playbacks: Vec<Playback>,
    attempt_directory: Option<PathBuf>,
    seconds: u16,
    /// Events drained by the final dispatch check, returned by `next_event`.
    pending: std::collections::VecDeque<RuntimeEvent>,
    /// The first terminal cause seen on this attempt's session; no stimulus
    /// starts after it.
    unavailable: Option<Withheld>,
    /// Serve: the prepared player for the next step, or why there is none.
    armed: Option<Armed>,
    arm_failure: Option<(String, String)>,
    /// Prepared players that never received a grant, with their reports.
    unused: Vec<(String, Value, JoinHandle<Value>)>,
    /// The trigger and configured delay the next `inject` serves.
    trigger: Option<(TriggerObservation, u64)>,
    /// Serve: runner evidence loss already reported as an event.
    evidence_lost: bool,
}

/// Run-end status of a runner-generated protocol stop.
const CUE_PROTOCOL_INVALID: &str = "cue_protocol_invalid";

/// End scheduling immediately, retaining the actual trace at finish. This is a
/// runner-generated protocol stop, not a Lamp failure or an acoustic event.
fn cue_failure(session: &mut Session, reason: String, received_us: u64) -> RuntimeEvent {
    session.cue_failed = true;
    session.protocol_errors.push(reason.clone());
    RuntimeEvent::new(
        EventKind::RunEnd {
            status: Some(CUE_PROTOCOL_INVALID.into()),
            error: Some(reason),
        },
        None,
        received_us,
        ClockDomain::RunnerMonotonic,
        EventSource::Cue,
    )
}

impl<'a> PhysicalBackend<'a> {
    pub fn new(
        config: PhysicalConfig,
        assets: &'a AssetIndex,
        player: Box<dyn Player + 'a>,
    ) -> Self {
        Self {
            config,
            assets,
            player,
            session: None,
            room: None,
            playbacks: Vec::new(),
            attempt_directory: None,
            seconds: 0,
            pending: std::collections::VecDeque::new(),
            unavailable: None,
            armed: None,
            arm_failure: None,
            unused: Vec::new(),
            trigger: None,
            evidence_lost: false,
        }
    }

    fn serve(&self) -> bool {
        matches!(self.config.mode, LampMode::Serve { .. })
    }

    /// Stop an armed player that will never be granted; its report is kept.
    fn release_armed(&mut self) {
        if let Some(armed) = self.armed.take() {
            let _ = armed.prepared.grant.try_send(GrantInput::Closed);
            self.unused
                .push((armed.step, armed.prepared.ready, armed.prepared.handle));
        }
    }

    fn send_stop(&mut self, reason: &str) -> Result<()> {
        let Some(session) = self.session.as_mut() else {
            return Ok(());
        };
        let Some(stdin) = session.stdin.as_mut() else {
            return Ok(());
        };
        serde_json::to_writer(
            &mut *stdin,
            &RunnerLine::Stop {
                reason: reason.into(),
            },
        )?;
        stdin.write_all(b"\n")?;
        stdin.flush()?;
        session.stop_sent = true;
        Ok(())
    }

    /// Serve: a player that ended without delivering, or a room recorder that
    /// ended during the session, becomes one runner event; every later
    /// stimulus is then withheld. Reports are kept for `finish`.
    fn poll_evidence(&mut self) -> Option<RuntimeEvent> {
        if !self.serve() || self.evidence_lost {
            return None;
        }
        let mut lost = None;
        for playback in &mut self.playbacks {
            if playback.report.is_some()
                || !playback
                    .handle
                    .as_ref()
                    .is_some_and(JoinHandle::is_finished)
            {
                continue;
            }
            let report = playback
                .handle
                .take()
                .and_then(|handle| handle.join().ok())
                .unwrap_or_else(|| json!({"valid": false, "status": "player thread panicked"}));
            if report["valid"] != true && lost.is_none() {
                lost = Some((
                    format!("player:{}", playback.step),
                    format!(
                        "stimulus not delivered: {} {}",
                        report["status"], report["error"]
                    ),
                ));
            }
            playback.report = Some(report);
        }
        if lost.is_none()
            && let Some((child, _, _)) = self.room.as_mut()
            && let Ok(Some(status)) = child.try_wait()
        {
            lost = Some((
                "room_recorder".into(),
                format!("the room recorder ended during the session ({status})"),
            ));
        }
        let (source, reason) = lost?;
        self.evidence_lost = true;
        let mut event = RuntimeEvent::new(
            EventKind::EvidenceLost { source, reason },
            None,
            monotonic_us(),
            ClockDomain::RunnerMonotonic,
            EventSource::Runner,
        );
        event.received_us = event.at_us;
        Some(event)
    }

    /// Send the armed player's one original start grant. Its expiry is the
    /// planned start's original lateness bound on this host's
    /// CLOCK_MONOTONIC, placed from the Lamp event time at the earliest
    /// runner time the clock map allows. Transport and receipt add nothing.
    fn grant(&mut self, step: &Step) -> std::result::Result<(Playback, Value), Withheld> {
        let (observation, delay_us) = self
            .trigger
            .clone()
            .ok_or_else(|| Withheld::Stale("no trigger observation for this grant".into()))?;
        let mapping = self
            .session
            .as_ref()
            .and_then(|s| s.best.clone())
            .ok_or_else(|| {
                Withheld::Stale("no clock map places the original deadline on this host".into())
            })?;
        let event_at = observation
            .event_at_us
            .filter(|_| observation.event_domain == ClockDomain::LampMonotonic)
            .ok_or_else(|| {
                Withheld::Stale(
                    "the trigger has no Lamp event time; its original deadline cannot be placed"
                        .into(),
                )
            })?;
        let earliest_us = mapping
            .to_runner(event_at)
            .saturating_sub(mapping.uncertainty_us);
        let deadline_us = earliest_us + delay_us + MAX_TRIGGER_LATENESS_US;
        let armed = match self.armed.take() {
            Some(armed) if armed.step == step.id => armed,
            other => {
                self.armed = other;
                let reason = match &self.arm_failure {
                    Some((failed, reason)) if *failed == step.id => reason.clone(),
                    _ => "no prepared player is armed for this step".into(),
                };
                return Err(Withheld::PlayerFailed(reason));
            }
        };
        let now = monotonic_us();
        if now >= deadline_us {
            let late = now - deadline_us;
            self.armed = Some(armed);
            return Err(Withheld::Stale(format!(
                "the original deadline passed {late} us before the grant"
            )));
        }
        let expires_ns = deadline_us * 1000;
        let line = json!({"schema": 1, "kind": "start",
            "attempt_id": armed.prepared.ready["attempt_id"], "expires_monotonic_ns": expires_ns})
        .to_string();
        if armed
            .prepared
            .grant
            .try_send(GrantInput::Line(line))
            .is_err()
        {
            let report = armed.prepared.handle.join().unwrap_or(Value::Null);
            return Err(Withheld::PlayerFailed(format!(
                "the prepared player ended before its grant: {} {}",
                report["status"], report["error"]
            )));
        }
        let sent_us = monotonic_us();
        let receipt = json!({"ready": armed.prepared.ready,
            "armed_runner_us": armed.armed_us, "grant_sent_runner_us": sent_us,
            "expires_monotonic_ns": expires_ns,
            "deadline": {"event_at_lamp_us": event_at, "mapped_runner_us": mapping.to_runner(event_at),
                "clock_uncertainty_us": mapping.uncertainty_us, "earliest_event_runner_us": earliest_us,
                "delay_us": delay_us, "lateness_bound_us": MAX_TRIGGER_LATENESS_US,
                "headroom_at_grant_us": deadline_us - sent_us}});
        Ok((
            Playback {
                step: step.id.clone(),
                requested_us: sent_us,
                cancel_file: armed.cancel_file,
                handle: Some(armed.prepared.handle),
                report: None,
                prepared: true,
            },
            receipt,
        ))
    }

    /// Latch the first terminal cause from a live event: a provider failure,
    /// the session's end, or a cue/relay protocol failure.
    fn note(&mut self, event: &RuntimeEvent) {
        if self.unavailable.is_some() {
            return;
        }
        self.unavailable = match &event.kind {
            EventKind::TurnCompleted { outcome, .. }
                if outcome == crate::events::PROVIDER_FAILED =>
            {
                Some(Withheld::ProviderFailed(format!(
                    "turn {} ended with a provider failure before dispatch; no stimulus while the provider is unavailable",
                    event.turn.unwrap_or(0)
                )))
            }
            EventKind::RunEnd { status, error }
                if status.as_deref() == Some(CUE_PROTOCOL_INVALID) =>
            {
                Some(Withheld::ProtocolFailed(format!(
                    "cue or relay protocol failed before dispatch: {}",
                    error.as_deref().unwrap_or("no detail")
                )))
            }
            EventKind::RunEnd { .. } if event.source == EventSource::Cue => Some(
                Withheld::SessionEnded("the runtime sent its run_end cue before dispatch".into()),
            ),
            EventKind::RunEnd { .. } => Some(Withheld::SessionEnded(
                match self.session.as_ref().and_then(|s| s.end.as_ref()) {
                    Some(SessionLine::SessionEnd {
                        exit_code,
                        killed,
                        error,
                        ..
                    }) => format!(
                        "the relay ended the session before dispatch (exit status {}{}{})",
                        exit_code.map_or("unknown".into(), |c| c.to_string()),
                        if *killed { ", killed" } else { "" },
                        error.as_deref().map_or(String::new(), |e| format!(": {e}"))
                    ),
                    _ => "the runtime session ended before dispatch".into(),
                },
            )),
            _ => None,
        };
    }

    /// Convert one relayed line, live or in the final dispatch drain, and
    /// latch any terminal cause. A relay framing error is a protocol failure:
    /// the lost line may have been a cue or the session's end.
    fn take(&mut self, line: std::result::Result<SessionLine, String>) -> Option<RuntimeEvent> {
        // The relay streams the final trace only after the runtime exited or
        // was killed: live admission closes at the first record (MAIN-030).
        if matches!(line, Ok(SessionLine::Trace { .. })) && self.unavailable.is_none() {
            self.unavailable = Some(Withheld::SessionEnded(
                "the runtime has exited; the relay is streaming its final trace".into(),
            ));
        }
        let event = match line {
            Ok(line) => self.handle(line)?,
            Err(error) => {
                let session = self.session.as_mut()?;
                if session.cue_failed {
                    session.protocol_errors.push(error);
                    return None;
                }
                cue_failure(
                    session,
                    format!("relay protocol failure: {error}"),
                    monotonic_us(),
                )
            }
        };
        self.note(&event);
        Some(event)
    }

    fn send_ping(&mut self, seq: u64) -> Result<()> {
        let Some(session) = self.session.as_mut() else {
            return Ok(());
        };
        let Some(stdin) = session.stdin.as_mut() else {
            return Ok(());
        };
        let sent = monotonic_us();
        serde_json::to_writer(&mut *stdin, &RunnerLine::Ping { seq })?;
        stdin.write_all(b"\n")?;
        stdin.flush()?;
        session.pings.push((seq, sent));
        session.next_ping_us = sent + PING_INTERVAL_US;
        Ok(())
    }

    /// Convert one relay line into a runtime event, updating the clock map.
    fn handle(&mut self, line: SessionLine) -> Option<RuntimeEvent> {
        let received = monotonic_us();
        // Human speech has no inferred onset window. Its timing must be
        // reviewed; timed acceptance is withheld by the evaluator.
        let path_allowance = match self.config.stimulus_source {
            StimulusSource::DirectHuman => 0,
            _ => 250_000,
        };
        let session = self.session.as_mut()?;
        let mut event = match line {
            SessionLine::Pong { seq, lamp_us } => {
                if let Some(&(_, sent)) = session.pings.iter().find(|(s, _)| *s == seq) {
                    let rtt = received.saturating_sub(sent);
                    let midpoint = sent + rtt / 2;
                    let mapping = ClockMapping {
                        from: ClockDomain::RunnerMonotonic,
                        to: ClockDomain::LampMonotonic,
                        offset_us: lamp_us as i64 - midpoint as i64,
                        uncertainty_us: rtt / 2 + 1_000,
                        path_allowance_us: path_allowance,
                        method: format!(
                            "ping/pong over the session transport, best round trip {rtt} us"
                        ),
                    };
                    if session
                        .best
                        .as_ref()
                        .is_none_or(|best| mapping.uncertainty_us < best.uncertainty_us)
                    {
                        session.best = Some(mapping);
                    }
                }
                return None;
            }
            SessionLine::Cue {
                lamp_received_us,
                cue,
            } => {
                if session.cue_failed {
                    return None;
                }
                let result = (|| -> Result<Cue> {
                    if session.start.is_none() {
                        return Err(invalid("cue arrived without relay capability receipt"));
                    }
                    if session.cues.len() >= crate::physical::protocol::MAX_TRACE_LINES {
                        return Err(invalid("cue receipt limit exceeded"));
                    }
                    let parsed = parse_cue(&serde_json::to_vec(&cue)?)?;
                    // The Lamp relay validates local receipt; a clock map also
                    // rejects cues that expired while crossing the transport.
                    // The upper uncertainty bound must still be before expiry.
                    if let Some(map) = &session.best {
                        let upper_lamp_us = i128::from(received)
                            + i128::from(map.offset_us)
                            + i128::from(map.uncertainty_us);
                        if upper_lamp_us >= i128::from(parsed.expires_us) {
                            return Err(invalid(
                                "stale cue at runner (including clock uncertainty)",
                            ));
                        }
                    } else if parsed.turn.is_some() {
                        return Err(invalid("owned cue has no clock mapping to prove freshness"));
                    }
                    if session
                        .cue_order
                        .boot
                        .is_some_and(|boot| boot != parsed.boot)
                    {
                        session
                            .boot_changes
                            .push(json!({"sequence": parsed.sequence,
                            "kind": parsed.kind, "event_lamp_us": parsed.event_us,
                            "runner_received_us": received}));
                    }
                    session.cue_order.accept(&parsed, lamp_received_us)?;
                    // A terminal provider link ends a serve run as well.
                    session.runtime_ended |= parsed.kind == "run_end"
                        || (parsed.kind == "provider_link"
                            && parsed.link.as_deref() == Some("terminal"));
                    Ok(parsed)
                })();
                if session.cues.len() < crate::physical::protocol::MAX_TRACE_LINES {
                    session
                        .cues
                        .push(json!({"cue": cue, "lamp_received_us": lamp_received_us,
                        "runner_received_us": received, "accepted": result.is_ok(),
                        "error": result.as_ref().err().map(ToString::to_string)}));
                }
                match result {
                    Ok(cue) => cue.event()?,
                    Err(error) => cue_failure(session, error.to_string(), received),
                }
            }
            SessionLine::CueInvalid { error, .. } => {
                if session.cue_failed {
                    return None;
                }
                cue_failure(session, format!("relay rejected a cue: {error}"), received)
            }
            SessionLine::Stdout {
                lamp_received_us,
                line,
            } => {
                let status: Value = serde_json::from_str(&line).ok()?;
                if session.cue_failed
                    || session.start.is_none()
                    || status["status"] != "listening_ready"
                {
                    return None;
                }
                if std::mem::replace(&mut session.ready_seen, true) {
                    return None;
                }
                RuntimeEvent::new(
                    EventKind::ListeningReady,
                    None,
                    lamp_received_us,
                    ClockDomain::LampMonotonic,
                    EventSource::Stdout,
                )
            }
            SessionLine::StdoutInvalid { error, .. } => {
                if session.cue_failed {
                    return None;
                }
                // The unreadable line may have been readiness or a failure report.
                cue_failure(
                    session,
                    format!("relay could not deliver a runtime stdout line: {error}"),
                    received,
                )
            }
            SessionLine::Trace { record } => {
                if session.trace.len() < crate::physical::protocol::MAX_TRACE_LINES {
                    session.trace.push(record);
                }
                return None;
            }
            end @ SessionLine::SessionEnd { .. } => {
                let SessionLine::SessionEnd { lamp_us, .. } = end else {
                    unreachable!()
                };
                session.end = Some(end);
                // The authoritative relay end also closes attempts whose end cue was lost.
                RuntimeEvent::new(
                    EventKind::RunEnd {
                        status: None,
                        error: None,
                    },
                    None,
                    lamp_us,
                    ClockDomain::LampMonotonic,
                    EventSource::Stdout,
                )
            }
            start @ SessionLine::SessionStart { .. } => {
                let SessionLine::SessionStart {
                    schema,
                    ref mode,
                    cue_socket,
                    ..
                } = start
                else {
                    unreachable!()
                };
                let expected = match self.config.mode {
                    LampMode::Fixture { .. } => "fixture",
                    LampMode::Directed { .. } => "directed",
                    LampMode::Serve { .. } => "serve",
                };
                if session.start.is_some()
                    || schema != crate::physical::protocol::SCHEMA
                    || mode != expected
                    || !cue_socket
                {
                    cue_failure(
                        session,
                        "relay did not establish the requested cue capability".into(),
                        received,
                    )
                } else {
                    session.start = Some(serde_json::to_value(start).ok()?);
                    return None;
                }
            }
        };
        // Both modes report readiness on stdout and the cue socket.
        if matches!(event.kind, EventKind::ListeningReady)
            && event.source == EventSource::Cue
            && std::mem::replace(&mut session.ready_seen, true)
        {
            return None;
        }
        event.received_us = Some(received);
        Some(event)
    }

    fn wait_line(&mut self, until_us: u64) -> Option<std::result::Result<SessionLine, String>> {
        let session = self.session.as_mut()?;
        let now = monotonic_us();
        let received = if now >= until_us {
            session.lines.try_recv().map_err(|error| match error {
                TryRecvError::Empty => RecvTimeoutError::Timeout,
                TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
            })
        } else {
            session
                .lines
                .recv_timeout(Duration::from_micros(until_us - now))
        };
        match received {
            Ok(line) => Some(line),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => {
                if session.end.is_none() {
                    session.end_synthesized = true;
                    session.end = Some(SessionLine::SessionEnd {
                        lamp_us: 0,
                        exit_code: None,
                        killed: false,
                        events_sha256: None,
                        events_lines: 0,
                        error: Some("session transport closed without session_end".into()),
                        stray_processes_stopped: false,
                        serve: None,
                    });
                    return Some(Ok(session.end.clone()?));
                }
                None
            }
        }
    }
}

/// An error or panic between `begin` and `finish` must not leave the
/// transport, the room recorder or a player running. Closing the transport's
/// input makes the Lamp relay stop the runtime; the transport itself is
/// killed if it has not exited shortly afterwards.
impl Drop for PhysicalBackend<'_> {
    fn drop(&mut self) {
        self.release_armed();
        for playback in &self.playbacks {
            let _ = fs::write(&playback.cancel_file, b"cancel\n");
        }
        if let Some(mut session) = self.session.take() {
            drop(session.stdin.take());
            let _ = wait_child(&mut session.child, monotonic_us() + 2_000_000);
        }
        if let Some((mut child, _, _)) = self.room.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Frame relay lines with the protocol's per-line and per-session bounds on
/// a separate thread. Any framing or parse failure becomes a protocol error.
/// Above the session byte bound the reader stops, so the transport looks
/// closed and the attempt is invalidated rather than buffered without limit.
fn relay_reader<R: std::io::Read + Send + 'static>(
    stdout: R,
) -> Receiver<std::result::Result<SessionLine, String>> {
    let (sender, lines) = mpsc::sync_channel(4096);
    thread::spawn(move || {
        // Raw bytes are counted before framing, so even one endless line
        // without a newline ends the session at the bound.
        let mut framer = LineFramer::with_total_limit(stdout, MAX_LINE_BYTES, MAX_SESSION_BYTES);
        loop {
            let frame = match framer.next_frame() {
                Ok(Some(frame)) => frame,
                Ok(None) => break,
                Err(error) if error.to_string() == STREAM_LIMIT => {
                    let _ = sender.send(Err(format!(
                        "relay exceeded {MAX_SESSION_BYTES} bytes in one session; reading stopped"
                    )));
                    break;
                }
                Err(error) => {
                    let _ = sender.send(Err(format!("relay transport read failed: {error}")));
                    break;
                }
            };
            let parsed = match frame {
                Frame::Line(line) => serde_json::from_str::<SessionLine>(&line)
                    .map_err(|e| format!("{e}: {line:.200}")),
                other => Err(other
                    .failure("relay transport", MAX_LINE_BYTES)
                    .unwrap_or_default()),
            };
            if sender.send(parsed).is_err() {
                break;
            }
        }
    });
    lines
}

fn wait_child(child: &mut Child, deadline_us: u64) -> Option<std::process::ExitStatus> {
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if monotonic_us() >= deadline_us {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn join_bounded(handle: JoinHandle<Value>, cancel_file: &Path, deadline_us: u64) -> Value {
    while !handle.is_finished() && monotonic_us() < deadline_us {
        thread::sleep(Duration::from_millis(20));
    }
    if !handle.is_finished() {
        let _ = fs::write(cancel_file, b"cancel\n");
        let grace = monotonic_us() + 2_000_000;
        while !handle.is_finished() && monotonic_us() < grace {
            thread::sleep(Duration::from_millis(20));
        }
    }
    if handle.is_finished() {
        handle
            .join()
            .unwrap_or_else(|_| json!({"valid": false, "status": "player thread panicked"}))
    } else {
        json!({"valid": false, "status": "playback did not finish within its bound; detached"})
    }
}

impl Backend for PhysicalBackend<'_> {
    fn describe(&self) -> Value {
        json!({
            "kind": "physical",
            "mode": match &self.config.mode {
                LampMode::Fixture { sha256, .. } => json!({"provider": "fixed_reply", "reply_sha256": sha256}),
                LampMode::Directed { .. } => json!({"provider": "gemini", "cues": "schema-1 socket required; software scheduling only"}),
                LampMode::Serve { .. } => json!({"provider": "gemini", "runtime": "lamp-live serve, one per attempt",
                    "cues": "schema-1 socket required; software scheduling only",
                    "start": "prepared player, one original expiring grant enforced at its first source copy",
                    "stop": "relay SIGTERM to the runtime, bounded grace, forced group kill"}),
            },
            "stimulus_source": self.config.stimulus_source,
            "noise_suppression": self.config.noise_suppression,
            "lamp_diagnostics": self.config.diagnostics,
            "player": self.player.describe(),
            "room_recorder": self.config.room_recorder.as_ref().map(|argv| argv.first().cloned()),
            "room_recorder_independent_of_lamp": self.config.room_independent,
            "assets": self.assets.scenes().map(|a| json!({"scene": a.scene, "wav_sha256": a.wav_sha256})).collect::<Vec<_>>(),
        })
    }
    fn stratum(&self, _: &Scenario) -> Stratum {
        match self.config.mode {
            LampMode::Fixture { .. } => Stratum::PhysicalFixture,
            LampMode::Directed { .. } => Stratum::PhysicalGemini,
            LampMode::Serve { .. } => Stratum::PhysicalServe,
        }
    }
    fn domain(&self) -> ClockDomain {
        ClockDomain::RunnerMonotonic
    }
    fn source(&self) -> StimulusSource {
        self.config.stimulus_source
    }
    fn unsupported(&self, scenario: &Scenario) -> Option<String> {
        if !scenario.physical {
            return Some("needs provider fault injection, which only the fake runtime has".into());
        }
        match (&self.config.mode, scenario.provider) {
            (LampMode::Fixture { .. }, ProviderKind::Gemini) => {
                return Some("Gemini scenario cannot run on the fixed-reply provider".into());
            }
            (LampMode::Directed { .. } | LampMode::Serve { .. }, ProviderKind::FixedReply) => {
                return Some("fixed-reply scenario needs lamp-live directed-fixture".into());
            }
            _ => {}
        }
        scenario
            .steps
            .iter()
            .filter_map(|s| s.scene.as_deref())
            .find(|scene| self.assets.get(scene).is_none())
            .map(|scene| format!("no verified cached asset for scene {scene}; cached speech is not regenerated by the runner"))
    }
    fn begin(&mut self, context: &AttemptContext) -> Result<Begin> {
        let directory = context.run_directory.join(&context.attempt_id);
        crate::ledger::create_run_directory(&directory)?;
        self.attempt_directory = Some(directory.clone());
        self.seconds = context.scenario.session_seconds;
        self.playbacks.clear();
        self.pending.clear();
        self.unavailable = None;
        self.release_armed();
        self.arm_failure = None;
        self.trigger = None;
        self.evidence_lost = false;
        let total_seconds = u64::from(self.seconds) + u64::from(self.config.allowance_seconds) + 10;
        if let Some(template) = &self.config.room_recorder {
            let out = directory.join("room");
            let argv: Vec<String> = template
                .iter()
                .map(|a| {
                    a.replace("{out}", &out.display().to_string())
                        .replace("{seconds}", &total_seconds.to_string())
                })
                .collect();
            let requested = monotonic_us();
            let child = Command::new(&argv[0])
                .args(&argv[1..])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .spawn()?;
            // Stimuli start only after the recorder reports readiness.
            let ready_deadline = monotonic_us() + 15_000_000;
            while !out.join("ready.json").exists() && monotonic_us() < ready_deadline {
                thread::sleep(Duration::from_millis(50));
            }
            if !out.join("ready.json").exists() {
                let mut child = child;
                let _ = child.kill();
                let _ = child.wait();
                return Ok(Begin::Withheld(
                    "room recorder did not report ready.json within 15 s".into(),
                ));
            }
            self.room = Some((child, out, requested));
        }
        let mut argv = self.config.lamp_command.clone();
        let work = format!(
            "{}/{}",
            self.config.lamp_work_root.trim_end_matches('/'),
            context.attempt_id
        );
        argv.extend([
            "--work".into(),
            work,
            "--seconds".into(),
            self.seconds.to_string(),
            "--noise-suppression".into(),
            self.config.noise_suppression.clone(),
            "--allowance-seconds".into(),
            self.config.allowance_seconds.to_string(),
        ]);
        match &self.config.mode {
            LampMode::Fixture { reply, sha256 } => {
                argv.extend([
                    "--fixture-reply".into(),
                    reply.clone(),
                    "--fixture-sha256".into(),
                    sha256.clone(),
                ]);
            }
            LampMode::Directed { provider_config } => {
                argv.extend(["--provider-config".into(), provider_config.clone()]);
                if self.config.diagnostics {
                    argv.push("--diagnostics".into());
                }
            }
            LampMode::Serve { provider_config } => {
                argv.extend(["--serve-config".into(), provider_config.clone()]);
            }
        }
        if let Some(ceiling) = self.config.ring_channel_ceiling {
            argv.extend(["--ring-channel-ceiling".into(), ceiling.to_string()]);
        }
        let mut child = Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| invalid("session stdout unavailable"))?;
        let lines = relay_reader(stdout);
        self.session = Some(Session {
            stdin: child.stdin.take(),
            next_ping_us: 0,
            child,
            lines,
            pings: Vec::new(),
            best: None,
            trace: Vec::new(),
            end: None,
            end_synthesized: false,
            ready_seen: false,
            protocol_errors: Vec::new(),
            start: None,
            cues: Vec::new(),
            cue_order: CueOrder::default(),
            cue_failed: false,
            boot_changes: Vec::new(),
            stop_sent: false,
            runtime_ended: false,
        });
        self.send_ping(1)?;
        fs::write(
            directory.join("lamp-command.json"),
            serde_json::to_vec_pretty(&json!({"argv": argv}))?,
        )?;
        Ok(Begin::Ready)
    }
    fn now_us(&mut self) -> u64 {
        monotonic_us()
    }
    fn next_event(&mut self, until_us: u64) -> Result<Option<RuntimeEvent>> {
        if let Some(event) = self.pending.pop_front() {
            return Ok(Some(event));
        }
        loop {
            if let Some(event) = self.poll_evidence() {
                return Ok(Some(event));
            }
            let (due, sent) = self.session.as_ref().map_or((u64::MAX, MAX_PINGS), |s| {
                (s.next_ping_us, s.pings.len() as u64)
            });
            if sent < MAX_PINGS && monotonic_us() >= due {
                // A failed write means the transport closed; session_end reports it.
                let _ = self.send_ping(sent + 1);
            }
            let mut wait_until = until_us.min(
                self.session
                    .as_ref()
                    .map_or(until_us, |s| s.next_ping_us.max(monotonic_us() + 1_000)),
            );
            if self.serve() {
                // Player and recorder ends are polled, not signalled.
                wait_until = wait_until.min(monotonic_us() + 20_000);
            }
            let Some(line) = self.wait_line(wait_until) else {
                if monotonic_us() >= until_us {
                    return Ok(None);
                }
                continue;
            };
            if let Some(event) = self.take(line) {
                return Ok(Some(event));
            }
            if monotonic_us() >= until_us {
                return Ok(None);
            }
        }
    }
    fn runner_time(&self, event: &RuntimeEvent) -> (u64, String) {
        let mapping = self.session.as_ref().and_then(|s| s.best.as_ref());
        match (mapping, event.at_us, event.domain) {
            (Some(mapping), Some(at), ClockDomain::LampMonotonic) => (
                mapping.to_runner(at),
                format!(
                    "Lamp event time mapped to the runner clock (+/- {} us)",
                    mapping.uncertainty_us
                ),
            ),
            _ => (
                event.received_us.unwrap_or_else(monotonic_us),
                "runner receipt time".into(),
            ),
        }
    }
    fn timing(&mut self, catalog: &StimulusCatalog, scene: &str) -> Result<SceneTiming> {
        let mut timing = catalog.estimate(scene)?;
        timing.measure(&self.assets.pcm(scene)?);
        Ok(timing)
    }
    /// Serve: prepare this step's stimulus now, before its trigger wait.
    /// A failure is kept and withholds the step at dispatch.
    fn arm(&mut self, step: &Step, scene: &str, timing: &SceneTiming) -> Result<()> {
        if !self.serve() {
            return Ok(());
        }
        self.release_armed();
        self.arm_failure = None;
        let asset = self
            .assets
            .get(scene)
            .ok_or_else(|| invalid("asset disappeared"))?;
        let directory = self
            .attempt_directory
            .clone()
            .ok_or_else(|| invalid("no attempt directory"))?;
        let cancel_file = directory.join(format!("cancel-{}", step.id));
        let max_wait_ms =
            (step.trigger.deadline_ms + step.trigger.delay_ms + 5_000).min(MAX_PREPARED_WAIT_MS);
        let armed_us = monotonic_us();
        let failure = match self.player.prepare(
            &asset.wav,
            timing,
            &directory.join(format!("play-{}", step.id)),
            &cancel_file,
            max_wait_ms,
        ) {
            Ok(Some(prepared)) if prepared.ready["source_sha256"] == asset.wav_sha256.as_str() => {
                self.armed = Some(Armed {
                    step: step.id.clone(),
                    prepared,
                    cancel_file,
                    armed_us,
                });
                return Ok(());
            }
            Ok(Some(prepared)) => {
                let reason = format!(
                    "prepared source {} is not the cached asset {}",
                    prepared.ready["source_sha256"], asset.wav_sha256
                );
                let _ = prepared.grant.try_send(GrantInput::Closed);
                self.unused
                    .push((step.id.clone(), prepared.ready, prepared.handle));
                reason
            }
            Ok(None) => "the player has no prepared start".into(),
            Err(error) => error.to_string(),
        };
        self.arm_failure = Some((step.id.clone(), failure));
        Ok(())
    }
    fn trigger_observed(&mut self, observation: &TriggerObservation, delay_us: u64) {
        self.trigger = Some((observation.clone(), delay_us));
    }
    fn inject(
        &mut self,
        step: &Step,
        scene: &str,
        timing: &SceneTiming,
        at_us: u64,
    ) -> Result<Injection> {
        let asset = self
            .assets
            .get(scene)
            .ok_or_else(|| invalid("asset disappeared"))?;
        let directory = self
            .attempt_directory
            .clone()
            .ok_or_else(|| invalid("no attempt directory"))?;
        while monotonic_us() < at_us {
            let remaining = at_us.saturating_sub(monotonic_us());
            thread::sleep(Duration::from_micros(remaining.min(1_000)));
        }
        // Final dispatch check: take the lines already relayed, within a
        // budget (keeping the events for the runner), and refuse to start
        // after any terminal cause latched on this attempt, including one
        // first seen in this drain.
        let mut exhausted = true;
        for _ in 0..MAX_DISPATCH_DRAIN_LINES {
            let Some(line) = self.wait_line(monotonic_us()) else {
                exhausted = false;
                break;
            };
            if let Some(event) = self.take(line) {
                self.pending.push_back(event);
            }
        }
        if self.session.is_none() {
            self.unavailable
                .get_or_insert_with(|| Withheld::SessionEnded("no runtime session is open".into()));
        }
        let refuse = |withheld: Withheld, pending: usize, exhausted: bool| {
            let now = monotonic_us();
            Injection {
                started_us: now,
                receipt: json!({"wav_sha256": asset.wav_sha256, "dispatched": false,
                    "withheld_cause": withheld.cause(), "reason": withheld.reason(),
                    "unobserved_events": pending, "drain_exhausted": exhausted,
                    "late_us": now.saturating_sub(at_us)}),
                withheld: Some(withheld),
            }
        };
        // Never start while the runner has not observed every consumed
        // event, or while lines remain undrained: one may end the overlap or
        // the session (MAIN-025, MAIN-030).
        let withheld = self
            .unavailable
            .clone()
            .or_else(|| (exhausted || !self.pending.is_empty()).then_some(Withheld::Revalidate));
        if let Some(withheld) = withheld {
            return Ok(refuse(withheld, self.pending.len(), exhausted));
        }
        let cancel_file = directory.join(format!("cancel-{}", step.id));
        // The original target's lateness bound holds at the side effect
        // itself, even when no new event surfaced (MAIN-030).
        let requested_us = monotonic_us();
        if requested_us > at_us.saturating_add(MAX_TRIGGER_LATENESS_US) {
            let stale = Withheld::Stale(format!(
                "planned start passed {} ms before the player could start",
                (requested_us - at_us) / 1000
            ));
            return Ok(refuse(stale, 0, false));
        }
        if self.serve() {
            return Ok(match self.grant(step) {
                Ok((playback, prepared)) => {
                    let started_us = playback.requested_us;
                    self.playbacks.push(playback);
                    Injection {
                        withheld: None,
                        started_us,
                        receipt: json!({"wav_sha256": asset.wav_sha256, "dispatched": true,
                            "prepared": prepared, "late_us": started_us.saturating_sub(at_us),
                            "boundary": "grant sent; the player checks the expiry with a fresh CLOCK_MONOTONIC sample before its first source copy; the acoustic onset is later"}),
                    }
                }
                Err(withheld) => refuse(withheld, 0, false),
            });
        }
        let handle = self.player.start(
            &asset.wav,
            timing,
            &directory.join(format!("play-{}", step.id)),
            &cancel_file,
        )?;
        self.playbacks.push(Playback {
            step: step.id.clone(),
            requested_us,
            cancel_file,
            handle: Some(handle),
            report: None,
            prepared: false,
        });
        Ok(Injection {
            withheld: None,
            started_us: requested_us,
            receipt: json!({"wav_sha256": asset.wav_sha256, "requested_runner_us": requested_us,
                "late_us": requested_us.saturating_sub(at_us),
                "boundary": "runner start request; the player's own start request and the acoustic onset are later"}),
        })
    }
    fn finish(&mut self, context: &AttemptContext) -> Result<Finished> {
        if self.serve() {
            // Nothing more is granted; the runtime is stopped through the
            // relay's one stop path. A failed write means the transport is
            // already closed, which the relay treats as a stop.
            self.release_armed();
            // A runtime that reported its own end is not stopped by the
            // runner, so its end keeps its own provenance; one that does not
            // exit within the bound is then stopped like any other.
            if self
                .session
                .as_ref()
                .is_some_and(|s| s.runtime_ended && s.end.is_none())
            {
                let until = monotonic_us() + SELF_END_BOUND_US;
                while self.session.as_ref().is_some_and(|s| s.end.is_none())
                    && monotonic_us() < until
                {
                    if self.next_event(until)?.is_none() {
                        break;
                    }
                }
            }
            if self.session.as_ref().is_some_and(|s| s.end.is_none()) {
                let reason = if self.session.as_ref().is_some_and(|s| s.runtime_ended) {
                    "runtime_reported_end_but_did_not_exit"
                } else {
                    "runner_finished"
                };
                let _ = self.send_stop(reason);
            }
        }
        let bound = monotonic_us()
            + (u64::from(self.seconds) + u64::from(self.config.allowance_seconds) + 30) * 1_000_000;
        while self.session.as_ref().is_some_and(|s| s.end.is_none()) && monotonic_us() < bound {
            if self.next_event(bound)?.is_none() {
                break;
            }
        }
        // Keep the transport open until session_end has relayed the trace.
        while self.session.as_ref().is_some_and(|s| s.end.is_some()) {
            match self.wait_line(monotonic_us() + 300_000) {
                Some(Ok(line)) => {
                    let _ = self.handle(line);
                }
                _ => break,
            }
        }
        let Some(mut session) = self.session.take() else {
            // Session start failed: release the recorder and players, keep nothing.
            if let Some((mut child, _, _)) = self.room.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            for playback in self.playbacks.drain(..) {
                if let Some(handle) = playback.handle {
                    let _ = join_bounded(handle, &playback.cancel_file, monotonic_us());
                }
            }
            for (_, _, handle) in self.unused.drain(..) {
                let _ = join_bounded(handle, Path::new("/nonexistent"), monotonic_us());
            }
            return Ok(Finished {
                events: Vec::new(),
                clock: None,
                evidence: json!({"room_audio": null}),
                aborted: Some("no lamp session was running".into()),
                unmapped: 0,
                spoken_failure_notice: None,
                corrected_starts: Vec::new(),
                failed_deliveries: Vec::new(),
            });
        };
        drop(session.stdin.take());
        let exited = wait_child(&mut session.child, monotonic_us() + 5_000_000);
        // A partial trace must never be scored as a complete one.
        let aborted = match &session.end {
            _ if !session.protocol_errors.is_empty() => Some(format!(
                "invalid scheduling/transport evidence: {}",
                session.protocol_errors.join("; ")
            )),
            _ if session.start.is_none() => Some("no relay cue capability receipt".into()),
            None => Some(
                "lamp session did not report its end within the bound; transport killed".to_owned(),
            ),
            Some(_) if session.end_synthesized => {
                Some("session transport closed before session_end; the trace may be partial".into())
            }
            Some(SessionLine::SessionEnd {
                killed: true,
                error,
                ..
            }) => Some(match error {
                Some(error) => format!("the relay stopped the runtime: {error}"),
                None => "the relay killed the runtime at its hard deadline".into(),
            }),
            Some(SessionLine::SessionEnd {
                error: Some(error), ..
            }) => Some(format!("relay: {error}")),
            Some(SessionLine::SessionEnd { events_lines, .. })
                if *events_lines != session.trace.len() =>
            {
                Some(format!(
                    "received {} of {events_lines} trace lines",
                    session.trace.len()
                ))
            }
            _ => None,
        };
        let playback_bound = monotonic_us() + 25_000_000;
        let mut playbacks = Vec::new();
        let mut corrected = Vec::new();
        let mut failed = Vec::new();
        for playback in self.playbacks.drain(..) {
            let report = match (playback.report, playback.handle) {
                (Some(report), _) => report,
                (None, Some(handle)) => join_bounded(handle, &playback.cancel_file, playback_bound),
                (None, None) => json!({"valid": false, "status": "no player report"}),
            };
            if report["valid"] != true {
                // Nothing reliable reached the room; the step tested nothing.
                failed.push((
                    playback.step.clone(),
                    format!(
                        "stimulus not delivered: {} {}",
                        report["status"], report["error"]
                    ),
                ));
            } else if playback.prepared {
                // A prepared stream started before readiness; its stimulus
                // starts at the first source copy.
                if let Some(ns) = report["counters"]["source_started_host_ns"]
                    .as_u64()
                    .filter(|ns| *ns > 0)
                {
                    corrected.push((playback.step.clone(), ns / 1000));
                }
            } else if let Some(ns) = report["start_requested_host_monotonic_ns"].as_u64() {
                corrected.push((playback.step.clone(), ns / 1000));
            }
            playbacks.push(
                json!({"step": playback.step, "runner_requested_us": playback.requested_us,
                "prepared": playback.prepared, "report": report}),
            );
        }
        let unused: Vec<Value> = self
            .unused
            .drain(..)
            .map(|(step, ready, handle)| {
                let report = join_bounded(handle, Path::new("/nonexistent"), monotonic_us() + 5_000_000);
                json!({"step": step, "ready": ready, "report": report,
                    "note": "prepared, never granted; no source frame was submitted unless the report says otherwise"})
            })
            .collect();
        let room = match self.room.take() {
            Some((mut child, out, requested)) => {
                let status = wait_child(
                    &mut child,
                    monotonic_us() + (u64::from(self.seconds) + 60) * 1_000_000,
                );
                let mut evidence = load_room_evidence(
                    &out.join("metadata.json"),
                    self.config.room_independent,
                    Some(status.is_some_and(|s| s.success())),
                );
                evidence["recorder_spawn_runner_us"] = json!(requested);
                evidence["recorder_exit"] = json!(status.and_then(|s| s.code()));
                evidence
            }
            None => Value::Null,
        };
        // Serve metadata can span service cycles. Only the first cycle is
        // the conversation; later cycles are counted, never scored with it.
        let cycle_starts: Vec<usize> = session
            .trace
            .iter()
            .enumerate()
            .filter(|(_, record)| record["kind"] == "service_cycle_start")
            .map(|(index, _)| index)
            .collect();
        let scored_end = cycle_starts.get(1).copied().unwrap_or(session.trace.len());
        let mut unmapped = 0;
        let mut events = Vec::new();
        for record in &session.trace[..scored_end] {
            match from_trace(record) {
                Some(event) => events.push(event),
                None => unmapped += 1,
            }
        }
        let continuity = self.serve().then(|| {
            let cycles: Vec<&Value> = session
                .trace
                .iter()
                .filter(|record| {
                    matches!(
                        record["kind"].as_str(),
                        Some("service_cycle_start" | "service_cycle_muted" | "service_stop_requested")
                    )
                })
                .take(64)
                .collect();
            // A restart is verified only by the runtime's own metadata: a
            // muted first cycle, then a second cycle started no later than
            // the first cue seen under the new boot.
            let first_new_boot = session.boot_changes.first().and_then(|c| c["event_lamp_us"].as_u64());
            let muted = session.trace.iter().any(|r| r["kind"] == "service_cycle_muted" && r["cycle"] == 1);
            let restarted_at = session
                .trace
                .iter()
                .find(|r| r["kind"] == "service_cycle_start" && r["cycle"] == 2)
                .and_then(|r| r["at_us"].as_u64());
            let verified = muted
                && restarted_at.zip(first_new_boot).is_some_and(|(start, cue)| start <= cue);
            json!({"service_cycles": cycle_starts.len(),
                "context_continuity_lost": cycle_starts.len() > 1 || !session.boot_changes.is_empty(),
                "boot_changes_observed": session.boot_changes,
                "verified_service_restart": verified,
                "cycle_records": cycles,
                "unscored_records_after_first_cycle": session.trace.len() - scored_end,
                "note": "a new boot ends the conversation; cues alone never qualify a restart"})
        });
        // Terminal provider failures and drain-path endpoints are kept
        // verbatim (bounded) so provider session and outcome survive scoring.
        let provider_records: Vec<&Value> = session
            .trace
            .iter()
            .filter(|record| {
                record["kind"] == "input_discarded_endpoint"
                    || (record["kind"] == "turn_finished"
                        && record["outcome"] == crate::events::PROVIDER_FAILED)
            })
            .take(32)
            .collect();
        let evidence = json!({
            "provider_failure_records": provider_records,
            "session_start": session.start,
            "session_end": session.end,
            "cue_receipts": session.cues,
            "transport_exit": exited.map(|s| s.code()),
            "protocol_errors": session.protocol_errors,
            "playbacks": playbacks,
            "unused_prepared_players": unused,
            "serve_continuity": continuity,
            "room_audio": room,
            "attempt_directory": self.attempt_directory.as_ref().map(|d| d.display().to_string()),
        });
        let _ = context;
        Ok(Finished {
            events,
            clock: session.best.clone(),
            evidence,
            aborted,
            unmapped,
            spoken_failure_notice: None,
            corrected_starts: corrected,
            failed_deliveries: failed,
        })
    }
    fn reproduce(&self, context: &AttemptContext) -> String {
        format!(
            "cargo run -p lamp-voice-eval -- physical-run --scenario {} --attempt-seed {} (with the same physical options as run {})",
            context.scenario.id, context.seed, context.run_id
        )
    }
}

#[cfg(test)]
mod cue_tests {
    use super::*;

    fn cue(sequence: u64, kind: &str, turn: u64) -> Cue {
        Cue {
            schema: 1,
            sequence,
            kind: kind.into(),
            boot: [7; 16],
            turn: Some(turn),
            generation: Some(turn + 1),
            capture_epoch: Some(1),
            reference_epoch_context: Some(1),
            event_us: sequence * 10,
            sent_us: sequence * 10 + 1,
            expires_us: sequence * 10 + 100_000,
            reason: None,
            playback_sequence: matches!(kind, "speaker_first_write" | "speech_retired")
                .then_some(1),
            outcome: (kind == "turn_completed").then(|| "audio_written_unscored".into()),
            playback_gaps: (kind == "turn_completed").then_some(0),
            available: None,
            link: None,
        }
    }
    #[test]
    fn readiness_cues_are_unowned_but_still_sequenced() {
        let unowned = |sequence: u64, kind: &str| {
            let mut value = cue(sequence, kind, 1);
            value.turn = None;
            value.generation = None;
            value.available = (kind == "input_availability").then_some(true);
            value.link = (kind == "provider_link").then(|| "ready".into());
            value
        };
        let mut order = CueOrder::default();
        order.accept(&unowned(1, "provider_link"), 12).unwrap();
        order.accept(&unowned(2, "input_availability"), 22).unwrap();
        order.accept(&unowned(3, "input_candidate"), 32).unwrap();
        order.accept(&cue(4, "input_admitted", 1), 42).unwrap();
        // A gap in the sequence is still a protocol failure.
        assert!(order.accept(&unowned(6, "input_availability"), 62).is_err());
    }

    #[test]
    fn cue_order_rejects_missing_duplicate_relabelled_and_out_of_order_owners() {
        let mut order = CueOrder::default();
        order.accept(&cue(1, "input_admitted", 1), 12).unwrap();
        for bad in [
            cue(1, "input_admitted", 1),
            cue(3, "local_endpoint", 1),
            cue(2, "local_endpoint", 2),
            cue(2, "speech_retired", 1),
        ] {
            assert!(order.accept(&bad, 40).is_err(), "{bad:?}");
        }
        let mut bad_boot = cue(2, "local_endpoint", 1);
        bad_boot.boot = [8; 16];
        assert!(order.accept(&bad_boot, 25).is_err());
        let mut relabel = cue(2, "local_endpoint", 1);
        relabel.generation = Some(9);
        assert!(order.accept(&relabel, 25).is_err());
        order.accept(&cue(2, "local_endpoint", 1), 25).unwrap();
        order.accept(&cue(3, "speaker_first_write", 1), 35).unwrap();
        order.accept(&cue(4, "cancelled", 1), 45).unwrap();
        order.accept(&cue(5, "input_admitted", 2), 55).unwrap();
        assert!(order.accept(&cue(6, "speech_retired", 1), 65).is_err());
    }
    #[test]
    fn cue_receipt_at_expiry_or_before_send_is_not_fresh() {
        for received in [10, 100010] {
            assert!(
                CueOrder::default()
                    .accept(&cue(1, "input_admitted", 1), received)
                    .is_err()
            );
        }
    }
    #[test]
    fn retired_audio_can_be_cancelled_without_reviving_playback() {
        let mut order = CueOrder::default();
        for (index, kind) in [
            "input_admitted",
            "local_endpoint",
            "speaker_first_write",
            "speech_retired",
        ]
        .iter()
        .enumerate()
        {
            let c = cue(index as u64 + 1, kind, 1);
            order.accept(&c, c.sent_us + 1).unwrap();
        }
        assert!(order.accept(&cue(5, "speech_retired", 1), 55).is_err());
        assert!(order.accept(&cue(5, "speaker_first_write", 1), 55).is_err());
        order.accept(&cue(5, "cancelled", 1), 55).unwrap();
        assert!(order.accept(&cue(6, "cancelled", 1), 65).is_err());
        order.accept(&cue(6, "input_admitted", 2), 65).unwrap();
        assert!(order.accept(&cue(7, "speaker_first_write", 1), 75).is_err());
    }

    #[test]
    fn fresh_delayed_child_observation_keeps_its_earlier_event_time() {
        let mut order = CueOrder::default();
        order.accept(&cue(1, "input_admitted", 1), 12).unwrap();
        order.accept(&cue(2, "local_endpoint", 1), 22).unwrap();
        let mut observed = cue(3, "speaker_first_write", 1);
        // Child observation and coordinator event times are not a global
        // ordered clock: only the producer's send/sequence order is promised.
        observed.event_us = 19;
        observed.expires_us = 100019;
        order.accept(&observed, 32).unwrap();
        assert_eq!(observed.event().unwrap().at_us, Some(19));
        let mut regressed_send = cue(4, "speech_retired", 1);
        regressed_send.sent_us = 30;
        assert!(order.accept(&regressed_send, 42).is_err());
    }
    #[test]
    fn each_occurrence_is_matched_and_only_whole_completion_is_terminal() {
        let mut order = CueOrder::default();
        for (index, kind) in [
            "input_admitted",
            "local_endpoint",
            "speaker_first_write",
            "speech_retired",
        ]
        .iter()
        .enumerate()
        {
            let c = cue(index as u64 + 1, kind, 1);
            order.accept(&c, c.sent_us + 1).unwrap();
        }
        let mut start = cue(5, "speaker_first_write", 1);
        start.playback_sequence = Some(2);
        order.accept(&start, 52).unwrap();
        // A stale retirement cannot close occurrence two, nor authorize completion.
        assert!(order.accept(&cue(6, "speech_retired", 1), 62).is_err());
        assert!(order.owners[&1].playback.active());
        assert!(order.accept(&cue(6, "turn_completed", 1), 62).is_err());
        let mut retire = cue(6, "speech_retired", 1);
        retire.playback_sequence = Some(2);
        order.accept(&retire, 62).unwrap();
        // A rejected cue invalidates the physical stream; a fresh order tests clean completion.
        let mut clean = CueOrder::default();
        for (index, kind) in [
            "input_admitted",
            "local_endpoint",
            "speaker_first_write",
            "speech_retired",
            "speaker_first_write",
            "speech_retired",
            "turn_completed",
        ]
        .iter()
        .enumerate()
        {
            let mut c = cue(index as u64 + 1, kind, 1);
            if index == 4 || index == 5 {
                c.playback_sequence = Some(2);
            }
            clean.accept(&c, c.sent_us + 1).unwrap();
        }
        assert!(clean.accept(&cue(8, "cancelled", 1), 82).is_err());
        assert!(clean.accept(&cue(8, "speaker_first_write", 1), 82).is_err());
    }
    #[test]
    fn legacy_missing_occurrence_live_cue_is_withheld() {
        let mut order = CueOrder::default();
        order.accept(&cue(1, "input_admitted", 1), 12).unwrap();
        order.accept(&cue(2, "local_endpoint", 1), 22).unwrap();
        let mut old = cue(3, "speaker_first_write", 1);
        old.playback_sequence = None;
        assert!(order.accept(&old, 32).is_err());
    }
}

#[cfg(test)]
mod relay_reader_tests {
    use super::*;

    fn read_all(bytes: Vec<u8>) -> Vec<std::result::Result<SessionLine, String>> {
        relay_reader(std::io::Cursor::new(bytes)).iter().collect()
    }

    #[test]
    fn oversized_non_utf8_and_cut_off_lines_are_protocol_errors() {
        let pong = br#"{"type":"pong","seq":1,"lamp_us":5}"#;
        let mut bytes = pong.to_vec();
        bytes.push(b'\n');
        bytes.extend(vec![b'x'; MAX_LINE_BYTES + 1]);
        bytes.push(b'\n');
        bytes.extend(b"\xff\n");
        bytes.extend(pong);
        bytes.push(b'\n');
        bytes.extend(br#"{"type":"pong","#);
        let items = read_all(bytes);
        assert_eq!(items.len(), 5, "{items:?}");
        assert_eq!(items[0], Ok(SessionLine::Pong { seq: 1, lamp_us: 5 }));
        assert!(
            items[1]
                .as_ref()
                .unwrap_err()
                .contains("exceeds the 65536-byte limit")
        );
        assert!(items[2].as_ref().unwrap_err().contains("not valid UTF-8"));
        assert!(items[3].is_ok());
        assert!(
            items[4]
                .as_ref()
                .unwrap_err()
                .contains("closed inside a line")
        );
    }

    /// Endless lines of `width` bytes, generated without storing them.
    struct Lines {
        width: usize,
        position: usize,
    }
    impl std::io::Read for Lines {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            for byte in buffer.iter_mut() {
                *byte = if self.position % (self.width + 1) == self.width {
                    b'\n'
                } else {
                    b'y'
                };
                self.position += 1;
            }
            Ok(buffer.len())
        }
    }

    /// An endless stream with no newline at all.
    struct NoNewline;
    impl std::io::Read for NoNewline {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            buffer.fill(b'z');
            Ok(buffer.len())
        }
    }

    #[test]
    fn a_never_terminated_line_still_ends_at_the_session_byte_bound() {
        let lines = relay_reader(NoNewline);
        let item = lines
            .recv_timeout(Duration::from_secs(60))
            .expect("the reader kept consuming an unterminated line without limit");
        let error = item.unwrap_err();
        assert!(error.contains("reading stopped"), "{error}");
        assert!(lines.recv_timeout(Duration::from_secs(5)).is_err());
    }

    #[test]
    fn reading_stops_with_an_error_above_the_session_byte_bound() {
        // Rejected oversized lines still count, so an endless stream ends.
        let width = MAX_LINE_BYTES * 4;
        let items: Vec<_> = relay_reader(Lines { width, position: 0 }).iter().collect();
        let last = items.last().unwrap().as_ref().unwrap_err();
        assert!(last.contains("reading stopped"), "{last}");
        // Every complete line before the bound, then the stop.
        assert_eq!(
            items.len() as u64,
            MAX_SESSION_BYTES / (width as u64 + 1) + 1
        );
    }
}
