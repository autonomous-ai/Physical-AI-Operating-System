//! Telling the model it was called, while the person is still speaking
//! (`docs/attention.md`).
//!
//! The lamp hears its name in the room's sound (`docs/name.md`); the model
//! mostly does not, and so it sometimes takes a called question for other
//! people's talk. A second look (`crate::second_look`) mends that after
//! the fact, seconds late. This is the same sign given in time: when the
//! name is heard plainly as a request begins, a short written note goes to
//! the service inside that request, beside the sound, and the model has it
//! in hand when it decides whether to answer.
//!
//! The note is for the model alone and says so. It is sent once for a
//! request, only while that request's sound is still being sent, and never
//! on the strength of a faint hearing.

/// `on` for the note to be sent. Anything else: never.
pub const ENVIRONMENT: &str = "LAMP_NAME_NOTE";

/// What the model is given beside what it hears. It opens as the lines
/// about the room do, which the character file tells it nobody said.
pub const NOTE: &str = "(For you to know, not to answer: your own ears caught your name, Lamp, just now. If what is being said is something a lamp on a desk could answer or chat about, it is said to you. Other people's own business still is not yours.)";

/// The least a hearing scores to be passed on (`crate::second_look`).
pub const LEAST_SCORE: f32 = crate::second_look::LEAST_SCORE;
/// The name counts for a request if it began no longer than this before
/// the request's sound began to be sent.
pub const BEFORE_US: u64 = crate::attention::NAME_BEFORE_US;

/// The provider worker's flag for it: the worker is started with a clean
/// environment and is told everything by its arguments.
pub const FLAG: &str = "--name-note";

/// The provider worker's argument for that switch in the environment.
pub fn arguments_from_environment() -> Vec<String> {
    if std::env::var(ENVIRONMENT).ok().as_deref() == Some("on") {
        vec![FLAG.to_owned()]
    } else {
        Vec::new()
    }
}

/// Whether the environment switches it on, for the coordinator, which
/// then takes no second look at a request the model was told in.
pub fn on() -> bool {
    std::env::var(ENVIRONMENT).ok().as_deref() == Some("on")
}

/// Take a leading `--name-note` off a worker's flags.
pub fn take(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == FLAG => (true, rest),
        _ => (false, flags),
    }
}

#[derive(Debug, Default)]
pub struct Called {
    /// When the newest plain hearing of the name began.
    heard_us: Option<u64>,
    /// The request whose sound is being sent, and since when.
    open: Option<(u64, u64)>,
    /// The newest request a note was offered for.
    told: Option<u64>,
}

impl Called {
    /// The name began to be heard at `began_us`, with this score.
    pub fn name_heard(&mut self, began_us: u64, score: f32) {
        if score >= LEAST_SCORE {
            self.heard_us = Some(
                self.heard_us
                    .map_or(began_us, |before| before.max(began_us)),
            );
        }
    }

    /// `open`: the request whose sound is being sent now, if there is one.
    /// That request, if the model is to be told now that the lamp's name
    /// was heard: once for a request, and only for a hearing that began as
    /// the request did or since.
    pub fn due(&mut self, open: Option<u64>, now_us: u64) -> Option<u64> {
        let Some(request) = open else {
            self.open = None;
            return None;
        };
        let since = match self.open {
            Some((current, since)) if current == request => since,
            _ => {
                self.open = Some((request, now_us));
                now_us
            }
        };
        if self.told == Some(request) {
            return None;
        }
        let heard = self.heard_us?;
        (heard.saturating_add(BEFORE_US) >= since).then_some(request)
    }

    /// A note was offered for `request`, whether or not it could be sent.
    pub fn told(&mut self, request: u64) {
        self.told = Some(request);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: u64 = 1_000_000;

    #[test]
    fn a_request_that_begins_with_the_name_is_told_of_it_once() {
        let mut called = Called::default();
        // Nothing open: nothing to tell, whatever was heard.
        called.name_heard(10 * SECOND - 300_000, 1.0);
        assert_eq!(called.due(None, 10 * SECOND - 100_000), None);
        // The request opens a moment after the name began.
        assert_eq!(called.due(Some(4), 10 * SECOND), Some(4));
        called.told(4);
        for tick in 1..50 {
            assert_eq!(called.due(Some(4), 10 * SECOND + tick * 20_000), None);
        }
        // The next request has no name in it.
        assert_eq!(called.due(None, 14 * SECOND), None);
        assert_eq!(called.due(Some(5), 20 * SECOND), None);
        assert_eq!(called.due(Some(5), 22 * SECOND), None);
    }

    #[test]
    fn the_name_heard_in_the_middle_of_a_request_is_told_then() {
        let mut called = Called::default();
        assert_eq!(called.due(Some(1), 5 * SECOND), None);
        // "So I was thinking, Lamp, ...": the hearing comes a second in.
        called.name_heard(6 * SECOND, 0.995);
        assert_eq!(called.due(Some(1), 6 * SECOND + 400_000), Some(1));
    }

    #[test]
    fn a_faint_hearing_or_an_old_one_tells_nothing() {
        let mut called = Called::default();
        called.name_heard(10 * SECOND, 0.85);
        assert_eq!(called.due(Some(1), 10 * SECOND + 200_000), None);
        // Heard plainly, but long before this request began.
        let mut called = Called::default();
        called.name_heard(10 * SECOND, 1.0);
        assert_eq!(called.due(Some(2), 10 * SECOND + BEFORE_US + 1), None);
        // And it stays untold however long that request goes on.
        assert_eq!(called.due(Some(2), 30 * SECOND), None);
        // To the limit, it counts.
        let mut called = Called::default();
        called.name_heard(10 * SECOND, 1.0);
        assert_eq!(called.due(Some(3), 10 * SECOND + BEFORE_US), Some(3));
    }

    #[test]
    fn only_its_own_flag_at_the_front_turns_it_on() {
        let flags = |list: &[&str]| list.iter().map(|flag| flag.to_string()).collect::<Vec<_>>();
        let given = flags(&[FLAG, "--bus-root", "/run/lampos-bus"]);
        let (on, rest) = take(&given);
        assert!(on);
        assert_eq!(rest, &given[1..]);
        let given = flags(&["--bus-root", "/run/lampos-bus", FLAG]);
        let (on, rest) = take(&given);
        assert!(!on);
        assert_eq!(rest, &given[..]);
        assert!(!take(&[]).0);
    }

    /// A note that could not be sent is not offered again and again.
    #[test]
    fn a_request_is_offered_its_note_once_whether_or_not_it_went() {
        let mut called = Called::default();
        called.name_heard(10 * SECOND, 1.0);
        assert_eq!(called.due(Some(7), 10 * SECOND + 100_000), Some(7));
        called.told(7);
        called.name_heard(11 * SECOND, 1.0);
        assert_eq!(called.due(Some(7), 11 * SECOND + 100_000), None);
    }
}
