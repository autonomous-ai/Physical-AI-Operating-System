//! What the lamp remembers from one day to the next, as the voice uses it.
//!
//! The memory is another process (`lamp-memory`, `docs/memory.md`) that the
//! voice asks over a socket and never waits on: the lamp talks on, without
//! recalling earlier days, when the memory has stopped, hung or gone. The
//! voice does four things with it:
//!
//! - **Three functions for the model** ([`tools`]): `remember` what a person
//!   asks it to keep, `recall` something from an earlier conversation, and
//!   `forget` the last thing it was asked to keep. Each is one request to
//!   the memory on a thread of its own, answered within
//!   [`crate::tools::ASK_DEADLINE`] or told to the model as not done.
//! - **A journal**: each exchange the lamp took part in, once it is over
//!   ([`finished`]), handed to one writer thread through a bounded queue.
//!   A full queue or a memory that does not answer loses that exchange and
//!   nothing else.
//! - **A note for each new session** ([`told`]): the newest things it was
//!   asked to keep, read by the writer thread and held here, so that
//!   opening a session never waits for the memory.
//! - **Nothing of an exchange in which it was asked to keep or forget
//!   something goes in the journal**: the kept list is the record of that,
//!   and a thing a person has had forgotten must not come back from there.
//!   This is not yet all of forgetting: a later conversation in which the
//!   thing came up is in the journal until it ages out, which is why the
//!   lamp says it is "off its list" and not that it is forgotten.
//!
//! No word of what was said, kept or asked for is ever logged.
use lamp_gemini::{Tool, ToolCall};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Where the coordinator reads the memory's socket: an absolute path.
pub const ENVIRONMENT: &str = "LAMP_MEMORY_SOCKET";
/// The provider worker's flag for it, followed by the path.
pub const FLAG: &str = "--memory-socket";

pub const REMEMBER: &str = "remember";
pub const RECALL: &str = "recall";
pub const FORGET: &str = "forget";

/// The longest the writer thread waits for the memory, which itself gives a
/// client two seconds. Nobody waits on the writer.
const WRITER_DEADLINE: Duration = Duration::from_secs(2);
/// Exchanges waiting to be written. More are dropped, newest first.
const QUEUE: usize = 32;
/// The most text of kept things a new session is told.
const NOTE_BYTES: u64 = 1_200;
/// How often the note is read again when nothing has changed it, and how
/// soon after the memory did not answer.
const REFRESH: Duration = Duration::from_secs(300);
const RETRY: Duration = Duration::from_secs(5);
/// How long starting waits for what is kept, so that the first session,
/// which opens within milliseconds, is told it. On lamp-4ace on 2026-10-13
/// the memory answered this in under a millisecond; a memory that is not
/// there costs nothing, and one that hangs costs this once.
const FIRST_READ: Duration = Duration::from_millis(300);
/// What a recall asks for: enough to answer from, and small enough to go
/// to the model in one answer ([`lamp_gemini::MAX_TOOL_JSON_BYTES`]).
const RECALL_ENTRIES: u64 = 6;
const RECALL_BYTES: u64 = 2_400;

/// The functions the model is told of when the voice has a memory.
pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: REMEMBER.into(),
            description: "Call this when someone asks you to remember something or keep it in mind for later. \
                          Give what to keep as one short sentence that will still make sense weeks from now, \
                          with their own words for names and numbers. Then say back in a few words what you kept, \
                          so they can put you right if you misheard."
                .into(),
            parameters: Some(json!({
                "type": "object",
                "properties": {"text": {"type": "string", "description": "What to keep, in one short sentence."}},
                "required": ["text"]
            })),
            silent: false,
        },
        Tool {
            name: RECALL.into(),
            description: "Call this when someone asks about something from an earlier conversation or an earlier \
                          day that you do not already know: what they told you, what you talked about yesterday, \
                          something they asked you to remember. Not for what was said in this conversation. \
                          If nothing is found, say you do not remember; never make something up. You cannot tell who \
                          is speaking now, so do not say 'you told me': just give what you remember, roughly \
                          and without dates or times."
                .into(),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "about": {"type": "string", "description": "One to four key words to look for, such as 'coffee' or 'dentist appointment'. Leave out for the most recent conversations."},
                    "days_ago": {"type": "integer", "description": "Only that day: 0 for today, 1 for yesterday. Leave out for any day."}
                }
            })),
            silent: false,
        },
        Tool {
            name: FORGET.into(),
            description: "Call this when someone asks you to forget the last thing they asked you to remember. \
                          Then tell them briefly that it is off your list, and do not use it again."
                .into(),
            parameters: None,
            silent: false,
        },
    ]
}

/// Whether the voice routes this function to the memory.
pub fn is_tool(name: &str) -> bool {
    matches!(name, REMEMBER | RECALL | FORGET)
}

/// What the model is told when the memory did not answer: words it can say.
pub fn gone() -> Value {
    json!({"error": "My memory is not answering right now, so I could not do that."})
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// Run one of the memory's functions and say what to tell the model; the
/// second value says whether the memory answered. Blocking for at most
/// `deadline`: for a thread of its own.
pub fn run(socket: &Path, call: &ToolCall, deadline: Duration) -> (Value, bool) {
    let now = unix_ms();
    let offset = crate::local_time::offset_here().unwrap_or(0);
    let Some(request) = request(call, now, offset) else {
        return (
            json!({"error": "There was nothing in that for me to keep."}),
            true,
        );
    };
    let Some(answer) = crate::tools::exchange(socket, &request, deadline) else {
        return (gone(), false);
    };
    let told = answer_for(&call.name, &answer, now, offset);
    if matches!(call.name.as_str(), REMEMBER | FORGET) && answer["ok"] == true {
        // The note a new session is told has changed.
        if let Some(jobs) = JOBS.get() {
            let _ = jobs.try_send(Job::Refresh);
        }
        if call.name == FORGET && !answer["removed"].is_null() {
            // Until the writer has read what is left, none of it is told.
            if let Ok(mut held) = NOTE.lock() {
                *held = note(&[]);
            }
            // What was said about it a moment ago is not told to the next
            // session either.
            if let Ok(mut thread) = crate::thread::THREAD.lock() {
                thread.forget();
            }
        }
    }
    match told {
        Some(told) if crate::tools::sendable(&told) => (told, true),
        _ => (gone(), false),
    }
}

/// The memory's request for a call, or nothing when it asks to keep nothing.
fn request(call: &ToolCall, now_unix_ms: u64, offset_seconds: i32) -> Option<Value> {
    Some(match call.name.as_str() {
        REMEMBER => {
            let text = call.args.get("text").and_then(Value::as_str)?.trim();
            if !text.chars().any(char::is_alphanumeric) {
                return None;
            }
            json!({"op":"remember","at_unix_ms":now_unix_ms,"text":text})
        }
        FORGET => json!({"op":"forget_last","kind":"kept"}),
        _ => {
            let about = call
                .args
                .get("about")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|about| !about.is_empty());
            let day = call
                .args
                .get("days_ago")
                .and_then(Value::as_u64)
                .filter(|days| *days <= 3_650)
                .map(|days| day_name(local_day(now_unix_ms, offset_seconds) - days as i64));
            json!({"op":"recall","query":about,"day":day,"limit":RECALL_ENTRIES,"max_bytes":RECALL_BYTES})
        }
    })
}

/// What to tell the model for the memory's answer. `None` for one that is
/// not the answer to that request.
fn answer_for(name: &str, answer: &Value, now_unix_ms: u64, offset_seconds: i32) -> Option<Value> {
    if answer.get("ok").and_then(Value::as_bool)? {
        return Some(match name {
            REMEMBER => {
                answer.get("id")?;
                json!({"remembered": true})
            }
            FORGET => match answer.get("removed")? {
                Value::Null => {
                    json!({"forgotten": false, "note": "There was nothing I had been asked to remember."})
                }
                _ => json!({"forgotten": true}),
            },
            _ => {
                let today = local_day(now_unix_ms, offset_seconds);
                let found: Vec<Value> = answer
                    .get("entries")?
                    .as_array()?
                    .iter()
                    .filter_map(|entry| {
                        let when = when(
                            today - local_day(entry.get("at_unix_ms")?.as_u64()?, offset_seconds),
                        );
                        Some(match entry.get("kind")?.as_str()? {
                            "kept" => json!({"when": when, "asked_to_remember": entry.get("text")?}),
                            _ => json!({"when": when, "heard": entry.get("heard")?, "you_said": entry.get("said")?}),
                        })
                    })
                    .collect();
                if found.is_empty() {
                    json!({"found": [], "note": "Nothing about that in what I remember."})
                } else {
                    json!({"found": found})
                }
            }
        });
    }
    match answer.get("error").and_then(Value::as_str)? {
        "empty" => Some(json!({"error": "There was nothing in that for me to keep."})),
        _ => None,
    }
}

/// The local day a moment falls on, as days since 1970-01-01.
fn local_day(unix_ms: u64, offset_seconds: i32) -> i64 {
    ((unix_ms / 1_000) as i64 + i64::from(offset_seconds)).div_euclid(86_400)
}

/// `YYYY-MM-DD`, as the memory names a day.
fn day_name(day: i64) -> String {
    let (year, month, day) = crate::local_time::civil(day);
    format!("{year:04}-{month:02}-{day:02}")
}

/// How long ago, the way a person would say it. A date and an hour read
/// out are what a machine does.
fn when(days_ago: i64) -> String {
    match days_ago {
        i64::MIN..=0 => "today".into(),
        1 => "yesterday".into(),
        2..=6 => format!("{days_ago} days ago"),
        7..=13 => "last week".into(),
        14..=59 => format!("{} weeks ago", days_ago / 7),
        _ => format!("about {} months ago", days_ago / 30),
    }
}

enum Job {
    Exchange {
        at_unix_ms: u64,
        heard: String,
        said: String,
    },
    Refresh,
}

/// The writer thread's queue, once a memory has been named.
static JOBS: OnceLock<SyncSender<Job>> = OnceLock::new();
/// What a new session is told of the things the lamp was asked to keep.
static NOTE: Mutex<String> = Mutex::new(String::new());
/// Requests in which the lamp was asked to keep or forget something.
static KEPT_OUT: Mutex<VecDeque<u64>> = Mutex::new(VecDeque::new());

/// Begin journalling to the memory on this socket, and keeping the note
/// for new sessions. Once for a process; a second call changes nothing.
pub fn start(socket: PathBuf) {
    let (jobs, work) = sync_channel(QUEUE);
    if JOBS.set(jobs).is_err() {
        return;
    }
    // Without this the first session after a start opened before the
    // writer had read anything: on lamp-4ace it was not told a thing kept
    // a minute earlier.
    let first = kept(&socket, FIRST_READ);
    if let Ok(mut held) = NOTE.lock() {
        *held = note(first.as_deref().unwrap_or_default());
    }
    let spawned = std::thread::Builder::new()
        .name("lamp-voice-memory".into())
        .spawn(move || writer(&socket, &work));
    if spawned.is_err() {
        eprintln!("lamp-voice provider: no thread for the memory; nothing is journalled");
    }
}

/// The request used a function that keeps or forgets: its exchange stays
/// out of the journal.
pub fn keep_out(request: u64) {
    if let Ok(mut kept_out) = KEPT_OUT.lock() {
        if kept_out.len() == 16 {
            kept_out.pop_front();
        }
        kept_out.push_back(request);
    }
}

/// The request is over: hand every finished exchange not handed on before
/// to the writer. Never waits; does nothing without a memory.
pub fn finished(request: u64) {
    let Some(jobs) = JOBS.get() else { return };
    let done = match crate::thread::THREAD.lock() {
        Ok(mut thread) => thread.finished(request, Instant::now()),
        Err(_) => return,
    };
    hand_on(done, unix_ms(), jobs);
}

/// Queue finished exchanges for the writer, but for those in which the lamp
/// was asked to keep or forget something.
fn hand_on(done: Vec<(u64, Duration, String, String)>, wall: u64, jobs: &SyncSender<Job>) {
    for (request, age, heard, said) in done {
        if KEPT_OUT
            .lock()
            .is_ok_and(|kept_out| kept_out.contains(&request))
        {
            continue;
        }
        let at_unix_ms = wall.saturating_sub(age.as_millis() as u64);
        if jobs
            .try_send(Job::Exchange {
                at_unix_ms,
                heard,
                said,
            })
            .is_err()
        {
            eprintln!(
                "lamp-voice provider: the memory's queue is full; an exchange is not journalled"
            );
        }
    }
}

/// The note for a session opened now: empty until a memory has been
/// named ([`start`]).
pub fn told() -> String {
    NOTE.lock().map(|note| note.clone()).unwrap_or_default()
}

/// What a session is told of its memory: that it has one and when to
/// consult it, then these kept things, oldest first. On lamp-4ace on
/// 2026-10-13 a new session told only the kept things answered "what did
/// we talk about earlier today?" with "I don't remember earlier
/// conversations" and never asked.
fn note(kept: &[String]) -> String {
    let mut note = String::from(
        "\n\nYour memory\nYou remember earlier days. When someone asks about an earlier conversation, or about something they told you before that you do not see here, call recall and answer from what it gives you. If it gives nothing, say you do not remember.\n",
    );
    if kept.is_empty() {
        return note;
    }
    note.push_str(
        "People in this room asked you to keep these in mind. Use one only when the person you are talking with brings its subject up, the way a colleague would. You cannot tell who is speaking, so never say \"you told me\": just use it. Never list them or bring them up yourself.\n",
    );
    for text in kept {
        note.push_str("- ");
        note.push_str(text);
        note.push('\n');
    }
    note
}

/// The newest kept things that fit the note, asked of the memory now.
fn kept(socket: &Path, deadline: Duration) -> Option<Vec<String>> {
    let line = json!({"op":"kept","max_bytes":NOTE_BYTES});
    let answer = crate::tools::exchange(socket, &line, deadline)?;
    Some(
        answer
            .get("entries")?
            .as_array()?
            .iter()
            .filter_map(|entry| Some(entry.get("text")?.as_str()?.to_owned()))
            .collect(),
    )
}

fn writer(socket: &Path, jobs: &Receiver<Job>) {
    let mut refresh_at = Instant::now();
    // Said when it changes, not with every exchange.
    let mut answering = None;
    let mut heard_from = |now_answering: bool| {
        if answering != Some(now_answering) {
            answering = Some(now_answering);
            eprintln!(
                "lamp-voice provider: the memory is {}",
                if now_answering {
                    "answering"
                } else {
                    "not answering; exchanges are not journalled until it does"
                }
            );
        }
    };
    loop {
        let wait = refresh_at.saturating_duration_since(Instant::now());
        match jobs.recv_timeout(wait) {
            Ok(Job::Exchange {
                at_unix_ms,
                heard,
                said,
            }) => {
                let line = json!({"op":"append","at_unix_ms":at_unix_ms,"heard":heard,"said":said});
                let taken = crate::tools::exchange(socket, &line, WRITER_DEADLINE)
                    .is_some_and(|answer| answer["ok"] == true);
                heard_from(taken);
            }
            Ok(Job::Refresh) => refresh_at = Instant::now(),
            Err(RecvTimeoutError::Timeout) => {
                let kept = kept(socket, WRITER_DEADLINE);
                heard_from(kept.is_some());
                refresh_at = Instant::now() + if kept.is_some() { REFRESH } else { RETRY };
                if let (Some(kept), Ok(mut held)) = (kept, NOTE.lock()) {
                    *held = note(&kept);
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
    };

    // 2026-10-13 22:30 in Ho Chi Minh City, seven hours east of UTC.
    const NOW: u64 = 1_791_905_400_000;
    const EAST: i32 = 25_200;
    const DAY: u64 = 86_400_000;

    fn call(name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: "c1".into(),
            name: name.into(),
            args,
        }
    }

    /// A memory that answers `requests` request lines, each with what
    /// `answer` makes of it, and gives back the requests it read.
    fn memory(
        name: &str,
        requests: usize,
        answer: fn(&Value) -> Value,
    ) -> (PathBuf, std::thread::JoinHandle<Vec<Value>>) {
        let socket =
            std::env::temp_dir().join(format!("lamp-remember-{name}-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).unwrap();
        let served = std::thread::spawn(move || {
            let mut read = Vec::new();
            for _ in 0..requests {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                stream
                    .write_all(format!("{}\n", answer(&request)).as_bytes())
                    .unwrap();
                read.push(request);
            }
            read
        });
        (socket, served)
    }

    #[test]
    fn the_three_functions_are_ones_the_link_takes_and_no_others_are_the_memorys() {
        let tools = tools();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            [REMEMBER, RECALL, FORGET]
        );
        for tool in &tools {
            tool.validate().unwrap();
            assert!(is_tool(&tool.name) && !tool.silent);
        }
        assert!(!is_tool("not_for_me") && !is_tool("volume_up") && !is_tool(""));
    }

    #[test]
    fn a_call_becomes_the_memorys_request_and_nothing_to_keep_becomes_none() {
        assert_eq!(
            request(
                &call(REMEMBER, json!({"text":"  takes coffee black "})),
                NOW,
                EAST
            ),
            Some(json!({"op":"remember","at_unix_ms":NOW,"text":"takes coffee black"}))
        );
        for nothing in [json!({}), json!({"text":" ... "}), json!({"text":7})] {
            assert_eq!(request(&call(REMEMBER, nothing), NOW, EAST), None);
        }
        assert_eq!(
            request(&call(FORGET, json!({})), NOW, EAST),
            Some(json!({"op":"forget_last","kind":"kept"}))
        );
        // Yesterday is the local day before, not UTC's: 22:30 here is
        // 15:30 UTC on the same date, and 02:00 here is the day before in UTC.
        assert_eq!(
            request(
                &call(RECALL, json!({"about":" coffee ","days_ago":1})),
                NOW,
                EAST
            ),
            Some(
                json!({"op":"recall","query":"coffee","day":"2026-10-12","limit":6,"max_bytes":2400})
            )
        );
        let early = NOW + 4 * 3_600_000;
        assert_eq!(
            request(&call(RECALL, json!({"days_ago":0})), early, EAST).unwrap()["day"],
            "2026-10-14"
        );
        assert_eq!(
            request(
                &call(RECALL, json!({"about":"","days_ago":99_999})),
                NOW,
                EAST
            ),
            Some(json!({"op":"recall","query":null,"day":null,"limit":6,"max_bytes":2400}))
        );
    }

    #[test]
    fn what_was_found_is_told_with_how_long_ago_and_never_a_clock_time() {
        let answer = json!({"ok":true,"entries":[
            {"kind":"exchange","id":1,"at_unix_ms":NOW - 20 * DAY,"heard":"How do I take my coffee?","said":"Black."},
            {"kind":"kept","id":2,"at_unix_ms":NOW - DAY,"text":"takes coffee black"},
            {"kind":"exchange","id":3,"at_unix_ms":NOW - 3_600_000,"heard":"And tea?","said":"No idea."}
        ]});
        assert_eq!(
            answer_for(RECALL, &answer, NOW, EAST),
            Some(json!({"found":[
                {"when":"2 weeks ago","heard":"How do I take my coffee?","you_said":"Black."},
                {"when":"yesterday","asked_to_remember":"takes coffee black"},
                {"when":"today","heard":"And tea?","you_said":"No idea."}
            ]}))
        );
        assert_eq!(
            answer_for(RECALL, &json!({"ok":true,"entries":[]}), NOW, EAST),
            Some(json!({"found":[],"note":"Nothing about that in what I remember."}))
        );
        assert_eq!(
            [
                "today",
                "yesterday",
                "4 days ago",
                "last week",
                "3 weeks ago",
                "about 3 months ago"
            ],
            [0, 1, 4, 9, 21, 100].map(when)
        );
    }

    #[test]
    fn keeping_and_forgetting_are_told_as_what_the_memory_established() {
        assert_eq!(
            answer_for(REMEMBER, &json!({"ok":true,"id":4}), NOW, EAST),
            Some(json!({"remembered":true}))
        );
        assert_eq!(
            answer_for(
                FORGET,
                &json!({"ok":true,"removed":{"kind":"kept","id":4,"at_unix_ms":1}}),
                NOW,
                EAST
            ),
            Some(json!({"forgotten":true}))
        );
        assert_eq!(
            answer_for(FORGET, &json!({"ok":true,"removed":null}), NOW, EAST).unwrap()["forgotten"],
            false
        );
        assert_eq!(
            answer_for(
                REMEMBER,
                &json!({"ok":false,"error":"empty","message":"m"}),
                NOW,
                EAST
            ),
            Some(json!({"error":"There was nothing in that for me to keep."}))
        );
        // An answer to something else, or a refusal that says nothing a
        // person could be told, is no answer.
        for other in [
            json!({"ok":true}),
            json!({"ok":false,"error":"storage","message":"disk"}),
            json!({"entries":[]}),
            json!("ok"),
        ] {
            assert_eq!(answer_for(REMEMBER, &other, NOW, EAST), None, "{other}");
        }
    }

    #[test]
    fn a_call_is_one_request_to_the_memory_and_a_memory_that_is_gone_is_said_so() {
        let (socket, served) = memory("run", 1, |_| json!({"ok":true,"id":1}));
        let (told, answered) = run(
            &socket,
            &call(REMEMBER, json!({"text":"sits by the window"})),
            Duration::from_secs(2),
        );
        assert_eq!((told, answered), (json!({"remembered":true}), true));
        let read = served.join().unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0]["op"], "remember");
        assert_eq!(read[0]["text"], "sits by the window");
        std::fs::remove_file(&socket).unwrap();
        // Nobody there now.
        let (told, answered) = run(
            &socket,
            &call(RECALL, json!({"about":"window"})),
            Duration::from_millis(300),
        );
        assert_eq!((told, answered), (gone(), false));
        // Nothing to keep never reaches the memory.
        let (told, answered) = run(
            &socket,
            &call(REMEMBER, json!({})),
            Duration::from_millis(300),
        );
        assert!(answered && told["error"].is_string());
    }

    #[test]
    fn an_answer_too_long_for_the_link_is_told_as_no_answer() {
        let (socket, served) = memory("long", 1, |_| {
            let long = "x".repeat(lamp_gemini::MAX_TOOL_JSON_BYTES);
            json!({"ok":true,"entries":[{"kind":"kept","id":1,"at_unix_ms":NOW,"text":long}]})
        });
        let (told, answered) = run(&socket, &call(RECALL, json!({})), Duration::from_secs(2));
        assert_eq!((told, answered), (gone(), false));
        served.join().unwrap();
        let _ = std::fs::remove_file(&socket);
    }

    #[test]
    fn finished_exchanges_are_queued_with_when_they_began_but_for_those_kept_out() {
        let (jobs, work) = sync_channel(2);
        // Requests no other test names.
        let (plain, private, late) = (u64::MAX - 40, u64::MAX - 41, u64::MAX - 42);
        keep_out(private);
        let done = |request| {
            (
                request,
                Duration::from_secs(3),
                "asked".to_owned(),
                "said".to_owned(),
            )
        };
        hand_on(
            vec![done(plain), done(private), done(late), done(late)],
            NOW,
            &jobs,
        );
        // Two fit; the private one was never queued, and the fourth found
        // the queue full and was dropped without waiting.
        for _ in 0..2 {
            let Ok(Job::Exchange {
                at_unix_ms,
                heard,
                said,
            }) = work.try_recv()
            else {
                panic!("an exchange was expected");
            };
            assert_eq!(
                (at_unix_ms, heard.as_str(), said.as_str()),
                (NOW - 3_000, "asked", "said")
            );
        }
        assert!(work.try_recv().is_err());
    }

    #[test]
    fn the_writer_reads_what_is_kept_then_journals_and_ends_with_its_queue() {
        let (socket, served) = memory("writer", 2, |request| match request["op"].as_str() {
            Some("kept") => json!({"ok":true,"entries":[
                {"id":1,"at_unix_ms":1,"text":"takes coffee black, only this test says so"}
            ]}),
            _ => json!({"ok":true,"id":2}),
        });
        let (jobs, work) = sync_channel(4);
        let path = socket.clone();
        let writer = std::thread::spawn(move || writer(&path, &work));
        // It asks what is kept before anything else, with nothing queued.
        let waited = Instant::now();
        while !told().contains("only this test says so") {
            assert!(
                waited.elapsed() < Duration::from_secs(5),
                "the note never came"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(told().starts_with("\n\nYour memory\n"));
        jobs.send(Job::Exchange {
            at_unix_ms: NOW,
            heard: "How do I take my coffee?".into(),
            said: "Black.".into(),
        })
        .unwrap();
        let read = served.join().unwrap();
        assert_eq!(read[0], json!({"op":"kept","max_bytes":NOTE_BYTES}));
        assert_eq!(
            read[1],
            json!({"op":"append","at_unix_ms":NOW,"heard":"How do I take my coffee?","said":"Black."})
        );
        drop(jobs);
        writer.join().unwrap();
        let _ = std::fs::remove_file(&socket);
    }

    #[test]
    fn the_note_says_there_is_a_memory_and_lists_what_is_kept() {
        let bare = note(&[]);
        assert!(
            bare.starts_with("\n\nYour memory\n") && bare.contains("call recall"),
            "{bare}"
        );
        assert!(!bare.contains("asked you to keep"), "{bare}");
        let told = note(&["takes coffee black".into(), "sits by the window".into()]);
        assert!(told.starts_with(&bare), "{told}");
        assert!(told.contains("never say \"you told me\""), "{told}");
        assert!(
            told.ends_with("- takes coffee black\n- sits by the window\n"),
            "{told}"
        );
    }

    #[test]
    fn what_is_kept_is_read_in_one_request_and_a_memory_that_is_gone_gives_none() {
        let (socket, served) = memory(
            "kept",
            1,
            |_| json!({"ok":true,"entries":[{"id":1,"at_unix_ms":1,"text":"a"},{"id":2,"at_unix_ms":2,"text":"b"}]}),
        );
        assert_eq!(
            kept(&socket, Duration::from_secs(2)),
            Some(vec!["a".to_owned(), "b".to_owned()])
        );
        assert_eq!(
            served.join().unwrap(),
            [json!({"op":"kept","max_bytes":NOTE_BYTES})]
        );
        std::fs::remove_file(&socket).unwrap();
        let started = Instant::now();
        assert_eq!(kept(&socket, FIRST_READ), None);
        assert!(
            started.elapsed() < FIRST_READ,
            "a socket that is not there costs no wait"
        );
    }
}
