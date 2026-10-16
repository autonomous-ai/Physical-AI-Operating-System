//! The say service's turn (#939): a say becomes the voice's written turn
//! through the real coordinator path, and its asker is told `spoken` only
//! when the turn completed with its audio played whole through the real
//! speaker control; a turn cut short, or one with no audio, is `dropped`
//! with what became of it. The provider PCM and speaker receipts are
//! authored, not acoustic evidence.
use super::*;
use crate::say::{Answer, Outcome};
use lamp_audio::ownership::PlaybackCursor;
use std::sync::mpsc;

struct Speaker {
    channels: WorkerChannels,
}
impl SpeakerControlIo for Speaker {
    fn next_speaker_event(&mut self) -> io::Result<Option<WorkerEvent>> {
        self.channels.control.receive()
    }
    fn publish_speaker_snapshot(&mut self, snapshot: Snapshot) -> io::Result<()> {
        self.channels.control.send(Control::Authority { snapshot })
    }
}

struct Test {
    _directory: SessionDirectory,
    speaker: Speaker,
    peer: WorkerChannels,
    owner: Controller,
    reply: Option<Reply>,
    pending: Option<PendingSpeaker>,
    snapshot: Snapshot,
    mask: PlaybackMask,
    trace: SessionState,
    cloud: CloudAvailability,
    sequence: u64,
    cursor: u64,
    answered: mpsc::Receiver<Answer>,
}

impl Test {
    /// A say taken by the voice and begun: its written turn submitted to
    /// the provider, which has started it.
    fn new() -> Self {
        let directory = SessionDirectory::create().unwrap();
        let (channels, peer) = pair(&directory, "speaker");
        let (mut provider, mut provider_peer) = pair(&directory, "provider");
        let mut speaker = Speaker { channels };
        let mut owner = Controller::new(new_boot().unwrap(), now());
        owner
            .set_microphone_permission(now(), Permission::Allowed)
            .unwrap();
        let until = MonoTime::from_micros(monotonic_us() + 1_000_000);
        owner
            .set_capture(now(), CaptureState::RetainingUntil(until))
            .unwrap();
        owner
            .set_admission(now(), AdmissionState::OpenUntil(until))
            .unwrap();
        let (ticket, answered) = crate::say::ticket_for_test("reminders");
        let mut reply = None;
        let mut queue = VecDeque::new();
        let mut trace = SessionState::default();
        let snapshot = commit_written_turn(
            &mut owner,
            &mut reply,
            &mut queue,
            "Remind them to call the bank.",
            false,
            "skill_says",
            1,
            PlayoutStart::default(),
            &mut trace,
            |state| speaker.publish_speaker_snapshot(state),
        )
        .unwrap();
        reply.as_mut().unwrap().say = Some(ticket);
        let turn = reply.as_ref().unwrap().owner;
        assert!(matches!(queue.front(), Some(ProviderInput::Written {
            request, session: 1, text, look: false, ..
        }) if *request == turn.turn() && text == "Remind them to call the bank."));
        let mut cloud = CloudAvailability::default();
        cloud.observe(ready_link(1, 1), monotonic_us()).unwrap();
        flush_provider_input(&cloud, &mut reply, &mut queue, &mut provider.data).unwrap();
        assert!(matches!(
            provider_peer.data.receive::<ProviderInput>().unwrap(),
            Some(ProviderInput::Written { request, .. }) if request == turn.turn()
        ));
        observe_provider_output(
            &mut cloud,
            &mut reply,
            &ProviderOutput::Started {
                request: turn.turn(),
                session: 1,
                waiting_for_barrier: false,
            },
        )
        .unwrap();
        Self {
            _directory: directory,
            speaker,
            peer,
            owner,
            reply,
            pending: None,
            snapshot,
            mask: PlaybackMask::new(TurnTaking::VadOnly),
            trace,
            cloud,
            sequence: 0,
            cursor: 0,
            answered,
        }
    }

    fn renew_input_evidence(&mut self) {
        let until = MonoTime::from_micros(monotonic_us() + 1_000_000);
        self.owner
            .set_capture(now(), CaptureState::RetainingUntil(until))
            .unwrap();
        self.owner
            .set_admission(now(), AdmissionState::OpenUntil(until))
            .unwrap();
    }

    fn event(&mut self, event: WorkerEvent) {
        self.peer.control.send(event).unwrap();
        service_speaker_control(
            &mut self.speaker,
            &mut self.owner,
            &mut self.reply,
            &mut self.pending,
            &mut self.mask,
            &mut self.snapshot,
            &mut 0,
            &mut 16,
            &mut self.trace,
        )
        .unwrap();
    }

    /// One generation of the model's audio, handed to the speaker.
    fn generation(&mut self, value: i16) -> SpeakerChunk {
        self.renew_input_evidence();
        let request = self.reply.as_ref().unwrap().owner.turn();
        let received_at = monotonic_us();
        let samples = vec![value; 240];
        observe_provider_output(
            &mut self.cloud,
            &mut self.reply,
            &ProviderOutput::Audio {
                request,
                sequence: self.sequence + 1,
                provider_event_at_us: received_at,
                source_frames: samples.len(),
                source_offset: 0,
                samples: samples.clone(),
            },
        )
        .unwrap();
        let current = self.reply.as_mut().unwrap();
        current.append_provider_audio(samples, received_at);
        current.had_audio = true;
        observe_provider_output(
            &mut self.cloud,
            &mut self.reply,
            &ProviderOutput::GenerationComplete { request },
        )
        .unwrap();
        dispatch_speaker_chunk(
            &mut self.owner,
            &mut self.reply,
            &mut self.speaker.channels.data,
            &mut self.pending,
            &mut self.sequence,
            &mut self.snapshot,
            &mut self.mask,
        )
        .unwrap();
        let chunk = self.peer.data.receive::<SpeakerChunk>().unwrap().unwrap();
        self.trace
            .receipts
            .dispatched(chunk.permit.owner(), chunk.chunk_sequence)
            .unwrap();
        chunk
    }

    fn accept(&mut self, chunk: &SpeakerChunk, frames: usize) {
        let at = monotonic_us();
        self.event(WorkerEvent::Playback {
            owner: chunk.permit.owner(),
            turn: chunk.permit.owner().turn(),
            sequence: chunk.chunk_sequence,
            first_sample: PlaybackCursor {
                epoch: 1,
                frame: self.cursor,
            },
            end_sample: PlaybackCursor {
                epoch: 1,
                frame: self.cursor + frames as u64,
            },
            queue_observed_at_us: at,
            queued_frames: 0,
            accepted_frames: frames,
            observed_at_us: at,
        });
        self.cursor += frames as u64;
    }

    fn retire(&mut self, chunk: &SpeakerChunk) {
        self.event(WorkerEvent::SpeechRetired {
            owner: chunk.permit.owner(),
            sequence: chunk.chunk_sequence,
            final_sample: PlaybackCursor {
                epoch: 1,
                frame: self.cursor,
            },
            observed_at_us: monotonic_us(),
        });
    }

    fn provider_complete(&mut self) {
        let request = self.reply.as_ref().unwrap().owner.turn();
        observe_provider_output(
            &mut self.cloud,
            &mut self.reply,
            &ProviderOutput::TurnComplete {
                request,
                idle: true,
            },
        )
        .unwrap();
    }

    /// As the voice's loop ends a settled turn: completed, published, and
    /// the say answered.
    fn finish_if_settled(&mut self) -> bool {
        let current = self.reply.as_mut().unwrap();
        if !current.settled(&self.pending) {
            return false;
        }
        self.owner.complete_turn(now(), current.owner).unwrap();
        self.snapshot = self.owner.snapshot(now()).unwrap();
        self.speaker
            .publish_speaker_snapshot(self.snapshot)
            .unwrap();
        current.answer_say(&self.pending, self.snapshot);
        self.reply = None;
        true
    }
}

/// Two generations, each accepted and retired, then the provider's
/// completion: `spoken`, and not a moment before the turn settles.
#[test]
fn a_say_whose_turn_completes_with_its_audio_played_is_spoken() {
    let mut test = Test::new();
    let first = test.generation(17);
    test.accept(&first, 240);
    test.retire(&first);
    assert!(!test.finish_if_settled(), "one generation is not the turn");
    assert!(test.answered.try_recv().is_err());
    let second = test.generation(23);
    test.provider_complete();
    test.accept(&second, 240);
    assert!(!test.finish_if_settled(), "accepted is not retired");
    assert!(test.answered.try_recv().is_err());
    test.retire(&second);
    assert!(test.finish_if_settled());
    assert_eq!(test.answered.try_recv(), Ok(Answer::Done(Outcome::Spoken)));
}

/// A person speaks over it: the voice cancels the turn, and the asker is
/// told it was dropped, and why.
#[test]
fn a_say_cut_short_is_dropped_with_why() {
    let mut test = Test::new();
    let first = test.generation(17);
    test.accept(&first, 120);
    cancel_reply(
        &mut test.owner,
        &mut test.reply,
        &mut test.trace,
        "user_interrupted",
    )
    .unwrap();
    assert_eq!(
        test.answered.try_recv(),
        Ok(Answer::Done(Outcome::Dropped("user_interrupted".into())))
    );
}

/// The model said nothing: the turn settles with no audio, and the say is
/// dropped as `no_audio_answer`.
#[test]
fn a_say_the_model_left_silent_is_dropped() {
    let mut test = Test::new();
    test.provider_complete();
    assert!(test.finish_if_settled());
    assert_eq!(
        test.answered.try_recv(),
        Ok(Answer::Done(Outcome::Dropped("no_audio_answer".into())))
    );
}

/// #939, (c): with the switch, a due reminder or news waits for a skill's
/// ask, at most `SKILL_WAITS_US` from when it was first held: taken with
/// the ask's ticket when one is there, said by the voice once it has
/// waited, and at once without the switch. Another one due begins its own
/// wait; an ask for the other source is not taken.
#[test]
fn a_due_reminder_or_news_waits_for_a_skill_at_most_its_time() {
    use crate::say::{SKILL_WAITS_US, Says, Source};
    let mut held = None;
    // Without the switch: said at once, as always.
    assert!(matches!(
        for_skill(None, Source::Schedule, false, "s/1/1", &mut held, 0),
        Some(None)
    ));
    assert_eq!(held, None);
    // With it and nobody asking: held, and said by the voice at 10 s.
    assert!(for_skill(None, Source::Schedule, true, "s/1/1", &mut held, 1_000).is_none());
    assert!(
        for_skill(
            None,
            Source::Schedule,
            true,
            "s/1/1",
            &mut held,
            SKILL_WAITS_US
        )
        .is_none()
    );
    // Another reminder due begins its own wait.
    assert!(
        for_skill(
            None,
            Source::Schedule,
            true,
            "s/2/1",
            &mut held,
            SKILL_WAITS_US + 5
        )
        .is_none()
    );
    assert!(matches!(
        for_skill(
            None,
            Source::Schedule,
            true,
            "s/2/1",
            &mut held,
            2 * SKILL_WAITS_US + 5
        ),
        Some(None)
    ));
    assert_eq!(held, None);
    // A skill asked for news: a reminder does not take it; news does.
    let (mut says, answered) = Says::asking_for_test(Source::Tasks, "task-news", 60_000_000);
    assert!(
        for_skill(
            Some(&mut says),
            Source::Schedule,
            true,
            "s/3/1",
            &mut held,
            2_000
        )
        .is_none()
    );
    let taken = for_skill(
        Some(&mut says),
        Source::Tasks,
        true,
        "t/7",
        &mut held,
        2_000,
    );
    let ticket = taken.flatten().expect("the skill's ticket");
    assert_eq!(ticket.skill(), "task-news");
    assert_eq!(held, None);
    ticket.answer(Outcome::Spoken);
    assert_eq!(answered.try_recv(), Ok(Answer::Done(Outcome::Spoken)));
    assert_eq!(says.waiting(), 0);
}
