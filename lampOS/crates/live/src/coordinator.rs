//! Finite, explicitly directed voice qualification on real hardware.
//!
//! This integration has optional guarded ring cues, but no addressee inference or motors.
//! Refuse to present its directed-session results as general desk-conversation
//! qualification. Every worker failure ends the run and retains a failure trace.
use crate::{
    activity::{Activity, ObservedAudio, TurnDetector},
    admission::{
        AcceptanceBasis, Accepted, CaptureLineage, Context as AdmissionContext, Decision,
        InputAdmission, Rejection, Step as AdmissionStep, Update as AdmissionUpdate,
    },
    choreography::RingChoreographer,
    diagnostic_control::{self, DiagnosticFinished, DiagnosticStream},
    fixture_provider::{CueSink, Fixture, FixtureInfo, FixtureOptions},
    options::{
        AudioWorkerOptions, DirectedOptions, NoiseSuppression, PlayoutStart, SessionOptions,
        TurnTaking,
    },
    privacy::PrivacyGate,
    process::{SessionDirectory, Worker, new_boot},
    provider_flow::{MAX_REPLY_SAMPLES, OutputReceiver, PACKET_SAMPLES},
    provider_link::{ProviderLink, ProviderLinkState},
    provider_worker::{ProviderInput, ProviderOutput},
    ring_wire::{RingBlankReason, RingFeedback},
    termination::Termination,
    transport::{Channel, WorkerChannels},
    turn_taking::{Disposition, PlaybackMask},
    wire::{
        Control, MicrophoneFrame, PlaybackDiscardReason, PlaybackGapPhase, SpeakerChunk,
        WorkerEvent,
    },
};
use lamp_audio::playout::{StartGate, StartNeed};
use lamp_interaction::{
    AdmissionState, AdmittedInput, BootId, CaptureState, Controller, MonoTime, OutputKind,
    OutputOwner, OutputPermit, Permission, PlaybackToken, Snapshot, TurnOwner,
};
use lamp_ipc::monotonic_us;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
mod cloud;
mod ring_trace;
use cloud::{CloudAvailability, CloudRequest, RequestOutcome};
use ring_trace::RingTrace;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const MAX_DIRECTED_SECONDS: u64 = 600;
const INPUT_READINESS_TIMEOUT: Duration = Duration::from_secs(25);
const DIAGNOSTIC_SHUTDOWN_GRACE: Duration = Duration::from_millis(750);
const INPUT_LEASE_US: u64 = 100_000;
const MAX_TRACE_EVENTS: usize = 20_000;
const PROVIDER_PACKETS_PER_SLICE: usize = 2;
const PROVIDER_SLICE_US: u64 = 2_000;

/// A cloud burst must yield to capture/cancellation and the next speaker chunk.
/// This is cooperative: one bounded datagram decode can exceed the time target.
struct ProviderSlice {
    started_at_us: u64,
    received: usize,
}
impl ProviderSlice {
    fn new(started_at_us: u64) -> Self {
        Self {
            started_at_us,
            received: 0,
        }
    }

    fn receive(&mut self, channel: &mut Channel, at_us: u64) -> io::Result<Option<ProviderOutput>> {
        let elapsed = at_us
            .checked_sub(self.started_at_us)
            .ok_or_else(|| io::Error::other("provider service clock moved backwards"))?;
        if self.received >= PROVIDER_PACKETS_PER_SLICE || elapsed >= PROVIDER_SLICE_US {
            return Ok(None);
        }
        let event = channel.receive()?;
        if event.is_some() {
            self.received += 1;
        }
        Ok(event)
    }
}

struct Reply {
    owner: TurnOwner,
    cloud: CloudRequest,
    local_input_ended: bool,
    plan: OutputOwner,
    playback: Option<PlaybackToken>,
    pcm: VecDeque<i16>,
    generation_done: bool,
    provider_idle: bool,
    input_sequence: u64,
    had_audio: bool,
    final_chunk: Option<u64>,
    playback_gaps: u64,
    last_provider_audio_at_us: Option<u64>,
    start: StartGate,
    need: StartNeed,
}
impl Reply {
    fn append_provider_audio(&mut self, samples: Vec<i16>, received_at_us: u64) {
        self.start.audio_arrived(received_at_us);
        self.need.arrival(received_at_us, samples.len());
        // Generation completion is not turn completion. A new generation may
        // arrive while the previous generation's accepted final PCM is still
        // playing. Preserve that receipt identity until it actually retires.
        self.generation_done = false;
        self.pcm.extend(samples);
        self.last_provider_audio_at_us = Some(received_at_us);
    }

    fn retire_playback(&mut self, sequence: u64, final_frame: u64) -> io::Result<PlaybackToken> {
        if self.final_chunk != Some(sequence) || final_frame == 0 {
            return Err(io::Error::other(
                "speech retirement has no matching final chunk",
            ));
        }
        let token = self
            .playback
            .take()
            .ok_or_else(|| io::Error::other("speech retirement has no playback occurrence"))?;
        self.final_chunk = None;
        Ok(token)
    }

    fn may_start(&mut self, at_us: u64) -> bool {
        // The gate receipt describes a dispatchable first block. A lone held
        // tail, empty queue, or prior unretired final cursor is not a start.
        self.has_speaker_chunk() && self.start.open(at_us, self.pcm.len(), self.generation_done)
    }

    fn has_speaker_chunk(&self) -> bool {
        self.final_chunk.is_none()
            && !self.pcm.is_empty()
            && (self.generation_done || self.pcm.len() > 240)
    }

    fn start_trace(&self) -> Value {
        let opened = self.start.opened();
        json!({
            "horizon_us":self.start.horizon_us(),
            "scope":"first_dispatchable_chunk_of_reply; later generations do not rebuffer",
            "opened_at_us":opened.map(|event| event.at_us),
            "age_since_first_audio_us":opened.map(|event| event.held_us),
            "queued_frames_at_open":opened.map(|event| event.buffered_frames),
            "opened_by_complete_flag":opened.map(|event| event.by == lamp_audio::playout::OpenedBy::Complete),
            "included_final_holdback_frames":opened.map(|event| if event.by == lamp_audio::playout::OpenedBy::Complete {0} else {event.buffered_frames.min(240)}),
            "added_latency_us":null,
            "boundary":"coordinator queue condition; not accepted PCM or acoustic onset"
        })
    }

    // Retain at most one full block until generation completion identifies the
    // final block. The trace exposes this holdback separately from provider delay.
    fn next_speaker_chunk(&mut self) -> Option<(Vec<i16>, bool)> {
        // The speaker keeps one final cursor. Serialize playback occurrences
        // through its real retirement receipt instead of overwriting that
        // cursor when the same turn receives another Gemini generation.
        if !self.has_speaker_chunk() {
            return None;
        }
        let final_chunk = self.generation_done && self.pcm.len() <= 240;
        let mut samples = Vec::with_capacity(240);
        for _ in 0..240 {
            match self.pcm.pop_front() {
                Some(sample) => samples.push(sample),
                None => break,
            }
        }
        samples.resize(240, 0);
        Some((samples, final_chunk))
    }
    fn held_final_samples(&self) -> usize {
        if self.generation_done {
            0
        } else {
            self.pcm.len().min(240)
        }
    }
    fn delivery_outcome(&self) -> &'static str {
        if matches!(self.cloud.outcome, Some(RequestOutcome::Failed { .. })) {
            "provider_failed"
        } else if !self.had_audio {
            "no_audio_answer"
        } else if self.playback_gaps > 0 {
            "audio_written_with_playback_gaps_unscored"
        } else {
            "audio_written_unscored"
        }
    }
}
struct PendingSpeaker {
    sequence: u64,
    permit: OutputPermit,
    accepted: usize,
    final_chunk: bool,
}
struct Rig {
    provider: Worker,
    capture: Worker,
    speaker: Worker,
    privacy: Worker,
    ring: Option<Worker>,
    ring_channel_ceiling: Option<u16>,
    diagnostic_directory: Option<PathBuf>,
    noise_suppression: NoiseSuppression,
    turn_taking: TurnTaking,
    playout_start: PlayoutStart,
}
impl Rig {
    fn publish(&mut self, snapshot: Snapshot) -> io::Result<()> {
        for worker in [
            &mut self.speaker,
            &mut self.capture,
            &mut self.privacy,
            &mut self.provider,
        ]
        .into_iter()
        .chain(self.ring.iter_mut())
        {
            worker
                .channels
                .control
                .send(Control::Authority { snapshot })?;
        }
        Ok(())
    }
    fn running(&mut self) -> io::Result<()> {
        for worker in [
            &mut self.provider,
            &mut self.capture,
            &mut self.speaker,
            &mut self.privacy,
        ]
        .into_iter()
        .chain(self.ring.iter_mut())
        {
            worker.check_running()?;
        }
        Ok(())
    }
    fn shutdown(&mut self, trace: &mut Vec<Value>) -> io::Result<()> {
        let grace = if self.diagnostic_directory.is_some() {
            DIAGNOSTIC_SHUTDOWN_GRACE
        } else {
            Duration::from_millis(300)
        };
        let deadline = Instant::now() + grace;
        let stop_requested_at_us = monotonic_us();
        // Send urgent stop to every child before waiting on any one child.
        for worker in [
            &mut self.speaker,
            &mut self.capture,
            &mut self.provider,
            &mut self.privacy,
        ]
        .into_iter()
        .chain(self.ring.iter_mut())
        {
            if let Err(error) = worker.channels.control.send(Control::Stop) {
                let _ = record(
                    trace,
                    json!({"kind":"stop_delivery_error", "worker":worker.role,
                    "at_us":monotonic_us(), "error":error.to_string()}),
                );
            }
        }
        let mut failure = None;
        let mut exited = [false, false, false, false, self.ring.is_none()];
        let mut receipts: [Option<DiagnosticFinished>; 2] = [None, None];
        let mut ring_blank_confirmed = self.ring.is_none();
        let mut final_drain = false;
        loop {
            // Keep the 100 ms control channel fresh while independent diagnostic
            // writers finish. A serial wait could age a valid finish receipt.
            for (index, worker) in [
                &mut self.speaker,
                &mut self.capture,
                &mut self.provider,
                &mut self.privacy,
            ]
            .into_iter()
            .chain(self.ring.iter_mut())
            .enumerate()
            {
                for _ in 0..32 {
                    match worker.channels.control.receive::<WorkerEvent>() {
                        Ok(Some(WorkerEvent::DiagnosticsFinished { receipt })) if index < 2 => {
                            if receipts[index].replace(receipt).is_some() {
                                failure =
                                    Some(io::Error::other("duplicate diagnostic finish receipt"));
                            }
                        }
                        Ok(Some(WorkerEvent::Ring {
                            report:
                                report @ RingFeedback::Blanked {
                                    reason: RingBlankReason::Shutdown,
                                    ..
                                },
                        })) => {
                            if index != 4
                                || ring_blank_confirmed
                                || !report.confirms_shutdown(stop_requested_at_us, monotonic_us())
                            {
                                failure =
                                    Some(io::Error::other("invalid ring shutdown write receipt"));
                            } else {
                                ring_blank_confirmed = true;
                            }
                            let _ = record(
                                trace,
                                json!({"kind":"ring_shutdown_write","at_us":monotonic_us(),
                                "details":report,"boundary":"SPI write return; optical darkness unmeasured"}),
                            );
                        }
                        Ok(Some(event)) => {
                            if matches!(
                                event,
                                WorkerEvent::Fault { .. } | WorkerEvent::ReferenceFault { .. }
                            ) {
                                failure = Some(io::Error::other(format!(
                                    "{} failed during shutdown: {event:?}",
                                    worker.role
                                )));
                            }
                            let _ = record(
                                trace,
                                json!({"kind":"worker_shutdown_event", "worker":worker.role, "at_us":monotonic_us(), "details":event}),
                            );
                        }
                        Ok(None) => break,
                        Err(error) => {
                            failure = Some(error);
                            break;
                        }
                    }
                }
                if !exited[index] {
                    match worker.try_exit() {
                        Ok(Some(status)) => {
                            exited[index] = true;
                            if let Err(error) = worker.exit_result(status) {
                                let _ = record(
                                    trace,
                                    json!({"kind":"worker_exit_failed", "worker":worker.role,
                                    "at_us":monotonic_us(), "error":error.to_string()}),
                                );
                                failure = Some(error);
                            }
                        }
                        Ok(None) => {}
                        Err(error) => {
                            exited[index] = true;
                            failure = Some(worker.terminate_and_reap(
                                io::ErrorKind::Other,
                                format!("shutdown status failed: {error}"),
                            ));
                        }
                    }
                }
            }
            if exited.iter().all(|done| *done) {
                // A final receipt may arrive between the earlier receive and
                // waitpid. Re-drain once after every sender has definitely exited.
                if final_drain {
                    break;
                }
                final_drain = true;
                continue;
            }
            if Instant::now() >= deadline {
                for (index, worker) in [
                    &mut self.speaker,
                    &mut self.capture,
                    &mut self.provider,
                    &mut self.privacy,
                ]
                .into_iter()
                .chain(self.ring.iter_mut())
                .enumerate()
                {
                    if !exited[index] {
                        let error = worker.terminate_and_reap(io::ErrorKind::TimedOut,
                            format!("did not stop within shared {} ms deadline; forced termination required", grace.as_millis()));
                        let _ = record(
                            trace,
                            json!({"kind":"worker_forced_shutdown", "worker":worker.role,
                            "at_us":monotonic_us(), "error":error.to_string()}),
                        );
                        failure = Some(error);
                    }
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        // No hardware is active during bounded metadata reads. Native kill/wait
        // has no userspace hard deadline; its result is never hidden as success.
        if !ring_blank_confirmed {
            failure = Some(io::Error::other(
                "ring shutdown has no verified black-write receipt",
            ));
        }
        if let Some(directory) = &self.diagnostic_directory {
            for (index, worker, leaf) in [
                (0, &self.speaker, "render-audio"),
                (1, &self.capture, "capture-audio"),
            ] {
                let certification = worker
                    .diagnostic_start
                    .ok_or_else(|| io::Error::other("missing diagnostic start identity"))
                    .and_then(|started| {
                        diagnostic_control::certify(
                            &directory.join(leaf),
                            started,
                            receipts[index].as_ref(),
                        )
                    });
                let _ = record(
                    trace,
                    json!({"kind":"audio_diagnostics_certified", "worker":worker.role,
                    "worker_boot":worker.boot, "at_us":monotonic_us(), "valid":certification.is_ok(),
                    "receipt":receipts[index], "error":certification.as_ref().err().map(ToString::to_string)}),
                );
                if let Err(error) = certification {
                    failure = Some(error);
                }
            }
        } else if receipts.iter().any(Option::is_some) {
            failure = Some(io::Error::other("unsolicited audio diagnostics receipt"));
        }
        failure.map_or(Ok(()), Err)
    }
}

#[derive(Clone, Copy)]
enum ProviderSource<'a> {
    Gemini(&'a Path),
    Fixture { wav: &'a Path, sha256: &'a str },
}
impl ProviderSource<'_> {
    fn arguments(self) -> io::Result<Vec<String>> {
        let text = |path: &Path| {
            path.to_str()
                .map(str::to_owned)
                .ok_or_else(|| io::Error::other("provider source path must be UTF-8"))
        };
        match self {
            Self::Gemini(config) => Ok(vec![text(config)?]),
            Self::Fixture { wav, sha256 } => {
                Ok(vec!["--fixture".into(), text(wav)?, sha256.into()])
            }
        }
    }
}

/// One real admitted input may receive one immutable cached reply. This finite
/// experiment uses normal privacy, ownership, speaker permits and AEC reference.
pub fn run_fixture(
    wav: &Path,
    sha256: &str,
    seconds: u64,
    output: &Path,
    options: FixtureOptions,
) -> Result<()> {
    let termination = Termination::install()?;
    if !options.directed.diagnostics {
        return Err(io::Error::other("fixture experiments require explicit --diagnostics").into());
    }
    let info = Fixture::load(wav, sha256)?.info;
    let cues = options
        .cue_socket
        .as_deref()
        .map(CueSink::connect)
        .transpose()?;
    run_source(
        ProviderSource::Fixture { wav, sha256 },
        seconds,
        output,
        options.directed,
        Some(info),
        cues,
        &termination,
    )
}

pub fn run_directed(config: &Path, seconds: u64, output: &Path) -> Result<()> {
    run_directed_with_diagnostics(config, seconds, output, false)
}

pub fn run_directed_with_diagnostics(
    config: &Path,
    seconds: u64,
    output: &Path,
    diagnostics: bool,
) -> Result<()> {
    run_directed_with_options(
        config,
        seconds,
        output,
        DirectedOptions {
            diagnostics,
            ..DirectedOptions::default()
        },
    )
}

pub fn run_directed_with_options(
    config: &Path,
    seconds: u64,
    output: &Path,
    options: DirectedOptions,
) -> Result<()> {
    run_directed_session(
        config,
        seconds,
        output,
        SessionOptions {
            directed: options,
            cue_socket: None,
        },
    )
}

/// Observe the same directed conversation without changing its admission,
/// ownership or audio policy. The optional sink never waits for its consumer.
pub fn run_directed_session(
    config: &Path,
    seconds: u64,
    output: &Path,
    options: SessionOptions,
) -> Result<()> {
    let termination = Termination::install()?;
    let cues = options
        .cue_socket
        .as_deref()
        .map(CueSink::connect)
        .transpose()?;
    run_source(
        ProviderSource::Gemini(config),
        seconds,
        output,
        options.directed,
        None,
        cues,
        &termination,
    )
}

fn run_source(
    source: ProviderSource<'_>,
    seconds: u64,
    output: &Path,
    options: DirectedOptions,
    fixture: Option<FixtureInfo>,
    mut cues: Option<CueSink>,
    termination: &Termination,
) -> Result<()> {
    if !(1..=MAX_DIRECTED_SECONDS).contains(&seconds) {
        return Err(io::Error::other("duration must be 1..600 seconds").into());
    }
    if let Some(ceiling) = options.ring_channel_ceiling {
        lamp_ring::ChannelCeiling::new(ceiling).map_err(io::Error::other)?;
    }
    termination.check()?;
    refuse_legacy_owners()?;
    fs::create_dir(output)?;
    fs::set_permissions(output, fs::Permissions::from_mode(0o700))?;
    let mut trace = Vec::with_capacity(MAX_TRACE_EVENTS);
    trace.push(json!({"kind":"run_start","at_us":monotonic_us(),"scope":if options.ring_channel_ceiling.is_some(){"directed_voice_with_ring"}else{"directed_voice_only"},"seconds":seconds,"acoustic_score":null,"audio_diagnostics_requested":options.diagnostics,
        "ring_channel_ceiling_requested":options.ring_channel_ceiling,
        "software_processing_requested":options.noise_suppression.software_processing(),
        "playout_start_horizon_us":options.playout_start.as_micros(),
        "turn_taking":options.turn_taking,"turn_taking_tail_us":if options.turn_taking == TurnTaking::V1Baseline { Some(crate::turn_taking::TAIL_US) } else { None },
        "provider_kind":if fixture.is_some(){"one_cached_reply"}else{"gemini"},
        "fixture":fixture,"cue_requested":cues.is_some()}));
    let result = (|| {
        let diagnostic_directory = options
            .diagnostics
            .then(|| fs::canonicalize(output))
            .transpose()?;
        run_inner(
            source,
            seconds,
            &mut trace,
            diagnostic_directory.as_deref(),
            options,
            &mut cues,
            termination,
        )
    })();
    finish_run(output, &mut trace, &mut cues, result, termination)
}

fn finish_run(
    output: &Path,
    trace: &mut Vec<Value>,
    cues: &mut Option<CueSink>,
    result: Result<()>,
    termination: &Termination,
) -> Result<()> {
    let result = result.and_then(|()| termination.check().map_err(Into::into));
    let end_index = trace.len();
    trace.push(json!({"kind":"run_end","at_us":monotonic_us(),"status":if result.is_ok(){"completed_unscored"}else{"failed"},"error":result.as_ref().err().map(ToString::to_string),"termination_signal":termination.signal_name()}));
    flush_cues(trace, cues);
    trace[end_index]["cue_valid"] = cues.as_ref().map(|sink| sink.fault().is_none()).into();
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(output.join("events.jsonl"))?;
    for event in trace.iter() {
        serde_json::to_writer(&mut file, &event)?;
        writeln!(file)?;
    }
    file.sync_all()?;
    result
}

fn flush_cues(trace: &mut Vec<Value>, cues: &mut Option<CueSink>) {
    if let Some(error) = cues
        .as_mut()
        .and_then(|sink| sink.scan(trace, monotonic_us()))
    {
        // Scheduling failure cannot change voice ownership or block privacy.
        let _ = record(
            trace,
            json!({"kind":"fixture_cue_invalid","at_us":monotonic_us(),"reason":error}),
        );
    }
}

fn refuse_legacy_owners() -> io::Result<()> {
    // This is a coexistence guard for the test board, not a legacy runtime
    // dependency. An absent unit is fine; a running writer forbids this run.
    for unit in [
        "hal.service",
        "os-server.service",
        "led-boot.service",
        "led-shutdown.service",
    ] {
        let status = Command::new("/usr/bin/systemctl")
            .args(["is-active", "--quiet", unit])
            .status()?;
        if status.success() {
            return Err(io::Error::other(format!(
                "{unit} still owns the test device; explicit exclusive cutover required"
            )));
        }
        if !matches!(status.code(), Some(3) | Some(4)) {
            return Err(io::Error::other("cannot verify legacy service ownership"));
        }
    }
    Ok(())
}

fn now() -> MonoTime {
    MonoTime::from_micros(monotonic_us())
}
fn record(trace: &mut Vec<Value>, event: Value) -> Result<()> {
    if trace.len() >= MAX_TRACE_EVENTS - 1 {
        return Err(io::Error::other("trace capacity reached").into());
    }
    trace.push(event);
    Ok(())
}

fn link_trace(link: ProviderLink, received_at_us: u64) -> Value {
    let kind = match link.state {
        ProviderLinkState::Ready { .. } => "provider_ready",
        ProviderLinkState::Unavailable { .. } => "provider_unavailable",
        ProviderLinkState::Terminal { .. } => "provider_terminal",
    };
    let mut event = json!({"kind":kind,"at_us":link.observed_at_us,
        "at_us_upper":link.source_clock.map_or(link.observed_at_us, |bounds| bounds.observed_upper_us),
        "at_us_role":if link.source_clock.is_some(){"source_interval_lower"}else{"host_native"},
        "coordinator_observed_at_us":received_at_us,"link":link});
    if let ProviderLinkState::Ready { setup_us, .. } = link.state {
        event["setup_us"] = json!(setup_us);
    }
    event
}

/// One shared finite control budget per coordinator tick. Returning false
/// defers cloud data/submission, but not capture, privacy or owned playback.
fn service_provider_control(
    channel: &mut Channel,
    cloud: &mut CloudAvailability,
    remaining: &mut usize,
    trace: &mut Vec<Value>,
) -> Result<bool> {
    while *remaining > 0 {
        let Some(event) = channel.receive::<WorkerEvent>()? else {
            return Ok(true);
        };
        *remaining -= 1;
        match event {
            WorkerEvent::ProviderLink { link } => {
                let at = monotonic_us();
                cloud.observe(link, at)?;
                record(trace, link_trace(link, at))?;
            }
            WorkerEvent::Fault { code } => return Err(io::Error::other(code).into()),
            _ => return Err(io::Error::other("unexpected provider control event").into()),
        }
    }
    Ok(false)
}

/// The same priority path is used at tick start and immediately before a
/// candidate can replace the current turn. A full budget defers admission.
trait SpeakerControlIo {
    fn next_speaker_event(&mut self) -> io::Result<Option<WorkerEvent>>;
    fn publish_speaker_snapshot(&mut self, snapshot: Snapshot) -> io::Result<()>;
}
impl SpeakerControlIo for Rig {
    fn next_speaker_event(&mut self) -> io::Result<Option<WorkerEvent>> {
        self.speaker.channels.control.receive()
    }
    fn publish_speaker_snapshot(&mut self, snapshot: Snapshot) -> io::Result<()> {
        self.publish(snapshot)
    }
}
#[allow(clippy::too_many_arguments)]
fn service_speaker_control(
    io: &mut impl SpeakerControlIo,
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    speaker_pending: &mut Option<PendingSpeaker>,
    playback_mask: &mut PlaybackMask,
    snapshot: &mut Snapshot,
    last_publish: &mut u64,
    remaining: &mut usize,
    trace: &mut Vec<Value>,
) -> Result<bool> {
    while *remaining > 0 {
        let Some(event) = io.next_speaker_event()? else {
            return Ok(true);
        };
        *remaining -= 1;
        match event {
            WorkerEvent::Playback {
                owner: written_owner,
                first_sample,
                end_sample,
                queue_observed_at_us,
                turn,
                sequence,
                queued_frames: _,
                accepted_frames,
                observed_at_us,
            } => {
                if written_owner.turn() != turn {
                    return Err(io::Error::other("speaker owner/turn mismatch").into());
                }
                playback_mask.accepted(
                    written_owner,
                    sequence,
                    first_sample,
                    end_sample,
                    accepted_frames,
                    observed_at_us,
                    queue_observed_at_us,
                    monotonic_us(),
                )?;
                if let Some(current) = reply.as_mut().filter(|r| r.owner.turn() == turn)
                    && accepted_frames > 0
                {
                    let Some(pending) = speaker_pending.as_mut().filter(|p| p.sequence == sequence)
                    else {
                        return Err(
                            io::Error::other("speaker acceptance has no matching chunk").into()
                        );
                    };
                    pending.accepted += accepted_frames;
                    if pending.accepted > 240 {
                        return Err(io::Error::other("speaker accepted too many frames").into());
                    }
                    if current.playback.is_none() {
                        let token = owner.playback_started(now(), pending.permit)?;
                        current.playback = Some(token);
                        *snapshot = owner.snapshot(now())?;
                        io.publish_speaker_snapshot(*snapshot)?;
                        *last_publish = monotonic_us();
                        record(
                            trace,
                            json!({"kind":"speaker_first_write","turn":turn,"owner":current.owner,"playback_sequence":token.sequence(),"at_us":observed_at_us,"boundary":"ALSA accepted; acoustic onset unmeasured",
                                "start_gate":current.start_trace()}),
                        )?;
                    }
                    if pending.accepted == 240 {
                        if pending.final_chunk {
                            current.final_chunk = Some(sequence);
                        }
                        *speaker_pending = None;
                    }
                }
            }
            WorkerEvent::PlaybackClockReset {
                previous,
                next_epoch,
                observed_at_us,
            } => {
                playback_mask.clock_reset(previous, next_epoch, observed_at_us, monotonic_us())?;
                record(
                    trace,
                    json!({"kind":"playback_clock_reset","previous":previous,"next_epoch":next_epoch,"at_us":observed_at_us,"boundary":"successful physical discard/reset; not speech retirement"}),
                )?;
            }
            WorkerEvent::SpeechDrained {
                owner: drained_owner,
                sequence,
                last_sample,
                observed_at_us,
            } => {
                let applied = playback_mask.drained(
                    drained_owner,
                    sequence,
                    last_sample,
                    observed_at_us,
                    monotonic_us(),
                )?;
                record(
                    trace,
                    json!({"kind":"speech_queue_drained","owner":drained_owner,"sequence":sequence,"last_sample":last_sample,"at_us":observed_at_us,"mask_updated":applied,"boundary":"ALSA speech cursor retirement; not turn completion or acoustic silence"}),
                )?;
            }
            WorkerEvent::SpeechRetired {
                owner: delivered,
                sequence,
                final_sample,
                observed_at_us,
            } => {
                if let Some(current) = reply.as_mut().filter(|reply| reply.owner == delivered) {
                    let token = current.retire_playback(sequence, final_sample.frame)?;
                    owner.playback_ended(now(), token)?;
                    *snapshot = owner.snapshot(now())?;
                    io.publish_speaker_snapshot(*snapshot)?;
                    *last_publish = monotonic_us();
                    record(
                        trace,
                        json!({"kind":"speech_final_sample_retired","turn":delivered.turn(),"owner":delivered,
                            "playback_sequence":token.sequence(),"at_us":observed_at_us,"final_sample":final_sample,"boundary":"ALSA delay retirement; idle zeros may remain queued; acoustic end unmeasured"}),
                    )?;
                }
            }
            event @ WorkerEvent::CancelledTailRetired {
                owner: old,
                end_sample,
                observed_at_us,
                ..
            } => {
                playback_mask.cancelled_tail(old, end_sample, observed_at_us, monotonic_us())?;
                handle_cancelled_tail_retirement(reply, trace, event, monotonic_us())?;
            }
            event @ WorkerEvent::PlaybackGap { .. } => {
                handle_playback_gap(reply, speaker_pending.as_ref(), trace, event)?;
            }
            WorkerEvent::PlaybackRejected {
                turn,
                sequence,
                reason,
            } => {
                playback_mask.rejected(sequence, monotonic_us())?;
                record(
                    trace,
                    json!({"kind":"speaker_rejected","turn":turn,"sequence":sequence,"reason":reason,"at_us":monotonic_us()}),
                )?;
                if reply.as_ref().is_some_and(|r| r.owner.turn() == turn) {
                    return Err(io::Error::other("current reply rejected at speaker").into());
                }
            }
            event @ WorkerEvent::PlaybackDiscarded {
                owner: old,
                observed_at_us,
                ..
            } => {
                playback_mask.discarded(old, observed_at_us, monotonic_us())?;
                handle_playback_discard(owner, reply, trace, event)?;
            }
            WorkerEvent::ReferenceFault { fault } => {
                record(
                    trace,
                    json!({"kind":"reference_fault","worker":"speaker","details":fault}),
                )?;
                return Err(fault.into());
            }
            WorkerEvent::Fault { code } => return Err(io::Error::other(code).into()),
            _ => return Err(io::Error::other("unexpected speaker event").into()),
        }
    }
    Ok(false)
}

fn accepted_decision(decision: Decision, trace: &mut Vec<Value>) -> Result<Option<Accepted>> {
    match decision {
        Decision::Accepted(accepted) => Ok(Some(accepted)),
        Decision::Rejected(rejection) => {
            record_input_rejection(trace, rejection)?;
            Ok(None)
        }
        Decision::Ignored => {
            Err(io::Error::other("directed candidate decision lost its identity").into())
        }
    }
}

/// Only a successful local admission decision may interrupt an older answer.
/// Cloud unavailability prevents submission, never that local cancellation.
/// No provider session/request is invented for this never-submitted input.
fn accepted_without_cloud(
    accepted: Accepted,
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    admission: &mut InputAdmission,
    trace: &mut Vec<Value>,
) -> Result<(Snapshot, bool)> {
    let displaced = reply.as_ref().map(|current| current.owner);
    cancel_reply(owner, reply, trace, "user_interrupted")?;
    admission.discard_stream_until_end(monotonic_us())?;
    let state = owner.snapshot(now())?;
    record(
        trace,
        json!({"kind":"input_not_submitted","at_us":monotonic_us(),
        "candidate":accepted.candidate.id,"admission_basis":accepted.basis,
        "interrupted_owner":displaced,"reason":"provider_unavailable_or_pending_control",
        "provider_request":null,"provider_session":null,
        "outcome":RequestOutcome::Failed { stage:crate::provider_worker::FailedStage::NotDelivered,
            reason:crate::provider_worker::Fault::Local, local_not_submitted:true },
        "prefix_first_host_read_us":accepted.audio.first().map(|frame| frame.captured_at_us)}),
    )?;
    Ok((state, !accepted.ended))
}

fn discard_request_input(
    current: &mut Reply,
    queue: &mut VecDeque<ProviderInput>,
    admission: &mut InputAdmission,
    draining: &mut bool,
    trace: &mut Vec<Value>,
    reason: &'static str,
) -> Result<()> {
    let discarded_commands = queue.len();
    queue.clear();
    current.cloud.input_abandoned = true;
    if !current.local_input_ended {
        admission.discard_stream_until_end(monotonic_us())?;
        *draining = true;
    }
    record(
        trace,
        json!({"kind":"provider_input_abandoned","at_us":monotonic_us(),
        "turn":current.owner.turn(),"provider_session":current.cloud.session,"reason":reason,
        "start_submitted":current.cloud.start_submitted,"input_end_submitted":current.cloud.input_end_submitted,
        "discarded_commands":discarded_commands}),
    )?;
    Ok(())
}

fn apply_submission_gate(
    cloud: &CloudAvailability,
    reply: &mut Option<Reply>,
    queue: &mut VecDeque<ProviderInput>,
    admission: &mut InputAdmission,
    draining: &mut bool,
    trace: &mut Vec<Value>,
) -> Result<()> {
    if let Some(current) = reply.as_mut()
        && current.cloud.abandon_if_changed(cloud)
    {
        discard_request_input(
            current,
            queue,
            admission,
            draining,
            trace,
            "provider_link_changed",
        )?;
        if current.cloud.outcome.is_some() {
            current.provider_idle = true;
            current.generation_done = true;
            record(
                trace,
                json!({"kind":"provider_turn_failed","at_us":monotonic_us(),
                "turn":current.owner.turn(),"outcome":current.cloud.outcome}),
            )?;
        }
    }
    Ok(())
}
/// Validate ordered model evidence before it can mutate playback. Availability
/// is solely the priority register: a stale credited Ready has no effect here.
fn observe_provider_output(
    cloud: &mut CloudAvailability,
    reply: &mut Option<Reply>,
    event: &ProviderOutput,
) -> io::Result<()> {
    if cloud.output_ended()
        && !matches!(
            event,
            ProviderOutput::TurnFailed { .. }
                | ProviderOutput::OwnershipAssumed { .. }
                | ProviderOutput::Ready { .. }
        )
    {
        return Err(io::Error::other("model output followed ordered Ended"));
    }
    // Keep match borrowing explicit: only this request's source receipt may
    // change its delivery evidence; cancelled old owners are already terminal.
    match event {
        ProviderOutput::Ready { .. } | ProviderOutput::Transcript { .. } => {}
        ProviderOutput::Ended { .. } => cloud.end_output()?,
        ProviderOutput::OwnershipAssumed { .. } => {
            return Err(io::Error::other(
                "provider assumed ownership under Require policy",
            ));
        }
        ProviderOutput::Started {
            request, session, ..
        } => {
            if let Some(current) = reply.as_mut().filter(|r| r.owner.turn() == *request) {
                current.cloud.confirm_start(*session)?;
            }
        }
        ProviderOutput::Audio { request, .. } | ProviderOutput::Interrupted { request } => {
            if let Some(current) = reply.as_mut().filter(|r| r.owner.turn() == *request) {
                current.cloud.require_output()?;
            }
        }
        ProviderOutput::GenerationComplete { request } => {
            if let Some(current) = reply.as_mut().filter(|r| r.owner.turn() == *request) {
                current.cloud.require_output()?;
                current.generation_done = true;
            }
        }
        ProviderOutput::TurnComplete { request, idle } => {
            if let Some(current) = reply.as_mut().filter(|r| r.owner.turn() == *request) {
                current.cloud.complete(*idle)?;
                current.provider_idle = *idle;
                if *idle {
                    current.generation_done = true;
                }
            }
        }
        ProviderOutput::TurnFailed {
            request,
            stage,
            reason,
        } => {
            if let Some(current) = reply.as_mut().filter(|r| r.owner.turn() == *request) {
                current.cloud.failed(*stage, *reason)?;
                // Prior PCM retains its owner but never becomes a successful
                // answer. Flush even a partial held final block, then fail.
                current.generation_done = true;
                current.provider_idle = true;
            }
        }
    }
    Ok(())
}

/// Sending is bounded independently of answer delivery. No queued Audio/End
/// can be moved to a replacement session, and WouldBlock is not delivery.
fn flush_provider_input(
    cloud: &CloudAvailability,
    reply: &mut Option<Reply>,
    queue: &mut VecDeque<ProviderInput>,
    channel: &mut Channel,
) -> io::Result<()> {
    flush_provider_input_with(cloud, reply, queue, |command| channel.send(command))
}

fn flush_provider_input_with(
    cloud: &CloudAvailability,
    reply: &mut Option<Reply>,
    queue: &mut VecDeque<ProviderInput>,
    mut send: impl FnMut(&ProviderInput) -> io::Result<()>,
) -> io::Result<()> {
    for _ in 0..8 {
        let Some(command) = queue.front() else {
            break;
        };
        let current = reply
            .as_mut()
            .ok_or_else(|| io::Error::other("provider input lost its owner"))?;
        if !current.cloud.can_submit(cloud) {
            break;
        }
        let request = match command {
            ProviderInput::Start { request, .. }
            | ProviderInput::Audio { request, .. }
            | ProviderInput::End { request, .. } => *request,
        };
        if request != current.owner.turn() {
            return Err(io::Error::other(
                "queued provider input has stale ownership",
            ));
        }
        current.cloud.validate_submission(command)?;
        match send(command) {
            Ok(()) => {
                current.cloud.submitted(command)?;
                queue.pop_front();
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[derive(Default)]
struct StartupEvents {
    cloud: CloudAvailability,
    output_flow: OutputReceiver,
    events: Vec<Value>,
}
impl StartupEvents {
    fn push(&mut self, event: Value) -> io::Result<()> {
        if self.events.len() >= 16 {
            return Err(io::Error::other("startup event capacity exceeded"));
        }
        self.events.push(event);
        Ok(())
    }
    fn poll(&mut self, channels: &mut WorkerChannels, role: &str) -> io::Result<()> {
        for _ in 0..16 {
            let Some(event) = channels.control.receive::<WorkerEvent>()? else {
                break;
            };
            match (&event, role) {
                (WorkerEvent::ProviderLink { link }, "provider") => {
                    let at = monotonic_us();
                    self.cloud.observe(*link, at)?;
                    self.push(link_trace(*link, at))?;
                }
                (WorkerEvent::CaptureDspReset { .. }, "capture") => self.push(
                    json!({"kind":"echo_reference_reset","at_us":monotonic_us(),"details":event}),
                )?,
                (
                    WorkerEvent::Ring {
                        report: RingFeedback::Blanked { .. },
                    },
                    "ring",
                ) => self.push(
                    json!({"kind":"ring_startup_event","at_us":monotonic_us(),"details":event}),
                )?,
                (WorkerEvent::Fault { code }, _) => return Err(io::Error::other(code.clone())),
                _ => {
                    return Err(io::Error::other(
                        "unexpected worker event before activation",
                    ));
                }
            }
        }
        for _ in 0..16 {
            match role {
                "provider" => match channels.data.receive::<ProviderOutput>()? {
                    // A provider can recover while later children are still
                    // starting. Count its repeated Ready against the original
                    // capacity sequence; it does not admit input by itself.
                    Some(ProviderOutput::Ready { setup_us }) => {
                        self.output_flow.received()?;
                        self.push(json!({"kind":"provider_ready_data_ignored","at_us":monotonic_us(),"setup_us":setup_us}))?;
                    }
                    Some(ProviderOutput::Ended {
                        reason,
                        fault,
                        attempts,
                    }) => {
                        self.output_flow.received()?;
                        self.cloud.end_output()?;
                        self.push(json!({"kind":"provider_output_ended","at_us":monotonic_us(),"reason":reason,"fault":fault,"attempts":attempts}))?;
                    }
                    Some(_) => {
                        return Err(io::Error::other("unexpected provider output before input"));
                    }
                    None => break,
                },
                "speaker" | "ring" => break,
                "capture" => {
                    if channels.data.receive::<MicrophoneFrame>()?.is_some() {
                        return Err(io::Error::other("microphone opened before activation"));
                    }
                    break;
                }
                _ => return Err(io::Error::other("unexpected startup worker role")),
            }
        }
        if role == "provider" {
            self.output_flow.replenish(0, &mut channels.control)?;
        }
        Ok(())
    }
}

fn pulse(
    owner: &mut Controller,
    workers: &mut [&mut Worker],
    last: &mut u64,
    startup: &mut StartupEvents,
    termination: &Termination,
) -> io::Result<()> {
    termination.check()?;
    // Consume operational notifications while later children start. A startup
    // wait must not age valid packets in the 100 ms runtime channels.
    for worker in workers.iter_mut() {
        startup.poll(&mut worker.channels, &worker.role)?;
    }
    let time = monotonic_us();
    if time.saturating_sub(*last) >= 20_000 {
        let snapshot = owner
            .snapshot(MonoTime::from_micros(time))
            .map_err(io::Error::other)?;
        for worker in workers {
            worker
                .channels
                .control
                .send(Control::Authority { snapshot })?;
            worker.check_running()?;
        }
        *last = time;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn spawn_rig(
    source: ProviderSource<'_>,
    directory: &Path,
    boot: BootId,
    owner: &mut Controller,
    startup: &mut StartupEvents,
    diagnostic_directory: Option<&Path>,
    options: DirectedOptions,
    termination: &Termination,
) -> Result<Rig> {
    termination.check()?;
    let executable = std::env::current_exe()?;
    let arguments = source.arguments()?;
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let mut provider =
        Worker::spawn_with_tick(&executable, directory, "provider", boot, &arguments, || {
            termination.check()
        })?;
    let mut last = 0;
    let capture_arguments = AudioWorkerOptions {
        diagnostic_directory: diagnostic_directory.map(|p| p.join("capture-audio")),
        noise_suppression: options.noise_suppression,
    }
    .child_arguments("plug:device_micro2", true)?;
    let speaker_arguments = AudioWorkerOptions {
        diagnostic_directory: diagnostic_directory.map(|p| p.join("render-audio")),
        ..AudioWorkerOptions::default()
    }
    .child_arguments("plug:device_speaker", false)?;
    let capture_arguments: Vec<&str> = capture_arguments.iter().map(String::as_str).collect();
    let speaker_arguments: Vec<&str> = speaker_arguments.iter().map(String::as_str).collect();
    let mut capture = Worker::spawn_with_tick(
        &executable,
        directory,
        "capture",
        boot,
        &capture_arguments,
        || pulse(owner, &mut [&mut provider], &mut last, startup, termination),
    )?;
    let mut speaker = Worker::spawn_with_tick(
        &executable,
        directory,
        "speaker",
        boot,
        &speaker_arguments,
        || {
            pulse(
                owner,
                &mut [&mut provider, &mut capture],
                &mut last,
                startup,
                termination,
            )
        },
    )?;
    let mut ring = if options.ring_channel_ceiling.is_some() {
        Some(Worker::spawn_with_tick(
            &executable,
            directory,
            "ring",
            boot,
            &[],
            || {
                pulse(
                    owner,
                    &mut [&mut provider, &mut capture, &mut speaker],
                    &mut last,
                    startup,
                    termination,
                )
            },
        )?)
    } else {
        None
    };
    let privacy = Worker::spawn_with_tick(&executable, directory, "privacy", boot, &[], || {
        if let Some(ring) = ring.as_mut() {
            pulse(
                owner,
                &mut [&mut provider, &mut capture, &mut speaker, ring],
                &mut last,
                startup,
                termination,
            )
        } else {
            pulse(
                owner,
                &mut [&mut provider, &mut capture, &mut speaker],
                &mut last,
                startup,
                termination,
            )
        }
    })?;
    for (worker, expected_stream) in [
        (&capture, DiagnosticStream::Capture),
        (&speaker, DiagnosticStream::Render),
    ] {
        match (diagnostic_directory, worker.diagnostic_start) {
            (Some(_), Some(started))
                if started.boot == boot
                    && started.stream == expected_stream
                    && started.started_at_us > 0
                    && started.started_at_us <= monotonic_us() =>
            {
                startup.push(json!({"kind":"audio_diagnostics_started", "worker":worker.role, "worker_boot":worker.boot, "details":started}))?;
            }
            (None, None) => {}
            _ => return Err(io::Error::other("audio diagnostic startup identity mismatch").into()),
        }
    }
    Ok(Rig {
        provider,
        capture,
        speaker,
        privacy,
        ring,
        ring_channel_ceiling: options.ring_channel_ceiling,
        diagnostic_directory: diagnostic_directory.map(Path::to_path_buf),
        noise_suppression: options.noise_suppression,
        turn_taking: options.turn_taking,
        playout_start: options.playout_start,
    })
}

fn run_inner(
    source: ProviderSource<'_>,
    seconds: u64,
    trace: &mut Vec<Value>,
    diagnostic_directory: Option<&Path>,
    options: DirectedOptions,
    cues: &mut Option<CueSink>,
    termination: &Termination,
) -> Result<()> {
    termination.check()?;
    let directory = SessionDirectory::create()?;
    let boot = new_boot()?;
    if let Some(sink) = cues.as_mut() {
        sink.set_boot(boot);
    }
    record(
        trace,
        json!({"kind":"session_boot","at_us":monotonic_us(),"boot":boot}),
    )?;
    let mut owner = Controller::new(boot, now());
    let mut startup = StartupEvents::default();
    let mut rig = spawn_rig(
        source,
        &directory.path,
        boot,
        &mut owner,
        &mut startup,
        diagnostic_directory,
        options,
        termination,
    )?;
    let mut reply = None;
    let result = (|| {
        termination.check()?;
        // The trusted supervisor supplies both immutable peer boots. No process
        // learns reference identity from incoming PCM or an unsolicited handshake.
        rig.capture
            .channels
            .control
            .send(Control::ConnectReference {
                peer: rig.speaker.boot,
            })?;
        rig.speaker
            .channels
            .control
            .send(Control::ConnectReference {
                peer: rig.capture.boot,
            })?;
        conversation(
            &mut rig,
            &mut owner,
            seconds,
            trace,
            startup,
            &mut reply,
            cues,
            termination,
        )
    })();
    finish_active_run(&mut rig, &mut owner, &mut reply, trace, cues, result)
}

fn finish_active_run(
    rig: &mut Rig,
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    trace: &mut Vec<Value>,
    cues: &mut Option<CueSink>,
    result: Result<()>,
) -> Result<()> {
    if result.is_err() {
        // Keep an explicit outcome even when the failing child cannot deliver
        // speech. Privacy revocation and stop below still run if tracing fails.
        let _ = cancel_reply(owner, reply, trace, "runtime_failed");
    }
    let _ = owner.set_microphone_permission(now(), Permission::Denied);
    if let Ok(snapshot) = owner.snapshot(now()) {
        let _ = rig.publish(snapshot);
    }
    flush_cues(trace, cues);
    let shutdown = rig.shutdown(trace);
    result.and_then(|()| shutdown.map_err(Into::into))
}

#[allow(clippy::too_many_arguments)]
fn conversation(
    rig: &mut Rig,
    owner: &mut Controller,
    seconds: u64,
    trace: &mut Vec<Value>,
    startup: StartupEvents,
    reply: &mut Option<Reply>,
    cues: &mut Option<CueSink>,
    termination: &Termination,
) -> Result<()> {
    let mut cloud = startup.cloud;
    let mut output_flow = startup.output_flow;
    for event in startup.events {
        record(trace, event)?;
    }
    let startup_deadline = Instant::now() + INPUT_READINESS_TIMEOUT;
    let mut deadline = None;
    let mut gate = PrivacyGate::default();
    let mut privacy_sequence = 0;
    let mut permitted = false;
    let mut capture_requested = false;
    let mut capture_epoch = None;
    let mut last_capture = None;
    let mut last_publish = 0;
    let mut last_health = 0;
    let mut choreography = rig
        .ring_channel_ceiling
        .map(RingChoreographer::new)
        .transpose()?;
    let mut ring_trace = RingTrace::default();
    let mut detector = match rig.turn_taking {
        TurnTaking::V1Baseline => TurnDetector::v1_baseline(),
        TurnTaking::VadOnly => TurnDetector::default(),
    };
    record(
        trace,
        json!({"kind":"endpoint_policy","at_us":monotonic_us(),
        "turn_taking":rig.turn_taking,"silence_wait_ms":detector.endpoint_silence_ms(),
        "basis":"consecutive_low_vad_scores","semantic_confirmation":false}),
    )?;
    let mut admission = InputAdmission::default();
    let mut playback_mask = PlaybackMask::new(rig.turn_taking);
    let mut policy_available = None;
    let mut capture_lineage: Option<CaptureLineage> = None;
    let mut provider_queue = VecDeque::with_capacity(64);
    let mut submission_draining = false;
    let mut speaker_pending: Option<PendingSpeaker> = None;
    let mut speaker_sequence = 0u64;
    let mut snapshot = owner.snapshot(now())?;
    rig.publish(snapshot)?;
    loop {
        let loop_start = Instant::now();
        termination.check()?;
        let clock = monotonic_us();
        if deadline.is_none() && Instant::now() >= startup_deadline {
            return Err(io::Error::other("voice input never became ready").into());
        }
        // Privacy always precedes input, model output, or actuator work.
        for _ in 0..16 {
            let Some(event) = rig.privacy.channels.control.receive::<WorkerEvent>()? else {
                break;
            };
            match event {
                WorkerEvent::Privacy {
                    acquired_at_us,
                    muted,
                } => {
                    privacy_sequence += 1;
                    gate.observe(
                        privacy_sequence,
                        MonoTime::from_micros(acquired_at_us),
                        now(),
                        muted,
                    );
                }
                WorkerEvent::Fault { code } => return Err(io::Error::other(code).into()),
                _ => return Err(io::Error::other("unexpected privacy event").into()),
            }
        }
        // Provider loss closes cloud submissions, never the microphone authority
        // needed to retain interruptions or finish already-owned playback.
        let can_open = gate.allowed(now());
        if permitted && !can_open {
            return Err(io::Error::other("physical privacy closed or lost freshness").into());
        }
        if can_open && !permitted {
            owner.set_microphone_permission(now(), Permission::Allowed)?;
            permitted = true;
            snapshot = owner.snapshot(now())?;
            rig.publish(snapshot)?;
        }
        let mut provider_control_budget = 16;
        let mut provider_control_empty = service_provider_control(
            &mut rig.provider.channels.control,
            &mut cloud,
            &mut provider_control_budget,
            trace,
        )?;
        apply_submission_gate(
            &cloud,
            reply,
            &mut provider_queue,
            &mut admission,
            &mut submission_draining,
            trace,
        )?;
        drain_capture_control(
            &mut rig.capture.channels.control,
            &mut capture_epoch,
            rig.noise_suppression,
            trace,
        )?;
        let mut speaker_control_budget = 32;
        service_speaker_control(
            rig,
            owner,
            reply,
            &mut speaker_pending,
            &mut playback_mask,
            &mut snapshot,
            &mut last_publish,
            &mut speaker_control_budget,
            trace,
        )?;
        // Return through input/cancellation and speaker dispatch after at most
        // two cloud packets, even when more PCM is immediately available.
        // Render reference still goes directly between the audio workers.
        let mut provider_slice = ProviderSlice::new(monotonic_us());
        while provider_control_empty {
            provider_control_empty = service_provider_control(
                &mut rig.provider.channels.control,
                &mut cloud,
                &mut provider_control_budget,
                trace,
            )?;
            apply_submission_gate(
                &cloud,
                reply,
                &mut provider_queue,
                &mut admission,
                &mut submission_draining,
                trace,
            )?;
            if !provider_control_empty {
                break;
            }
            let Some(event) =
                provider_slice.receive(&mut rig.provider.channels.data, monotonic_us())?
            else {
                break;
            };
            output_flow.received()?;
            observe_provider_output(&mut cloud, reply, &event)?;
            match event {
                ProviderOutput::Ready { setup_us } => {
                    record(
                        trace,
                        json!({"kind":"provider_ready_data_ignored","at_us":monotonic_us(),"setup_us":setup_us}),
                    )?;
                }
                ProviderOutput::Audio {
                    request,
                    sequence,
                    provider_event_at_us,
                    source_frames,
                    source_offset,
                    samples,
                } => {
                    let received_at_us = monotonic_us();
                    if samples.is_empty()
                        || samples.len() > PACKET_SAMPLES
                        || source_frames == 0
                        || source_frames > lamp_gemini::MAX_OUTPUT_SAMPLES
                        || source_offset
                            .checked_add(samples.len())
                            .is_none_or(|end| end > source_frames)
                        || provider_event_at_us == 0
                        || provider_event_at_us > received_at_us
                    {
                        return Err(io::Error::other("invalid provider audio frame").into());
                    }
                    if let Some(current) = reply.as_mut().filter(|r| r.owner.turn() == request) {
                        if current.pcm.len() + samples.len() > MAX_REPLY_SAMPLES {
                            return Err(io::Error::other(
                                "provider violated reserved reply capacity",
                            )
                            .into());
                        }
                        if !current.had_audio {
                            record(
                                trace,
                                json!({"kind":"provider_first_audio","turn":request,"at_us":monotonic_us()}),
                            )?;
                            current.had_audio = true;
                        }
                        let received_frames = samples.len();
                        current.append_provider_audio(samples, received_at_us);
                        record(
                            trace,
                            json!({"kind":"provider_audio_enqueued","turn":request,
                            "at_us":received_at_us,"sequence":sequence,"provider_event_at_us":provider_event_at_us,
                            "provider_event_to_enqueue_us":received_at_us-provider_event_at_us,
                            "source_frames":source_frames,"source_offset":source_offset,"frames":received_frames,
                            "reply_queued_frames":current.pcm.len(),"held_final_frames":current.held_final_samples(),
                            "speaker_pending_sequence":speaker_pending.as_ref().map(|pending| pending.sequence)}),
                        )?;
                    }
                }
                ProviderOutput::GenerationComplete { .. } => {}
                ProviderOutput::TurnComplete { request, idle } => {
                    record(
                        trace,
                        json!({"kind":"provider_turn_complete","turn":request,"idle":idle,"at_us":monotonic_us()}),
                    )?;
                }
                ProviderOutput::Transcript {
                    request,
                    text,
                    finished,
                } => record(
                    trace,
                    json!({"kind":"transcript","turn":request,"text":text,"finished":finished,"at_us":monotonic_us(),"input_correlation":if request.is_none(){"unreliable; session scoped"}else{"output lineage"}}),
                )?,
                ProviderOutput::Interrupted { request } => {
                    record(
                        trace,
                        json!({"kind":"provider_interrupted","turn":request,"at_us":monotonic_us()}),
                    )?;
                    if reply
                        .as_ref()
                        .is_some_and(|current| current.owner.turn() == request)
                    {
                        let input_active = reply
                            .as_ref()
                            .is_some_and(|current| !current.local_input_ended);
                        cancel_reply(owner, reply, trace, "provider_interrupted")?;
                        speaker_pending = None;
                        provider_queue.clear();
                        // Keep the detector's utterance boundary. Reconnecting or
                        // cancellation must never reinterpret its tail as a new input.
                        if input_active {
                            admission.discard_stream_until_end(monotonic_us())?;
                            submission_draining = true;
                        }
                        snapshot = owner.snapshot(now())?;
                        rig.publish(snapshot)?;
                        last_publish = monotonic_us();
                    }
                }
                ProviderOutput::Started {
                    request,
                    session,
                    waiting_for_barrier,
                } => {
                    record(
                        trace,
                        json!({"kind":"provider_input_started","turn":request,
                        "provider_session":session,"waiting_for_barrier":waiting_for_barrier,"at_us":monotonic_us()}),
                    )?;
                }
                ProviderOutput::TurnFailed {
                    request,
                    stage,
                    reason,
                } => {
                    if let Some(current) = reply.as_mut().filter(|r| r.owner.turn() == request) {
                        discard_request_input(
                            current,
                            &mut provider_queue,
                            &mut admission,
                            &mut submission_draining,
                            trace,
                            "provider_turn_failed",
                        )?;
                    }
                    record(
                        trace,
                        json!({"kind":"provider_turn_failed","turn":request,
                        "stage":stage,"reason":reason,"at_us":monotonic_us()}),
                    )?;
                }
                ProviderOutput::OwnershipAssumed { .. } => {
                    unreachable!("rejected by output validation")
                }
                ProviderOutput::Ended {
                    reason,
                    fault,
                    attempts,
                } => {
                    record(
                        trace,
                        json!({"kind":"provider_output_ended","at_us":monotonic_us(),
                        "reason":reason,"fault":fault,"attempts":attempts}),
                    )?;
                    apply_submission_gate(
                        &cloud,
                        reply,
                        &mut provider_queue,
                        &mut admission,
                        &mut submission_draining,
                        trace,
                    )?;
                }
            }
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            // Drain fault channels and inspect children before accepting the
            // deadline. A crashed or forcibly killed worker is never success.
            rig.running()?;
            record(
                trace,
                json!({"kind":"turn_taking_summary","at_us":monotonic_us(),"policy":rig.turn_taking,"suppressed_blocks":playback_mask.suppressed_blocks(),"tail_us":crate::turn_taking::TAIL_US,"acoustic_tail_measured":false}),
            )?;
            cancel_reply(owner, reply, trace, "deadline_cancelled")?;
            snapshot = owner.snapshot(now())?;
            rig.publish(snapshot)?;
            return Ok(());
        }
        if let Some(capture) = capture_lineage
            && let Some(rejection) = admission.maintain(
                AdmissionContext {
                    capture,
                    authority: owner.snapshot(now())?,
                },
                monotonic_us(),
            )?
        {
            record_input_rejection(trace, rejection)?;
        }
        if permitted && !capture_requested {
            rig.capture.channels.control.send(Control::StartCapture)?;
            capture_requested = true;
        }
        for _ in 0..8 {
            let Some(frame) = rig.capture.channels.data.receive::<MicrophoneFrame>()? else {
                break;
            };
            validate_capture_frame(
                &mut rig.capture.channels.control,
                &mut capture_epoch,
                snapshot.microphone_generation(),
                &frame,
                rig.noise_suppression,
                trace,
            )?;
            let at = monotonic_us();
            if at
                .checked_sub(frame.read_completed_at_us)
                .is_none_or(|age| age >= INPUT_LEASE_US)
            {
                return Err(io::Error::other("capture became stale").into());
            }
            last_capture = Some(frame.read_completed_at_us);
            owner.set_capture(
                now(),
                CaptureState::RetainingUntil(MonoTime::from_micros(
                    frame.read_completed_at_us + INPUT_LEASE_US,
                )),
            )?;
            owner.set_admission(
                now(),
                AdmissionState::OpenUntil(MonoTime::from_micros(at + INPUT_LEASE_US)),
            )?;
            if deadline.is_none()
                && cloud.available()
                && provider_control_empty
                && !submission_draining
            {
                deadline = Some(Instant::now() + Duration::from_secs(seconds));
                record(
                    trace,
                    json!({"kind":"listening_ready","at_us":at,"scope":"explicit directed session; addressee inference absent"}),
                )?;
                println!("{}", json!({"status":"listening_ready","seconds":seconds}));
            }
            let lineage = CaptureLineage {
                worker: rig.capture.boot,
                epoch: frame.epoch,
                dsp_epoch: frame.dsp_epoch,
                privacy_generation: frame.privacy_generation,
            };
            if capture_lineage.is_some_and(|old| old != lineage) {
                detector.discard_inactive_prefix();
                playback_mask.clear_read_lineage();
            }
            capture_lineage = Some(lineage);
            let context = AdmissionContext {
                capture: lineage,
                authority: owner.snapshot(now())?,
            };
            let observation = ObservedAudio {
                sequence: frame.frame_sequence,
                captured_at_us: frame.read_completed_at_us,
                samples: frame
                    .samples
                    .try_into()
                    .map_err(|_| io::Error::other("invalid capture block"))?,
            };
            let (update, input_ended) = observe_prospective_input(
                &mut playback_mask,
                &mut detector,
                &mut admission,
                context,
                observation,
                frame.timing.first_read_started_at_us,
                frame.probability,
                monotonic_us(),
            )?;
            let discarded_endpoint = submission_draining && input_ended;
            if discarded_endpoint {
                submission_draining = false;
                if let Some(current) = reply.as_mut()
                    && current.cloud.input_abandoned
                    && !current.local_input_ended
                {
                    current.local_input_ended = true;
                    owner.input_ended(now(), current.owner)?;
                    snapshot = owner.snapshot(now())?;
                    rig.publish(snapshot)?;
                    last_publish = monotonic_us();
                }
                record(
                    trace,
                    json!({"kind":"input_discarded_endpoint","at_us":at,
                    "last_block_host_read_us":frame.read_completed_at_us}),
                )?;
            }
            if let Some(rejection) = update.rejection {
                record_input_rejection(trace, rejection)?;
            }
            if let AdmissionStep::Candidate(candidate) = &update.step {
                record(
                    trace,
                    json!({"kind":"input_candidate","candidate":candidate,"at_us":monotonic_us()}),
                )?;
            }
            let step = match update.step {
                AdmissionStep::None => admission
                    .pending_evidence(monotonic_us(), AcceptanceBasis::DirectedSessionVadOnly)
                    .map_or(AdmissionStep::None, |(candidate, _)| {
                        AdmissionStep::Candidate(candidate)
                    }),
                step => step,
            };
            match step {
                AdmissionStep::None => {}
                AdmissionStep::Rejected(rejection) => record_input_rejection(trace, rejection)?,
                AdmissionStep::Candidate(candidate) => {
                    // Control and PCM travel independently. Recheck the original
                    // write receipts after reading this candidate, with the same
                    // finite tick budget; exhaustion keeps the input pending.
                    if !service_speaker_control(
                        rig,
                        owner,
                        reply,
                        &mut speaker_pending,
                        &mut playback_mask,
                        &mut snapshot,
                        &mut last_publish,
                        &mut speaker_control_budget,
                        trace,
                    )? {
                        continue;
                    }
                    let Some(accepted) = decide_prospective_input(
                        &playback_mask,
                        &mut detector,
                        &mut admission,
                        candidate,
                        AdmissionContext {
                            capture: lineage,
                            authority: owner.snapshot(now())?,
                        },
                        monotonic_us(),
                        trace,
                    )?
                    else {
                        continue;
                    };
                    // Recheck priority immediately before admitting a new owner.
                    // A coalesced recovery is a session change even when its outage
                    // observation never fitted in the priority socket.
                    provider_control_empty = service_provider_control(
                        &mut rig.provider.channels.control,
                        &mut cloud,
                        &mut provider_control_budget,
                        trace,
                    )?;
                    apply_submission_gate(
                        &cloud,
                        reply,
                        &mut provider_queue,
                        &mut admission,
                        &mut submission_draining,
                        trace,
                    )?;
                    let Some(provider_session) = cloud
                        .ready_session()
                        .filter(|_| provider_control_empty && !submission_draining)
                    else {
                        let (state, draining) =
                            accepted_without_cloud(accepted, owner, reply, &mut admission, trace)?;
                        provider_queue.clear();
                        speaker_pending = None;
                        submission_draining = draining;
                        snapshot = state;
                        rig.publish(snapshot)?;
                        last_publish = monotonic_us();
                        continue;
                    };
                    snapshot = commit_admitted_input(
                        owner,
                        reply,
                        &mut speaker_pending,
                        &mut provider_queue,
                        accepted,
                        provider_session,
                        rig.playout_start,
                        trace,
                        |state| rig.publish(state),
                    )?;
                    last_publish = monotonic_us();
                }
                AdmissionStep::Audio(audio) => {
                    if let Some(current) = reply.as_mut() {
                        queue_input(
                            &mut provider_queue,
                            current,
                            snapshot.microphone_generation(),
                            audio,
                        )?;
                    } else {
                        return Err(io::Error::other("speech continuation lost its owner").into());
                    }
                }
                AdmissionStep::End(audio) => {
                    let current = reply
                        .as_mut()
                        .ok_or_else(|| io::Error::other("speech end lost its owner"))?;
                    let input_end_us = audio.captured_at_us;
                    current.local_input_ended = true;
                    queue_input(
                        &mut provider_queue,
                        current,
                        snapshot.microphone_generation(),
                        audio,
                    )?;
                    owner.input_ended(now(), current.owner)?;
                    snapshot = owner.snapshot(now())?;
                    rig.publish(snapshot)?;
                    last_publish = monotonic_us();
                    provider_queue.push_back(ProviderInput::End {
                        request: current.owner.turn(),
                        privacy_generation: snapshot.microphone_generation(),
                    });
                    record(
                        trace,
                        json!({"kind":"local_endpoint","owner":current.owner,"turn":current.owner.turn(),"at_us":at,"last_block_host_read_us":input_end_us,"includes_silence_wait_ms":detector.endpoint_silence_ms(),"acoustic_speech_end":null}),
                    )?;
                }
            }
        }
        if let Some(at) = last_capture
            && monotonic_us().saturating_sub(at) >= INPUT_LEASE_US
        {
            return Err(io::Error::other("microphone retention heartbeat expired").into());
        }
        if clock.saturating_sub(last_publish) >= 20_000 {
            snapshot = owner.snapshot(now())?;
            rig.publish(snapshot)?;
            last_publish = clock;
        }
        provider_control_empty = service_provider_control(
            &mut rig.provider.channels.control,
            &mut cloud,
            &mut provider_control_budget,
            trace,
        )?;
        apply_submission_gate(
            &cloud,
            reply,
            &mut provider_queue,
            &mut admission,
            &mut submission_draining,
            trace,
        )?;
        if provider_control_empty {
            flush_provider_input(
                &cloud,
                reply,
                &mut provider_queue,
                &mut rig.provider.channels.data,
            )?;
        }
        dispatch_speaker_chunk(
            owner,
            reply,
            &mut rig.speaker.channels.data,
            &mut speaker_pending,
            &mut speaker_sequence,
            &mut snapshot,
            &mut playback_mask,
        )?;
        let available = playback_mask.available(monotonic_us());
        if policy_available != Some(available) {
            record(
                trace,
                json!({"kind":"turn_taking_availability","at_us":monotonic_us(),"policy":rig.turn_taking,"prospective_input_available":available,"hard_capture_authority_unchanged":true}),
            )?;
            policy_available = Some(available);
        }
        output_flow.replenish(
            reply.as_ref().map_or(0, |current| current.pcm.len()),
            &mut rig.provider.channels.control,
        )?;
        if let Some(current) = reply.as_mut()
            && current.provider_idle
            && current.cloud.outcome.is_some()
            && current.local_input_ended
            && current.pcm.is_empty()
            && speaker_pending.is_none()
            && current.playback.is_none()
        {
            let turn = current.owner.turn();
            owner.complete_turn(now(), current.owner)?;
            snapshot = owner.snapshot(now())?;
            admission.owner_completed(current.owner, snapshot, monotonic_us())?;
            rig.publish(snapshot)?;
            last_publish = monotonic_us();
            record(
                trace,
                json!({"kind":"turn_finished","turn":turn,"generation":current.owner.generation(),"at_us":monotonic_us(),"outcome":current.delivery_outcome(),"playback_gaps":current.playback_gaps,
                    "provider_session":current.cloud.session,"provider_outcome":current.cloud.outcome,
                "start_gate":current.start_trace(),"start_deficit_us":current.need.deficit_us(),
                "start_horizon_needed_us":current.need.horizon_us(),
                "start_need_scope":"all retained reply arrivals including later generations; excludes local handoff",
                "start_need_censored":!matches!(current.cloud.outcome,Some(RequestOutcome::Complete))}),
            )?;
            *reply = None;
        }
        service_ring(
            rig.ring.as_mut(),
            choreography.as_mut(),
            owner,
            trace,
            &mut ring_trace,
        )?;
        if clock.saturating_sub(last_health) >= 50_000 {
            rig.running()?;
            last_health = clock;
        }
        flush_cues(trace, cues);
        if let Some(wait) = Duration::from_millis(2).checked_sub(loop_start.elapsed()) {
            std::thread::sleep(wait);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn observe_prospective_input(
    mask: &mut PlaybackMask,
    detector: &mut TurnDetector,
    admission: &mut InputAdmission,
    context: AdmissionContext,
    observation: ObservedAudio,
    first_read_us: u64,
    probability: f32,
    at_us: u64,
) -> Result<(AdmissionUpdate, bool)> {
    mask.remember_read(
        observation.sequence,
        first_read_us,
        observation.captured_at_us,
        at_us,
    )?;
    if !admission.owns_stream() && mask.frame() == Disposition::Suppress {
        let rejection = admission.suppress_prospective(context, at_us)?;
        detector
            .suppress(&observation, probability)
            .map_err(io::Error::other)?;
        mask.suppressed()?;
        return Ok((
            AdmissionUpdate {
                rejection,
                step: AdmissionStep::None,
            },
            false,
        ));
    }
    let activity = detector.push(observation, probability);
    let ended = matches!(activity, Activity::End(_));
    Ok((admission.observe(activity, context, at_us)?, ended))
}

fn decide_prospective_input(
    mask: &PlaybackMask,
    detector: &mut TurnDetector,
    admission: &mut InputAdmission,
    candidate: crate::admission::Candidate,
    context: AdmissionContext,
    at_us: u64,
    trace: &mut Vec<Value>,
) -> Result<Option<Accepted>> {
    match mask.candidate(candidate)? {
        Disposition::AwaitWriteEvidence => return Ok(None),
        Disposition::Suppress => {
            if let Some(rejection) = admission.suppress_prospective(context, at_us)? {
                record_input_rejection(trace, rejection)?;
            }
            detector.clear_prospective();
            return Ok(None);
        }
        Disposition::Pass => {}
    }
    if mask.enabled() {
        let Some(audio) = admission.pending_audio(candidate.id) else {
            return Ok(None);
        };
        let mut last_masked = None;
        let mut awaiting_write = false;
        for frame in audio {
            match mask.retained_frame(frame)? {
                Disposition::Suppress => last_masked = Some(frame.sequence),
                Disposition::AwaitWriteEvidence => awaiting_write = true,
                Disposition::Pass => {}
            }
        }
        // A delayed receipt may cover pre-roll retained before the six fresh
        // onset blocks. Never splice disjoint blocks or erase an accepted input.
        if let Some(through_sequence) = last_masked {
            let first_onset = candidate
                .trigger_sequence
                .checked_sub(u64::from(crate::activity::START_BLOCKS) - 1)
                .ok_or_else(|| io::Error::other("candidate has no complete onset"))?;
            if through_sequence >= first_onset {
                if let Some(rejection) = admission.suppress_prospective(context, at_us)? {
                    record_input_rejection(trace, rejection)?;
                }
                detector.clear_prospective();
                return Ok(None);
            }
            let removed = admission.discard_pending_prefix(candidate.id, through_sequence)?;
            record(
                trace,
                json!({"kind":"input_prefix_sanitized","candidate":candidate.id,
                    "through_sequence":through_sequence,"removed_blocks":removed,
                    "reason":"own_playback_prefix","at_us":at_us}),
            )?;
        }
        if awaiting_write {
            return Ok(None);
        }
    }
    let basis = if mask.enabled() {
        AcceptanceBasis::DirectedSessionV1Baseline
    } else {
        AcceptanceBasis::DirectedSessionVadOnly
    };
    let Some((_, evidence)) = admission.pending_evidence(at_us, basis) else {
        return Ok(None);
    };
    accepted_decision(admission.decide(evidence, context, at_us)?, trace)
}

#[allow(clippy::too_many_arguments)]
fn commit_admitted_input(
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    speaker_pending: &mut Option<PendingSpeaker>,
    provider_queue: &mut VecDeque<ProviderInput>,
    accepted: Accepted,
    provider_session: u64,
    playout_start: PlayoutStart,
    trace: &mut Vec<Value>,
    mut publish: impl FnMut(Snapshot) -> io::Result<()>,
) -> Result<Snapshot> {
    let prefix = accepted.audio;
    let replacing = reply.is_some();
    cancel_reply(owner, reply, trace, "user_interrupted")?;
    let input = if replacing {
        AdmittedInput::Interruption
    } else {
        AdmittedInput::NewTurn
    };
    *speaker_pending = None;
    provider_queue.clear();
    let turn = owner.admit(now(), input)?;
    let plan = owner.plan_output(now(), turn, OutputKind::Speech, None)?;
    let mut snapshot = owner.snapshot(now())?;
    publish(snapshot)?;
    let admitted_at_us = monotonic_us();
    record(
        trace,
        json!({"kind":"input_admitted","owner":turn,"turn":turn.turn(),"at_us":admitted_at_us,"authority_issued_at_us":snapshot.issued_at().as_micros(),"prefix_first_host_read_us":prefix.first().map(|p|p.captured_at_us),"candidate":accepted.candidate.id,"admission_basis":accepted.basis,"provider_session":provider_session}),
    )?;
    provider_queue.push_back(ProviderInput::Start {
        session: provider_session,
        request: turn.turn(),
        privacy_generation: snapshot.microphone_generation(),
    });
    let mut current = Reply {
        owner: turn,
        cloud: CloudRequest::new(provider_session)?,
        local_input_ended: false,
        plan,
        playback: None,
        pcm: VecDeque::new(),
        generation_done: false,
        provider_idle: false,
        input_sequence: 0,
        had_audio: false,
        final_chunk: None,
        playback_gaps: 0,
        last_provider_audio_at_us: None,
        start: StartGate::new(playout_start.as_micros()),
        need: StartNeed::default(),
    };
    for audio in prefix {
        queue_input(
            provider_queue,
            &mut current,
            snapshot.microphone_generation(),
            audio,
        )?;
    }
    if accepted.ended {
        current.local_input_ended = true;
        owner.input_ended(now(), current.owner)?;
        provider_queue.push_back(ProviderInput::End {
            request: current.owner.turn(),
            privacy_generation: snapshot.microphone_generation(),
        });
        snapshot = owner.snapshot(now())?;
        publish(snapshot)?;
    }
    *reply = Some(current);
    Ok(snapshot)
}

/// All hardware I/O remains in the child. This path has one outstanding frame,
/// bounded nonblocking socket work, and no retry wait that can block capture.
/// Worker reports are evidence only; they never change conversational ownership.
fn service_ring(
    worker: Option<&mut Worker>,
    choreography: Option<&mut RingChoreographer>,
    owner: &mut Controller,
    trace: &mut Vec<Value>,
    ring_trace: &mut RingTrace,
) -> Result<()> {
    service_ring_with_clock(worker, choreography, owner, trace, ring_trace, now)
}

fn service_ring_with_clock(
    worker: Option<&mut Worker>,
    choreography: Option<&mut RingChoreographer>,
    owner: &mut Controller,
    trace: &mut Vec<Value>,
    ring_trace: &mut RingTrace,
    mut clock: impl FnMut() -> MonoTime,
) -> Result<()> {
    let (worker, choreography) = match (worker, choreography) {
        (None, None) => return Ok(()),
        (Some(worker), Some(choreography)) => (worker, choreography),
        _ => return Err(io::Error::other("ring worker and policy configuration disagree").into()),
    };
    let result: Result<()> = (|| {
        for _ in 0..16 {
            match worker.channels.control.receive::<WorkerEvent>()? {
                Some(WorkerEvent::Ring { report }) => {
                    let validation = owner
                        .snapshot(clock())
                        .map_err(io::Error::other)
                        .and_then(|current| choreography.feedback(&report, current, clock()));
                    let error = validation.as_ref().err().map(ToString::to_string);
                    ring_trace.feedback(report, clock().as_micros(), error.as_deref(), trace)?;
                    validation?;
                }
                Some(WorkerEvent::Fault { code }) => return Err(io::Error::other(code).into()),
                Some(_) => return Err(io::Error::other("unexpected ring worker event").into()),
                None => break,
            }
        }
        if let Some(frame) = choreography.prepare(owner, clock())? {
            // The data frame can never install authority or extend its own lease.
            // The worker rereads this priority path after receiving the data frame.
            worker.channels.control.send(Control::Authority {
                snapshot: frame.snapshot,
            })?;
            worker.channels.data.send(frame)?;
            ring_trace.requested(frame, trace)?;
        }
        Ok(())
    })();
    if let Err(error) = &result {
        // Include a submitted renewal's original identity even when the next
        // receipt/transport faults. Trace exhaustion still retains run_end.
        let _ = record(
            trace,
            json!({"kind":"ring_service_fault",
            "at_us":clock().as_micros(), "error":error.to_string(),
            "pending_frame":choreography.pending()}),
        );
    }
    result
}

/// Called only after this tick's retained input and cancellation work. Keeping
/// one outstanding chunk bounds handoff. The optional reply-start gate makes
/// one bounded queue decision and never waits inside this loop.
#[allow(clippy::too_many_arguments)]
fn dispatch_speaker_chunk(
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    channel: &mut Channel,
    pending: &mut Option<PendingSpeaker>,
    sequence: &mut u64,
    snapshot: &mut Snapshot,
    playback_mask: &mut PlaybackMask,
) -> Result<()> {
    if pending.is_some() {
        return Ok(());
    }
    let Some(current) = reply.as_mut() else {
        return Ok(());
    };
    // Only source-confirmed output can acquire a speech permit. Link loss does
    // not invalidate previously owned PCM; the real input endpoint still does
    // determine when speaking is locally permissible.
    if !current.cloud.started || !current.local_input_ended {
        return Ok(());
    }
    if !current.may_start(monotonic_us()) {
        return Ok(());
    }
    let Some((samples, final_chunk)) = current.next_speaker_chunk() else {
        return Ok(());
    };
    let permit = owner.issue_output(now(), current.plan, 100_000)?;
    *snapshot = owner.snapshot(now())?;
    *sequence = sequence
        .checked_add(1)
        .ok_or_else(|| io::Error::other("speaker counter exhausted"))?;
    let dispatch_at = monotonic_us();
    channel.send(SpeakerChunk {
        snapshot: *snapshot,
        permit,
        chunk_sequence: *sequence,
        end_of_speech: final_chunk,
        samples,
    })?;
    playback_mask.dispatched(permit.owner(), *sequence, dispatch_at, 240)?;
    *pending = Some(PendingSpeaker {
        sequence: *sequence,
        permit,
        accepted: 0,
        final_chunk,
    });
    Ok(())
}

fn queue_input(
    queue: &mut VecDeque<ProviderInput>,
    reply: &mut Reply,
    privacy_generation: u64,
    audio: ObservedAudio,
) -> Result<()> {
    if queue.len() >= 64 {
        return Err(io::Error::other("microphone to provider queue exceeded bound").into());
    }
    reply.input_sequence += 1;
    queue.push_back(ProviderInput::Audio {
        request: reply.owner.turn(),
        privacy_generation,
        sequence: reply.input_sequence,
        read_completed_at_us: audio.captured_at_us,
        samples: audio.samples.to_vec(),
    });
    Ok(())
}

fn drain_capture_control(
    channel: &mut Channel,
    epoch: &mut Option<u64>,
    expected_noise_suppression: NoiseSuppression,
    trace: &mut Vec<Value>,
) -> Result<()> {
    for _ in 0..16 {
        let Some(event) = channel.receive::<WorkerEvent>()? else {
            break;
        };
        match event {
            WorkerEvent::CapturePrepared { .. } => record(
                trace,
                json!({"kind":"capture_prepared","at_us":monotonic_us(),"details":event}),
            )?,
            WorkerEvent::CaptureStarted {
                epoch: started,
                noise_suppression,
                ..
            } if epoch.is_none() => {
                if noise_suppression != expected_noise_suppression {
                    record(
                        trace,
                        json!({"kind":"capture_processing_mismatch", "at_us":monotonic_us(),
                        "expected_noise_suppression":expected_noise_suppression, "details":event}),
                    )?;
                    return Err(io::Error::other(
                        "capture processing mode differs from requested mode",
                    )
                    .into());
                }
                *epoch = Some(started);
                record(
                    trace,
                    json!({"kind":"capture_started","at_us":monotonic_us(),"details":event}),
                )?;
            }
            WorkerEvent::CaptureDspReset { .. } => record(
                trace,
                json!({"kind":"echo_reference_reset","at_us":monotonic_us(),"details":event}),
            )?,
            WorkerEvent::ReferenceClock { timing } => record(
                trace,
                json!({"kind":"reference_clock","at_us":monotonic_us(),"details":timing}),
            )?,
            WorkerEvent::ReferenceFault { fault } => {
                record(
                    trace,
                    json!({"kind":"reference_fault","worker":"capture","details":fault}),
                )?;
                return Err(fault.into());
            }
            WorkerEvent::Fault { code } => return Err(io::Error::other(code).into()),
            _ => return Err(io::Error::other("unexpected capture lifecycle event").into()),
        }
    }
    Ok(())
}

fn validate_capture_frame(
    channel: &mut Channel,
    epoch: &mut Option<u64>,
    privacy_generation: u64,
    frame: &MicrophoneFrame,
    expected_noise_suppression: NoiseSuppression,
    trace: &mut Vec<Value>,
) -> Result<()> {
    // The worker sends its start report before its first frame, but the two
    // sockets may become readable between our control drain and data receive.
    // Re-read control after receiving the frame instead of discarding its words.
    if epoch.is_none() {
        drain_capture_control(channel, epoch, expected_noise_suppression, trace)?;
    }
    if Some(frame.epoch) != *epoch || frame.privacy_generation != privacy_generation {
        return Err(io::Error::other("microphone lineage changed during test").into());
    }
    if frame.timing.reference.playback_epoch == 0
        || (frame.frame_sequence == 1 && frame.timing.reference.clock_started_at_us.is_none())
    {
        return Err(
            io::Error::other("capture frame preceded certified running reference clock").into(),
        );
    }
    Ok(())
}

fn record_input_rejection(trace: &mut Vec<Value>, rejection: Rejection) -> Result<()> {
    record(
        trace,
        json!({"kind":"input_candidate_rejected", "candidate":rejection.candidate,
        "reason":rejection.reason, "at_us":monotonic_us()}),
    )
}

fn cancel_reply(
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    trace: &mut Vec<Value>,
    reason: &str,
) -> Result<()> {
    if let Some(current) = reply.take() {
        // Cancellation invalidates permits already delivered to the speaker.
        // Dropping this reply also discards PCM waiting in the controller.
        let cancel = owner.cancel_turn(now(), current.owner);
        record(
            trace,
            json!({"kind":"turn_finished","owner":current.owner,"turn":current.owner.turn(),"at_us":monotonic_us(),"outcome":reason,"provider_audio_seen":current.had_audio,"playback_gaps":current.playback_gaps,
                "provider_session":current.cloud.session,"provider_outcome":current.cloud.outcome,
                "start_gate":current.start_trace(),"start_deficit_us":current.need.deficit_us(),
                "start_horizon_needed_us":current.need.horizon_us(),
                "start_need_scope":"all retained reply arrivals including later generations; excludes local handoff",
                "start_need_censored":true}),
        )?;
        cancel?;
    }
    Ok(())
}

/// An old accepted tail can retire after the successor has started or even
/// finished. This receipt is evidence only: it cannot complete either answer or
/// borrow the newer turn's playback occurrence.
fn handle_cancelled_tail_retirement(
    reply: &Option<Reply>,
    trace: &mut Vec<Value>,
    event: WorkerEvent,
    received_at_us: u64,
) -> Result<()> {
    let WorkerEvent::CancelledTailRetired {
        owner: affected,
        superseded_by,
        first_sample,
        end_sample,
        queued_frames,
        speech_frames,
        started_at_us,
        observed_at_us,
    } = event
    else {
        return Err(io::Error::other("expected cancelled tail retirement event").into());
    };
    let elapsed = observed_at_us.checked_sub(started_at_us);
    if affected.boot() != superseded_by.boot()
        || affected.turn() >= superseded_by.turn()
        || affected.generation() >= superseded_by.generation()
        || first_sample.epoch == 0
        || first_sample.epoch != end_sample.epoch
        || end_sample.frame.checked_sub(first_sample.frame) != Some(queued_frames as u64)
        || queued_frames == 0
        || queued_frames > lamp_audio::ownership::MAX_QUEUED_FRAMES
        || speech_frames == 0
        || speech_frames > queued_frames
        || started_at_us == 0
        || elapsed.is_none_or(|elapsed| elapsed >= lamp_audio::ownership::MAX_CANCELLED_TAIL_US)
        || observed_at_us > received_at_us
        || reply
            .as_ref()
            .is_some_and(|current| current.owner == affected)
    {
        return Err(io::Error::other("invalid cancelled tail retirement receipt").into());
    }
    let affected_json = json!(affected);
    let successor_json = json!(superseded_by);
    let cancelled_index = trace.iter().rposition(|entry| {
        entry["kind"] == "turn_finished"
            && entry["owner"] == affected_json
            && entry["outcome"] == "user_interrupted"
            && entry["at_us"]
                .as_u64()
                .is_some_and(|at| at <= started_at_us)
    });
    let matched_admission = cancelled_index.is_some_and(|index| {
        let Some(cancelled_at) = trace[index]["at_us"].as_u64() else {
            return false;
        };
        trace[index + 1..].iter().any(|entry| {
            entry["kind"] == "input_admitted"
                && entry["owner"] == successor_json
                && entry["authority_issued_at_us"]
                    .as_u64()
                    .is_some_and(|issued| cancelled_at <= issued && issued <= started_at_us)
        })
    });
    if !matched_admission
        || trace.iter().any(|entry| {
            entry["kind"] == "cancelled_tail_retired" && entry["owner"] == affected_json
        })
    {
        return Err(io::Error::other("cancelled tail has no unique matching supersession").into());
    }
    record(
        trace,
        json!({"kind":"cancelled_tail_retired","owner":affected,"turn":affected.turn(),
        "superseded_by":superseded_by,"first_sample":first_sample,"end_sample":end_sample,
        "queued_frames":queued_frames,"speech_frames":speech_frames,
        "started_at_us":started_at_us,"at_us":observed_at_us,
        "coordinator_observed_at_us":received_at_us,
        "outcome":"cancelled","boundary":"ALSA retirement of previously accepted cancelled PCM; acoustic silence unmeasured"}),
    )?;
    Ok(())
}

fn handle_playback_discard(
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    trace: &mut Vec<Value>,
    event: WorkerEvent,
) -> Result<()> {
    let WorkerEvent::PlaybackDiscarded {
        owner: affected,
        reason,
        queued_frames,
        other_owners,
        observed_at_us,
    } = event
    else {
        return Err(io::Error::other("expected playback discard event").into());
    };
    if queued_frames == 0 || queued_frames > lamp_audio::ownership::MAX_QUEUED_FRAMES {
        return Err(io::Error::other("invalid playback discard frame count").into());
    }
    let affected_json = json!(affected);
    // The existing finite trace is an immutable receipt of explicit cancellation.
    // Owner identity includes controller boot and generation, not just turn ID.
    let cancelled = trace
        .iter()
        .rev()
        .find(|entry| entry["kind"] == "turn_finished" && entry["owner"] == affected_json);
    let expected = reason == PlaybackDiscardReason::StaleOwner
        && !other_owners
        && reply
            .as_ref()
            .is_none_or(|current| current.owner != affected)
        && cancelled.is_some_and(|entry| {
            matches!(
                entry["outcome"].as_str(),
                Some("user_interrupted" | "provider_interrupted" | "deadline_cancelled")
            )
        });
    record(
        trace,
        json!({"kind":"playback_discarded","owner":affected,"turn":affected.turn(),
            "reason":reason,"queued_frames":queued_frames,"other_owners":other_owners,
            "observed_at_us":observed_at_us,"at_us":monotonic_us(),
            "expected_cancellation":expected}),
    )?;
    if expected {
        return Ok(());
    }
    // The physical writer already stopped. Revoke any remaining output and end
    // this trial explicitly; late queue-empty callbacks cannot turn it into success.
    cancel_reply(owner, reply, trace, "playback_discarded")?;
    Err(io::Error::other(format!(
        "unexpected playback discard: turn={} reason={reason:?} frames={queued_frames} mixed={other_owners}",
        affected.turn()
    ))
    .into())
}

fn handle_playback_gap(
    reply: &mut Option<Reply>,
    pending: Option<&PendingSpeaker>,
    trace: &mut Vec<Value>,
    event: WorkerEvent,
) -> Result<()> {
    let WorkerEvent::PlaybackGap {
        phase,
        gap,
        observed_at_us,
    } = event
    else {
        return Err(io::Error::other("expected playback gap event").into());
    };
    if gap.first_sample.epoch == 0
        || gap.first_sample.epoch != gap.end_sample.epoch
        || gap.zero_frames == 0
        || gap.end_sample.frame.checked_sub(gap.first_sample.frame) != Some(gap.zero_frames)
        || gap.started_at_us == 0
        || gap.last_zero_accepted_at_us < gap.started_at_us
        || observed_at_us < gap.last_zero_accepted_at_us
        || (phase == PlaybackGapPhase::Started && gap.zero_frames > 240)
    {
        return Err(io::Error::other("invalid accepted playback gap receipt").into());
    }
    let current = reply.as_mut().filter(|reply| reply.owner == gap.owner);
    let (queued, held, last_provider_audio) = if let Some(current) = current {
        if phase == PlaybackGapPhase::Started {
            current.playback_gaps = current
                .playback_gaps
                .checked_add(1)
                .ok_or_else(|| io::Error::other("reply playback gap counter exhausted"))?;
        }
        (
            Some(current.pcm.len()),
            Some(current.held_final_samples()),
            current.last_provider_audio_at_us,
        )
    } else {
        (None, None, None)
    };
    record(
        trace,
        json!({"kind":"playback_gap","phase":phase,"gap":gap,
        "at_us":observed_at_us,"coordinator_observed_at_us":monotonic_us(),
        "reply_queued_frames_at_observation":queued,"held_final_frames_at_observation":held,
        "speaker_pending_sequence_at_observation":pending.map(|pending| pending.sequence),
        "last_provider_audio_enqueued_at_us":last_provider_audio,
        "boundary":"actual zero PCM accepted while reply unfinished; acoustic gap unmeasured"}),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    mod playout_start;
    mod provider_pair;
    mod termination;
    mod turn_taking;

    use super::*;
    use crate::{
        admission::{Evidence, Verdict},
        wire::CaptureTiming,
    };
    use lamp_interaction::BoundaryGuard;

    #[test]
    fn ring_lifetime_includes_startup_and_shutdown_around_maximum_active_session() {
        let required = Duration::from_secs(MAX_DIRECTED_SECONDS)
            + crate::process::WORKER_STARTUP_TIMEOUT
            + INPUT_READINESS_TIMEOUT
            + DIAGNOSTIC_SHUTDOWN_GRACE;
        assert!(required < Duration::from_micros(crate::ring_worker::MAX_RUNTIME_US));
    }

    #[test]
    fn ring_service_orders_authority_and_preserves_successor_on_late_feedback() {
        use crate::ring_wire::{RingFrame, RingPhase};

        let directory = SessionDirectory::create().unwrap();
        let (mut worker, mut peer) = Worker::sleeping_fixture(&directory.path, "ring");
        let mut owner = Controller::new(new_boot().unwrap(), now());
        let at = now();
        let until = at.checked_add(INPUT_LEASE_US).unwrap();
        owner
            .set_microphone_permission(at, Permission::Allowed)
            .unwrap();
        owner
            .set_capture(at, CaptureState::RetainingUntil(until))
            .unwrap();
        owner
            .set_admission(at, AdmissionState::OpenUntil(until))
            .unwrap();
        let previous = owner.admit(at, AdmittedInput::NewTurn).unwrap();
        let mut policy = RingChoreographer::new(24).unwrap();
        let mut trace = Vec::new();
        let mut ring_trace = RingTrace::default();

        service_ring(
            Some(&mut worker),
            Some(&mut policy),
            &mut owner,
            &mut trace,
            &mut ring_trace,
        )
        .unwrap();
        let frame = peer.data.receive::<RingFrame>().unwrap().unwrap();
        assert_eq!(frame.permit.owner(), previous);
        assert!(matches!(peer.control.receive::<Control>().unwrap(),
            Some(Control::Authority { snapshot }) if snapshot == frame.snapshot));
        let written_at_us = monotonic_us();
        let successor = owner.admit(now(), AdmittedInput::NewTurn).unwrap();
        peer.control
            .send(WorkerEvent::Ring {
                report: RingFeedback::Presented {
                    owner: previous,
                    phase: frame.phase,
                    requested_at_us: frame.requested_at_us,
                    write_started_at_us: written_at_us,
                    write_finished_at_us: written_at_us,
                },
            })
            .unwrap();

        service_ring(
            Some(&mut worker),
            Some(&mut policy),
            &mut owner,
            &mut trace,
            &mut ring_trace,
        )
        .unwrap();
        assert_eq!(owner.snapshot(now()).unwrap().owner(), Some(successor));
        let next = peer.data.receive::<RingFrame>().unwrap().unwrap();
        assert_eq!(next.permit.owner(), successor);
        assert_eq!(next.phase, RingPhase::Listening);
        assert!(matches!(peer.control.receive::<Control>().unwrap(),
            Some(Control::Authority { snapshot }) if snapshot == next.snapshot));
        assert_eq!(
            trace
                .iter()
                .filter(|event| event["kind"] == "ring_requested")
                .count(),
            2
        );
        assert_eq!(
            trace
                .iter()
                .filter(|event| event["kind"] == "ring_feedback")
                .count(),
            1
        );
    }

    #[test]
    fn diagnostic_shutdown_uses_one_deadline_and_reaps_every_hung_worker() {
        let directory = SessionDirectory::create().unwrap();
        let (speaker, _speaker_peer) = Worker::sleeping_fixture(&directory.path, "speaker");
        let (capture, _capture_peer) = Worker::sleeping_fixture(&directory.path, "capture");
        let (provider, _provider_peer) = Worker::sleeping_fixture(&directory.path, "provider");
        let (privacy, _privacy_peer) = Worker::sleeping_fixture(&directory.path, "privacy");
        let (ring, _ring_peer) = Worker::sleeping_fixture(&directory.path, "ring");
        let mut rig = Rig {
            speaker,
            capture,
            provider,
            privacy,
            ring: Some(ring),
            ring_channel_ceiling: Some(24),
            diagnostic_directory: Some(directory.path.clone()),
            noise_suppression: NoiseSuppression::On,
            turn_taking: TurnTaking::VadOnly,
            playout_start: PlayoutStart::default(),
        };
        let started = Instant::now();
        let mut trace = Vec::new();
        assert!(rig.shutdown(&mut trace).is_err());
        // A five-times-750 ms serial grace would fail this generous regression
        // bound. It is a test observation, not a kernel kill/wait deadline.
        assert!(started.elapsed() < Duration::from_millis(2500));
        for worker in [
            &mut rig.speaker,
            &mut rig.capture,
            &mut rig.provider,
            &mut rig.privacy,
        ]
        .into_iter()
        .chain(rig.ring.iter_mut())
        {
            assert!(worker.try_exit().unwrap().is_some());
        }
        assert_eq!(
            trace
                .iter()
                .filter(|entry| entry["kind"] == "audio_diagnostics_certified"
                    && entry["valid"] == false)
                .count(),
            2
        );
    }

    fn pair(dir: &SessionDirectory, role: &str) -> (WorkerChannels, WorkerChannels) {
        let parent = new_boot().unwrap();
        let worker = new_boot().unwrap();
        let mut a = WorkerChannels::bind(&dir.path, role, parent, worker, false).unwrap();
        let mut b = WorkerChannels::bind(&dir.path, role, parent, worker, true).unwrap();
        a.connect(&dir.path, role, false).unwrap();
        b.connect(&dir.path, role, true).unwrap();
        (a, b)
    }

    fn capture_started(mode: NoiseSuppression) -> WorkerEvent {
        let at = monotonic_us();
        WorkerEvent::CaptureStarted {
            epoch: 1,
            dsp_epoch: 1,
            noise_suppression: mode,
            privacy_generation: 2,
            rate: 16_000,
            channels: 1,
            period_frames: 160,
            buffer_frames: 640,
            monotonic_status_timestamps: false,
            open_started_at_us: at,
            prepared_at_us: at,
            capture_started_at_us: at,
            start_completed_at_us: at,
        }
    }

    #[test]
    fn actual_capture_mode_must_match_requested_mode_before_epoch_acceptance() {
        for requested in [NoiseSuppression::On, NoiseSuppression::Off] {
            for observed in [NoiseSuppression::On, NoiseSuppression::Off] {
                let dir = SessionDirectory::create().unwrap();
                let (mut parent, mut child) = pair(&dir, "capture");
                let mut epoch = None;
                let mut trace = Vec::new();
                child.control.send(capture_started(observed)).unwrap();
                let result =
                    drain_capture_control(&mut parent.control, &mut epoch, requested, &mut trace);
                assert_eq!(result.is_ok(), requested == observed);
                assert_eq!(epoch, (requested == observed).then_some(1));
                assert_eq!(trace.len(), 1);
                assert_eq!(trace[0]["details"]["noise_suppression"], observed.as_str());
                assert_eq!(
                    trace[0]["kind"],
                    if requested == observed {
                        "capture_started"
                    } else {
                        "capture_processing_mismatch"
                    }
                );
            }
        }
    }

    #[test]
    fn an_old_or_malformed_start_report_cannot_silently_infer_a_processing_mode() {
        let mut report = serde_json::to_value(capture_started(NoiseSuppression::Off)).unwrap();
        report.as_object_mut().unwrap().remove("noise_suppression");
        assert!(serde_json::from_value::<WorkerEvent>(report.clone()).is_err());
        report["noise_suppression"] = json!(false);
        assert!(serde_json::from_value::<WorkerEvent>(report).is_err());
    }

    #[test]
    fn first_microphone_frame_survives_start_report_between_channel_drains() {
        let dir = SessionDirectory::create().unwrap();
        let (mut parent, mut capture) = pair(&dir, "capture");
        let mut epoch = None;
        let mut trace = Vec::new();
        drain_capture_control(
            &mut parent.control,
            &mut epoch,
            NoiseSuppression::On,
            &mut trace,
        )
        .unwrap();
        assert!(epoch.is_none());
        let at = monotonic_us();
        capture
            .control
            .send(WorkerEvent::CaptureStarted {
                epoch: 1,
                dsp_epoch: 1,
                noise_suppression: NoiseSuppression::On,
                privacy_generation: 2,
                rate: 16_000,
                channels: 1,
                period_frames: 160,
                buffer_frames: 640,
                monotonic_status_timestamps: false,
                open_started_at_us: at,
                prepared_at_us: at,
                capture_started_at_us: at,
                start_completed_at_us: at,
            })
            .unwrap();
        capture
            .data
            .send(MicrophoneFrame {
                epoch: 1,
                dsp_epoch: 1,
                privacy_generation: 2,
                frame_sequence: 1,
                read_completed_at_us: at,
                probability: 0.95,
                samples: vec![123; 160],
                timing: CaptureTiming {
                    first_read_started_at_us: at,
                    last_read_started_at_us: at,
                    successful_reads: 1,
                    alsa_status_monotonic_us: None,
                    status_observed_at_us: at,
                    available_frames: 0,
                    delayed_frames: 0,
                    processing_completed_at_us: at,
                    aec_queue_delay_ms: 0,
                    reference: crate::wire::ReferenceTiming {
                        playback_epoch: 1,
                        clock_started_at_us: Some(at),
                        ..crate::wire::ReferenceTiming::default()
                    },
                },
            })
            .unwrap();
        let mut frame = parent.data.receive::<MicrophoneFrame>().unwrap().unwrap();
        validate_capture_frame(
            &mut parent.control,
            &mut epoch,
            2,
            &frame,
            NoiseSuppression::On,
            &mut trace,
        )
        .unwrap();
        assert_eq!(epoch, Some(1));
        assert_eq!(frame.samples, vec![123; 160]);
        assert_eq!(trace.len(), 1);
        frame.epoch = 2;
        assert!(
            validate_capture_frame(
                &mut parent.control,
                &mut epoch,
                2,
                &frame,
                NoiseSuppression::On,
                &mut trace
            )
            .is_err()
        );
    }

    fn ready_link(revision: u64, session: u64) -> ProviderLink {
        let at = monotonic_us();
        ProviderLink {
            revision,
            session,
            observed_at_us: at,
            source_clock: None,
            state: ProviderLinkState::Ready {
                setup_us: 1234,
                attempts: 1,
                context: if session == 1 {
                    crate::provider_link::LinkContext::Initial
                } else {
                    crate::provider_link::LinkContext::Resumed
                },
                outage_started_at_us: (session > 1).then_some(at - 10),
                after: (session > 1).then_some(crate::provider_worker::Fault::Transport),
            },
        }
    }

    #[test]
    fn startup_consumes_provider_ready_before_later_worker_exceeds_packet_age() {
        let dir = SessionDirectory::create().unwrap();
        let (mut parent, mut provider) = pair(&dir, "provider");
        provider
            .control
            .send(WorkerEvent::ProviderLink {
                link: ready_link(1, 1),
            })
            .unwrap();
        let mut startup = StartupEvents::default();
        startup.poll(&mut parent, "provider").unwrap();
        // Readiness is retained state, not an unread packet whose 100 ms
        // transport lease could expire during a later child's startup.
        std::thread::sleep(Duration::from_millis(120));
        startup.poll(&mut parent, "provider").unwrap();
        assert_eq!(startup.cloud.ready_session(), Some(1));
        assert_eq!(startup.events[0]["setup_us"], 1234);
        provider
            .control
            .send(WorkerEvent::ProviderLink {
                link: ready_link(3, 2),
            })
            .unwrap();
        startup.poll(&mut parent, "provider").unwrap();
        assert_eq!(startup.cloud.ready_session(), Some(2));
        provider
            .data
            .send(ProviderOutput::TurnComplete {
                request: 1,
                idle: true,
            })
            .unwrap();
        assert!(startup.poll(&mut parent, "provider").is_err());
    }

    fn submitted_request() -> CloudRequest {
        let mut request = CloudRequest::new(1).unwrap();
        request
            .submitted(&ProviderInput::Start {
                session: 1,
                request: 1,
                privacy_generation: 1,
            })
            .unwrap();
        request.confirm_start(1).unwrap();
        request
            .submitted(&ProviderInput::End {
                request: 1,
                privacy_generation: 1,
            })
            .unwrap();
        request
    }

    fn active_reply() -> (Controller, Reply, OutputPermit) {
        let mut owner = Controller::new(new_boot().unwrap(), now());
        owner
            .set_microphone_permission(now(), Permission::Allowed)
            .unwrap();
        let until = MonoTime::from_micros(monotonic_us() + 500_000);
        owner
            .set_capture(now(), CaptureState::RetainingUntil(until))
            .unwrap();
        owner
            .set_admission(now(), AdmissionState::OpenUntil(until))
            .unwrap();
        let turn = owner.admit(now(), AdmittedInput::NewTurn).unwrap();
        let plan = owner
            .plan_output(now(), turn, OutputKind::Speech, None)
            .unwrap();
        owner.input_ended(now(), turn).unwrap();
        let permit = owner.issue_output(now(), plan, 100_000).unwrap();
        let playback = owner.playback_started(now(), permit).unwrap();
        (
            owner,
            Reply {
                owner: turn,
                cloud: submitted_request(),
                local_input_ended: true,
                plan,
                playback: Some(playback),
                pcm: vec![123; 480].into(),
                generation_done: false,
                provider_idle: false,
                input_sequence: 1,
                had_audio: true,
                final_chunk: None,
                playback_gaps: 0,
                last_provider_audio_at_us: None,
                start: StartGate::new(0),
                need: StartNeed::default(),
            },
            permit,
        )
    }

    fn provider_audio(sequence: u64, samples: Vec<i16>) -> ProviderOutput {
        ProviderOutput::Audio {
            request: 1,
            sequence,
            provider_event_at_us: monotonic_us(),
            source_frames: samples.len(),
            source_offset: 0,
            samples,
        }
    }

    #[test]
    fn continued_generation_preserves_the_accepted_final_chunk_retirement() {
        let (mut owner, mut current, _) = active_reply();
        let original_playback = current.playback.unwrap();
        current.pcm.clear();
        current.generation_done = true;
        current.final_chunk = Some(41);
        current.append_provider_audio(vec![22; 480], monotonic_us());
        let token = current.retire_playback(41, 720).unwrap();
        assert_eq!(token, original_playback);
        owner.playback_ended(now(), token).unwrap();
        assert_eq!(current.final_chunk, None);
        assert_eq!(current.pcm, vec![22; 480]);
        assert!(!current.generation_done);
        assert!(current.playback.is_none());
        assert_eq!(owner.snapshot(now()).unwrap().owner(), Some(current.owner));
    }

    #[test]
    fn continued_generation_waits_for_the_previous_playback_occurrence_to_retire() {
        let (_, mut current, _) = active_reply();
        current.pcm.clear();
        current.generation_done = true;
        current.final_chunk = Some(41);
        current.append_provider_audio(vec![22; 480], monotonic_us());
        assert!(
            current.next_speaker_chunk().is_none(),
            "a later generation must not overwrite the speaker's unretired final cursor"
        );
        assert_eq!(current.pcm, vec![22; 480]);
    }

    #[test]
    fn continued_generation_interleavings_preserve_pcm_and_distinct_playback_tokens() {
        use crate::playback::SpeechPlayback;
        use lamp_audio::ownership::{PlaybackCursor, QueuedRange};

        // Audio may continue before final acceptance, before final retirement,
        // or after retirement. All are legal within one Gemini request.
        for arrival in 0..3 {
            let dir = SessionDirectory::create().unwrap();
            let (mut parent, mut speaker) = pair(&dir, "speaker");
            let (mut owner, mut current, _) = active_reply();
            let turn = current.owner;
            let first_token = current.playback.unwrap();
            current.pcm = vec![11; 240].into();
            current.generation_done = true;
            let mut reply = Some(current);
            let mut pending = None;
            let mut sequence = 0;
            let mut snapshot = owner.snapshot(now()).unwrap();
            dispatch_speaker_chunk(
                &mut owner,
                &mut reply,
                &mut parent.data,
                &mut pending,
                &mut sequence,
                &mut snapshot,
                &mut PlaybackMask::new(TurnTaking::VadOnly),
            )
            .unwrap();
            let first = speaker.data.receive::<SpeakerChunk>().unwrap().unwrap();
            assert!(first.end_of_speech);
            let mut played = first.samples.clone();
            if arrival == 0 {
                reply
                    .as_mut()
                    .unwrap()
                    .append_provider_audio(vec![22; 480], monotonic_us());
            }
            // No second chunk can bypass the outstanding final acceptance.
            dispatch_speaker_chunk(
                &mut owner,
                &mut reply,
                &mut parent.data,
                &mut pending,
                &mut sequence,
                &mut snapshot,
                &mut PlaybackMask::new(TurnTaking::VadOnly),
            )
            .unwrap();
            assert!(speaker.data.receive::<SpeakerChunk>().unwrap().is_none());
            assert_eq!(pending.as_ref().unwrap().sequence, first.chunk_sequence);
            pending = None; // Simulated full speaker acceptance of this chunk.
            reply.as_mut().unwrap().final_chunk = Some(first.chunk_sequence);
            if arrival == 1 {
                reply
                    .as_mut()
                    .unwrap()
                    .append_provider_audio(vec![22; 480], monotonic_us());
            }
            dispatch_speaker_chunk(
                &mut owner,
                &mut reply,
                &mut parent.data,
                &mut pending,
                &mut sequence,
                &mut snapshot,
                &mut PlaybackMask::new(TurnTaking::VadOnly),
            )
            .unwrap();
            assert!(pending.is_none());
            assert!(speaker.data.receive::<SpeakerChunk>().unwrap().is_none());

            let mut playback = SpeechPlayback::default();
            let first_cursor = PlaybackCursor { epoch: 1, frame: 0 };
            let end_cursor = PlaybackCursor {
                epoch: 1,
                frame: 240,
            };
            playback
                .speech_accepted(
                    turn,
                    first.chunk_sequence,
                    first_cursor,
                    end_cursor,
                    1,
                    true,
                )
                .unwrap();
            let retired = playback
                .take_retired(QueuedRange {
                    epoch: 1,
                    retired_through: 240,
                    accepted_through: 240,
                })
                .unwrap();
            let token = reply
                .as_mut()
                .unwrap()
                .retire_playback(retired.sequence, retired.cursor.frame)
                .unwrap();
            assert_eq!(token, first_token);
            owner.playback_ended(now(), token).unwrap();
            assert_eq!(owner.snapshot(now()).unwrap().owner(), Some(turn));
            if arrival == 2 {
                reply
                    .as_mut()
                    .unwrap()
                    .append_provider_audio(vec![22; 480], monotonic_us());
            }
            reply.as_mut().unwrap().generation_done = true;
            let mut frame = 240;
            let mut second_token = None;
            for _ in 0..2 {
                dispatch_speaker_chunk(
                    &mut owner,
                    &mut reply,
                    &mut parent.data,
                    &mut pending,
                    &mut sequence,
                    &mut snapshot,
                    &mut PlaybackMask::new(TurnTaking::VadOnly),
                )
                .unwrap();
                let chunk = speaker.data.receive::<SpeakerChunk>().unwrap().unwrap();
                let accepted = pending.take().unwrap();
                assert_eq!(accepted.sequence, chunk.chunk_sequence);
                let current = reply.as_mut().unwrap();
                if current.playback.is_none() {
                    let token = owner.playback_started(now(), accepted.permit).unwrap();
                    assert_eq!(token.owner(), turn);
                    assert!(token.sequence() > first_token.sequence());
                    current.playback = Some(token);
                    second_token = Some(token);
                }
                if accepted.final_chunk {
                    current.final_chunk = Some(accepted.sequence);
                }
                // A duplicated receipt from occurrence one cannot retire two.
                assert!(current.retire_playback(first.chunk_sequence, 240).is_err());
                assert_eq!(current.playback, second_token);
                playback
                    .speech_accepted(
                        turn,
                        chunk.chunk_sequence,
                        PlaybackCursor { epoch: 1, frame },
                        PlaybackCursor {
                            epoch: 1,
                            frame: frame + 240,
                        },
                        frame,
                        chunk.end_of_speech,
                    )
                    .unwrap();
                frame += 240;
                played.extend(chunk.samples);
            }
            let retired = playback
                .take_retired(QueuedRange {
                    epoch: 1,
                    retired_through: frame,
                    accepted_through: frame,
                })
                .unwrap();
            let current = reply.as_mut().unwrap();
            assert_eq!(retired.cursor.epoch, 1);
            let token = current
                .retire_playback(retired.sequence, retired.cursor.frame)
                .unwrap();
            assert_eq!(Some(token), second_token);
            owner.playback_ended(now(), token).unwrap();
            assert!(current.pcm.is_empty());
            assert!(current.playback.is_none());
            assert!(current.final_chunk.is_none());
            assert!(
                !current.provider_idle,
                "retirement is not provider completion"
            );
            assert_eq!(played, [vec![11; 240], vec![22; 480]].concat());
            current.provider_idle = true;
            owner.complete_turn(now(), current.owner).unwrap();
            assert!(owner.snapshot(now()).unwrap().owner().is_none());
        }
    }

    #[test]
    fn cancellation_between_or_during_later_generations_revokes_the_same_turn() {
        for second_started in [false, true] {
            let (mut owner, mut current, first_permit) = active_reply();
            current.pcm.clear();
            current.final_chunk = Some(1);
            let token = current.retire_playback(1, 240).unwrap();
            owner.playback_ended(now(), token).unwrap();
            current.append_provider_audio(vec![22; 480], monotonic_us());
            let permit = owner.issue_output(now(), current.plan, 100_000).unwrap();
            if second_started {
                current.playback = Some(owner.playback_started(now(), permit).unwrap());
                current.final_chunk = Some(3);
            }
            let turn = current.owner;
            let mut reply = Some(current);
            let mut trace = Vec::new();
            cancel_reply(&mut owner, &mut reply, &mut trace, "user_interrupted").unwrap();
            assert!(reply.is_none());
            let state = owner.snapshot(now()).unwrap();
            assert!(state.owner().is_none());
            let mut guard = BoundaryGuard::new(state.boot(), now());
            guard.install(now(), state).unwrap();
            for permit in [first_permit, permit] {
                assert!(guard.check(now(), permit, OutputKind::Speech).is_err());
            }
            assert_eq!(trace[0]["outcome"], "user_interrupted");
            assert_eq!(trace[0]["owner"], serde_json::to_value(turn).unwrap());
        }
    }

    #[test]
    fn ninety_second_reply_uses_bounded_credit_and_preserves_every_sample() {
        use crate::provider_flow::OutputSender;
        let dir = SessionDirectory::create().unwrap();
        let (mut parent, mut provider) = pair(&dir, "provider");
        let mut send_flow = OutputSender::default();
        let mut receive_flow = OutputReceiver::default();
        let (_, mut reply, _) = active_reply();
        reply.pcm.clear();
        let original: Vec<i16> = (0..24_000 * 90 + 73)
            .map(|index| (index % 32_749) as i16 - 16_000)
            .collect();
        let mut source = VecDeque::from([
            ProviderOutput::Ready { setup_us: 1 },
            ProviderOutput::Started {
                request: 1,
                session: 1,
                waiting_for_barrier: false,
            },
        ]);
        source.extend(
            original
                .chunks(PACKET_SAMPLES)
                .enumerate()
                .map(|(index, samples)| provider_audio(index as u64 + 1, samples.to_vec())),
        );
        source.extend([
            ProviderOutput::Transcript {
                request: Some(1),
                text: "done".into(),
                finished: true,
            },
            ProviderOutput::GenerationComplete { request: 1 },
            ProviderOutput::TurnComplete {
                request: 1,
                idle: true,
            },
        ]);
        let packets = source.len();
        let mut received = 0;
        let mut played = Vec::new();
        let mut high_water = 0;
        let mut credit_waits = 0;
        let mut final_chunks = 0;
        for tick in 0..50_000 {
            while let Some(Control::ProviderOutputCapacity { through }) =
                provider.control.receive().unwrap()
            {
                send_flow.grant(through).unwrap();
            }
            for _ in 0..8 {
                let Some(event) = source.front() else {
                    break;
                };
                if !send_flow.try_send(|| provider.data.send(event)).unwrap() {
                    credit_waits += 1;
                    break;
                }
                source.pop_front();
            }
            let mut slice = ProviderSlice::new(monotonic_us());
            while let Some(event) = slice.receive(&mut parent.data, monotonic_us()).unwrap() {
                receive_flow.received().unwrap();
                received += 1;
                match event {
                    ProviderOutput::Audio { samples, .. } => reply.pcm.extend(samples),
                    ProviderOutput::GenerationComplete { .. } => reply.generation_done = true,
                    ProviderOutput::TurnComplete { idle, .. } => reply.provider_idle = idle,
                    _ => {}
                }
                high_water = high_water.max(reply.pcm.len());
                assert!(reply.pcm.len() <= MAX_REPLY_SAMPLES);
            }
            // The first second deliberately withholds playback, then the
            // production 240-frame chunk drains once per virtual 10 ms. This
            // exercises playback-rate flow control without claiming wall-clock
            // or acoustic timing from a unit test.
            if tick >= 500
                && tick % 5 == 0
                && let Some((samples, final_chunk)) = reply.next_speaker_chunk()
            {
                played.extend(samples);
                final_chunks += usize::from(final_chunk);
            }
            receive_flow
                .replenish(reply.pcm.len(), &mut parent.control)
                .unwrap();
            if reply.provider_idle && reply.pcm.is_empty() {
                break;
            }
        }
        assert_eq!(received, packets);
        assert!(source.is_empty());
        assert!(high_water >= MAX_REPLY_SAMPLES - PACKET_SAMPLES);
        assert!(credit_waits > 1_000);
        assert_eq!(final_chunks, 1);
        assert_eq!(&played[..original.len()], original);
        assert!(played[original.len()..].iter().all(|sample| *sample == 0));
        assert_eq!(played.len(), original.len().div_ceil(240) * 240);
    }

    #[test]
    fn provider_bursts_yield_to_real_speaker_channel_without_losing_or_reordering_pcm() {
        // These are the two observed burst sizes and the PCM already waiting
        // before each burst, not claimed hardware timing in this host test.
        for (burst_packets, existing_frames) in [(13, 3_840), (18, 43_920)] {
            let dir = SessionDirectory::create().unwrap();
            let (mut provider_parent, mut provider_child) = pair(&dir, "provider");
            let (mut speaker_parent, mut speaker_child) = pair(&dir, "speaker");
            let expected: Vec<i16> = (0..existing_frames + burst_packets * 960)
                .map(|index| (index % 32_767) as i16)
                .collect();
            let mut packets: VecDeque<_> = (0..burst_packets)
                .map(|index| {
                    provider_audio(
                        index as u64 + 1,
                        expected
                            [existing_frames + index * 960..existing_frames + (index + 1) * 960]
                            .to_vec(),
                    )
                })
                .collect();
            let (mut owner, mut current, _) = active_reply();
            current.pcm = expected[..existing_frames].to_vec().into();
            let mut reply = Some(current);
            let mut snapshot = owner.snapshot(now()).unwrap();
            let mut pending = None;
            let mut speaker_sequence = 0;
            let mut received = 0;
            let mut handed_off = Vec::new();
            // Kernel queue capacity differs by OS. Backpressure leaves each
            // unsent packet at the source instead of enlarging a socket/queue.
            for slice_index in 0..burst_packets {
                for _ in 0..burst_packets {
                    let Some(packet) = packets.front() else {
                        break;
                    };
                    match provider_child.data.send(packet) {
                        Ok(()) => {
                            packets.pop_front();
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                        Err(error) => panic!("provider fixture send: {error}"),
                    }
                }
                let mut slice = ProviderSlice::new(0);
                let before = received;
                // 750 us per handled packet models the observed processing
                // cost deterministically; no sleep or CI latency assertion.
                while let Some(event) = slice
                    .receive(&mut provider_parent.data, (received - before) * 750)
                    .unwrap()
                {
                    let ProviderOutput::Audio {
                        sequence, samples, ..
                    } = event
                    else {
                        panic!("unexpected provider fixture event");
                    };
                    received += 1;
                    assert_eq!(sequence, received);
                    reply.as_mut().unwrap().pcm.extend(samples);
                }
                assert!(received - before <= 2);
                if slice_index == 0 {
                    assert_eq!(received, 2);
                    assert!(received < burst_packets as u64);
                }
                dispatch_speaker_chunk(
                    &mut owner,
                    &mut reply,
                    &mut speaker_parent.data,
                    &mut pending,
                    &mut speaker_sequence,
                    &mut snapshot,
                    &mut PlaybackMask::new(TurnTaking::VadOnly),
                )
                .unwrap();
                let chunk = speaker_child
                    .data
                    .receive::<SpeakerChunk>()
                    .unwrap()
                    .unwrap();
                assert_eq!(chunk.chunk_sequence, speaker_sequence);
                assert!(!chunk.end_of_speech);
                handed_off.extend(chunk.samples);
                let retained = reply.as_ref().unwrap().pcm.len();
                dispatch_speaker_chunk(
                    &mut owner,
                    &mut reply,
                    &mut speaker_parent.data,
                    &mut pending,
                    &mut speaker_sequence,
                    &mut snapshot,
                    &mut PlaybackMask::new(TurnTaking::VadOnly),
                )
                .unwrap();
                assert_eq!(reply.as_ref().unwrap().pcm.len(), retained);
                assert!(
                    speaker_child
                        .data
                        .receive::<SpeakerChunk>()
                        .unwrap()
                        .is_none()
                );
                // Model successful acceptance of this one outstanding chunk;
                // the next slice is permitted to dispatch its successor.
                pending = None;
                if received == burst_packets as u64 {
                    break;
                }
            }
            assert!(packets.is_empty());
            assert_eq!(received, burst_packets as u64);
            handed_off.extend(reply.unwrap().pcm);
            assert_eq!(handed_off, expected);
            assert!(
                provider_parent
                    .data
                    .receive::<ProviderOutput>()
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn exhausted_provider_time_slice_leaves_next_real_packet_for_the_next_tick() {
        let dir = SessionDirectory::create().unwrap();
        let (mut parent, mut child) = pair(&dir, "provider");
        child.data.send(provider_audio(1, vec![11; 960])).unwrap();
        child.data.send(provider_audio(2, vec![22; 960])).unwrap();
        let mut slice = ProviderSlice::new(10_000);
        assert!(slice.receive(&mut parent.data, 10_000).unwrap().is_some());
        assert!(slice.receive(&mut parent.data, 12_000).unwrap().is_none());
        let mut next_tick = ProviderSlice::new(12_000);
        let Some(ProviderOutput::Audio {
            sequence, samples, ..
        }) = next_tick.receive(&mut parent.data, 12_000).unwrap()
        else {
            panic!("budget consumed the next packet instead of yielding");
        };
        assert_eq!(sequence, 2);
        assert_eq!(samples, vec![22; 960]);
        assert!(next_tick.receive(&mut parent.data, 11_999).is_err());
    }

    #[test]
    fn interruption_after_provider_slice_revokes_old_pcm_before_speaker_dispatch() {
        let dir = SessionDirectory::create().unwrap();
        let (mut provider_parent, mut provider_child) = pair(&dir, "provider");
        let (mut speaker_parent, mut speaker_child) = pair(&dir, "speaker");
        for sequence in 1..=3 {
            provider_child
                .data
                .send(provider_audio(sequence, vec![sequence as i16; 960]))
                .unwrap();
        }
        let (mut owner, current, old_permit) = active_reply();
        let mut reply = Some(current);
        let mut slice = ProviderSlice::new(0);
        while let Some(event) = slice.receive(&mut provider_parent.data, 0).unwrap() {
            let ProviderOutput::Audio { samples, .. } = event else {
                panic!("unexpected fixture event");
            };
            reply.as_mut().unwrap().pcm.extend(samples);
        }
        // The production loop handles retained input after this slice and before
        // dispatch. Cancelling here must not dispatch even the preexisting PCM.
        cancel_reply(&mut owner, &mut reply, &mut Vec::new(), "user_interrupted").unwrap();
        let mut pending = None;
        let mut sequence = 0;
        let mut snapshot = owner.snapshot(now()).unwrap();
        dispatch_speaker_chunk(
            &mut owner,
            &mut reply,
            &mut speaker_parent.data,
            &mut pending,
            &mut sequence,
            &mut snapshot,
            &mut PlaybackMask::new(TurnTaking::VadOnly),
        )
        .unwrap();
        assert!(
            speaker_child
                .data
                .receive::<SpeakerChunk>()
                .unwrap()
                .is_none()
        );
        assert!(pending.is_none());
        assert_eq!(sequence, 0);
        let mut guard = BoundaryGuard::new(snapshot.boot(), now());
        guard.install(now(), snapshot).unwrap();
        assert!(guard.check(now(), old_permit, OutputKind::Speech).is_err());
        assert!(matches!(
            provider_parent.data.receive::<ProviderOutput>().unwrap(),
            Some(ProviderOutput::Audio { sequence: 3, .. })
        ));
    }

    #[test]
    fn provider_interruption_revokes_delivered_permit_and_discards_queued_speech() {
        let (mut owner, current, permit) = active_reply();
        let before = owner.snapshot(now()).unwrap();
        let mut guard = BoundaryGuard::new(before.boot(), now());
        guard.install(now(), before).unwrap();
        assert!(guard.check(now(), permit, OutputKind::Speech).is_ok());
        let mut reply = Some(current);
        let mut trace = Vec::new();
        cancel_reply(&mut owner, &mut reply, &mut trace, "provider_interrupted").unwrap();
        let after = owner.snapshot(now()).unwrap();
        guard.install(now(), after).unwrap();
        assert!(guard.check(now(), permit, OutputKind::Speech).is_err());
        assert!(reply.is_none());
        assert_eq!(trace[0]["outcome"], "provider_interrupted");
        assert!(after.owner().is_none());
    }

    #[test]
    fn a_100_ms_provider_burst_exposes_its_final_holdback_without_losing_pcm() {
        let (_, mut current, _) = active_reply();
        let original: Vec<i16> = (0..2400).map(|i| i as i16).collect();
        current.pcm = original.clone().into();
        let mut delivered = Vec::new();
        for _ in 0..9 {
            let (samples, final_chunk) = current.next_speaker_chunk().unwrap();
            assert!(!final_chunk);
            delivered.extend(samples);
        }
        assert!(current.next_speaker_chunk().is_none());
        assert_eq!(current.pcm.len(), 240);
        assert_eq!(current.held_final_samples(), 240);
        assert_eq!(&delivered, &original[..2160]);
        current.generation_done = true;
        assert_eq!(current.held_final_samples(), 0);
        let (samples, final_chunk) = current.next_speaker_chunk().unwrap();
        assert!(final_chunk);
        delivered.extend(samples);
        assert_eq!(delivered, original);
        assert!(current.next_speaker_chunk().is_none());
    }

    #[test]
    fn empty_completion_is_not_fabricated_and_all_zero_provider_pcm_is_preserved() {
        let (_, mut current, _) = active_reply();
        current.pcm.clear();
        current.had_audio = false;
        current.generation_done = true;
        assert!(current.next_speaker_chunk().is_none());
        assert_eq!(current.delivery_outcome(), "no_audio_answer");
        current.pcm = vec![0; 240].into();
        current.had_audio = true;
        let (samples, final_chunk) = current.next_speaker_chunk().unwrap();
        assert_eq!(samples, vec![0; 240]);
        assert!(
            final_chunk,
            "zero-valued provider speech still has a final owned cursor"
        );
        assert!(current.next_speaker_chunk().is_none());
        // This internal outcome explicitly remains unscored; zeros do not prove
        // a substantive audible answer. The room recording decides that metric.
        assert_eq!(current.delivery_outcome(), "audio_written_unscored");
        current.pcm = vec![9; 73].into();
        let (samples, final_chunk) = current.next_speaker_chunk().unwrap();
        assert!(final_chunk);
        assert_eq!(&samples[..73], &[9; 73]);
        assert!(samples[73..].iter().all(|sample| *sample == 0));
    }

    #[test]
    fn accepted_gap_latches_discontinuous_delivery_and_old_gap_cannot_taint_new_reply() {
        let (_, mut current, _) = active_reply();
        current.pcm = vec![7; 240].into();
        let affected = current.owner;
        let mut reply = Some(current);
        let mut trace = Vec::new();
        let gap = crate::wire::PlaybackGap {
            owner: affected,
            first_sample: lamp_audio::ownership::PlaybackCursor {
                epoch: 1,
                frame: 2640,
            },
            end_sample: lamp_audio::ownership::PlaybackCursor {
                epoch: 1,
                frame: 2880,
            },
            started_at_us: 11,
            last_zero_accepted_at_us: 11,
            zero_frames: 240,
        };
        let event = WorkerEvent::PlaybackGap {
            phase: PlaybackGapPhase::Started,
            gap,
            observed_at_us: 11,
        };
        let bytes = crate::wire::encode(&event).unwrap();
        assert!(bytes.len() < 1024);
        handle_playback_gap(
            &mut reply,
            None,
            &mut trace,
            crate::wire::decode(&bytes).unwrap(),
        )
        .unwrap();
        let current = reply.as_ref().unwrap();
        assert_eq!(
            current.pcm,
            vec![7; 240],
            "gap accounting cannot consume owed speech"
        );
        assert_eq!(current.playback_gaps, 1);
        assert_eq!(
            current.delivery_outcome(),
            "audio_written_with_playback_gaps_unscored"
        );
        assert_eq!(trace[0]["held_final_frames_at_observation"], 240);
        let (_, next, _) = active_reply();
        assert_ne!(next.owner, affected);
        reply = Some(next);
        handle_playback_gap(
            &mut reply,
            None,
            &mut trace,
            WorkerEvent::PlaybackGap {
                phase: PlaybackGapPhase::Cancelled,
                gap,
                observed_at_us: 12,
            },
        )
        .unwrap();
        assert_eq!(reply.as_ref().unwrap().playback_gaps, 0);
        assert_eq!(
            reply.as_ref().unwrap().delivery_outcome(),
            "audio_written_unscored"
        );
        assert!(trace[1]["reply_queued_frames_at_observation"].is_null());
    }

    #[test]
    fn session_deadline_records_unfinished_reply_once() {
        let (mut owner, current, _) = active_reply();
        let mut reply = Some(current);
        let mut trace = Vec::new();
        cancel_reply(&mut owner, &mut reply, &mut trace, "deadline_cancelled").unwrap();
        cancel_reply(&mut owner, &mut reply, &mut trace, "runtime_failed").unwrap();
        assert!(reply.is_none());
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0]["kind"], "turn_finished");
        assert_eq!(trace[0]["outcome"], "deadline_cancelled");
    }

    fn discarded(
        affected: TurnOwner,
        reason: PlaybackDiscardReason,
        other_owners: bool,
    ) -> WorkerEvent {
        WorkerEvent::PlaybackDiscarded {
            owner: affected,
            reason,
            queued_frames: 240,
            other_owners,
            observed_at_us: monotonic_us(),
        }
    }

    fn cancelled_tail_fixture() -> (Controller, Option<Reply>, Vec<Value>, WorkerEvent, u64) {
        let (mut controller, old, _) = active_reply();
        let affected = old.owner;
        let mut reply = Some(old);
        let mut trace = Vec::new();
        cancel_reply(&mut controller, &mut reply, &mut trace, "user_interrupted").unwrap();
        let successor = controller
            .admit(now(), AdmittedInput::Interruption)
            .unwrap();
        trace.push(
            json!({"kind":"input_admitted","owner":successor,"at_us":monotonic_us(),
            "authority_issued_at_us":controller.snapshot(now()).unwrap().issued_at().as_micros()}),
        );
        controller.input_ended(now(), successor).unwrap();
        let plan = controller
            .plan_output(now(), successor, OutputKind::Speech, None)
            .unwrap();
        let permit = controller.issue_output(now(), plan, 100_000).unwrap();
        let playback = controller.playback_started(now(), permit).unwrap();
        reply = Some(Reply {
            owner: successor,
            cloud: submitted_request(),
            local_input_ended: true,
            plan,
            playback: Some(playback),
            pcm: vec![456; 240].into(),
            generation_done: false,
            provider_idle: false,
            input_sequence: 1,
            had_audio: true,
            final_chunk: None,
            playback_gaps: 0,
            last_provider_audio_at_us: None,
            start: StartGate::new(0),
            need: StartNeed::default(),
        });
        let started = monotonic_us();
        let observed = started + 20_000;
        let event = WorkerEvent::CancelledTailRetired {
            owner: affected,
            superseded_by: successor,
            first_sample: lamp_audio::ownership::PlaybackCursor {
                epoch: 1,
                frame: 156_024,
            },
            end_sample: lamp_audio::ownership::PlaybackCursor {
                epoch: 1,
                frame: 156_480,
            },
            queued_frames: 456,
            speech_frames: 288,
            started_at_us: started,
            observed_at_us: observed,
        };
        (controller, reply, trace, event, observed)
    }

    #[test]
    fn cancelled_tail_receipt_preserves_successor_and_original_cancelled_outcome() {
        let (mut controller, reply, mut trace, event, observed) = cancelled_tail_fixture();
        let previous_owner = controller.snapshot(now()).unwrap().owner();
        let previous_playback = reply.as_ref().unwrap().playback;
        let bytes = crate::wire::encode(&event).unwrap();
        assert!(bytes.len() < 1024);
        handle_cancelled_tail_retirement(
            &reply,
            &mut trace,
            crate::wire::decode(&bytes).unwrap(),
            observed,
        )
        .unwrap();
        assert_eq!(controller.snapshot(now()).unwrap().owner(), previous_owner);
        let current = reply.as_ref().unwrap();
        assert_eq!(current.playback, previous_playback);
        assert_eq!(current.pcm, vec![456; 240]);
        assert!(!current.generation_done);
        assert_eq!(trace[0]["outcome"], "user_interrupted");
        assert_eq!(trace[2]["kind"], "cancelled_tail_retired");
        assert_eq!(trace[2]["outcome"], "cancelled");
        // A duplicate cannot become a second delivery receipt.
        assert!(handle_cancelled_tail_retirement(&reply, &mut trace, event, observed).is_err());
        assert_eq!(trace.len(), 3);
    }

    #[test]
    fn late_cancelled_tail_does_not_require_the_successor_to_still_be_current() {
        let (mut controller, mut reply, mut trace, event, observed) = cancelled_tail_fixture();
        cancel_reply(&mut controller, &mut reply, &mut trace, "user_interrupted").unwrap();
        let third = controller
            .admit(now(), AdmittedInput::Interruption)
            .unwrap();
        handle_cancelled_tail_retirement(&reply, &mut trace, event, observed).unwrap();
        assert_eq!(controller.snapshot(now()).unwrap().owner(), Some(third));
        assert!(reply.is_none());
        assert_eq!(
            trace
                .iter()
                .filter(|entry| entry["kind"] == "turn_finished")
                .count(),
            2
        );
        assert_eq!(trace.last().unwrap()["outcome"], "cancelled");
    }

    #[test]
    fn cancelled_tail_rejects_invalid_ranges_deadlines_and_owner_identity() {
        let (_, reply, trace, event, observed) = cancelled_tail_fixture();
        let original = serde_json::to_value(event).unwrap();
        let started = original["started_at_us"].as_u64().unwrap();
        let mut invalid = Vec::new();
        for (field, value) in [
            ("queued_frames", json!(961)),
            ("speech_frames", json!(0)),
            ("speech_frames", json!(457)),
            ("started_at_us", json!(0)),
            ("started_at_us", json!(observed + 1)),
            (
                "observed_at_us",
                json!(started + lamp_audio::ownership::MAX_CANCELLED_TAIL_US),
            ),
            ("observed_at_us", json!(observed + 1)),
            ("superseded_by", original["owner"].clone()),
        ] {
            let mut changed = original.clone();
            changed[field] = value;
            invalid.push(changed);
        }
        let mut epoch = original.clone();
        epoch["end_sample"]["epoch"] = json!(2);
        invalid.push(epoch);
        let mut other_boot = original.clone();
        let (_, unrelated, _) = active_reply();
        other_boot["superseded_by"] = json!(unrelated.owner);
        invalid.push(other_boot);
        for value in invalid {
            let mut trace_copy = trace.clone();
            let event = serde_json::from_value(value).unwrap();
            assert!(
                handle_cancelled_tail_retirement(&reply, &mut trace_copy, event, observed).is_err()
            );
            assert_eq!(trace_copy, trace);
            assert_eq!(reply.as_ref().unwrap().pcm, vec![456; 240]);
        }
    }

    #[test]
    fn cancelled_tail_requires_actual_cancellation_and_following_admission() {
        let (_, reply, trace, event, observed) = cancelled_tail_fixture();
        let started = serde_json::to_value(&event).unwrap()["started_at_us"]
            .as_u64()
            .unwrap();
        let event = crate::wire::encode(&event).unwrap();
        let mut future_admission = trace.clone();
        future_admission[1]["authority_issued_at_us"] = json!(started + 1);
        let mut early_admission = trace.clone();
        early_admission[1]["authority_issued_at_us"] =
            json!(trace[0]["at_us"].as_u64().unwrap() - 1);
        let mut missing_time = trace.clone();
        missing_time[1]
            .as_object_mut()
            .unwrap()
            .remove("authority_issued_at_us");
        let mut missing_admission = trace.clone();
        missing_admission.pop();
        let mut wrong_reason = trace.clone();
        wrong_reason[0]["outcome"] = json!("audio_written_unscored");
        let mut wrong_order = trace.clone();
        wrong_order.swap(0, 1);
        let mut wrong_owner = trace.clone();
        let (_, unrelated, _) = active_reply();
        wrong_owner[1]["owner"] = json!(unrelated.owner);
        for mut incomplete in [
            missing_admission,
            wrong_reason,
            wrong_order,
            wrong_owner,
            future_admission,
            early_admission,
            missing_time,
        ] {
            let before = incomplete.clone();
            assert!(
                handle_cancelled_tail_retirement(
                    &reply,
                    &mut incomplete,
                    crate::wire::decode(&event).unwrap(),
                    observed
                )
                .is_err()
            );
            assert_eq!(incomplete, before);
        }
    }

    #[test]
    fn expired_current_playback_is_failed_delivery_not_a_completed_answer() {
        let (mut owner, current, _) = active_reply();
        let affected = current.owner;
        let mut reply = Some(current);
        let mut trace = Vec::new();
        let event = discarded(affected, PlaybackDiscardReason::ExpiredPermit, false);
        // Exercise the actual typed process envelope; no display-string parsing.
        let bytes = crate::wire::encode(&event).unwrap();
        assert!(bytes.len() < 1024);
        let event = crate::wire::decode(&bytes).unwrap();
        assert!(handle_playback_discard(&mut owner, &mut reply, &mut trace, event).is_err());
        assert!(reply.is_none());
        assert!(owner.snapshot(now()).unwrap().owner().is_none());
        assert_eq!(trace[0]["reason"], "expired_permit");
        assert_eq!(trace[0]["expected_cancellation"], false);
        assert_eq!(trace[1]["outcome"], "playback_discarded");
        // The enclosing failure path cannot append a second or successful result.
        cancel_reply(&mut owner, &mut reply, &mut trace, "runtime_failed").unwrap();
        assert_eq!(trace.len(), 2);
    }

    #[test]
    fn cancelled_old_owner_discard_preserves_the_new_answer_and_permission() {
        let (mut owner, old, _) = active_reply();
        let affected = old.owner;
        let mut reply = Some(old);
        let mut trace = Vec::new();
        cancel_reply(&mut owner, &mut reply, &mut trace, "user_interrupted").unwrap();
        let turn = owner.admit(now(), AdmittedInput::Interruption).unwrap();
        owner.input_ended(now(), turn).unwrap();
        let plan = owner
            .plan_output(now(), turn, OutputKind::Speech, None)
            .unwrap();
        let permit = owner.issue_output(now(), plan, 100_000).unwrap();
        let playback = owner.playback_started(now(), permit).unwrap();
        reply = Some(Reply {
            owner: turn,
            cloud: submitted_request(),
            local_input_ended: true,
            plan,
            playback: Some(playback),
            pcm: vec![456; 240].into(),
            generation_done: false,
            provider_idle: false,
            input_sequence: 1,
            had_audio: true,
            final_chunk: None,
            playback_gaps: 0,
            last_provider_audio_at_us: None,
            start: StartGate::new(0),
            need: StartNeed::default(),
        });
        handle_playback_discard(
            &mut owner,
            &mut reply,
            &mut trace,
            discarded(affected, PlaybackDiscardReason::StaleOwner, false),
        )
        .unwrap();
        let current = reply.as_ref().unwrap();
        assert_eq!(current.owner, turn);
        assert_eq!(current.pcm, vec![456; 240]);
        assert_eq!(current.playback, Some(playback));
        assert_eq!(trace[1]["expected_cancellation"], true);
        let state = owner.snapshot(now()).unwrap();
        let mut guard = BoundaryGuard::new(state.boot(), now());
        guard.install(now(), state).unwrap();
        guard.check(now(), permit, OutputKind::Speech).unwrap();

        // An expiry cannot borrow the earlier cancellation classification, even
        // when the affected owner is older than the current answer.
        assert!(
            handle_playback_discard(
                &mut owner,
                &mut reply,
                &mut trace,
                discarded(affected, PlaybackDiscardReason::ExpiredPermit, false)
            )
            .is_err()
        );
        assert!(reply.is_none());
        assert_eq!(trace.last().unwrap()["outcome"], "playback_discarded");
    }

    #[test]
    fn mixed_or_unrecorded_owner_loss_never_counts_as_expected_cancellation() {
        for mixed in [false, true] {
            let (mut owner, current, _) = active_reply();
            let affected = current.owner;
            let mut reply = Some(current);
            let mut trace = Vec::new();
            if mixed {
                cancel_reply(&mut owner, &mut reply, &mut trace, "provider_interrupted").unwrap();
            }
            assert!(
                handle_playback_discard(
                    &mut owner,
                    &mut reply,
                    &mut trace,
                    discarded(affected, PlaybackDiscardReason::StaleOwner, mixed)
                )
                .is_err()
            );
            assert!(
                trace
                    .iter()
                    .any(|event| event["kind"] == "playback_discarded"
                        && event["expected_cancellation"] == false)
            );
        }
    }

    fn unavailable_link(revision: u64, session: u64) -> ProviderLink {
        ProviderLink {
            revision,
            session,
            observed_at_us: monotonic_us(),
            source_clock: None,
            state: ProviderLinkState::Unavailable {
                reason: crate::provider_worker::Fault::Transport,
            },
        }
    }

    fn current_input() -> (Controller, Option<Reply>) {
        let (mut owner, mut reply, _) = active_reply();
        owner.cancel_turn(now(), reply.owner).unwrap();
        reply.owner = owner.admit(now(), AdmittedInput::NewTurn).unwrap();
        reply.plan = owner
            .plan_output(now(), reply.owner, OutputKind::Speech, None)
            .unwrap();
        reply.cloud = CloudRequest::new(1).unwrap();
        reply.local_input_ended = false;
        reply.playback = None;
        reply.pcm.clear();
        reply.had_audio = false;
        (owner, Some(reply))
    }

    fn start_command(reply: &Option<Reply>) -> ProviderInput {
        ProviderInput::Start {
            session: reply.as_ref().unwrap().cloud.session,
            request: reply.as_ref().unwrap().owner.turn(),
            privacy_generation: 1,
        }
    }

    fn end_command(reply: &Option<Reply>) -> ProviderInput {
        ProviderInput::End {
            request: reply.as_ref().unwrap().owner.turn(),
            privacy_generation: 1,
        }
    }

    fn ended_output() -> ProviderOutput {
        ProviderOutput::Ended {
            reason: crate::provider_link::LinkFailure::NoResumableContext,
            fault: crate::provider_worker::Fault::Transport,
            attempts: 1,
        }
    }

    #[test]
    fn priority_outage_wins_over_queued_data_ready_without_revoking_owned_playback() {
        let dir = SessionDirectory::create().unwrap();
        let (mut parent, mut provider) = pair(&dir, "provider");
        let (mut speaker_parent, mut speaker) = pair(&dir, "speaker");
        let (mut owner, current, _) = active_reply();
        let original_owner = current.owner;
        let mut reply = Some(current);
        let mut cloud = CloudAvailability::default();
        cloud.observe(ready_link(1, 1), monotonic_us()).unwrap();
        provider
            .control
            .send(WorkerEvent::ProviderLink {
                link: unavailable_link(2, 1),
            })
            .unwrap();
        provider
            .data
            .send(ProviderOutput::Ready { setup_us: 999 })
            .unwrap();
        let mut trace = Vec::new();
        assert!(
            service_provider_control(&mut parent.control, &mut cloud, &mut 16, &mut trace).unwrap()
        );
        observe_provider_output(
            &mut cloud,
            &mut reply,
            &parent.data.receive().unwrap().unwrap(),
        )
        .unwrap();
        assert!(!cloud.available());
        let mut admission = InputAdmission::default();
        let mut draining = false;
        let mut queue = VecDeque::new();
        apply_submission_gate(
            &cloud,
            &mut reply,
            &mut queue,
            &mut admission,
            &mut draining,
            &mut trace,
        )
        .unwrap();
        assert!(!draining, "submitted End needs no utterance drain");
        let mut state = owner.snapshot(now()).unwrap();
        assert!(state.listening_ready(now()));
        let mut pending = None;
        dispatch_speaker_chunk(
            &mut owner,
            &mut reply,
            &mut speaker_parent.data,
            &mut pending,
            &mut 0,
            &mut state,
            &mut PlaybackMask::new(TurnTaking::VadOnly),
        )
        .unwrap();
        let packet = speaker.data.receive::<SpeakerChunk>().unwrap().unwrap();
        assert_eq!(packet.samples, vec![123; 240]);
        assert_eq!(state.owner(), Some(original_owner));
        assert_eq!(reply.as_ref().unwrap().pcm.len(), 240);
        assert_eq!(trace[0]["kind"], "provider_unavailable");
    }

    #[test]
    fn crossed_session_start_cannot_authorize_current_audio_or_a_speaker_write() {
        let dir = SessionDirectory::create().unwrap();
        let (mut parent, mut speaker) = pair(&dir, "speaker");
        let (mut owner, mut reply) = current_input();
        let command = start_command(&reply);
        reply.as_mut().unwrap().cloud.submitted(&command).unwrap();
        let request = reply.as_ref().unwrap().owner.turn();
        let mut cloud = CloudAvailability::default();
        cloud.observe(ready_link(3, 2), monotonic_us()).unwrap();
        assert!(
            observe_provider_output(
                &mut cloud,
                &mut reply,
                &ProviderOutput::Started {
                    request,
                    session: 2,
                    waiting_for_barrier: false
                }
            )
            .is_err()
        );
        let mut event = provider_audio(1, vec![8; 480]);
        if let ProviderOutput::Audio { request: id, .. } = &mut event {
            *id = request;
        }
        assert!(observe_provider_output(&mut cloud, &mut reply, &event).is_err());
        // Even an accidentally populated PCM queue cannot bypass source proof.
        reply.as_mut().unwrap().pcm = vec![8; 480].into();
        reply.as_mut().unwrap().local_input_ended = true;
        owner
            .input_ended(now(), reply.as_ref().unwrap().owner)
            .unwrap();
        let mut state = owner.snapshot(now()).unwrap();
        dispatch_speaker_chunk(
            &mut owner,
            &mut reply,
            &mut parent.data,
            &mut None,
            &mut 0,
            &mut state,
            &mut PlaybackMask::new(TurnTaking::VadOnly),
        )
        .unwrap();
        assert!(speaker.data.receive::<SpeakerChunk>().unwrap().is_none());
        assert_eq!(reply.as_ref().unwrap().pcm.len(), 480);
        assert!(!reply.as_ref().unwrap().cloud.started);
    }

    #[test]
    fn would_block_end_is_not_submitted_and_recovery_never_replays_queued_tail() {
        let (_owner, mut reply) = current_input();
        let request = reply.as_ref().unwrap().owner.turn();
        let mut cloud = CloudAvailability::default();
        cloud.observe(ready_link(1, 1), monotonic_us()).unwrap();
        let mut queue = VecDeque::from([start_command(&reply), end_command(&reply)]);
        let mut sends = 0;
        flush_provider_input_with(&cloud, &mut reply, &mut queue, |_| {
            sends += 1;
            if sends == 2 {
                Err(io::ErrorKind::WouldBlock.into())
            } else {
                Ok(())
            }
        })
        .unwrap();
        let current = reply.as_ref().unwrap();
        assert!(current.cloud.start_submitted);
        assert!(!current.cloud.input_end_submitted);
        assert_eq!(queue.len(), 1);
        // Skip directly to Ready B: its coalesced outage is sufficient.
        cloud.observe(ready_link(3, 2), monotonic_us()).unwrap();
        let mut admission = InputAdmission::default();
        let mut draining = false;
        apply_submission_gate(
            &cloud,
            &mut reply,
            &mut queue,
            &mut admission,
            &mut draining,
            &mut Vec::new(),
        )
        .unwrap();
        assert!(draining);
        assert!(queue.is_empty());
        assert!(
            reply.as_ref().unwrap().cloud.outcome.is_none(),
            "submitted Start still needs its exact outcome"
        );
        flush_provider_input_with(&cloud, &mut reply, &mut queue, |_| panic!("tail replayed"))
            .unwrap();
        // A's delayed source-stamped Started is legitimate even though B is now
        // available; it only confirms A, and cannot reopen input to B.
        observe_provider_output(
            &mut cloud,
            &mut reply,
            &ProviderOutput::Started {
                request,
                session: 1,
                waiting_for_barrier: false,
            },
        )
        .unwrap();
        observe_provider_output(
            &mut cloud,
            &mut reply,
            &ProviderOutput::TurnFailed {
                request,
                stage: crate::provider_worker::FailedStage::InputOpen,
                reason: crate::provider_worker::Fault::Transport,
            },
        )
        .unwrap();
        assert_eq!(
            reply.as_ref().unwrap().delivery_outcome(),
            "provider_failed"
        );
        assert!(!reply.as_ref().unwrap().cloud.can_submit(&cloud));
    }

    #[test]
    fn local_never_submitted_request_fails_without_fabricating_a_provider_receipt() {
        let (_owner, mut reply) = current_input();
        let mut queue = VecDeque::from([start_command(&reply)]);
        let mut draining = false;
        apply_submission_gate(
            &CloudAvailability::default(),
            &mut reply,
            &mut queue,
            &mut InputAdmission::default(),
            &mut draining,
            &mut Vec::new(),
        )
        .unwrap();
        let current = reply.as_ref().unwrap();
        assert!(matches!(
            current.cloud.outcome,
            Some(RequestOutcome::Failed {
                stage: crate::provider_worker::FailedStage::NotDelivered,
                local_not_submitted: true,
                ..
            })
        ));
        assert!(queue.is_empty());
        assert!(!current.cloud.start_submitted);
        assert!(
            !current.local_input_ended,
            "cloud loss is not an acoustic endpoint"
        );
        assert!(draining);
    }

    #[test]
    fn ordered_failure_preserves_received_pcm_at_zero_new_credit_and_never_succeeds() {
        let dir = SessionDirectory::create().unwrap();
        let (mut parent, mut speaker) = pair(&dir, "speaker");
        let (mut owner, mut current, _) = active_reply();
        current.pcm = vec![44; 240].into();
        let request = current.owner.turn();
        let mut reply = Some(current);
        let mut cloud = CloudAvailability::default();
        cloud
            .observe(unavailable_link(2, 1), monotonic_us())
            .unwrap();
        observe_provider_output(
            &mut cloud,
            &mut reply,
            &ProviderOutput::TurnFailed {
                request,
                stage: crate::provider_worker::FailedStage::Responding,
                reason: crate::provider_worker::Fault::Transport,
            },
        )
        .unwrap();
        observe_provider_output(&mut cloud, &mut reply, &ended_output()).unwrap();
        assert!(cloud.output_ended());
        assert_eq!(reply.as_ref().unwrap().pcm.len(), 240);
        let mut state = owner.snapshot(now()).unwrap();
        let mut pending = None;
        dispatch_speaker_chunk(
            &mut owner,
            &mut reply,
            &mut parent.data,
            &mut pending,
            &mut 0,
            &mut state,
            &mut PlaybackMask::new(TurnTaking::VadOnly),
        )
        .unwrap();
        let sent = speaker.data.receive::<SpeakerChunk>().unwrap().unwrap();
        assert!(sent.end_of_speech);
        assert_eq!(sent.samples, vec![44; 240]);
        assert_eq!(
            reply.as_ref().unwrap().delivery_outcome(),
            "provider_failed"
        );
        assert!(
            reply.as_ref().unwrap().playback.is_some(),
            "Ended is not playback retirement"
        );
        assert!(
            observe_provider_output(&mut cloud, &mut reply, &provider_audio(2, vec![1; 240]))
                .is_err()
        );
    }

    #[test]
    fn ended_waits_for_in_flight_start_refusal_and_does_not_mark_an_open_input_complete() {
        let (_owner, mut reply) = current_input();
        let request = reply.as_ref().unwrap().owner.turn();
        let start = start_command(&reply);
        reply.as_mut().unwrap().cloud.submitted(&start).unwrap();
        let mut cloud = CloudAvailability::default();
        cloud.observe(ready_link(1, 1), monotonic_us()).unwrap();
        observe_provider_output(&mut cloud, &mut reply, &ended_output()).unwrap();
        let mut queue = VecDeque::from([end_command(&reply)]);
        let mut draining = false;
        apply_submission_gate(
            &cloud,
            &mut reply,
            &mut queue,
            &mut InputAdmission::default(),
            &mut draining,
            &mut Vec::new(),
        )
        .unwrap();
        assert!(reply.as_ref().unwrap().cloud.outcome.is_none());
        let refused = ProviderOutput::TurnFailed {
            request,
            stage: crate::provider_worker::FailedStage::NotDelivered,
            reason: crate::provider_worker::Fault::Transport,
        };
        observe_provider_output(&mut cloud, &mut reply, &refused).unwrap();
        assert_eq!(
            reply.as_ref().unwrap().delivery_outcome(),
            "provider_failed"
        );
        assert!(!reply.as_ref().unwrap().local_input_ended);
        assert!(
            observe_provider_output(&mut cloud, &mut reply, &refused).is_err(),
            "duplicate outcome"
        );
        assert!(observe_provider_output(&mut cloud, &mut reply, &ended_output()).is_err());
    }

    #[test]
    fn abandoned_stream_recovery_keeps_original_endpoint_and_only_accepts_fresh_speech() {
        let (mut owner, mut reply) = current_input();
        let request = reply.as_ref().unwrap().owner.turn();
        let start = start_command(&reply);
        reply.as_mut().unwrap().cloud.submitted(&start).unwrap();
        let mut cloud = CloudAvailability::default();
        cloud.observe(ready_link(1, 1), monotonic_us()).unwrap();
        let mut detector = TurnDetector::default();
        let mut admission = InputAdmission::default();
        let capture = CaptureLineage {
            worker: new_boot().unwrap(),
            epoch: 1,
            dsp_epoch: 1,
            privacy_generation: owner.snapshot(now()).unwrap().microphone_generation(),
        };
        let base = monotonic_us() - 1_000;
        let mut first = None;
        let mut draining = false;
        let mut queue = VecDeque::new();
        let mut trace = Vec::new();
        for sequence in 1..=90 {
            let context = AdmissionContext {
                capture,
                authority: owner.snapshot(now()).unwrap(),
            };
            let probability = if sequence <= 20 || sequence > 80 {
                0.99
            } else {
                0.0
            };
            let activity = detector.push(
                ObservedAudio {
                    sequence,
                    captured_at_us: base + sequence,
                    samples: [sequence as i16; 160],
                },
                probability,
            );
            let is_endpoint = matches!(activity, Activity::End(_));
            let update = admission
                .observe(activity, context, monotonic_us())
                .unwrap();
            if sequence == 6 {
                let AdmissionStep::Candidate(candidate) = update.step else {
                    panic!("no opening prefix")
                };
                first = Some(candidate.id);
                assert!(matches!(
                    admission
                        .decide(
                            Evidence {
                                candidate: candidate.id,
                                through_sequence: candidate.trigger_sequence,
                                produced_at_us: monotonic_us(),
                                verdict: Verdict::Accept(AcceptanceBasis::DirectedSessionVadOnly)
                            },
                            context,
                            monotonic_us()
                        )
                        .unwrap(),
                    Decision::Accepted(_)
                ));
                cloud
                    .observe(unavailable_link(2, 1), monotonic_us())
                    .unwrap();
                apply_submission_gate(
                    &cloud,
                    &mut reply,
                    &mut queue,
                    &mut admission,
                    &mut draining,
                    &mut trace,
                )
                .unwrap();
                assert!(draining);
            } else if sequence < 80 {
                assert!(matches!(update.step, AdmissionStep::None));
                if sequence == 10 {
                    cloud.observe(ready_link(3, 2), monotonic_us()).unwrap();
                }
            } else if sequence == 80 {
                assert!(is_endpoint);
                assert!(matches!(update.step, AdmissionStep::None));
                draining = false;
                let current = reply.as_mut().unwrap();
                current.local_input_ended = true;
                owner.input_ended(now(), current.owner).unwrap();
            } else if sequence == 86 {
                let AdmissionStep::Candidate(candidate) = update.step else {
                    panic!("fresh input lost")
                };
                assert_ne!(Some(candidate.id), first);
                let accepted = admission
                    .decide(
                        Evidence {
                            candidate: candidate.id,
                            through_sequence: candidate.trigger_sequence,
                            produced_at_us: monotonic_us(),
                            verdict: Verdict::Accept(AcceptanceBasis::DirectedSessionVadOnly),
                        },
                        context,
                        monotonic_us(),
                    )
                    .unwrap();
                let Decision::Accepted(accepted) = accepted else {
                    panic!("fresh input not accepted")
                };
                assert_eq!(accepted.audio.first().unwrap().sequence, 81);
                break;
            }
        }
        assert!(!draining);
        assert_eq!(cloud.ready_session(), Some(2));
        assert_eq!(reply.as_ref().unwrap().owner.turn(), request);
        assert!(reply.as_ref().unwrap().cloud.input_abandoned);
        assert!(queue.is_empty());
        assert_eq!(trace.len(), 1);
    }

    #[test]
    fn assumed_ownership_and_completion_without_submitted_end_fail_closed() {
        let (_owner, mut reply) = current_input();
        let start = start_command(&reply);
        reply.as_mut().unwrap().cloud.submitted(&start).unwrap();
        let request = reply.as_ref().unwrap().owner.turn();
        let mut cloud = CloudAvailability::default();
        observe_provider_output(
            &mut cloud,
            &mut reply,
            &ProviderOutput::Started {
                request,
                session: 1,
                waiting_for_barrier: false,
            },
        )
        .unwrap();
        assert!(
            observe_provider_output(
                &mut cloud,
                &mut reply,
                &ProviderOutput::OwnershipAssumed {
                    request: Some(request),
                    superseded: None
                }
            )
            .is_err()
        );
        assert!(
            observe_provider_output(
                &mut cloud,
                &mut reply,
                &ProviderOutput::TurnComplete {
                    request,
                    idle: true
                }
            )
            .is_err()
        );
        assert!(reply.as_ref().unwrap().cloud.outcome.is_none());
    }

    #[test]
    fn actual_fixture_worker_enters_the_same_priority_readiness_gate() {
        use sha2::{Digest, Sha256};
        let dir = SessionDirectory::create().unwrap();
        let wav = dir.path.join("reply.wav");
        let mut writer = hound::WavWriter::create(
            &wav,
            hound::WavSpec {
                channels: 1,
                sample_rate: 24_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..240 {
            writer.write_sample(100_i16).unwrap();
        }
        writer.finalize().unwrap();
        let hash = format!("{:x}", Sha256::digest(std::fs::read(&wav).unwrap()));
        let (mut parent, worker) = pair(&dir, "provider");
        let mut startup = StartupEvents::default();
        std::thread::scope(|scope| {
            let handle = scope
                .spawn(|| crate::fixture_provider::run(worker, new_boot().unwrap(), &wav, &hash));
            let until = Instant::now() + Duration::from_secs(1);
            loop {
                if let Some(event) = parent.control.receive::<WorkerEvent>().unwrap() {
                    assert!(matches!(event, WorkerEvent::Ready));
                    break;
                }
                assert!(Instant::now() < until, "fixture process readiness deadline");
                std::thread::yield_now();
            }
            while !startup.cloud.available() {
                startup.poll(&mut parent, "provider").unwrap();
                assert!(
                    Instant::now() < until,
                    "fixture priority availability deadline"
                );
                std::thread::yield_now();
            }
            assert_eq!(startup.cloud.ready_session(), Some(1));
            assert_eq!(startup.events.len(), 1);
            assert_eq!(startup.events[0]["kind"], "provider_ready");
            assert!(
                parent.data.receive::<ProviderOutput>().unwrap().is_none(),
                "no credited Ready may bypass the gate"
            );
            parent.control.send(Control::Stop).unwrap();
            handle.join().unwrap().unwrap();
        });
    }

    fn local_policy_decision(
        owner: &mut Controller,
        admission: &mut InputAdmission,
        verdict: Verdict,
    ) -> Decision {
        let context = AdmissionContext {
            capture: CaptureLineage {
                worker: new_boot().unwrap(),
                epoch: 1,
                dsp_epoch: 1,
                privacy_generation: owner.snapshot(now()).unwrap().microphone_generation(),
            },
            authority: owner.snapshot(now()).unwrap(),
        };
        let at = monotonic_us();
        let update = admission
            .observe(
                Activity::Start(
                    (1..=6)
                        .map(|sequence| ObservedAudio {
                            sequence,
                            captured_at_us: at - 7 + sequence,
                            samples: [123; 160],
                        })
                        .collect(),
                ),
                context,
                at,
            )
            .unwrap();
        let AdmissionStep::Candidate(candidate) = update.step else {
            panic!("no candidate")
        };
        admission
            .decide(
                Evidence {
                    candidate: candidate.id,
                    through_sequence: candidate.trigger_sequence,
                    produced_at_us: monotonic_us(),
                    verdict,
                },
                context,
                monotonic_us(),
            )
            .unwrap()
    }

    #[test]
    fn accepted_local_interruption_during_outage_revokes_old_playback_without_cloud_submission() {
        let (mut owner, current, permit) = active_reply();
        let old = current.owner;
        let mut reply = Some(current);
        let mut admission = InputAdmission::default();
        let decision = local_policy_decision(
            &mut owner,
            &mut admission,
            Verdict::Accept(AcceptanceBasis::DirectedSessionVadOnly),
        );
        let mut trace = Vec::new();
        let accepted = accepted_decision(decision, &mut trace).unwrap().unwrap();
        let (state, draining) =
            accepted_without_cloud(accepted, &mut owner, &mut reply, &mut admission, &mut trace)
                .unwrap();
        assert!(reply.is_none());
        assert!(draining);
        assert!(state.owner().is_none());
        assert!(state.listening_ready(now()));
        let mut guard = BoundaryGuard::new(state.boot(), now());
        guard.install(now(), state).unwrap();
        assert!(guard.check(now(), permit, OutputKind::Speech).is_err());
        assert_eq!(trace[0]["outcome"], "user_interrupted");
        assert_eq!(trace[0]["turn"], old.turn());
        assert_eq!(trace[1]["outcome"]["stage"], "not_delivered");
        assert_eq!(trace[1]["outcome"]["local_not_submitted"], true);
        assert!(trace[1]["provider_request"].is_null());
        assert!(trace[1]["provider_session"].is_null());
        assert!(
            !trace
                .iter()
                .any(|event| event["kind"] == "provider_input_started")
        );
    }

    #[test]
    fn rejected_background_or_ignored_decision_during_outage_preserves_playback() {
        let (mut owner, current, permit) = active_reply();
        let old = current.owner;
        let reply = Some(current);
        let mut admission = InputAdmission::default();
        let decision = local_policy_decision(&mut owner, &mut admission, Verdict::Reject);
        let mut trace = Vec::new();
        assert!(accepted_decision(decision, &mut trace).unwrap().is_none());
        assert!(accepted_decision(Decision::Ignored, &mut trace).is_err());
        let state = owner.snapshot(now()).unwrap();
        assert_eq!(state.owner(), Some(old));
        let mut guard = BoundaryGuard::new(state.boot(), now());
        guard.install(now(), state).unwrap();
        guard.check(now(), permit, OutputKind::Speech).unwrap();
        assert_eq!(reply.as_ref().unwrap().pcm.len(), 480);
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0]["kind"], "input_candidate_rejected");
    }
}

#[cfg(test)]
#[path = "coordinator/ring_trace_tests.rs"]
mod ring_trace_tests;
