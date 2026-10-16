//! Telling the model a request names one of the staff, while the request is
//! still being heard (`docs/tasks.md`, "Named in the words"), off unless
//! `LAMP_STAFF_NOTE=on`.
//!
//! On lamp-4ace on 2026-10-16 "Hey Lamp, ask Codex to write a haiku about
//! coffee." was heard right at 22:51:02, and the lamp said a haiku of its
//! own with no `start_agent_task` call; at about 21:05 "ask Claude Code to
//! write a short poem about the sea" got a poem of its own. The session's
//! note named both of the staff; the model did the work itself all the
//! same. This is a sign given in time, from the words the service heard:
//! when the words of the request whose input is open name one of the staff
//! ([`crate::agent_tasks::named_in_words`]), a short written note goes to
//! the service inside that request, beside its sound, as the note for the
//! lamp's name does ([`crate::called`]).
//!
//! **Before the answer:** a request's note is sent inside its activity,
//! after its audio and just before its end (`Session::try_note`), and the
//! model answers only after the end. So a note given while the request's
//! input is still open always reaches the model first. Words the service
//! writes only after the input was closed come too late: nothing is sent
//! then, and the journal says so, for counting how often that is.
//!
//! The note is sent once for a request. It says only which of the staff
//! was named, never what was said.

/// `on` for the note to be sent. Anything else: never.
pub const ENVIRONMENT: &str = "LAMP_STAFF_NOTE";
/// The provider worker's flag for it, after `--policy-holds-actions`.
pub const FLAG: &str = "--staff-note";

/// What the model is given beside what it hears, for the one of the staff
/// named (`who`, as the person calls them).
pub fn note(who: &str) -> String {
    format!(
        "(For you to know, not to answer: the person named {who}, one of the staff. If this is \
         work for {who}, hand it over with start_agent_task; never do it yourself.)"
    )
}

/// The provider worker's arguments for the switch in the environment.
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

/// Take a leading [`FLAG`] off a worker's flags.
pub fn take(flags: &[String]) -> (bool, &[String]) {
    match flags {
        [flag, rest @ ..] if flag == FLAG => (true, rest),
        _ => (false, flags),
    }
}

/// The words heard for the request whose input began last, and whether
/// its note was given.
#[derive(Debug, Default)]
pub struct Heard {
    request: Option<u64>,
    words: String,
    done: bool,
}

impl Heard {
    /// A request's input began: what is heard from now is its.
    pub fn began(&mut self, request: u64) {
        *self = Self {
            request: Some(request),
            ..Self::default()
        };
    }

    /// A piece of what the service heard. The request it belongs to and
    /// all its words so far, unless its note was given or it has none.
    pub fn piece(&mut self, text: &str) -> Option<(u64, &str)> {
        let request = self.request.filter(|_| !self.done)?;
        self.words.push_str(text);
        Some((request, &self.words))
    }

    /// Its note was given, or was due too late: nothing more for it.
    pub fn done(&mut self) {
        self.done = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_and_its_flag() {
        assert_eq!(arguments(Some("on")), vec![FLAG.to_owned()]);
        assert!(arguments(Some("yes")).is_empty());
        assert!(arguments(None).is_empty());
        let flags = [FLAG.to_owned(), "--camera".to_owned()];
        assert_eq!(take(&flags), (true, &flags[1..]));
        assert_eq!(take(&flags[1..]), (false, &flags[1..]));
    }

    /// The note opens as the lamp's other notes do, which the character
    /// file tells the model nobody said, and names the function.
    #[test]
    fn the_note_names_who_and_the_function() {
        let told = note("Codex");
        assert!(told.starts_with("(For you to know, not to answer"));
        assert!(told.contains("named Codex") && told.contains("start_agent_task"));
        assert!(told.ends_with(".)"));
    }

    /// A request's pieces add up; a new request starts afresh; nothing is
    /// given after its note, nor before any request began.
    #[test]
    fn a_requests_words_add_up_until_its_note_is_given() {
        let mut heard = Heard::default();
        assert_eq!(heard.piece("ask"), None);
        heard.began(7);
        assert_eq!(
            heard.piece(" Hey Lamp, ask Co"),
            Some((7, " Hey Lamp, ask Co"))
        );
        assert_eq!(
            heard.piece("dex to write"),
            Some((7, " Hey Lamp, ask Codex to write"))
        );
        heard.done();
        assert_eq!(heard.piece(" a haiku"), None);
        heard.began(8);
        assert_eq!(heard.piece(" thanks"), Some((8, " thanks")));
    }
}
