//! The opt-in early end of input (`smart-turn-early`) through the real
//! conversation loop and the real Coordinated provider worker, started as
//! `--provider-continuation kept-start` starts it, against a scripted
//! in-memory service. Capture, privacy and speaker are the emulated peers of
//! `endpoint_session`; the endpoint classifier is an in-process double. No
//! audio device, cloud service or model is used, so every latency here is a
//! candidate result about this loop, never evidence about a person or Gemini.
use super::endpoint_session::{
    QUIET, SPEECH, Script, Seen, capture_peer, delayed, one_session, privacy_peer, ready,
    scripted_process, speaker_peer,
};
use super::*;
use crate::{
    activity::{ConfirmPolicy, EarlyEnd},
    provider_worker::{Continuation, serve_coordinated_test},
};
use lamp_endpoint_ipc::{
    Availability, Classifier, Limits, Outcome, Submit, Token, Verdict, frame::Response,
};
use lamp_gemini::{
    BarrierPolicy, Credential, RecoveryPolicy, Resumption, SessionConfig, Supervisor,
    testing::{Dial, audio, connector, sent_audio},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
};
use tokio::sync::mpsc::UnboundedReceiver;

/// How the scripted service answers each activity end.
#[derive(Clone, Copy)]
struct Answering {
    /// The first answer packet follows the activity end by this much.
    after: Duration,
    /// Packets of 100 ms of 24 kHz audio, `PACKET_GAP` apart, then idle.
    packets: usize,
    /// Close the connection with 1011 instead of answering this activity
    /// end (1-based).
    fail_on_end: Option<usize>,
}

const PACKET_GAP: Duration = Duration::from_millis(40);

impl Answering {
    fn after(ms: u64) -> Self {
        Self {
            after: Duration::from_millis(ms),
            packets: 5,
            fail_on_end: None,
        }
    }
}

/// What the service saw and did, in order.
#[derive(Clone, Debug, PartialEq)]
enum Wire {
    Dial {
        resumed: bool,
    },
    Start,
    /// One input message; `loud` when it carries a speech block.
    Audio {
        loud: bool,
    },
    End,
    Answer {
        end: usize,
    },
    Interrupted,
    Idle,
    Closed,
}

type Log = Arc<Mutex<Vec<(Instant, Wire)>>>;

fn interrupted() -> Value {
    json!({"serverContent":{"interrupted":true}})
}

fn idle() -> Value {
    json!({"serverContent":{"turnComplete":true,"interactionStatus":"IDLE"}})
}

/// Answers every activity end after `answering.after`. A new activity start
/// interrupts an answer still under way, as the live service does; with the
/// kept start the worker writes that start only after the earlier answer
/// began or settled, or after its patience.
async fn service(
    mut dials: UnboundedReceiver<Dial>,
    answering: Answering,
    log: Log,
    stop: Arc<AtomicBool>,
) {
    let note = |wire: Wire| log.lock().unwrap().push((Instant::now(), wire));
    let mut ends = 0usize;
    let mut current: Option<Dial> = None;
    // The answer under way: its activity end, when its next packet is due,
    // and how many packets remain.
    let mut answer: Option<(usize, Instant, usize)> = None;
    while !stop.load(Ordering::SeqCst) {
        if let Ok(mut dial) = dials.try_recv() {
            dial.service.accept().await;
            // As the live service does, so that a lost connection resumes.
            dial.service
                .send(json!({"sessionResumptionUpdate":{
                    "newHandle":"endpoint-fast-handle","resumable":true}}))
                .await;
            note(Wire::Dial {
                resumed: dial.resumed,
            });
            answer = None;
            current = Some(dial);
        }
        let Some(dial) = current.as_mut() else {
            tokio::time::sleep(Duration::from_millis(1)).await;
            continue;
        };
        if let Some(message) = dial.service.try_receive(Duration::from_millis(1)).await {
            let input = &message["realtimeInput"];
            if input.get("activityStart").is_some() {
                note(Wire::Start);
                if answer.take().is_some() {
                    dial.service.burst([interrupted(), idle()]).await;
                    note(Wire::Interrupted);
                }
            } else if input.get("activityEnd").is_some() {
                ends += 1;
                note(Wire::End);
                if answering.fail_on_end == Some(ends) {
                    dial.service.close(1011, "scripted service failure").await;
                    note(Wire::Closed);
                    current = None;
                    answer = None;
                    continue;
                }
                answer = Some((ends, Instant::now() + answering.after, answering.packets));
            } else if let Some(samples) = sent_audio(&message) {
                note(Wire::Audio {
                    loud: samples.iter().any(|sample| sample.unsigned_abs() >= 1_000),
                });
            }
            continue;
        }
        if let Some((end, due, left)) = answer.as_mut()
            && Instant::now() >= *due
        {
            dial.service.send(audio(*end as i16 * 100, 2_400)).await;
            note(Wire::Answer { end: *end });
            *left -= 1;
            *due += PACKET_GAP;
            if *left == 0 {
                dial.service.send(idle()).await;
                note(Wire::Idle);
                answer = None;
            }
        }
    }
}

/// The early end evaluated as the candidate, with a classifier double that
/// answers `probability` 60 ms after each request.
fn early(probability: f32) -> EndpointRig {
    delayed(probability, Duration::from_millis(60)).with_policy(ConfirmPolicy {
        early: Some(EarlyEnd::CANDIDATE),
        ..ConfirmPolicy::default()
    })
}

struct Fast {
    result: Result<()>,
    trace: Vec<Value>,
    seen: Seen,
    wire: Vec<(Instant, Wire)>,
}

/// One service activity: when it started and ended, and its input.
#[derive(Debug)]
struct Activity {
    started: Instant,
    audio: usize,
    /// Speech blocks among `audio`: every one the person spoke must arrive.
    speech: usize,
    ended: Option<Instant>,
}

impl Fast {
    fn events(&self, kind: &str) -> Vec<&Value> {
        self.trace
            .iter()
            .filter(|row| row["kind"] == kind)
            .collect()
    }

    fn activities(&self) -> Vec<Activity> {
        let mut all: Vec<Activity> = Vec::new();
        for (at, wire) in &self.wire {
            match wire {
                Wire::Start => all.push(Activity {
                    started: *at,
                    audio: 0,
                    speech: 0,
                    ended: None,
                }),
                Wire::Audio { loud } => {
                    let activity = all.last_mut().expect("audio before a start");
                    activity.audio += 1;
                    activity.speech += usize::from(*loud);
                }
                Wire::End => all.last_mut().expect("end before a start").ended = Some(*at),
                _ => {}
            }
        }
        all
    }

    fn first(&self, wanted: &Wire) -> Option<Instant> {
        self.wire
            .iter()
            .find(|(_, wire)| wire == wanted)
            .map(|(at, _)| *at)
    }

    /// Turns that reached the speaker, in order, without repeats.
    fn spoken(&self) -> Vec<u64> {
        let mut turns: Vec<u64> = Vec::new();
        for (turn, _) in &self.seen.chunks {
            if turns.last() != Some(turn) {
                turns.push(*turn);
            }
        }
        turns
    }

    fn first_chunk(&self) -> Option<Instant> {
        self.seen.chunks.first().map(|(_, at)| *at)
    }

    fn admitted_turns(&self) -> Vec<u64> {
        self.events("input_admitted")
            .iter()
            .map(|row| row["turn"].as_u64().expect("admitted turn"))
            .collect()
    }
}

/// Removes the early-end hold for one negative control and restores it
/// however the test ends.
struct HoldRemoved;

impl HoldRemoved {
    fn new() -> Self {
        EARLY_HOLD_REMOVED.store(true, Ordering::SeqCst);
        Self
    }
}

impl Drop for HoldRemoved {
    fn drop(&mut self) {
        EARLY_HOLD_REMOVED.store(false, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy)]
struct Shape {
    turn_taking: TurnTaking,
    /// The service's lifetime instead of a finite run.
    serve: bool,
    /// Muted from this capture block.
    mute_at: Option<u64>,
    /// SIGTERM requested at this capture block.
    stop_at: Option<u64>,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            turn_taking: TurnTaking::V1Baseline,
            serve: false,
            mute_at: None,
            stop_at: None,
        }
    }
}

fn run(seconds: u64, script: Script, endpoint: EndpointRig, answering: Answering) -> Fast {
    run_shaped(seconds, script, endpoint, answering, Shape::default())
}

fn run_shaped(
    seconds: u64,
    script: Script,
    endpoint: EndpointRig,
    answering: Answering,
    shape: Shape,
) -> Fast {
    let dir = SessionDirectory::create().unwrap();
    let (capture, capture_peer_channels) = Worker::sleeping_fixture(&dir.path, "capture");
    let (speaker, speaker_peer_channels) = Worker::sleeping_fixture(&dir.path, "speaker");
    let (mut provider, provider_child) = Worker::sleeping_fixture(&dir.path, "provider");
    let (privacy, privacy_peer_channels) = Worker::sleeping_fixture(&dir.path, "privacy");
    let mut owner = Controller::new(new_boot().unwrap(), now());
    let boot = owner.snapshot(now()).unwrap().boot();
    let stop = Arc::new(AtomicBool::new(false));
    let log: Log = Arc::default();
    // The worker and the service share a runtime on a thread of their own,
    // as the worker process has one; this thread is the coordinator's.
    let (done, worker) = std::sync::mpsc::channel();
    let provider_thread = thread::spawn({
        let (log, stop) = (log.clone(), stop.clone());
        move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let result = runtime.block_on(async move {
                let config = Continuation::KeptStart
                    .apply(
                        SessionConfig::new(
                            lamp_gemini::GOOGLE_ENDPOINT,
                            Credential::api_key("test-only").unwrap(),
                        )
                        .unwrap()
                        .resumption(Resumption::Retain)
                        .barrier_policy(BarrierPolicy::Require),
                    )
                    .unwrap();
                let (connect, dials) = connector(config);
                let supervisor = Supervisor::new(
                    connect,
                    RecoveryPolicy {
                        first_backoff: Duration::from_millis(20),
                        max_backoff: Duration::from_millis(40),
                        ..RecoveryPolicy::default()
                    },
                )
                .unwrap();
                let service = tokio::spawn(service(dials, answering, log, stop));
                let result = serve_coordinated_test(provider_child, boot, supervisor).await;
                let _ = service.await;
                result
            });
            let _ = done.send(result);
        }
    });
    // The worker says Ready before anything else, as at production startup.
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(event) = provider.channels.control.receive::<WorkerEvent>().unwrap() {
            assert!(matches!(event, WorkerEvent::Ready), "{event:?}");
            break;
        }
        assert!(Instant::now() < deadline, "provider worker never ready");
        thread::sleep(Duration::from_millis(1));
    }
    let seen = Arc::new(Mutex::new(Seen::default()));
    let signal = Arc::new(AtomicUsize::new(0));
    let peers = [
        thread::spawn({
            let (stop, seen) = (stop.clone(), seen.clone());
            move || capture_peer(capture_peer_channels, script, seen, stop)
        }),
        thread::spawn({
            let (stop, seen) = (stop.clone(), seen.clone());
            move || privacy_peer(privacy_peer_channels, seen, shape.mute_at, stop)
        }),
        thread::spawn({
            let (stop, seen) = (stop.clone(), seen.clone());
            move || speaker_peer(speaker_peer_channels, seen, stop)
        }),
        thread::spawn({
            let (stop, seen, signal) = (stop.clone(), seen.clone(), signal.clone());
            move || {
                let Some(block) = shape.stop_at else {
                    return;
                };
                while !stop.load(Ordering::SeqCst) {
                    let due = {
                        let seen = seen.lock().unwrap();
                        seen.capture_started.is_some() && Instant::now() >= seen.block(block)
                    };
                    if due {
                        signal.store(15, Ordering::SeqCst);
                        return;
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            }
        }),
    ];
    let mut rig = Rig {
        provider,
        capture,
        speaker,
        privacy,
        ring: None.into(),
        camera: None,
        motor: None,
        ring_channel_ceiling: None,
        diagnostic_directory: None,
        noise_suppression: NoiseSuppression::On,
        turn_taking: shape.turn_taking,
        playout_start: PlayoutStart::default(),
        endpoint: Some(endpoint),
    };
    let (lifetime, mut trace) = if shape.serve {
        (Lifetime::Serve, SessionState::default())
    } else {
        (Lifetime::Finite(seconds), SessionState::finite())
    };
    let mut reply = None;
    let result = conversation(
        &mut rig,
        &mut owner,
        lifetime,
        &mut trace,
        StartupEvents::default(),
        &mut reply,
        &mut None,
        &Termination::shared(signal),
        &FailureNotices::none(),
    );
    // The provider worker ends on its Stop; sleeping fixtures need forced
    // reaping, which is not under test here.
    let _ = rig.shutdown(&mut trace);
    stop.store(true, Ordering::SeqCst);
    for peer in peers {
        peer.join().unwrap();
    }
    let worker = worker
        .recv_timeout(Duration::from_secs(5))
        .expect("provider worker ended on its Stop");
    provider_thread.join().unwrap();
    worker.expect("provider worker result");
    let seen = std::mem::take(&mut *seen.lock().unwrap());
    let wire = std::mem::take(&mut *log.lock().unwrap());
    Fast {
        result,
        trace: trace.to_vec(),
        seen,
        wire,
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

impl Fast {
    fn bases(&self) -> Vec<(String, Option<String>, u64)> {
        self.events("local_endpoint")
            .iter()
            .map(|row| {
                let endpoint = &row["endpoint"];
                (
                    endpoint["basis"].as_str().unwrap().to_owned(),
                    endpoint["fallback"].as_str().map(str::to_owned),
                    endpoint["silence_ms"].as_u64().unwrap(),
                )
            })
            .collect()
    }

    /// What became of each early end, in order: `confirmed` or `continued`.
    fn outcomes(&self) -> Vec<String> {
        self.events("endpoint_provisional")
            .iter()
            .map(|row| row["event"]["kind"].as_str().unwrap().to_owned())
            .collect()
    }

    fn finished(&self, turn: u64) -> String {
        let rows = self.events("turn_finished");
        let row = rows
            .iter()
            .find(|row| row["turn"] == turn)
            .unwrap_or_else(|| panic!("turn {turn} never finished: {rows:?}"));
        row["outcome"].as_str().unwrap().to_owned()
    }

    fn held_ended(&self) -> Vec<u64> {
        self.events("endpoint_hold_ended")
            .iter()
            .map(|row| row["turn"].as_u64().unwrap())
            .collect()
    }

    /// The service's view: speech blocks per activity, and whether each ended.
    fn speech(&self) -> Vec<(usize, bool)> {
        self.activities()
            .iter()
            .map(|activity| (activity.speech, activity.ended.is_some()))
            .collect()
    }

    fn early_policy(&self) -> &Value {
        &self.events("endpoint_policy")[0]["confirmation"]["early"]
    }
}

fn early_rows(n: usize) -> Vec<(String, Option<String>, u64)> {
    vec![("model_early".to_owned(), None, 500); n]
}

/// A thought, a 600 ms breath, then more of it.
fn breath(block: u64) -> f32 {
    if (50..200).contains(&block) || (260..360).contains(&block) {
        SPEECH
    } else {
        QUIET
    }
}

#[test]
fn an_unambiguous_finish_ends_input_at_half_a_second_and_its_answer_waits_for_confirmation() {
    let _one = one_session();
    let fast = run(5, one_question, early(0.95), Answering::after(300));
    fast.result.as_ref().unwrap();
    assert_eq!(fast.early_policy()["end_blocks"], 50);
    assert_eq!(fast.bases(), early_rows(1));
    let seen = &fast.seen;
    let activities = fast.activities();
    assert_eq!(fast.speech(), [(150, true)], "{activities:?}");
    // Speech ends after block 199: the service has the end of input after
    // 500 ms of quiet, not after the 1.2 s window.
    let ended = activities[0].ended.unwrap();
    assert!(
        ended >= seen.block(248) && ended < seen.block(262),
        "ended {:?} after capture",
        ended - seen.capture_started.unwrap()
    );
    // The answer is ready before confirmation, and nothing of it is heard
    // until the 1.2 s of quiet that confirms the pause.
    let answered = fast.first(&Wire::Answer { end: 1 }).unwrap();
    assert!(answered < seen.block(318));
    let first = fast.first_chunk().expect("the answer was never heard");
    assert!(first >= seen.block(318));
    assert!(first - ended >= Duration::from_millis(600));
    assert_eq!(fast.outcomes(), ["confirmed"]);
    let confirmed = &fast.events("endpoint_provisional")[0]["event"];
    assert_eq!(
        (
            &confirmed["quiet_ms"],
            &confirmed["after_ms"],
            &confirmed["bounded"]
        ),
        (&json!(1_200), &json!(700), &json!(false))
    );
    let held = fast.events("endpoint_confirmed");
    assert_eq!(held.len(), 1);
    assert!(held[0]["held_us"].as_u64().unwrap() >= 650_000, "{held:?}");
    assert_eq!(fast.spoken(), fast.admitted_turns());
    assert!(fast.held_ended().is_empty());
    assert!(fast.events("provider_input_discarded").is_empty());
}

/// Negative control: without the hold the same session is heard before
/// confirmation, so the bound asserted above can fail.
#[test]
fn without_the_hold_the_same_answer_is_heard_before_confirmation() {
    let _one = one_session();
    let removed = HoldRemoved::new();
    let fast = run(5, one_question, early(0.95), Answering::after(300));
    drop(removed);
    fast.result.as_ref().unwrap();
    assert_eq!(fast.bases(), early_rows(1));
    let first = fast.first_chunk().expect("the answer was never heard");
    assert!(first < fast.seen.block(310));
    assert!(fast.events("endpoint_confirmed").is_empty());
}

#[test]
fn a_breath_retires_the_unspoken_answer_and_the_whole_continuation_reaches_the_service() {
    let _one = one_session();
    let fast = run(6, breath, early(0.95), Answering::after(300));
    fast.result.as_ref().unwrap();
    assert_eq!(fast.bases(), early_rows(2));
    assert_eq!(fast.outcomes(), ["continued", "confirmed"]);
    let turns = fast.admitted_turns();
    assert_eq!(turns.len(), 2, "{turns:?}");
    // Kept until the first answer began, then that answer was interrupted;
    // every word of the continuation arrived in the second activity.
    assert_eq!(fast.speech(), [(150, true), (100, true)]);
    assert!(fast.first(&Wire::Interrupted).is_some(), "{:?}", fast.wire);
    assert_eq!(fast.finished(turns[0]), "user_interrupted");
    assert_eq!(fast.held_ended(), [turns[0]]);
    // Only the continued thought is answered aloud, after its own
    // confirmation (speech ends after block 359).
    assert_eq!(fast.spoken(), [turns[1]]);
    assert!(fast.first_chunk().unwrap() >= fast.seen.block(478));
    assert!(fast.events("provider_input_discarded").is_empty());
}

#[test]
fn repeated_continuations_keep_every_part_and_only_the_last_answer_is_heard() {
    let _one = one_session();
    fn three_parts(block: u64) -> f32 {
        if (50..150).contains(&block) || (210..280).contains(&block) || (340..420).contains(&block)
        {
            SPEECH
        } else {
            QUIET
        }
    }
    let fast = run(7, three_parts, early(0.95), Answering::after(300));
    fast.result.as_ref().unwrap();
    assert_eq!(fast.bases(), early_rows(3));
    assert_eq!(fast.outcomes(), ["continued", "continued", "confirmed"]);
    let turns = fast.admitted_turns();
    assert_eq!(turns.len(), 3, "{turns:?}");
    assert_eq!(fast.speech(), [(100, true), (70, true), (80, true)]);
    assert_eq!(fast.finished(turns[0]), "user_interrupted");
    assert_eq!(fast.finished(turns[1]), "user_interrupted");
    assert_eq!(fast.held_ended(), [turns[0], turns[1]]);
    assert_eq!(fast.spoken(), [turns[2]]);
    assert!(fast.first_chunk().unwrap() >= fast.seen.block(538));
    assert!(fast.events("provider_input_discarded").is_empty());
}

#[test]
fn a_new_topic_just_before_confirmation_drops_a_fully_received_answer() {
    let _one = one_session();
    // 1.1 s after the first question, 100 ms before it would be confirmed,
    // a different question begins.
    fn new_topic(block: u64) -> f32 {
        if (50..200).contains(&block) || (310..450).contains(&block) {
            SPEECH
        } else {
            QUIET
        }
    }
    let fast = run(7, new_topic, early(0.95), Answering::after(300));
    fast.result.as_ref().unwrap();
    assert_eq!(fast.bases(), early_rows(2));
    assert_eq!(fast.outcomes(), ["continued", "confirmed"]);
    let turns = fast.admitted_turns();
    // The whole first answer had arrived and settled, so the new start
    // was not kept and nothing needed interrupting.
    let settled = fast.first(&Wire::Idle).unwrap();
    assert!(settled < fast.seen.block(305));
    assert!(fast.first(&Wire::Interrupted).is_none(), "{:?}", fast.wire);
    assert_eq!(fast.speech(), [(150, true), (140, true)]);
    assert_eq!(fast.finished(turns[0]), "user_interrupted");
    assert_eq!(fast.held_ended(), [turns[0]]);
    assert_eq!(fast.spoken(), [turns[1]]);
    assert!(fast.first_chunk().unwrap() >= fast.seen.block(568));
}

/// Main's A733 measurement of the pinned worker: about 370 ms of warm
/// inference per verdict. A 400 ms verdict, submitted at 200 ms of pause,
/// arrives at 600 ms: the early end follows it, inside the unchanged 500 ms
/// result timeout, and still overlaps 600 ms of the confirmation.
#[test]
fn an_a733_verdict_cost_ends_input_at_its_arrival_and_still_overlaps_the_confirmation() {
    let _one = one_session();
    let slow = delayed(0.95, Duration::from_millis(400)).with_policy(early_policy());
    let fast = run(5, one_question, slow, Answering::after(300));
    fast.result.as_ref().unwrap();
    let bases = fast.bases();
    assert_eq!(bases.len(), 1, "{bases:?}");
    assert_eq!(bases[0].0, "model_early");
    assert!((590..=640).contains(&bases[0].2), "{bases:?}");
    assert_eq!(fast.outcomes(), ["confirmed"]);
    let confirmed = &fast.events("endpoint_provisional")[0]["event"];
    assert_eq!(confirmed["quiet_ms"], 1_200);
    let ended = fast.activities()[0].ended.unwrap();
    assert!(ended >= fast.seen.block(255) && ended < fast.seen.block(275));
    let first = fast.first_chunk().expect("the answer was never heard");
    assert!(first >= fast.seen.block(318));
    assert!(first - ended >= Duration::from_millis(500));
    assert_eq!(fast.spoken(), fast.admitted_turns());
}

#[test]
fn a_late_answer_plays_after_confirmation() {
    let _one = one_session();
    let fast = run(6, one_question, early(0.95), Answering::after(1_500));
    fast.result.as_ref().unwrap();
    assert_eq!(fast.bases(), early_rows(1));
    assert_eq!(fast.outcomes(), ["confirmed"]);
    let answered = fast.first(&Wire::Answer { end: 1 }).unwrap();
    assert!(answered >= fast.seen.block(390));
    let kinds: Vec<&str> = fast
        .trace
        .iter()
        .map(|row| row["kind"].as_str().unwrap_or(""))
        .collect();
    let confirmed = kinds.iter().position(|kind| *kind == "endpoint_confirmed");
    let first_audio = kinds
        .iter()
        .position(|kind| *kind == "provider_first_audio");
    assert!(confirmed.unwrap() < first_audio.unwrap(), "{kinds:?}");
    assert!(fast.first_chunk().unwrap() > answered);
    assert_eq!(fast.spoken(), fast.admitted_turns());
}

#[test]
fn an_old_answer_later_than_the_continuation_is_never_heard_and_the_kept_words_arrive() {
    let _one = one_session();
    // The first answer begins 1.5 s after its end of input: the continuation
    // is kept for about 1.4 s, inside the kept start's own bound.
    let fast = run(7, breath, early(0.95), Answering::after(1_500));
    fast.result.as_ref().unwrap();
    assert_eq!(fast.bases(), early_rows(2));
    assert_eq!(fast.outcomes(), ["continued", "confirmed"]);
    let turns = fast.admitted_turns();
    let activities = fast.activities();
    assert_eq!(fast.speech(), [(150, true), (100, true)], "{activities:?}");
    // The second start waited for the first answer to begin.
    let answered = fast.first(&Wire::Answer { end: 1 }).unwrap();
    assert!(activities[1].started >= answered);
    assert!(fast.first(&Wire::Interrupted).is_some());
    assert_eq!(fast.finished(turns[0]), "user_interrupted");
    assert_eq!(fast.spoken(), [turns[1]]);
    assert!(fast.events("provider_input_discarded").is_empty());
    assert!(fast.events("provider_turn_failed").is_empty());
}

/// Without a usable verdict nothing ends early: the end of input waits for
/// the unchanged 1.2 s window and nothing is held.
fn assert_unchanged_window(fast: &Fast, fallback: Option<&str>) {
    fast.result.as_ref().unwrap();
    let bases = fast.bases();
    assert_eq!(bases.len(), 1, "{bases:?}");
    assert_eq!(
        (bases[0].0.as_str(), bases[0].2),
        ("silence_fallback", 1_200),
        "{bases:?}"
    );
    if let Some(fallback) = fallback {
        assert_eq!(bases[0].1.as_deref(), Some(fallback));
    }
    assert!(fast.outcomes().is_empty());
    assert!(fast.events("endpoint_confirmed").is_empty());
    let activities = fast.activities();
    let ended = activities[0].ended.unwrap();
    assert!(ended >= fast.seen.block(318), "{activities:?}");
    assert!(fast.first_chunk().unwrap() > ended);
    assert_eq!(fast.spoken(), fast.admitted_turns());
}

fn early_policy() -> ConfirmPolicy {
    ConfirmPolicy {
        early: Some(EarlyEnd::CANDIDATE),
        ..ConfirmPolicy::default()
    }
}

#[test]
fn a_classifier_timeout_keeps_the_unchanged_window() {
    let _one = one_session();
    let late = delayed(0.99, Duration::from_millis(700)).with_policy(early_policy());
    let fast = run(5, one_question, late, Answering::after(300));
    assert_unchanged_window(&fast, Some("timeout"));
}

#[test]
fn a_missing_model_keeps_the_unchanged_window() {
    let _one = one_session();
    let missing = Response::Error {
        request: 0,
        reason: "model /lamp/smart-turn-v3.2-gpu.onnx: No such file or directory".into(),
    };
    let endpoint =
        scripted_process(&[missing], "exit 2", Limits::default()).with_policy(early_policy());
    let fast = run(5, one_question, endpoint, Answering::after(300));
    assert_unchanged_window(&fast, Some("failed"));
}

#[test]
fn a_worker_crash_during_its_request_keeps_the_unchanged_window() {
    let _one = one_session();
    // Ready, then exits as soon as the first request arrives.
    let endpoint = scripted_process(
        &[ready()],
        "head -c 1 >/dev/null; exit 3",
        Limits::default(),
    )
    .with_policy(early_policy());
    let fast = run(5, one_question, endpoint, Answering::after(300));
    assert_unchanged_window(&fast, None);
}

#[test]
fn a_provider_failure_while_held_is_never_heard_and_the_next_question_is_answered() {
    let _one = one_session();
    fn two(block: u64) -> f32 {
        if (50..200).contains(&block) || (450..600).contains(&block) {
            SPEECH
        } else {
            QUIET
        }
    }
    let answering = Answering {
        fail_on_end: Some(1),
        ..Answering::after(300)
    };
    let fast = run(8, two, early(0.95), answering);
    fast.result.as_ref().unwrap();
    let turns = fast.admitted_turns();
    assert_eq!(turns.len(), 2, "{turns:?}");
    assert!(fast.wire.iter().any(|(_, wire)| *wire == Wire::Closed));
    assert!(
        fast.wire
            .iter()
            .any(|(_, wire)| *wire == Wire::Dial { resumed: true })
    );
    // The failure arrived while the answer was held: the turn stayed open
    // until confirmation, as any notice about it would, and then ended.
    // The outage left nothing that swallows the next question.
    assert_eq!(fast.finished(turns[0]), "provider_failed");
    let confirmed: Vec<u64> = fast
        .events("endpoint_confirmed")
        .iter()
        .map(|row| row["turn"].as_u64().unwrap())
        .collect();
    assert_eq!(confirmed, turns);
    assert!(fast.events("input_not_submitted").is_empty());
    assert!(fast.events("input_discarded_endpoint").is_empty());
    assert_eq!(fast.speech(), [(150, true), (150, true)]);
    assert_eq!(fast.spoken(), [turns[1]]);
    assert!(fast.first_chunk().unwrap() >= fast.seen.block(718));
}

#[test]
fn stop_while_the_answer_is_held_speaks_nothing() {
    let _one = one_session();
    // The whole answer has arrived by block 300 and is confirmed at 319.
    let shape = Shape {
        stop_at: Some(305),
        ..Shape::default()
    };
    let fast = run_shaped(5, one_question, early(0.95), Answering::after(300), shape);
    let error = fast.result.as_ref().unwrap_err();
    assert!(error.to_string().contains("SIGTERM"), "{error}");
    // The answer had begun arriving, and was held.
    assert!(fast.first(&Wire::Answer { end: 1 }).is_some());
    assert_eq!(fast.bases(), early_rows(1));
    assert!(fast.events("endpoint_confirmed").is_empty());
    assert!(fast.seen.chunks.is_empty());
}

#[test]
fn mute_while_the_answer_is_held_speaks_nothing() {
    use crate::service_privacy::PhysicalMute;
    let _one = one_session();
    // As the service runs: only a mute or a stop ends it. The stop at 6 s
    // only bounds a failing test.
    let shape = Shape {
        serve: true,
        mute_at: Some(305),
        stop_at: Some(600),
        ..Shape::default()
    };
    let fast = run_shaped(5, one_question, early(0.95), Answering::after(300), shape);
    let error = fast.result.as_ref().unwrap_err();
    assert!(error.downcast_ref::<PhysicalMute>().is_some(), "{error}");
    assert!(fast.first(&Wire::Answer { end: 1 }).is_some());
    assert_eq!(fast.bases(), early_rows(1));
    assert!(fast.events("endpoint_confirmed").is_empty());
    assert!(fast.seen.chunks.is_empty());
}

/// Answers every request with the token of the first one it was given.
struct Stale {
    first: Option<Token>,
    pending: Option<(Token, Instant)>,
    requests: u64,
}

impl Classifier for Stale {
    fn submit(&mut self, token: Token, _pcm: &[i16]) -> Submit {
        if self.pending.is_some() {
            return Submit::Busy;
        }
        let first = *self.first.get_or_insert(token);
        self.requests += 1;
        self.pending = Some((first, Instant::now()));
        Submit::Accepted
    }
    fn poll(&mut self) -> Option<Outcome> {
        let (token, at) = self.pending?;
        if at.elapsed() < Duration::from_millis(60) {
            return None;
        }
        self.pending = None;
        Some(Outcome {
            token,
            worker: 1,
            request: self.requests,
            result: Ok(Verdict {
                probability: 0.99,
                round_trip_us: 60_000,
                ..Verdict::default()
            }),
        })
    }
    fn availability(&self) -> Availability {
        Availability::Ready
    }
    fn cancel(&mut self) {}
}

/// Negative control: a confident verdict about an earlier pause never ends
/// a later one early.
#[test]
fn a_stale_verdict_never_ends_a_later_pause_early() {
    let _one = one_session();
    // A 300 ms pause (judged complete, then obsolete), more speech, and
    // the final pause, whose only verdict names the first pause.
    fn resumed(block: u64) -> f32 {
        if (50..150).contains(&block) || (180..260).contains(&block) {
            SPEECH
        } else {
            QUIET
        }
    }
    let stale = EndpointRig::double(Box::new(Stale {
        first: None,
        pending: None,
        requests: 0,
    }))
    .with_policy(early_policy());
    let fast = run(5, resumed, stale, Answering::after(300));
    fast.result.as_ref().unwrap();
    assert_eq!(
        fast.bases(),
        [("silence_fallback".into(), Some("timeout".into()), 1_200)]
    );
    assert!(fast.outcomes().is_empty());
    let activities = fast.activities();
    assert_eq!(fast.speech(), [(180, true)], "{activities:?}");
    assert!(activities[0].ended.unwrap() >= fast.seen.block(378));
}

#[test]
fn duplex_early_end_holds_its_answer_until_confirmation() {
    let _one = one_session();
    let shape = Shape {
        turn_taking: TurnTaking::Duplex,
        ..Shape::default()
    };
    let fast = run_shaped(5, one_question, early(0.95), Answering::after(300), shape);
    fast.result.as_ref().unwrap();
    assert_eq!(fast.bases(), early_rows(1));
    assert_eq!(fast.outcomes(), ["confirmed"]);
    let ended = fast.activities()[0].ended.unwrap();
    assert!(ended < fast.seen.block(262));
    assert!(fast.first_chunk().unwrap() >= fast.seen.block(318));
    assert_eq!(fast.spoken(), fast.admitted_turns());
}

#[test]
fn the_policy_record_names_the_early_end_only_when_chosen() {
    for (policy, early) in [(ConfirmPolicy::default(), false), (early_policy(), true)] {
        let mut rig = delayed(0.95, Duration::from_millis(60)).with_policy(policy);
        let (detector, event) = EndpointRig::attach(
            Some(&mut rig),
            TurnTaking::V1Baseline,
            crate::activity::TurnDetector::v1_baseline(),
        )
        .unwrap();
        assert_eq!(detector.confirmation_policy(), Some(policy));
        // The pilot's record keeps exactly its fields.
        assert_eq!(
            event["confirmation"].get("early").is_some(),
            early,
            "{event}"
        );
        if early {
            assert_eq!(
                event["confirmation"]["early"],
                json!({"probability":0.9_f32,"end_blocks":50,"confirm_blocks":120,"limit_blocks":300})
            );
        }
    }
}
