//! Directed sessions through the real coordinator loop with opt-in endpoint
//! confirmation. Capture, privacy, provider and speaker are emulated peers on
//! the production channels; the endpoint worker is a real child process or an
//! in-process double. No audio device, cloud service or model is used.
use super::*;
use crate::{
    provider_link::{LinkContext, ProviderLinkState},
    provider_worker::{ProviderInput, ProviderOutput},
    wire::{CaptureTiming, ReferenceTiming, SpeakerChunk},
};
use lamp_audio::ownership::PlaybackCursor;
use lamp_endpoint_ipc::{
    Availability, Classifier, Limits, Outcome, Submit, Token, Verdict, WorkerCommand,
    frame::{PROTOCOL, Response},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

const SPEECH: f32 = 0.95;
const QUIET: f32 = 0.05;

/// Speech probability of each 10 ms capture block, by block index.
type Script = fn(u64) -> f32;

#[derive(Default)]
struct Seen {
    provider: Vec<&'static str>,
    /// Every request the provider peer saw, in order.
    requests: Vec<SeenRequest>,
    /// Every speech chunk dispatched to the speaker: its turn and when.
    chunks: Vec<(u64, Instant)>,
    /// When the capture peer sent its first frame.
    capture_started: Option<Instant>,
}

#[derive(Clone, Debug)]
struct SeenRequest {
    request: u64,
    audio_blocks: usize,
    started_at: Instant,
    ended_at: Option<Instant>,
}

impl Seen {
    /// Capture block `index` (1-based) as an instant.
    fn block(&self, index: u64) -> Instant {
        self.capture_started.expect("capture started") + Duration::from_millis(10 * (index - 1))
    }
}

/// How the emulated provider answers each request after its end.
#[derive(Clone, Copy)]
struct ProviderReply {
    after_end: Duration,
    packets: usize,
}

fn drain_control(channels: &mut WorkerChannels, generation: &mut u64) -> bool {
    while let Some(control) = channels.control.receive::<Control>().unwrap() {
        match control {
            Control::Authority { snapshot } => *generation = snapshot.microphone_generation(),
            Control::Stop => return false,
            Control::StartCapture => {}
            _ => {}
        }
    }
    true
}

fn capture_peer(
    mut channels: WorkerChannels,
    script: Script,
    seen: Arc<Mutex<Seen>>,
    stop: Arc<AtomicBool>,
) {
    let mut generation = 0;
    let mut started = None;
    let mut sequence = 0u64;
    while !stop.load(Ordering::SeqCst) {
        while let Some(control) = channels.control.receive::<Control>().unwrap() {
            match control {
                Control::Authority { snapshot } => generation = snapshot.microphone_generation(),
                Control::StartCapture if started.is_none() => {
                    let at = monotonic_us();
                    channels
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
                    let now = Instant::now();
                    started = Some(now);
                    seen.lock().unwrap().capture_started = Some(now);
                }
                Control::Stop => return,
                _ => {}
            }
        }
        // Frames follow a fixed 10 ms schedule, like a capture device.
        while let Some(start) = started
            && start + Duration::from_millis(10 * sequence) <= Instant::now()
        {
            sequence += 1;
            let at = monotonic_us();
            let probability = script(sequence);
            let level = if probability >= 0.5 { 2_000 } else { 20 };
            let frame = MicrophoneFrame {
                epoch: 1,
                dsp_epoch: 1,
                privacy_generation: generation,
                frame_sequence: sequence,
                read_completed_at_us: at,
                probability,
                samples: (0..160)
                    .map(|i| if i % 2 == 0 { level } else { -level })
                    .collect(),
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
            };
            channels.data.send(frame).unwrap();
        }
        thread::sleep(Duration::from_millis(1));
    }
}

/// Fresh privacy observations every 10 ms; muted from capture block
/// `mute_at` when given.
fn privacy_peer(
    mut channels: WorkerChannels,
    seen: Arc<Mutex<Seen>>,
    mute_at: Option<u64>,
    stop: Arc<AtomicBool>,
) {
    let mut generation = 0;
    let mut next = Instant::now();
    while !stop.load(Ordering::SeqCst) && drain_control(&mut channels, &mut generation) {
        if Instant::now() >= next {
            let muted = {
                let seen = seen.lock().unwrap();
                match (mute_at, seen.capture_started) {
                    (Some(block), Some(_)) => Instant::now() >= seen.block(block),
                    _ => false,
                }
            };
            channels
                .control
                .send(WorkerEvent::Privacy {
                    acquired_at_us: monotonic_us(),
                    muted,
                })
                .unwrap();
            next += Duration::from_millis(10);
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn provider_peer(
    mut channels: WorkerChannels,
    seen: Arc<Mutex<Seen>>,
    reply: Option<ProviderReply>,
    stop: Arc<AtomicBool>,
) {
    let at = monotonic_us();
    channels
        .control
        .send(WorkerEvent::ProviderLink {
            link: crate::provider_link::ProviderLink {
                revision: 1,
                session: 1,
                observed_at_us: at,
                source_clock: None,
                state: ProviderLinkState::Ready {
                    setup_us: 1_000,
                    attempts: 1,
                    context: LinkContext::Initial,
                    outage_started_at_us: None,
                    after: None,
                },
            },
        })
        .unwrap();
    // Output is credited: the coordinator grants two packets, then more.
    let (mut granted, mut sent) = (2u64, 0u64);
    let mut outbox: VecDeque<ProviderOutput> = VecDeque::new();
    let mut due: Vec<(Instant, u64)> = Vec::new();
    let mut sequence = 0u64;
    while !stop.load(Ordering::SeqCst) {
        let mut open = true;
        while let Some(control) = channels.control.receive::<Control>().unwrap() {
            match control {
                Control::ProviderOutputCapacity { through } => granted = granted.max(through),
                Control::Stop => open = false,
                _ => {}
            }
        }
        if !open {
            break;
        }
        while let Some(input) = channels.data.receive::<ProviderInput>().unwrap() {
            let mut seen = seen.lock().unwrap();
            match input {
                ProviderInput::Start {
                    request, session, ..
                } => {
                    seen.provider.push("start");
                    seen.requests.push(SeenRequest {
                        request,
                        audio_blocks: 0,
                        started_at: Instant::now(),
                        ended_at: None,
                    });
                    outbox.push_back(ProviderOutput::Started {
                        request,
                        session,
                        waiting_for_barrier: false,
                    });
                }
                ProviderInput::Audio { request, .. } => {
                    seen.provider.push("audio");
                    if let Some(entry) = seen.requests.iter_mut().find(|r| r.request == request) {
                        entry.audio_blocks += 1;
                    }
                }
                ProviderInput::End { request, .. } => {
                    seen.provider.push("end");
                    if let Some(entry) = seen.requests.iter_mut().find(|r| r.request == request) {
                        entry.ended_at = Some(Instant::now());
                    }
                    if let Some(reply) = reply {
                        due.push((Instant::now() + reply.after_end, request));
                    }
                }
            }
        }
        let now = Instant::now();
        due.retain(|&(at, request)| {
            if at > now {
                return true;
            }
            let packets = reply.map_or(0, |reply| reply.packets);
            for _ in 0..packets {
                sequence += 1;
                outbox.push_back(ProviderOutput::Audio {
                    request,
                    sequence,
                    provider_event_at_us: monotonic_us(),
                    source_frames: 960,
                    source_offset: 0,
                    samples: vec![1_000; 960],
                });
            }
            outbox.push_back(ProviderOutput::GenerationComplete { request });
            outbox.push_back(ProviderOutput::TurnComplete {
                request,
                idle: true,
            });
            false
        });
        while sent < granted
            && let Some(output) = outbox.pop_front()
        {
            channels.data.send(output).unwrap();
            sent += 1;
        }
        thread::sleep(Duration::from_millis(1));
    }
}

/// Records every chunk and acknowledges it as ALSA would, so the v1-baseline
/// mask sees genuine write evidence.
fn speaker_peer(mut channels: WorkerChannels, seen: Arc<Mutex<Seen>>, stop: Arc<AtomicBool>) {
    let mut generation = 0;
    let mut frame = 1_000u64;
    while !stop.load(Ordering::SeqCst) && drain_control(&mut channels, &mut generation) {
        while let Some(chunk) = channels.data.receive::<SpeakerChunk>().unwrap() {
            let owner = chunk.permit.owner();
            seen.lock()
                .unwrap()
                .chunks
                .push((owner.turn(), Instant::now()));
            let frames = chunk.samples.len();
            let at = monotonic_us();
            channels
                .control
                .send(WorkerEvent::Playback {
                    owner,
                    first_sample: PlaybackCursor { epoch: 1, frame },
                    end_sample: PlaybackCursor {
                        epoch: 1,
                        frame: frame + frames as u64,
                    },
                    queue_observed_at_us: at,
                    turn: owner.turn(),
                    sequence: chunk.chunk_sequence,
                    queued_frames: frames,
                    accepted_frames: frames,
                    observed_at_us: at,
                })
                .unwrap();
            frame += frames as u64;
        }
        thread::sleep(Duration::from_millis(1));
    }
}

struct Session {
    result: Result<()>,
    trace: Vec<Value>,
    seen: Seen,
    elapsed: Duration,
}

impl Session {
    fn events(&self, kind: &str) -> Vec<&Value> {
        self.trace
            .iter()
            .filter(|row| row["kind"] == kind)
            .collect()
    }
}

fn run(seconds: u64, script: Script, endpoint: EndpointRig) -> Session {
    run_with(seconds, script, endpoint, None)
}

fn run_with(
    seconds: u64,
    script: Script,
    endpoint: EndpointRig,
    reply: Option<ProviderReply>,
) -> Session {
    run_muting(seconds, script, endpoint, reply, None)
}

fn run_muting(
    seconds: u64,
    script: Script,
    endpoint: EndpointRig,
    reply: Option<ProviderReply>,
    mute_at: Option<u64>,
) -> Session {
    let dir = SessionDirectory::create().unwrap();
    let (capture, capture_peer_channels) = Worker::sleeping_fixture(&dir.path, "capture");
    let (speaker, speaker_peer_channels) = Worker::sleeping_fixture(&dir.path, "speaker");
    let (provider, provider_peer_channels) = Worker::sleeping_fixture(&dir.path, "provider");
    let (privacy, privacy_peer_channels) = Worker::sleeping_fixture(&dir.path, "privacy");
    let stop = Arc::new(AtomicBool::new(false));
    let seen = Arc::new(Mutex::new(Seen::default()));
    let peers = [
        thread::spawn({
            let (stop, seen) = (stop.clone(), seen.clone());
            move || capture_peer(capture_peer_channels, script, seen, stop)
        }),
        thread::spawn({
            let (stop, seen) = (stop.clone(), seen.clone());
            move || privacy_peer(privacy_peer_channels, seen, mute_at, stop)
        }),
        thread::spawn({
            let (stop, seen) = (stop.clone(), seen.clone());
            move || provider_peer(provider_peer_channels, seen, reply, stop)
        }),
        thread::spawn({
            let (stop, seen) = (stop.clone(), seen.clone());
            move || speaker_peer(speaker_peer_channels, seen, stop)
        }),
    ];
    let mut rig = Rig {
        provider,
        capture,
        speaker,
        privacy,
        ring: None.into(),
        camera: None,
        ring_channel_ceiling: None,
        diagnostic_directory: None,
        noise_suppression: NoiseSuppression::On,
        turn_taking: TurnTaking::V1Baseline,
        playout_start: PlayoutStart::default(),
        endpoint: Some(endpoint),
    };
    let mut owner = Controller::new(new_boot().unwrap(), now());
    let mut trace = SessionState::finite();
    let mut reply = None;
    let started = Instant::now();
    let result = conversation(
        &mut rig,
        &mut owner,
        Lifetime::Finite(seconds),
        &mut trace,
        StartupEvents::default(),
        &mut reply,
        &mut None,
        &Termination::inactive(),
    );
    let elapsed = started.elapsed();
    // Sleeping fixtures need forced reaping; only the endpoint record matters.
    let _ = rig.shutdown(&mut trace);
    stop.store(true, Ordering::SeqCst);
    for peer in peers {
        peer.join().unwrap();
    }
    let seen = std::mem::take(&mut *seen.lock().unwrap());
    Session {
        result,
        trace: trace.to_vec(),
        seen,
        elapsed,
    }
}

/// A real child process that replays fixed answer bytes and then sleeps or
/// exits, written with POSIX `printf` octal escapes.
fn scripted_process(responses: &[Response], then: &str, limits: Limits) -> EndpointRig {
    let bytes: String = responses
        .iter()
        .flat_map(Response::encode)
        .map(|byte| format!("\\{byte:03o}"))
        .collect();
    EndpointRig::process(
        WorkerCommand {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), format!("printf '{bytes}'; {then}").into()],
            env: Vec::new(),
            // `ready()` reports this identity.
            model_sha256: [0; 32],
        },
        limits,
    )
}

fn ready() -> Response {
    Response::Ready {
        protocol: PROTOCOL,
        load_us: 1,
        cold_us: 1,
        model_sha256: [0; 32],
    }
}

/// 0.5 s quiet, 1.5 s speech, then quiet.
fn one_question(block: u64) -> f32 {
    if (50..200).contains(&block) {
        SPEECH
    } else {
        QUIET
    }
}

fn endpoint_rows(session: &Session) -> Vec<&Value> {
    session.events("local_endpoint")
}

#[test]
fn missing_model_session_ends_on_the_unchanged_silence_fallback() {
    let missing = Response::Error {
        request: 0,
        reason: "model /lamp/smart-turn-v3.2-gpu.onnx: No such file or directory".into(),
    };
    let session = run(
        4,
        one_question,
        scripted_process(&[missing], "exit 2", Limits::default()),
    );
    session.result.as_ref().unwrap();
    let policy = &session.events("endpoint_policy")[0];
    assert_eq!(policy["basis"], "smart_turn_with_silence_fallback");
    assert_eq!(policy["silence_wait_ms"], 1_200);
    let states: Vec<_> = session
        .events("endpoint_worker")
        .iter()
        .map(|row| row["availability"]["state"].as_str().unwrap().to_owned())
        .collect();
    // The worker may fail before the loop first looks; it never became ready.
    assert_eq!(
        states.last().map(String::as_str),
        Some("failed"),
        "{states:?}"
    );
    assert!(!states.contains(&"ready".into()), "{states:?}");
    let failed = session.events("endpoint_worker").last().copied().unwrap();
    assert!(
        failed["availability"]["detail"]
            .as_str()
            .unwrap()
            .contains("No such file")
    );
    assert_eq!(failed["report"]["spawned"], 1);
    let endpoints = endpoint_rows(&session);
    assert_eq!(endpoints.len(), 1, "{endpoints:?}");
    let decision = &endpoints[0]["endpoint"];
    assert_eq!(decision["basis"], "silence_fallback");
    assert_eq!(decision["fallback"], "failed");
    assert_eq!(decision["silence_ms"], 1_200);
    assert_eq!(endpoints[0]["includes_silence_wait_ms"], 1_200);
    assert_eq!(
        session
            .seen
            .provider
            .iter()
            .filter(|kind| **kind == "end")
            .count(),
        1
    );
    assert!(session.seen.provider.contains(&"start"));
    let last = session.events("endpoint_worker_final")[0];
    assert_eq!(last["report"]["kills"], 0);
    assert_eq!(last["reaped"], true);
}

#[test]
fn hung_worker_never_delays_capture_endpoint_or_session_end() {
    let limits = Limits {
        hang: Duration::from_millis(700),
        backoff: Duration::from_millis(100),
        ..Limits::default()
    };
    // Two questions; the worker accepts each request and never answers.
    fn two_questions(block: u64) -> f32 {
        if (50..150).contains(&block) || (300..400).contains(&block) {
            SPEECH
        } else {
            QUIET
        }
    }
    let session = run(
        6,
        two_questions,
        scripted_process(&[ready()], "exec sleep 60", limits),
    );
    // A stalled loop would have failed the 100 ms capture and privacy leases.
    session.result.as_ref().unwrap();
    assert!(
        session.elapsed < Duration::from_secs(8),
        "{:?}",
        session.elapsed
    );
    let endpoints = endpoint_rows(&session);
    assert!(!endpoints.is_empty());
    let decision = &endpoints[0]["endpoint"];
    assert_eq!(decision["basis"], "silence_fallback");
    assert_eq!(decision["fallback"], "timeout");
    assert_eq!(decision["silence_ms"], 1_200);
    let states: Vec<_> = session
        .events("endpoint_worker")
        .iter()
        .map(|row| row["availability"]["state"].as_str().unwrap().to_owned())
        .collect();
    // Loading started with the other workers and may already be complete.
    assert!(
        matches!(states[0].as_str(), "loading" | "ready"),
        "{states:?}"
    );
    assert!(states.contains(&"recovering".into()), "{states:?}");
    let last = session.events("endpoint_worker_final")[0];
    // The worker hung during shutdown too: killed after its grace, then reaped.
    assert_eq!(last["reaped"], true, "{last}");
    assert!(last["report"]["kills"].as_u64().unwrap() >= 1, "{last}");
    assert!(last["report"]["spawned"].as_u64().unwrap() >= 2, "{last}");
}

/// Answers each request with a fixed probability after a real delay.
struct Delayed {
    probability: f32,
    delay: Duration,
    pending: Option<(Token, u64, Instant)>,
    requests: Arc<Mutex<Vec<Token>>>,
}

impl Classifier for Delayed {
    fn submit(&mut self, token: Token, _pcm: &[i16]) -> Submit {
        if self.pending.is_some() {
            return Submit::Busy;
        }
        let mut requests = self.requests.lock().unwrap();
        requests.push(token);
        self.pending = Some((token, requests.len() as u64, Instant::now()));
        Submit::Accepted
    }
    fn poll(&mut self) -> Option<Outcome> {
        let (token, request, at) = self.pending?;
        if at.elapsed() < self.delay {
            return None;
        }
        self.pending = None;
        Some(Outcome {
            token,
            worker: 1,
            request,
            result: Ok(Verdict {
                probability: self.probability,
                round_trip_us: self.delay.as_micros() as u64,
                ..Verdict::default()
            }),
        })
    }
    fn availability(&self) -> Availability {
        Availability::Ready
    }
    fn cancel(&mut self) {}
}

#[test]
fn complete_verdict_ends_early_but_resumed_speech_keeps_the_turn() {
    // Speech, a 350 ms pause (verdict arrives, commit point not reached), more
    // speech, then the final pause.
    fn resumed(block: u64) -> f32 {
        if (50..150).contains(&block) || (185..260).contains(&block) {
            SPEECH
        } else {
            QUIET
        }
    }
    let requests = Arc::new(Mutex::new(Vec::new()));
    let double = Delayed {
        probability: 0.95,
        delay: Duration::from_millis(60),
        pending: None,
        requests: requests.clone(),
    };
    let session = run(4, resumed, EndpointRig::double(Box::new(double)));
    session.result.as_ref().unwrap();
    let endpoints = endpoint_rows(&session);
    assert_eq!(endpoints.len(), 1, "split: {endpoints:?}");
    let decision = &endpoints[0]["endpoint"];
    assert_eq!(decision["basis"], "model_complete");
    assert_eq!(decision["silence_ms"], 1_000);
    // The verdict about the first pause was obsolete once speech resumed.
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert_eq!(requests[0].utterance, requests[1].utterance);
    assert_ne!(requests[0].generation, requests[1].generation);
    assert_eq!(decision["verdict"]["request"], 2);
    assert_eq!(
        session
            .seen
            .provider
            .iter()
            .filter(|kind| **kind == "end")
            .count(),
        1
    );
}

/// The installed pair: `LAMP_ENDPOINT_WORKER` and `LAMP_SMART_TURN_MODEL`.
/// Synthetic tones are not speech, so only provenance is asserted.
#[test]
#[ignore = "requires a built lamp-endpoint-worker and the pinned model"]
fn real_worker_session_records_verdict_provenance() {
    let worker = std::env::var_os("LAMP_ENDPOINT_WORKER").expect("LAMP_ENDPOINT_WORKER");
    let model = std::env::var_os("LAMP_SMART_TURN_MODEL").expect("LAMP_SMART_TURN_MODEL");
    let endpoint = EndpointRig::process(
        WorkerCommand::smart_turn(worker.into(), model.into()),
        Limits::default(),
    );
    let session = run(4, one_question, endpoint);
    session.result.as_ref().unwrap();
    for row in session.events("endpoint_worker") {
        eprintln!("{row}");
    }
    let endpoints = endpoint_rows(&session);
    assert_eq!(endpoints.len(), 1, "{endpoints:?}");
    let decision = &endpoints[0]["endpoint"];
    assert!(
        matches!(
            decision["basis"].as_str(),
            Some("model_complete" | "model_uncertain" | "model_incomplete")
        ),
        "{decision}"
    );
    assert_eq!(decision["verdict"]["worker"], 1);
    let last = session.events("endpoint_worker_final")[0];
    assert_eq!(last["report"]["ready"], 1);
    assert_eq!(last["report"]["kills"], 0);
    eprintln!("{decision}\n{last}");
}

/// A verified physical mute during an active reply, with an endpoint worker
/// attached, through `conversation` and `finish_active_run` as the service
/// runs them. The four audio/provider fixtures stop cleanly, so only the
/// endpoint worker's teardown can fail the cleanup.
fn mute_with_endpoint(endpoint: EndpointRig) -> (Result<()>, SessionState) {
    use crate::service_privacy::PhysicalMute;
    let directory = SessionDirectory::create().unwrap();
    let (speaker, speaker_peer, speaker_input) =
        Worker::cooperative_fixture(&directory.path, "speaker");
    let (capture, capture_peer, capture_input) =
        Worker::cooperative_fixture(&directory.path, "capture");
    let (provider, provider_peer, provider_input) =
        Worker::cooperative_fixture(&directory.path, "provider");
    let (privacy, privacy_peer, privacy_input) =
        Worker::cooperative_fixture(&directory.path, "privacy");
    let peers = [
        (speaker_peer, speaker_input, false),
        (capture_peer, capture_input, false),
        (provider_peer, provider_input, false),
        (privacy_peer, privacy_input, true),
    ]
    .map(|(mut channels, input, privacy)| {
        thread::spawn(move || {
            let mut sent = false;
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match channels.control.receive::<Control>().unwrap() {
                    Some(Control::Authority { .. }) if privacy && !sent => {
                        channels
                            .control
                            .send(WorkerEvent::Privacy {
                                acquired_at_us: monotonic_us(),
                                muted: true,
                            })
                            .unwrap();
                        sent = true;
                    }
                    Some(Control::Stop) => break,
                    _ => {}
                }
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(1));
            }
            drop(input);
            channels
        })
    });
    let mut rig = Rig {
        speaker,
        capture,
        provider,
        privacy,
        ring: None.into(),
        camera: None,
        ring_channel_ceiling: None,
        diagnostic_directory: None,
        noise_suppression: NoiseSuppression::On,
        turn_taking: TurnTaking::V1Baseline,
        playout_start: PlayoutStart::default(),
        endpoint: Some(endpoint),
    };
    let (mut controller, current, _) = active_reply();
    let mut reply = Some(current);
    let mut trace = SessionState::default();
    let result = conversation(
        &mut rig,
        &mut controller,
        Lifetime::Serve,
        &mut trace,
        StartupEvents::default(),
        &mut reply,
        &mut None,
        &Termination::inactive(),
    );
    assert!(
        result
            .as_ref()
            .unwrap_err()
            .downcast_ref::<PhysicalMute>()
            .is_some()
    );
    let result = finish_active_run(
        &mut rig,
        &mut controller,
        &mut reply,
        &mut trace,
        &mut None,
        result,
    );
    for peer in peers {
        drop(peer.join().unwrap());
    }
    drop(rig);
    let _ = fs::remove_dir(&directory.path);
    (result, trace)
}

fn final_record(trace: &SessionState) -> Value {
    trace
        .iter()
        .find(|row| row["kind"] == "endpoint_worker_final")
        .cloned()
        .expect("endpoint teardown record")
}

#[test]
fn unconfirmed_endpoint_exit_fails_mute_cleanup_and_never_reopens_service() {
    use crate::service_privacy::{PhysicalMute, WaitOutcome};
    // A worker that ignores its closed input and is only killed after 1.5 s:
    // teardown's 500 ms bound passes with its process still alive.
    let limits = Limits {
        grace: Duration::from_millis(1_500),
        ..Limits::default()
    };
    let endpoint = scripted_process(&[ready()], "exec sleep 60", limits);
    let (result, mut trace) = mute_with_endpoint(endpoint);
    let error = result.unwrap_err();
    assert!(
        error.downcast_ref::<PhysicalMute>().is_none(),
        "unconfirmed endpoint exit kept a restartable mute: {error}"
    );
    assert!(error.to_string().contains("physical mute"), "{error}");
    assert!(
        error
            .to_string()
            .contains("endpoint worker exit not confirmed"),
        "{error}"
    );
    let record = final_record(&trace);
    assert_eq!(record["reaped"], false, "{record}");
    assert_eq!(record["teardown"], "Pending");
    let waits = std::cell::Cell::new(0);
    let mut failure = Some(error);
    let restarted = crate::coordinator::service::supervise(
        &mut trace,
        &mut None,
        &Termination::inactive(),
        |_, _| {
            waits.set(waits.get() + 1);
            Ok(WaitOutcome::Allowed)
        },
        |_, _| {
            Err(failure
                .take()
                .expect("must never reopen after failed cleanup"))
        },
    );
    assert!(restarted.is_err());
    assert_eq!(waits.get(), 1);
}

#[test]
fn confirmed_endpoint_exit_keeps_the_restartable_mute() {
    use crate::service_privacy::PhysicalMute;
    let endpoint = scripted_process(&[ready()], "exec sleep 60", Limits::default());
    let (result, trace) = mute_with_endpoint(endpoint);
    let error = result.unwrap_err();
    assert!(
        error.downcast_ref::<PhysicalMute>().is_some(),
        "confirmed teardown lost the mute: {error}"
    );
    let record = final_record(&trace);
    assert_eq!(record["reaped"], true, "{record}");
    assert_eq!(record["teardown"], "Reaped");
}

fn early_policy() -> crate::activity::ConfirmPolicy {
    crate::activity::ConfirmPolicy {
        early: Some(crate::activity::EarlyEnd::CANDIDATE),
        ..crate::activity::ConfirmPolicy::default()
    }
}

/// An in-process verdict of `probability` for every pause, after `delay`.
fn confident(probability: f32, delay: Duration) -> (EndpointRig, Arc<Mutex<Vec<Token>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let double = Delayed {
        probability,
        delay,
        pending: None,
        requests: requests.clone(),
    };
    (
        EndpointRig::double(Box::new(double)).with_policy(early_policy()),
        requests,
    )
}

fn provisional_events(session: &Session) -> Vec<String> {
    session
        .events("endpoint_provisional")
        .iter()
        .map(|row| row["event"]["kind"].as_str().unwrap().to_owned())
        .collect()
}

fn chunk_turns(session: &Session) -> Vec<u64> {
    let mut turns: Vec<u64> = session.seen.chunks.iter().map(|(turn, _)| *turn).collect();
    turns.dedup();
    turns
}

/// 1.0 s of speech from block 50, then quiet.
fn single(block: u64) -> f32 {
    if (50..150).contains(&block) {
        SPEECH
    } else {
        QUIET
    }
}

/// Speech, a 700 ms pause, the rest of the thought, then quiet.
fn continued(block: u64) -> f32 {
    if (50..150).contains(&block) || (220..320).contains(&block) {
        SPEECH
    } else {
        QUIET
    }
}

#[test]
fn early_end_starts_the_answer_sooner_but_stays_silent_until_confirmed() {
    let (endpoint, _) = confident(0.95, Duration::from_millis(60));
    let reply = ProviderReply {
        after_end: Duration::from_millis(150),
        packets: 3,
    };
    let session = run_with(5, single, endpoint, Some(reply));
    session.result.as_ref().unwrap();
    let seen = &session.seen;
    assert_eq!(seen.requests.len(), 1, "{:?}", seen.requests);
    // Speech ends after block 149: the end of input reaches the provider at a
    // 500 ms pause, not the baseline's 1.2 s.
    let ended = seen.requests[0].ended_at.unwrap();
    assert!(ended >= seen.block(199), "ended before 500 ms of pause");
    assert!(ended < seen.block(240), "end of input not early");
    let endpoint = &endpoint_rows(&session)[0]["endpoint"];
    assert_eq!(endpoint["basis"], "model_early");
    assert_eq!(endpoint["silence_ms"], 500);
    // The answer is ready about 150 ms later but nothing is spoken until the
    // baseline's 1.2 s of quiet confirmed the end.
    let first_chunk = seen.chunks.first().expect("answer spoken").1;
    assert!(first_chunk >= seen.block(268), "spoke before confirmation");
    assert_eq!(chunk_turns(&session), [1]);
    assert_eq!(provisional_events(&session), ["confirmed"]);
}

#[test]
fn continuation_while_the_old_answer_arrives_replaces_it_and_keeps_both_parts() {
    let (endpoint, tokens) = confident(0.95, Duration::from_millis(60));
    // Request 1's audio arrives about 300 ms after its early end: during the
    // resumed speech, before anything could be spoken.
    let reply = ProviderReply {
        after_end: Duration::from_millis(300),
        packets: 2,
    };
    let session = run_with(6, continued, endpoint, Some(reply));
    session.result.as_ref().unwrap();
    let seen = &session.seen;
    assert_eq!(seen.requests.len(), 2, "{:?}", seen.requests);
    assert!(seen.requests[0].ended_at.unwrap() < seen.block(215));
    // The continuation's request carries its opening: pre-roll, the six onset
    // blocks and all of the 100 resumed speech blocks.
    let second = &seen.requests[1];
    assert!(second.audio_blocks >= 100, "{second:?}");
    assert!(second.started_at < seen.block(240));
    // Only the successor is ever spoken; the superseded answer never is.
    assert_eq!(chunk_turns(&session), [second.request]);
    assert_eq!(provisional_events(&session), ["continued", "confirmed"]);
    let cancelled = session
        .events("turn_finished")
        .into_iter()
        .find(|row| row["turn"] == seen.requests[0].request)
        .cloned()
        .expect("first turn finished");
    assert_eq!(cancelled["outcome"], "user_interrupted");
    assert_eq!(cancelled["provider_audio_seen"], false, "{cancelled}");
    // Each utterance got its own verdict request.
    let tokens = tokens.lock().unwrap();
    assert_eq!(tokens.len(), 2);
    assert_ne!(tokens[0].utterance, tokens[1].utterance);
}

#[test]
fn answer_held_by_the_gate_is_dropped_when_the_person_continues() {
    let (endpoint, _) = confident(0.95, Duration::from_millis(60));
    // Request 1's answer is complete before the person resumes, and is held.
    let reply = ProviderReply {
        after_end: Duration::from_millis(30),
        packets: 2,
    };
    let session = run_with(6, continued, endpoint, Some(reply));
    session.result.as_ref().unwrap();
    let seen = &session.seen;
    assert_eq!(seen.requests.len(), 2);
    assert_eq!(chunk_turns(&session), [seen.requests[1].request]);
    let first = session
        .events("turn_finished")
        .into_iter()
        .find(|row| row["turn"] == seen.requests[0].request)
        .cloned()
        .unwrap();
    assert_eq!(first["provider_audio_seen"], true, "{first}");
    assert_eq!(first["outcome"], "user_interrupted");
}

#[test]
fn two_continuations_then_a_genuine_end() {
    fn twice(block: u64) -> f32 {
        if (50..120).contains(&block) || (190..260).contains(&block) || (330..400).contains(&block)
        {
            SPEECH
        } else {
            QUIET
        }
    }
    let (endpoint, _) = confident(0.95, Duration::from_millis(60));
    let reply = ProviderReply {
        after_end: Duration::from_millis(200),
        packets: 2,
    };
    let session = run_with(7, twice, endpoint, Some(reply));
    session.result.as_ref().unwrap();
    let seen = &session.seen;
    assert_eq!(seen.requests.len(), 3, "{:?}", seen.requests);
    assert_eq!(chunk_turns(&session), [seen.requests[2].request]);
    assert_eq!(
        provisional_events(&session),
        ["continued", "continued", "confirmed"]
    );
    assert!(seen.chunks[0].1 >= seen.block(518));
}

#[test]
fn early_mode_without_a_usable_model_is_the_baseline() {
    let missing = Response::Error {
        request: 0,
        reason: "model missing".into(),
    };
    for (endpoint, fallback) in [
        (
            scripted_process(&[missing], "exit 2", Limits::default()).with_policy(early_policy()),
            "failed",
        ),
        (
            scripted_process(&[ready()], "exec sleep 60", Limits::default())
                .with_policy(early_policy()),
            "timeout",
        ),
    ] {
        let reply = ProviderReply {
            after_end: Duration::from_millis(100),
            packets: 2,
        };
        let session = run_with(5, single, endpoint, Some(reply));
        session.result.as_ref().unwrap();
        let endpoint = &endpoint_rows(&session)[0]["endpoint"];
        assert_eq!(endpoint["basis"], "silence_fallback");
        assert_eq!(endpoint["fallback"], fallback);
        assert_eq!(endpoint["silence_ms"], 1_200);
        assert!(provisional_events(&session).is_empty());
        let seen = &session.seen;
        assert!(seen.requests[0].ended_at.unwrap() >= seen.block(268));
        assert_eq!(chunk_turns(&session), [1]);
    }
}

#[test]
fn privacy_during_the_provisional_end_stops_without_any_answer() {
    let (endpoint, _) = confident(0.95, Duration::from_millis(60));
    let reply = ProviderReply {
        after_end: Duration::from_millis(100),
        packets: 2,
    };
    // The early end is at block 199; the mute arrives at block 230, while
    // the answer is ready but held.
    let session = run_muting(5, single, endpoint, Some(reply), Some(230));
    let error = session.result.as_ref().unwrap_err().to_string();
    assert!(error.contains("privacy"), "{error}");
    assert!(session.seen.chunks.is_empty(), "spoke after the mute");
    assert!(session.elapsed < Duration::from_secs(4));
    assert_eq!(
        endpoint_rows(&session)[0]["endpoint"]["basis"],
        "model_early"
    );
}
