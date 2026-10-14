//! Sounds that are not something said (`docs/not-speech.md`).
//!
//! The detector takes any short loud sound for speech, and the model hears
//! words in it. The coordinator asks here, when an utterance ends, whether
//! it was long enough to have been said at all.

/// Fewer blocks of speech than this in a whole utterance and it was a sound,
/// not something said: a cough, a click, a bump. On lamp-4ace on 2026-10-13
/// the fourteen such sounds the lamp answered ("Sorry, I missed that", "I'm
/// Lamp. What's your name?") had 11 to 33 blocks, twelve of them under 25;
/// the four shortest things really said to it that morning had 34 to 41.
/// It was 25 until two sounds of 25 and 26 blocks, one after the other, got
/// "I'm here if you need anything."
pub const SCRAP_BLOCKS: usize = 30;
/// An utterance taken as said with fewer blocks of speech than this has how
/// its blocks were judged written into its record, as one thrown out has:
/// 0.8 s of speech, which is every one-word and two-word thing said.
pub const SHORT_BLOCKS: usize = 80;
/// A short word soon after the lamp's own is an answer to it ("Yes.", "No.")
/// and is never taken for a sound. It was ten seconds at first: on lamp-4ace
/// each sound the lamp answered then excused the next one, eight seconds on.
pub const AFTER_OWN_WORDS_US: u64 = 4_000_000;

/// Where the coordinator reads whether an utterance judged a sound still goes
/// to the model in a room that is not busy (`quiet-room`), for the model to
/// judge by its sound. Off unless set. Measured on lamp-4ace on 2026-10-14:
/// the gate threw out 13 of 36 short words said into quiet ("Hi.", "Yes.",
/// "Hello?") and no count of their blocks tells them from tonal sounds
/// (`docs/not-speech.md`).
pub const SHORT_ENVIRONMENT: &str = "LAMP_SHORT_SOUNDS";

/// Whether the environment lets a short utterance through in a quiet room.
pub fn short_sounds_from_environment() -> bool {
    std::env::var(SHORT_ENVIRONMENT).ok().as_deref() == Some("quiet-room")
}

/// Whether an utterance judged a sound is still sent on: with the switch,
/// in a room that is not busy. A room where the last three things heard went
/// unanswered is busy, so a run of sounds the model leaves alone closes the
/// door again by itself.
pub fn sent_anyway(switch: bool, busy: bool) -> bool {
    switch && !busy
}

/// Whether speech that begins over a turn in progress is excused from being
/// judged a sound. Over the lamp's own answer it is someone cutting in, and
/// over something that was itself said it is someone going on with it. Over
/// a sound it is neither.
pub fn takes_over(lamp_answering: bool, replaced_was_said: bool) -> bool {
    lamp_answering || replaced_was_said
}

/// Whether an utterance that has just ended, with this many blocks scored
/// as speech, was a sound. `since_own_words_us` is how long ago the lamp was
/// last heard, if it ever was.
pub fn was_a_sound(speech_blocks: usize, took_over: bool, since_own_words_us: Option<u64>) -> bool {
    speech_blocks < SCRAP_BLOCKS
        && !took_over
        && since_own_words_us.is_none_or(|since| since >= AFTER_OWN_WORDS_US)
}

/// Whether an answer opens with the name of a function said aloud, not
/// called: `not_for_me`, or the service's own way of writing a call
/// (`print(default_api.not_for_me())`). On lamp-4ace on 2026-10-13, with
/// seven functions declared, three answers in one test were the words
/// "not_for_me", played through the speaker to two people talking to each
/// other. A word with an underscore in it at the start of an answer is
/// never something a lamp says.
pub fn names_a_function(opening: &str) -> bool {
    let opening = opening.trim_start();
    if opening.starts_with("print(")
        || opening.starts_with("tool_code")
        || opening.starts_with("default_api")
    {
        return true;
    }
    let name: &str = opening
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
        .next()
        .unwrap_or_default();
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.contains('_')
        && name.split('_').all(|part| !part.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sound_goes_on_only_with_the_switch_and_only_in_a_room_that_is_not_busy() {
        assert!(sent_anyway(true, false));
        assert!(!sent_anyway(true, true));
        assert!(!sent_anyway(false, false));
        assert!(!sent_anyway(false, true));
    }

    #[test]
    fn a_function_named_aloud_is_told_from_something_said() {
        for named in [
            "not_for_me",
            " not_for_me.",
            "not_for_me()",
            "volume_down",
            "heard_my_name, yes",
            "print(default_api.not_for_me())",
            "default_api.recall(about='coffee')",
            "tool_code",
        ] {
            assert!(names_a_function(named), "{named}");
        }
        for said in [
            "Not for me.",
            "not for me",
            "Remember, I told you.",
            "recall",
            "Four. Easy one.",
            "...",
            "",
            "_",
            "a_",
            "_a",
            "Good_morning",
            "I can't do that_one",
        ] {
            assert!(!names_a_function(said), "{said}");
        }
    }

    const LONG_AGO: Option<u64> = Some(60_000_000);

    #[test]
    fn the_sounds_the_lamp_answered_are_sounds() {
        // 16:30 on 2026-10-13: two a second apart. The first is dropped, so
        // the second has no turn to take over.
        assert!(was_a_sound(25, takes_over(false, false), LONG_AGO));
        assert!(was_a_sound(26, takes_over(false, false), LONG_AGO));
        // Before the lamp has said anything at all.
        assert!(was_a_sound(11, false, None));
    }

    #[test]
    fn the_shortest_things_really_said_are_not() {
        for blocks in [34, 37, 41] {
            assert!(!was_a_sound(blocks, false, LONG_AGO));
        }
    }

    #[test]
    fn a_quick_word_over_the_lamp_or_after_it_is_not_a_sound() {
        // "Stop." over the lamp's answer.
        assert!(!was_a_sound(12, takes_over(true, false), Some(0)));
        // "Yes." two seconds after the lamp's question; the same word after
        // five seconds of nothing is on its own.
        assert!(!was_a_sound(15, false, Some(2_000_000)));
        assert!(was_a_sound(15, false, Some(5_000_000)));
    }

    #[test]
    fn the_end_of_a_sentence_is_excused_and_a_sound_after_a_sound_is_not() {
        // "What is the capital" ... "of France?": the second half is short
        // and replaces a turn whose speech was long enough to be said.
        assert!(!was_a_sound(18, takes_over(false, true), LONG_AGO));
        // A sound let through for being close to the lamp's words, then
        // another over it once that excuse has run out.
        assert!(was_a_sound(18, takes_over(false, false), Some(4_500_000)));
    }
}
