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
//! answer is dropped when the next thing is heard.
//!
//! By itself this lives in the process's memory and a restart of the
//! service forgets it. Given a file ([`Thread::save`], [`Thread::load`];
//! `--thread-file` to the provider worker) the same few exchanges are also
//! kept there, so that a service that ended and was started again a few
//! seconds later carries on the conversation it was in. On a lamp that file
//! belongs in the service's own runtime directory, which is memory and not
//! a disk, and holds nothing older than the ten minutes a thread is told
//! for.
use lamp_gemini::{Attempt, Connect, Dialer, ResumptionHandle, SessionConfig, SessionId};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
/// A conversation whose last exchange is this new is told as one that may
/// go on; an older one as one that is finished. Until 2026-10-13 every one
/// was told as "this conversation ... for you to carry on from the way
/// anyone carries on a conversation they are in". That afternoon lamp-4ace
/// had a character that answers only when called or when it is already
/// talking with someone, in an office where it was neither, and five new
/// sessions in a row, opened from one to seven minutes after its last
/// words, each answered the first or second thing it heard: colleagues'
/// talk ("por" got a sentence about the weather). Each of those answers was
/// then the newest exchange of the thread the next session was told, and
/// the last of the five answered nine times in two minutes
/// (`docs/conversation-thread.md`).
const GOING_ON_FOR: Duration = Duration::from_secs(45);

struct Exchange {
    request: u64,
    at: Instant,
    person: String,
    lamp: String,
    /// Handed on to the lamp's memory, or not for it to have.
    journalled: bool,
}

pub struct Thread {
    exchanges: VecDeque<Exchange>,
    /// What has been heard since the last answer, and when.
    heard: String,
    heard_at: Option<Instant>,
    heard_done: bool,
    /// An answer has grown since the thread was last saved.
    changed: bool,
    /// The answer that opened with a function's name, if the newest did.
    unsaid: Option<u64>,
}

/// One exchange as it is kept in the file: when, by the wall clock, since
/// the process that reads it back has another `Instant`.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    at_unix_ms: u64,
    person: String,
    lamp: String,
}

/// This process's one thread. The provider worker is one process with one
/// session supervisor.
pub static THREAD: Mutex<Thread> = Mutex::new(Thread::new());

/// [`Thread::unsay`] on this process's thread.
pub fn unsay(request: u64) {
    if let Ok(mut thread) = THREAD.lock() {
        thread.unsay(request);
    }
}

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
            changed: false,
            unsaid: None,
        }
    }

    /// The exchanges still worth telling, as the file's text, if anything
    /// has changed since the last time this was asked.
    pub fn save(&mut self, now: Instant, wall: SystemTime) -> Option<String> {
        if !std::mem::take(&mut self.changed) {
            return None;
        }
        let wall_ms = wall.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64;
        let kept: Vec<Kept> = self
            .exchanges
            .iter()
            .filter(|exchange| {
                now.saturating_duration_since(exchange.at) <= REMEMBERED_FOR
                    && exchange.lamp.chars().any(char::is_alphanumeric)
            })
            .map(|exchange| Kept {
                at_unix_ms: wall_ms
                    .saturating_sub(now.saturating_duration_since(exchange.at).as_millis() as u64),
                person: exchange.person.clone(),
                lamp: exchange.lamp.clone(),
            })
            .collect();
        serde_json::to_string(&kept).ok()
    }

    /// A thread from the file's text: what in it is from the last ten
    /// minutes, cut to the usual lengths. Text that is not such a file, or
    /// that claims to be from the future, gives an empty thread.
    pub fn load(text: &str, now: Instant, wall: SystemTime) -> Self {
        let mut thread = Self::new();
        let (Ok(kept), Ok(wall)) = (
            serde_json::from_str::<Vec<Kept>>(text),
            wall.duration_since(UNIX_EPOCH),
        ) else {
            return thread;
        };
        let wall_ms = wall.as_millis() as u64;
        let from = kept.len().saturating_sub(EXCHANGES);
        for (index, kept) in kept.into_iter().enumerate().skip(from) {
            let Some(age) = wall_ms
                .checked_sub(kept.at_unix_ms)
                .map(Duration::from_millis)
            else {
                continue;
            };
            let Some(at) = now.checked_sub(age).filter(|_| age <= REMEMBERED_FOR) else {
                continue;
            };
            let (mut person, mut lamp) = (String::new(), String::new());
            add(&mut person, &kept.person, PERSON_BYTES);
            add(&mut lamp, &kept.lamp, LAMP_BYTES);
            thread.exchanges.push_back(Exchange {
                // Never the number of a request this process will make.
                request: u64::MAX - index as u64,
                at,
                person,
                lamp,
                // The process that wrote the file handed it on.
                journalled: true,
            });
        }
        thread
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
        // The name of a function said where it should have been called is
        // not something the lamp said: neither that piece nor what follows
        // it in the same answer is kept, told again or taken for a
        // conversation going on.
        if self.unsaid == Some(request) {
            return;
        }
        if self.exchanges.back().map(|last| last.request) != Some(request) {
            if crate::not_speech::names_a_function(text) {
                self.unsaid = Some(request);
                return;
            }
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
                journalled: false,
            });
        }
        if let Some(last) = self.exchanges.back_mut() {
            add(&mut last.lamp, text, LAMP_BYTES);
            self.changed = true;
        }
    }

    /// The exchanges that are over and were not given out before, for the
    /// lamp's memory ([`crate::remember`]): every one but the newest, and
    /// the newest too when `done` is its request. Each with its request,
    /// how long ago it began, what the person said and what the lamp did.
    /// An answer with no word in it is passed over: it is not something
    /// the lamp said.
    pub fn finished(&mut self, done: u64, now: Instant) -> Vec<(u64, Duration, String, String)> {
        let newest = self.exchanges.back().map(|last| last.request);
        self.exchanges
            .iter_mut()
            .filter(|exchange| {
                !exchange.journalled
                    && (Some(exchange.request) != newest || exchange.request == done)
            })
            .filter_map(|exchange| {
                exchange.journalled = true;
                exchange.lamp.chars().any(char::is_alphanumeric).then(|| {
                    (
                        exchange.request,
                        now.saturating_duration_since(exchange.at),
                        exchange.person.trim().to_owned(),
                        exchange.lamp.trim().to_owned(),
                    )
                })
            })
            .collect()
    }

    /// The lamp's words for `request` were never said: the request was
    /// given up before its answer could be played (its end was sent early,
    /// and the person went on). Its exchange is dropped, and what the person
    /// had said before it waits again, as heard, for the answer that
    /// follows, before anything heard since.
    pub fn unsay(&mut self, request: u64) {
        let Some(index) = self
            .exchanges
            .iter()
            .rposition(|exchange| exchange.request == request)
        else {
            return;
        };
        let exchange = self.exchanges.remove(index).expect("found above");
        if self.unsaid == Some(request) {
            self.unsaid = None;
        }
        let mut heard = exchange.person;
        if !self.heard.is_empty() {
            add(&mut heard, " ", PERSON_BYTES);
            add(&mut heard, &self.heard, PERSON_BYTES);
        }
        self.heard = heard;
        self.heard_at = Some(self.heard_at.map_or(exchange.at, |at| at.max(exchange.at)));
        // Whatever comes next of what they say belongs with it.
        self.heard_done = false;
        self.changed = true;
    }

    /// Drop everything: a person has had something forgotten, and what was
    /// said about it is not to be told to the next session.
    pub fn forget(&mut self) {
        self.exchanges.clear();
        self.heard.clear();
        self.heard_at = None;
        self.heard_done = false;
        self.changed = true;
    }

    /// Whether the lamp said something to someone so lately that a
    /// conversation may still be going on.
    pub fn going_on(&self, now: Instant) -> bool {
        self.exchanges.iter().any(|exchange| {
            now.saturating_duration_since(exchange.at) <= GOING_ON_FOR
                && exchange.lamp.chars().any(char::is_alphanumeric)
        })
    }

    /// What to tell a new session, if anything was said lately, in at most
    /// `room` bytes: the latest exchanges that fit, in the order they went.
    pub fn recall(&self, now: Instant, room: usize) -> Option<String> {
        // An answer with no word in it ("...") is the model keeping quiet
        // aloud. Told back to it, it is what it does next.
        let told = |exchange: &&Exchange| {
            now.saturating_duration_since(exchange.at) <= REMEMBERED_FOR
                && exchange.lamp.chars().any(char::is_alphanumeric)
        };
        let newest = self.exchanges.iter().rev().find(told)?;
        let note = note(now.saturating_duration_since(newest.at));
        let mut kept: Vec<String> = Vec::new();
        let mut bytes = note.len();
        for exchange in self.exchanges.iter().rev().filter(told) {
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
            format!("{note}{}", kept.concat())
        })
    }
}

/// What goes before the exchanges, the newest of which is this old. See the
/// top of this file for what it must not say.
fn note(age: Duration) -> String {
    if age <= GOING_ON_FOR {
        return GOING_ON.to_owned();
    }
    let minutes = (age.as_secs() + 30) / 60;
    let ago = if minutes <= 1 {
        "About a minute ago".to_owned()
    } else {
        format!("About {minutes} minutes ago")
    };
    format!(
        "\n\nEarlier\n{ago} you and a person at your desk said this to each other. That conversation is finished and they have gone back to their work. It is background only, for when someone comes back to it: never read it out and never answer any of it again.\n"
    )
}

const GOING_ON: &str = "\n\nA moment ago\nThis is what you and the person at your desk have just said to each other, for you to carry on from if they go on with it. It is background only: never read it out and never answer any of it again.\n";

/// The provider worker's flag for the file, and where the coordinator
/// reads its path. Unset: the thread is in memory only.
pub const FILE_FLAG: &str = "--thread-file";
pub const FILE_ENVIRONMENT: &str = "LAMP_THREAD_FILE";

fn usable(path: &Path) -> bool {
    path.is_absolute()
        && path.as_os_str().len() <= 200
        && !path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
}

/// The provider worker's arguments for the path in the environment.
pub fn arguments_from_environment() -> Vec<String> {
    std::env::var(FILE_ENVIRONMENT)
        .ok()
        .filter(|path| usable(Path::new(path)))
        .map(|path| vec![FILE_FLAG.to_owned(), path])
        .unwrap_or_default()
}

/// Take a leading `--thread-file PATH` off a worker's flags.
pub fn take_file(flags: &[String]) -> std::io::Result<(Option<std::path::PathBuf>, &[String])> {
    match flags {
        [flag, path, rest @ ..] if flag == FILE_FLAG && usable(Path::new(path)) => {
            Ok((Some(path.into()), rest))
        }
        [flag, ..] if flag == FILE_FLAG => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the thread's file needs a short absolute path",
        )),
        _ => Ok((None, flags)),
    }
}

/// Put the thread's text in its file, whole or not at all, for its owner
/// only.
pub fn write_file(path: &Path, text: &str) -> std::io::Result<()> {
    let mut writing = path.as_os_str().to_owned();
    writing.push(".new");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&writing)?;
    file.write_all(text.as_bytes())?;
    drop(file);
    std::fs::rename(&writing, path)
}

impl Default for Thread {
    fn default() -> Self {
        Self::new()
    }
}

/// Opens each session with the thread so far added to its instruction.
pub struct Recalling {
    config: SessionConfig,
    tools: crate::tools::Settings,
    name_signal: bool,
}

impl Recalling {
    pub fn new(config: SessionConfig) -> Self {
        Self {
            config,
            tools: crate::tools::Settings::default(),
            name_signal: false,
        }
    }

    /// Each session is also asked to say when it hears the lamp's name
    /// ([`crate::attention::name_tool`]).
    pub fn name_signal(mut self, on: bool) -> Self {
        self.name_signal = on;
        self
    }

    /// Each session is also told what the model may do other than speak
    /// ([`crate::tools`]), as that stands when the session opens.
    pub fn tools(mut self, tools: crate::tools::Settings) -> Self {
        self.tools = tools;
        self
    }

    /// `config` with the functions declared now. The executive's come and
    /// go with the executive; a list the link would refuse leaves the
    /// session without any.
    fn equipped(&self, config: SessionConfig) -> SessionConfig {
        let mut tools = crate::tools::declared(&self.tools);
        if self.name_signal && tools.len() < lamp_gemini::MAX_TOOLS {
            tools.push(crate::attention::name_tool());
        }
        config.clone().tools(tools).unwrap_or(config)
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
        let room = lamp_gemini::MAX_INSTRUCTION_BYTES.saturating_sub(written.len() + time.len());
        thread
            .recall(now, room)
            .and_then(|thread| told(&format!("{time}{thread}")))
            .or_else(|| told(time))
            .unwrap_or_else(|| self.config.clone())
    }
}

impl Connect for Recalling {
    fn connect(&mut self, session: SessionId, resume: Option<ResumptionHandle>) -> Attempt {
        let mut time = crate::local_time::told_here(self.tools.clock);
        // The legacy cached note, or an explicitly opted-in Days snapshot
        // with its provenance and a total acquisition budget of 300 ms.
        if let Some(socket) = &self.tools.memory {
            let kept = crate::remember::told_for_session(socket, session.get());
            if !kept.is_empty() {
                time = Some(time.unwrap_or_default() + &kept);
            }
        }
        let config = match THREAD.lock() {
            Ok(mut thread) => {
                crate::remember::clear_forgotten_thread(&mut thread);
                self.recalled(&thread, Instant::now(), time.as_deref())
            }
            Err(_) => self.config.clone(),
        };
        Dialer::new(self.equipped(config)).connect(session, resume)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a session is sent at setup follows the switch: nothing about
    /// functions with it off, the voice's own with it on.
    #[test]
    fn a_session_is_opened_with_the_functions_the_model_may_call_now() {
        let config = || {
            SessionConfig::new(
                lamp_gemini::GOOGLE_ENDPOINT,
                lamp_gemini::Credential::api_key("test-only").unwrap(),
            )
            .unwrap()
        };
        let setup = |recalling: &Recalling| -> serde_json::Value {
            let text = lamp_gemini::wire::encode_setup(&recalling.equipped(config())).unwrap();
            serde_json::from_str(&text).unwrap()
        };
        let plain = Recalling::new(config());
        assert!(setup(&plain)["setup"].get("tools").is_none());
        let declining = Recalling::new(config()).tools(crate::tools::Settings {
            decline: true,
            exec: None,
            memory: None,
            clock: false,
            tasks: None,
        });
        let told = setup(&declining);
        let declared = &told["setup"]["tools"][0]["functionDeclarations"];
        assert_eq!(declared.as_array().unwrap().len(), 1);
        assert_eq!(declared[0]["name"], crate::tools::NOT_FOR_ME);
        assert!(declared[0].get("parameters").is_none());
    }

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
        // Nothing in it for the lamp to talk about but the conversation,
        // whether that is going on or finished.
        assert!(recalled.starts_with("\n\nA moment ago\n"), "{recalled}");
        assert!(thread.going_on(at(61)) && !thread.going_on(at(60 + 46)));
        let later = thread.recall(at(60 + 46), ROOM).unwrap();
        assert!(
            later.starts_with("\n\nEarlier\nAbout a minute ago you and a person"),
            "{later}"
        );
        assert!(later.contains("That conversation is finished"));
        assert!(later.ends_with("You: Still here if you need me.\n"));
        assert!(
            thread
                .recall(at(60 + 400), ROOM)
                .unwrap()
                .starts_with("\n\nEarlier\nAbout 7 minutes ago you")
        );
        for told in [&recalled, &later] {
            let note = told.split("Person: ").next().unwrap().to_lowercase();
            for word in [
                "connect", "renew", "session", "memory", "forg", "lost", "sorry", "silen", "quiet",
                "nothing",
            ] {
                assert!(!note.contains(word), "{word}: {note}");
            }
            // Only a conversation that may go on is one to carry on.
            assert_eq!(note.contains("carry on"), std::ptr::eq(told, &recalled));
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

    /// The end sent early (`--early-end-ms`): an answer that began to
    /// arrive before the lamp was sure, for a request then given up because
    /// the person went on, was never said. The person's words wait for the
    /// answer to what they went on to say.
    #[test]
    fn words_of_a_request_given_up_before_its_answer_played_were_never_said() {
        let mut thread = Thread::new();
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        thread.person("Hey Lamp,", true, at(1));
        thread.lamp(1, "Yes? How can", at(2));
        thread.person(" what time", false, at(2));
        thread.unsay(1);
        assert_eq!(thread.recall(at(3), ROOM), None);
        assert!(!thread.going_on(at(3)));
        thread.person(" is it?", true, at(3));
        thread.lamp(2, "It's three.", at(4));
        let recalled = thread.recall(at(5), ROOM).unwrap();
        assert!(
            recalled.ends_with("Person: Hey Lamp, what time is it?\nYou: It's three.\n"),
            "{recalled}"
        );
        assert!(!recalled.contains("How can"));
        // Nothing for a request that has no words, and nothing else moves.
        thread.unsay(7);
        assert_eq!(thread.recall(at(5), ROOM).unwrap(), recalled);
        let finished = thread.finished(2, at(5));
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].0, 2);
    }

    #[test]
    fn a_saved_thread_is_told_again_by_the_process_that_reads_it_back() {
        let mut thread = Thread::new();
        let start = Instant::now() + Duration::from_secs(3_600);
        let wall = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        assert_eq!(
            thread.save(at(0), wall),
            None,
            "nothing said, nothing to save"
        );
        thread.person("My name is Sam.", true, at(1));
        thread.lamp(3, "Nice to meet you, Sam.", at(2));
        thread.person("Are you coming to lunch?", true, at(3));
        thread.lamp(4, "...", at(4));
        thread.person("What is my name?", true, at(100));
        thread.lamp(5, "Sam!", at(101));
        let text = thread
            .save(at(102), wall + Duration::from_secs(102))
            .unwrap();
        assert_eq!(thread.save(at(103), wall), None, "unchanged since");
        assert!(!text.contains("lunch") && text.contains("Sam!"));
        // Another process, a few seconds later by the wall clock, with a
        // clock of its own that started somewhere else.
        let other = Instant::now() + Duration::from_secs(9_000);
        let later = wall + Duration::from_secs(110);
        let read = Thread::load(&text, other, later);
        let told = read.recall(other, ROOM).unwrap();
        assert!(
            told.ends_with(
                "Person: My name is Sam.\nYou: Nice to meet you, Sam.\nPerson: What is my name?\nYou: Sam!\n"
            ),
            "{told}"
        );
        // Its own request numbers never run into what it read.
        let mut read = read;
        read.person("And yours?", true, other);
        read.lamp(1, "Lamp.", other);
        assert_eq!(
            read.recall(other, ROOM).unwrap().matches("You: ").count(),
            3
        );
        // Eleven minutes on, the first exchange is not read back; an hour
        // on, nothing is. Nor from a file that says it is from the future,
        // or that is not a thread at all.
        let aged = Thread::load(&text, other, wall + Duration::from_secs(2 + 660));
        assert_eq!(
            aged.recall(other, ROOM).unwrap().matches("You: ").count(),
            1
        );
        assert_eq!(
            Thread::load(&text, other, wall + Duration::from_secs(4_000)).recall(other, ROOM),
            None
        );
        assert_eq!(Thread::load(&text, other, wall).recall(other, ROOM), None);
        for junk in ["", "{}", "[{\"at_unix_ms\":1}]", "not json"] {
            assert_eq!(Thread::load(junk, other, later).recall(other, ROOM), None);
        }
        // The file itself: whole, private, and replaced whole.
        let directory = std::env::temp_dir().join(format!(
            "lamp-voice-thread-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("thread.json");
        write_file(&path, &text).unwrap();
        write_file(&path, "[]").unwrap();
        write_file(&path, &text).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        std::fs::remove_dir_all(&directory).unwrap();
        // And its flag.
        let flags: Vec<String> = [
            FILE_FLAG,
            "/run/lampos-voice/thread.json",
            "--camera-context",
        ]
        .iter()
        .map(|flag| (*flag).to_owned())
        .collect();
        let (file, rest) = take_file(&flags).unwrap();
        assert_eq!(
            file,
            Some(std::path::PathBuf::from("/run/lampos-voice/thread.json"))
        );
        assert_eq!(rest, &flags[2..]);
        assert_eq!(take_file(&flags[2..]).unwrap(), (None, &flags[2..]));
        assert!(take_file(&flags[..1]).is_err());
    }

    #[test]
    fn a_finished_exchange_is_given_out_once_and_forgetting_drops_them_all() {
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        let mut thread = Thread::new();
        thread.person("How do I take my coffee?", true, at(0));
        thread.lamp(1, "Black.", at(1));
        // Not over yet: another request is done, and this is the newest.
        assert!(thread.finished(0, at(2)).is_empty());
        assert_eq!(
            thread.finished(1, at(4)),
            [(
                1,
                Duration::from_secs(3),
                "How do I take my coffee?".to_owned(),
                "Black.".to_owned()
            )]
        );
        assert!(thread.finished(1, at(5)).is_empty());
        // An answer that never completed is given out when the next begins;
        // one with no word in it never is.
        thread.lamp(2, "And", at(6));
        thread.lamp(3, "...", at(7));
        thread.lamp(4, "Yes?", at(8));
        let given: Vec<u64> = thread
            .finished(9, at(9))
            .into_iter()
            .map(|(request, ..)| request)
            .collect();
        assert_eq!(given, [2]);
        assert_eq!(thread.finished(4, at(9)).len(), 1);
        // What a restarted service reads back was handed on by the one
        // that wrote it.
        let saved = thread.save(at(9), SystemTime::now()).unwrap();
        let mut loaded = Thread::load(&saved, at(9), SystemTime::now());
        assert!(loaded.recall(at(9), 4_000).is_some());
        assert!(loaded.finished(4, at(9)).is_empty());
        thread.forget();
        assert_eq!(thread.recall(at(9), 4_000), None);
        assert!(!thread.going_on(at(9)));
        assert_eq!(thread.save(at(9), SystemTime::now()).as_deref(), Some("[]"));
    }

    #[test]
    fn a_function_named_aloud_is_not_something_the_lamp_said() {
        let start = Instant::now();
        let mut thread = Thread::new();
        thread.person("Then I will bring an umbrella.", true, start);
        thread.lamp(7, "not_for_me", start);
        thread.lamp(7, " and more of the same answer", start);
        assert!(!thread.going_on(start));
        assert_eq!(thread.recall(start, 4_000), None);
        assert!(thread.finished(7, start).is_empty());
        // The next answer is an answer.
        thread.lamp(8, "Yes?", start);
        assert!(thread.going_on(start));
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
        assert!(recalled.ends_with(&fewer[GOING_ON.len()..]));
        assert_eq!(thread.recall(at(730), GOING_ON.len() + 10), None);
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
            both.instruction_text()
                .starts_with("You are a lamp.\n\nThe time\nNoon.\n\nA moment ago\n")
        );
        // A thread with no room left leaves the time; a time too long, nothing.
        let long = "x".repeat(lamp_gemini::MAX_INSTRUCTION_BYTES - 200);
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
                .starts_with("You are a lamp.\n\nA moment ago\n")
        );
        assert!(
            told.instruction_text()
                .ends_with("Person: My name is Sam.\nYou: Nice to meet you, Sam.\n")
        );
    }
}
