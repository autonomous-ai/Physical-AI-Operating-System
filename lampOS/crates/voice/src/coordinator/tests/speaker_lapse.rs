//! #1178, the reviewer on #1214: the speaker's authority lapsed while it was
//! held up during an answer. What it reports of the speech it lost is the
//! lapse, not a failed voice: the answer is cut, and the voice goes on.
use super::*;

struct Reports {
    events: VecDeque<WorkerEvent>,
}
impl SpeakerControlIo for Reports {
    fn next_speaker_event(&mut self) -> io::Result<Option<WorkerEvent>> {
        Ok(self.events.pop_front())
    }
    fn publish_speaker_snapshot(&mut self, _: Snapshot) -> io::Result<()> {
        Ok(())
    }
}

/// The current answer, its owner, and what the speaker's reports are read
/// with.
struct Answer {
    owner: Controller,
    reply: Option<Reply>,
    affected: TurnOwner,
    trace: SessionState,
    pending: Option<PendingSpeaker>,
    mask: PlaybackMask,
    snapshot: Snapshot,
}

impl Answer {
    fn new() -> Self {
        let (mut owner, current, _) = active_reply();
        let affected = current.owner;
        let mut trace = SessionState::default();
        trace.receipts.admitted(affected, monotonic_us()).unwrap();
        let snapshot = owner.snapshot(now()).unwrap();
        Self {
            owner,
            reply: Some(current),
            affected,
            trace,
            pending: None,
            mask: PlaybackMask::new(TurnTaking::VadOnly),
            snapshot,
        }
    }

    /// Read `events` as the coordinator does; the answer to cut, if any.
    fn read(&mut self, events: Vec<WorkerEvent>) -> Result<Option<TurnOwner>> {
        let mut lapsed = None;
        service_speaker_control(
            &mut Reports {
                events: events.into(),
            },
            &mut self.owner,
            &mut self.reply,
            &mut self.pending,
            &mut self.mask,
            &mut self.snapshot,
            &mut 0,
            &mut 16,
            &mut self.trace,
            &mut lapsed,
        )?;
        Ok(lapsed)
    }

    fn rows(&self, kind: &str) -> Vec<&Value> {
        (0..self.trace.len())
            .map(|row| &self.trace[row])
            .filter(|row| row["kind"] == kind)
            .collect()
    }
}

/// Queued speech dropped by the speaker's ownership check as its authority
/// lapsed (`ExpiredState`). Before, it ended the voice ("unexpected
/// playback discard"); now the answer is cut, with what the provider had
/// queued for it, and its owner's turn is over.
#[test]
fn a_lapse_that_drops_the_answers_speech_cuts_the_answer_not_the_voice() {
    let mut answer = Answer::new();
    let affected = answer.affected;
    let lapsed = answer
        .read(vec![discarded(
            affected,
            PlaybackDiscardReason::ExpiredState,
            false,
        )])
        .unwrap();
    assert_eq!(lapsed, Some(affected));
    let discard = answer.rows("playback_discarded");
    assert_eq!(
        (&discard[0]["lapsed"], &discard[0]["expected_cancellation"]),
        (&json!(true), &json!(false))
    );
    let mut provider_queue = VecDeque::from([1, 2, 3]);
    assert!(
        cut_after_speaker_lapse(
            affected,
            &mut answer.owner,
            &mut answer.reply,
            &mut provider_queue,
            &mut answer.pending,
            &mut answer.trace,
        )
        .unwrap()
    );
    assert!(answer.reply.is_none());
    assert!(provider_queue.is_empty());
    assert!(answer.owner.snapshot(now()).unwrap().owner().is_none());
    assert_eq!(answer.rows("turn_finished")[0]["outcome"], SPEAKER_LAPSED);
    // The speaker's own revocation of what is left is that cut's.
    assert!(
        answer
            .read(vec![discarded(
                affected,
                PlaybackDiscardReason::StaleOwner,
                false,
            )])
            .unwrap()
            .is_none()
    );
    assert_eq!(
        answer.rows("playback_discarded")[1]["expected_cancellation"],
        true
    );
}

/// The chunk the speaker had not finished writing, refused as the lapse. A
/// refusal of the current answer for any other reason still ends the voice.
#[test]
fn a_chunk_refused_on_a_lapse_cuts_the_answer_and_any_other_refusal_fails() {
    let refused = |answer: &Answer, lapsed: bool| WorkerEvent::PlaybackRejected {
        turn: answer.affected.turn(),
        sequence: 7,
        reason: lamp_turn_owner::Error::ExpiredState.to_string(),
        lapsed,
    };
    let mut answer = Answer::new();
    let event = refused(&answer, true);
    assert_eq!(answer.read(vec![event]).unwrap(), Some(answer.affected));
    assert_eq!(answer.rows("speaker_rejected")[0]["lapsed"], true);
    let mut answer = Answer::new();
    let event = refused(&answer, false);
    assert_eq!(
        answer.read(vec![event]).unwrap_err().to_string(),
        "current reply rejected at speaker"
    );
}

/// A lapse that drops an earlier answer's speech, after this one was cut
/// or finished, cuts nothing.
#[test]
fn a_lapse_of_an_answer_no_longer_current_cuts_nothing() {
    let mut answer = Answer::new();
    let affected = answer.affected;
    cancel_reply(
        &mut answer.owner,
        &mut answer.reply,
        &mut answer.trace,
        "capture_gap",
    )
    .unwrap();
    let lapsed = answer
        .read(vec![discarded(
            affected,
            PlaybackDiscardReason::ExpiredState,
            false,
        )])
        .unwrap();
    assert_eq!(lapsed, None);
    assert_eq!(
        answer.rows("playback_discarded")[0]["expected_cancellation"],
        true
    );
    let mut provider_queue = VecDeque::from([1]);
    assert!(
        !cut_after_speaker_lapse(
            affected,
            &mut answer.owner,
            &mut answer.reply,
            &mut provider_queue,
            &mut answer.pending,
            &mut answer.trace,
        )
        .unwrap()
    );
    assert_eq!(provider_queue.len(), 1);
}
