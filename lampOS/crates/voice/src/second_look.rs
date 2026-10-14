//! A second look at something said to the lamp by name that it said
//! nothing to (`docs/attention.md`).
//!
//! The lamp hears its name in the room's sound (`docs/name.md`). The model
//! does not: the text it works from leaves the name out, or makes another
//! word of it. After two people have talked about lunch, a called "what do
//! you think we should have for lunch?" reads to it as more of their talk,
//! and it stays out, with the lamp's own ears having heard "Hey Lamp".
//!
//! So when a request that was heard ends with nothing said, and the name
//! was heard plainly in the sound as that request began, the lamp tells
//! the model so, once, in a written note in place of speech (as it does
//! for a hello, `crate::speak_up`), and what the model then says is
//! spoken. The note gives the one sign the model lacked and leaves the
//! rest of the judgement where it was: other people's business said with
//! a name like the lamp's is still not the lamp's.
//!
//! It is late: a silent answer is only known to be one when the service's
//! wait for it has run out. Someone who speaks first is listened to and
//! the second look is dropped.

/// `on` for the lamp to take a second look. Anything else: nothing here
/// is ever due.
pub const ENVIRONMENT: &str = "LAMP_SECOND_LOOK";

/// What the model is given in place of something heard.
pub const NOTE: &str = "(Your own ears caught your name, Lamp, at the start of what was just said to you, so you were being called. If it is something a lamp on a desk could answer or chat about, answer it now in your usual way, as if you had not waited, and do not mention the wait or your name. If it is other people's own business, say nothing.)";

/// The least a hearing scores for the lamp to pass it on: the score from
/// which the part that listens reports one at all (`docs/name.md`). It was
/// 0.98 for a morning. On lamp-4ace on 2026-10-14 about a quarter of the
/// hearings of played calls scored 0.91 to 0.98, one at 0.967 got no answer
/// and no second look, and lines to a name like the lamp's ("Hey Lan")
/// scored 1.000 as often as real calls did: the score does not tell the
/// two apart, and what keeps the lamp out of other people's business is
/// the note's own last sentence and the model's judgement.
pub const LEAST_SCORE: f32 = 0.9;
/// The name counts for a request if it began no longer than this before
/// the request did (`crate::attention::NAME_BEFORE_US`).
pub const BEFORE_US: u64 = crate::attention::NAME_BEFORE_US;
/// A second look that has not begun this long after the silence is
/// dropped: the moment has gone.
pub const WAIT_US: u64 = 3_000_000;

/// Whether the environment switches it on.
pub fn on() -> bool {
    std::env::var(ENVIRONMENT).ok().as_deref() == Some("on")
}

#[derive(Debug, Default)]
pub struct SecondLook {
    /// The request in hand that was heard, and when it began.
    began: Option<(u64, u64)>,
    /// The plainest hearing so far whose name began at or after
    /// [`BEFORE_US`] before the request in hand: when, and its score.
    named: Option<(u64, f32)>,
    /// The newest hearing of all, for the request that begins next.
    last: Option<(u64, f32)>,
    /// A second look that is owed: for which request, and since when.
    due: Option<(u64, u64)>,
    /// The model is also told while the person is still speaking
    /// (`crate::called`), when the name is heard in time for that.
    told_while_speaking: bool,
    /// The sound of the request in hand is still going to the service.
    open: bool,
    /// The name was heard in time for the model to have been told of it
    /// inside the request in hand. One asking is enough.
    told: bool,
}

impl SecondLook {
    /// A second look for a lamp whose model is also told of the name while
    /// the person is still speaking: a request the model was told in is
    /// not asked about again.
    pub fn with_the_note(mut self, told_while_speaking: bool) -> Self {
        self.told_while_speaking = told_while_speaking;
        self
    }

    /// The name began to be heard at `began_us`, with this score.
    pub fn name_heard(&mut self, began_us: u64, score: f32) {
        self.last = Some((began_us, score));
        if let Some((_, request_began)) = self.began
            && began_us.saturating_add(BEFORE_US) >= request_began
        {
            if self.named.is_none_or(|(_, best)| score > best) {
                self.named = Some((began_us, score));
            }
            // Heard while the request's sound was still being sent: the
            // note went with it.
            if self.open && score >= LEAST_SCORE {
                self.told = true;
            }
        }
    }

    /// Someone began to speak: `turn` is heard from `now_us`. Whatever was
    /// owed for an earlier request is dropped, for they are listened to.
    pub fn request_began(&mut self, turn: u64, now_us: u64) {
        self.began = Some((turn, now_us));
        self.due = None;
        self.named = self
            .last
            .filter(|(at, _)| at.saturating_add(BEFORE_US) >= now_us);
        self.open = true;
        // Heard as the request began: the note goes at its opening.
        self.told = self.named.is_some_and(|(_, score)| score >= LEAST_SCORE);
    }

    /// The person has stopped: the sound of `turn` is no longer being
    /// sent, and a name heard from now on comes too late for a note.
    pub fn speech_ended(&mut self, turn: u64) {
        if self.began.is_some_and(|(heard, _)| heard == turn) {
            self.open = false;
        }
    }

    /// `turn` is over. `said_nothing`: the service gave no answer to it.
    /// The score of the hearing if a second look is now owed.
    pub fn finished(&mut self, turn: u64, said_nothing: bool, now_us: u64) -> Option<f32> {
        let (heard, _) = self.began?;
        if heard != turn {
            return None;
        }
        self.began = None;
        self.open = false;
        let told = std::mem::take(&mut self.told) && self.told_while_speaking;
        let (_, score) = self.named.take()?;
        if !said_nothing || score < LEAST_SCORE || told {
            return None;
        }
        self.due = Some((turn, now_us));
        Some(score)
    }

    /// The request a second look is owed for, if one still is at `now_us`.
    pub fn due(&mut self, now_us: u64) -> Option<u64> {
        let (turn, since) = self.due?;
        if now_us.saturating_sub(since) > WAIT_US {
            self.due = None;
            return None;
        }
        Some(turn)
    }

    /// The second look was taken.
    pub fn taken(&mut self) {
        self.due = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: u64 = 1_000_000;

    /// The scene it is for: called by name, heard plainly, nothing said.
    #[test]
    fn a_request_called_by_name_that_got_nothing_is_owed_a_second_look_once() {
        let mut look = SecondLook::default();
        // The name is the first thing said, a little before the voice is
        // sure someone is speaking.
        look.name_heard(10 * SECOND - 300_000, 1.0);
        look.request_began(7, 10 * SECOND);
        assert_eq!(look.due(11 * SECOND), None);
        assert_eq!(look.finished(7, true, 17 * SECOND), Some(1.0));
        assert_eq!(look.due(17 * SECOND + 50_000), Some(7));
        look.taken();
        assert_eq!(look.due(17 * SECOND + 60_000), None);
        // The written turn that follows is not a heard request: when it
        // ends, silent or not, nothing more is owed.
        assert_eq!(look.finished(8, true, 22 * SECOND), None);
        assert_eq!(look.due(22 * SECOND), None);
    }

    #[test]
    fn nothing_is_owed_when_it_answered_or_was_not_called_or_was_called_faintly() {
        // It answered.
        let mut look = SecondLook::default();
        look.name_heard(10 * SECOND, 1.0);
        look.request_began(1, 10 * SECOND + 200_000);
        assert_eq!(look.finished(1, false, 14 * SECOND), None);
        assert_eq!(look.due(14 * SECOND), None);
        // No name was heard.
        let mut look = SecondLook::default();
        look.request_began(2, 20 * SECOND);
        assert_eq!(look.finished(2, true, 26 * SECOND), None);
        // The name was heard, under the score a second look asks for.
        let mut look = SecondLook::default();
        look.name_heard(30 * SECOND, 0.85);
        look.request_began(3, 30 * SECOND + 200_000);
        assert_eq!(look.finished(3, true, 36 * SECOND), None);
        // The name was heard long before: it was for an earlier request.
        let mut look = SecondLook::default();
        look.name_heard(40 * SECOND, 1.0);
        look.request_began(4, 40 * SECOND + BEFORE_US + 1);
        assert_eq!(look.finished(4, true, 50 * SECOND), None);
        // To the limit, it counts.
        let mut look = SecondLook::default();
        look.name_heard(40 * SECOND, 1.0);
        look.request_began(5, 40 * SECOND + BEFORE_US);
        assert_eq!(look.finished(5, true, 50 * SECOND), Some(1.0));
    }

    #[test]
    fn the_plainest_hearing_in_a_request_is_the_one_that_counts() {
        let mut look = SecondLook::default();
        look.request_began(1, 10 * SECOND);
        // Heard in the middle of the request, twice: once faintly.
        look.name_heard(11 * SECOND, 0.91);
        look.name_heard(12 * SECOND, 0.995);
        look.name_heard(12 * SECOND + 500_000, 0.92);
        assert_eq!(look.finished(1, true, 18 * SECOND), Some(0.995));
    }

    #[test]
    fn someone_who_speaks_first_is_listened_to_and_a_stale_look_is_dropped() {
        let mut look = SecondLook::default();
        look.name_heard(10 * SECOND, 1.0);
        look.request_began(1, 10 * SECOND + 100_000);
        assert!(look.finished(1, true, 16 * SECOND).is_some());
        // They speak again before the lamp has begun: listen to that.
        look.request_began(2, 16 * SECOND + 400_000);
        assert_eq!(look.due(16 * SECOND + 500_000), None);
        // And the name of the first request is not carried to the second.
        assert_eq!(look.finished(2, true, 22 * SECOND), None);
        // One that could not begin in three seconds is dropped.
        let mut look = SecondLook::default();
        look.name_heard(30 * SECOND, 1.0);
        look.request_began(3, 30 * SECOND);
        assert!(look.finished(3, true, 36 * SECOND).is_some());
        assert_eq!(look.due(36 * SECOND + WAIT_US), Some(3));
        assert_eq!(look.due(36 * SECOND + WAIT_US + 1), None);
        assert_eq!(look.due(36 * SECOND + 10), None);
    }

    /// With the note switched on, a request the model was told in is not
    /// asked about again; one whose name came only after the person had
    /// stopped still is.
    #[test]
    fn a_request_the_model_was_told_in_is_not_asked_about_again() {
        // Heard as the request began: told at its opening.
        let mut look = SecondLook::default().with_the_note(true);
        look.name_heard(10 * SECOND - 300_000, 1.0);
        look.request_began(1, 10 * SECOND);
        look.speech_ended(1);
        assert_eq!(look.finished(1, true, 16 * SECOND), None);
        assert_eq!(look.due(16 * SECOND), None);
        // Heard in the middle, while the sound was still going: told then.
        let mut look = SecondLook::default().with_the_note(true);
        look.request_began(2, 20 * SECOND);
        look.name_heard(21 * SECOND, 0.95);
        look.speech_ended(2);
        assert_eq!(look.finished(2, true, 27 * SECOND), None);
        // Heard only once the person had stopped: no note could go, and
        // the second look is what is left.
        let mut look = SecondLook::default().with_the_note(true);
        look.request_began(3, 30 * SECOND);
        look.speech_ended(3);
        look.name_heard(30 * SECOND + 200_000, 1.0);
        assert_eq!(look.finished(3, true, 36 * SECOND), Some(1.0));
        // What was told in one request says nothing of the next.
        let mut look = SecondLook::default().with_the_note(true);
        look.name_heard(40 * SECOND, 1.0);
        look.request_began(4, 40 * SECOND + 100_000);
        look.speech_ended(4);
        assert_eq!(look.finished(4, true, 46 * SECOND), None);
        look.request_began(5, 60 * SECOND);
        look.speech_ended(5);
        look.name_heard(60 * SECOND + 300_000, 1.0);
        assert_eq!(look.finished(5, true, 66 * SECOND), Some(1.0));
        // Without the note every such request is asked about, as before.
        let mut look = SecondLook::default();
        look.name_heard(70 * SECOND, 1.0);
        look.request_began(6, 70 * SECOND + 100_000);
        look.speech_ended(6);
        assert_eq!(look.finished(6, true, 76 * SECOND), Some(1.0));
    }

    /// A turn that ends which is not the one in hand changes nothing.
    #[test]
    fn another_turns_ending_does_not_settle_the_request_in_hand() {
        let mut look = SecondLook::default();
        look.name_heard(10 * SECOND, 1.0);
        look.request_began(5, 10 * SECOND);
        assert_eq!(look.finished(4, true, 11 * SECOND), None);
        assert_eq!(look.finished(5, true, 16 * SECOND), Some(1.0));
    }
}
