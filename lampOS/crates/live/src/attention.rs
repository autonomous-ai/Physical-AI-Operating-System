//! Whom the lamp answers: its own rule, applied before any sound is let out.
//!
//! The model decides what to say. Whether an answer is played is decided
//! here, by a rule a person can state: in a busy room the lamp speaks when
//! its name was said or when it is already in a conversation, and
//! otherwise stays out; in a quiet room it answers whatever is said near
//! it. That is the owner's rule (docs/brain-build.md, "follows the room").
//! The model was asked to keep it and does so most of the time. On
//! lamp-4ace on 2026-10-13, over three runs of new sessions that each
//! heard two people talking to each other, 9 of 35 spoke words to them,
//! whether the session was told how the room was, given a function to
//! decline with, or neither; "How was your weekend?" was answered in all
//! three runs.
//!
//! What makes the rule possible is timing. The service writes down what it
//! heard as well as what it says. In those runs the text of what was heard
//! had arrived before the first sound of every one of 24 answers, by 0.32 s
//! at the least and 0.44 s in the middle. So when an answer's first sound
//! comes, the lamp can already read whether it was called, and holding the
//! answer back costs nothing in time.
//!
//! An answer held back is dropped in the provider worker like one the
//! model declined itself (docs/tools.md): none of its sound reaches the
//! coordinator, and its words are not kept as something the lamp said.
//! The rule fails open. With no text of what was heard, with the room not
//! known to be busy, or in a turn the lamp began itself, the answer is
//! played.
//!
//! Off unless the service's environment says `on`. What it does not know
//! yet: whether the person is looking at the lamp. Someone who walks up to
//! it in a busy office and speaks without its name is held back today.
//! The eyes' report of who faces the lamp is to be added to the rule.

use crate::room_told::Room;
use std::sync::atomic::{AtomicU64, Ordering};

/// Where the coordinator reads whether the rule is applied. `on` only.
pub const ENVIRONMENT: &str = "LAMP_ATTENTION";
/// The provider worker's flag for it.
pub const FLAG: &str = "--attention";

/// The newest request the lamp began itself, in writing: a hello, a look.
/// Nothing was said to it, so nothing can have been said to someone else.
pub static OWN_REQUEST: AtomicU64 = AtomicU64::new(0);

/// The provider worker's argument for the switch in the environment.
pub fn arguments_from_environment() -> Vec<String> {
    arguments(std::env::var(ENVIRONMENT).ok().as_deref())
}

fn arguments(value: Option<&str>) -> Vec<String> {
    if value == Some("on") {
        vec![FLAG.to_owned()]
    } else {
        Vec::new()
    }
}

/// Take a leading `--attention` off a worker's flags.
pub fn take(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == FLAG => (true, rest),
        _ => (false, flags),
    }
}

/// Whether the lamp's name is in what was heard, as the service wrote it
/// down. "Lamb" is how it writes "Lamp" when it mishears; no other word is
/// taken for the name.
pub fn named(heard: &str) -> bool {
    heard
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("lamp") || word.eq_ignore_ascii_case("lamb"))
}

/// Whether an answer is played, and why, for the record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Play(&'static str),
    Hold(&'static str),
}

/// The rule. `heard` is the text of what was heard since the request
/// began, empty when none has arrived.
pub fn decide(room: Room, going_on: bool, own: bool, heard: &str) -> Verdict {
    if own {
        Verdict::Play("the lamp began this turn itself")
    } else if room != Room::Busy {
        Verdict::Play("the room is not known to be busy")
    } else if going_on {
        Verdict::Play("a conversation with the lamp is going on")
    } else if heard.trim().is_empty() {
        Verdict::Play("no text of what was heard had arrived")
    } else if named(heard) {
        Verdict::Play("its name was said")
    } else {
        Verdict::Hold("the room is busy and its name was not said")
    }
}

/// What the worker keeps for the request in hand.
#[derive(Default)]
pub struct Attention {
    request: u64,
    /// What was heard since it began, as far as the service has written it.
    heard: String,
    verdict: Option<Verdict>,
}

/// The most text kept for one request.
const MOST_HEARD_BYTES: usize = 2_048;

impl Attention {
    /// A request began.
    pub fn began(&mut self, request: u64) {
        self.request = request;
        self.heard.clear();
        self.verdict = None;
    }

    /// The service wrote down more of what it heard.
    pub fn heard(&mut self, text: &str) {
        let room = MOST_HEARD_BYTES.saturating_sub(self.heard.len());
        let cut = text
            .char_indices()
            .map(|(index, c)| index + c.len_utf8())
            .take_while(|end| *end <= room)
            .last()
            .unwrap_or(0);
        self.heard.push_str(&text[..cut]);
    }

    /// The first of an answer to `request` has come: decide once, and say
    /// so. `None` for a request this is not following, or already decided.
    pub fn first_output(&mut self, request: u64, room: Room, going_on: bool) -> Option<Verdict> {
        if request != self.request || self.verdict.is_some() {
            return None;
        }
        let own = OWN_REQUEST.load(Ordering::Acquire) == request;
        let verdict = decide(room, going_on, own, &self.heard);
        self.verdict = Some(verdict);
        Some(verdict)
    }

    /// Whether `request`'s answer is being held back.
    pub fn holds(&self, request: u64) -> bool {
        request == self.request && matches!(self.verdict, Some(Verdict::Hold(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_on_turns_it_on_and_the_flag_is_taken_from_the_front() {
        assert_eq!(arguments(Some("on")), [FLAG]);
        assert!(arguments(Some("yes")).is_empty());
        assert!(arguments(None).is_empty());
        let flags = [FLAG.to_owned(), "rest".to_owned()];
        assert_eq!(take(&flags), (true, &flags[1..]));
        assert_eq!(take(&flags[1..]), (false, &flags[1..]));
    }

    /// The name as the service has written it in the lamp's own records,
    /// and words that only contain it.
    #[test]
    fn the_name_is_a_word_of_its_own_however_it_is_written() {
        for called in [
            "Lamp, what is two plus two?",
            "Hey Lamp, tell me a joke.",
            "hey lamp",
            "What time is it, Lamp?",
            "LAMP!",
            "Hey, Lamb, how many days are there in a week?",
            "Okay lamp. Stop.",
        ] {
            assert!(named(called), "{called}");
        }
        for not_called in [
            "How was your weekend?",
            "Do you know where the new monitors went?",
            "The lamps in the storage room are new.",
            "We clamped it down yesterday.",
            "Bây giờ mình có một vấn đề hơi khó.",
            "",
        ] {
            assert!(!named(not_called), "{not_called}");
        }
    }

    /// Each line of the rule, and that every doubt plays the answer.
    #[test]
    fn in_a_busy_room_an_answer_is_held_back_unless_called_and_every_doubt_plays_it() {
        let weekend = "How was your weekend?";
        assert_eq!(
            decide(Room::Busy, false, false, weekend),
            Verdict::Hold("the room is busy and its name was not said")
        );
        // Called, in a conversation already, or the lamp's own turn.
        assert!(matches!(
            decide(Room::Busy, false, false, "Lamp, how was your weekend?"),
            Verdict::Play("its name was said")
        ));
        assert!(matches!(
            decide(Room::Busy, true, false, weekend),
            Verdict::Play("a conversation with the lamp is going on")
        ));
        assert!(matches!(
            decide(Room::Busy, false, true, weekend),
            Verdict::Play(_)
        ));
        // The room quiet, or not yet known.
        for room in [Room::Quiet, Room::Unknown] {
            assert!(matches!(
                decide(room, false, false, weekend),
                Verdict::Play("the room is not known to be busy")
            ));
        }
        // Nothing written of what was heard: no ground to hold on.
        for nothing in ["", "  "] {
            assert!(matches!(
                decide(Room::Busy, false, false, nothing),
                Verdict::Play("no text of what was heard had arrived")
            ));
        }
    }

    #[test]
    fn a_request_is_decided_once_from_what_was_heard_since_it_began() {
        let mut attention = Attention::default();
        attention.began(7);
        attention.heard("How was ");
        attention.heard("your weekend?");
        // Another request's output decides nothing.
        assert_eq!(attention.first_output(6, Room::Busy, false), None);
        assert!(!attention.holds(7));
        assert_eq!(
            attention.first_output(7, Room::Busy, false),
            Some(Verdict::Hold("the room is busy and its name was not said"))
        );
        assert!(attention.holds(7) && !attention.holds(8));
        // Decided once: the room changing mid-answer changes nothing.
        assert_eq!(attention.first_output(7, Room::Quiet, true), None);
        assert!(attention.holds(7));
        // The next request starts clean: the name said in the last one
        // does not count for this one, nor does that one's verdict.
        attention.began(8);
        assert!(!attention.holds(8));
        attention.heard("Lamp, what time is it?");
        assert!(matches!(
            attention.first_output(8, Room::Busy, false),
            Some(Verdict::Play("its name was said"))
        ));
        // A turn the lamp began itself is played whatever was heard.
        attention.began(9);
        attention.heard("so did you send the report");
        OWN_REQUEST.store(9, Ordering::Release);
        assert!(matches!(
            attention.first_output(9, Room::Busy, false),
            Some(Verdict::Play("the lamp began this turn itself"))
        ));
        OWN_REQUEST.store(0, Ordering::Release);
    }

    #[test]
    fn what_is_kept_of_one_request_is_bounded_and_cut_at_a_character() {
        let mut attention = Attention::default();
        attention.began(1);
        attention.heard(&"é".repeat(MOST_HEARD_BYTES));
        assert_eq!(attention.heard.len(), MOST_HEARD_BYTES);
        attention.heard("Lamp");
        assert_eq!(attention.heard.len(), MOST_HEARD_BYTES);
        assert!(attention.heard.chars().all(|c| c == 'é'));
    }
}
