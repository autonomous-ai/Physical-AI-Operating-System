//! Whom the lamp answers: its own rule, applied before any sound is let out.
//!
//! The model decides what to say. Whether an answer is played is, for the
//! first things a new session hears, decided here.
//!
//! **What goes wrong.** A session that has just opened has no history of
//! leaving talk alone, and takes what it hears for the start of a
//! conversation with it. On lamp-4ace on 2026-10-13, over three runs of
//! new sessions that each heard two people talking to each other, 9 of 35
//! spoke words to them within their first two requests, whether the
//! session was told how the room was, given a function to decline with, or
//! neither. A session that has been running does this far less: 2 of 87
//! things heard, that afternoon.
//!
//! **The rule.** In a room the lamp knows to be busy, an answer to one of
//! a session's first [`FRESH_REQUESTS`] requests is held back, unless the
//! lamp is already in a conversation, began the turn itself, or the text
//! of what it heard shows it was called. Everywhere else the model's own
//! judgement stands, as before.
//!
//! **What the rule may not lean on.** The first form of it held back every
//! answer in a busy room unless the lamp's name was in the service's text
//! of what it heard. Played the same evening it stopped every answer to
//! other people's talk (0 of 11 sessions) and also held back both of two
//! questions that used the name: the service does not write the name. It
//! wrote "Milan, what is the capital of Italy?" for "Hey Lamp, ...",
//! dropped "Lamp," from the other, and in 310 lines heard that day wrote
//! "Lamp" not once. So the text can show that the lamp was called and can
//! never show that it was not. Here it is used the first way only, and
//! the hold is kept to the few requests where the model is known to be
//! unreliable. The cost that remains: someone who calls the lamp in a busy
//! room within its first two requests after a new session, in words the
//! text gives no sign of, is not answered until they say it again.
//!
//! What makes holding possible at all is timing: in those runs the text of
//! what was heard had arrived before the first sound of every one of 24
//! answers, by 0.32 s at the least.
//!
//! An answer held back is dropped in the provider worker like one the
//! model declined itself (docs/tools.md). Off unless the service's
//! environment says `on`. A better sign of being called (the lamp
//! spotting its name in its own sound, the eyes seeing someone face it)
//! is to take the text's place (docs/attention.md).

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

/// Where the coordinator reads whether the model is asked to say when it
/// hears the lamp's name. `on` only. A measurement switch: it changes
/// nothing the lamp does, and exists to count whether the model's own sign
/// can stand where the heard text could not.
pub const NAME_ENVIRONMENT: &str = "LAMP_NAME_SIGNAL";
/// The provider worker's flag for it.
pub const NAME_FLAG: &str = "--name-signal";
/// The function the model calls when it hears the name.
pub const HEARD_MY_NAME: &str = "heard_my_name";

/// The provider worker's argument for that switch in the environment.
pub fn name_arguments_from_environment() -> Vec<String> {
    if std::env::var(NAME_ENVIRONMENT).ok().as_deref() == Some("on") {
        vec![NAME_FLAG.to_owned()]
    } else {
        Vec::new()
    }
}

/// Take a leading `--name-signal` off a worker's flags.
pub fn take_name(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == NAME_FLAG => (true, rest),
        _ => (false, flags),
    }
}

/// The function itself: silent, so that calling it neither delays the
/// answer nor adds a word to it.
pub fn name_tool() -> lamp_gemini::Tool {
    lamp_gemini::Tool {
        name: HEARD_MY_NAME.into(),
        description: "Call this every time someone says your name, Lamp, when speaking to you, \
                      whatever else they say. It makes no sound and nobody sees it. Then answer \
                      them exactly as you would have."
            .into(),
        parameters: None,
        silent: true,
    }
}

/// Take a leading `--attention` off a worker's flags.
pub fn take(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == FLAG => (true, rest),
        _ => (false, flags),
    }
}

/// How many of a session's first requests the rule covers. Counted in the
/// played runs: the answers to other people's talk came on a new session's
/// first or second request.
pub const FRESH_REQUESTS: u32 = 2;

/// Whether the text of what was heard shows the lamp was called. Its name,
/// or one of the words the service has written in its place at the start
/// of a called request in the lamp's own records ("Milan, what is the
/// capital of Italy?", "Man, tell me a joke.", "Lam", "Lan", "plan").
/// Positive evidence only: a text without them says nothing either way.
pub fn called(heard: &str) -> bool {
    let mut words = heard
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty());
    let is = |word: &str, list: &[&str]| list.iter().any(|known| word.eq_ignore_ascii_case(known));
    let all: Vec<&str> = words.by_ref().collect();
    if all.iter().any(|word| is(word, &["lamp", "lamb"])) {
        return true;
    }
    // A misheard name stands where the name stood: first, or after a
    // greeting.
    let first = all
        .iter()
        .find(|word| !is(word, &["hey", "hi", "ok", "okay", "oh", "hello"]));
    first.is_some_and(|word| is(word, &["lam", "lan", "milan", "man", "plan"]))
}

/// Whether an answer is played, and why, for the record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Play(&'static str),
    Hold(&'static str),
}

/// The lamp's name counts for a request if it began to be heard no longer
/// than this before the request began: the name is the first thing said,
/// and the voice takes a moment to be sure someone is speaking.
pub const NAME_BEFORE_US: u64 = 1_500_000;
const NAMED: &str = "its name was heard in the room's sound";

/// The rule. `fresh`: the request is one of its session's first
/// [`FRESH_REQUESTS`]. `heard`: the text of what was heard since the
/// request began, empty when none has arrived. `named`: the lamp's name
/// was heard in the sound itself as the request began (`docs/name.md`),
/// which like the text can speak for playing and never against.
pub fn decide(
    room: Room,
    going_on: bool,
    own: bool,
    fresh: bool,
    heard: &str,
    named: bool,
) -> Verdict {
    if own {
        Verdict::Play("the lamp began this turn itself")
    } else if !fresh {
        Verdict::Play("the session has heard enough to be trusted")
    } else if room != Room::Busy {
        Verdict::Play("the room is not known to be busy")
    } else if going_on {
        Verdict::Play("a conversation with the lamp is going on")
    } else if called(heard) {
        Verdict::Play("the text shows it was called")
    } else if named {
        Verdict::Play(NAMED)
    } else {
        Verdict::Hold("a new session in a busy room, and nothing shows it was called")
    }
}

/// What the worker keeps for the request in hand.
#[derive(Default)]
pub struct Attention {
    request: u64,
    /// The session the request began on, and how many requests have begun
    /// on it.
    session: u64,
    begun: u32,
    /// What was heard since it began, as far as the service has written it.
    heard: String,
    verdict: Option<Verdict>,
    /// When the request began, if that is known, and when the lamp's name
    /// last began to be heard in the room's sound, by this process's clock.
    began_us: Option<u64>,
    named_us: Option<u64>,
}

/// The most text kept for one request.
const MOST_HEARD_BYTES: usize = 2_048;

impl Attention {
    /// A request began on `session`, at `now_us`.
    pub fn began_at(&mut self, request: u64, session: u64, now_us: u64) {
        self.began(request, session);
        self.began_us = Some(now_us);
    }

    /// The lamp's name began to be heard in the room's sound at `at_us`.
    pub fn name_heard(&mut self, at_us: u64) {
        self.named_us = Some(self.named_us.map_or(at_us, |before| before.max(at_us)));
    }

    /// Whether the name was heard for the request in hand: as it began,
    /// or since. A request whose beginning has no time has no name.
    fn named(&self) -> bool {
        match (self.named_us, self.began_us) {
            (Some(named), Some(began)) => named.saturating_add(NAME_BEFORE_US) >= began,
            _ => false,
        }
    }

    /// A request began on `session`.
    pub fn began(&mut self, request: u64, session: u64) {
        if session != self.session {
            self.session = session;
            self.begun = 0;
        }
        self.begun = self.begun.saturating_add(1);
        self.request = request;
        self.heard.clear();
        self.verdict = None;
        self.began_us = None;
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
        let fresh = self.begun <= FRESH_REQUESTS;
        let verdict = decide(room, going_on, own, fresh, &self.heard, self.named());
        self.verdict = Some(verdict);
        Some(verdict)
    }

    /// Whether the answer to the request in hand was let out because the
    /// name was heard, for the record: the one case the sound decided.
    pub fn played_for_its_name(&self) -> bool {
        self.verdict == Some(Verdict::Play(NAMED))
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

    /// What the service really wrote for called requests in the lamp's
    /// records of 2026-10-13, and what it wrote for other people's talk.
    #[test]
    fn the_text_can_show_a_call_by_the_name_or_by_what_the_service_writes_in_its_place() {
        for shows in [
            "Lamp, what is two plus two?",
            "Hey Lamp, tell me a joke.",
            "What time is it, Lamp?",
            "Hey, Lamb, how many days are there in a week?",
            "Milan, what is the capital of Italy?",
            "Milan, how are you feeling right now?",
            "Man, tell me a joke.",
            "Ok Lam, stop.",
            "Hey Lan",
            "plan",
        ] {
            assert!(called(shows), "{shows}");
        }
        for silent in [
            // Called, and written with nothing left of the name: the text
            // cannot show it, which is why it is never read as "not called".
            "What color is the sky on a clear day?",
            "What is your favorite color?",
            // Other people's talk.
            "How was your weekend?",
            "Do you know where the new monitors went?",
            "The lamps in the storage room are new.",
            "We planned it with the man from Milan.",
            "Bây giờ mình có một vấn đề hơi khó.",
            "",
        ] {
            assert!(!called(silent), "{silent}");
        }
    }

    /// Each line of the rule. Only one case holds an answer back.
    #[test]
    fn only_a_new_sessions_first_requests_in_a_busy_room_are_held_and_every_doubt_plays() {
        let weekend = "How was your weekend?";
        assert_eq!(
            decide(Room::Busy, false, false, true, weekend, false),
            Verdict::Hold("a new session in a busy room, and nothing shows it was called")
        );
        // No text at all is held too: the text can only ever speak for
        // playing, and its absence says nothing.
        assert!(matches!(
            decide(Room::Busy, false, false, true, "", false),
            Verdict::Hold(_)
        ));
        // The same request on a session that has heard enough: played.
        assert!(matches!(
            decide(Room::Busy, false, false, false, weekend, false),
            Verdict::Play("the session has heard enough to be trusted")
        ));
        assert!(matches!(
            decide(
                Room::Busy,
                false,
                false,
                true,
                "Milan, how was your weekend?",
                false,
            ),
            Verdict::Play("the text shows it was called")
        ));
        assert!(matches!(
            decide(Room::Busy, true, false, true, weekend, false),
            Verdict::Play("a conversation with the lamp is going on")
        ));
        assert!(matches!(
            decide(Room::Busy, false, true, true, weekend, false),
            Verdict::Play("the lamp began this turn itself")
        ));
        for room in [Room::Quiet, Room::Unknown] {
            assert!(matches!(
                decide(room, false, false, true, weekend, false),
                Verdict::Play("the room is not known to be busy")
            ));
        }
        // The name heard in the sound plays what would have been held,
        // whatever the text says or does not say.
        for text in [weekend, ""] {
            assert_eq!(
                decide(Room::Busy, false, false, true, text, true),
                Verdict::Play("its name was heard in the room's sound")
            );
        }
    }

    /// The name is the first thing said, a little before the voice is
    /// sure someone is speaking. It counts for the request that begins
    /// then, and for no later one.
    #[test]
    fn the_name_heard_as_a_request_began_plays_it_and_is_not_carried_to_the_next() {
        let second = 1_000_000u64;
        let mut attention = Attention::default();
        // Heard 0.4 s before the request began.
        attention.name_heard(10 * second - 400_000);
        attention.began_at(1, 7, 10 * second);
        assert_eq!(
            attention.first_output(1, Room::Busy, false),
            Some(Verdict::Play("its name was heard in the room's sound"))
        );
        assert!(attention.played_for_its_name() && !attention.holds(1));
        // The next request, twenty seconds on, with no name: held.
        attention.began_at(2, 7, 30 * second);
        assert!(matches!(
            attention.first_output(2, Room::Busy, false),
            Some(Verdict::Hold(_))
        ));
        assert!(!attention.played_for_its_name() && attention.holds(2));
        // A name heard in the middle of a request counts for it too.
        let mut attention = Attention::default();
        attention.began_at(1, 8, 50 * second);
        attention.name_heard(51 * second);
        assert!(matches!(
            attention.first_output(1, Room::Busy, false),
            Some(Verdict::Play(_))
        ));
        // To the limit: 1.5 s before counts, a moment more does not.
        for (before, plays) in [(NAME_BEFORE_US, true), (NAME_BEFORE_US + 1, false)] {
            let mut attention = Attention::default();
            attention.name_heard(80 * second - before);
            attention.began_at(1, 9, 80 * second);
            let verdict = attention.first_output(1, Room::Busy, false).unwrap();
            assert_eq!(matches!(verdict, Verdict::Play(_)), plays, "{before}");
        }
        // Nothing is listening for the name, or it was not heard: as before.
        let mut attention = Attention::default();
        attention.began_at(1, 10, 90 * second);
        assert!(matches!(
            attention.first_output(1, Room::Busy, false),
            Some(Verdict::Hold(_))
        ));
        // A request that began at no known time takes no name, however
        // lately one was heard.
        let mut attention = Attention::default();
        attention.name_heard(95 * second);
        attention.began(1, 11);
        assert!(matches!(
            attention.first_output(1, Room::Busy, false),
            Some(Verdict::Hold(_))
        ));
    }

    /// A session's first two requests are the fresh ones; the count starts
    /// again with each new session, and a request is decided once.
    #[test]
    fn the_first_two_requests_of_each_session_are_the_ones_held() {
        let mut attention = Attention::default();
        let outcome = |attention: &mut Attention, request: u64, session: u64| {
            attention.began(request, session);
            attention.heard("How was your weekend?");
            attention.first_output(request, Room::Busy, false)
        };
        assert!(matches!(
            outcome(&mut attention, 1, 5),
            Some(Verdict::Hold(_))
        ));
        assert!(attention.holds(1) && !attention.holds(2));
        // Decided once: the room changing mid-answer changes nothing.
        assert_eq!(attention.first_output(1, Room::Quiet, true), None);
        assert!(attention.holds(1));
        assert!(matches!(
            outcome(&mut attention, 2, 5),
            Some(Verdict::Hold(_))
        ));
        assert!(matches!(
            outcome(&mut attention, 3, 5),
            Some(Verdict::Play(_))
        ));
        assert!(matches!(
            outcome(&mut attention, 4, 5),
            Some(Verdict::Play(_))
        ));
        // A new session: its first two again.
        assert!(matches!(
            outcome(&mut attention, 5, 6),
            Some(Verdict::Hold(_))
        ));
        assert!(matches!(
            outcome(&mut attention, 6, 6),
            Some(Verdict::Hold(_))
        ));
        assert!(matches!(
            outcome(&mut attention, 7, 6),
            Some(Verdict::Play(_))
        ));
        // Another request's output decides nothing.
        attention.began(8, 7);
        assert_eq!(attention.first_output(7, Room::Busy, false), None);
        // A turn the lamp began itself is played whatever was heard.
        attention.began(9, 8);
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
        attention.began(1, 1);
        attention.heard(&"é".repeat(MOST_HEARD_BYTES));
        assert_eq!(attention.heard.len(), MOST_HEARD_BYTES);
        attention.heard("Lamp");
        assert_eq!(attention.heard.len(), MOST_HEARD_BYTES);
        assert!(attention.heard.chars().all(|c| c == 'é'));
    }
}
