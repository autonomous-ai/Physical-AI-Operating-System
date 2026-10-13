//! How the room is, told to the voice.
//!
//! In an office that is talking, a lamp that answers whatever seems meant
//! for it answers colleagues: about thirty times in fifty minutes on
//! lamp-4ace on 2026-10-13. So its character answers only when called. In a
//! room where one person is left, the same rule makes the lamp deaf to
//! them: they speak, it says nothing, and they have to learn to say its
//! name first. A person at the next desk does neither. They join in less
//! the more talk there is around them.
//!
//! The provider worker sees every request and whether the lamp spoke in it.
//! From that alone:
//!
//! - **quiet**: nothing has been heard for five minutes. What is said next
//!   is said into a quiet room, most likely to the lamp.
//! - **busy**: three things in a row, inside a minute, that the lamp said
//!   nothing to. People are talking to each other.
//!
//! Busy holds until the five quiet minutes come; quiet holds until the run
//! of three. Neither is known when the service starts. The session is told
//! when the room changes, and a new session is told how it is, in a turn
//! that asks for no answer ([`crate::session_fact`] has how that is sent
//! and what was tried). The character file says what each means for who is
//! being spoken to; here only the room is described.
//!
//! Off unless the service's environment says `on`. Not yet heard on a lamp.
use std::{collections::VecDeque, sync::Mutex};

/// Where the coordinator reads whether the room is told. `on` only.
pub const ENVIRONMENT: &str = "LAMP_ROOM_FACTS";
/// The provider worker's flag for it.
pub const FLAG: &str = "--room-facts";
pub const QUIET: &str = "(For you to know, not to answer: the room around you has gone quiet.)";
pub const BUSY: &str = "(For you to know, not to answer: the office around you is busy, with people talking to each other.)";

/// Nothing heard for this long and the room is quiet.
const QUIET_AFTER_US: u64 = 300_000_000;
/// This many things in a row that the lamp said nothing to, inside this
/// long, and the room is busy.
const BUSY_RUN: usize = 3;
const BUSY_WITHIN_US: u64 = 60_000_000;
/// The room is told only when nothing has been heard or said for this
/// long: a fact written into a request would be taken for part of it.
const SETTLED_US: u64 = 2_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Room {
    Unknown,
    Quiet,
    Busy,
}

impl Room {
    fn fact(self) -> Option<&'static str> {
        match self {
            Self::Unknown => None,
            Self::Quiet => Some(QUIET),
            Self::Busy => Some(BUSY),
        }
    }
}

pub struct RoomTold {
    /// When watching began, or when something was last heard.
    quiet_since_us: Option<u64>,
    /// When anything was last heard or said.
    last_sound_us: u64,
    /// The request in hand, while the lamp has said nothing in it.
    open: Option<u64>,
    /// When each of the latest things the lamp said nothing to ended.
    unanswered: VecDeque<u64>,
    room: Room,
    /// The session last told, and what it was told.
    told: Option<(u64, Room)>,
}

/// This process's one room. The provider worker is one process.
pub static ROOM: Mutex<RoomTold> = Mutex::new(RoomTold::new());

impl RoomTold {
    pub const fn new() -> Self {
        Self {
            quiet_since_us: None,
            last_sound_us: 0,
            open: None,
            unanswered: VecDeque::new(),
            room: Room::Unknown,
            told: None,
        }
    }

    /// A request began: something was heard.
    pub fn heard(&mut self, request: u64, now_us: u64) {
        // The one before it, replaced by more talk before the lamp spoke.
        if let Some(previous) = self.open.filter(|previous| *previous != request) {
            self.unanswered(previous, now_us);
        }
        self.open = Some(request);
        self.quiet_since_us = Some(now_us);
        self.last_sound_us = now_us;
    }

    /// The lamp is speaking in `request`.
    pub fn spoke(&mut self, request: u64, now_us: u64) {
        if self.open == Some(request) {
            self.open = None;
        }
        self.unanswered.clear();
        self.quiet_since_us = Some(now_us);
        self.last_sound_us = now_us;
    }

    /// `request` ended with the lamp saying nothing.
    pub fn unanswered(&mut self, request: u64, now_us: u64) {
        if self.open == Some(request) {
            self.open = None;
        }
        if self.unanswered.len() == BUSY_RUN {
            self.unanswered.pop_front();
        }
        self.unanswered.push_back(now_us);
    }

    /// How the room is now.
    pub fn at(&mut self, now_us: u64) -> Room {
        let since = *self.quiet_since_us.get_or_insert(now_us);
        if self.unanswered.len() == BUSY_RUN
            && self
                .unanswered
                .front()
                .zip(self.unanswered.back())
                .is_some_and(|(first, last)| last.saturating_sub(*first) <= BUSY_WITHIN_US)
        {
            self.room = Room::Busy;
            // The run has been counted; the next three decide afresh.
            self.unanswered.clear();
        } else if now_us.saturating_sub(since) >= QUIET_AFTER_US {
            self.room = Room::Quiet;
            self.unanswered.clear();
        }
        self.room
    }

    /// What `session` should now be told, if anything: how the room is,
    /// when it is known, differs from what this session was told, and
    /// nothing has been heard or said for a moment.
    pub fn due(&mut self, session: u64, now_us: u64) -> Option<&'static str> {
        let room = self.at(now_us);
        if self.told == Some((session, room))
            || self.open.is_some()
            || now_us.saturating_sub(self.last_sound_us) < SETTLED_US
        {
            return None;
        }
        room.fact()
    }

    /// `session` has been told how the room is now.
    pub fn told(&mut self, session: u64) {
        self.told = Some((session, self.room));
    }
}

impl Default for RoomTold {
    fn default() -> Self {
        Self::new()
    }
}

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

/// Take a leading `--room-facts` off a worker's flags.
pub fn take(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == FLAG => (true, rest),
        _ => (false, flags),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1_000_000;

    #[test]
    fn five_minutes_of_nothing_is_quiet_and_three_unanswered_in_a_minute_is_busy() {
        let mut room = RoomTold::new();
        // Just started: not known, nothing to tell.
        assert_eq!(room.at(10 * S), Room::Unknown);
        assert_eq!(room.due(1, 20 * S), None);
        // Five minutes on with nothing heard: quiet, and the session is told once.
        assert_eq!(room.due(1, 311 * S), Some(QUIET));
        room.told(1);
        assert_eq!(room.due(1, 320 * S), None);
        // Someone speaks into the quiet and is answered: still quiet.
        room.heard(1, 400 * S);
        assert_eq!(room.due(1, 401 * S), None);
        room.spoke(1, 402 * S);
        assert_eq!(room.at(410 * S), Room::Quiet);
        // Colleagues start talking: three things the lamp says nothing to.
        room.heard(2, 500 * S);
        room.unanswered(2, 505 * S);
        room.heard(3, 510 * S);
        // The fourth replaces the third before any answer: that counts.
        room.heard(4, 513 * S);
        assert_eq!(room.at(514 * S), Room::Quiet);
        room.unanswered(4, 518 * S);
        assert_eq!(room.at(519 * S), Room::Busy);
        assert_eq!(room.due(1, 519 * S), Some(BUSY));
        room.told(1);
        assert_eq!(room.due(1, 530 * S), None);
        // A new session is told how the room is.
        assert_eq!(room.due(2, 531 * S), Some(BUSY));
        room.told(2);
        // Called and answered in a busy office: it is still busy. A third
        // session opening just then is told once the lamp has finished.
        room.heard(5, 540 * S);
        assert_eq!(room.due(3, 540 * S + S / 2), None);
        room.spoke(5, 541 * S);
        assert_eq!(room.due(3, 542 * S), None);
        assert_eq!(room.due(3, 544 * S), Some(BUSY));
        assert_eq!(room.at(600 * S), Room::Busy);
        // Everyone leaves. Five minutes after the last thing heard: quiet.
        assert_eq!(room.at(541 * S + 299 * S), Room::Busy);
        assert_eq!(room.due(2, 541 * S + 301 * S), Some(QUIET));
    }

    #[test]
    fn three_unanswered_spread_over_an_afternoon_are_not_a_busy_room() {
        let mut room = RoomTold::new();
        assert_eq!(room.at(S), Room::Unknown);
        assert_eq!(room.at(302 * S), Room::Quiet);
        for (request, at) in [(1, 400), (2, 500), (3, 600)] {
            room.heard(request, at * S);
            room.unanswered(request, (at + 4) * S);
        }
        assert_eq!(room.at(610 * S), Room::Quiet);
        // Nothing is told while a request is open.
        room.told(1);
        room.heard(4, 700 * S);
        room.unanswered(4, 701 * S);
        room.heard(5, 702 * S);
        room.unanswered(5, 703 * S);
        room.heard(6, 704 * S);
        assert_eq!(room.due(1, 720 * S), None);
        room.unanswered(6, 721 * S);
        assert_eq!(room.due(1, 724 * S), Some(BUSY));
    }

    #[test]
    fn the_facts_describe_the_room_and_ask_for_nothing() {
        for fact in [QUIET, BUSY] {
            let lower = fact.to_lowercase();
            for word in [
                "silen",
                "say nothing",
                "do not",
                "don't",
                "never",
                "answer only",
            ] {
                assert!(!lower.contains(word), "{word}");
            }
            assert!(lamp_gemini::wire::encode_context(fact).is_ok());
        }
        assert_eq!(arguments(Some("on")), [FLAG]);
        assert!(arguments(Some("1")).is_empty() && arguments(None).is_empty());
        let flags = [FLAG.to_owned(), "x".to_owned()];
        assert_eq!(take(&flags), (true, &flags[1..]));
        assert_eq!(take(&flags[1..]), (false, &flags[1..]));
    }
}
