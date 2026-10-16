//! The say service (#939, [skills.md](../../../docs/skills.md)): another
//! part of the lamp, `lamp-skills` first, asks the voice to say something
//! unprompted, and is told what became of it. Off unless `LAMP_SAY=on`.
//!
//! - **The socket** is [`SOCKET_ENVIRONMENT`], [`DEFAULT_SOCKET`] if unset:
//!   one JSON line in, `{skill, note | text, priority, expires_in_ms}`, and
//!   one JSON line out once the outcome is known ([`Outcome`]): `spoken`,
//!   `dropped` with why, `expired`, or `refused` with why. Before it, when
//!   the voice begins the turn, `{"event":"begun","at_us":…}`, so that the
//!   body and the light can move with the words (the first builder on #939).
//!   The connection stays open until the outcome.
//! - **Said** as a written turn, as the hello, task news and reminders are
//!   ([`crate::coordinator`]'s `commit_written_turn`), through the gates a
//!   reminder has: the keeper's pause, nobody being heard and nothing owed
//!   or under way, the speaker free; and a quiet since the last turn of
//!   [`QUIET_US`], or [`URGENT_QUIET_US`] from [`URGENT_FROM`] up.
//! - **Spoken** only on the proof a reminder's acknowledgement takes: the
//!   turn completed with the model's audio played whole. Anything else once
//!   begun is `dropped`, never tried again here: the asker decides.
//! - **The Policy is not asked here.** Whoever asks the voice to speak has
//!   asked the Policy's gate first (`docs/skills.md`).
//!
//! Nothing of what is said is logged: the skill's name, the priority and
//! the outcome only.
use serde::Deserialize;
use serde_json::json;
use std::{
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

/// Where the coordinator reads the switch. `on` only.
pub const ENVIRONMENT: &str = "LAMP_SAY";
/// The socket's path, absolute.
pub const SOCKET_ENVIRONMENT: &str = "LAMP_SAY_SOCKET";
/// The group, by name, that may ask: the socket's directory and the socket
/// are opened to it. Unset, both are the voice's user's alone.
pub const GROUP_ENVIRONMENT: &str = "LAMP_SAY_GROUP";
/// The voice runs as root and its own runtime directory is closed to
/// others: the socket has a directory of its own.
pub const DEFAULT_SOCKET: &str = "/run/lampos-say/say.sock";
/// The longest note or text: a written turn is at most 1,024 bytes, and a
/// text goes with the words that tell the model to say it as it is.
pub const MOST_WORDS_BYTES: usize = 900;
/// Asks waiting at once, answered or not; one more is refused.
pub const MOST_WAITING: usize = 16;
/// The longest an ask may wait to be begun.
pub const LONGEST_EXPIRY_MS: u64 = 3_600_000;
/// The priority when none is given, from 0 to 9.
pub const NORMAL_PRIORITY: u8 = 5;
/// From this priority up, a say needs only [`URGENT_QUIET_US`] after the
/// last turn, as a reminder does.
pub const URGENT_FROM: u8 = 8;
/// Quiet since the last turn before a say below [`URGENT_FROM`], as task
/// news (`crate::task_news::QUIET_US`).
pub const QUIET_US: u64 = crate::task_news::QUIET_US;
/// Quiet since the last turn before an urgent say.
pub const URGENT_QUIET_US: u64 = 1_000_000;
/// How long a client has to send its line.
const READ_WITHIN: Duration = Duration::from_secs(1);
/// The most a request line may be.
const MOST_LINE_BYTES: u64 = 4_096;
/// Told the model before a text, so that it says the words as they are.
pub const AS_WRITTEN: &str = "Say exactly these words, in your own voice, and nothing else:";

/// What the asker is told, in order: begun, if it was, then the outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The voice began the turn at this moment of the shared monotonic
    /// clock (`lamp_ipc::monotonic_us`), in microseconds: its first sound
    /// follows when the model's comes, about a second later.
    Begun(u64),
    Done(Outcome),
}

impl Answer {
    /// The answer's one line, without its newline.
    pub fn line(&self) -> String {
        match self {
            // The event's name first, as a reader looks for it.
            Self::Begun(at_us) => format!(r#"{{"event":"begun","at_us":{at_us}}}"#),
            Self::Done(outcome) => outcome.line(),
        }
    }
}

/// What became of an ask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Said: the turn completed with its audio played whole.
    Spoken,
    /// Begun and not said, with why.
    Dropped(String),
    /// Not begun in time.
    Expired,
    /// Not taken, with why.
    Refused(String),
}

impl Outcome {
    /// The answer's one line, without its newline.
    pub fn line(&self) -> String {
        match self {
            Self::Spoken => json!({"outcome": "spoken"}),
            Self::Dropped(why) => json!({"outcome": "dropped", "why": why}),
            Self::Expired => json!({"outcome": "expired"}),
            Self::Refused(why) => json!({"outcome": "refused", "why": why}),
        }
        .to_string()
    }
}

/// What to say: an instruction the model phrases, or words to say as they
/// are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Words {
    Note(String),
    Text(String),
}

impl Words {
    /// The written turn the model is given.
    pub fn turn(&self) -> String {
        match self {
            Self::Note(note) => note.clone(),
            Self::Text(text) => format!("{AS_WRITTEN} {text}"),
        }
    }
}

/// One request, read and checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub skill: String,
    pub words: Words,
    pub priority: u8,
    pub expires_in_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    skill: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    priority: Option<u8>,
    expires_in_ms: u64,
}

/// Read and check one request line. The reason, if refused, names the
/// field and never repeats what was asked.
pub fn parse(line: &str) -> Result<Request, String> {
    let wire: Wire = serde_json::from_str(line.trim_end_matches(['\n', '\r']))
        .map_err(|_| "not a say request".to_owned())?;
    let skill = wire.skill;
    if skill.is_empty()
        || skill.len() > 64
        || !skill
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_".contains(&byte))
    {
        return Err("skill is 1 to 64 of a-z, 0-9, - and _".into());
    }
    let words = match (wire.note, wire.text) {
        (Some(note), None) => Words::Note(note),
        (None, Some(text)) => Words::Text(text),
        _ => return Err("exactly one of note and text".into()),
    };
    let (Words::Note(said) | Words::Text(said)) = &words;
    if said.trim().is_empty() || said.len() > MOST_WORDS_BYTES || said.chars().any(char::is_control)
    {
        return Err(format!(
            "note or text is 1 to {MOST_WORDS_BYTES} bytes, with no control characters"
        ));
    }
    let priority = wire.priority.unwrap_or(NORMAL_PRIORITY);
    if priority > 9 {
        return Err("priority is 0 to 9".into());
    }
    if !(1..=LONGEST_EXPIRY_MS).contains(&wire.expires_in_ms) {
        return Err(format!("expires_in_ms is 1 to {LONGEST_EXPIRY_MS}"));
    }
    Ok(Request {
        skill,
        words,
        priority,
        expires_in_ms: wire.expires_in_ms,
    })
}

/// A request handed to the voice, with where its answer goes.
struct Asked {
    request: Request,
    answer: mpsc::Sender<Answer>,
}

/// One waiting, dated by the voice's clock as it was taken.
struct Waiting {
    request: Request,
    answer: mpsc::Sender<Answer>,
    expires_at_us: u64,
    order: u64,
}

/// A say begun: it lives on the voice's reply and is answered when the
/// turn ends. Dropped unanswered, it is `dropped` with "ended".
pub struct Ticket {
    skill: String,
    answer: Option<mpsc::Sender<Answer>>,
}

impl Ticket {
    pub fn skill(&self) -> &str {
        &self.skill
    }

    /// The voice began the turn at `at_us`. A client that has gone is
    /// nothing to the voice.
    pub fn begun(&self, at_us: u64) {
        if let Some(answer) = self.answer.as_ref() {
            let _ = answer.send(Answer::Begun(at_us));
        }
    }

    /// Answer it.
    pub fn answer(mut self, outcome: Outcome) {
        if let Some(answer) = self.answer.take() {
            let _ = answer.send(Answer::Done(outcome));
        }
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if let Some(answer) = self.answer.take() {
            let _ = answer.send(Answer::Done(Outcome::Dropped("ended".into())));
        }
    }
}

/// A ticket for a test of the voice's turn, and where its answer goes.
#[cfg(test)]
pub(crate) fn ticket_for_test(skill: &str) -> (Ticket, mpsc::Receiver<Answer>) {
    let (answer, answered) = mpsc::channel();
    (
        Ticket {
            skill: skill.to_owned(),
            answer: Some(answer),
        },
        answered,
    )
}

/// The voice's side: what has been asked and waits to be said.
pub struct Says {
    incoming: mpsc::Receiver<Asked>,
    waiting: Vec<Waiting>,
    order: u64,
    /// Kept so that the socket goes with the voice.
    _socket: Option<Bound>,
}

impl Says {
    /// The service, when [`ENVIRONMENT`] is `on` and the socket can be
    /// made; one that cannot is said once, and the voice goes on without.
    pub fn from_environment() -> Option<Self> {
        if std::env::var(ENVIRONMENT).ok().as_deref() != Some("on") {
            return None;
        }
        let path = std::env::var_os(SOCKET_ENVIRONMENT)
            .map_or_else(|| PathBuf::from(DEFAULT_SOCKET), PathBuf::from);
        let group = match std::env::var(GROUP_ENVIRONMENT) {
            Ok(name) => match fs::read_to_string("/etc/group")
                .ok()
                .and_then(|groups| group_id(&groups, &name))
            {
                Some(id) => Some(id),
                None => {
                    eprintln!(
                        "lamp-voice: the say service's group {name:?} is not known; not started"
                    );
                    return None;
                }
            },
            Err(_) => None,
        };
        match Self::serve(&path, group) {
            Ok(says) => {
                eprintln!("lamp-voice: the say service listens at {}", path.display());
                Some(says)
            }
            Err(error) => {
                eprintln!(
                    "lamp-voice: the say service cannot listen at {}: {error}",
                    path.display()
                );
                None
            }
        }
    }

    /// Listen at `path`, opened to `group` if given, on a thread of its
    /// own.
    pub fn serve(path: &Path, group: Option<u32>) -> io::Result<Self> {
        let (listener, bound) = bind(path, group)?;
        let (ask, incoming) = mpsc::channel();
        let open = Arc::new(AtomicUsize::new(0));
        thread::Builder::new()
            .name("lamp-voice-say".into())
            .spawn(move || accept(&listener, &ask, &open))?;
        Ok(Self {
            incoming,
            waiting: Vec::new(),
            order: 0,
            _socket: Some(bound),
        })
    }

    /// For a test: asks are put in by hand.
    #[cfg(test)]
    fn by_hand() -> (Self, mpsc::Sender<Asked>) {
        let (ask, incoming) = mpsc::channel();
        (
            Self {
                incoming,
                waiting: Vec::new(),
                order: 0,
                _socket: None,
            },
            ask,
        )
    }

    /// Take what has been asked, dated `now_us`, and answer those not begun
    /// in time. Called on every pass of the voice's loop; never waits.
    pub fn poll(&mut self, now_us: u64) {
        while let Ok(asked) = self.incoming.try_recv() {
            self.order += 1;
            let expires_at_us =
                now_us.saturating_add(asked.request.expires_in_ms.saturating_mul(1_000));
            self.waiting.push(Waiting {
                request: asked.request,
                answer: asked.answer,
                expires_at_us,
                order: self.order,
            });
        }
        self.waiting.retain(|waiting| {
            let live = now_us < waiting.expires_at_us;
            if !live {
                let _ = waiting.answer.send(Answer::Done(Outcome::Expired));
            }
            live
        });
    }

    fn next_index(&self) -> Option<usize> {
        (0..self.waiting.len()).max_by(|&a, &b| {
            let (a, b) = (&self.waiting[a], &self.waiting[b]);
            a.request
                .priority
                .cmp(&b.request.priority)
                .then(b.order.cmp(&a.order))
        })
    }

    /// The quiet the next say needs since the last turn, if one waits.
    pub fn quiet_needed_us(&self) -> Option<u64> {
        self.next_index().map(|index| {
            if self.waiting[index].request.priority >= URGENT_FROM {
                URGENT_QUIET_US
            } else {
                QUIET_US
            }
        })
    }

    /// Take the next say to begin it: its written turn, its priority, and
    /// its ticket.
    pub fn take(&mut self) -> Option<(String, u8, Ticket)> {
        let waiting = self.waiting.remove(self.next_index()?);
        Some((
            waiting.request.words.turn(),
            waiting.request.priority,
            Ticket {
                skill: waiting.request.skill,
                answer: Some(waiting.answer),
            },
        ))
    }

    /// How many wait.
    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }
}

/// The group id of `name` in the text of `/etc/group`.
pub fn group_id(groups: &str, name: &str) -> Option<u32> {
    groups.lines().find_map(|line| {
        let mut fields = line.split(':');
        (fields.next()? == name)
            .then(|| fields.nth(1)?.parse().ok())
            .flatten()
    })
}

/// The socket, removed when the voice ends if it is still the one made.
struct Bound {
    path: PathBuf,
}

impl Drop for Bound {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn bind(path: &Path, group: Option<u32>) -> io::Result<(UnixListener, Bound)> {
    let invalid = |why: &str| io::Error::new(io::ErrorKind::InvalidInput, why.to_owned());
    if !path.is_absolute() || path.as_os_str().len() > 103 {
        return Err(invalid(
            "the socket's path is absolute and at most 103 bytes",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| invalid("the socket needs a directory"))?;
    let mode = if group.is_some() { 0o750 } else { 0o700 };
    if !parent.exists() {
        fs::DirBuilder::new().mode(mode).create(parent)?;
    }
    fs::set_permissions(parent, fs::Permissions::from_mode(mode))?;
    if let Some(group) = group {
        std::os::unix::fs::chown(parent, None, Some(group))?;
    }
    // A socket left by a voice that has gone is taken over; one that
    // answers belongs to a voice still running.
    if let Ok(metadata) = fs::symlink_metadata(path) {
        use std::os::unix::fs::FileTypeExt;
        if !metadata.file_type().is_socket() {
            return Err(invalid("something that is not a socket is at the path"));
        }
        if UnixStream::connect(path).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "another voice listens there",
            ));
        }
        fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if group.is_some() { 0o660 } else { 0o600 }),
    )?;
    if let Some(group) = group {
        std::os::unix::fs::chown(path, None, Some(group))?;
    }
    Ok((
        listener,
        Bound {
            path: path.to_path_buf(),
        },
    ))
}

/// Take each client on a thread of its own, at most [`MOST_WAITING`] at
/// once. Ends when the voice has gone.
fn accept(listener: &UnixListener, ask: &mpsc::Sender<Asked>, open: &Arc<AtomicUsize>) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if open.fetch_add(1, Ordering::AcqRel) >= MOST_WAITING {
            open.fetch_sub(1, Ordering::AcqRel);
            let _ = writeln!(
                stream,
                "{}",
                Outcome::Refused(format!("{MOST_WAITING} wait already")).line()
            );
            continue;
        }
        let (ask, held) = (ask.clone(), Arc::clone(open));
        let spawned = thread::Builder::new()
            .name("lamp-voice-say-client".into())
            .spawn(move || {
                serve_client(stream, &ask);
                held.fetch_sub(1, Ordering::AcqRel);
            });
        if spawned.is_err() {
            open.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

/// One client: its line, checked, handed to the voice, and the answer
/// written back when it comes.
fn serve_client(stream: UnixStream, ask: &mpsc::Sender<Asked>) {
    let _ = stream.set_read_timeout(Some(READ_WITHIN));
    let mut line = String::new();
    let Ok(reader) = stream.try_clone() else {
        return;
    };
    let read = BufReader::new(reader)
        .take(MOST_LINE_BYTES)
        .read_line(&mut line);
    let mut stream = stream;
    let outcome = match read {
        Ok(0) | Err(_) => Outcome::Refused("no request line in time".into()),
        Ok(_) if !line.ends_with('\n') => Outcome::Refused("one line, ended by a newline".into()),
        Ok(_) => match parse(&line) {
            Err(why) => Outcome::Refused(why),
            Ok(request) => {
                let (answer, answered) = mpsc::channel();
                if ask.send(Asked { request, answer }).is_err() {
                    Outcome::Refused("the voice has gone".into())
                } else {
                    // The voice answers an ask not begun in time itself;
                    // begun, the turn may run on past its expiry, so the
                    // answer is waited for until it comes or the voice goes.
                    loop {
                        match answered.recv() {
                            Ok(Answer::Begun(at_us)) => {
                                let _ = writeln!(stream, "{}", Answer::Begun(at_us).line());
                            }
                            Ok(Answer::Done(outcome)) => break outcome,
                            Err(_) => break Outcome::Dropped("the voice has gone".into()),
                        }
                    }
                }
            }
        },
    };
    let _ = writeln!(stream, "{}", outcome.line());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(skill: &str, priority: u8, expires_in_ms: u64) -> (Asked, mpsc::Receiver<Answer>) {
        let (answer, answered) = mpsc::channel();
        (
            Asked {
                request: Request {
                    skill: skill.into(),
                    words: Words::Note(format!("{skill}'s note")),
                    priority,
                    expires_in_ms,
                },
                answer,
            },
            answered,
        )
    }

    #[test]
    fn a_request_is_read_and_checked_and_a_refusal_names_the_field() {
        let reminder = parse(
            r#"{"skill":"reminders","note":"Remind them to call the bank.","priority":8,"expires_in_ms":120000}"#,
        )
        .unwrap();
        assert_eq!(
            reminder,
            Request {
                skill: "reminders".into(),
                words: Words::Note("Remind them to call the bank.".into()),
                priority: 8,
                expires_in_ms: 120_000,
            }
        );
        let text =
            parse(r#"{"skill":"task-news","text":"The tests pass.","expires_in_ms":1}"#).unwrap();
        assert_eq!(text.priority, NORMAL_PRIORITY);
        assert_eq!(text.words.turn(), format!("{AS_WRITTEN} The tests pass."));
        let long = "a".repeat(MOST_WORDS_BYTES + 1);
        for (line, field) in [
            ("not json", "not a say request"),
            (
                r#"{"skill":"x","note":"n","expires_in_ms":5,"loud":true}"#,
                "not a say request",
            ),
            (r#"{"skill":"","note":"n","expires_in_ms":5}"#, "skill"),
            (
                r#"{"skill":"Welcome Back","note":"n","expires_in_ms":5}"#,
                "skill",
            ),
            (r#"{"skill":"x","expires_in_ms":5}"#, "exactly one"),
            (
                r#"{"skill":"x","note":"n","text":"t","expires_in_ms":5}"#,
                "exactly one",
            ),
            (
                r#"{"skill":"x","note":"  ","expires_in_ms":5}"#,
                "note or text",
            ),
            (
                r#"{"skill":"x","note":"a\u0007b","expires_in_ms":5}"#,
                "note or text",
            ),
            (
                &format!(r#"{{"skill":"x","note":"{long}","expires_in_ms":5}}"#),
                "note or text",
            ),
            (
                r#"{"skill":"x","note":"n","priority":10,"expires_in_ms":5}"#,
                "priority",
            ),
            (
                r#"{"skill":"x","note":"n","expires_in_ms":0}"#,
                "expires_in_ms",
            ),
            (
                r#"{"skill":"x","note":"n","expires_in_ms":3600001}"#,
                "expires_in_ms",
            ),
            (r#"{"skill":"x","note":"n"}"#, "not a say request"),
        ] {
            let why = parse(line).unwrap_err();
            assert!(why.contains(field), "{line}: {why}");
            // Nothing asked is repeated.
            assert!(!why.contains("aaaa") && !why.contains("Welcome"), "{why}");
        }
    }

    #[test]
    fn the_highest_priority_goes_first_then_the_oldest_and_the_late_expire() {
        let (mut says, ask) = Says::by_hand();
        let (news, news_answer) = asked("task-news", 5, 60_000);
        let (stretch, _stretch_answer) = asked("stretch", 5, 60_000);
        let (reminder, _) = asked("reminders", 8, 60_000);
        let (late, late_answer) = asked("air", 3, 2);
        for one in [news, stretch, reminder, late] {
            ask.send(one).unwrap();
        }
        says.poll(1_000_000);
        assert_eq!(says.waiting(), 4);
        // An urgent one needs a second's quiet, the rest fifteen.
        assert_eq!(says.quiet_needed_us(), Some(URGENT_QUIET_US));
        let (_, priority, ticket) = says.take().unwrap();
        assert_eq!((ticket.skill(), priority), ("reminders", 8));
        assert_eq!(says.quiet_needed_us(), Some(QUIET_US));
        // Two at 5: the older first.
        let (turn, _, ticket) = says.take().unwrap();
        assert_eq!(
            (ticket.skill(), turn.as_str()),
            ("task-news", "task-news's note")
        );
        ticket.answer(Outcome::Spoken);
        assert_eq!(news_answer.try_recv(), Ok(Answer::Done(Outcome::Spoken)));
        // 2 ms after it was taken, the air's has expired, and is told so.
        says.poll(1_002_000);
        assert_eq!(late_answer.try_recv(), Ok(Answer::Done(Outcome::Expired)));
        assert_eq!(says.waiting(), 1);
        assert_eq!(says.take().unwrap().2.skill(), "stretch");
        assert_eq!(
            says.take().map(|(_, _, ticket)| ticket.skill().to_owned()),
            None
        );
        assert_eq!(says.quiet_needed_us(), None);
    }

    #[test]
    fn a_ticket_dropped_unanswered_is_dropped_and_a_client_gone_is_nothing() {
        let (mut says, ask) = Says::by_hand();
        let (one, answer) = asked("welcome-back", 5, 60_000);
        ask.send(one).unwrap();
        says.poll(0);
        drop(says.take().unwrap().2);
        assert_eq!(
            answer.try_recv(),
            Ok(Answer::Done(Outcome::Dropped("ended".into())))
        );
        let (two, gone) = asked("welcome-back", 5, 60_000);
        ask.send(two).unwrap();
        drop(gone);
        says.poll(0);
        says.take()
            .unwrap()
            .2
            .answer(Outcome::Dropped("user_interrupted".into()));
    }

    #[test]
    fn the_answers_are_one_line_each() {
        assert_eq!(Answer::Begun(42).line(), r#"{"event":"begun","at_us":42}"#);
        assert_eq!(Outcome::Spoken.line(), r#"{"outcome":"spoken"}"#);
        assert_eq!(
            Outcome::Dropped("user_interrupted".into()).line(),
            r#"{"outcome":"dropped","why":"user_interrupted"}"#
        );
        assert_eq!(Outcome::Expired.line(), r#"{"outcome":"expired"}"#);
        assert_eq!(
            Outcome::Refused("priority is 0 to 9".into()).line(),
            r#"{"outcome":"refused","why":"priority is 0 to 9"}"#
        );
    }

    #[test]
    fn a_group_is_found_by_name() {
        let groups = "root:x:0:\nlampos:x:998:voice\naudio:x:29:lampos\n";
        assert_eq!(group_id(groups, "lampos"), Some(998));
        assert_eq!(group_id(groups, "audio"), Some(29));
        assert_eq!(group_id(groups, "lamp"), None);
        assert_eq!(group_id("broken", "broken"), None);
    }

    fn socket_dir(tag: &str) -> PathBuf {
        // Short: a socket's whole path must fit in 103 bytes.
        let dir = PathBuf::from(format!("/tmp/lvs-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn exchange(path: &Path, line: &str) -> String {
        let mut stream = UnixStream::connect(path).unwrap();
        stream.write_all(line.as_bytes()).unwrap();
        let mut answer = String::new();
        BufReader::new(stream).read_to_string(&mut answer).unwrap();
        answer
    }

    /// Through the socket: a request is handed to the voice and its answer
    /// comes back when the voice gives it; a bad one is refused at once; the
    /// socket is the voice user's alone without a group.
    #[test]
    fn through_the_socket_an_ask_is_answered_when_the_voice_says_what_became_of_it() {
        let dir = socket_dir("sock");
        let path = dir.join("say.sock");
        let mut says = Says::serve(&path, None).unwrap();
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            exchange(&path, "{\"skill\":\"x\",\"expires_in_ms\":5}\n"),
            format!(
                "{}\n",
                Outcome::Refused("exactly one of note and text".into()).line()
            )
        );
        let client = {
            let path = path.clone();
            thread::spawn(move || {
                exchange(
                    &path,
                    "{\"skill\":\"reminders\",\"text\":\"Call the bank.\",\"priority\":9,\"expires_in_ms\":60000}\n",
                )
            })
        };
        // The voice takes it on a pass, says it, and answers.
        let mut taken = None;
        for _ in 0..200 {
            says.poll(5_000_000);
            if let Some(one) = says.take() {
                taken = Some(one);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let (turn, priority, ticket) = taken.expect("the ask reached the voice");
        assert_eq!(
            (turn, priority),
            (format!("{AS_WRITTEN} Call the bank."), 9)
        );
        // Begun, then spoken: two lines, in that order.
        ticket.begun(1_234_567);
        ticket.answer(Outcome::Spoken);
        assert_eq!(
            client.join().unwrap(),
            "{\"event\":\"begun\",\"at_us\":1234567}\n{\"outcome\":\"spoken\"}\n"
        );
        // A second voice cannot take the socket of one still listening.
        assert_eq!(
            Says::serve(&path, None).err().map(|error| error.kind()),
            Some(io::ErrorKind::AddrInUse)
        );
        drop(says);
        assert!(!path.exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
