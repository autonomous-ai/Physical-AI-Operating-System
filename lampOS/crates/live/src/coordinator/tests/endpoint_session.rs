//! Directed sessions through the real coordinator loop with opt-in endpoint
//! confirmation. Capture, privacy, provider and speaker are emulated peers on
//! the production channels; the endpoint worker is a real child process or an
//! in-process double. No audio device, cloud service or model is used.
use super::*;
use crate::{
    provider_link::{LinkContext, ProviderLinkState},
    provider_worker::ProviderInput,
    wire::{CaptureTiming, ReferenceTiming},
};
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

fn capture_peer(mut channels: WorkerChannels, script: Script, stop: Arc<AtomicBool>) {
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
                    started = Some(Instant::now());
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

fn privacy_peer(mut channels: WorkerChannels, stop: Arc<AtomicBool>) {
    let mut generation = 0;
    let mut next = Instant::now();
    while !stop.load(Ordering::SeqCst) && drain_control(&mut channels, &mut generation) {
        if Instant::now() >= next {
            channels
                .control
                .send(WorkerEvent::Privacy {
                    acquired_at_us: monotonic_us(),
                    muted: false,
                })
                .unwrap();
            next += Duration::from_millis(10);
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn provider_peer(mut channels: WorkerChannels, seen: Arc<Mutex<Seen>>, stop: Arc<AtomicBool>) {
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
    let mut generation = 0;
    while !stop.load(Ordering::SeqCst) && drain_control(&mut channels, &mut generation) {
        while let Some(input) = channels.data.receive::<ProviderInput>().unwrap() {
            seen.lock().unwrap().provider.push(match input {
                ProviderInput::Start { .. } => "start",
                ProviderInput::Audio { .. } => "audio",
                ProviderInput::End { .. } => "end",
            });
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn speaker_peer(mut channels: WorkerChannels, stop: Arc<AtomicBool>) {
    let mut generation = 0;
    while !stop.load(Ordering::SeqCst) && drain_control(&mut channels, &mut generation) {
        while channels.data.receive::<Value>().unwrap().is_some() {}
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
    let dir = SessionDirectory::create().unwrap();
    let (capture, capture_peer_channels) = Worker::sleeping_fixture(&dir.path, "capture");
    let (speaker, speaker_peer_channels) = Worker::sleeping_fixture(&dir.path, "speaker");
    let (provider, provider_peer_channels) = Worker::sleeping_fixture(&dir.path, "provider");
    let (privacy, privacy_peer_channels) = Worker::sleeping_fixture(&dir.path, "privacy");
    let stop = Arc::new(AtomicBool::new(false));
    let seen = Arc::new(Mutex::new(Seen::default()));
    let peers = [
        thread::spawn({
            let stop = stop.clone();
            move || capture_peer(capture_peer_channels, script, stop)
        }),
        thread::spawn({
            let stop = stop.clone();
            move || privacy_peer(privacy_peer_channels, stop)
        }),
        thread::spawn({
            let (stop, seen) = (stop.clone(), seen.clone());
            move || provider_peer(provider_peer_channels, seen, stop)
        }),
        thread::spawn({
            let stop = stop.clone();
            move || speaker_peer(speaker_peer_channels, stop)
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
