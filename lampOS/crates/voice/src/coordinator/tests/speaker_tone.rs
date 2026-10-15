//! The self-test's tone through the production dispatch and receipt
//! handlers, with owned local IPC; no audio device.
use super::*;
use crate::coordinator::speaker_tone::{SpeakerTone, begin_tone, commit_tone_turn, verdict};
use crate::speaker_check::{self, Heard};
use lamp_audio::ownership::PlaybackCursor;
use lamp_bus::Bus;
use lamp_msgs::{SpeakerCheck, SpeakerCheckRequest};

struct Receipts {
    events: VecDeque<WorkerEvent>,
}
impl SpeakerControlIo for Receipts {
    fn next_speaker_event(&mut self) -> io::Result<Option<WorkerEvent>> {
        Ok(self.events.pop_front())
    }
    fn publish_speaker_snapshot(&mut self, _: Snapshot) -> io::Result<()> {
        Ok(())
    }
}

fn accepted(chunk: &SpeakerChunk, offset: u64) -> WorkerEvent {
    let at = monotonic_us();
    WorkerEvent::Playback {
        owner: chunk.permit.owner(),
        turn: chunk.permit.owner().turn(),
        sequence: chunk.chunk_sequence,
        first_sample: PlaybackCursor {
            epoch: 1,
            frame: offset,
        },
        end_sample: PlaybackCursor {
            epoch: 1,
            frame: offset + 240,
        },
        queue_observed_at_us: at,
        queued_frames: 0,
        accepted_frames: 240,
        observed_at_us: at,
    }
}

fn listening_owner() -> Controller {
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
    owner
}

#[test]
fn the_tone_plays_whole_through_the_speaker_path_and_its_turn_settles_with_no_provider() {
    let directory = SessionDirectory::create().unwrap();
    let (mut parent, mut speaker) = pair(&directory, "speaker");
    let mut owner = listening_owner();
    let mut reply = None;
    let mut trace = SessionState::default();
    let (mut snapshot, turn) = commit_tone_turn(
        &mut owner,
        &mut reply,
        0,
        PlayoutStart::default(),
        &mut trace,
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(
        reply.as_ref().unwrap().notice.origin(),
        SpeechOrigin::SelfTest
    );
    let tone = speaker_check::tone();
    let mut pending = None;
    let mut mask = PlaybackMask::new(TurnTaking::VadOnly);
    let mut sequence = 0;
    let mut last_publish = 0;
    let mut played = Vec::new();
    let mut last = None;
    for _ in 0..tone.len() / 240 {
        dispatch_speaker_chunk(
            &mut owner,
            &mut reply,
            &mut parent.data,
            &mut pending,
            &mut sequence,
            &mut snapshot,
            &mut mask,
        )
        .unwrap();
        let chunk = speaker.data.receive::<SpeakerChunk>().unwrap().unwrap();
        assert_eq!(chunk.permit.owner(), turn);
        trace.receipts.dispatched(turn, sequence).unwrap();
        let mut receipts = Receipts {
            events: vec![accepted(&chunk, played.len() as u64)].into(),
        };
        played.extend_from_slice(&chunk.samples);
        service_speaker_control(
            &mut receipts,
            &mut owner,
            &mut reply,
            &mut pending,
            &mut mask,
            &mut snapshot,
            &mut last_publish,
            &mut 16,
            &mut trace,
        )
        .unwrap();
        last = Some(chunk);
    }
    assert_eq!(played, tone, "the tone, whole and in order");
    let last = last.unwrap();
    assert!(last.end_of_speech);
    assert!(
        !reply.as_ref().unwrap().cloud.started,
        "no invented provider Started"
    );
    assert!(
        reply
            .as_ref()
            .unwrap()
            .notice
            .first_accepted_at_us()
            .is_some()
    );
    let mut retired = Receipts {
        events: vec![WorkerEvent::SpeechRetired {
            owner: turn,
            sequence: last.chunk_sequence,
            final_sample: PlaybackCursor {
                epoch: 1,
                frame: played.len() as u64,
            },
            observed_at_us: monotonic_us(),
        }]
        .into(),
    };
    service_speaker_control(
        &mut retired,
        &mut owner,
        &mut reply,
        &mut pending,
        &mut mask,
        &mut snapshot,
        &mut last_publish,
        &mut 16,
        &mut trace,
    )
    .unwrap();
    let current = reply.as_ref().unwrap();
    assert!(current.settled(&pending), "nothing is owed to a provider");
    assert_eq!(current.delivery_outcome(), "no_audio_answer");
    owner.complete_turn(now(), current.owner).unwrap();
    assert!(owner.snapshot(now()).unwrap().owner().is_none());
}

fn bus(name: &str) -> (Bus, std::path::PathBuf) {
    let root = PathBuf::from(format!("/tmp/lv-tone-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    (Bus::new(&root).unwrap(), root)
}

fn answer(bus: &Bus) -> SpeakerCheck {
    bus.latest(SpeakerCheck::TOPIC)
        .unwrap()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn a_request_not_for_the_factory_or_on_demand_is_answered_and_never_played() {
    let (bus, root) = bus("occasion");
    let mut tone = SpeakerTone::on(&bus).unwrap();
    let mut asks = bus
        .publisher_declared(
            SpeakerCheckRequest::TOPIC,
            &SpeakerCheckRequest::declaration(),
            "selftest",
        )
        .unwrap();
    asks.publish_json(
        1,
        &SpeakerCheckRequest {
            id: "boot".into(),
            occasion: "first_boot".into(),
        },
    )
    .unwrap();
    assert_eq!(tone.next_request(monotonic_us()), None);
    let refused = answer(&bus);
    assert_eq!(
        (refused.id.as_str(), refused.verdict.as_str()),
        ("boot", "not_checked")
    );
    asks.publish_json(
        2,
        &SpeakerCheckRequest {
            id: "asked".into(),
            occasion: "on_demand".into(),
        },
    )
    .unwrap();
    assert_eq!(tone.next_request(monotonic_us()).as_deref(), Some("asked"));
    // A turn that ends before its first chunk is said so.
    let mut owner = listening_owner();
    let turn = owner.admit(now(), AdmittedInput::NewTurn).unwrap();
    tone.began("asked".into(), turn);
    tone.tick(None, monotonic_us());
    let ended = answer(&bus);
    assert_eq!(ended.id, "asked");
    assert!(
        ended.reason.contains("before the speaker took"),
        "{ended:?}"
    );
    drop(asks);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_verdict_passes_a_cancelled_tone_and_fails_an_unheard_or_uncancelled_one() {
    let heard = Heard::Yes {
        rise_db: 24.0,
        delay_ms: 70,
        cancelled_db: 18.0,
    };
    assert_eq!(verdict(&heard).0, "pass");
    let echoing = Heard::Yes {
        rise_db: 24.0,
        delay_ms: 70,
        cancelled_db: 2.0,
    };
    assert!(verdict(&echoing).1.contains("canceller"));
    let silent = Heard::No {
        baseline_db: -60.0,
        loudest_db: -59.0,
    };
    assert_eq!(verdict(&silent).0, "fail");
    assert_eq!(verdict(&Heard::NoBaseline).0, "not_checked");
}

#[test]
fn the_ring_prepares_no_frame_for_the_tone_even_in_the_pass_that_admits_it() {
    // Without telling the ring first, the pass that admits a turn can
    // prepare a Waiting frame for it: the control.
    let mut owner = listening_owner();
    let mut reply = None;
    let mut ring = crate::choreography::RingChoreographer::new(40).unwrap();
    commit_tone_turn(
        &mut owner,
        &mut reply,
        0,
        PlayoutStart::default(),
        &mut SessionState::default(),
        |_| Ok(()),
    )
    .unwrap();
    let frame = ring.prepare(&mut owner, now()).unwrap();
    assert!(
        frame.is_some(),
        "the control shows the frame the tone must not have"
    );
    // Begun as the loop begins it: nothing, in that pass and after.
    let mut owner = listening_owner();
    let mut reply = None;
    let mut ring = crate::choreography::RingChoreographer::new(40).unwrap();
    begin_tone(
        &mut owner,
        &mut reply,
        Some(&mut ring),
        PlayoutStart::default(),
        &mut SessionState::default(),
        |_| Ok(()),
    )
    .unwrap();
    assert!(owner.snapshot(now()).unwrap().owner().is_some());
    assert!(ring.prepare(&mut owner, now()).unwrap().is_none());
    assert!(ring.prepare(&mut owner, now()).unwrap().is_none());
}

/// A tone whose answers cannot be written: the answer topic's kept value
/// is a directory, so every publication fails.
fn broken_answers(name: &str) -> (SpeakerTone, std::path::PathBuf) {
    let (bus, root) = bus(name);
    let tone = SpeakerTone::on(&bus).unwrap();
    let kept = root.join(SpeakerCheck::TOPIC).join("latest");
    let _ = std::fs::remove_file(&kept);
    std::fs::create_dir_all(kept.join("in-the-way")).unwrap();
    (tone, root)
}

#[test]
fn an_answer_that_cannot_be_written_switches_the_check_off_and_the_voice_goes_on() {
    // On a refusal.
    let (mut tone, root) = broken_answers("broken-refusal");
    tone.refuse("r1", "the voice is busy with a turn", monotonic_us());
    assert!(tone.is_off());
    assert_eq!(tone.next_request(monotonic_us()), None);
    std::fs::remove_dir_all(root).unwrap();
    // On completion: a tone played, its answer due.
    let (mut tone, root) = broken_answers("broken-answer");
    let directory = SessionDirectory::create().unwrap();
    let (mut parent, _speaker) = pair(&directory, "speaker");
    let mut owner = listening_owner();
    let mut reply = None;
    let (mut snapshot, turn) = commit_tone_turn(
        &mut owner,
        &mut reply,
        0,
        PlayoutStart::default(),
        &mut SessionState::default(),
        |_| Ok(()),
    )
    .unwrap();
    tone.began("r2".into(), turn);
    dispatch_speaker_chunk(
        &mut owner,
        &mut reply,
        &mut parent.data,
        &mut None,
        &mut 0,
        &mut snapshot,
        &mut PlaybackMask::new(TurnTaking::VadOnly),
    )
    .unwrap();
    tone.tick(reply.as_ref(), monotonic_us());
    assert!(!tone.is_off(), "nothing to answer yet");
    tone.tick(None, monotonic_us() + speaker_check::LISTEN_US + 1);
    assert!(tone.is_off());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn requests_that_cannot_be_read_switch_the_check_off() {
    let (bus, root) = bus("unreadable");
    let mut tone = SpeakerTone::on(&bus).unwrap();
    assert_eq!(
        tone.take(
            Err(io::Error::other("the socket went away")),
            monotonic_us()
        ),
        None
    );
    assert!(tone.is_off());
    assert_eq!(tone.next_request(monotonic_us()), None);
    std::fs::remove_dir_all(root).unwrap();
}

/// Plays the tone's first `chunks` chunks through the real dispatch and
/// receipt handlers, each accepted `accept_after` after it was sent, and
/// retires its final sample if all were played and `retire`.
fn play(
    chunks: usize,
    accept_after: Duration,
    retire: bool,
) -> (Controller, Option<Reply>, TurnOwner, u64) {
    let directory = SessionDirectory::create().unwrap();
    let (mut parent, mut speaker) = pair(&directory, "speaker");
    let mut owner = listening_owner();
    let mut reply = None;
    let mut trace = SessionState::default();
    let (mut snapshot, turn) = commit_tone_turn(
        &mut owner,
        &mut reply,
        0,
        PlayoutStart::default(),
        &mut trace,
        |_| Ok(()),
    )
    .unwrap();
    let mut pending = None;
    let mut mask = PlaybackMask::new(TurnTaking::VadOnly);
    let (mut sequence, mut last_publish, mut played) = (0, 0, 0u64);
    let sent_us = monotonic_us();
    let mut last = None;
    for _ in 0..chunks {
        dispatch_speaker_chunk(
            &mut owner,
            &mut reply,
            &mut parent.data,
            &mut pending,
            &mut sequence,
            &mut snapshot,
            &mut mask,
        )
        .unwrap();
        let chunk = speaker.data.receive::<SpeakerChunk>().unwrap().unwrap();
        trace.receipts.dispatched(turn, sequence).unwrap();
        std::thread::sleep(accept_after);
        let mut receipts = Receipts {
            events: vec![accepted(&chunk, played)].into(),
        };
        service_speaker_control(
            &mut receipts,
            &mut owner,
            &mut reply,
            &mut pending,
            &mut mask,
            &mut snapshot,
            &mut last_publish,
            &mut 16,
            &mut trace,
        )
        .unwrap();
        played += 240;
        last = Some(chunk);
    }
    if retire && let Some(last) = last.filter(|chunk| chunk.end_of_speech) {
        let mut retired = Receipts {
            events: vec![WorkerEvent::SpeechRetired {
                owner: turn,
                sequence: last.chunk_sequence,
                final_sample: PlaybackCursor {
                    epoch: 1,
                    frame: played,
                },
                observed_at_us: monotonic_us(),
            }]
            .into(),
        };
        service_speaker_control(
            &mut retired,
            &mut owner,
            &mut reply,
            &mut pending,
            &mut mask,
            &mut snapshot,
            &mut last_publish,
            &mut 16,
            &mut trace,
        )
        .unwrap();
    }
    (owner, reply, turn, sent_us)
}

/// The tone's answer after its turn: watched while it played, then gone.
fn answered(name: &str, reply: Option<&Reply>, turn: TurnOwner) -> SpeakerCheck {
    let (bus, root) = bus(name);
    let mut tone = SpeakerTone::on(&bus).unwrap();
    tone.began("t".into(), turn);
    tone.tick(reply, monotonic_us());
    tone.tick(None, monotonic_us() + speaker_check::LISTEN_US + 1);
    let said = answer(&bus);
    std::fs::remove_dir_all(root).unwrap();
    said
}

#[test]
fn a_tone_cancelled_before_the_speaker_took_it_or_cut_short_is_not_judged() {
    let (_, reply, turn, _) = play(0, Duration::ZERO, false);
    let before = answered("cut-before", reply.as_ref(), turn);
    assert_eq!(before.verdict, "not_checked");
    assert!(
        before.reason.contains("before the speaker took"),
        "{before:?}"
    );
    let (_, reply, turn, _) = play(5, Duration::ZERO, false);
    let partial = answered("cut-partial", reply.as_ref(), turn);
    assert_eq!(partial.verdict, "not_checked");
    assert!(partial.reason.contains("cut short"), "{partial:?}");
    // All of it played but its final sample never retired: still cut short.
    let (_, reply, turn, _) = play(30, Duration::ZERO, false);
    assert!(
        answered("cut-unretired", reply.as_ref(), turn)
            .reason
            .contains("cut short")
    );
}

#[test]
fn a_tone_played_to_its_end_is_judged_on_its_evidence() {
    let (_, reply, turn, _) = play(30, Duration::ZERO, true);
    assert!(reply.as_ref().unwrap().notice.retired());
    // No microphone evidence was given here: judged, and too little of it.
    let judged = answered("whole", reply.as_ref(), turn);
    assert_eq!(judged.verdict, "not_checked");
    assert!(judged.reason.contains("evidence"), "{judged:?}");
}

#[test]
fn the_delay_starts_at_the_speakers_first_acceptance_not_at_dispatch() {
    let (_, reply, _, sent_us) = play(1, Duration::from_millis(30), false);
    let first = reply
        .as_ref()
        .unwrap()
        .notice
        .first_accepted_at_us()
        .unwrap();
    assert!(
        first >= sent_us + 30_000,
        "accepted {} us after sending",
        first - sent_us
    );
}

#[test]
fn a_tone_that_settles_in_the_pass_it_retires_is_judged_not_called_cut_short() {
    // As the loop runs it: the final sample retires and the turn settles
    // in one pass, and the tone is ticked only after the reply is gone.
    let (bus, root) = bus("settled");
    let mut tone = SpeakerTone::on(&bus).unwrap();
    let (_, reply, turn, _) = play(30, Duration::ZERO, true);
    tone.began("t".into(), turn);
    // The settle block, before the reply is let go.
    tone.finished(reply.as_ref().unwrap());
    tone.tick(None, monotonic_us() + speaker_check::LISTEN_US + 1);
    let judged = answer(&bus);
    assert!(judged.reason.contains("evidence"), "{judged:?}");
    // The control: let go without it, as a cancellation is, it is cut short.
    let mut tone = SpeakerTone::on(&bus).unwrap();
    tone.began("u".into(), turn);
    tone.tick(None, monotonic_us() + speaker_check::LISTEN_US + 1);
    assert!(answer(&bus).reason.contains("before the speaker took"));
    std::fs::remove_dir_all(root).unwrap();
}
