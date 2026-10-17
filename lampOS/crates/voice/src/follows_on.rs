//! A line begun soon after the lamp's own words finished may follow on
//! from them (`LAMP_FOLLOWS_ON=on`, #829, the first builder at 09:01 on
//! 17 October 2026). Off unless set.
//!
//! Why: on lamp-4ace at 08:58, in a quiet room with one person, "Hey Lamp,
//! what is your favourite season?" was answered, and "And what do you like
//! about it?", said 12 s after the answer, was heard right, sent to the
//! model, and left unanswered. The character file gives the model a line
//! for the lamp only when it is called, or when "you said something to
//! them a moment ago, and this follows on from it"; nothing told it how
//! long ago the lamp spoke, and at 12 s it kept quiet. A follow-up said
//! about 3 s after an answer was answered.
//!
//! With the switch, a line that begins within [`WITHIN_US`] of the lamp's
//! own words finishing playing, and not over them, is sent with
//! [`NOTE`] beside it, unless the room is busy. The note says only what the
//! lamp did, in the file's own words: whether the line follows on is still
//! the model's to judge, so talk that doesn't stays unanswered.

/// Where the coordinator reads the switch. `on` only.
pub const ENVIRONMENT: &str = "LAMP_FOLLOWS_ON";

/// How soon after the lamp's words finished playing a line may follow on
/// from them: the first builder's "at least 20 s" (#829, 09:01). The
/// conversation itself is going on for 45 s from the answer's start
/// (`crate::thread`).
pub const WITHIN_US: u64 = 20_000_000;

/// What the model is told beside such a line.
pub const NOTE: &str = "(For you to know, not to answer: you said something a moment ago, so what you hear now may follow on from it.)";

/// Whether the switch is on in this process's environment.
pub fn on() -> bool {
    on_for(std::env::var(ENVIRONMENT).ok().as_deref())
}

fn on_for(value: Option<&str>) -> bool {
    value == Some("on")
}

/// Whether a line whose speech began at `began_us` may follow on from the
/// lamp's words, which last finished playing at `ended_us`: with the
/// switch `on`, not said over the lamp, and begun within [`WITHIN_US`]. A
/// line begun a moment before the words finished counts as begun at once.
pub fn due(on: bool, over_the_lamp: bool, ended_us: Option<u64>, began_us: u64) -> bool {
    on && !over_the_lamp
        && ended_us.is_some_and(|ended| began_us.saturating_sub(ended) <= WITHIN_US)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first builder's scene of 08:58 on lamp-4ace: the follow-up 12 s
    /// after the answer finished, and one at 19 s, follow on; one at 21 s
    /// doesn't. Nor does a line said over the lamp, one with the lamp never
    /// having spoken, or any without the switch.
    #[test]
    fn a_line_begun_within_twenty_seconds_of_the_lamps_words_may_follow_on() {
        const S: u64 = 1_000_000;
        let ended = Some(100 * S);
        for (began, follows) in [
            (112 * S, true),
            (119 * S, true),
            (120 * S, true),
            (120 * S + 1, false),
            (121 * S, false),
            (100 * S - 200_000, true),
        ] {
            assert_eq!(due(true, false, ended, began), follows, "{began}");
        }
        assert!(!due(true, true, ended, 112 * S));
        assert!(!due(true, false, None, 112 * S));
        assert!(!due(false, false, ended, 112 * S));
    }

    #[test]
    fn the_switch_is_on_only() {
        assert!(on_for(Some("on")));
        for value in [None, Some(""), Some("off"), Some("ON"), Some("yes")] {
            assert!(!on_for(value), "{value:?}");
        }
    }
}
