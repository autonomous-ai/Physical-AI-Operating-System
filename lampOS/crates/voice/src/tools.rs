//! What the model may do other than speak.
//!
//! A Live session can be told of functions and asks for one by name
//! (`lamp-gemini`, `SessionConfig::tools`). The voice declares two kinds and
//! runs neither itself:
//!
//! - **`not_for_me`**, its own. A session has had only one thing to do with
//!   what it hears: answer it. On lamp-4ace on 2026-10-13, of 23 new
//!   sessions that heard two people talking to each other, 7 spoke words to
//!   them and 3 more made a sound, whether or not the session had been told
//!   how the room was. With this function the model has something else to
//!   do with speech that is not for it, and when it does it, whatever sound
//!   the service still sends for that request is dropped before the speaker.
//!   Off unless the service's environment says `on`: what it does to those
//!   counts, and what it costs an ordinary answer, has not been measured.
//! - **The executive's**, read from the file it keeps in its directory
//!   (`tools.json`) each time a session opens, so that they appear when the
//!   executive starts and go when its directory does. A call is sent to its
//!   socket (`call.sock`) and waited for no longer than [`ASK_DEADLINE`];
//!   with the executive gone, late or unreadable the model is told so in
//!   words it can say, and the voice goes on. The voice never waits on the
//!   executive for anything else.
//!
//! Every call is answered, whatever becomes of it: the service generates
//! nothing more for a request until its calls are.

use lamp_gemini::{Tool, ToolCall};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Where the coordinator reads whether the model may say a thing was not
/// for it. `on` only.
pub const ENVIRONMENT: &str = "LAMP_TOOLS";
/// The provider worker's flag for it.
pub const FLAG: &str = "--tools";
/// Where the coordinator reads the executive's directory: an absolute path.
pub const ACTIONS_ENVIRONMENT: &str = "LAMP_ACTIONS_DIR";
/// The provider worker's flag for it, followed by the path.
pub const ACTIONS_FLAG: &str = "--actions-dir";

/// The function the model calls instead of speaking.
pub const NOT_FOR_ME: &str = "not_for_me";
/// The executive's list of functions, in its directory.
pub const CATALOG_FILE: &str = "tools.json";
/// The executive's socket, in its directory.
pub const CALL_SOCKET: &str = "call.sock";
/// The longest a call waits for the executive, connecting included. A
/// person hears nothing while it waits.
pub const ASK_DEADLINE: Duration = Duration::from_millis(1_500);
/// The most the executive's list or one of its answers may be.
const MAX_CATALOG_BYTES: u64 = 65_536;
const MAX_ANSWER_BYTES: u64 = 8_192;

/// What the worker was told by its flags.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Settings {
    /// The model may call [`NOT_FOR_ME`].
    pub decline: bool,
    /// The executive's directory.
    pub exec: Option<PathBuf>,
}

/// The provider worker's arguments for what the environment says.
pub fn arguments_from_environment() -> Vec<String> {
    arguments(
        std::env::var(ENVIRONMENT).ok().as_deref(),
        std::env::var(ACTIONS_ENVIRONMENT).ok().as_deref(),
    )
}

fn arguments(decline: Option<&str>, exec: Option<&str>) -> Vec<String> {
    let mut arguments = Vec::new();
    if decline == Some("on") {
        arguments.push(FLAG.to_owned());
    }
    if let Some(directory) = exec.filter(|path| Path::new(path).is_absolute()) {
        arguments.push(ACTIONS_FLAG.to_owned());
        arguments.push(directory.to_owned());
    }
    arguments
}

/// Take a leading `--tools` and `--actions-dir PATH` off a worker's flags.
pub fn take(flags: &[String]) -> std::io::Result<(Settings, &[String])> {
    let mut settings = Settings::default();
    let mut flags = flags;
    if let [flag, rest @ ..] = flags
        && flag == FLAG
    {
        settings.decline = true;
        flags = rest;
    }
    if let [flag, rest @ ..] = flags
        && flag == ACTIONS_FLAG
    {
        let [directory, rest @ ..] = rest else {
            return Err(std::io::Error::other("--actions-dir needs a path"));
        };
        if !Path::new(directory).is_absolute() {
            return Err(std::io::Error::other("--actions-dir must be absolute"));
        }
        settings.exec = Some(PathBuf::from(directory));
        flags = rest;
    }
    Ok((settings, flags))
}

fn not_for_me() -> Tool {
    Tool {
        name: NOT_FOR_ME.into(),
        description: "Call this, and say nothing at all, when what you just heard was not said to you: \
                      people talking to each other, someone on a call or reading aloud, a television, \
                      a noise. It is how you stay out of a conversation that is not yours."
            .into(),
        parameters: None,
        // Declared as one nothing is said on account of. Declared as an
        // ordinary function, on lamp-4ace on 2026-10-13, its answer made
        // the model speak ("I'm sorry, I missed that. Could you please
        // repeat it?"), talk nonsense, or call it again eleven times.
        silent: true,
    }
}

/// The functions a session opened now is told of: the voice's own if it is
/// switched on, then the executive's if its list is there and reads. A name
/// the voice uses itself is left out of the executive's.
pub fn declared(settings: &Settings) -> Vec<Tool> {
    let mut tools = Vec::new();
    if settings.decline {
        tools.push(not_for_me());
    }
    if let Some(directory) = &settings.exec {
        let room = lamp_gemini::MAX_TOOLS - tools.len();
        tools.extend(
            catalog(&directory.join(CATALOG_FILE))
                .into_iter()
                .filter(|tool| tool.name != NOT_FOR_ME)
                .take(room),
        );
    }
    tools
}

/// The executive's list, or nothing: a missing file, one too large, one
/// that is not `{"tools":[...]}`, or one with a function the provider link
/// would refuse or a name twice. All or nothing, so that a session is
/// never opened with half of what the executive can do.
fn catalog(path: &Path) -> Vec<Tool> {
    #[derive(serde::Deserialize)]
    struct Catalog {
        tools: Vec<Tool>,
    }
    let read = || -> Option<Vec<Tool>> {
        let file = std::fs::File::open(path).ok()?;
        let mut text = String::new();
        file.take(MAX_CATALOG_BYTES + 1)
            .read_to_string(&mut text)
            .ok()?;
        if text.len() as u64 > MAX_CATALOG_BYTES {
            return None;
        }
        let tools = serde_json::from_str::<Catalog>(&text).ok()?.tools;
        let sound = tools.iter().enumerate().all(|(index, tool)| {
            tool.validate().is_ok() && tools[..index].iter().all(|other| other.name != tool.name)
        });
        sound.then_some(tools)
    };
    read().unwrap_or_default()
}

/// What to do with one call.
#[derive(Debug, Eq, PartialEq)]
pub enum Route {
    /// The model said the request was not for it: answer the call, and let
    /// none of the request's sound out.
    Decline,
    /// Ask the executive in this directory.
    Executive(PathBuf),
    /// Nobody here runs that.
    Nobody,
}

pub fn route(settings: &Settings, name: &str) -> Route {
    match (&settings.exec, name) {
        (_, NOT_FOR_ME) if settings.decline => Route::Decline,
        (Some(directory), name) if name != NOT_FOR_ME => Route::Executive(directory.clone()),
        _ => Route::Nobody,
    }
}

/// What the model is told when a call could not be run: words it can say.
pub fn unavailable() -> Value {
    json!({"error": "That part of me is not running right now, so I could not do it."})
}

/// What the model is told when it declines.
pub fn declined() -> Value {
    json!({"ok": true})
}

/// Ask the executive to run one call and wait at most `deadline` for its
/// answer, connecting included. Blocking: for a thread of its own. Always
/// returns what to tell the model; the second value says whether the
/// executive answered.
///
/// One line out, `{"op":"call","name":...,"args":{...}}`, and one line
/// back: `{"ok":true,"result":{...}}`, whose result goes to the model as it
/// is, or `{"ok":false,"error":"..."}`, whose words do.
pub fn ask(directory: &Path, call: &ToolCall, deadline: Duration) -> (Value, bool) {
    let started = Instant::now();
    let left = || {
        deadline
            .checked_sub(started.elapsed())
            .filter(|left| !left.is_zero())
    };
    let asked = || -> Option<Value> {
        // A local socket connects at once or not at all: there is no wait
        // to bound here that the deadlines below do not bound.
        let mut stream = UnixStream::connect(directory.join(CALL_SOCKET)).ok()?;
        stream.set_write_timeout(Some(left()?)).ok()?;
        let mut line =
            serde_json::to_vec(&json!({"op":"call","name":call.name,"args":call.args})).ok()?;
        line.push(b'\n');
        stream.write_all(&line).ok()?;
        let mut answer = Vec::new();
        let mut reader = BufReader::new(stream.try_clone().ok()?.take(MAX_ANSWER_BYTES));
        loop {
            // The whole answer inside what is left, however it is cut up.
            stream.set_read_timeout(Some(left()?)).ok()?;
            let read = reader.read_until(b'\n', &mut answer).ok()?;
            if answer.last() == Some(&b'\n') {
                break;
            }
            if read == 0 {
                return None;
            }
        }
        let answer: Value = serde_json::from_slice(&answer).ok()?;
        match answer.get("ok").and_then(Value::as_bool)? {
            true => Some(answer.get("result").cloned().unwrap_or_else(|| json!({}))),
            false => Some(json!({"error": answer.get("error").and_then(Value::as_str)?})),
        }
    };
    match asked() {
        // What the link would refuse to send is never offered to it: an
        // answer that is not sent leaves the service waiting on the call.
        Some(told) if told.to_string().len() + 16 <= lamp_gemini::MAX_TOOL_JSON_BYTES => {
            (told, true)
        }
        _ => (unavailable(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lamp-tools-{name}-{}-{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn call(name: &str) -> ToolCall {
        ToolCall {
            id: "c1".into(),
            name: name.into(),
            args: json!({"colour":"blue"}),
        }
    }
    /// An executive that reads one request and answers what it is given,
    /// after a wait. Returns the request it read.
    fn executive(
        directory: &Path,
        answer: &'static [u8],
        wait: Duration,
    ) -> std::thread::JoinHandle<Value> {
        let listener = UnixListener::bind(directory.join(CALL_SOCKET)).unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            std::thread::sleep(wait);
            let _ = stream.write_all(answer);
            serde_json::from_str(&line).unwrap()
        })
    }

    #[test]
    fn only_on_and_an_absolute_path_become_flags_and_they_are_taken_from_the_front() {
        assert!(arguments(None, None).is_empty());
        assert!(arguments(Some("yes"), Some("relative/dir")).is_empty());
        let both = arguments(Some("on"), Some("/run/lampos-actions"));
        assert_eq!(both, [FLAG, ACTIONS_FLAG, "/run/lampos-actions"]);
        let mut flags = both.clone();
        flags.push("rest".into());
        let (settings, rest) = take(&flags).unwrap();
        assert_eq!(
            settings,
            Settings {
                decline: true,
                exec: Some("/run/lampos-actions".into())
            }
        );
        assert_eq!(rest, ["rest"]);
        let (settings, rest) = take(&flags[1..]).unwrap();
        assert!(!settings.decline && settings.exec.is_some());
        assert_eq!(rest, ["rest"]);
        let (settings, rest) = take(&flags[3..]).unwrap();
        assert_eq!(settings, Settings::default());
        assert_eq!(rest, ["rest"]);
        assert!(take(&[ACTIONS_FLAG.to_owned()]).is_err());
        assert!(take(&[ACTIONS_FLAG.to_owned(), "relative".to_owned()]).is_err());
    }

    #[test]
    fn a_session_is_told_of_the_voices_own_function_and_of_a_whole_readable_catalog_or_none_of_it()
    {
        assert!(declared(&Settings::default()).is_empty());
        let own = Settings {
            decline: true,
            exec: None,
        };
        let told = declared(&own);
        assert_eq!(told.len(), 1);
        assert_eq!(told[0].name, NOT_FOR_ME);
        // What the link will be given must be something it accepts.
        assert!(told[0].validate().is_ok());

        let dir = directory("catalog");
        let with = Settings {
            decline: true,
            exec: Some(dir.clone()),
        };
        // No file yet: the executive has not started.
        assert_eq!(declared(&with).len(), 1);
        let write = |text: &str| std::fs::write(dir.join(CATALOG_FILE), text).unwrap();
        write(
            r#"{"tools":[{"name":"light_on","description":"Turn the light on."},
            {"name":"light_colour","description":"Set the colour.","parameters":{"type":"object","properties":{"colour":{"type":"string","enum":["red","blue"]}},"required":["colour"]}},
            {"name":"not_for_me","description":"A name the voice keeps."}]}"#,
        );
        let names: Vec<String> = declared(&with).into_iter().map(|tool| tool.name).collect();
        assert_eq!(names, [NOT_FOR_ME, "light_on", "light_colour"]);
        // The executive's alone, when the voice's own is off.
        let alone = Settings {
            decline: false,
            exec: Some(dir.clone()),
        };
        assert_eq!(declared(&alone).len(), 2);
        // One function the link would refuse, a name twice, a file that is
        // not the shape or not JSON, a file too large: none of it is told.
        for unsound in [
            r#"{"tools":[{"name":"light_on","description":"ok"},{"name":"Light Off","description":"bad name"}]}"#,
            r#"{"tools":[{"name":"light_on","description":"a"},{"name":"light_on","description":"b"}]}"#,
            r#"{"tools":[{"name":"light_on","description":"a","extra":1}]}"#,
            r#"[{"name":"light_on","description":"a"}]"#,
            "not json",
        ] {
            write(unsound);
            assert_eq!(declared(&with).len(), 1, "{unsound}");
        }
        write(&format!(
            r#"{{"tools":[{{"name":"light_on","description":"a"}}],"pad":"{}"}}"#,
            "p".repeat(MAX_CATALOG_BYTES as usize)
        ));
        assert_eq!(declared(&with).len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_call_goes_to_whoever_runs_it_and_to_nobody_when_that_is_switched_off() {
        let dir = PathBuf::from("/run/lampos-actions");
        let all = Settings {
            decline: true,
            exec: Some(dir.clone()),
        };
        assert_eq!(route(&all, NOT_FOR_ME), Route::Decline);
        assert_eq!(route(&all, "light_on"), Route::Executive(dir.clone()));
        let own = Settings {
            decline: true,
            exec: None,
        };
        assert_eq!(route(&own, "light_on"), Route::Nobody);
        // Not declared, so not meant: it is never handed to the executive.
        let exec = Settings {
            decline: false,
            exec: Some(dir),
        };
        assert_eq!(route(&exec, NOT_FOR_ME), Route::Nobody);
        assert_eq!(route(&Settings::default(), "light_on"), Route::Nobody);
    }

    #[test]
    fn the_executive_is_asked_in_one_line_and_its_result_or_its_words_go_to_the_model() {
        let dir = directory("ask");
        let served = executive(
            &dir,
            b"{\"ok\":true,\"result\":{\"light\":\"blue\"}}\n",
            Duration::ZERO,
        );
        let (told, answered) = ask(&dir, &call("light_colour"), ASK_DEADLINE);
        assert_eq!(told, json!({"light":"blue"}));
        assert!(answered);
        assert_eq!(
            served.join().unwrap(),
            json!({"op":"call","name":"light_colour","args":{"colour":"blue"}})
        );
        std::fs::remove_file(dir.join(CALL_SOCKET)).unwrap();

        let served = executive(
            &dir,
            b"{\"ok\":false,\"error\":\"The light is already at its brightest.\"}\n",
            Duration::ZERO,
        );
        let (told, answered) = ask(&dir, &call("light_brighter"), ASK_DEADLINE);
        assert_eq!(
            told,
            json!({"error":"The light is already at its brightest."})
        );
        assert!(answered);
        served.join().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The executive gone, late, cut off or talking nonsense: the model is
    /// told in words it can say, inside the deadline, and nothing else
    /// happens to the voice.
    #[test]
    fn an_executive_that_is_gone_late_or_unreadable_costs_one_deadline_and_an_honest_answer() {
        let dir = directory("gone");
        // No socket at all.
        let started = Instant::now();
        assert_eq!(
            ask(&dir, &call("light_on"), ASK_DEADLINE),
            (unavailable(), false)
        );
        assert!(started.elapsed() < Duration::from_millis(500));
        // A socket file nobody listens on.
        drop(UnixListener::bind(dir.join(CALL_SOCKET)).unwrap());
        assert_eq!(
            ask(&dir, &call("light_on"), ASK_DEADLINE),
            (unavailable(), false)
        );
        std::fs::remove_file(dir.join(CALL_SOCKET)).unwrap();
        // One that answers after the deadline.
        let deadline = Duration::from_millis(200);
        let served = executive(
            &dir,
            b"{\"ok\":true,\"result\":{}}\n",
            Duration::from_millis(600),
        );
        let started = Instant::now();
        assert_eq!(
            ask(&dir, &call("light_on"), deadline),
            (unavailable(), false)
        );
        let waited = started.elapsed();
        assert!(
            waited >= deadline && waited < Duration::from_millis(500),
            "{waited:?}"
        );
        served.join().unwrap();
        std::fs::remove_file(dir.join(CALL_SOCKET)).unwrap();
        // An answer larger than a session will carry to the model.
        let large: &'static [u8] = Box::leak(
            format!(
                "{{\"ok\":true,\"result\":{{\"pad\":\"{}\"}}}}\n",
                "p".repeat(lamp_gemini::MAX_TOOL_JSON_BYTES)
            )
            .into_bytes()
            .into_boxed_slice(),
        );
        let served = executive(&dir, large, Duration::ZERO);
        assert_eq!(
            ask(&dir, &call("light_on"), ASK_DEADLINE),
            (unavailable(), false)
        );
        served.join().unwrap();
        std::fs::remove_file(dir.join(CALL_SOCKET)).unwrap();
        // One that hangs up without a whole line, and three that answer
        // something that is not an answer.
        for nonsense in [
            b"{\"ok\":true".as_slice(),
            b"hello\n".as_slice(),
            b"{\"result\":{}}\n".as_slice(),
            b"{\"ok\":false}\n".as_slice(),
        ] {
            let served = executive(&dir, nonsense, Duration::ZERO);
            assert_eq!(
                ask(&dir, &call("light_on"), ASK_DEADLINE),
                (unavailable(), false),
                "{}",
                String::from_utf8_lossy(nonsense)
            );
            served.join().unwrap();
            std::fs::remove_file(dir.join(CALL_SOCKET)).unwrap();
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
