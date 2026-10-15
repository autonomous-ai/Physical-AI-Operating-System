//! The self-test's tone through the production dispatch and receipt
//! handlers, with owned local IPC; no audio device.
use super::*;
use crate::coordinator::speaker_tone::{SpeakerTone, commit_tone_turn, verdict};
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
    assert!(reply.as_ref().unwrap().notice.attempted_at_us().is_some());
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
    assert_eq!(tone.next_request(monotonic_us()).unwrap(), None);
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
    assert_eq!(
        tone.next_request(monotonic_us()).unwrap().as_deref(),
        Some("asked")
    );
    // A turn that ends before its first chunk is said so.
    let mut owner = listening_owner();
    let turn = owner.admit(now(), AdmittedInput::NewTurn).unwrap();
    tone.began("asked".into(), turn);
    tone.tick(None, monotonic_us()).unwrap();
    let ended = answer(&bus);
    assert_eq!(ended.id, "asked");
    assert!(ended.reason.contains("before it was played"), "{ended:?}");
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
