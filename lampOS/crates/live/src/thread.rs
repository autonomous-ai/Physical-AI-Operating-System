//! The thread of a conversation, kept so that a new provider session can be
//! told how the last one went.
//!
//! A session with the provider does not last: it announces its own close
//! (on lamp-4ace anywhere from 3 to 39 minutes after it opened), and a lost
//! connection is replaced. With resumption off the new session knows nothing
//! of the old one, and a person in the middle of a conversation finds the
//! lamp has forgotten it. This keeps the last few exchanges, as the
//! provider's own transcripts gave them, in this process's memory only, and
//! [`Recalling`] adds them to the instruction of the next session it opens.
//!
//! The note says nothing of sessions, connections or memory, and does not
//! tell the lamp to keep quiet. The first one did both ("your connection was
//! just renewed ... do not mention the renewal ... if nobody speaks to you,
//! say nothing"). The lamp opened its next two answers with "Sorry about
//! that, my connection dropped!", and then answered sixteen times with a
//! spoken "...", four of them to questions put to it.
//!
//! Only exchanges the lamp took part in are kept: what a person said just
//! before an answer, and the answer. Talk around the lamp that it did not
//! answer is dropped when the next thing is heard. Nothing here is written
//! anywhere; a restart of the service forgets it.
use lamp_gemini::{Attempt, Connect, Dialer, ResumptionHandle, SessionConfig, SessionId};
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Exchanges kept, and the most of each side of one that is kept.
const EXCHANGES: usize = 8;
const PERSON_BYTES: usize = 300;
const LAMP_BYTES: usize = 400;
/// What a person said counts as the question to an answer that begins within
/// this long of it.
const ASKED_WITHIN: Duration = Duration::from_secs(20);
/// An exchange older than this is not told to a new session: whoever it was
/// with has probably gone.
const REMEMBERED_FOR: Duration = Duration::from_secs(600);

struct Exchange {
    request: u64,
    at: Instant,
    person: String,
    lamp: String,
}

pub struct Thread {
    exchanges: VecDeque<Exchange>,
    /// What has been heard since the last answer, and when.
    heard: String,
    heard_at: Option<Instant>,
    heard_done: bool,
}

/// This process's one thread. The provider worker is one process with one
/// session supervisor.
pub static THREAD: Mutex<Thread> = Mutex::new(Thread::new());

/// Text of at most `most` bytes, with every run of whitespace or control
/// characters one space.
fn add(to: &mut String, text: &str, most: usize) {
    for c in text.chars() {
        let c = if c.is_control() || c.is_whitespace() {
            ' '
        } else {
            c
        };
        if to.len() + c.len_utf8() > most || (c == ' ' && to.ends_with(' ')) {
            continue;
        }
        to.push(c);
    }
}

impl Thread {
    pub const fn new() -> Self {
        Self {
            exchanges: VecDeque::new(),
            heard: String::new(),
            heard_at: None,
            heard_done: false,
        }
    }

    /// A piece of what the provider heard a person say.
    pub fn person(&mut self, text: &str, finished: bool, now: Instant) {
        // Something new after something that was never answered: the old
        // was not said to the lamp.
        let stale = self
            .heard_at
            .is_some_and(|at| now.saturating_duration_since(at) > ASKED_WITHIN);
        if self.heard_done || stale {
            self.heard.clear();
            self.heard_done = false;
        }
        add(&mut self.heard, text, PERSON_BYTES);
        self.heard_at = Some(now);
        self.heard_done = finished;
    }

    /// A piece of what the lamp said in answer `request`.
    pub fn lamp(&mut self, request: u64, text: &str, now: Instant) {
        if self.exchanges.back().map(|last| last.request) != Some(request) {
            let asked = self
                .heard_at
                .is_some_and(|at| now.saturating_duration_since(at) <= ASKED_WITHIN);
            let person = if asked {
                std::mem::take(&mut self.heard)
            } else {
                String::new()
            };
            self.heard.clear();
            self.heard_at = None;
            self.heard_done = false;
            if self.exchanges.len() == EXCHANGES {
                self.exchanges.pop_front();
            }
            self.exchanges.push_back(Exchange {
                request,
                at: now,
                person,
                lamp: String::new(),
            });
        }
        if let Some(last) = self.exchanges.back_mut() {
            add(&mut last.lamp, text, LAMP_BYTES);
        }
    }

    /// What to tell a new session, if anything was said lately, in at most
    /// `room` bytes: the latest exchanges that fit, in the order they went.
    pub fn recall(&self, now: Instant, room: usize) -> Option<String> {
        let mut kept: Vec<String> = Vec::new();
        let mut bytes = NOTE.len();
        for exchange in self.exchanges.iter().rev() {
            // An answer with no word in it ("...") is the model keeping
            // quiet aloud. Told back to it, it is what it does next.
            if now.saturating_duration_since(exchange.at) > REMEMBERED_FOR
                || !exchange.lamp.chars().any(char::is_alphanumeric)
            {
                continue;
            }
            let mut lines = String::new();
            if !exchange.person.trim().is_empty() {
                lines.push_str("Person: ");
                lines.push_str(exchange.person.trim());
                lines.push('\n');
            }
            lines.push_str("You: ");
            lines.push_str(exchange.lamp.trim());
            lines.push('\n');
            if bytes + lines.len() > room {
                break;
            }
            bytes += lines.len();
            kept.push(lines);
        }
        (!kept.is_empty()).then(|| {
            kept.reverse();
            format!("{NOTE}{}", kept.concat())
        })
    }
}

/// What goes before the exchanges. See the top of this file for what it
/// must not say.
const NOTE: &str = "\n\nEarlier in this conversation\nThis is what you and the person at your desk said to each other in the last few minutes, for you to carry on from the way anyone carries on a conversation they are in. It is background only: never read it out and never answer any of it again.\n";

impl Default for Thread {
    fn default() -> Self {
        Self::new()
    }
}

/// Opens each session with the thread so far added to its instruction.
pub struct Recalling {
    config: SessionConfig,
}

impl Recalling {
    pub fn new(config: SessionConfig) -> Self {
        Self { config }
    }

    /// The configuration for a session opened now, told the time and this
    /// thread.
    fn recalled(&self, thread: &Thread, now: Instant, time: Option<&str>) -> SessionConfig {
        let written = self.config.instruction_text();
        let time = time.unwrap_or_default();
        let told = |more: &str| {
            if more.is_empty() {
                return Some(self.config.clone());
            }
            self.config
                .clone()
                .instruction(&format!("{written}{more}"))
                .ok()
        };
        // The thread takes the room the file and the time leave it; a time
        // too long to add leaves the session as it always was.
        let room = lamp_gemini::MAX_TEXT_BYTES.saturating_sub(written.len() + time.len());
        thread
            .recall(now, room)
            .and_then(|thread| told(&format!("{time}{thread}")))
            .or_else(|| told(time))
            .unwrap_or_else(|| self.config.clone())
    }
}

impl Connect for Recalling {
    fn connect(&mut self, session: SessionId, resume: Option<ResumptionHandle>) -> Attempt {
        let time = crate::local_time::told_here();
        let config = match THREAD.lock() {
            Ok(thread) => self.recalled(&thread, Instant::now(), time.as_deref()),
            Err(_) => self.config.clone(),
        };
        Dialer::new(config).connect(session, resume)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM: usize = 8_192;

    #[test]
    fn it_keeps_what_was_asked_and_answered_and_not_the_talk_around_it() {
        let mut thread = Thread::new();
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        assert_eq!(thread.recall(at(0), ROOM), None);
        // Two people talking to each other: heard, never answered.
        thread.person("Did you send the report", false, at(1));
        thread.person(" to Maria?", true, at(2));
        thread.person("I did, but she has not replied.", true, at(5));
        assert_eq!(thread.recall(at(6), ROOM), None);
        // Then the lamp is asked something, in pieces, and answers in pieces.
        thread.person("Hey Lamp, my favourite", false, at(10));
        thread.person("  colour is\ngreen.", true, at(11));
        thread.lamp(7, "Green is a ", at(12));
        thread.lamp(7, "lovely colour!", at(13));
        // An answer with nothing heard before it: the lamp's line alone.
        thread.lamp(8, "Still here if you need me.", at(60));
        let recalled = thread.recall(at(61), ROOM).unwrap();
        assert!(recalled.ends_with(
            "Person: Hey Lamp, my favourite colour is green.\nYou: Green is a lovely colour!\nYou: Still here if you need me.\n"
        ), "{recalled}");
        assert!(!recalled.contains("Maria") && !recalled.contains("replied"));
        // Nothing in it for the lamp to talk about but the conversation.
        let note = recalled.split("Person: ").next().unwrap().to_lowercase();
        for word in [
            "connect", "renew", "session", "memory", "forg", "lost", "sorry", "silen", "quiet",
            "nothing",
        ] {
            assert!(!note.contains(word), "{word}: {note}");
        }
        // Keeping quiet aloud is not an answer to carry on from.
        thread.person("Are you coming to lunch with us?", true, at(70));
        thread.lamp(9, "...", at(71));
        thread.person("Maybe.", true, at(72));
        thread.lamp(10, " \u{2026} ", at(73));
        let recalled = thread.recall(at(74), ROOM).unwrap();
        assert!(
            recalled.ends_with("You: Still here if you need me.\n"),
            "{recalled}"
        );
        assert!(!recalled.contains("lunch") && !recalled.contains("Maybe"));
    }

    #[test]
    fn it_forgets_what_is_old_and_holds_only_so_much() {
        let mut thread = Thread::new();
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        // Something said long before an answer is not its question.
        thread.person("What time is it?", true, at(0));
        thread.lamp(1, "It is a fine day.", at(30));
        assert_eq!(
            thread.recall(at(31), ROOM).unwrap().lines().last(),
            Some("You: It is a fine day.")
        );
        assert!(!thread.recall(at(31), ROOM).unwrap().contains("What time"));
        // Ten minutes on, nothing of it is told.
        assert_eq!(thread.recall(at(30 + 601), ROOM), None);
        // Only the last eight exchanges, each side cut to its length.
        for request in 10..22u64 {
            thread.person(&"word ".repeat(200), true, at(700 + request));
            thread.lamp(request, &"answer ".repeat(200), at(701 + request));
        }
        let recalled = thread.recall(at(730), ROOM).unwrap();
        assert_eq!(recalled.matches("Person: ").count(), 8);
        assert_eq!(recalled.matches("You: ").count(), 8);
        assert!(recalled.len() < 8 * (PERSON_BYTES + LAMP_BYTES + 20) + 400);
        assert!(!recalled.contains('\0') && !recalled.contains("  "));
        // With less room, the latest that fit, still in order; with none, nothing.
        let whole = recalled.len();
        let fewer = thread.recall(at(730), whole - 1).unwrap();
        assert_eq!(fewer.matches("You: ").count(), 7);
        assert!(recalled.ends_with(&fewer[NOTE.len()..]));
        assert_eq!(thread.recall(at(730), NOTE.len() + 10), None);
    }

    #[test]
    fn a_new_session_is_told_the_thread_and_an_empty_one_changes_nothing() {
        use lamp_gemini::{Credential, GOOGLE_ENDPOINT};
        let config = SessionConfig::new(GOOGLE_ENDPOINT, Credential::api_key("test-only").unwrap())
            .unwrap()
            .instruction("You are a lamp.")
            .unwrap();
        let recalling = Recalling::new(config);
        let mut thread = Thread::new();
        let now = Instant::now();
        assert_eq!(
            recalling.recalled(&thread, now, None).instruction_text(),
            "You are a lamp."
        );
        // The time alone, and before the thread when there is one.
        assert_eq!(
            recalling
                .recalled(&thread, now, Some("\n\nThe time\nNoon."))
                .instruction_text(),
            "You are a lamp.\n\nThe time\nNoon."
        );
        thread.person("My name is Sam.", true, now);
        thread.lamp(3, "Nice to meet you, Sam.", now);
        let both = recalling.recalled(&thread, now, Some("\n\nThe time\nNoon."));
        assert!(
            both.instruction_text().starts_with(
                "You are a lamp.\n\nThe time\nNoon.\n\nEarlier in this conversation\n"
            )
        );
        // A thread with no room left leaves the time; a time too long, nothing.
        let long = "x".repeat(lamp_gemini::MAX_TEXT_BYTES - 200);
        let full = Recalling::new(recalling.config.clone().instruction(&long).unwrap());
        assert_eq!(
            full.recalled(&thread, now, Some("\n\nThe time\nNoon."))
                .instruction_text(),
            format!("{long}\n\nThe time\nNoon.")
        );
        assert_eq!(
            full.recalled(&thread, now, Some(&"y".repeat(300)))
                .instruction_text(),
            long
        );
        let told = recalling.recalled(&thread, now, None);
        assert!(
            told.instruction_text()
                .starts_with("You are a lamp.\n\nEarlier in this conversation\n")
        );
        assert!(
            told.instruction_text()
                .ends_with("Person: My name is Sam.\nYou: Nice to meet you, Sam.\n")
        );
    }
}
