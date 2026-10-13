//! What a new session is told about the moment it opens in.
//!
//! A session that has just opened has heard nothing, and takes the first
//! thing it hears for the start of a conversation with it. On lamp-4ace on
//! 2026-10-13, in an office that was talking, six sessions opened with
//! nothing told to them beyond the character and the time; two of them
//! answered the first sentence they heard, a colleague's to a colleague
//! ("Está bien, gracias." got "You're welcome!"). A lamp that has sat
//! through an hour of such talk without answering does not do that: what it
//! has already left alone is its guide.
//!
//! So a new session can be told how things stand, in one written turn that
//! asks for no answer (`InputSender::try_context` in `lamp-gemini`), unless
//! the lamp was talking with someone in the last moments
//! (`Thread::going_on`): then the thread's own note says so. The fact
//! describes; it does not tell the lamp to keep quiet. Telling it that made
//! it answer aloud with "..." ([`crate::thread`]).
//!
//! Off unless the service's environment says `on`. As silent probes on the
//! lamp, one run each, a session told this still answered a question put
//! to it by name, three times in three. What it does to the first thing a
//! session hears has not been measured.

/// Where the coordinator reads whether new sessions are told. `on` only.
pub const ENVIRONMENT: &str = "LAMP_SESSION_FACT";
/// The provider worker's flag for it.
pub const FLAG: &str = "--session-fact";
/// What is told.
pub const FACT: &str = "(For you to know, not to answer: you have been at your desk for a while, and nobody is talking to you at the moment.)";

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

/// Take a leading `--session-fact` off a worker's flags.
pub fn take(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == FLAG => (true, rest),
        _ => (false, flags),
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
        let flags = [FLAG.to_owned(), "--camera".to_owned()];
        assert_eq!(take(&flags), (true, &flags[1..]));
        assert_eq!(take(&flags[1..]), (false, &flags[1..]));
        assert_eq!(take(&[]), (false, &[][..]));
    }

    #[test]
    fn the_fact_describes_and_does_not_ask_for_quiet() {
        let fact = FACT.to_lowercase();
        for word in ["quiet", "silen", "say nothing", "do not", "don't", "never"] {
            assert!(!fact.contains(word), "{word}");
        }
        assert!(lamp_gemini::wire::encode_context(FACT).is_ok());
    }
}
