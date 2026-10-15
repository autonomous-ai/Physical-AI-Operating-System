//! Finite, explicitly directed voice qualification on real hardware.
//!
//! This integration has optional guarded ring cues, but no addressee inference or automatic motor gestures.
//! Refuse to present its directed-session results as general desk-conversation
//! qualification. Audio-critical failures end the run; optional ring failure disables
//! presentation while retaining the child and truthful cleanup evidence.
#[cfg(test)]
use crate::scenario_clock::monotonic_us;
use crate::{
    activity::{Activity, ObservedAudio, TurnDetector},
    admission::{
        AcceptanceBasis, Accepted, CaptureLineage, Context as AdmissionContext, Decision, Hold,
        InputAdmission, Rejection, Step as AdmissionStep, Update as AdmissionUpdate,
    },
    camera_config::CameraConfig,
    camera_worker::CameraEvent,
    choreography::RingChoreographer,
    diagnostic_control::{self, DiagnosticFinished, DiagnosticStream},
    fixture_provider::{CueSink, Fixture, FixtureInfo, FixtureOptions},
    hand_controls::{ControlsOptions, HandControls},
    options::{
        AudioWorkerOptions, DirectedOptions, NoiseSuppression, PlayoutStart, SessionOptions,
        TurnTaking,
    },
    privacy::{PrivacyGate, PrivacyState},
    process::{SessionDirectory, Worker, boot_text, new_boot},
    provider_flow::{MAX_REPLY_SAMPLES, OutputReceiver, PACKET_SAMPLES},
    provider_link::{ProviderLink, ProviderLinkState},
    provider_worker::{ProviderInput, ProviderOutput},
    ring_wire::{RingBlankReason, RingFeedback},
    service_privacy::{self, PhysicalMute, SoftwareMute},
    session_lifetime::{Lifetime, SessionClock},
    session_state::{SessionState, YieldInterval},
    termination::Termination,
    transport::{Channel, WorkerChannels},
    turn_taking::{Disposition, FRUITLESS_YIELDS, PlaybackMask, RetainedHistoryLoss},
    visual_context::Source as VisualSource,
    wire::{
        Control, MicrophoneFrame, PlaybackDiscardReason, PlaybackGapPhase, SpeakerChunk,
        WorkerEvent,
    },
};
use lamp_audio::{
    duplex::{Confirmation, Hearing, Step as DuplexStep},
    playout::{StartGate, StartNeed},
};
#[cfg(not(test))]
use lamp_ipc::monotonic_us;
use lamp_turn_owner::{
    AdmissionState, AdmittedInput, BootId, CaptureState, Controller, MonoTime, OutputKind,
    OutputOwner, OutputPermit, Permission, PlaybackToken, Snapshot, TurnOwner,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
mod camera;
pub mod motor;
use motor::{OptionalMotor, PreparedMotor};
mod cloud;
mod early_end;
mod endpoint;
mod hand;
mod notices;
mod second;
mod service;
use camera::{OptionalCamera, PreparedCamera, service_camera};
use early_end::{EarlyEnd, Heard as HeardAfterEnd};
use endpoint::EndpointRig;
mod optional_ring;
mod ring_trace;
use cloud::{CloudAvailability, CloudRequest, RequestOutcome};
use notices::{FailureNotices, NoticeDelivery, SpeechOrigin, select_failure_notice};
use optional_ring::OptionalRing;
use ring_trace::RingTrace;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const MAX_DIRECTED_SECONDS: u64 = 600;
const INPUT_READINESS_TIMEOUT: Duration = Duration::from_secs(25);
const DIAGNOSTIC_SHUTDOWN_GRACE: Duration = Duration::from_millis(750);
const INPUT_LEASE_US: u64 = 100_000;

/// Only explicitly scoped unit-test scenarios replace this clock. Child
/// startup/shutdown deadlines continue using Instant::now directly.
#[inline]
fn session_instant() -> Instant {
    #[cfg(test)]
    {
        crate::scenario_clock::instant()
    }
    #[cfg(not(test))]
    {
        Instant::now()
    }
}
/// How long a just-asked lamp keeps the floor from the next voice in a busy
/// room while its answer has not begun. Answers begin 1.0 to 1.5 s after the
/// question ends.
const HOLD_FLOOR_US: u64 = 1_800_000;
/// How long an answer to something asked in a busy room is not stopped for a
/// voice over it. The first time the lamp kept its turn there, on lamp-4ace,
/// it began "It's a beautiful, bright blue" under a colleague's sentence and
/// was stopped by that same sentence 1.2 s later. Most answers are shorter
/// than this; a story can still be stopped once it has passed.
const KEEP_ANSWER_US: u64 = 4_000_000;
#[cfg(test)]
const MAX_TRACE_EVENTS: usize = crate::session_state::MAX_FINITE_EVENTS;
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
    notice: NoticeDelivery,
    reminder: Option<crate::reminders::Receipt>,
    /// How loud the block last handed to the speaker is, for the ring's face.
    voice_level: u8,
}
impl Reply {
    fn settled(&self, speaker_pending: &Option<PendingSpeaker>) -> bool {
        self.provider_idle
            && self.cloud.outcome.is_some()
            && self.local_input_ended
            && self.pcm.is_empty()
            && speaker_pending.is_none()
            && self.playback.is_none()
            && self.notice.settled()
    }

    fn complete_reminder(&mut self, speaker_pending: &Option<PendingSpeaker>, snapshot: Snapshot) {
        if self.settled(speaker_pending)
            && snapshot.owner().is_none()
            && snapshot.microphone_permission() == Permission::Allowed
            && matches!(self.cloud.outcome, Some(RequestOutcome::Complete))
            && self.had_audio
            && self.playback_gaps == 0
            && self.notice.origin() == SpeechOrigin::Provider
            && let Some(reminder) = &mut self.reminder
        {
            reminder.completed();
        }
    }

    fn append_provider_audio(&mut self, samples: Vec<i16>, received_at_us: u64) {
        if !samples.is_empty()
            && let Some(reminder) = &mut self.reminder
        {
            reminder.audio();
        }
        // The gate counts frames to measure the service's lead. A plain gate
        // only records it; a continuity gate starts a new hold with the first
        // audio after a generation's final block left.
        self.start.arrival(received_at_us, samples.len());
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
            "continuity":self.start.continuity().map(|policy| json!({"margin_us":policy.margin_us,"hold_us":policy.hold_us,"cover_us":policy.cover_us})),
            "measured_lead_us":self.start.lead_us(),
            "opened_by":opened.map(|event| match event.by {
                lamp_audio::playout::OpenedBy::Covered => "covered",
                lamp_audio::playout::OpenedBy::Complete => "complete",
                lamp_audio::playout::OpenedBy::Margin => "margin",
                lamp_audio::playout::OpenedBy::HoldBound => "hold_bound",
            }),
            "scope":if self.start.continuity().is_some() {
                "first_dispatchable_chunk_of_each_generation; a later generation is held again once the previous final block left"
            } else {
                "first_dispatchable_chunk_of_reply; later generations do not rebuffer"
            },
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
        if final_chunk {
            self.start.generation_ended();
        }
        let mut samples = Vec::with_capacity(240);
        for _ in 0..240 {
            match self.pcm.pop_front() {
                Some(sample) => samples.push(sample),
                None => break,
            }
        }
        samples.resize(240, 0);
        self.voice_level = crate::ring_face::level_of(&samples);
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
    origin: SpeechOrigin,
    sequence: u64,
    permit: OutputPermit,
    accepted: usize,
    final_chunk: bool,
    /// See [`check_speaker_progress`].
    report_due_us: u64,
}

/// The output lease requested for each speaker chunk. Its permit may expire
/// sooner (it never outlives the input lease), never later.
const SPEAKER_PERMIT_LEASE_US: u64 = 100_000;
/// Every worker channel refuses a packet older than this (the age bound in
/// `transport.rs`), so a speaker's report reaches the coordinator within it of
/// being produced.
const SPEAKER_REPORT_AGE_US: u64 = 100_000;

/// A chunk handed to the speaker is settled (written in full, refused or
/// discarded) within its permit's lease, after which the speaker may no
/// longer write it, and the speaker's report then arrives within the channel
/// packet age. A speaker that stays connected but has reported nothing that
/// settles the chunk by then has stopped making progress. This holds under
/// every turn-taking policy; it is checked after each capture block, once
/// that block's policy checks have run, so a stricter policy bound keeps
/// reporting first.
fn check_speaker_progress(pending: Option<&PendingSpeaker>, now_us: u64) -> io::Result<()> {
    match pending {
        Some(pending) if now_us > pending.report_due_us => {
            Err(io::Error::other("speaker acknowledgment expired"))
        }
        _ => Ok(()),
    }
}
struct Rig {
    provider: Worker,
    capture: Worker,
    speaker: Worker,
    privacy: Worker,
    ring: OptionalRing,
    camera: Option<OptionalCamera>,
    motor: Option<OptionalMotor>,
    ring_channel_ceiling: Option<u16>,
    diagnostic_directory: Option<PathBuf>,
    noise_suppression: NoiseSuppression,
    turn_taking: TurnTaking,
    end_silence_blocks: Option<u16>,
    /// The end sent early, after this many blocks of quiet (`--early-end-ms`).
    early_end_blocks: Option<u16>,
    faint_level: Option<u8>,
    playout_start: PlayoutStart,
    /// Which microphone the capture worker reads.
    microphone: crate::options::Microphone,
    /// Opt-in end-of-turn worker client; its process is not a `Rig` child.
    endpoint: Option<EndpointRig>,
    /// Whether the keeper has paused the lamp ([`keeper_pause`]).
    paused: KeeperPause,
    /// The standby voice route, when one is configured (`second.rs`).
    second: Option<second::Route>,
}

/// Whether the keeper has paused the lamp, asked once a loop.
type KeeperPause = Box<dyn FnMut() -> bool>;

/// The voice's one look at `lamp/pause` (`docs/keeper.md`, "Named states
/// and the pause"), read as the body reads it: from the kept value, at most
/// once a second, never waiting longer than a file read. Nothing kept, or a
/// bus it cannot reach, is not paused. Paused, the voice takes no new turn
/// and begins none itself; a turn under way is finished.
fn keeper_pause(bus: io::Result<lamp_bus::Bus>) -> KeeperPause {
    let mut pause = bus.ok().map(lamp_body::pause_in::PauseIn::on);
    Box::new(move || pause.as_mut().is_some_and(|pause| pause.paused()))
}

/// The provider session a turn nobody asked for may begin on: the hello,
/// the news, a reminder, the look the lamp said it would take, and the
/// second look. None while the keeper has paused the lamp; otherwise
/// exactly the ready session.
fn unasked_session(cloud: &CloudAvailability, paused: bool) -> Option<u64> {
    cloud.ready_session().filter(|_| !paused)
}

impl Rig {
    /// A hand cancellation has already reached the speaker. Deliver revocation
    /// to every remaining child even if one local channel has gone away.
    fn publish_hand_revocation(&mut self, snapshot: Snapshot) -> io::Result<()> {
        let mut failure = None;
        for worker in [&mut self.capture, &mut self.privacy, &mut self.provider] {
            if let Err(error) = worker
                .channels
                .control
                .send(Control::Authority { snapshot })
            {
                failure.get_or_insert(error);
            }
        }
        if let Some(Err(error)) = self.second.as_mut().map(|route| route.publish(snapshot)) {
            failure.get_or_insert(error);
        }
        if let Some(motor) = &mut self.motor {
            motor.publish(snapshot);
        }
        self.ring.publish(snapshot);
        if let Some(camera) = &mut self.camera {
            let _ = camera
                .worker
                .channels
                .control
                .send(Control::Authority { snapshot });
        }
        failure.map_or(Ok(()), Err)
    }

    fn publish(&mut self, snapshot: Snapshot) -> io::Result<()> {
        // Motor revocation precedes mandatory audio sends, including their failure.
        if let Some(motor) = &mut self.motor {
            motor.publish(snapshot);
        }
        for worker in [
            &mut self.speaker,
            &mut self.capture,
            &mut self.privacy,
            &mut self.provider,
        ] {
            worker
                .channels
                .control
                .send(Control::Authority { snapshot })?;
        }
        if let Some(route) = &mut self.second {
            route.publish(snapshot)?;
        }
        self.ring.publish(snapshot);
        if let Some(camera) = self.camera.as_mut()
            && !camera.failed
            && camera
                .worker
                .channels
                .control
                .send(Control::Authority { snapshot })
                .is_err()
        {
            camera.pending_failure = Some("camera authority transport failed");
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
        .chain(self.second.as_mut().map(second::Route::worker_mut))
        {
            worker.check_running()?;
        }
        self.ring.check_running();
        Ok(())
    }
    fn shutdown(&mut self, trace: &mut SessionState) -> io::Result<()> {
        let grace = if self.diagnostic_directory.is_some() {
            DIAGNOSTIC_SHUTDOWN_GRACE
        } else {
            Duration::from_millis(300)
        };
        let deadline = Instant::now() + grace;
        let stop_requested_at_us = monotonic_us();
        if let Some(motor) = &mut self.motor {
            motor.begin_shutdown();
        }
        let mut camera_watch = self.camera.as_mut().map(|camera| {
            camera.begin_shutdown(stop_requested_at_us);
            for event in camera.shutdown_events.iter().flatten() {
                let _ = record(trace, json!({"kind":"camera_shutdown_receipt", "at_us":monotonic_us(), "details":event,
                    "boundary":"local revocation and driver teardown are separate; physical sensor shutdown unmeasured"}));
            }
            camera.shutdown_watch.take().expect("camera shutdown watch")
        });
        // Send urgent stop to every child before waiting on any one child.
        for worker in [
            &mut self.speaker,
            &mut self.capture,
            &mut self.provider,
            &mut self.privacy,
        ]
        .into_iter()
        .chain(self.ring.iter_mut())
        .chain(self.camera.iter_mut().map(|camera| &mut camera.worker))
        .chain(self.second.as_mut().map(second::Route::worker_mut))
        {
            if let Err(error) = worker.channels.control.send(Control::Stop) {
                let _ = record(
                    trace,
                    json!({"kind":"stop_delivery_error", "worker":worker.role,
                    "at_us":monotonic_us(), "error":error.to_string()}),
                );
            }
        }
        let mut failure = self.ring.cleanup_error();
        let ring_stop_requested_at_us = self.ring.stop_requested_at(stop_requested_at_us);
        let mut exited = vec![
            false;
            4 + usize::from(self.ring.is_some())
                + usize::from(self.camera.is_some())
                + usize::from(self.second.is_some())
        ];
        let mut receipts: [Option<DiagnosticFinished>; 2] = [None, None];
        let mut ring_blank_confirmed = self.ring.blank_confirmed();
        let mut final_drain = false;
        loop {
            #[cfg(test)]
            if let Err(error) =
                crate::scenario_clock::step(crate::scenario_clock::Phase::Shutdown { deadline })
            {
                // Fixture failure never bypasses the real supervisor's cleanup.
                failure = Some(error);
            }
            if let Some(motor) = &mut self.motor {
                motor.poll(|event| { let _ = record(trace, json!({"kind":"motor_shutdown_receipt","at_us":monotonic_us(),"details":event,"physical_stop_verified":false})); });
            }
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
            .chain(self.camera.iter_mut().map(|camera| &mut camera.worker))
            .chain(self.second.as_mut().map(second::Route::worker_mut))
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
                            if worker.role != "ring"
                                || ring_blank_confirmed
                                || !report
                                    .confirms_shutdown(ring_stop_requested_at_us, monotonic_us())
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
                if worker.role == "camera" {
                    for _ in 0..32 {
                        match worker.channels.data.receive::<CameraEvent>() {
                            Ok(Some(event)) => {
                                if let Err(error) = camera_watch
                                    .as_mut()
                                    .expect("camera watch")
                                    .observe(&event, monotonic_us())
                                {
                                    failure = Some(error);
                                }
                                if matches!(
                                    event,
                                    CameraEvent::Revoked { .. } | CameraEvent::Stopped { .. }
                                ) {
                                    let _ = record(
                                        trace,
                                        json!({"kind":"camera_shutdown_receipt", "at_us":monotonic_us(), "details":event}),
                                    );
                                }
                            }
                            Ok(None) => break,
                            Err(error) => {
                                let _ = record(
                                    trace,
                                    json!({"kind":"camera_shutdown_channel_failed", "at_us":monotonic_us(), "error":error.to_string()}),
                                );
                                failure = Some(error);
                                break;
                            }
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
                        Ok(None)
                            if worker.role == "camera"
                                && monotonic_us()
                                    >= camera_watch
                                        .as_ref()
                                        .expect("camera watch")
                                        .deadline_us() =>
                        {
                            exited[index] = true;
                            failure = Some(worker.terminate_and_reap(
                                io::ErrorKind::TimedOut,
                                "camera local-revoke/driver-teardown deadline expired".into(),
                            ));
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
            if exited.iter().all(|done| *done)
                && self.motor.as_ref().is_none_or(OptionalMotor::reaped)
            {
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
                .chain(self.camera.iter_mut().map(|camera| &mut camera.worker))
                .chain(self.second.as_mut().map(second::Route::worker_mut))
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
        if let Some(motor) = &self.motor {
            let cleanup = motor.finish();
            let _ = record(
                trace,
                json!({"kind":"motor_cleanup","at_us":monotonic_us(),"worker":motor.identity().map(|i|i.worker),"started":motor.started(),"reaped":motor.reaped(),"confirmed":cleanup.is_ok(),"error":cleanup.as_ref().err().map(ToString::to_string),"physical_stop_verified":false}),
            );
            if let Err(error) = cleanup {
                failure = Some(error);
            }
        }
        // Mandatory hardware cleanup preceded bounded metadata reads. Native kill/wait
        // has no userspace hard deadline; its result is never hidden as success.
        if !ring_blank_confirmed {
            failure = Some(io::Error::other(
                "ring shutdown has no verified black-write receipt",
            ));
        }
        if let Some(watch) = camera_watch
            && let Err(error) = watch.finish()
        {
            failure = Some(error);
        }
        // The endpoint client closed when the session's detector dropped. Its
        // loss while running only meant the silence fallback, but a worker
        // whose exit is not confirmed must keep this cleanup from succeeding,
        // so no later runtime can start beside it.
        let remaining = deadline.saturating_duration_since(Instant::now());
        if let Some((event, teardown)) = self
            .endpoint
            .take()
            .and_then(|endpoint| endpoint.finish(remaining.max(Duration::from_millis(500))))
        {
            let _ = record(trace, event);
            if let Err(error) = teardown {
                failure = Some(error);
            }
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
        Lifetime::Finite(seconds),
        output,
        options.directed,
        None,
        None,
        None,
        Some(info),
        &ControlsOptions::default(),
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
            camera_config: None,
            motor: None,
            failure_notices: None,
            controls: ControlsOptions::default(),
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
        Lifetime::Finite(seconds),
        output,
        options.directed,
        options.camera_config.as_deref(),
        options.motor.as_ref(),
        options.failure_notices.as_ref(),
        None,
        &options.controls,
        cues,
        &termination,
    )
}

/// Continuous conversation, with bounded best-effort metadata. It does not
/// relax privacy, worker, provider or per-turn permission/watchdog checks.
/// The provider refuses for now: its quota or its rate limit. The run ended
/// in order like any other terminal end.
#[derive(Debug)]
pub struct ProviderRefusesForNow;
impl std::fmt::Display for ProviderRefusesForNow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "terminal provider output settled; the provider refuses for now (quota or rate limit); service stopped without retry",
        )
    }
}
impl std::error::Error for ProviderRefusesForNow {}

/// How long the service stays before it leaves after such a refusal. Whoever
/// restarts it would ask the provider again at once: on lamp-4ace on
/// 2026-10-13 systemd did, every five seconds, about 380 times in half an
/// hour of a spent quota.
const REFUSAL_WAIT: Duration = Duration::from_secs(120);

pub fn run_serve(config: &Path, output: &Path, options: SessionOptions) -> Result<()> {
    options.validate_serve()?;
    let termination = Termination::install()?;
    let cues = options
        .cue_socket
        .as_deref()
        .map(CueSink::connect)
        .transpose()?;
    let result = run_source(
        ProviderSource::Gemini(config),
        Lifetime::Serve,
        output,
        options.directed,
        options.camera_config.as_deref(),
        options.motor.as_ref(),
        options.failure_notices.as_ref(),
        None,
        &options.controls,
        cues,
        &termination,
    );
    // Everything is shut down and recorded by now. What is left is not to
    // come straight back; a request to stop ends the wait at once.
    if result
        .as_ref()
        .is_err_and(|error| error.is::<ProviderRefusesForNow>())
    {
        eprintln!(
            "lamp-voice: the provider refuses for now (quota or rate limit); staying {} s before leaving",
            REFUSAL_WAIT.as_secs()
        );
        let until = Instant::now() + REFUSAL_WAIT;
        while Instant::now() < until && termination.signal_name().is_none() {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    result
}

/// The first record of a run: what it was asked to do.
fn run_start_record(
    options: &DirectedOptions,
    lifetime: Lifetime,
    camera_requested: bool,
    motor_requested: bool,
    fixture: Option<&FixtureInfo>,
    cue_requested: bool,
) -> Value {
    let mut run_start = json!({"kind":"run_start","at_us":monotonic_us(),"scope":if options.ring_channel_ceiling.is_some(){"directed_voice_with_ring"}else{"directed_voice_only"},"seconds":lifetime.seconds(),"mode":if lifetime.continuous(){"serve"}else{"finite"},"acoustic_score":null,"audio_diagnostics_requested":options.diagnostics,
        "ring_channel_ceiling_requested":options.ring_channel_ceiling,
        "camera_context_requested":camera_requested,
        "motor_worker_requested":motor_requested,
        "microphone":options.microphone,
        "capture_device_requested":options.microphone.device(),
        "software_processing_requested":options.noise_suppression.software_processing(),
        "playout_start_horizon_us":options.playout_start.as_micros(),
        "playout_continuity":options.playout_start.policy().map(|policy| json!({"margin_us":policy.margin_us,"hold_us":policy.hold_us,"cover_us":policy.cover_us})),
        "turn_taking":options.turn_taking,"turn_taking_tail_us":if matches!(options.turn_taking, TurnTaking::V1Baseline | TurnTaking::Duplex) { Some(crate::turn_taking::TAIL_US) } else { None },
        "reference_clock":options.reference_clock,
        "reference_recovery":options.reference_recovery,
        "echo_reference":options.echo_reference,
        "provider_continuation":options.provider_continuation,
        "provider_kind":if fixture.is_some(){"one_cached_reply"}else{"gemini"},
        "fixture":fixture,"cue_requested":cue_requested});
    // Without the option the record keeps exactly its existing fields.
    if let Some(blocks) = options.early_end_blocks {
        run_start["early_end_ms"] = json!(u64::from(blocks) * 10);
    }
    run_start
}

#[allow(clippy::too_many_arguments)]
fn run_source(
    source: ProviderSource<'_>,
    lifetime: Lifetime,
    output: &Path,
    options: DirectedOptions,
    camera_config: Option<&Path>,
    motor_options: Option<&crate::options::MotorOptions>,
    notice_options: Option<&crate::options::NoticeOptions>,
    fixture: Option<FixtureInfo>,
    controls_options: &ControlsOptions,
    mut cues: Option<CueSink>,
    termination: &Termination,
) -> Result<()> {
    lifetime.validate()?;
    // Restore the software latch before any worker, including metadata/GPIO.
    let controls = RefCell::new(HandControls::restore(controls_options)?);
    // One optional storage/IPC process across every privacy cycle. Its failure
    // only withdraws reminder authority; healthy capture and conversation stay.
    let reminders = RefCell::new(match crate::reminders::Reminders::from_environment() {
        Ok(reminders) => reminders,
        Err(_) => {
            eprintln!("lamp-voice: Schedule consumer unavailable; reminders remain pending");
            None
        }
    });
    if lifetime.continuous() && options.diagnostics {
        return Err(io::Error::other("serve does not allow raw audio diagnostics").into());
    }
    if let Some(ceiling) = options.ring_channel_ceiling {
        lamp_ring::ChannelCeiling::new(ceiling).map_err(io::Error::other)?;
    }
    termination.check()?;
    refuse_legacy_owners()?;
    fs::create_dir(output)?;
    fs::set_permissions(output, fs::Permissions::from_mode(0o700))?;
    let mut trace = if lifetime.continuous() {
        SessionState::service(output)
    } else {
        SessionState::finite()
    };
    record(
        &mut trace,
        run_start_record(
            &options,
            lifetime,
            camera_config.is_some(),
            motor_options.is_some(),
            fixture.as_ref(),
            cues.is_some(),
        ),
    )?;
    // Bounded local reads happen once before any capture/speaker worker.
    let notices = FailureNotices::load(notice_options);
    let mut motor_rearm = true;
    record(
        &mut trace,
        json!({"kind":"failure_notice_capability","at_us":monotonic_us(),
        "capability":notices.capability(),"automatic_synthesis":false}),
    )?;
    if controls_options.enabled() {
        record(
            &mut trace,
            json!({"kind":"hand_controls_policy","at_us":monotonic_us(),
            "stop":controls_options.stop,"software_mute":controls_options.mute_record.is_some(),
            "clear":"fresh second local hold, durably saved before physical privacy checks",
            "observation_to_speaker_target_us":2000,"acoustic_stop_measured":false}),
        )?;
    }
    let result = (|| {
        let diagnostic_directory = options
            .diagnostics
            .then(|| fs::canonicalize(output))
            .transpose()?;
        if lifetime.continuous() {
            let executable = std::env::current_exe()?;
            service::supervise(
                &mut trace,
                &mut cues,
                termination,
                |trace, cues| {
                    let trace = RefCell::new(trace);
                    service_privacy::wait_with_gate(
                        &executable,
                        termination,
                        |state, boot, at_us| {
                            let mut trace = trace.borrow_mut();
                            record(
                                &mut trace,
                                json!({"kind":"service_privacy_state", "at_us":at_us,"privacy_worker_session":boot,"state":format!("{state:?}"),"audio_workers_open":false}),
                            )?;
                            flush_cues(&mut trace, cues);
                            Ok(())
                        },
                        || {
                            if let Some(reminders) = reminders.borrow_mut().as_mut() {
                                reminders.poll(monotonic_us());
                            }
                            let Some(controls) = &mut *controls.borrow_mut() else {
                                return Ok(true);
                            };
                            let (allowed, observation) = controls.quiescent_observation()?;
                            record_hand_pats(&mut trace.borrow_mut(), observation.pets)?;
                            Ok(allowed)
                        },
                    )
                },
                |trace, cues| {
                    run_inner(
                        source,
                        lifetime,
                        trace,
                        None,
                        options,
                        camera_config,
                        motor_options,
                        &mut motor_rearm,
                        &notices,
                        cues,
                        termination,
                        &mut controls.borrow_mut(),
                        &mut reminders.borrow_mut(),
                    )
                },
            )
        } else {
            run_inner(
                source,
                lifetime,
                &mut trace,
                diagnostic_directory.as_deref(),
                options,
                camera_config,
                motor_options,
                &mut motor_rearm,
                &notices,
                &mut cues,
                termination,
                &mut controls.borrow_mut(),
                &mut reminders.borrow_mut(),
            )
        }
    })();
    // This runs even after a startup or teardown fault. A previously armed
    // recovery record already inhibits restart if this update cannot be saved.
    let persistence = controls
        .borrow_mut()
        .as_mut()
        .map_or(Ok(()), HandControls::persist_muted);
    let result = service_privacy::finish(result, persistence);
    finish_run(output, &mut trace, &mut cues, result, termination)
}

fn finish_run(
    output: &Path,
    trace: &mut SessionState,
    cues: &mut Option<CueSink>,
    result: Result<()>,
    termination: &Termination,
) -> Result<()> {
    if trace.continuous() {
        record(
            trace,
            json!({"kind":"run_end","at_us":monotonic_us(),
            "status":if result.is_ok(){"stopped_unscored"}else{"failed"},
            "error":result.as_ref().err().map(ToString::to_string),
            "termination_signal":termination.signal_name(),"complete_evidence":false}),
        )?;
        flush_cues(trace, cues);
        // Hardware/authority cleanup completed before this bounded optional wait.
        let health = trace.finish_logging();
        println!(
            "{}",
            json!({"kind":"service_metadata_finished","health":health,"output":output,"complete_evidence":false})
        );
        return result;
    }
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

fn flush_cues(trace: &mut SessionState, cues: &mut Option<CueSink>) {
    trace.flush(cues, monotonic_us());
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
/// How the last two seconds of 10 ms blocks were judged, for a record:
/// the detector's score in twentieths, the level on the face's scale, and
/// how many the detector scored as speech and the faint floor turned away.
/// Numbers about the sound, and nothing of the sound itself.
fn last_blocks(judged: &VecDeque<(u8, u8, bool)>) -> Value {
    json!({
        "score_x20": judged.iter().map(|block| block.0).collect::<Vec<_>>(),
        "level": judged.iter().map(|block| block.1).collect::<Vec<_>>(),
        "under_floor": judged.iter().filter(|block| block.2 && block.0 >= 10).count(),
    })
}

fn record(trace: &mut SessionState, event: Value) -> Result<()> {
    trace.record(event).map_err(Into::into)
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
    trace: &mut SessionState,
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
            WorkerEvent::VisualContext { receipt } => {
                if receipt.at_us > monotonic_us() || receipt.request == 0 || receipt.session == 0 {
                    return Err(io::Error::other("invalid visual context receipt").into());
                }
                record(
                    trace,
                    json!({"kind":"visual_context", "at_us":receipt.at_us, "receipt":receipt,
                    "boundary":"provider image offer; not model consumption or visual correctness"}),
                )?;
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
    trace: &mut SessionState,
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
                queued_frames,
                accepted_frames,
                observed_at_us,
            } => {
                if written_owner.turn() != turn {
                    return Err(io::Error::other("speaker owner/turn mismatch").into());
                }
                if accepted_frames > 0 {
                    playback_mask.speaker_took(crate::turn_taking::Taken {
                        turn,
                        frames: accepted_frames,
                        queued_frames,
                        queue_observed_at_us,
                        level: reply
                            .as_ref()
                            .filter(|current| current.owner.turn() == turn)
                            .map_or(0, |current| current.voice_level),
                    });
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
                trace
                    .receipts
                    .accepted(written_owner, sequence, accepted_frames, end_sample)?;
                if let Some(current) = reply.as_mut().filter(|r| r.owner.turn() == turn)
                    && accepted_frames > 0
                {
                    let Some(pending) = speaker_pending.as_mut().filter(|p| p.sequence == sequence)
                    else {
                        return Err(
                            io::Error::other("speaker acceptance has no matching chunk").into()
                        );
                    };
                    if pending.origin != current.notice.origin() {
                        return Err(
                            io::Error::other("speaker receipt output origin changed").into()
                        );
                    }
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
                            json!({"kind":if pending.origin == SpeechOrigin::Provider {"speaker_first_write"}else{"failure_notice_first_write"},"origin":pending.origin,"notice":current.notice.evidence(),"turn":turn,"owner":current.owner,"playback_sequence":token.sequence(),"at_us":observed_at_us,"boundary":"ALSA accepted; acoustic onset unmeasured",
                                "start_gate":current.start_trace()}),
                        )?;
                    }
                    if pending.accepted == 240 {
                        if pending.final_chunk {
                            if pending.origin == SpeechOrigin::FailureNotice {
                                current.notice.accepted_final(sequence, end_sample)?;
                            }
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
                if let Some(reminder) = reply.as_mut().and_then(|current| current.reminder.as_mut())
                {
                    reminder.invalidate();
                }
                trace.receipts.reset(previous);
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
                trace.receipts.drained(drained_owner, last_sample);
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
                trace.receipts.drained(delivered, final_sample);
                if let Some(current) = reply.as_mut().filter(|reply| reply.owner == delivered) {
                    let origin = current.notice.origin();
                    if origin == SpeechOrigin::FailureNotice {
                        current.notice.validate_retirement(sequence, final_sample)?;
                    }
                    let token = current.retire_playback(sequence, final_sample.frame)?;
                    owner.playback_ended(now(), token)?;
                    if origin == SpeechOrigin::Provider
                        && let Some(reminder) = &mut current.reminder
                    {
                        reminder.retired(token.sequence(), final_sample.epoch, final_sample.frame);
                    }
                    if origin == SpeechOrigin::FailureNotice {
                        current.notice.retire(observed_at_us)?;
                    }
                    *snapshot = owner.snapshot(now())?;
                    io.publish_speaker_snapshot(*snapshot)?;
                    *last_publish = monotonic_us();
                    record(
                        trace,
                        json!({"kind":if origin == SpeechOrigin::Provider {"speech_final_sample_retired"}else{"failure_notice_final_sample_retired"},"origin":origin,"notice":current.notice.evidence(),"turn":delivered.turn(),"owner":delivered,
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
                trace.receipts.rejected(sequence);
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

fn accepted_decision(decision: Decision, trace: &mut SessionState) -> Result<Option<Accepted>> {
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
    trace: &mut SessionState,
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
    trace: &mut SessionState,
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
    trace: &mut SessionState,
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
        // Evidence about input of a request that is already retired: it
        // changes no request's delivery state.
        ProviderOutput::Ready { .. }
        | ProviderOutput::Transcript { .. }
        | ProviderOutput::ReminderDeferred { .. }
        | ProviderOutput::InputDiscarded { .. } => {}
        // Only a provider configured to take a silent earlier response as
        // over reports this. It grants nothing here: the request still needs
        // its own Started before any of its output is usable.
        ProviderOutput::OwnershipAssumed { .. } => {}
        ProviderOutput::Ended { .. } => cloud.end_output()?,
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
            | ProviderInput::End { request, .. }
            | ProviderInput::Reminder { request, .. }
            | ProviderInput::Written { request, .. } => *request,
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
                (WorkerEvent::ReferenceStatus { status }, "capture") => {
                    self.push(reference_status_trace(status))?
                }
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
                "camera" => match channels.data.receive::<CameraEvent>()? {
                    Some(CameraEvent::Progress { .. } | CameraEvent::FrameLink { .. }) => {}
                    Some(_) => return Err(io::Error::other("camera activated before privacy")),
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
/// Startup workers have denied capture authority. A mute observed here is
/// persisted and aborts startup; partial-start teardown is not a resumable cycle.
fn controls_startup(controls: &mut Option<HandControls>) -> io::Result<()> {
    if let Some(controls) = controls.as_mut()
        && controls.poll()?.muted
    {
        controls.persist_muted()?;
        return Err(io::Error::other(
            "software mute during worker startup; restart remains muted",
        ));
    }
    Ok(())
}

fn record_hand_pats(trace: &mut SessionState, count: u64) -> Result<()> {
    if count != 0 {
        record(
            trace,
            json!({"kind":"hand_pat_observed","at_us":monotonic_us(),"count":count,"action":"none"}),
        )?;
    }
    Ok(())
}

/// The provider worker's arguments. Every one comes from the options the
/// worker acts on or from the environment; the coordinator's own settings,
/// such as when it sends the end (`--early-end-ms`), are not among them.
fn provider_arguments(
    source: ProviderSource<'_>,
    options: &DirectedOptions,
    lifetime: Lifetime,
    camera: Option<&PreparedCamera>,
) -> io::Result<Vec<String>> {
    // A service that runs all day never gives its provider link up.
    let keep_link = lifetime.continuous() && matches!(source, ProviderSource::Gemini(_));
    let mut arguments = source.arguments()?;
    // Before the camera context, which the worker reads as the rest.
    arguments.extend(options.provider_continuation.arguments());
    arguments.extend(crate::sight::arguments_from_environment());
    arguments.extend(crate::thread::arguments_from_environment());
    arguments.extend(crate::session_fact::arguments_from_environment());
    arguments.extend(crate::room_told::arguments_from_environment());
    arguments.extend(crate::air_told::arguments_from_environment());
    if keep_link {
        arguments.push(crate::provider_worker::KEEP_LINK_FLAG.to_owned());
    }
    arguments.extend(crate::tools::arguments_from_environment());
    arguments.extend(crate::attention::arguments_from_environment());
    arguments.extend(crate::addressed::arguments_from_environment());
    arguments.extend(crate::attention::name_arguments_from_environment());
    arguments.extend(crate::called::arguments_from_environment());
    arguments.extend(crate::bus_out::worker_arguments_from_environment());
    arguments.extend(crate::first_output::arguments_from_environment());
    arguments.extend(crate::not_speech::arguments_from_environment());
    if let Some(camera) = camera {
        arguments.extend(camera.source.arguments());
    }
    Ok(arguments)
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
    camera: Option<PreparedCamera>,
    lifetime: Lifetime,
    termination: &Termination,
    controls: &mut Option<HandControls>,
) -> Result<Rig> {
    controls_startup(controls)?;
    termination.check()?;
    let executable = std::env::current_exe()?;
    // Model loading overlaps the remaining startup; nothing waits for it.
    let endpoint = EndpointRig::start(&options, &executable);
    let provider_boot = new_boot()?;
    let arguments = provider_arguments(source, &options, lifetime, camera.as_ref())?;
    // A service that runs all day may keep a second voice route standing by.
    let second_arguments = second::configured(
        std::env::var_os(crate::second_config::SECOND_CONFIG_ENV)
            .filter(|_| lifetime.continuous() && matches!(source, ProviderSource::Gemini(_))),
    )
    .map(|config| second::arguments(&arguments, &config))
    .transpose()?;
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let mut provider = Worker::spawn_with_boot_and_tick(
        &executable,
        directory,
        "provider",
        boot,
        provider_boot,
        &arguments,
        || {
            controls_startup(controls)?;
            termination.check()
        },
    )?;
    let mut last = 0;
    let capture_arguments =
        options.capture_arguments(diagnostic_directory.map(|p| p.join("capture-audio")))?;
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
        || {
            controls_startup(controls)?;
            pulse(owner, &mut [&mut provider], &mut last, startup, termination)
        },
    )?;
    let mut speaker = Worker::spawn_with_tick(
        &executable,
        directory,
        "speaker",
        boot,
        &speaker_arguments,
        || {
            controls_startup(controls)?;
            pulse(
                owner,
                &mut [&mut provider, &mut capture],
                &mut last,
                startup,
                termination,
            )
        },
    )?;
    // A service that runs all day also shows a resting face between turns,
    // under the same ceiling as its cues. A bounded directed run does not.
    let resting_ceiling = options
        .ring_channel_ceiling
        .map(|ceiling| ceiling.to_string());
    let mut ring_arguments: Vec<&str> = match (&resting_ceiling, lifetime.continuous()) {
        (Some(ceiling), true) => vec!["--continuous", "--resting", ceiling],
        (None, true) => vec!["--continuous"],
        (_, false) => Vec::new(),
    };
    // The lamp's own light is asked for and said on the bus, by a ring
    // that runs all day (`ring_light`). A worker is started with no
    // environment, so it is told where the bus is.
    let ring_bus = crate::bus_out::worker_arguments_from_environment();
    // And whether its face is the eye (`LAMP_RING_EYE=on`, task 34), which
    // needs that bus to see where people are.
    let ring_eye = crate::ring_sight::arguments_from_environment();
    if lifetime.continuous() {
        ring_arguments.extend(ring_bus.iter().map(String::as_str));
        ring_arguments.extend(ring_eye.iter().map(String::as_str));
    }
    let mut ring = if options.ring_channel_ceiling.is_some() {
        OptionalRing::spawn(&executable, directory, boot, &ring_arguments, || {
            controls_startup(controls)?;
            pulse(
                owner,
                &mut [&mut provider, &mut capture, &mut speaker],
                &mut last,
                startup,
                termination,
            )
        })?
    } else {
        OptionalRing::from(None)
    };
    // The optional worker is created closed, before privacy activation. Its
    // startup failure is evidence of unavailable sight, not a voice failure.
    let mut camera = if let Some(prepared) = camera {
        let consumer = format!("context={}", boot_text(provider_boot));
        let extra = [
            prepared
                .path
                .to_str()
                .ok_or_else(|| io::Error::other("camera path UTF-8"))?,
            &prepared.digest,
            "--frame-consumer",
            &consumer,
        ];
        match camera::optional_startup(
            |tick| {
                Worker::spawn_with_boot_and_tick(
                    &executable,
                    directory,
                    "camera",
                    boot,
                    prepared.source.camera,
                    &extra,
                    tick,
                )
            },
            || {
                controls_startup(controls)?;
                let previous = last;
                pulse(
                    owner,
                    &mut [&mut provider, &mut capture, &mut speaker],
                    &mut last,
                    startup,
                    termination,
                )?;
                let snapshot = (previous != last)
                    .then(|| owner.snapshot(now()))
                    .transpose()
                    .map_err(io::Error::other)?;
                ring.startup_tick(snapshot);
                Ok(())
            },
        )? {
            Some(worker) => Some(OptionalCamera::new(worker, prepared.source)),
            None => {
                startup.push(json!({"kind":"camera_unavailable", "at_us":monotonic_us(), "stage":"worker_startup", "audio_continues":true}))?;
                None
            }
        }
    } else {
        None
    };
    let mut later_tick = |second: Option<&mut second::Route>| -> io::Result<()> {
        controls_startup(controls)?;
        let previous = last;
        pulse(
            owner,
            &mut [&mut provider, &mut capture, &mut speaker],
            &mut last,
            startup,
            termination,
        )?;
        let ring_snapshot = (previous != last)
            .then(|| owner.snapshot(now()))
            .transpose()
            .map_err(io::Error::other)?;
        ring.startup_tick(ring_snapshot);
        // A post-handshake camera failure is optional even while a mandatory
        // child starts. It never joins the voice worker failure domain.
        if let Some(camera) = camera.as_mut() {
            let snapshot = (previous != last)
                .then(|| owner.snapshot(now()))
                .transpose()
                .map_err(io::Error::other)?;
            camera.startup_tick(startup, snapshot);
        }
        if let Some(route) = second {
            let snapshot = (previous != last)
                .then(|| owner.snapshot(now()))
                .transpose()
                .map_err(io::Error::other)?;
            route.startup_tick(snapshot, startup)?;
        }
        Ok(())
    };
    let mut second = second::Route::spawn(
        &executable,
        directory,
        boot,
        second_arguments.as_deref(),
        || later_tick(None),
    )?;
    let privacy = Worker::spawn_with_tick(&executable, directory, "privacy", boot, &[], || {
        later_tick(second.as_mut())
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
        camera,
        motor: None,
        ring_channel_ceiling: options.ring_channel_ceiling,
        diagnostic_directory: diagnostic_directory.map(Path::to_path_buf),
        noise_suppression: options.noise_suppression,
        turn_taking: options.turn_taking,
        end_silence_blocks: options.end_silence_blocks,
        early_end_blocks: options.early_end_blocks,
        faint_level: options.faint_level,
        playout_start: options.playout_start,
        microphone: options.microphone,
        endpoint,
        paused: keeper_pause(lamp_bus::Bus::from_environment()),
        second,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_inner(
    source: ProviderSource<'_>,
    lifetime: Lifetime,
    trace: &mut SessionState,
    diagnostic_directory: Option<&Path>,
    options: DirectedOptions,
    camera_config: Option<&Path>,
    motor_options: Option<&crate::options::MotorOptions>,
    motor_rearm: &mut bool,
    notices: &FailureNotices,
    cues: &mut Option<CueSink>,
    termination: &Termination,
    controls: &mut Option<HandControls>,
    reminders: &mut Option<crate::reminders::Reminders>,
) -> Result<()> {
    termination.check()?;
    if let Some(controls) = controls.as_mut() {
        controls.fence_clear()?;
        if controls.poll()?.muted {
            controls.persist_muted()?;
            return Err(Box::new(SoftwareMute {
                observed_at_us: monotonic_us(),
            }));
        }
        controls.arm_active()?;
    }
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
    let camera = camera_config.and_then(|path| {
        match PreparedCamera::load(path, &directory.path) {
            Ok(camera) => {
                let _ = record(trace, json!({"kind":"camera_context_configured", "at_us":monotonic_us(),
                    "config_sha256":camera.digest, "camera_worker":camera.source.camera,
                    "source":camera.source.source, "width":camera.source.width,"height":camera.source.height,
                    "capture_buffer_limit":camera.source.max_frame_bytes,"cloud_jpeg_limit":lamp_gemini::MAX_IMAGE_BYTES,
                    "dequeue_age_limit_us":crate::visual_context::MAX_FRAME_AGE_US}));
                Some(camera)
            },
            Err(_) => {
                // Configuration/device absence never silently turns into sight.
                let _ = record(trace, json!({"kind":"camera_unavailable", "at_us":monotonic_us(), "stage":"configuration", "audio_continues":true}));
                None
            }
        }
    });
    let head_options = motor_options.and_then(|m| m.head.as_ref());
    let head = head_options
        .map(|options| {
            let camera = camera
                .as_ref()
                .ok_or_else(|| io::Error::other("head qualification requires prepared camera"))?;
            crate::head_qualification::Driver::load(
                options,
                &camera.digest,
                camera.source.camera,
                boot,
            )
        })
        .transpose()?;
    let head_camera = head
        .as_ref()
        .and(camera.as_ref())
        .map(|c| {
            crate::head_qualification::Frames::new(
                c.source.camera,
                c.config.clone(),
                c.digest.clone(),
            )
        })
        .transpose()?;
    let motor = motor_options.filter(|_| *motor_rearm).and_then(|options| match PreparedMotor::load_bound(options, &directory.path, head.as_ref().map(crate::head_qualification::Driver::binding)) {
        Ok(prepared) => {
            let _ = record(trace, json!({"kind":"motor_configured","at_us":monotonic_us(),"identity":prepared.identity(),"automatic_motion":false}));
            Some(prepared)
        }
        Err(error) => {
            let _ = record(trace, json!({"kind":"motor_unavailable","at_us":monotonic_us(),"stage":"configuration","error":motor::bounded(error),"audio_continues":true}));
            *motor_rearm = false;
            None
        }
    });
    if head.is_some() && motor.is_none() {
        return Err(io::Error::other("head qualification motor provisioning failed").into());
    }
    let mut motor = motor.map(|prepared| OptionalMotor::new(boot, Some(prepared)));
    if let Some(driver) = head {
        record(
            trace,
            json!({"kind":"head_qualification_configured","at_us":monotonic_us(),"plan_sha256":driver.plan_sha256,"placement_binding":driver.binding(),"automatic_clearance_inference":false,"automatic_motion":false}),
        )?;
        motor.as_mut().expect("checked above").attach_head(driver)?;
    }
    let mut rig = spawn_rig(
        source,
        &directory.path,
        boot,
        &mut owner,
        &mut startup,
        diagnostic_directory,
        options,
        camera,
        lifetime,
        termination,
        controls,
    )?;
    rig.motor = motor;
    if let Some(frames) = head_camera {
        if let Some(camera) = &mut rig.camera {
            camera.retain_head_frames(frames);
        } else if let Some(motor) = &mut rig.motor {
            motor.begin_shutdown();
        }
    }
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
        conversation_with_controls(
            &mut rig,
            &mut owner,
            lifetime,
            trace,
            startup,
            &mut reply,
            cues,
            termination,
            notices,
            controls,
            reminders,
        )
    })();
    let privacy_mute = result
        .as_ref()
        .err()
        .is_some_and(|error| error.is::<PhysicalMute>() || error.is::<SoftwareMute>());
    let result = finish_active_run(&mut rig, &mut owner, &mut reply, trace, cues, result);
    if let Some(motor) = &rig.motor {
        *motor_rearm &= motor.may_rearm_after_cycle(privacy_mute);
    }
    // Reopening is allowed only after all child and local endpoint ownership
    // has ended. Never accumulate per-cycle output/IPC directories.
    drop(rig);
    let result = if lifetime.continuous() {
        service_privacy::finish(result, fs::remove_dir(&directory.path))
    } else {
        result
    };
    if result.is_ok()
        && let Some(controls) = controls.as_mut()
    {
        controls.disarm_after_cleanup()?;
    }
    result
}

fn service_head(
    rig: &mut Rig,
    owner: &mut Controller,
    privacy_allowed: bool,
    trace: &mut SessionState,
) -> Result<()> {
    let frame = rig.camera.as_ref().and_then(OptionalCamera::head_frame);
    if let Some(motor) = &mut rig.motor
        && let Some(mut head) = motor.head.take()
    {
        let events = head.tick(owner, motor, frame, privacy_allowed);
        motor.head = Some(head);
        for event in events {
            record(trace, event)?;
        }
    }
    Ok(())
}

fn service_motor(rig: &mut Rig, trace: &mut SessionState) -> Result<()> {
    if let Some(motor) = &mut rig.motor {
        let mut error = None;
        motor.poll(|event| {
            if let Err(e) = record(trace, json!({"kind":"motor_lifecycle","at_us":monotonic_us(),"details":event,"listening_readiness":false,"physical_stop_verified":false})) { error.get_or_insert(e); }
        });
        let worker = motor.identity().map(|identity| identity.worker);
        if let Some(reason) = motor.take_failure() {
            record(
                trace,
                json!({"kind":"motor_degraded","at_us":monotonic_us(),"worker":worker,"reason":reason,"audio_continues":true,"physical_stop_verified":false}),
            )?;
        }
        if let Some(error) = error {
            return Err(error);
        }
    }
    Ok(())
}

fn finish_active_run(
    rig: &mut Rig,
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    trace: &mut SessionState,
    cues: &mut Option<CueSink>,
    result: Result<()>,
) -> Result<()> {
    if result.is_err() {
        // Keep an explicit outcome even when the failing child cannot deliver
        // speech. Privacy revocation and stop below still run if tracing fails.
        let reason = if result
            .as_ref()
            .err()
            .is_some_and(|error| error.downcast_ref::<PhysicalMute>().is_some())
        {
            "privacy_muted"
        } else {
            "runtime_failed"
        };
        let _ = cancel_reply(owner, reply, trace, reason);
    }
    // Also close a yield with no active reply, including any error exit. Lost
    // observation storage still cannot inhibit privacy revocation or cleanup.
    let _ = finish_duplex_yield(
        trace,
        monotonic_us(),
        if result.is_err() {
            "session_failed"
        } else {
            "session_ended"
        },
        None,
        None,
    );
    let _ = owner.set_camera_permission(now(), Permission::Denied);
    let _ = owner.set_microphone_permission(now(), Permission::Denied);
    if let Ok(snapshot) = owner.snapshot(now()) {
        let _ = rig.publish(snapshot);
    }
    flush_cues(trace, cues);
    let shutdown = rig.shutdown(trace);
    service_privacy::finish(result, shutdown)
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn conversation(
    rig: &mut Rig,
    owner: &mut Controller,
    lifetime: Lifetime,
    trace: &mut SessionState,
    startup: StartupEvents,
    reply: &mut Option<Reply>,
    cues: &mut Option<CueSink>,
    termination: &Termination,
    notices: &FailureNotices,
) -> Result<()> {
    conversation_with_controls(
        rig,
        owner,
        lifetime,
        trace,
        startup,
        reply,
        cues,
        termination,
        notices,
        &mut None,
        &mut None,
    )
}

#[allow(clippy::too_many_arguments)]
fn conversation_with_controls(
    rig: &mut Rig,
    owner: &mut Controller,
    lifetime: Lifetime,
    trace: &mut SessionState,
    startup: StartupEvents,
    reply: &mut Option<Reply>,
    cues: &mut Option<CueSink>,
    termination: &Termination,
    notices: &FailureNotices,
    controls: &mut Option<HandControls>,
    reminders: &mut Option<crate::reminders::Reminders>,
) -> Result<()> {
    conversation_with_gate(
        rig,
        owner,
        lifetime,
        trace,
        startup,
        reply,
        cues,
        termination,
        notices,
        controls,
        reminders,
        PrivacyGate::default(),
        #[cfg(test)]
        None,
    )
}

/// An explicit policy allowance for a real-worker ordering fixture. The
/// source clock and IPC bounds stay real; the watchdog bounds the host run.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn conversation_with_fixture_privacy(
    rig: &mut Rig,
    owner: &mut Controller,
    lifetime: Lifetime,
    trace: &mut SessionState,
    startup: StartupEvents,
    reply: &mut Option<Reply>,
    cues: &mut Option<CueSink>,
    termination: &Termination,
    notices: &FailureNotices,
    gate: PrivacyGate,
    host_deadline: Instant,
) -> Result<()> {
    conversation_with_gate(
        rig,
        owner,
        lifetime,
        trace,
        startup,
        reply,
        cues,
        termination,
        notices,
        &mut None,
        &mut None,
        gate,
        Some(host_deadline),
    )
}

#[allow(clippy::too_many_arguments)]
fn conversation_with_gate(
    rig: &mut Rig,
    owner: &mut Controller,
    lifetime: Lifetime,
    trace: &mut SessionState,
    startup: StartupEvents,
    reply: &mut Option<Reply>,
    cues: &mut Option<CueSink>,
    termination: &Termination,
    notices: &FailureNotices,
    controls: &mut Option<HandControls>,
    reminders: &mut Option<crate::reminders::Reminders>,
    mut gate: PrivacyGate,
    #[cfg(test)] host_deadline: Option<Instant>,
) -> Result<()> {
    let mut cloud = startup.cloud;
    let mut output_flow = startup.output_flow;
    for event in startup.events {
        record(trace, event)?;
    }
    let mut session_clock =
        SessionClock::new(lifetime, session_instant(), INPUT_READINESS_TIMEOUT)?;
    let privacy_started = session_instant();
    let mut privacy_sequence = 0u64;
    let mut permitted = false;
    let mut capture_requested = false;
    let mut capture_epoch = None;
    let mut last_capture = None;
    let mut last_publish = 0;
    let mut last_health = 0;
    let mut last_hand_health = 0;
    let mut choreography = if rig.ring.is_active() {
        rig.ring_channel_ceiling
            .map(RingChoreographer::new)
            .transpose()?
    } else {
        None
    };
    let mut ring_trace = RingTrace::default();
    // How loud the person's last captured block was, for the ring's face.
    let mut heard_level = 0u8;
    let mut speech_levels: VecDeque<(u64, u8)> = VecDeque::new();
    // When the detector last called a block speech.
    let mut last_speech_us = 0u64;
    let reminder_cycle_started_us = monotonic_us();
    // A service that runs all day may also tell a body what it is doing.
    let mut body_link = if lifetime.continuous() {
        crate::body_link::BodyLink::from_environment()
    } else {
        None
    };
    // And its eyes that they may be open, while the microphone is.
    let mut eyes_link = if lifetime.continuous() {
        crate::body_link::EyesLink::from_environment()
    } else {
        None
    };
    let mut attention = crate::body_link::Attention::default();
    // The same count kept whether or not there is a body to tell: whether
    // the talk in the room has been for the lamp lately.
    let mut room = crate::body_link::Attention::default();
    // Short utterances in a room that is not busy go to the model after all
    // (`LAMP_SHORT_SOUNDS=quiet-room`, off unless set).
    let short_sounds = crate::not_speech::short_sounds_from_environment();
    // The turn whose speech last ended and when, and the turn for which the
    // floor was last held, so that the record says so once.
    let mut speech_ended: Option<(u64, u64)> = None;
    let mut floor_held_for: Option<u64> = None;
    // The turn that was asked while the room was busy, and when the lamp
    // began to be heard answering it.
    let mut asked_in_busy_room: Option<u64> = None;
    let mut busy_answer_since: Option<(u64, u64)> = None;
    // When the lamp was last speaking, and the turn that took the place of
    // another: neither a word after its own nor one that cut in is a sound.
    let mut last_spoke_us: Option<u64> = None;
    let mut spoke_in_turn: Option<u64> = None;
    let mut took_over_turn: Option<u64> = None;
    // The turn whose speech last ended as something said, not a sound, and
    // when: someone was there then.
    let mut said_turn: Option<u64> = None;
    let mut said_at_us: Option<u64> = None;
    // The turn last judged a sound and sent on only for the model to judge
    // (`LAMP_SHORT_SOUNDS`): until it is answered the lamp does not ponder
    // it, and the room and the provider do not count it as talk.
    let mut sound_turn: Option<u64> = None;
    // The answer whose words are arriving, and whether any of them has been
    // a word yet.
    let mut answer_words: Option<(u64, bool)> = None;
    // Saying hello first to someone who comes and sits down, when the eyes
    // keep a presence file for it; and when anything was last said to the
    // lamp or by it.
    let mut arrival = if lifetime.continuous() {
        crate::speak_up::Arrival::from_environment(monotonic_us())
    } else {
        None
    };
    let mut last_turn_us = monotonic_us();
    // News from the work handed to agents on other computers, told
    // unasked at a good moment (`docs/tasks.md`).
    let mut task_news = if lifetime.continuous() {
        crate::task_news::TaskNews::from_environment()
    } else {
        None
    };
    // The level under which sound is not speech, which may follow the room;
    // the speech blocks of the turn in hand and of the moment before it,
    // until it is known whether they were for the lamp; and the turn they
    // belong to.
    let mut room_floor = rig
        .faint_level
        .map(|lowest| crate::room_floor::from_environment(lowest, monotonic_us()));
    let mut floor_recorded: Option<(u8, Option<u8>)> = None;
    let mut near = crate::room_floor::Near::from_environment();
    // The last two seconds of blocks as the detector and the floor judged
    // them, for the record of an utterance thrown out as not speech:
    // twentieths of the detector's score, the level, and whether the floor
    // turned the block away. Numbers only.
    let mut judged_blocks: VecDeque<(u8, u8, bool)> = VecDeque::new();
    let mut turn_blocks: Vec<u8> = Vec::new();
    let mut loose_blocks: VecDeque<(u64, u8)> = VecDeque::new();
    let mut floor_turn: Option<u64> = None;
    // Taking a look when asked: the lamp's own "Let me look" is how it asks
    // for its eyes, and the look is the turn after it. Only where the eyes
    // keep a frame for it.
    let looks = lifetime.continuous()
        && std::env::var_os(crate::sight::LOOK_ENVIRONMENT).is_some_and(|path| !path.is_empty());
    let mut answer_opening: Option<(u64, String)> = None;
    let mut look_wanted: Option<(u64, u64)> = None;
    // A due reminder's quoted words cannot authorize a later camera turn.
    // Turn IDs increase within this conversation; fence older late words too.
    let mut last_reminder_turn: Option<u64> = None;
    let mut gesture_cue = crate::body_link::Cue::default();
    // What the voice is doing, heard and said, for whoever listens on the
    // lamp's bus. Nothing here waits for a listener.
    let mut bus_out = crate::bus_out::BusOut::from_environment();
    // The voice's own timing, for the part that plans what the lamp shows
    // (docs/expression.md). Only if that is switched on.
    let mut expression_out = crate::expression_out::ExpressionOut::from_environment();
    // And, only if someone switched it on, what the microphone hears, for
    // a part of the lamp that listens for itself.
    let mut mic_out = crate::bus_out::MicOut::from_environment(rig.microphone.as_str());
    // The lamp's name as its own part hears it in the room's sound, for a
    // second look at something said to it by name that got nothing. Only
    // if that is switched on.
    let mut name_in = crate::second_look::on()
        .then(crate::bus_out::NameIn::from_environment)
        .flatten();
    let mut second_look =
        crate::second_look::SecondLook::default().with_the_note(crate::called::on());
    let policy_detector = match rig.turn_taking {
        // Duplex gates the scores this detector sees; its rule is unchanged.
        TurnTaking::V1Baseline | TurnTaking::Duplex => TurnDetector::v1_baseline(),
        TurnTaking::VadOnly => TurnDetector::default(),
    };
    let (mut detector, endpoint_policy) = EndpointRig::attach(
        rig.endpoint.as_mut(),
        rig.turn_taking,
        match rig.end_silence_blocks {
            Some(blocks) => policy_detector
                .with_end_silence(blocks)
                .map_err(io::Error::other)?,
            None => policy_detector,
        },
    )?;
    record(trace, endpoint_policy)?;
    // The end sent early, if that is switched on: the request is ended to
    // the provider after a short quiet, and its answer held until the
    // detector's own end of speech confirms it (`early_end`).
    let mut early = rig.early_end_blocks.map(EarlyEnd::new);
    if let Some(early) = early.as_ref() {
        record(
            trace,
            json!({"kind":"early_end_policy","at_us":monotonic_us(),
                "early_end_ms":u64::from(early.blocks()) * 10,
                "confirmed_by":"the endpoint policy: its silence wait, or the end-of-turn verdict",
                "speaking_again_after_blocks":crate::activity::START_BLOCKS,
                "speaking_again_score":crate::activity::START_PROBABILITY,
                "opening_kept_blocks":crate::activity::PRE_ROLL_BLOCKS,
                "given_up_as":"user_interrupted, continued by the next request"}),
        )?;
    }
    let mut endpoint_watch = endpoint::Watch::default();
    // Duplex is the conservative mask with ears. A candidate that began over
    // the lamp is held until the lamp's own sound has gone, so its decision
    // budget is longer; everything else about admission is unchanged.
    let duplex = rig.turn_taking == TurnTaking::Duplex;
    let hold = if duplex { Hold::DUPLEX } else { Hold::STANDARD };
    let mut admission = InputAdmission::with_hold(hold);
    let mut playback_mask = if duplex {
        record(
            trace,
            json!({"kind":"duplex_policy","at_us":monotonic_us(),
            "speech_score":Hearing::EVALUATED.speech,
            "explained_percent":Hearing::EVALUATED.explained_percent,
            "floor_percent":Hearing::EVALUATED.floor_percent,
            "clear_after_blocks":Hearing::EVALUATED.clear_after_blocks,
            "yield_after_blocks":Confirmation::EVALUATED.onset_blocks,
            "commit_after_clear_blocks":Confirmation::EVALUATED.speech_blocks,
            "resume_after_still_blocks":Confirmation::EVALUATED.silence_blocks,
            "resume_deadline_blocks":Confirmation::EVALUATED.deadline_blocks,
            "plain_percent":Hearing::EVALUATED.plain_percent,
            "commit_on_stillness_after_plain_blocks":Confirmation::EVALUATED.plain_blocks,
            "fruitless_yields_before_fallback":FRUITLESS_YIELDS,
            "decision_budget_us":hold.decision_budget_us,
            "provider_queue_bound":provider_queue_bound(hold),
            "provider_total_queue_bound":provider_queue_bound(hold) + 1,
            "provider_terminal_allowance":1,
            "fallback":"v1-baseline mask for a block without echo evidence and for the rest of a reply after its fruitless yields",
            "basis":"echo evidence with the detector score; not level or score alone",
            "addressee_inference":false}),
        )?;
        PlaybackMask::duplex()
    } else {
        PlaybackMask::new(rig.turn_taking)
    };
    let mut heard_reply: Option<TurnOwner> = None;
    let mut policy_available = None;
    let mut capture_lineage: Option<CaptureLineage> = None;
    let provider_queue_bound = provider_queue_bound(hold);
    let mut provider_queue = VecDeque::with_capacity(provider_queue_bound);
    let mut submission_draining = false;
    let mut speaker_pending: Option<PendingSpeaker> = None;
    let mut speaker_sequence = 0u64;
    let mut hand_cancelled_through: Option<u64> = None;
    let mut snapshot = owner.snapshot(now())?;
    let mut highest_known_turn = snapshot.owner().map(|owner| owner.turn());
    rig.publish(snapshot)?;
    // Paused by the keeper as of the last loop, for the record of a change.
    let mut was_paused = false;
    // Whether a sound not taken for the pause has been recorded during it:
    // once a pause, not once a block.
    let mut refused_while_paused = false;
    loop {
        #[cfg(test)]
        if host_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "real-provider fixture exceeded its host watchdog",
            )
            .into());
        }
        #[cfg(test)]
        crate::scenario_clock::step(crate::scenario_clock::Phase::Run)?;
        let loop_start = Instant::now();
        if let Some(current) = reply.as_ref() {
            let turn = current.owner.turn();
            highest_known_turn = Some(highest_known_turn.map_or(turn, |old| old.max(turn)));
        }
        if lifetime.continuous() && termination.signal_name().is_some() {
            record(
                trace,
                json!({"kind":"service_stop_requested","at_us":monotonic_us(),"signal":termination.signal_name()}),
            )?;
            cancel_reply(owner, reply, trace, "service_stopped")?;
            return Ok(());
        }
        termination.check()?;
        let clock = monotonic_us();
        let paused = (rig.paused)();
        if paused != was_paused {
            was_paused = paused;
            refused_while_paused = false;
            eprintln!(
                "lamp-voice: {}",
                if paused {
                    "paused by the keeper: no new turn; one under way is finished"
                } else {
                    "resumed by the keeper"
                }
            );
            record(
                trace,
                json!({"kind":"keeper_pause","paused":paused,"at_us":clock}),
            )?;
        }
        if session_clock.startup_expired(session_instant()) {
            return Err(io::Error::other("voice input never became ready").into());
        }
        // Privacy always precedes input, model output, or actuator work.
        // A later fresh mute cannot relabel an already-lost sensor lease as a
        // healthy user-mute lifecycle that permits automatic reopening.
        if lifetime.continuous()
            && privacy_sequence != 0
            && gate.state(now()) == PrivacyState::Unavailable
        {
            return Err(
                io::Error::other("service privacy freshness lost before next observation").into(),
            );
        }
        for _ in 0..16 {
            let Some(event) = rig.privacy.channels.control.receive::<WorkerEvent>()? else {
                break;
            };
            match event {
                WorkerEvent::Privacy {
                    acquired_at_us,
                    muted,
                } => {
                    privacy_sequence = privacy_sequence
                        .checked_add(1u64)
                        .ok_or_else(|| io::Error::other("privacy sequence overflow"))?;
                    gate.observe(
                        privacy_sequence,
                        MonoTime::from_micros(acquired_at_us),
                        now(),
                        muted,
                    );
                    if lifetime.continuous() {
                        match gate.state(now()) {
                            PrivacyState::Unavailable => {
                                return Err(io::Error::other(
                                    "invalid or stale service privacy observation",
                                )
                                .into());
                            }
                            PrivacyState::Muted => {
                                let observed_at_us = monotonic_us();
                                record(
                                    trace,
                                    json!({"kind":"physical_mute", "at_us":observed_at_us,"acquired_at_us":acquired_at_us,"fresh_observation":true}),
                                )?;
                                return Err(Box::new(PhysicalMute { observed_at_us }));
                            }
                            _ => {}
                        }
                    }
                }
                WorkerEvent::Fault { code } => return Err(io::Error::other(code).into()),
                _ => return Err(io::Error::other("unexpected privacy event").into()),
            }
        }
        // Provider loss closes cloud submissions, never the microphone authority
        // needed to retain interruptions or finish already-owned playback.
        let privacy_state = gate.state(now());
        if lifetime.continuous() {
            match privacy_state {
                PrivacyState::Muted => {
                    let at_us = monotonic_us();
                    record(
                        trace,
                        json!({"kind":"physical_mute", "at_us":at_us,"fresh_observation":true}),
                    )?;
                    return Err(Box::new(PhysicalMute {
                        observed_at_us: at_us,
                    }));
                }
                PrivacyState::Unavailable
                    if privacy_sequence != 0
                        || session_instant().saturating_duration_since(privacy_started)
                            >= Duration::from_micros(gate.freshness_us()) =>
                {
                    return Err(
                        io::Error::other("physical privacy unavailable; not a user mute").into(),
                    );
                }
                _ => {}
            }
        }
        if let Some(controls) = controls.as_mut() {
            let observation = controls.poll()?;
            if observation.stop || observation.muted {
                let observed_at_us = monotonic_us();
                let cancellation = hand::cancel(
                    owner,
                    reply,
                    &mut rig.speaker.channels.control,
                    &mut speaker_pending,
                    &mut provider_queue,
                    &mut admission,
                    &mut detector,
                    observation.muted,
                    observed_at_us,
                    monotonic_us,
                )?;
                snapshot = cancellation.snapshot;
                // A reply may already be complete while its independent text
                // socket still holds a "let me look" tail. Idle Stop also
                // fences that last known request, not just a currently owned one.
                if let Some(turn) = cancellation.cancelled_turn.or(highest_known_turn) {
                    hand_cancelled_through =
                        Some(hand_cancelled_through.map_or(turn, |old| old.max(turn)));
                }
                last_publish = monotonic_us();
                // No provider, optional presentation, filesystem or trace call
                // precedes speaker revocation. Capture is next for mute.
                let publish = rig.publish_hand_revocation(snapshot);
                submission_draining = admission.draining();
                look_wanted = None;
                answer_opening = None;
                second_look =
                    crate::second_look::SecondLook::default().with_the_note(crate::called::on());
                last_turn_us = monotonic_us();
                speech_ended = None;
                floor_held_for = None;
                asked_in_busy_room = None;
                busy_answer_since = None;
                let reason = if observation.muted {
                    "software_muted"
                } else {
                    "hand_stop"
                };
                record(
                    trace,
                    json!({"kind":"hand_control_cancel", "reason":reason,
                    "at_us":observed_at_us, "acquired_at_us":observation.acquired_at_us,
                    "speaker_told_at_us":cancellation.speaker_told_at_us,
                    "observation_to_speaker_dispatch_us":cancellation.speaker_told_at_us.saturating_sub(observed_at_us),
                    "boundary":"coordinator observation to local speaker control send; excludes bus delivery and acoustic stop"}),
                )?;
                if let Some(current) = cancellation.reply {
                    record_cancelled_reply(
                        current,
                        trace,
                        reason,
                        cancellation.speaker_told_at_us,
                    )?;
                }
                publish?;
                if observation.muted {
                    return Err(Box::new(SoftwareMute { observed_at_us }));
                }
            }
            record_hand_pats(trace, observation.pets)?;
        }
        let can_open = privacy_state == PrivacyState::Allowed;
        if let Some(reminders) = reminders.as_mut() {
            reminders.poll(monotonic_us());
            if reply.as_ref().is_some_and(|current| {
                current.reminder.is_some() && !reminders.active_valid(current.owner, monotonic_us())
            }) {
                cancel_reply(owner, reply, trace, "reminder_authority_lost")?;
                provider_queue.clear();
                speaker_pending = None;
                snapshot = owner.snapshot(now())?;
                rig.publish(snapshot)?;
                last_publish = monotonic_us();
            }
        }
        if permitted && !can_open {
            return Err(io::Error::other("physical privacy closed or lost freshness").into());
        }
        if can_open && !permitted {
            owner.set_microphone_permission(now(), Permission::Allowed)?;
            permitted = true;
            snapshot = owner.snapshot(now())?;
            rig.publish(snapshot)?;
        }
        service_camera(rig, owner, can_open, trace)?;
        // One pre-capture spawn; no initial denied authority is suppressed.
        if can_open
            && !capture_requested
            && let Some(motor) = &mut rig.motor
        {
            motor.start_before_capture(
                owner.snapshot(now())?,
                false,
                rig.camera.as_ref().is_some_and(|camera| !camera.failed),
            );
        }
        service_motor(rig, trace)?;
        // The original physical sample was drained immediately above. Recheck
        // its age before the optional operator channel; no new permission.
        service_head(
            rig,
            owner,
            gate.state(now()) == PrivacyState::Allowed,
            trace,
        )?;
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
        second::service(
            rig,
            &mut cloud,
            &mut output_flow,
            reply,
            &mut provider_queue,
            submission_draining,
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
            if hand::cancelled_output(&event, hand_cancelled_through) {
                continue;
            }
            if second::takes_failed_turn(
                rig,
                &mut cloud,
                &mut output_flow,
                reply,
                &mut provider_queue,
                &event,
                trace,
            )? {
                continue;
            }
            observe_provider_output(&mut cloud, reply, &event)?;
            match event {
                ProviderOutput::ReminderDeferred {
                    request,
                    session,
                    reason,
                } => {
                    if reply.as_ref().is_some_and(|current| {
                        current.reminder.is_some()
                            && current.owner.turn() == request
                            && current.cloud.session == session
                    }) {
                        // Keep the receipt out of Reply's destructor until the
                        // real owner has been revoked and published. No PCM
                        // ever received is the only automatic retry case.
                        let mut reminder =
                            reply.as_mut().and_then(|current| current.reminder.take());
                        cancel_reply(owner, reply, trace, "reminder_deferred")?;
                        provider_queue.clear();
                        speaker_pending = None;
                        snapshot = owner.snapshot(now())?;
                        rig.publish(snapshot)?;
                        if let Some(reminder) = &mut reminder {
                            reminder.deferred_before_audio();
                        }
                        last_publish = monotonic_us();
                    }
                    record(
                        trace,
                        json!({"kind":"reminder_deferred", "at_us":monotonic_us(),
                        "request":request,"provider_session":session,"reason":reason,
                        "acknowledged":false}),
                    )?;
                }
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
                } => {
                    // Words for an answer whose end was sent early are not
                    // the lamp's until it is sure the person has finished:
                    // held, and said then, or dropped with the request.
                    let held = match (early.as_mut(), request) {
                        (Some(early), Some(turn)) => {
                            early.keep_current(reply.as_ref().map(|current| current.owner.turn()));
                            early.hold_words(turn, &text, finished)
                        }
                        _ => false,
                    };
                    if !held && let Some(bus) = bus_out.as_mut() {
                        bus.speech(request, &text, finished, monotonic_us());
                    }
                    // The model keeping quiet aloud: an answer that opens
                    // with no word in it ("...") while none of it has been
                    // played is not an answer. It ends here, unheard, and
                    // counts as a turn the lamp did not answer.
                    if let Some(turn) = request {
                        if answer_words.map(|(of, _)| of) != Some(turn) {
                            answer_words = Some((turn, false));
                        }
                        let worded = text.chars().any(char::is_alphanumeric);
                        let had_word = answer_words.is_some_and(|(_, word)| word);
                        answer_words = Some((turn, had_word || worded));
                        // Nor is the name of a function said aloud where it
                        // should have been called.
                        let named = !had_word && crate::not_speech::names_a_function(&text);
                        if !had_word
                            && (named || (!worded && !text.trim().is_empty()))
                            && reply.as_ref().is_some_and(|current| {
                                current.owner.turn() == turn
                                    && (current.local_input_ended || held)
                                    && current.playback.is_none()
                            })
                            // Not yet sure the person has finished: noted,
                            // and it ends unheard at that moment unless they
                            // go on first.
                            && !(held
                                && early
                                    .as_mut()
                                    .is_some_and(|early| early.nothing_said(turn, named)))
                        {
                            if named {
                                record(
                                    trace,
                                    json!({"kind":"function_named","turn":turn,"at_us":monotonic_us()}),
                                )?;
                            }
                            let heard_on = reply.as_ref().map(|current| current.cloud.session);
                            cancel_reply(owner, reply, trace, "nothing_said")?;
                            // An answer with no word in it is nothing said:
                            // someone who called the lamp by name is owed
                            // the second look all the same.
                            if let Some(score) = second_look.finished(turn, true, monotonic_us()) {
                                if let Some(session) = heard_on {
                                    second_look.heard_on(session);
                                }
                                record(
                                    trace,
                                    json!({"kind":"second_look_due","turn":turn,"at_us":monotonic_us(),"name_score":score,"after":"nothing_said"}),
                                )?;
                            }
                            speaker_pending = None;
                            provider_queue.clear();
                            snapshot = owner.snapshot(now())?;
                            rig.publish(snapshot)?;
                            last_publish = monotonic_us();
                        }
                    }
                    record_look_intent(
                        looks,
                        request,
                        &text,
                        last_reminder_turn,
                        &mut answer_opening,
                        &mut look_wanted,
                        trace,
                    )?;
                    // The lamp's own words, as its answer begins: a nod
                    // with a yes, a shake with a no, when it has a body.
                    // Held words are not its own yet.
                    if !held
                        && let (Some(turn), Some(body)) = (request, body_link.as_mut())
                        && let Some(gesture) = gesture_cue.heard(turn, &text, finished)
                    {
                        body.gesture(gesture);
                        record(
                            trace,
                            json!({"kind":"body_gesture_asked","turn":turn,"gesture":gesture,"at_us":monotonic_us()}),
                        )?;
                    }
                    record(
                        trace,
                        json!({"kind":"transcript","turn":request,"text":text,"finished":finished,"at_us":monotonic_us(),"input_correlation":if request.is_none(){"unreliable; session scoped"}else{"output lineage"}}),
                    )?
                }
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
                ProviderOutput::OwnershipAssumed {
                    request,
                    superseded,
                } => {
                    record(
                        trace,
                        json!({"kind":"provider_ownership_assumed","at_us":monotonic_us(),
                        "turn":request,"superseded_turn":superseded,
                        "basis":"earlier response silent after it was asked to stop; not confirmed by the service"}),
                    )?;
                }
                ProviderOutput::InputDiscarded {
                    request,
                    dropped_samples,
                    sent_samples,
                } => {
                    record(
                        trace,
                        json!({"kind":"provider_input_discarded","at_us":monotonic_us(),
                        "through_turn":request,"dropped_unsent_samples":dropped_samples,
                        "already_sent_samples":sent_samples,
                        "boundary":"transport bookkeeping; samples already sent were not discarded and precede the next request in its activity"}),
                    )?;
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
        if session_clock.expired(session_instant()) {
            // Drain fault channels and inspect children before accepting the
            // deadline. A crashed or forcibly killed worker is never success.
            rig.running()?;
            record(
                trace,
                json!({"kind":"turn_taking_summary","at_us":monotonic_us(),"policy":rig.turn_taking,"suppressed_blocks":playback_mask.suppressed_blocks(),"tail_us":crate::turn_taking::TAIL_US,"acoustic_tail_measured":false}),
            )?;
            if let Some(((yields, commits, resumes), fallen_back)) = playback_mask.duplex_summary()
            {
                record(
                    trace,
                    json!({"kind":"duplex_summary","at_us":monotonic_us(),"yields":yields,"commits":commits,"resumes":resumes,
                    "fallen_back_at_end":fallen_back,"hearing_margin_db":playback_mask.hearing_margin_db(),
                    "margin_scope":"person in the clear against own voice, both before cancellation; not addressee evidence"}),
                )?;
            }
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
            if !session_clock.ready()
                && cloud.available()
                && provider_control_empty
                && !submission_draining
            {
                session_clock.activate(session_instant())?;
                // The link state readiness was just validated against, as a
                // current-state publication. Retained startup link records can
                // be older than the cue bound; they stay history, never re-stamped.
                record(
                    trace,
                    json!({"kind":"provider_link_current","at_us":at,"state":"ready",
                    "session":cloud.ready_session(),"basis":"coordinator link register at listening readiness"}),
                )?;
                record(
                    trace,
                    json!({"kind":"listening_ready","at_us":at,"scope":"explicit directed session; addressee inference absent"}),
                )?;
                println!(
                    "{}",
                    json!({"status":"listening_ready","seconds":lifetime.seconds()})
                );
            }
            let lineage = CaptureLineage {
                worker: rig.capture.boot,
                epoch: frame.epoch,
                dsp_epoch: frame.dsp_epoch,
                privacy_generation: frame.privacy_generation,
            };
            if capture_lineage.is_some_and(|old| old != lineage) {
                clear_capture_lineage(
                    &mut detector,
                    &mut playback_mask,
                    trace,
                    at,
                    frame.frame_sequence,
                    frame.read_completed_at_us,
                )?;
            }
            capture_lineage = Some(lineage);
            let context = AdmissionContext {
                capture: lineage,
                authority: owner.snapshot(now())?,
            };
            heard_level = crate::ring_face::level_of(&frame.samples);
            if let Some(mic) = mic_out.as_mut() {
                mic.block(&frame.samples, at);
            }
            // What is too faint to be someone at the lamp is not speech,
            // whatever the detector makes of it: on lamp-4ace a run of
            // "utterances" at the bottom of the scale, voices across the
            // office or its own motors, were sent to the provider, came
            // back as "¿Qué?", and were taken for the person at the lamp
            // going on or cutting in.
            // Nor is a voice over the first seconds of an answer the lamp
            // is giving to something asked in a busy room: it finishes what
            // it was asked, as a person among people talking would.
            let keeping_answer = reply.as_ref().is_some_and(|current| {
                busy_answer_since.is_some_and(|(turn, since)| {
                    turn == current.owner.turn()
                        && frame.read_completed_at_us.saturating_sub(since) < KEEP_ANSWER_US
                })
            });
            let faint = room_floor
                .as_mut()
                .map(|floor| floor.at(frame.read_completed_at_us));
            // Recorded when it moves a decibel, and when the level the
            // room alone would ask for does: with a fixed floor that second
            // number is what following the room would have done.
            let unbounded = room_floor.as_ref().and_then(|floor| floor.unbounded());
            if let Some(faint) = faint
                && floor_recorded.is_none_or(|(last, last_unbounded)| {
                    last.abs_diff(faint) >= 8
                        || match (last_unbounded, unbounded) {
                            (Some(before), Some(now)) => before.abs_diff(now) >= 8,
                            (None, None) => false,
                            _ => true,
                        }
                })
            {
                floor_recorded = Some((faint, unbounded));
                let dbfs = |level: u8| -48.0 + f32::from(level) * 34.0 / 255.0;
                record(
                    trace,
                    json!({"kind":"room_floor","at_us":frame.read_completed_at_us,
                        "dbfs":dbfs(faint),"room_alone_dbfs":unbounded.map(dbfs)}),
                )?;
            }
            // What the detector takes for speech while the lamp is silent,
            // kept until it is known whether it was for the lamp: a turn's
            // own blocks with the turn, the rest for a moment in case a
            // turn is about to begin with them.
            if room_floor.is_some()
                && frame.probability.is_finite()
                && frame.probability >= 0.5
                && reply
                    .as_ref()
                    .is_none_or(|current| current.playback.is_none())
            {
                if reply
                    .as_ref()
                    .is_some_and(|current| !current.local_input_ended)
                {
                    if turn_blocks.len() < 6_000 {
                        turn_blocks.push(heard_level);
                    }
                } else {
                    loose_blocks.push_back((frame.read_completed_at_us, heard_level));
                }
            }
            while let Some((at, level)) = loose_blocks.front().copied() {
                if frame.read_completed_at_us.saturating_sub(at) < 400_000 {
                    break;
                }
                loose_blocks.pop_front();
                if let Some(floor) = room_floor.as_mut() {
                    floor.not_for_the_lamp(level);
                }
            }
            let under_floor = faint
                .is_some_and(|faint| !near.hears(heard_level, faint, frame.read_completed_at_us));
            if judged_blocks.len() == 200 {
                judged_blocks.pop_front();
            }
            judged_blocks.push_back((
                (frame.probability.clamp(0.0, 1.0) * 20.0).round() as u8,
                heard_level,
                under_floor,
            ));
            let probability = if keeping_answer || under_floor {
                0.0
            } else {
                frame.probability
            };
            let heard = hear_capture_block(
                &mut playback_mask,
                reply,
                &mut heard_reply,
                frame.frame_sequence,
                frame.read_completed_at_us,
                probability,
                frame.echo.as_ref(),
                monotonic_us(),
                trace,
            )?;
            // How loud what the detector calls speech has been lately, for
            // the record of each utterance: its last eight seconds.
            if probability.is_finite() && probability >= 0.5 {
                speech_levels.push_back((frame.read_completed_at_us, heard_level));
                last_speech_us = frame.read_completed_at_us;
            }
            while speech_levels
                .front()
                .is_some_and(|(at, _)| frame.read_completed_at_us.saturating_sub(*at) > 8_000_000)
            {
                speech_levels.pop_front();
            }
            let observation = ObservedAudio {
                sequence: frame.frame_sequence,
                captured_at_us: frame.read_completed_at_us,
                samples: frame
                    .samples
                    .try_into()
                    .map_err(|_| io::Error::other("invalid capture block"))?,
            };
            // What was kept for an end sent early goes with its request,
            // whatever ended that request.
            if let Some(early) = early.as_mut() {
                early.keep_current(reply.as_ref().map(|current| current.owner.turn()));
            }
            let observed_at_us = monotonic_us();
            let (update, input_ended) = observe_prospective_input(
                &mut playback_mask,
                &mut detector,
                &mut admission,
                context,
                observation,
                frame.timing.first_read_started_at_us,
                heard,
                observed_at_us,
            )?;
            check_speaker_progress(speaker_pending.as_ref(), observed_at_us)?;
            let discarded_endpoint = submission_draining && input_ended;
            if discarded_endpoint {
                submission_draining = false;
                // The utterance whose rest was discarded has ended; nothing
                // kept for an end sent early in it is wanted any more.
                if let Some(early) = early.as_mut() {
                    early.forget();
                }
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
                    // In a room busy with other people's talk, what the
                    // lamp has just been asked is not dropped for the next
                    // voice: until its answer begins, or 1.8 s have gone
                    // without one, a new sound does not take the turn.
                    let at_us = monotonic_us();
                    let holding = room.busy()
                        && reply.as_ref().is_some_and(|current| {
                            let turn = current.owner.turn();
                            current.local_input_ended
                                && current.playback.is_none()
                                && if current.had_audio {
                                    current
                                        .last_provider_audio_at_us
                                        .is_some_and(|at| at_us.saturating_sub(at) < HOLD_FLOOR_US)
                                } else {
                                    speech_ended.is_some_and(|(ended, at)| {
                                        ended == turn && at_us.saturating_sub(at) < HOLD_FLOOR_US
                                    })
                                }
                        });
                    if paused {
                        // Paused by the keeper: a new sound takes no turn,
                        // whether or not one is under way; that one goes on.
                        if let Some(rejection) = admission.release_prospective(monotonic_us())? {
                            record_input_rejection(trace, rejection)?;
                        }
                        detector.clear_prospective();
                        if !refused_while_paused {
                            refused_while_paused = true;
                            record(
                                trace,
                                json!({"kind":"input_not_taken_paused","at_us":at_us}),
                            )?;
                        }
                        continue;
                    }
                    if holding {
                        // Read the clock again: the admission's own must
                        // not be handed a time before its last.
                        if let Some(rejection) = admission.release_prospective(monotonic_us())? {
                            record_input_rejection(trace, rejection)?;
                        }
                        detector.clear_prospective();
                        let turn = reply.as_ref().map(|current| current.owner.turn());
                        if floor_held_for != turn {
                            floor_held_for = turn;
                            record(
                                trace,
                                json!({"kind":"floor_held","turn":turn,"at_us":at_us,
                                    "boundary":"a new sound did not take the turn while an answer was awaited in a busy room"}),
                            )?;
                        }
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
                    // Taking over from the lamp's own answer is cutting in,
                    // and taking over from something that was said is going
                    // on with it. Taking over from a sound is neither.
                    let took_over = reply.as_ref().is_some_and(|current| {
                        crate::not_speech::takes_over(
                            current.had_audio || current.playback.is_some(),
                            said_turn == Some(current.owner.turn()),
                        )
                    });
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
                    if took_over {
                        took_over_turn = reply.as_ref().map(|current| current.owner.turn());
                    }
                    if let Some(current) = reply.as_ref() {
                        second_look.request_began(current.owner.turn(), monotonic_us());
                    }
                    // The sound just before a turn began is the turn's.
                    turn_blocks.extend(loose_blocks.drain(..).map(|(_, level)| level));
                }
                AdmissionStep::Audio(audio) => {
                    let Some(current) = reply.as_mut() else {
                        return Err(io::Error::other("speech continuation lost its owner").into());
                    };
                    let turn = current.owner.turn();
                    let mut resumed = None;
                    if early.as_ref().is_some_and(|early| early.holds(turn)) {
                        // The end was sent early: nothing more goes to the
                        // provider for this request. Someone speaking again
                        // gives it up, and what they go on to say is its
                        // successor.
                        if let Some(early) = early.as_mut()
                            && let HeardAfterEnd::Resumed { given_up, opening } =
                                early.hear(audio, heard)
                        {
                            resumed = Some((given_up, opening));
                        }
                    } else {
                        let captured_at_us = audio.captured_at_us;
                        queue_input(
                            &mut provider_queue,
                            current,
                            snapshot.microphone_generation(),
                            audio,
                            provider_queue_bound,
                        )?;
                        // The end sent early, after a short quiet. Not for
                        // what would be thrown out as a sound if the quiet
                        // went on: that is judged when the lamp is sure, as
                        // it always was, so that a lone "Lamp," is never
                        // thrown out for a pause before its question.
                        if let Some(early) = early.as_mut()
                            && early.due(turn, detector.pause_blocks())
                            && !crate::not_speech::was_a_sound(
                                speech_levels.len(),
                                took_over_turn == Some(turn),
                                last_spoke_us.map(|at| monotonic_us().saturating_sub(at)),
                            )
                        {
                            let sent_at = monotonic_us();
                            queue_input_end(
                                &mut provider_queue,
                                current,
                                snapshot.microphone_generation(),
                                provider_queue_bound,
                                false,
                            )?;
                            early.send(turn, sent_at, captured_at_us);
                            // The request's sound has ended, for the floor,
                            // the second look and the room. The lamp's own
                            // authority still says it is listening.
                            speech_ended = Some((turn, sent_at));
                            second_look.speech_ended(turn, sent_at);
                            if speech_levels.len() >= crate::not_speech::SCRAP_BLOCKS {
                                said_turn = Some(turn);
                                // Someone was there, for the news: the
                                // confirmation later skips the usual place
                                // (found by the third builder on #709).
                                said_at_us = Some(sent_at);
                            }
                            asked_in_busy_room = room.busy().then_some(turn);
                            record(
                                trace,
                                json!({"kind":"early_end","owner":current.owner,"turn":turn,"at_us":sent_at,
                                    "last_block_host_read_us":captured_at_us,
                                    "pause_ms":u64::from(early.blocks()) * 10,
                                    "speech_blocks":speech_levels.len(),
                                    "boundary":"request ended to the provider queue; not yet the lamp's end of speech, which still decides playback"}),
                            )?;
                        }
                    }
                    if let Some((given_up, opening)) = resumed {
                        // Whether the provider can take the successor now,
                        // checked as for any new input.
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
                        let took_over = reply.as_ref().is_some_and(|current| {
                            crate::not_speech::takes_over(
                                current.had_audio || current.playback.is_some(),
                                said_turn == Some(current.owner.turn()),
                            )
                        });
                        let resumed_at = monotonic_us();
                        let opening_blocks = opening.len();
                        match cloud
                            .ready_session()
                            .filter(|_| provider_control_empty && !submission_draining)
                        {
                            Some(provider_session) => {
                                snapshot = continue_after_early_end(
                                    owner,
                                    reply,
                                    &mut speaker_pending,
                                    &mut provider_queue,
                                    opening,
                                    provider_session,
                                    rig.playout_start,
                                    trace,
                                    |state| rig.publish(state),
                                )?;
                                if took_over {
                                    took_over_turn =
                                        reply.as_ref().map(|current| current.owner.turn());
                                }
                                if let Some(current) = reply.as_ref() {
                                    second_look.request_began(current.owner.turn(), monotonic_us());
                                }
                            }
                            None => {
                                // No provider to take it: given up all the
                                // same, and the rest of the utterance is
                                // drained, as for any input it cannot take.
                                let displaced = reply.as_ref().map(|current| current.owner);
                                cancel_reply(owner, reply, trace, "user_interrupted")?;
                                admission.discard_stream_until_end(monotonic_us())?;
                                submission_draining = true;
                                provider_queue.clear();
                                speaker_pending = None;
                                record(
                                    trace,
                                    json!({"kind":"input_not_submitted","at_us":monotonic_us(),
                                    "candidate":null,"admission_basis":"continued_after_early_end",
                                    "interrupted_owner":displaced,"reason":"provider_unavailable_or_pending_control",
                                    "provider_request":null,"provider_session":null,
                                    "outcome":RequestOutcome::Failed { stage:crate::provider_worker::FailedStage::NotDelivered,
                                        reason:crate::provider_worker::Fault::Local, local_not_submitted:true },
                                    "prefix_first_host_read_us":null}),
                                )?;
                                snapshot = owner.snapshot(now())?;
                                rig.publish(snapshot)?;
                            }
                        }
                        last_publish = monotonic_us();
                        record(
                            trace,
                            json!({"kind":"early_end_given_up","turn":given_up.turn,"at_us":resumed_at,
                                "early_end_at_us":given_up.at_us,"after_early_end_ms":given_up.ago_us(resumed_at) / 1_000,
                                "successor":reply.as_ref().map(|current| current.owner.turn()),
                                "opening_blocks":opening_blocks,
                                "boundary":"someone spoke again before the lamp was sure: given up as a continued utterance"}),
                        )?;
                    }
                }
                AdmissionStep::End(audio)
                    if reply.as_ref().is_some_and(|current| {
                        crate::not_speech::was_a_sound(
                            speech_levels.len(),
                            took_over_turn == Some(current.owner.turn()),
                            last_spoke_us.map(|at| monotonic_us().saturating_sub(at)),
                        )
                    }) && !crate::not_speech::sent_anyway(short_sounds, room.busy()) =>
                {
                    // A sound, not something said. It is not sent on as a
                    // finished question: the turn ends here, unanswered.
                    if let Some(current) = reply.as_ref() {
                        let mut endpoint_record = json!({"kind":"local_endpoint","owner":current.owner,"turn":current.owner.turn(),"at_us":at,
                                "last_block_host_read_us":audio.captured_at_us,
                                "includes_silence_wait_ms":detector.last_endpoint().map_or(detector.endpoint_silence_ms(), |decision| decision.silence_ms),
                                "acoustic_speech_end":null,
                                "not_speech":{"speech_blocks":speech_levels.len(),"fewer_than":crate::not_speech::SCRAP_BLOCKS,
                                    "last_blocks":last_blocks(&judged_blocks)}});
                        // Its end went early, while it was not yet a sound
                        // (the lamp's own words were still near); its
                        // answer is never played.
                        if let Some(sent) = early
                            .as_mut()
                            .and_then(|early| early.confirm(current.owner.turn()))
                        {
                            endpoint_record["early_end"] = early_end_json(&sent, at);
                        }
                        record(trace, endpoint_record)?;
                    }
                    speech_levels.clear();
                    cancel_reply(owner, reply, trace, "not_speech")?;
                    speaker_pending = None;
                    provider_queue.clear();
                    snapshot = owner.snapshot(now())?;
                    rig.publish(snapshot)?;
                    last_publish = monotonic_us();
                }
                AdmissionStep::End(audio) => {
                    let current = reply
                        .as_mut()
                        .ok_or_else(|| io::Error::other("speech end lost its owner"))?;
                    let input_end_us = audio.captured_at_us;
                    // The lamp is sure the person has finished. A request
                    // whose end went early sends nothing more: what it
                    // waited for was this moment, and from here its answer
                    // may be played.
                    let sent = early
                        .as_mut()
                        .and_then(|early| early.confirm(current.owner.turn()));
                    current.local_input_ended = true;
                    if sent.is_none() {
                        speech_ended = Some((current.owner.turn(), monotonic_us()));
                        second_look.speech_ended(current.owner.turn(), monotonic_us());
                        if speech_levels.len() >= crate::not_speech::SCRAP_BLOCKS {
                            said_turn = Some(current.owner.turn());
                            said_at_us = Some(monotonic_us());
                        } else if crate::not_speech::was_a_sound(
                            speech_levels.len(),
                            took_over_turn == Some(current.owner.turn()),
                            last_spoke_us.map(|at| monotonic_us().saturating_sub(at)),
                        ) {
                            // Let through only for the model to judge
                            // (`LAMP_SHORT_SOUNDS`): unanswered, it is no one
                            // else's talk.
                            room.not_counted(current.owner.turn());
                            sound_turn = Some(current.owner.turn());
                        }
                        asked_in_busy_room = room.busy().then(|| current.owner.turn());
                        queue_input(
                            &mut provider_queue,
                            current,
                            snapshot.microphone_generation(),
                            audio,
                            provider_queue_bound,
                        )?;
                    }
                    owner.input_ended(now(), current.owner)?;
                    snapshot = owner.snapshot(now())?;
                    rig.publish(snapshot)?;
                    last_publish = monotonic_us();
                    let as_sound = sound_turn == Some(current.owner.turn());
                    if sent.is_none() {
                        queue_input_end(
                            &mut provider_queue,
                            current,
                            snapshot.microphone_generation(),
                            provider_queue_bound,
                            as_sound,
                        )?;
                    }
                    let mut endpoint_record = json!({"kind":"local_endpoint","owner":current.owner,"turn":current.owner.turn(),"at_us":at,"last_block_host_read_us":input_end_us,
                        "includes_silence_wait_ms":detector.last_endpoint().map_or(detector.endpoint_silence_ms(), |decision| decision.silence_ms),
                        "acoustic_speech_end":null});
                    if as_sound {
                        endpoint_record["let_through_as_sound"] = json!(true);
                    }
                    // Baseline records keep exactly their existing fields.
                    if detector.confirmation_policy().is_some() {
                        endpoint_record["endpoint"] = json!(detector.last_endpoint());
                    }
                    if let Some(sent) = sent.as_ref() {
                        endpoint_record["early_end"] = early_end_json(sent, at);
                    }
                    let confirmed = sent.map(|sent| (current.owner.turn(), sent));
                    // How loud this utterance was at the microphone after
                    // cancellation, on the face's scale (0 is -48 dBFS or
                    // under, 255 is -14 or over): the middle and the upper
                    // tenth of its speech blocks. For telling someone at
                    // the lamp from someone across the room.
                    if !speech_levels.is_empty() {
                        let mut levels: Vec<u8> =
                            speech_levels.iter().map(|(_, level)| *level).collect();
                        levels.sort_unstable();
                        endpoint_record["speech_level"] = json!({
                            "blocks": levels.len(),
                            "median": levels[levels.len() / 2],
                            "upper_tenth": levels[levels.len() * 9 / 10],
                        });
                        // A short utterance that was taken as said carries
                        // how its blocks were judged, as one thrown out as
                        // a sound does: what tells a short word from a
                        // sound is to be read from the two side by side.
                        if levels.len() < crate::not_speech::SHORT_BLOCKS {
                            endpoint_record["last_blocks"] = last_blocks(&judged_blocks);
                        }
                        speech_levels.clear();
                    }
                    record(trace, endpoint_record)?;
                    if let Some((turn, mut sent)) = confirmed {
                        // The words held for its answer are the lamp's now.
                        for (text, finished) in sent.take_words() {
                            if let Some(bus) = bus_out.as_mut() {
                                bus.speech(Some(turn), &text, finished, monotonic_us());
                            }
                            if let Some(body) = body_link.as_mut()
                                && let Some(gesture) = gesture_cue.heard(turn, &text, finished)
                            {
                                body.gesture(gesture);
                                record(
                                    trace,
                                    json!({"kind":"body_gesture_asked","turn":turn,"gesture":gesture,"at_us":monotonic_us()}),
                                )?;
                            }
                        }
                        // An answer that opened with nothing said ends
                        // unheard now, as it would have when it came.
                        if let Some(named) = sent.nothing_said()
                            && reply.as_ref().is_some_and(|current| {
                                current.owner.turn() == turn && current.playback.is_none()
                            })
                        {
                            if named {
                                record(
                                    trace,
                                    json!({"kind":"function_named","turn":turn,"at_us":monotonic_us()}),
                                )?;
                            }
                            let heard_on = reply.as_ref().map(|current| current.cloud.session);
                            cancel_reply(owner, reply, trace, "nothing_said")?;
                            if let Some(score) = second_look.finished(turn, true, monotonic_us()) {
                                if let Some(session) = heard_on {
                                    second_look.heard_on(session);
                                }
                                record(
                                    trace,
                                    json!({"kind":"second_look_due","turn":turn,"at_us":monotonic_us(),"name_score":score,"after":"nothing_said"}),
                                )?;
                            }
                            speaker_pending = None;
                            provider_queue.clear();
                            snapshot = owner.snapshot(now())?;
                            rig.publish(snapshot)?;
                            last_publish = monotonic_us();
                        }
                    }
                }
            }
        }
        if let Some(at) = last_capture
            && monotonic_us().saturating_sub(at) >= INPUT_LEASE_US
        {
            return Err(io::Error::other("microphone retention heartbeat expired").into());
        }
        let monitor = rig.endpoint.as_ref().and_then(|e| e.monitor.as_ref());
        if let Some(event) = endpoint_watch.observe(&detector, monitor) {
            record(trace, event)?;
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
        if let Some(reminders) = reminders.as_mut()
            && reply.as_ref().is_some_and(|current| {
                current.reminder.is_some() && !reminders.active_valid(current.owner, monotonic_us())
            })
        {
            cancel_reply(owner, reply, trace, "reminder_lease_expired")?;
            provider_queue.clear();
            speaker_pending = None;
            snapshot = owner.snapshot(now())?;
            rig.publish(snapshot)?;
            last_publish = monotonic_us();
        }
        if provider_control_empty {
            second::flush(rig, &cloud, reply, &mut provider_queue)?;
        }
        select_failure_notice(notices, reply, &speaker_pending, trace)?;
        let prior_speaker_sequence = speaker_sequence;
        settle_duplex_yield_before_dispatch(&playback_mask, reply, monotonic_us(), trace)?;
        let notice_attempt = dispatch_speaker_chunk(
            owner,
            reply,
            &mut rig.speaker.channels.data,
            &mut speaker_pending,
            &mut speaker_sequence,
            &mut snapshot,
            &mut playback_mask,
        )?;
        if let Some((notice_owner, evidence)) = notice_attempt {
            record(
                trace,
                json!({"kind":"failure_notice_attempted","at_us":monotonic_us(),
                "owner":notice_owner,"turn":notice_owner.turn(),"notice":evidence,
                "boundary":"reserved before speaker transport; not audible delivery"}),
            )?;
        }
        if speaker_sequence != prior_speaker_sequence {
            let pending = speaker_pending
                .as_ref()
                .ok_or_else(|| io::Error::other("dispatched speech has no pending receipt"))?;
            trace
                .receipts
                .dispatched(pending.permit.owner(), pending.sequence)?;
        }
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
            && current.settled(&speaker_pending)
        {
            let turn = current.owner.turn();
            owner.complete_turn(now(), current.owner)?;
            trace.receipts.completed(current.owner)?;
            snapshot = owner.snapshot(now())?;
            admission.owner_completed(current.owner, snapshot, monotonic_us())?;
            rig.publish(snapshot)?;
            last_publish = monotonic_us();
            current.complete_reminder(&speaker_pending, snapshot);
            record(
                trace,
                json!({"kind":"turn_finished","turn":turn,"generation":current.owner.generation(),"at_us":monotonic_us(),"outcome":current.delivery_outcome(),"playback_gaps":current.playback_gaps,
                    "provider_session":current.cloud.session,"provider_outcome":current.cloud.outcome,"failure_notice":current.notice.evidence(),
                "start_gate":current.start_trace(),"start_deficit_us":current.need.deficit_us(),
                "start_horizon_needed_us":current.need.horizon_us(),
                "start_need_scope":"all retained reply arrivals including later generations; excludes local handoff",
                "start_need_censored":!matches!(current.cloud.outcome,Some(RequestOutcome::Complete))}),
            )?;
            finish_duplex_yield(trace, monotonic_us(), "reply_completed", None, None)?;
            // Said to the lamp by name, and nothing came of it: the model
            // is told what the lamp's own ears heard, once.
            let said_nothing = current.delivery_outcome() == "no_audio_answer"
                && matches!(current.cloud.outcome, Some(RequestOutcome::Complete));
            if let Some(score) = second_look.finished(turn, said_nothing, monotonic_us()) {
                second_look.heard_on(current.cloud.session);
                record(
                    trace,
                    json!({"kind":"second_look_due","turn":turn,"at_us":monotonic_us(),"name_score":score}),
                )?;
            }
            *reply = None;
        }
        // Terminal priority status can overtake valid PCM/outcomes. Only the
        // ordered Ended closes that stream; a submitted Start may still receive
        // its local NotDelivered afterwards. Keep serving capture and speaker
        // receipts until the owned input/output obligations actually settle.
        if lifetime.continuous()
            && cloud.output_ended()
            && reply.is_none()
            && provider_queue.is_empty()
            && !submission_draining
            && speaker_pending.is_none()
            && trace.receipts.is_settled()
        {
            record(
                trace,
                json!({"kind":"service_provider_ended","at_us":monotonic_us(),
                    "ordered_output_ended":true,"retained_work_settled":true,
                    "automatic_retry":false}),
            )?;
            flush_cues(trace, cues);
            if cloud.refused_for_now() {
                return Err(ProviderRefusesForNow.into());
            }
            return Err(io::Error::other(
                "terminal provider output settled; service stopped without retry",
            )
            .into());
        }
        // A lamp that has stopped speaking for someone looks at them: the
        // listening face and pose at once, whatever becomes of the yield.
        let attending = playback_mask.yielded(clock);
        // The lamp's own voice while it speaks, the person's otherwise.
        let voice_level = match reply.as_ref() {
            Some(current) if current.playback.is_some() && !attending => current.voice_level,
            _ => heard_level,
        };
        if let Some(choreography) = choreography.as_mut() {
            choreography.set_level(voice_level);
            choreography.set_attending(attending);
        }
        if let Some(current) = reply.as_ref().filter(|current| current.playback.is_some()) {
            let turn = current.owner.turn();
            if asked_in_busy_room == Some(turn) && busy_answer_since.map(|(of, _)| of) != Some(turn)
            {
                busy_answer_since = Some((turn, monotonic_us()));
                record(
                    trace,
                    json!({"kind":"answer_kept","turn":turn,"at_us":monotonic_us(),"for_us":KEEP_ANSWER_US,
                        "boundary":"asked in a busy room: a voice over the start of this answer does not stop it"}),
                )?;
            }
            last_spoke_us = Some(monotonic_us());
            spoke_in_turn = Some(turn);
        }
        // A turn that ended by itself with the lamp saying nothing was the
        // room's talk, and tells the floor how loud the room is. One the
        // lamp answered does not, nor one replaced by more talk: that may
        // be a person pausing in what they are saying to the lamp.
        let turn_now = reply.as_ref().map(|current| current.owner.turn());
        if floor_turn != turn_now {
            if let (Some(ended), Some(floor)) = (floor_turn, room_floor.as_mut())
                && spoke_in_turn != Some(ended)
                && turn_now.is_none()
            {
                for level in turn_blocks.drain(..) {
                    floor.not_for_the_lamp(level);
                }
            }
            if turn_now.is_none() {
                turn_blocks.clear();
            }
            floor_turn = turn_now;
        }
        // For the room, a turn is answered when the lamp was heard in it:
        // an answer that arrived and was never played is not one.
        room.observe(
            reply.as_ref().map(|current| {
                let turn = current.owner.turn();
                (turn, spoke_in_turn == Some(turn))
            }),
            clock,
        );
        if let Some(bus) = bus_out.as_mut() {
            bus.state(crate::ring_wire::RingPhase::from_snapshot(snapshot), clock);
            bus.report(cloud.available(), clock);
        }
        if let Some(mic) = mic_out.as_mut() {
            mic.tick(clock);
        }
        let taken = playback_mask.take_taken();
        if let Some(out) = expression_out.as_mut() {
            // The first sound of an answer that has come and is not yet
            // playing: when its start will open, and the speaker's queue
            // after that.
            let first_sound_us = reply
                .as_ref()
                .filter(|current| current.playback.is_none() && current.local_input_ended)
                .and_then(|current| {
                    current
                        .start
                        .opens_by(clock, current.pcm.len(), current.generation_done)
                })
                .map(|opens| opens.saturating_add(out.ahead_us()));
            out.tick(crate::expression_out::Seen {
                now_us: clock,
                turn: reply.as_ref().map(|current| current.owner.turn()),
                phase: crate::ring_wire::RingPhase::from_snapshot(snapshot),
                accepting_input: snapshot.listening_ready(now()),
                yielding: attending,
                person_speaking: clock.saturating_sub(last_speech_us)
                    < crate::expression_out::SPEECH_LASTS_US,
                first_sound_us,
            });
            for piece in taken {
                out.played(
                    piece.turn,
                    piece.frames,
                    piece.queued_frames,
                    piece.queue_observed_at_us,
                    piece.level,
                    clock,
                );
            }
            // What is held before the speaker plays next: said ahead, so
            // that what the lamp shows can move with it.
            if let Some(current) = reply.as_ref().filter(|current| {
                current.playback.is_some() && !attending && out.foretell_due(clock)
            }) {
                let ahead: Vec<i16> = current
                    .pcm
                    .iter()
                    .take(crate::expression_out::foretell_samples())
                    .copied()
                    .collect();
                out.foretell(current.owner.turn(), &ahead, clock);
            }
        }
        if let Some(body) = body_link.as_mut() {
            use crate::ring_wire::RingPhase;
            let phase = RingPhase::from_snapshot(snapshot);
            // The answer has arrived and is about to be spoken: the head
            // comes back to the person before the first word, not during it.
            let about_to_speak = reply
                .as_ref()
                .is_some_and(|current| current.had_audio && current.local_input_ended);
            attention.observe(
                reply
                    .as_ref()
                    .map(|current| (current.owner.turn(), current.had_audio)),
                clock,
            );
            body.tell(
                match phase {
                    Some(RingPhase::Speaking) if attending => Some(RingPhase::Listening),
                    Some(RingPhase::Waiting) if about_to_speak => Some(RingPhase::Speaking),
                    // A sound sent on for the model to judge is not something
                    // to think about: the head stays as it was while the
                    // sound was heard, and an answer brings it back.
                    Some(RingPhase::Waiting)
                        if sound_turn.is_some()
                            && sound_turn == reply.as_ref().map(|current| current.owner.turn()) =>
                    {
                        None
                    }
                    // The talk around it has not been for it: the head
                    // stays where it is until the lamp has something to say.
                    Some(RingPhase::Listening | RingPhase::Waiting) if attention.settled() => None,
                    phase => phase,
                },
                voice_level,
                clock,
            );
        }
        if let Some(eyes) = eyes_link.as_mut() {
            eyes.permit(
                snapshot.microphone_permission() == Permission::Allowed,
                clock,
            );
        }
        // The one thing the lamp says unasked. Only from rest: nothing
        // being heard, nothing owed, a provider ready to take the turn.
        if reply.is_some() || admission.owns_stream() {
            last_turn_us = clock;
        }
        // The look the lamp said it would take, as soon as it has finished
        // saying so. Someone speaking first takes the turn and the look is
        // dropped; so is one that could not begin within fifteen seconds of being asked for.
        if let Some((turn, asked_us)) = look_wanted {
            match reply.as_ref().map(|current| current.owner.turn()) {
                Some(current) if current == turn => {}
                Some(_) => look_wanted = None,
                None if clock.saturating_sub(asked_us) > 15_000_000 => look_wanted = None,
                None => {
                    if provider_queue.is_empty()
                        && !admission.owns_stream()
                        && !submission_draining
                        && snapshot.listening_ready(now())
                        && let Some(provider_session) = unasked_session(&cloud, paused)
                    {
                        look_wanted = None;
                        snapshot = commit_written_turn(
                            owner,
                            reply,
                            &mut provider_queue,
                            crate::speak_up::LOOK_NOTE,
                            true,
                            "lamp_looks",
                            provider_session,
                            rig.playout_start,
                            trace,
                            |state| rig.publish(state),
                        )?;
                        last_publish = monotonic_us();
                    }
                }
            }
        }
        if let Some(names) = name_in.as_mut()
            && let Some((began, score)) = names.newest_scored(clock)
        {
            second_look.name_heard(began, score);
        }
        // A second look at something said to the lamp by name that got
        // nothing, as soon as the lamp is at rest. Someone who has spoken
        // since is being listened to, and it was dropped for them.
        if let Some(turn) = second_look.due(clock)
            && reply.is_none()
            && provider_queue.is_empty()
            && !admission.owns_stream()
            && !submission_draining
            && snapshot.listening_ready(now())
            && let Some(provider_session) = unasked_session(&cloud, paused)
        {
            let heard_it = second_look.is_for(provider_session);
            second_look.taken();
            if heard_it {
                eprintln!(
                    "lamp-voice: request {turn} was said to the lamp by name and got nothing: the model is told its name was heard"
                );
                snapshot = commit_written_turn(
                    owner,
                    reply,
                    &mut provider_queue,
                    crate::second_look::NOTE,
                    false,
                    "lamp_looks_again",
                    provider_session,
                    rig.playout_start,
                    trace,
                    |state| rig.publish(state),
                )?;
                last_publish = monotonic_us();
            } else {
                // The note speaks of what was just said, and this session
                // was not there for it.
                eprintln!(
                    "lamp-voice: request {turn} was said to the lamp by name and got nothing, and the session that heard it has gone: nothing is asked"
                );
                record(
                    trace,
                    json!({"kind":"second_look_dropped","turn":turn,"at_us":monotonic_us(),"why":"session_gone"}),
                )?;
            }
        }
        // Finish the camera/name follow-ups owed above before a reminder can
        // take the next turn. A reminder is not someone speaking first.
        let reminder_now_us = if reminders.is_some() {
            monotonic_us()
        } else {
            clock
        };
        if let Some(reminders) = reminders.as_mut()
            && reply.is_none()
            && provider_queue.is_empty()
            && speaker_pending.is_none()
            && admission.idle_for_reminder()
            && !submission_draining
            && snapshot.listening_ready(MonoTime::from_micros(reminder_now_us))
            && reminder_quiet(
                reminder_now_us,
                reminder_cycle_started_us,
                last_speech_us,
                last_capture,
            )
            && playback_mask.available(reminder_now_us)
            && let Some(provider_session) = unasked_session(&cloud, paused)
            && let Some(offer) = idle_reminder_offer(
                reminders,
                reply,
                look_wanted,
                &mut second_look,
                reminder_now_us,
            )
        {
            snapshot = commit_reminder_turn(
                owner,
                reply,
                &mut provider_queue,
                offer,
                reminders,
                provider_session,
                rig.playout_start,
                trace,
                |state| rig.publish(state),
            )?;
            last_reminder_turn = reply.as_ref().map(|current| current.owner.turn());
            last_publish = monotonic_us();
        }
        if let Some(arrival) = arrival.as_mut() {
            arrival.watch(clock);
        }
        if let Some(arrival) = arrival.as_mut()
            && arrival.due(clock.saturating_sub(last_turn_us), clock)
            && reply.is_none()
            && provider_queue.is_empty()
            && !admission.owns_stream()
            && !submission_draining
            && snapshot.listening_ready(now())
            && let Some(provider_session) = unasked_session(&cloud, paused)
        {
            arrival.said(clock);
            if arrival.speaks() {
                // By name, for someone who asked the lamp to know their face.
                let hello = crate::known_faces::known_from_environment()
                    .map(|name| crate::known_faces::hello(&name));
                snapshot = commit_written_turn(
                    owner,
                    reply,
                    &mut provider_queue,
                    hello.as_deref().unwrap_or(crate::speak_up::NOTE),
                    false,
                    "lamp_speaks_first",
                    provider_session,
                    rig.playout_start,
                    trace,
                    |state| rig.publish(state),
                )?;
                last_publish = monotonic_us();
            } else {
                eprintln!(
                    "lamp-voice: someone has sat down; a hello was due and is not switched on"
                );
                record(
                    trace,
                    json!({"kind":"hello_due","at_us":clock,"said":false,"provider_session":provider_session}),
                )?;
            }
        }
        // News from an agent, told the way a chief of staff would: when
        // someone is at the lamp (its eyes say so, or a person said
        // something a moment ago; a bump is not a person, and a restart is
        // not someone speaking: on lamp-4ace on 2026-10-15 the first build
        // took the start of the service for it and told the empty office at
        // 00:49:48), nothing has been said for a little while, nothing is
        // owed, and the room is not busy with other talk.
        if let Some(news) = task_news.as_mut()
            && news.waiting().is_some()
            && clock.saturating_sub(last_turn_us) >= crate::task_news::QUIET_US
            && (arrival
                .as_ref()
                .is_some_and(|arrival| arrival.present(clock))
                || said_at_us
                    .is_some_and(|at| clock.saturating_sub(at) < crate::task_news::HEARD_WITHIN_US))
            && !room.busy()
            && reply.is_none()
            && provider_queue.is_empty()
            && !admission.owns_stream()
            && !submission_draining
            && snapshot.listening_ready(now())
            && let Some(provider_session) = unasked_session(&cloud, paused)
            && let Some(told) = news.tell()
        {
            eprintln!(
                "lamp-voice: telling news from the work handed over ({})",
                told.key
            );
            record(
                trace,
                json!({"kind":"task_news","at_us":clock,"key":told.key,"provider_session":provider_session}),
            )?;
            snapshot = commit_written_turn(
                owner,
                reply,
                &mut provider_queue,
                &told.note,
                false,
                "lamp_tells_news",
                provider_session,
                rig.playout_start,
                trace,
                |state| rig.publish(state),
            )?;
            last_publish = monotonic_us();
        }
        service_optional_ring(rig, &mut choreography, owner, trace, &mut ring_trace)?;
        if clock.saturating_sub(last_health) >= 50_000 {
            rig.running()?;
            last_health = clock;
        }
        if clock.saturating_sub(last_hand_health) >= 1_000_000
            && let Some(controls) = controls.as_ref()
        {
            let health = controls.health();
            record(
                trace,
                json!({"kind":"hand_controls_health","at_us":monotonic_us(),
                "received":health.received,"rejected":health.rejected,"lost":health.lost,
                "reconnects":health.reconnects,"discontinuities":health.discontinuities,
                "pump_running":health.pump_running,"persistence_failed":health.persistence_failed}),
            )?;
            last_hand_health = clock;
        }
        flush_cues(trace, cues);
        #[cfg(test)]
        if crate::scenario_clock::is_active() {
            continue;
        }
        if let Some(wait) = Duration::from_millis(2).checked_sub(loop_start.elapsed()) {
            std::thread::sleep(wait);
        }
    }
}

/// Close the one coordinator withholding interval. This never changes input,
/// output ownership, raw accepted-zero receipts or their starvation meaning.
fn finish_duplex_yield(
    trace: &mut SessionState,
    at_us: u64,
    outcome: &str,
    frame_sequence: Option<u64>,
    read_completed_at_us: Option<u64>,
) -> Result<()> {
    let Some(interval) = trace.yield_interval.take() else {
        return Ok(());
    };
    record(
        trace,
        json!({"kind":"duplex_yield_end", "owner":interval.owner,
        "started_at_us":interval.started_at_us, "at_us":at_us,
        "first_frame_sequence":interval.first_frame_sequence,
        "first_read_completed_at_us":interval.first_read_completed_at_us,
        "last_frame_sequence":interval.last_frame_sequence,
        "ending_frame_sequence":frame_sequence,
        "ending_read_completed_at_us":read_completed_at_us,
        "missing_evidence_since_sequence":interval.missing_evidence_since,
        "outcome":outcome,
        "clock_regressed":at_us < interval.started_at_us,
        "boundary":"coordinator withholding interval; not an acoustic gap or input admission",
        "starvation_excluded":false}),
    )
}

/// Run immediately before the normal speaker dispatch. No stale hearing state
/// can leave an open audit interval after the mask ceases to withhold output.
fn settle_duplex_yield_before_dispatch(
    mask: &PlaybackMask,
    reply: &Option<Reply>,
    at_us: u64,
    trace: &mut SessionState,
) -> Result<()> {
    if let Some(interval) = trace.yield_interval {
        let current_owner = reply.as_ref().map(|current| current.owner);
        if interval.owner != current_owner {
            finish_duplex_yield(trace, at_us, "owner_changed", None, None)?;
        } else if !mask.yielded(at_us) {
            finish_duplex_yield(trace, at_us, "hearing_expired", None, None)?;
        }
    }
    Ok(())
}

fn clear_capture_lineage(
    detector: &mut TurnDetector,
    mask: &mut PlaybackMask,
    trace: &mut SessionState,
    at_us: u64,
    sequence: u64,
    read_completed_at_us: u64,
) -> Result<()> {
    finish_duplex_yield(
        trace,
        at_us,
        "capture_lineage_changed",
        Some(sequence),
        Some(read_completed_at_us),
    )?;
    detector.discard_inactive_prefix();
    mask.clear_read_lineage();
    Ok(())
}

/// Duplex hearing for one capture block, before admission sees it. Returns
/// the score the turn detector is to be given: the block's own, except that
/// the lamp's own voice counts as silence and a person over it as speech.
/// Every other policy gets the block's own score and no trace.
#[allow(clippy::too_many_arguments)]
fn hear_capture_block(
    mask: &mut PlaybackMask,
    reply: &Option<Reply>,
    heard_reply: &mut Option<TurnOwner>,
    sequence: u64,
    captured_at_us: u64,
    probability: f32,
    echo: Option<&lamp_audio::evidence::EchoEvidence>,
    at_us: u64,
    trace: &mut SessionState,
) -> Result<f32> {
    if !mask.is_duplex() {
        return Ok(probability);
    }
    let owner = reply.as_ref().map(|current| current.owner);
    if owner != *heard_reply {
        finish_duplex_yield(
            trace,
            at_us,
            "owner_changed",
            Some(sequence),
            Some(captured_at_us),
        )?;
        mask.reply_closed();
        *heard_reply = owner;
    }
    // Something to yield: reply audio waiting to be handed over, or the
    // lamp's sound still accepted, queued or within its tail.
    let speaking = !mask.available(at_us)
        || reply.as_ref().is_some_and(|current| {
            current.cloud.started && current.local_input_ended && !current.pcm.is_empty()
        });
    let listened = mask.listen(sequence, probability, echo, speaking, at_us)?;
    trace.heard(
        sequence,
        speaking,
        listened.heard,
        probability,
        echo,
        mask.hearing_levels_db(),
        at_us,
    )?;
    if let Some(interval) = trace.yield_interval.as_mut() {
        interval.last_frame_sequence = sequence;
        if echo.is_none() && interval.missing_evidence_since.is_none() {
            interval.missing_evidence_since = Some(sequence);
        }
    }
    if listened.step == DuplexStep::Yield {
        finish_duplex_yield(
            trace,
            at_us,
            "superseded_decision",
            Some(sequence),
            Some(captured_at_us),
        )?;
        trace.yield_interval = Some(YieldInterval {
            owner,
            started_at_us: at_us,
            first_frame_sequence: sequence,
            first_read_completed_at_us: captured_at_us,
            last_frame_sequence: sequence,
            missing_evidence_since: None,
        });
    }
    let kind = match listened.step {
        DuplexStep::Carry => {
            if trace.yield_interval.is_some() && !mask.yielded(at_us) {
                finish_duplex_yield(
                    trace,
                    at_us,
                    "evidence_discontinuity",
                    Some(sequence),
                    Some(captured_at_us),
                )?;
            }
            return Ok(listened.score);
        }
        DuplexStep::Yield => "duplex_yield",
        DuplexStep::Commit => "duplex_commit",
        DuplexStep::Resume => "duplex_resume",
    };
    record(
        trace,
        json!({"kind":kind,"at_us":at_us,"frame_sequence":sequence,
        "last_block_host_read_us":captured_at_us,"owner":owner,
        "reply_queued_frames":reply.as_ref().map(|current| current.pcm.len()),
        "hearing_margin_db":mask.hearing_margin_db(),
        "boundary":"coordinator hearing decision; not acoustic onset or stopped output"}),
    )?;
    match listened.step {
        DuplexStep::Commit => finish_duplex_yield(
            trace,
            at_us,
            "commit_decision",
            Some(sequence),
            Some(captured_at_us),
        )?,
        DuplexStep::Resume => finish_duplex_yield(
            trace,
            at_us,
            "resumed",
            Some(sequence),
            Some(captured_at_us),
        )?,
        _ => {}
    }
    Ok(listened.score)
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
    if !admission.owns_stream() && !admission.draining() && mask.frame() == Disposition::Suppress {
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
    trace: &mut SessionState,
) -> Result<Option<Accepted>> {
    if let Some(rejection) = admission.maintain(context, at_us)? {
        record_input_rejection(trace, rejection)?;
        return Ok(None);
    }
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
            let disposition = match mask.retained_frame(frame) {
                Ok(disposition) => disposition,
                Err(error)
                    if error
                        .get_ref()
                        .is_some_and(|cause| cause.is::<RetainedHistoryLoss>()) =>
                {
                    if let Some(rejection) =
                        admission.reject_retained_history(candidate.id, context, at_us)?
                    {
                        let loss = error
                            .get_ref()
                            .and_then(|cause| cause.downcast_ref::<RetainedHistoryLoss>())
                            .expect("matched history loss");
                        record(
                            trace,
                            json!({"kind":"input_candidate_rejected", "candidate":rejection.candidate,
                            "reason":rejection.reason, "history_loss":loss, "at_us":at_us}),
                        )?;
                    }
                    return Ok(None);
                }
                Err(error) => return Err(error.into()),
            };
            match disposition {
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
    let basis = if mask.is_duplex() {
        AcceptanceBasis::DirectedSessionDuplex
    } else if mask.enabled() {
        AcceptanceBasis::DirectedSessionV1Baseline
    } else {
        AcceptanceBasis::DirectedSessionVadOnly
    };
    let Some((_, evidence)) = admission.pending_evidence(at_us, basis) else {
        return Ok(None);
    };
    accepted_decision(admission.decide(evidence, context, at_us)?, trace)
}

fn reminder_quiet(
    now_us: u64,
    cycle_started_us: u64,
    last_speech_us: u64,
    last_capture_us: Option<u64>,
) -> bool {
    last_capture_us.is_some_and(|captured| {
        now_us
            .checked_sub(captured)
            .is_some_and(|age| age <= INPUT_LEASE_US)
    }) && now_us
        .checked_sub(last_speech_us.max(cycle_started_us))
        .is_some_and(|age| age >= 1_000_000)
}

/// A prepared reminder waits behind an existing turn's promised follow-up,
/// including the gap after its final PCM retires and before the next owner.
fn idle_reminder_offer(
    reminders: &crate::reminders::Reminders,
    reply: &Option<Reply>,
    look_wanted: Option<(u64, u64)>,
    second_look: &mut crate::second_look::SecondLook,
    now_us: u64,
) -> Option<crate::reminders::Offer> {
    if reply.is_some() || look_wanted.is_some() || second_look.due(now_us).is_some() {
        return None;
    }
    reminders.offer(now_us)
}

fn record_look_intent(
    enabled: bool,
    request: Option<u64>,
    text: &str,
    last_reminder_turn: Option<u64>,
    answer_opening: &mut Option<(u64, String)>,
    look_wanted: &mut Option<(u64, u64)>,
    trace: &mut SessionState,
) -> Result<()> {
    let Some(turn) = request
        .filter(|turn| enabled && last_reminder_turn.is_none_or(|reminder| *turn > reminder))
    else {
        return Ok(());
    };
    // Ordinary answer words retain the existing camera-request policy. Stored
    // reminder text, including a delayed transcript, is never that authority.
    if answer_opening.as_ref().map(|(of, _)| *of) != Some(turn) {
        *answer_opening = Some((turn, String::new()));
    }
    if let Some((_, opening)) = answer_opening.as_mut()
        && opening.len() < 64
    {
        opening.push_str(text);
        if look_wanted.map(|(of, _)| of) != Some(turn) && crate::speak_up::asks_to_look(opening) {
            *look_wanted = Some((turn, monotonic_us()));
            record(
                trace,
                json!({"kind":"look_asked","turn":turn,"at_us":monotonic_us()}),
            )?;
        }
    }
    Ok(())
}

/// A turn the lamp begins itself. Nothing was heard: the request is a
/// written note, started and ended at once, and what comes back is spoken
/// like any answer. The caller has checked that nothing else owns the lamp.
#[allow(clippy::too_many_arguments)]
fn commit_reminder_turn(
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    provider_queue: &mut VecDeque<ProviderInput>,
    offer: crate::reminders::Offer,
    reminders: &mut crate::reminders::Reminders,
    provider_session: u64,
    playout_start: PlayoutStart,
    trace: &mut SessionState,
    publish: impl FnMut(Snapshot) -> io::Result<()>,
) -> Result<Snapshot> {
    // Reuse the existing local owner transition. This call only queues input;
    // replace that unsent item with the reminder-only provider purpose before
    // any flush can run. Ordinary Written bytes and behavior are unchanged.
    let snapshot = commit_written_turn(
        owner,
        reply,
        provider_queue,
        &offer.text,
        false,
        "schedule_reminder",
        provider_session,
        playout_start,
        trace,
        publish,
    )?;
    let current = reply
        .as_mut()
        .ok_or_else(|| io::Error::other("reminder owner missing"))?;
    let command = provider_queue
        .back_mut()
        .ok_or_else(|| io::Error::other("reminder input missing"))?;
    *command = ProviderInput::Reminder {
        request: current.owner.turn(),
        session: provider_session,
        privacy_generation: snapshot.microphone_generation(),
        text: offer.text.clone(),
        late: offer.late,
        late_by_ms: offer.late_by_ms,
    };
    current.reminder = Some(reminders.admitted(
        offer,
        current.owner,
        snapshot.microphone_generation(),
        provider_session,
    ));
    Ok(snapshot)
}

#[allow(clippy::too_many_arguments)]
fn commit_written_turn(
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    provider_queue: &mut VecDeque<ProviderInput>,
    note: &str,
    look: bool,
    basis: &'static str,
    provider_session: u64,
    playout_start: PlayoutStart,
    trace: &mut SessionState,
    mut publish: impl FnMut(Snapshot) -> io::Result<()>,
) -> Result<Snapshot> {
    let turn = owner.admit(now(), AdmittedInput::NewTurn)?;
    let plan = owner.plan_output(now(), turn, OutputKind::Speech, None)?;
    let mut snapshot = owner.snapshot(now())?;
    publish(snapshot)?;
    let admitted_at_us = monotonic_us();
    trace
        .receipts
        .admitted(turn, snapshot.issued_at().as_micros())?;
    record(
        trace,
        json!({"kind":"input_admitted","owner":turn,"turn":turn.turn(),"at_us":admitted_at_us,"authority_issued_at_us":snapshot.issued_at().as_micros(),
            "admission_basis":basis,"provider_session":provider_session,
            "boundary":"no sound was heard: a written note stands in for speech"}),
    )?;
    provider_queue.push_back(ProviderInput::Written {
        request: turn.turn(),
        session: provider_session,
        privacy_generation: snapshot.microphone_generation(),
        text: note.to_owned(),
        look,
    });
    let current = Reply {
        owner: turn,
        cloud: CloudRequest::new(provider_session)?,
        local_input_ended: true,
        plan,
        playback: None,
        pcm: VecDeque::new(),
        generation_done: false,
        provider_idle: false,
        input_sequence: 0,
        had_audio: false,
        voice_level: 0,
        final_chunk: None,
        playback_gaps: 0,
        notice: NoticeDelivery::default(),
        reminder: None,
        last_provider_audio_at_us: None,
        start: playout_start.gate(),
        need: StartNeed::default(),
    };
    owner.input_ended(now(), current.owner)?;
    snapshot = owner.snapshot(now())?;
    publish(snapshot)?;
    *reply = Some(current);
    Ok(snapshot)
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
    trace: &mut SessionState,
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
    trace
        .receipts
        .admitted(turn, snapshot.issued_at().as_micros())?;
    record(
        trace,
        json!({"kind":"input_admitted","owner":turn,"turn":turn.turn(),"at_us":admitted_at_us,"authority_issued_at_us":snapshot.issued_at().as_micros(),"prefix_first_host_read_us":prefix.first().map(|p|p.captured_at_us),"candidate":accepted.candidate.id,"admission_basis":accepted.basis,"provider_session":provider_session}),
    )?;
    provider_queue.push_back(ProviderInput::Start {
        session: provider_session,
        request: turn.turn(),
        privacy_generation: snapshot.microphone_generation(),
        continues: false,
        continues_cut_off: false,
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
        voice_level: 0,
        final_chunk: None,
        playback_gaps: 0,
        notice: NoticeDelivery::default(),
        reminder: None,
        last_provider_audio_at_us: None,
        start: playout_start.gate(),
        need: StartNeed::default(),
    };
    // One candidate's audio, already bounded by the admission's hold. The
    // standard hold's prefix fits the standing bound as it always has.
    let bound = PROVIDER_QUEUE_BOUND.max(prefix.len() + 2);
    for audio in prefix {
        queue_input(
            provider_queue,
            &mut current,
            snapshot.microphone_generation(),
            audio,
            bound,
        )?;
    }
    if accepted.ended {
        current.local_input_ended = true;
        owner.input_ended(now(), current.owner)?;
        queue_input_end(
            provider_queue,
            &current,
            snapshot.microphone_generation(),
            bound,
            false,
        )?;
        snapshot = owner.snapshot(now())?;
        publish(snapshot)?;
    }
    *reply = Some(current);
    Ok(snapshot)
}

/// Someone spoke again after the end was sent early and before the lamp was
/// sure they had finished. The request is given up exactly as a continued
/// utterance is in `commit_admitted_input`: cancelled as `user_interrupted`
/// and its successor admitted at once, with no authority published between
/// the two, which the provider worker reads as a continuation (with
/// `--provider-continuation kept-start` its start then waits for the given-up
/// request's answer to begin, and stops it). Nothing of that answer was
/// played: the lamp's authority never stopped listening. The blocks heard
/// since the end are the successor's opening, and the utterance goes on in
/// the detector, so its further blocks come as the admitted stream's.
#[allow(clippy::too_many_arguments)]
fn continue_after_early_end(
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    speaker_pending: &mut Option<PendingSpeaker>,
    provider_queue: &mut VecDeque<ProviderInput>,
    opening: Vec<ObservedAudio>,
    provider_session: u64,
    playout_start: PlayoutStart,
    trace: &mut SessionState,
    mut publish: impl FnMut(Snapshot) -> io::Result<()>,
) -> Result<Snapshot> {
    cancel_reply(owner, reply, trace, "user_interrupted")?;
    *speaker_pending = None;
    provider_queue.clear();
    let turn = owner.admit(now(), AdmittedInput::Interruption)?;
    let plan = owner.plan_output(now(), turn, OutputKind::Speech, None)?;
    let snapshot = owner.snapshot(now())?;
    publish(snapshot)?;
    let admitted_at_us = monotonic_us();
    trace
        .receipts
        .admitted(turn, snapshot.issued_at().as_micros())?;
    record(
        trace,
        json!({"kind":"input_admitted","owner":turn,"turn":turn.turn(),"at_us":admitted_at_us,"authority_issued_at_us":snapshot.issued_at().as_micros(),"prefix_first_host_read_us":opening.first().map(|p|p.captured_at_us),"candidate":null,"admission_basis":"continued_after_early_end","provider_session":provider_session}),
    )?;
    provider_queue.push_back(ProviderInput::Start {
        session: provider_session,
        request: turn.turn(),
        privacy_generation: snapshot.microphone_generation(),
        // The same utterance going on: with the switch, the busy-room rule
        // keeps what it knew of the first part (#782).
        continues: crate::attention::cut_keeps_name(),
        // The model is told this goes on from the request given up.
        continues_cut_off: true,
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
        voice_level: 0,
        final_chunk: None,
        playback_gaps: 0,
        notice: NoticeDelivery::default(),
        reminder: None,
        last_provider_audio_at_us: None,
        start: playout_start.gate(),
        need: StartNeed::default(),
    };
    // At most a pre-roll, as a new candidate's opening is.
    let bound = PROVIDER_QUEUE_BOUND.max(opening.len() + 2);
    for audio in opening {
        queue_input(
            provider_queue,
            &mut current,
            snapshot.microphone_generation(),
            audio,
            bound,
        )?;
    }
    *reply = Some(current);
    Ok(snapshot)
}

/// What a record says of an end sent early, by the time `at_us`.
fn early_end_json(sent: &early_end::Sent, at_us: u64) -> Value {
    json!({"at_us":sent.at_us,"last_block_host_read_us":sent.last_block_host_read_us,
        "before_this_ms":sent.ago_us(at_us) / 1_000})
}

/// Presentation failure disables only the optional ring. Cleanup is observed
/// in bounded slices; no wait, restart or hardware write runs on this path.
fn service_optional_ring(
    rig: &mut Rig,
    choreography: &mut Option<RingChoreographer>,
    owner: &mut Controller,
    trace: &mut SessionState,
    ring_trace: &mut RingTrace,
) -> Result<()> {
    if rig.ring.is_active()
        && let Err(error) = service_ring(
            rig.ring.active_mut(),
            choreography.as_mut(),
            owner,
            trace,
            ring_trace,
        )
    {
        rig.ring.disable(error);
    }
    if !rig.ring.is_active() {
        *choreography = None;
    }
    rig.ring.poll_cleanup(trace)
}

/// All hardware I/O remains in the child. This path has one outstanding frame,
/// bounded nonblocking socket work, and no retry wait that can block capture.
/// Worker reports are evidence only; they never change conversational ownership.
fn service_ring(
    worker: Option<&mut Worker>,
    choreography: Option<&mut RingChoreographer>,
    owner: &mut Controller,
    trace: &mut SessionState,
    ring_trace: &mut RingTrace,
) -> Result<()> {
    service_ring_with_clock(worker, choreography, owner, trace, ring_trace, now)
}

fn service_ring_with_clock(
    worker: Option<&mut Worker>,
    choreography: Option<&mut RingChoreographer>,
    owner: &mut Controller,
    trace: &mut SessionState,
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
) -> Result<Option<(TurnOwner, Value)>> {
    if pending.is_some() {
        return Ok(None);
    }
    // Duplex: someone is being heard over the lamp. Keep the reply and hand
    // nothing more to the speaker until that is settled; it resumes or is
    // replaced through the ordinary cancellation path.
    if playback_mask.yielded(monotonic_us()) {
        return Ok(None);
    }
    let Some(current) = reply.as_mut() else {
        return Ok(None);
    };
    let origin = current.notice.origin();
    // Local notices never forge a provider Started receipt. Provider PCM keeps
    // its exact existing proof; both sources require the real local endpoint.
    if !current.local_input_ended || (origin == SpeechOrigin::Provider && !current.cloud.started) {
        return Ok(None);
    }
    if origin == SpeechOrigin::Provider && !current.may_start(monotonic_us()) {
        return Ok(None);
    }
    if origin == SpeechOrigin::FailureNotice
        && (!current.notice.has_chunk() || current.final_chunk.is_some())
    {
        return Ok(None);
    }
    let permit = owner.issue_output(now(), current.plan, SPEAKER_PERMIT_LEASE_US)?;
    let (samples, final_chunk, first_notice) = if origin == SpeechOrigin::FailureNotice {
        current.notice.reserve_chunk(monotonic_us())?
    } else {
        let Some((samples, last)) = current.next_speaker_chunk() else {
            return Ok(None);
        };
        (samples, last, false)
    };
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
        origin,
        sequence: *sequence,
        permit,
        accepted: 0,
        final_chunk,
        report_due_us: dispatch_at + SPEAKER_PERMIT_LEASE_US + SPEAKER_REPORT_AGE_US,
    });
    Ok(first_notice.then(|| (current.owner, current.notice.evidence())))
}

/// Queue threshold before appending PCM: a start, the largest accepted prefix
/// and what arrives while it drains. One terminal End may follow at this bound;
/// total command capacity is 65 in standard mode or 115 in duplex mode.
const PROVIDER_QUEUE_BOUND: usize = 64;

/// A longer hold accepts a longer prefix; the queue keeps the same spare
/// beyond it. The standard hold gives [`PROVIDER_QUEUE_BOUND`] itself.
fn provider_queue_bound(hold: Hold) -> usize {
    PROVIDER_QUEUE_BOUND
        + hold
            .decision_blocks
            .saturating_sub(Hold::STANDARD.decision_blocks)
}

fn queue_input(
    queue: &mut VecDeque<ProviderInput>,
    reply: &mut Reply,
    privacy_generation: u64,
    audio: ObservedAudio,
    bound: usize,
) -> Result<()> {
    if queue.len() >= bound {
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

/// The endpoint has one explicit slot after the PCM threshold. It cannot
/// grow an already-full terminal queue, and it does not enlarge PCM admission.
fn queue_input_end(
    queue: &mut VecDeque<ProviderInput>,
    reply: &Reply,
    privacy_generation: u64,
    pcm_bound: usize,
    sound: bool,
) -> Result<()> {
    if queue.len() > pcm_bound {
        return Err(io::Error::other("provider terminal queue exceeded bound").into());
    }
    queue.push_back(ProviderInput::End {
        request: reply.owner.turn(),
        privacy_generation,
        sound,
    });
    Ok(())
}

fn drain_capture_control(
    channel: &mut Channel,
    epoch: &mut Option<u64>,
    expected_noise_suppression: NoiseSuppression,
    trace: &mut SessionState,
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
            WorkerEvent::ReferenceStatus { status } => {
                record(trace, reference_status_trace(&status))?
            }
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
    trace: &mut SessionState,
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

/// A change of trust in the capture worker's echo reference. It is a status,
/// not a fault: the session goes on. While the reference is untrusted the
/// frames carry no echo evidence and did not pass the canceller, so a policy
/// that masks own playback is what keeps the lamp's voice from being a turn.
fn reference_status_trace(status: &crate::wire::ReferenceStatus) -> Value {
    json!({"kind":"reference_status","at_us":monotonic_us(),"details":status,
        "boundary":"capture worker's pairing of reference and capture by count; not an acoustic measurement"})
}

fn record_input_rejection(trace: &mut SessionState, rejection: Rejection) -> Result<()> {
    record(
        trace,
        json!({"kind":"input_candidate_rejected", "candidate":rejection.candidate,
        "reason":rejection.reason, "at_us":monotonic_us()}),
    )
}

fn cancel_reply(
    owner: &mut Controller,
    reply: &mut Option<Reply>,
    trace: &mut SessionState,
    reason: &str,
) -> Result<()> {
    if let Some(current) = reply.take() {
        let cancel = owner.cancel_turn(now(), current.owner);
        record_cancelled_reply(current, trace, reason, monotonic_us())?;
        cancel?;
    }
    Ok(())
}

fn record_cancelled_reply(
    current: Reply,
    trace: &mut SessionState,
    reason: &str,
    cancelled_at_us: u64,
) -> Result<()> {
    finish_duplex_yield(trace, cancelled_at_us, reason, None, None)?;
    trace
        .receipts
        .cancelled(current.owner, cancelled_at_us, reason)?;
    record(
        trace,
        json!({"kind":"turn_finished","owner":current.owner,"turn":current.owner.turn(),"at_us":cancelled_at_us,"outcome":reason,"provider_audio_seen":current.had_audio,"playback_gaps":current.playback_gaps,
                "provider_session":current.cloud.session,"provider_outcome":current.cloud.outcome,"failure_notice":current.notice.evidence(),
                "start_gate":current.start_trace(),"start_deficit_us":current.need.deficit_us(),
                "start_horizon_needed_us":current.need.horizon_us(),
                "start_need_scope":"all retained reply arrivals including later generations; excludes local handoff",
                "start_need_censored":true}),
    )?;
    Ok(())
}

/// An old accepted tail can retire after the successor has started or even
/// finished. This receipt is evidence only: it cannot complete either answer or
/// borrow the newer turn's playback occurrence.
fn handle_cancelled_tail_retirement(
    reply: &Option<Reply>,
    trace: &mut SessionState,
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
    trace
        .receipts
        .tail(affected, superseded_by, started_at_us)?;
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
    trace: &mut SessionState,
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
    let expected = reason == PlaybackDiscardReason::StaleOwner
        && !other_owners
        && reply
            .as_ref()
            .is_none_or(|current| current.owner != affected)
        && trace.receipts.discard(affected);
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
    trace: &mut SessionState,
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
    let origin = current.as_ref().map(|r| r.notice.origin());
    let (queued, held, last_provider_audio) = if let Some(current) = current {
        if phase == PlaybackGapPhase::Started && origin == Some(SpeechOrigin::FailureNotice) {
            current.notice.gap()?;
        } else if phase == PlaybackGapPhase::Started {
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
        json!({"kind":"playback_gap","origin":origin,"phase":phase,"gap":gap,
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
    mod duplex;
    mod early_end;
    mod endpoint_session;
    mod failure_notice;
    mod hand_controls;
    mod pilot_faults;
    mod playout_continuity;
    mod playout_start;
    mod provider_pair;
    mod provider_terminal;
    mod reminder_roundtrip;
    mod reminders;
    mod second_route;
    mod service;
    mod service_mute;
    mod startup_cues;
    mod termination;
    mod turn_taking;

    use super::*;
    use crate::{
        admission::{Evidence, Verdict},
        wire::CaptureTiming,
    };
    use lamp_turn_owner::BoundaryGuard;

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
        let mut trace = SessionState::default();
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
            second: None,
            end_silence_blocks: None,
            early_end_blocks: None,
            faint_level: None,
            microphone: crate::options::Microphone::Mic2,
            speaker,
            capture,
            provider,
            privacy,
            ring: Some(ring).into(),
            camera: None,
            motor: None,
            ring_channel_ceiling: Some(24),
            diagnostic_directory: Some(directory.path.clone()),
            noise_suppression: NoiseSuppression::On,
            turn_taking: TurnTaking::VadOnly,
            playout_start: PlayoutStart::default(),
            endpoint: None,
            paused: Box::new(|| false),
        };
        let started = Instant::now();
        let mut trace = SessionState::default();
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
                let mut trace = SessionState::default();
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
        let mut trace = SessionState::default();
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
                echo: None,
                reference: Default::default(),
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

    #[test]
    fn the_keeper_pause_is_read_as_the_body_reads_it() {
        use lamp_msgs::Pause;
        // A bus it cannot reach is not paused.
        assert!(!keeper_pause(lamp_bus::Bus::new("not/absolute"))());
        let root = std::env::temp_dir().join(format!("lamp-voice-pause-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let bus = lamp_bus::Bus::new(&root).unwrap();
        // Nothing kept is not paused.
        assert!(!keeper_pause(Ok(bus.clone()))());
        let mut keeper = bus
            .publisher_declared(Pause::TOPIC, &Pause::declaration(), "keeper")
            .unwrap();
        keeper.publish_json(1, &Pause { paused: false }).unwrap();
        assert!(!keeper_pause(Ok(bus.clone()))());
        keeper.publish_json(2, &Pause { paused: true }).unwrap();
        assert!(keeper_pause(Ok(bus.clone()))());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unasked_turns_wait_for_the_pause_and_are_otherwise_unchanged() {
        let mut startup = StartupEvents::default();
        assert_eq!(unasked_session(&startup.cloud, false), None);
        assert_eq!(unasked_session(&startup.cloud, true), None);
        startup
            .cloud
            .observe(ready_link(1, 1), monotonic_us())
            .unwrap();
        // Not paused, exactly the ready session, as before the pause.
        assert_eq!(
            unasked_session(&startup.cloud, false),
            startup.cloud.ready_session()
        );
        assert_eq!(unasked_session(&startup.cloud, false), Some(1));
        assert_eq!(unasked_session(&startup.cloud, true), None);
    }

    fn submitted_request() -> CloudRequest {
        let mut request = CloudRequest::new(1).unwrap();
        request
            .submitted(&ProviderInput::Start {
                session: 1,
                request: 1,
                privacy_generation: 1,
                continues: false,
                continues_cut_off: false,
            })
            .unwrap();
        request.confirm_start(1).unwrap();
        request
            .submitted(&ProviderInput::End {
                request: 1,
                privacy_generation: 1,
                sound: false,
            })
            .unwrap();
        request
    }

    pub(super) fn active_reply() -> (Controller, Reply, OutputPermit) {
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
                voice_level: 0,
                final_chunk: None,
                playback_gaps: 0,
                notice: NoticeDelivery::default(),
                reminder: None,
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
            let mut trace = SessionState::default();
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
        cancel_reply(
            &mut owner,
            &mut reply,
            &mut SessionState::default(),
            "user_interrupted",
        )
        .unwrap();
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
        let mut trace = SessionState::default();
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
        let mut trace = SessionState::default();
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
        let mut trace = SessionState::default();
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

    fn cancelled_tail_fixture() -> (Controller, Option<Reply>, SessionState, WorkerEvent, u64) {
        let (mut controller, old, _) = active_reply();
        let affected = old.owner;
        let mut reply = Some(old);
        let mut trace = SessionState::default();
        cancel_reply(&mut controller, &mut reply, &mut trace, "user_interrupted").unwrap();
        let successor = controller
            .admit(now(), AdmittedInput::Interruption)
            .unwrap();
        trace
            .receipts
            .admitted(
                successor,
                controller.snapshot(now()).unwrap().issued_at().as_micros(),
            )
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
            voice_level: 0,
            final_chunk: None,
            playback_gaps: 0,
            notice: NoticeDelivery::default(),
            reminder: None,
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
        let (_, reply, original, event, observed) = cancelled_tail_fixture();
        let WorkerEvent::CancelledTailRetired {
            owner,
            superseded_by,
            started_at_us,
            ..
        } = event
        else {
            unreachable!()
        };
        let cancelled = original[0]["at_us"].as_u64().unwrap();
        for case in 0..7 {
            let mut trace = SessionState::default();
            // Plausible JSON cannot mint live ownership/cancellation lineage.
            trace.extend(original.iter().cloned());
            if case != 0 {
                trace
                    .receipts
                    .cancelled(
                        owner,
                        cancelled,
                        if case == 1 {
                            "provider_interrupted"
                        } else {
                            "user_interrupted"
                        },
                    )
                    .unwrap();
            }
            if case != 2 {
                let successor = if case == 3 {
                    active_reply().1.owner
                } else {
                    superseded_by
                };
                let issued = match case {
                    4 => started_at_us + 1,
                    5 => cancelled - 1,
                    _ => started_at_us,
                };
                if case != 6 {
                    trace.receipts.admitted(successor, issued).unwrap();
                }
            }
            assert!(
                handle_cancelled_tail_retirement(
                    &reply,
                    &mut trace,
                    crate::wire::decode(&crate::wire::encode(&event).unwrap()).unwrap(),
                    observed
                )
                .is_err()
            );
        }
        let mut no_metadata = original.clone();
        no_metadata.clear();
        handle_cancelled_tail_retirement(&reply, &mut no_metadata, event, observed).unwrap();
        assert_eq!(no_metadata.len(), 1);
    }

    #[test]
    fn expired_current_playback_is_failed_delivery_not_a_completed_answer() {
        let (mut owner, current, _) = active_reply();
        let affected = current.owner;
        let mut reply = Some(current);
        let mut trace = SessionState::default();
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
        let mut trace = SessionState::default();
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
            voice_level: 0,
            final_chunk: None,
            playback_gaps: 0,
            notice: NoticeDelivery::default(),
            reminder: None,
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
            let mut trace = SessionState::default();
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
            continues: false,
            continues_cut_off: false,
        }
    }

    fn end_command(reply: &Option<Reply>) -> ProviderInput {
        ProviderInput::End {
            request: reply.as_ref().unwrap().owner.turn(),
            privacy_generation: 1,
            sound: false,
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
        let mut trace = SessionState::default();
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
            &mut SessionState::default(),
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
            &mut SessionState::default(),
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
            &mut SessionState::default(),
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
        let mut trace = SessionState::default();
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
    fn assumed_ownership_grants_nothing_and_completion_without_submitted_end_fails_closed() {
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
        observe_provider_output(
            &mut cloud,
            &mut reply,
            &ProviderOutput::OwnershipAssumed {
                request: Some(request),
                superseded: None,
            },
        )
        .unwrap();
        assert!(reply.as_ref().unwrap().cloud.outcome.is_none());
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
        let mut trace = SessionState::default();
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
        let mut trace = SessionState::default();
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

#[cfg(test)]
#[path = "coordinator/optional_ring_tests.rs"]
mod optional_ring_tests;
