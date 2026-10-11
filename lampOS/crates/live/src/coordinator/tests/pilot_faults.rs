//! Supervised-pilot fault qualification through the production coordinator.
//!
//! Every test runs the real `conversation()` service loop (`Lifetime::Serve`,
//! V1-baseline turn-taking, default options) and `finish_active_run`, with
//! real child processes and real local IPC. The children are cooperative
//! fixtures; one test peer answers for capture, privacy, provider and speaker
//! on the production channels and injects exactly one fault at an observed
//! event, not after a fixed sleep. The speaker peer applies the production
//! `BoundaryGuard` to every chunk, as the real speaker does.
//!
//! Every fault here is injected. Nothing is a device, microphone, speaker,
//! cloud or acoustic measurement.
use super::*;
use crate::{
    provider_flow::OutputSender,
    provider_worker::{FailedStage, Fault},
    wire::{CaptureTiming, ReferenceTiming},
};
use lamp_audio::ownership::PlaybackCursor;
use lamp_interaction::BoundaryGuard;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

/// The peer keeps production freshness bounds (50 ms privacy, 100 ms
/// capture leases) and polls every millisecond. One session runs at a time
/// so parallel test threads cannot starve it.
static ONE_SESSION: Mutex<()> = Mutex::new(());

fn one_session() -> std::sync::MutexGuard<'static, ()> {
    ONE_SESSION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Provider samples per answer packet: the production maximum, 40 ms at
/// 24 kHz.
const PACKET: usize = crate::provider_flow::PACKET_SAMPLES;
/// Upper bound on any single scenario, including cleanup. A run that needs
/// it has hung; the peer then reports instead of blocking the test binary.
const SCENARIO_LIMIT: Duration = Duration::from_secs(12);

/// What the provider peer does with one request after its End.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Answer {
    /// `packets` of audio, then GenerationComplete and TurnComplete.
    Complete(usize),
    /// TurnFailed(AwaitingResponse) before any audio.
    FailBeforeOutput,
    /// `packets` of audio, then TurnFailed(Responding) once the speaker has
    /// accepted at least one chunk of this request.
    FailDuringOutput(usize),
    /// `packets` of audio and then nothing: output stays pending.
    Hold(usize),
}

/// An observed point in the run at which the peer injects an action.
#[derive(Clone, Copy, Debug, PartialEq)]
enum When {
    /// The capture peer has sent this many frames.
    Block(u64),
    /// The provider peer has received End for this many requests.
    InputEnded(usize),
    /// The speaker has accepted this many chunks in total.
    Accepted(usize),
    /// The speaker has accepted this many chunks of the notice.
    NoticeAccepted(usize),
    /// The latest authority snapshot shows no owner, after this many
    /// requests have ended: the coordinator has settled them.
    Settled(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    /// Physical privacy reports muted from now on.
    Mute,
    /// The capture child reports a worker fault.
    CaptureFault,
    /// The capture peer stops sending frames but stays connected.
    CaptureSilent,
    /// The provider peer closes its channels: the provider is gone.
    ProviderGone,
    /// The speaker keeps reading control but stops reading and acknowledging
    /// chunks, like a writer stuck behind a starved device.
    StallSpeaker,
    /// Real SIGTERM to this process, under an installed `Termination`.
    Sigterm,
    /// Speech starts on capture: a new user turn begins now.
    Speak,
}

#[derive(Clone, Debug)]
struct Script {
    /// Capture frames scored as speech, by 1-based frame index. Later
    /// turns start with `Act::Speak`.
    speech: std::ops::Range<u64>,
    /// Answers in order of provider Start.
    answers: Vec<Answer>,
    actions: Vec<(When, Act)>,
    notices: bool,
    signals: bool,
    /// The pilot runs V1-baseline; other policies are cross-checks only.
    turn_taking: TurnTaking,
}

impl Script {
    /// One user question, then the given answer, then a mute once the
    /// coordinator has settled the turn.
    fn question(answer: Answer) -> Self {
        Self {
            speech: 20..80,
            answers: vec![answer],
            actions: vec![(When::Settled(1), Act::Mute)],
            notices: false,
            signals: false,
            turn_taking: TurnTaking::V1Baseline,
        }
    }
    fn then(mut self, when: When, act: Act) -> Self {
        self.actions.insert(0, (when, act));
        self
    }
}

/// One chunk as the speaker peer saw it.
#[derive(Clone, Debug)]
struct Chunk {
    turn: u64,
    sequence: u64,
    frames: usize,
    /// The coordinator's authority view when it dispatched the chunk.
    snapshot_issued_us: u64,
    /// The production guard's verdict, as the real speaker would apply it.
    accepted: bool,
    notice: bool,
}

#[derive(Default)]
struct Seen {
    chunks: Vec<Chunk>,
    starts: Vec<u64>,
    ends: Vec<u64>,
    capture_frames: u64,
    /// When each action fired.
    fired: Vec<(Act, u64)>,
    /// Roles that observed priority Stop.
    stopped: Vec<&'static str>,
    /// Roles whose peer stopped answering by design.
    abandoned: Vec<&'static str>,
    /// Longest interval between two privacy observations the peer sent, in
    /// microseconds: the peer's own share of any freshness loss.
    privacy_gap_us: u64,
    last_privacy_us: Option<u64>,
}

struct Run {
    result: Result<()>,
    trace: Vec<Value>,
    seen: Seen,
    elapsed: Duration,
}

impl Run {
    fn events(&self, kind: &str) -> Vec<&Value> {
        self.trace.iter().filter(|e| e["kind"] == kind).collect()
    }
    fn fired_at(&self, act: Act) -> u64 {
        self.seen
            .fired
            .iter()
            .find(|(fired, _)| *fired == act)
            .unwrap_or_else(|| panic!("{act:?} never fired"))
            .1
    }
    fn outcomes(&self) -> BTreeMap<u64, Vec<String>> {
        let mut outcomes: BTreeMap<u64, Vec<String>> = BTreeMap::new();
        for event in self.events("turn_finished") {
            outcomes
                .entry(event["turn"].as_u64().unwrap())
                .or_default()
                .push(event["outcome"].as_str().unwrap().to_owned());
        }
        outcomes
    }
    fn accepted(&self, turn: u64) -> Vec<&Chunk> {
        self.seen
            .chunks
            .iter()
            .filter(|chunk| chunk.turn == turn && chunk.accepted)
            .collect()
    }
    /// Printed with every run, so a failure shows its own sequence.
    fn summary(&self) -> String {
        let mut kinds: Vec<String> = Vec::new();
        for event in &self.trace {
            let kind = event["kind"].as_str().unwrap_or("?");
            let detail = match kind {
                "turn_finished" => format!("({})", event["outcome"]),
                "local_endpoint" | "input_admitted" => format!("({})", event["turn"]),
                _ => String::new(),
            };
            let entry = format!("{kind}{detail}");
            if kinds.last() != Some(&entry) {
                kinds.push(entry);
            }
        }
        format!(
            "result {:?}\nkinds {kinds:?}\npeer privacy gap {} us\nframes {} starts {:?} ends {:?} chunks {} accepted {} fired {:?} stopped {:?} abandoned {:?}",
            self.result.as_ref().map_err(|e| e.to_string()),
            self.seen.privacy_gap_us,
            self.seen.capture_frames,
            self.seen.starts,
            self.seen.ends,
            self.seen.chunks.len(),
            self.seen.chunks.iter().filter(|c| c.accepted).count(),
            self.seen.fired,
            self.seen.stopped,
            self.seen.abandoned,
        )
    }
    /// The invariants every scenario must keep, whatever its fault.
    fn assert_invariants(&self) {
        eprintln!("{}", self.summary());
        // No duplicate outcome for any turn, and no duplicate request.
        for (turn, outcomes) in self.outcomes() {
            assert_eq!(
                outcomes.len(),
                1,
                "turn {turn} finished twice: {outcomes:?}"
            );
        }
        let mut starts = self.seen.starts.clone();
        starts.dedup();
        assert_eq!(starts, self.seen.starts, "a request started twice");
        // Every chunk sequence reached the speaker once.
        let mut sequences: Vec<_> = self.seen.chunks.iter().map(|c| c.sequence).collect();
        sequences.sort_unstable();
        sequences.dedup();
        assert_eq!(
            sequences.len(),
            self.seen.chunks.len(),
            "a chunk was sent twice"
        );
        // No stale speech: nothing was dispatched under an authority view
        // issued after its owner had finished, privacy was observed muted, or
        // Stop was requested.
        let finished: BTreeMap<u64, u64> = self
            .events("turn_finished")
            .iter()
            .map(|e| (e["turn"].as_u64().unwrap(), e["at_us"].as_u64().unwrap()))
            .collect();
        let closed = self
            .events("physical_mute")
            .iter()
            .chain(self.events("service_stop_requested").iter())
            .map(|e| e["at_us"].as_u64().unwrap())
            .min();
        for chunk in &self.seen.chunks {
            if let Some(&at) = finished.get(&chunk.turn) {
                assert!(
                    chunk.snapshot_issued_us <= at,
                    "stale dispatch for finished turn {}: {chunk:?}",
                    chunk.turn
                );
            }
            if let Some(at) = closed {
                assert!(
                    chunk.snapshot_issued_us <= at,
                    "dispatch after mute or stop: {chunk:?}"
                );
            }
        }
        // No false readiness: once a session-ending fault, mute or stop was
        // recorded, nothing claims to listen again in this run.
        let ended = self
            .trace
            .iter()
            .filter(|e| {
                matches!(
                    e["kind"].as_str(),
                    Some("physical_mute" | "service_stop_requested" | "provider_output_ended")
                )
            })
            .map(|e| e["at_us"].as_u64().unwrap())
            .min();
        if let Some(at) = ended {
            for ready in self.events("listening_ready") {
                assert!(
                    ready["at_us"].as_u64().unwrap() <= at,
                    "readiness after the run ended: {ready}"
                );
            }
        }
        // No orphan: every child was reaped by the supervisor without a
        // forced kill unless the peer abandoned it by design.
        assert!(
            self.events("worker_forced_shutdown").len() <= self.seen.abandoned.len(),
            "forced shutdowns {:?}, abandoned {:?}",
            self.events("worker_forced_shutdown"),
            self.seen.abandoned
        );
        for role in ["capture", "privacy", "provider", "speaker"] {
            assert!(
                self.seen.stopped.contains(&role) || self.seen.abandoned.contains(&role),
                "{role} never saw Stop"
            );
        }
        assert!(self.elapsed < SCENARIO_LIMIT, "run took {:?}", self.elapsed);
    }
}

struct Peer {
    role: &'static str,
    channels: WorkerChannels,
    input: Option<std::process::ChildStdin>,
    snapshot: Option<Snapshot>,
    stopped: bool,
}

/// What the peer thread owns between ticks.
struct Peers {
    script: Script,
    peers: Vec<Peer>,
    seen: Arc<Mutex<Seen>>,
    guard: BoundaryGuard,
    output: OutputSender,
    queued: VecDeque<ProviderOutput>,
    /// Requests by Start order, with the samples their answer still owes.
    requests: Vec<u64>,
    pending_failure: Option<u64>,
    muted: bool,
    capture_started: bool,
    capture_silent: bool,
    speaker_stalled: bool,
    provider_gone: bool,
    speech_offset: Option<u64>,
    next_capture: Instant,
    next_privacy: Instant,
    /// The speaker accepts PCM at the device rate, as ALSA does.
    next_write: Instant,
    cursor: u64,
    audio_sequence: u64,
    accepted: usize,
    notice_accepted: usize,
    ended: usize,
    settled_after: Option<usize>,
}

impl Peers {
    fn fire(&mut self, act: Act) {
        self.seen.lock().unwrap().fired.push((act, monotonic_us()));
        match act {
            Act::Mute => self.muted = true,
            Act::CaptureFault => {
                if let Some(capture) = self.peers.iter_mut().find(|p| p.role == "capture") {
                    let _ = capture.channels.control.send(WorkerEvent::Fault {
                        code: "injected capture fault".into(),
                    });
                }
            }
            Act::CaptureSilent => self.capture_silent = true,
            Act::ProviderGone => {
                self.provider_gone = true;
                if let Some(provider) = self.peers.iter_mut().find(|p| p.role == "provider") {
                    // Closing stdin ends the child; the sockets close with it.
                    provider.input.take();
                    provider.stopped = true;
                }
                self.seen.lock().unwrap().abandoned.push("provider");
            }
            Act::StallSpeaker => self.speaker_stalled = true,
            Act::Sigterm => {
                rustix::process::kill_process(
                    rustix::process::getpid(),
                    rustix::process::Signal::TERM,
                )
                .unwrap();
            }
            Act::Speak => {
                let frames = self.seen.lock().unwrap().capture_frames;
                self.speech_offset = Some(frames);
            }
        }
    }

    fn due(&self, when: When) -> bool {
        match when {
            When::Block(block) => self.seen.lock().unwrap().capture_frames >= block,
            When::InputEnded(count) => self.ended >= count,
            When::Accepted(count) => self.accepted >= count,
            When::NoticeAccepted(count) => self.notice_accepted >= count,
            When::Settled(count) => self.settled_after.is_some_and(|after| after >= count),
        }
    }

    fn speech(&self, frame: u64) -> bool {
        let scripted = self.script.speech.contains(&frame);
        let spoken = self
            .speech_offset
            .is_some_and(|offset| (offset..offset + 60).contains(&frame));
        scripted || spoken
    }

    fn tick(&mut self) {
        let actions = std::mem::take(&mut self.script.actions);
        let (due, waiting): (Vec<_>, Vec<_>) =
            actions.into_iter().partition(|&(when, _)| self.due(when));
        self.script.actions = waiting;
        for (_, act) in due {
            self.fire(act);
        }
        for index in 0..self.peers.len() {
            self.tick_peer(index);
        }
    }

    fn tick_peer(&mut self, index: usize) {
        if self.peers[index].stopped {
            return;
        }
        let role = self.peers[index].role;
        if role == "speaker" && self.speaker_stalled {
            // A stuck writer still sees Stop, but nothing else.
            if let Ok(Some(Control::Stop)) = self.peers[index].channels.control.receive::<Control>()
            {
                self.stop(index);
            }
            return;
        }
        for _ in 0..16 {
            match self.peers[index].channels.control.receive::<Control>() {
                Ok(Some(Control::Authority { snapshot })) => {
                    self.peers[index].snapshot = Some(snapshot);
                    if role == "speaker" {
                        let _ = self.guard.install(now(), snapshot);
                    }
                    if snapshot.owner().is_none() && self.ended > 0 {
                        self.settled_after = Some(self.ended);
                    }
                }
                Ok(Some(Control::ProviderOutputCapacity { through })) => {
                    self.output.grant(through).unwrap()
                }
                Ok(Some(Control::StartCapture)) => {
                    self.capture_started = true;
                    let at = monotonic_us();
                    let generation = self.peers[index].snapshot.unwrap().microphone_generation();
                    self.peers[index]
                        .channels
                        .control
                        .send(WorkerEvent::CaptureStarted {
                            epoch: 1,
                            dsp_epoch: 1,
                            noise_suppression: NoiseSuppression::On,
                            privacy_generation: generation,
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
                }
                Ok(Some(Control::Stop)) => {
                    self.stop(index);
                    return;
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }
        match role {
            "privacy" if Instant::now() >= self.next_privacy => {
                let at = monotonic_us();
                {
                    let mut seen = self.seen.lock().unwrap();
                    if let Some(last) = seen.last_privacy_us {
                        seen.privacy_gap_us = seen.privacy_gap_us.max(at - last);
                    }
                    seen.last_privacy_us = Some(at);
                }
                let _ = self.peers[index]
                    .channels
                    .control
                    .send(WorkerEvent::Privacy {
                        acquired_at_us: at,
                        muted: self.muted,
                    });
                self.next_privacy = Instant::now() + Duration::from_millis(5);
            }
            "capture" if self.capture_started && !self.capture_silent => {
                // Frames follow a fixed 10 ms schedule, like a capture device.
                while Instant::now() >= self.next_capture {
                    let frame = {
                        let mut seen = self.seen.lock().unwrap();
                        seen.capture_frames += 1;
                        seen.capture_frames
                    };
                    let at = monotonic_us();
                    let speech = self.speech(frame);
                    let peer = &mut self.peers[index];
                    let sent = peer.channels.data.send(MicrophoneFrame {
                        epoch: 1,
                        dsp_epoch: 1,
                        privacy_generation: peer.snapshot.unwrap().microphone_generation(),
                        frame_sequence: frame,
                        read_completed_at_us: at,
                        probability: if speech { 0.99 } else { 0.0 },
                        echo: None,
                        reference: Default::default(),
                        samples: vec![if speech { 2_000 } else { 7 }; 160],
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
                            reference: ReferenceTiming {
                                playback_epoch: 1,
                                clock_started_at_us: Some(at),
                                ..ReferenceTiming::default()
                            },
                        },
                    });
                    self.next_capture += Duration::from_millis(10);
                    if sent.is_err() {
                        break;
                    }
                }
            }
            "provider" if !self.provider_gone => self.tick_provider(index),
            "speaker" => self.tick_speaker(index),
            _ => {}
        }
    }

    fn tick_provider(&mut self, index: usize) {
        for _ in 0..64 {
            match self.peers[index].channels.data.receive::<ProviderInput>() {
                Ok(Some(ProviderInput::Start {
                    request, session, ..
                })) => {
                    self.seen.lock().unwrap().starts.push(request);
                    self.requests.push(request);
                    self.queued.push_back(ProviderOutput::Started {
                        request,
                        session,
                        waiting_for_barrier: false,
                    });
                }
                Ok(Some(ProviderInput::End { request, .. })) => {
                    self.seen.lock().unwrap().ends.push(request);
                    self.ended += 1;
                    let position = self.requests.iter().position(|&r| r == request);
                    let answer = position.and_then(|p| self.script.answers.get(p)).copied();
                    match answer {
                        Some(Answer::Complete(packets)) => {
                            self.audio(request, packets);
                            self.queued
                                .push_back(ProviderOutput::GenerationComplete { request });
                            self.queued.push_back(ProviderOutput::TurnComplete {
                                request,
                                idle: true,
                            });
                        }
                        Some(Answer::FailBeforeOutput) => {
                            self.queued.push_back(ProviderOutput::TurnFailed {
                                request,
                                stage: FailedStage::AwaitingResponse,
                                reason: Fault::Transport,
                            })
                        }
                        Some(Answer::FailDuringOutput(packets)) => {
                            self.audio(request, packets);
                            self.pending_failure = Some(request);
                        }
                        Some(Answer::Hold(packets)) => self.audio(request, packets),
                        None => {}
                    }
                }
                Ok(Some(ProviderInput::Audio { .. })) => {}
                Ok(None) | Err(_) => break,
            }
        }
        if let Some(request) = self.pending_failure
            && self.accepted > 0
        {
            self.pending_failure = None;
            self.queued.push_back(ProviderOutput::TurnFailed {
                request,
                stage: FailedStage::Responding,
                reason: Fault::Transport,
            });
        }
        let peer = &mut self.peers[index];
        while let Some(event) = self.queued.front() {
            match self.output.try_send(|| peer.channels.data.send(event)) {
                Ok(true) => {
                    self.queued.pop_front();
                }
                Ok(false) | Err(_) => break,
            }
        }
    }

    fn audio(&mut self, request: u64, packets: usize) {
        for _ in 0..packets {
            self.audio_sequence += 1;
            self.queued.push_back(ProviderOutput::Audio {
                request,
                sequence: self.audio_sequence,
                provider_event_at_us: monotonic_us(),
                source_frames: PACKET,
                source_offset: 0,
                samples: vec![1_000; PACKET],
            });
        }
    }

    fn tick_speaker(&mut self, index: usize) {
        if Instant::now() < self.next_write {
            return;
        }
        let Ok(Some(chunk)) = self.peers[index].channels.data.receive::<SpeakerChunk>() else {
            return;
        };
        // 24 kHz output: 24 samples per millisecond.
        self.next_write = self
            .next_write
            .max(Instant::now() - Duration::from_millis(20))
            + Duration::from_micros(chunk.samples.len() as u64 * 1_000 / 24);
        // As the real speaker: install the chunk's own view (an older one is
        // ignored), then check the permit immediately before the write.
        let _ = self.guard.install(now(), chunk.snapshot);
        let accepted = self
            .guard
            .check(now(), chunk.permit, OutputKind::Speech)
            .is_ok();
        let owner = chunk.permit.owner();
        // The synthetic notice is constant 321; provider answers are 1,000.
        let notice = chunk.samples.contains(&321);
        self.seen.lock().unwrap().chunks.push(Chunk {
            turn: owner.turn(),
            sequence: chunk.chunk_sequence,
            frames: chunk.samples.len(),
            snapshot_issued_us: chunk.snapshot.issued_at().as_micros(),
            accepted,
            notice,
        });
        let channels = &mut self.peers[index].channels;
        if !accepted {
            let _ = channels.control.send(WorkerEvent::PlaybackRejected {
                turn: owner.turn(),
                sequence: chunk.chunk_sequence,
                reason: "fixture guard refused the permit".into(),
            });
            return;
        }
        self.accepted += 1;
        if notice {
            self.notice_accepted += 1;
        }
        let first = PlaybackCursor {
            epoch: 1,
            frame: self.cursor,
        };
        self.cursor += chunk.samples.len() as u64;
        let end = PlaybackCursor {
            epoch: 1,
            frame: self.cursor,
        };
        let at = monotonic_us();
        let _ = channels.control.send(WorkerEvent::Playback {
            owner,
            first_sample: first,
            end_sample: end,
            queue_observed_at_us: at,
            turn: owner.turn(),
            sequence: chunk.chunk_sequence,
            queued_frames: 0,
            accepted_frames: chunk.samples.len(),
            observed_at_us: at,
        });
        let _ = channels.control.send(WorkerEvent::SpeechDrained {
            owner,
            sequence: chunk.chunk_sequence,
            last_sample: end,
            observed_at_us: at,
        });
        if chunk.end_of_speech {
            let _ = channels.control.send(WorkerEvent::SpeechRetired {
                owner,
                sequence: chunk.chunk_sequence,
                final_sample: end,
                observed_at_us: at,
            });
        }
    }

    fn stop(&mut self, index: usize) {
        let peer = &mut self.peers[index];
        peer.stopped = true;
        peer.input.take();
        self.seen.lock().unwrap().stopped.push(peer.role);
    }
}

fn run(script: Script) -> Run {
    let notices = if script.notices {
        FailureNotices::synthetic()
    } else {
        FailureNotices::none()
    };
    let termination = if script.signals {
        Termination::install().unwrap()
    } else {
        Termination::inactive()
    };
    let directory = SessionDirectory::create().unwrap();
    let mut peers = Vec::new();
    let mut spawn = |role| {
        let (worker, channels, input) = Worker::cooperative_fixture(&directory.path, role);
        peers.push(Peer {
            role,
            channels,
            input: Some(input),
            snapshot: None,
            stopped: false,
        });
        worker
    };
    let mut rig = Rig {
        speaker: spawn("speaker"),
        capture: spawn("capture"),
        provider: spawn("provider"),
        privacy: spawn("privacy"),
        ring: None.into(),
        camera: None,
        ring_channel_ceiling: None,
        diagnostic_directory: None,
        noise_suppression: NoiseSuppression::On,
        turn_taking: script.turn_taking,
        playout_start: PlayoutStart::default(),
    };
    let mut owner = Controller::new(new_boot().unwrap(), now());
    let boot = owner.snapshot(now()).unwrap().boot();
    let mut startup = StartupEvents::default();
    startup
        .cloud
        .observe(ready_link(1, 1), monotonic_us())
        .unwrap();
    let seen = Arc::new(Mutex::new(Seen::default()));
    let finished = Arc::new(AtomicBool::new(false));
    let worker = thread::spawn({
        let (seen, finished) = (seen.clone(), finished.clone());
        move || {
            let mut state = Peers {
                script,
                peers,
                seen,
                guard: BoundaryGuard::new(boot, now()),
                output: OutputSender::default(),
                queued: VecDeque::new(),
                requests: Vec::new(),
                pending_failure: None,
                muted: false,
                capture_started: false,
                capture_silent: false,
                speaker_stalled: false,
                provider_gone: false,
                speech_offset: None,
                next_capture: Instant::now(),
                next_privacy: Instant::now(),
                next_write: Instant::now(),
                cursor: 0,
                audio_sequence: 0,
                accepted: 0,
                notice_accepted: 0,
                ended: 0,
                settled_after: None,
            };
            let started = Instant::now();
            let mut limit_reported = false;
            // Endpoints stay open until the supervisor has finished, as a
            // worker's sockets do until its process is reaped.
            while !finished.load(Ordering::SeqCst) {
                if started.elapsed() > SCENARIO_LIMIT && !limit_reported {
                    limit_reported = true;
                    // Report a hung run through the session instead of
                    // blocking: a fault the coordinator must end on.
                    if let Some(privacy) = state.peers.iter_mut().find(|p| p.role == "privacy") {
                        let _ = privacy.channels.control.send(WorkerEvent::Fault {
                            code: "pilot fault scenario limit".into(),
                        });
                    }
                }
                state.tick();
                thread::sleep(Duration::from_millis(1));
            }
            for peer in &mut state.peers {
                peer.input.take();
            }
        }
    });
    let mut trace = SessionState::default();
    let mut reply = None;
    let started = Instant::now();
    let result = conversation(
        &mut rig,
        &mut owner,
        Lifetime::Serve,
        &mut trace,
        startup,
        &mut reply,
        &mut None,
        &termination,
        &notices,
    );
    let result = finish_active_run(
        &mut rig, &mut owner, &mut reply, &mut trace, &mut None, result,
    );
    let elapsed = started.elapsed();
    finished.store(true, Ordering::SeqCst);
    worker.join().unwrap();
    assert!(reply.is_none(), "an answer outlived its run");
    let seen = std::mem::take(&mut *seen.lock().unwrap());
    Run {
        result,
        trace: trace.iter().cloned().collect(),
        seen,
        elapsed,
    }
}

fn is_mute(result: &Result<()>) -> bool {
    result
        .as_ref()
        .err()
        .is_some_and(|error| error.downcast_ref::<PhysicalMute>().is_some())
}

#[test]
fn control_one_question_is_answered_once_then_mute_ends_the_run() {
    let _one = one_session();
    let run = run(Script::question(Answer::Complete(12)));
    run.assert_invariants();
    assert!(is_mute(&run.result), "{:?}", run.result);
    assert_eq!(run.seen.starts.len(), 1);
    let turn = run.seen.starts[0];
    assert_eq!(run.outcomes()[&turn].len(), 1);
    // Every answer sample reached the speaker, once, and was accepted.
    let frames: usize = run.accepted(turn).iter().map(|c| c.frames).sum();
    assert!(frames >= 12 * PACKET, "{frames}");
    assert!(run.seen.chunks.iter().all(|c| c.accepted));
    assert_eq!(run.events("listening_ready").len(), 1);
}

/// Turn of the first request.
fn first(run: &Run) -> u64 {
    *run.seen.starts.first().expect("a request started")
}

#[test]
fn provider_failure_before_output_settles_failed_without_speech() {
    let _one = one_session();
    let run = run(Script::question(Answer::FailBeforeOutput));
    run.assert_invariants();
    assert!(is_mute(&run.result), "{:?}", run.result);
    assert_eq!(run.outcomes()[&first(&run)], ["provider_failed"]);
    assert!(run.seen.chunks.is_empty());
    assert!(run.events("speaker_first_write").is_empty());
}

#[test]
fn provider_failure_during_output_never_becomes_a_completed_answer() {
    let _one = one_session();
    let run = run(Script::question(Answer::FailDuringOutput(12)));
    run.assert_invariants();
    assert!(is_mute(&run.result), "{:?}", run.result);
    let turn = first(&run);
    // Audio that arrived before the failure may play; the turn stays failed.
    assert!(!run.accepted(turn).is_empty());
    assert_eq!(run.outcomes()[&turn], ["provider_failed"]);
}

#[test]
fn provider_process_lost_before_output_ends_the_run_without_speech() {
    let _one = one_session();
    let run = run(Script {
        answers: vec![],
        ..Script::question(Answer::FailBeforeOutput)
    }
    .then(When::InputEnded(1), Act::ProviderGone));
    run.assert_invariants();
    assert!(
        run.result.is_err() && !is_mute(&run.result),
        "{:?}",
        run.result
    );
    assert!(run.seen.chunks.is_empty());
    assert_eq!(run.outcomes()[&first(&run)], ["runtime_failed"]);
}

#[test]
fn provider_process_lost_during_output_stops_speech_and_fails_the_turn() {
    let _one = one_session();
    let run = run(Script::question(Answer::Hold(40)).then(When::Accepted(3), Act::ProviderGone));
    run.assert_invariants();
    assert!(
        run.result.is_err() && !is_mute(&run.result),
        "{:?}",
        run.result
    );
    let turn = first(&run);
    assert_eq!(run.outcomes()[&turn], ["runtime_failed"]);
    let lost = run.fired_at(Act::ProviderGone);
    // Only chunks already accepted around the loss; the 40-packet answer is cut.
    let frames: usize = run.accepted(turn).iter().map(|c| c.frames).sum();
    assert!(frames < 40 * PACKET, "{frames}");
    assert!(
        run.seen
            .chunks
            .iter()
            .all(|c| c.snapshot_issued_us <= lost + 100_000),
        "dispatch long after the provider was lost"
    );
}

#[test]
fn capture_fault_during_output_cancels_the_answer_and_ends_the_run() {
    let _one = one_session();
    let run = run(Script::question(Answer::Hold(40)).then(When::Accepted(3), Act::CaptureFault));
    run.assert_invariants();
    assert!(
        run.result.is_err() && !is_mute(&run.result),
        "{:?}",
        run.result
    );
    assert_eq!(run.outcomes()[&first(&run)], ["runtime_failed"]);
}

#[test]
fn capture_stall_during_a_question_ends_the_run_without_an_answer() {
    let _one = one_session();
    let run = run(Script {
        speech: 20..400,
        ..Script::question(Answer::Complete(12))
    }
    .then(When::Block(60), Act::CaptureSilent));
    run.assert_invariants();
    assert!(
        run.result.is_err() && !is_mute(&run.result),
        "{:?}",
        run.result
    );
    assert!(run.seen.chunks.is_empty());
    assert!(
        run.seen.ends.is_empty(),
        "a stalled question was sent as finished"
    );
}

#[test]
fn mute_while_answer_speech_is_queued_cuts_it_and_never_resumes() {
    let _one = one_session();
    let run = run(Script {
        actions: vec![(When::Accepted(3), Act::Mute)],
        ..Script::question(Answer::Complete(40))
    });
    run.assert_invariants();
    assert!(is_mute(&run.result), "{:?}", run.result);
    let turn = first(&run);
    assert_eq!(run.outcomes()[&turn], ["privacy_muted"]);
    let frames: usize = run.accepted(turn).iter().map(|c| c.frames).sum();
    assert!(frames < 40 * PACKET, "{frames}");
}

#[test]
fn sigterm_while_answer_speech_is_pending_stops_cleanly() {
    let _one = one_session();
    let run = run(Script {
        actions: vec![(When::Accepted(3), Act::Sigterm)],
        signals: true,
        ..Script::question(Answer::Hold(40))
    });
    run.assert_invariants();
    assert_eq!(run.events("service_stop_requested").len(), 1);
    assert_eq!(run.outcomes()[&first(&run)], ["service_stopped"]);
}

#[test]
fn failure_notice_plays_once_keeps_the_failure_and_yields_to_the_next_owner() {
    let _one = one_session();
    let run = run(Script {
        answers: vec![Answer::FailBeforeOutput, Answer::Complete(12)],
        actions: vec![
            (When::NoticeAccepted(2), Act::Speak),
            (When::Settled(2), Act::Mute),
        ],
        notices: true,
        ..Script::question(Answer::FailBeforeOutput)
    });
    run.assert_invariants();
    assert!(is_mute(&run.result), "{:?}", run.result);
    assert_eq!(run.seen.starts.len(), 2, "the next owner was not served");
    let (failed, next) = (run.seen.starts[0], run.seen.starts[1]);
    assert_eq!(run.outcomes()[&failed], ["provider_failed"]);
    assert_eq!(run.events("failure_notice_attempted").len(), 1);
    // No notice audio after the next owner was admitted.
    let admitted = run
        .events("input_admitted")
        .iter()
        .find(|e| e["turn"] == next)
        .map(|e| e["at_us"].as_u64().unwrap())
        .unwrap();
    for chunk in run.seen.chunks.iter().filter(|c| c.notice) {
        assert_eq!(chunk.turn, failed);
        assert!(chunk.snapshot_issued_us <= admitted, "{chunk:?}");
    }
    assert!(!run.accepted(next).is_empty());
}

#[test]
fn stalled_speaker_during_output_is_bounded_and_never_completes_the_answer() {
    let _one = one_session();
    let run =
        run(Script::question(Answer::Complete(40)).then(When::Accepted(3), Act::StallSpeaker));
    run.assert_invariants();
    let turn = first(&run);
    let outcome = &run.outcomes()[&turn];
    assert_ne!(outcome[0], "audio_written_unscored", "{outcome:?}");
    let stalled = run.fired_at(Act::StallSpeaker);
    let ended = run
        .events("turn_finished")
        .iter()
        .map(|e| e["at_us"].as_u64().unwrap())
        .min()
        .unwrap();
    eprintln!("stall to turn end {} ms", (ended - stalled) / 1_000);
}

/// Finding F1, retained as a failing reproducer. Not the pilot policy:
/// `vad-only` (the option default) has no speaker write-evidence lease, so a
/// speaker that keeps reading control but stops acknowledging chunks leaves
/// the answer open until something else ends the run. Under V1-baseline the
/// same stall ends the run in about 200 ms.
#[test]
#[ignore = "finding F1: unbounded stalled speaker under vad-only; see docs/pilot-faults.md"]
fn stalled_speaker_under_vad_only_is_bounded_too() {
    let _one = one_session();
    let run = run(Script {
        turn_taking: TurnTaking::VadOnly,
        ..Script::question(Answer::Complete(40))
    }
    .then(When::Accepted(3), Act::StallSpeaker));
    run.assert_invariants();
    let outcome = &run.outcomes()[&first(&run)];
    assert_ne!(outcome[0], "audio_written_unscored", "{outcome:?}");
}

/// Not the pilot policy: without the V1-baseline playback mask, speech over
/// the notice admits the next owner while the notice is still playing.
#[test]
fn failure_notice_yields_mid_play_to_a_new_owner_under_vad_only() {
    let _one = one_session();
    let run = run(Script {
        answers: vec![Answer::FailBeforeOutput, Answer::Complete(12)],
        actions: vec![
            (When::NoticeAccepted(1), Act::Speak),
            (When::Settled(2), Act::Mute),
        ],
        notices: true,
        turn_taking: TurnTaking::VadOnly,
        ..Script::question(Answer::FailBeforeOutput)
    });
    run.assert_invariants();
    assert_eq!(run.seen.starts.len(), 2, "the next owner was not served");
    let (failed, next) = (run.seen.starts[0], run.seen.starts[1]);
    assert_eq!(run.outcomes()[&failed].len(), 1);
    assert_ne!(run.outcomes()[&failed][0], "audio_written_unscored");
    assert_eq!(run.events("failure_notice_attempted").len(), 1);
    let admitted = run
        .events("input_admitted")
        .iter()
        .find(|e| e["turn"] == next)
        .map(|e| e["at_us"].as_u64().unwrap())
        .unwrap();
    for chunk in run.seen.chunks.iter().filter(|c| c.notice) {
        assert!(chunk.snapshot_issued_us <= admitted, "{chunk:?}");
    }
    assert!(!run.accepted(next).is_empty());
}
