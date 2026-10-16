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
//! after its audio and just before its end (`Session::try_note`; the
//! newest note for a request is the one sent), and the model answers only
//! after the end. So a note given while the request's input is still open
//! always reaches the model first. Words the service writes only after the
//! input was closed come too late: nothing is sent then. The journal says
//! which each was, "in time" or "after its input was closed", for counting
//! how often each is on a lamp (the first builder on #1173).
//!
//! All the request's words so far are matched each time a piece comes, so
//! a name said later in it ("ask Claude Code and Codex …") changes the
//! note before it is sent. It says only which of the staff were named,
//! never what was said.

/// `on` for the note to be sent. Anything else: never.
pub const ENVIRONMENT: &str = "LAMP_STAFF_NOTE";
/// The provider worker's flag for it, after `--policy-holds-actions`.
pub const FLAG: &str = "--staff-note";

/// What the model is given beside what it hears, for the staff named
/// (`who`, as the person calls them, in the order named; never empty).
pub fn note(who: &[&str]) -> String {
    match who {
        [one] => format!(
            "(For you to know, not to answer: the person named {one}, one of the staff. If this \
             is work for {one}, hand it over with start_agent_task; never do it yourself.)"
        ),
        [all @ .., last] => format!(
            "(For you to know, not to answer: the person named {} and {last}, of the staff. If \
             this is work for them, hand it over with start_agent_task; never do it yourself.)",
            all.join(", ")
        ),
        [] => String::new(),
    }
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

/// The words heard for the request whose input began last, the note it
/// was given, and whether its words came too late.
#[derive(Debug, Default)]
pub struct Heard {
    request: Option<u64>,
    words: String,
    told: Option<String>,
    late: bool,
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
    /// all its words so far, unless its words came too late or no request
    /// began.
    pub fn piece(&mut self, text: &str) -> Option<(u64, &str)> {
        let request = self.request.filter(|_| !self.late)?;
        self.words.push_str(text);
        Some((request, &self.words))
    }

    /// The note the request was last given, if any.
    pub fn told(&self) -> Option<&str> {
        self.told.as_deref()
    }

    /// It was given `note`.
    pub fn tell(&mut self, note: String) {
        self.told = Some(note);
    }

    /// Its words named the staff after its input was closed: nothing more
    /// for it.
    pub fn too_late(&mut self) {
        self.late = true;
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
        let told = note(&["Codex"]);
        assert!(told.starts_with("(For you to know, not to answer"));
        assert!(told.contains("named Codex,") && told.contains("start_agent_task"));
        assert!(told.ends_with(".)"));
        let both = note(&["Claude Code", "Codex"]);
        assert!(
            both.contains("named Claude Code and Codex, of the staff"),
            "{both}"
        );
        assert!(
            note(&["Claude Code", "Codex", "OpenCode"])
                .contains("named Claude Code, Codex and OpenCode,")
        );
    }

    /// A request's pieces add up; a new request starts afresh; nothing is
    /// taken after its words came too late, nor before any request began.
    #[test]
    fn a_requests_words_add_up_until_they_come_too_late() {
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
        heard.tell(note(&["Codex"]));
        assert_eq!(heard.told(), Some(note(&["Codex"]).as_str()));
        heard.too_late();
        assert_eq!(heard.piece(" a haiku"), None);
        heard.began(8);
        assert_eq!(heard.told(), None);
        assert_eq!(heard.piece(" thanks"), Some((8, " thanks")));
    }
}
