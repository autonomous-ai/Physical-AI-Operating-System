//! Work handed to an agent on another computer: the voice's side of
//! `lamp-tasks` (`docs/tasks.md`), which starts a Claude Code session there
//! through Harness.
//!
//! Two functions for the model. [`START`] prepares the task: nothing runs,
//! and the model is told to read it back and ask. [`CONFIRM`] starts it, and
//! only the voice decides whether it may: the person must have said
//! something to the lamp after the task was prepared (a request the lamp
//! heard, not one it wrote itself), within [`CONFIRM_WITHIN`]. The model
//! hears the yes; the voice checks that there was one to hear. The
//! confirmation goes on the tasks part's own socket for the voice, which
//! nothing the model can name reaches.
//!
//! Off unless set: `LAMP_TASKS_DIR=/absolute/directory` holding `lamp-tasks`'s
//! `request.sock` and `voice.sock`, passed to the provider worker as
//! `--tasks-dir DIR`. The target is the tasks part's alias `office` unless
//! `LAMP_TASKS_TARGET` names another.
//!
//! Hearing back: the tasks part watches what it started (on unless `serve --no-watch`),
//! [`crate::task_news`] tells the person unasked when the agent has
//! finished or asks something, and [`CHECK`] answers "how's it going?" from
//! what was last seen, at once. What the person heard through [`CHECK`] is
//! kept as told, so it is not told again unasked.
use lamp_gemini::{Tool, ToolCall};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

/// Where the coordinator reads the tasks part's directory.
pub const ENVIRONMENT: &str = "LAMP_TASKS_DIR";
/// Where it reads the target's alias, if not [`DEFAULT_TARGET`].
pub const TARGET_ENVIRONMENT: &str = "LAMP_TASKS_TARGET";
/// The provider worker's flags for them.
pub const FLAG: &str = "--tasks-dir";
pub const TARGET_FLAG: &str = "--tasks-target";
pub const DEFAULT_TARGET: &str = "office";
/// The model's two functions.
pub const START: &str = "start_agent_task";
pub const CONFIRM: &str = "confirm_agent_task";
pub const CHECK: &str = "check_agent_task";
/// A yes counts this long after the task was prepared.
pub const CONFIRM_WITHIN: Duration = Duration::from_secs(120);
/// The longest task text the tasks part takes.
const MOST_TASK_BYTES: usize = 1_000;

/// The task prepared and not yet started.
#[derive(Clone, Debug)]
struct Pending {
    id: u64,
    digest: String,
    request: u64,
    at: Instant,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: START.into(),
            description: "Prepare a piece of computer work for Claude Code on the person's office \
                          computer: writing or changing code, documents or files there. Call it only \
                          when the person asks for such work to be done there. Nothing starts yet: \
                          then say the task back in one short sentence and ask whether to start it."
                .into(),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "The work, in the person's own words, complete enough to do without asking."
                    }
                },
                "required": ["task"]
            })),
            silent: false,
        },
        Tool {
            name: CONFIRM.into(),
            description: "Start the task you prepared with start_agent_task. Call it only right \
                          after the person has said yes to the task you read back; never on your \
                          own, and not if they said no or something else."
                .into(),
            parameters: None,
            silent: false,
        },
        Tool {
            name: CHECK.into(),
            description: "How the latest work you handed over is going, and what the agent came \
                          back with if it has finished. Call it when the person asks about that \
                          work (\"how's it going?\", \"what did Claude Code say?\")."
                .into(),
            parameters: None,
            silent: false,
        },
    ]
}

pub fn is_tool(name: &str) -> bool {
    name == START || name == CONFIRM || name == CHECK
}

/// What the tasks part is reached by.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Place {
    pub directory: PathBuf,
    pub target: String,
}

/// The provider worker's arguments for what the environment says.
pub fn arguments_from_environment() -> Vec<String> {
    arguments(
        std::env::var(ENVIRONMENT).ok().as_deref(),
        std::env::var(TARGET_ENVIRONMENT).ok().as_deref(),
    )
}

fn arguments(directory: Option<&str>, target: Option<&str>) -> Vec<String> {
    let Some(directory) = directory.filter(|path| Path::new(path).is_absolute()) else {
        return Vec::new();
    };
    let mut arguments = vec![FLAG.to_owned(), directory.to_owned()];
    if let Some(target) = target.filter(|target| alias(target)) {
        arguments.push(TARGET_FLAG.to_owned());
        arguments.push(target.to_owned());
    }
    arguments
}

fn alias(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Take a leading `--tasks-dir DIR [--tasks-target ALIAS]` off the flags.
pub fn take(flags: &[String]) -> std::io::Result<(Option<Place>, &[String])> {
    let [flag, rest @ ..] = flags else {
        return Ok((None, flags));
    };
    if flag != FLAG {
        return Ok((None, flags));
    }
    let [directory, rest @ ..] = rest else {
        return Err(std::io::Error::other("--tasks-dir needs a path"));
    };
    if !Path::new(directory).is_absolute() {
        return Err(std::io::Error::other("--tasks-dir must be absolute"));
    }
    let (target, rest) = match rest {
        [flag, target, rest @ ..] if flag == TARGET_FLAG => {
            if !alias(target) {
                return Err(std::io::Error::other("--tasks-target is not an alias"));
            }
            (target.clone(), rest)
        }
        _ => (DEFAULT_TARGET.to_owned(), rest),
    };
    Ok((
        Some(Place {
            directory: PathBuf::from(directory),
            target,
        }),
        rest,
    ))
}

/// Whether a yes in request `request` may start what was prepared: it came
/// in a request after the one that prepared it, one the lamp heard rather
/// than wrote, soon enough.
fn may_confirm(pending: &Pending, request: u64, heard: bool, now: Instant) -> bool {
    heard && request > pending.request && now.duration_since(pending.at) <= CONFIRM_WITHIN
}

/// Run one call. Blocking, for a thread of its own. `heard`: the request the
/// call came in was something a person said, not something the lamp wrote.
pub fn run(
    place: &Place,
    call: &ToolCall,
    request: u64,
    heard: bool,
    deadline: Duration,
) -> (Value, bool) {
    let started = Instant::now();
    let left = || deadline.saturating_sub(started.elapsed());
    match call.name.as_str() {
        START => {
            let Some(task) = call
                .args
                .get("task")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|task| !task.is_empty())
            else {
                return (json!({"error": "Say what the work is."}), true);
            };
            if task.len() > MOST_TASK_BYTES || task.starts_with("--") {
                return (
                    json!({"error": "That task is too long to send as it is: say it more shortly."}),
                    true,
                );
            }
            let Some(status) = ask(
                &place.directory.join("request.sock"),
                &json!({"op":"status"}),
                left(),
            ) else {
                return (crate::tools::unavailable(), false);
            };
            let Some(guard) = guard(&status, format!("voice-{request}-prepare-{}", nonce())) else {
                return (crate::tools::unavailable(), false);
            };
            let prepared = ask(
                &place.directory.join("request.sock"),
                &json!({"op":"prepare","guard":guard,"text":task,"target":place.target}),
                left(),
            );
            match prepared {
                Some(answer) if answer.get("ok") == Some(&json!(true)) => {
                    let (Some(id), Some(digest)) = (
                        answer.get("id").and_then(Value::as_u64),
                        answer.get("prepared_digest").and_then(Value::as_str),
                    ) else {
                        return (crate::tools::unavailable(), false);
                    };
                    if let Ok(mut pending) = PENDING.lock() {
                        *pending = Some(Pending {
                            id,
                            digest: digest.to_owned(),
                            request,
                            at: Instant::now(),
                        });
                    }
                    (
                        json!({
                            "prepared": true,
                            "task": task,
                            "where": "Claude Code on the office computer",
                            "next": "Say the task back in one short sentence and ask whether to start it. Start it only if the person says yes."
                        }),
                        true,
                    )
                }
                Some(_) => (
                    json!({"error": "The work could not be prepared just now."}),
                    true,
                ),
                None => (crate::tools::unavailable(), false),
            }
        }
        CONFIRM => {
            let pending = PENDING.lock().ok().and_then(|pending| pending.clone());
            let Some(pending) = pending else {
                return (
                    json!({"error": "There is no task waiting to be started."}),
                    true,
                );
            };
            if !may_confirm(&pending, request, heard, Instant::now()) {
                return (
                    json!({"error": "Not started: the person has not said yes to it since you asked. Ask again if they still want it."}),
                    true,
                );
            }
            let Some(status) = ask(
                &place.directory.join("request.sock"),
                &json!({"op":"status"}),
                left(),
            ) else {
                return (crate::tools::unavailable(), false);
            };
            let (Some(guard), Some(session)) = (
                guard(&status, format!("voice-{request}-confirm-{}", nonce())),
                status
                    .get("summary")
                    .and_then(|summary| summary.get("session_id"))
                    .and_then(Value::as_str),
            ) else {
                return (crate::tools::unavailable(), false);
            };
            let confirmed = ask(
                &place.directory.join("voice.sock"),
                &json!({"op":"confirm","guard":guard,"id":pending.id,"prepared_digest":pending.digest,"session_id":session}),
                left(),
            );
            match confirmed {
                Some(answer) if answer.get("ok") == Some(&json!(true)) => {
                    if let Ok(mut waiting) = PENDING.lock() {
                        *waiting = None;
                    }
                    (
                        json!({
                            "handing_over": true,
                            "who": "Claude Code",
                            "next": "Tell them in a few natural words that you are handing it to Claude Code now and will let them know when it is done, the way a chief of staff would (\"Sure, I'm giving that to Claude Code. I'll tell you when it's done.\"). It has not been taken up yet: do not say it has."
                        }),
                        true,
                    )
                }
                Some(_) => (json!({"error": "It could not be started just now."}), true),
                None => (crate::tools::unavailable(), false),
            }
        }
        CHECK => check(place, &crate::task_news::told_file(), left()),
        _ => (crate::tools::unavailable(), false),
    }
}

/// How the newest task handed over is going, from what the tasks part last
/// saw: no new look at the agent, so the answer comes at once. A finished
/// one the person now hears about is kept as told.
fn check(place: &Place, told_file: &Path, deadline: Duration) -> (Value, bool) {
    let started = Instant::now();
    let left = || deadline.saturating_sub(started.elapsed());
    let socket = place.directory.join("request.sock");
    let mut newest: Option<u64> = None;
    let mut store;
    let mut after = 0;
    loop {
        let Some(page) = ask(&socket, &json!({"op":"list","after_id":after}), left()) else {
            return (crate::tools::unavailable(), false);
        };
        store = page["store_id"].as_str().unwrap_or_default().to_owned();
        for item in page["items"].as_array().into_iter().flatten() {
            if item["dispatch"]["state"] == json!("accepted")
                && let Some(id) = item["id"].as_u64()
            {
                newest = newest.max(Some(id));
            }
        }
        match page["next_after_id"].as_u64() {
            Some(next) if next > after => after = next,
            _ => break,
        }
    }
    let Some(id) = newest else {
        return (
            json!({"none": true, "next": "Tell them there is no work handed over just now."}),
            true,
        );
    };
    let Some(task) = ask(&socket, &json!({"op":"get_task","id":id}), left()) else {
        return (crate::tools::unavailable(), false);
    };
    let who = crate::task_news::who(&task);
    let (asked, _) = crate::task_news::quoted(
        task["text"].as_str().unwrap_or_default(),
        crate::task_news::MOST_TASK_CHARS,
    );
    let Some(observation_id) = task["observations"]
        .as_array()
        .and_then(|seen| seen.iter().filter_map(|seen| seen["id"].as_u64()).max())
    else {
        return (
            json!({"who": who, "task": asked, "state": "working",
                "next": format!("Tell them {who} is still working on it, and that you will tell them when it is done.")}),
            true,
        );
    };
    let Some(observation) = ask(
        &socket,
        &json!({"op":"get_observation","id":id,"observation_id":observation_id}),
        left(),
    ) else {
        return (crate::tools::unavailable(), false);
    };
    let data = &observation["data"];
    let state = data["state"].as_str().unwrap_or("unknown");
    let (answer, cut) = crate::task_news::quoted(
        data["answer"].as_str().unwrap_or_default(),
        crate::task_news::MOST_ANSWER_CHARS,
    );
    let settled = match state {
        "idle" => !answer.is_empty(),
        "needs-input" | "stopped" | "failed" => true,
        _ => false,
    };
    if settled {
        crate::task_news::keep_told(told_file, &format!("{store}:{id}:{observation_id}"));
    }
    let next = match state {
        "idle" if settled => {
            "Give them the gist of what it came back with in one or two plain spoken sentences; no code, lists, paths or links. If there is more, say the rest is on their computer."
        }
        "needs-input" => {
            "Tell them it has stopped to ask them something, and ask its question in your own words."
        }
        "stopped" | "failed" => {
            "Tell them in one plain sentence that it stopped without finishing."
        }
        _ => "Tell them it is still working on it, and that you will tell them when it is done.",
    };
    (
        json!({"who": who, "task": asked, "state": state, "answer": answer,
            "more": cut || data["truncated"] == json!(true), "next": next}),
        true,
    )
}

/// The tasks part's guard for a mutation: its store, its revision now, and
/// a request id of our own.
fn guard(status: &Value, request_id: String) -> Option<Value> {
    let store = status.get("store_id")?.as_str()?;
    let revision = status.get("revision")?.as_u64()?;
    Some(json!({"store_id": store, "revision": revision, "request_id": request_id}))
}

fn ask(socket: &Path, line: &Value, deadline: Duration) -> Option<Value> {
    if deadline.is_zero() {
        return None;
    }
    crate::tools::exchange(socket, line, deadline)
}

fn nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
        sync::mpsc,
    };

    #[test]
    fn a_yes_counts_only_when_heard_after_the_task_and_soon() {
        let at = Instant::now();
        let pending = Pending {
            id: 1,
            digest: "d".into(),
            request: 5,
            at,
        };
        assert!(may_confirm(&pending, 6, true, at + Duration::from_secs(10)));
        // In the request that prepared it: the model asking itself.
        assert!(!may_confirm(&pending, 5, true, at));
        // A request the lamp wrote, not one it heard.
        assert!(!may_confirm(&pending, 6, false, at));
        // Too late.
        assert!(!may_confirm(
            &pending,
            6,
            true,
            at + CONFIRM_WITHIN + Duration::from_secs(1)
        ));
    }

    #[test]
    fn the_switch_is_an_absolute_directory_and_an_alias_and_is_taken_from_the_front() {
        assert!(arguments(None, None).is_empty());
        assert!(arguments(Some("relative"), None).is_empty());
        assert_eq!(arguments(Some("/run/t"), None), [FLAG, "/run/t"]);
        assert_eq!(
            arguments(Some("/run/t"), Some("m2")),
            [FLAG, "/run/t", TARGET_FLAG, "m2"]
        );
        assert_eq!(
            arguments(Some("/run/t"), Some("no spaces")),
            [FLAG, "/run/t"]
        );
        let flags: Vec<String> = ["--tasks-dir", "/run/t", "--other"]
            .map(String::from)
            .into();
        let (place, rest) = take(&flags).unwrap();
        assert_eq!(
            place,
            Some(Place {
                directory: "/run/t".into(),
                target: DEFAULT_TARGET.into()
            })
        );
        assert_eq!(rest, &flags[2..]);
        assert!(take(&["--tasks-dir".to_owned(), "x".to_owned()]).is_err());
        assert_eq!(take(&flags[2..]).unwrap().0, None);
    }

    /// A tasks part of one socket pair, answering each line with what the
    /// test gives it, and passing on what it was sent.
    fn tasks_part(directory: &Path, answers: Vec<(&'static str, Value)>) -> mpsc::Receiver<Value> {
        let (sent, seen) = mpsc::channel();
        for name in ["request.sock", "voice.sock"] {
            let listener = UnixListener::bind(directory.join(name)).unwrap();
            let answers: Vec<Value> = answers
                .iter()
                .filter(|(socket, _)| *socket == name)
                .map(|(_, answer)| answer.clone())
                .collect();
            let sent = sent.clone();
            std::thread::spawn(move || {
                for (stream, answer) in listener.incoming().zip(answers) {
                    let mut stream = stream.unwrap();
                    let mut line = String::new();
                    BufReader::new(&stream).read_line(&mut line).unwrap();
                    sent.send(serde_json::from_str::<Value>(&line).unwrap())
                        .unwrap();
                    writeln!(stream, "{answer}").unwrap();
                }
            });
        }
        seen
    }

    #[test]
    fn a_task_is_prepared_on_the_request_socket_and_started_only_on_the_voice_socket_after_a_yes() {
        let directory = std::env::temp_dir().join(format!("agent-tasks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let status = |revision: u64| json!({"ok":true,"store_id":"s1","revision":revision,"summary":{"session_id":"sess"}});
        let seen = tasks_part(
            &directory,
            vec![
                ("request.sock", status(0)),
                (
                    "request.sock",
                    json!({"ok":true,"id":7,"prepared_digest":"abc","revision":1}),
                ),
                ("request.sock", status(1)),
                (
                    "voice.sock",
                    json!({"ok":true,"id":7,"queued":true,"revision":2}),
                ),
            ],
        );
        let place = Place {
            directory: directory.clone(),
            target: "office".into(),
        };
        let call = |name: &str, args: Value| ToolCall {
            id: "c".into(),
            name: name.into(),
            args,
        };
        let deadline = Duration::from_secs(2);
        let (told, answered) = run(
            &place,
            &call(START, json!({"task":"Write hello.txt"})),
            3,
            true,
            deadline,
        );
        assert!(answered && told["prepared"] == json!(true), "{told}");
        assert_eq!(seen.recv().unwrap()["op"], "status");
        let prepare = seen.recv().unwrap();
        assert_eq!(prepare["op"], "prepare");
        assert_eq!(prepare["text"], "Write hello.txt");
        assert_eq!(prepare["target"], "office");
        assert_eq!(prepare["guard"]["revision"], 0);
        // The same request cannot start it.
        let (told, _) = run(&place, &call(CONFIRM, json!({})), 3, true, deadline);
        assert!(told.get("error").is_some(), "{told}");
        // The next thing the person said can.
        let (told, answered) = run(&place, &call(CONFIRM, json!({})), 4, true, deadline);
        assert!(answered && told["handing_over"] == json!(true), "{told}");
        assert_eq!(seen.recv().unwrap()["op"], "status");
        let confirm = seen.recv().unwrap();
        assert_eq!(confirm["op"], "confirm");
        assert_eq!(confirm["id"], 7);
        assert_eq!(confirm["prepared_digest"], "abc");
        assert_eq!(confirm["session_id"], "sess");
        assert_eq!(confirm["guard"]["revision"], 1);
        // Started once: nothing is waiting any more.
        let (told, _) = run(&place, &call(CONFIRM, json!({})), 5, true, deadline);
        assert!(told.get("error").is_some(), "{told}");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn an_empty_or_option_like_task_is_refused_before_anything_is_asked() {
        let place = Place {
            directory: "/nonexistent".into(),
            target: "office".into(),
        };
        let call = |task: &str| ToolCall {
            id: "c".into(),
            name: START.into(),
            args: json!({"task": task}),
        };
        assert!(
            run(&place, &call("  "), 1, true, Duration::from_secs(1))
                .0
                .get("error")
                .is_some()
        );
        assert!(
            run(
                &place,
                &call("--mode full"),
                1,
                true,
                Duration::from_secs(1)
            )
            .0
            .get("error")
            .is_some()
        );
        // Nobody there: unavailable, not answered.
        assert!(
            !run(
                &place,
                &call("Write a file"),
                1,
                true,
                Duration::from_millis(200)
            )
            .1
        );
    }

    #[test]
    fn asked_how_it_is_going_the_lamp_answers_from_the_last_look_and_keeps_it_told() {
        let directory = std::env::temp_dir().join(format!("agent-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let told = directory.join("told");
        let place = Place {
            directory: directory.clone(),
            target: DEFAULT_TARGET.into(),
        };
        let task = json!({"ok":true,"text":"Suggest three names","target":{"engine":"claude"},
            "observations":[{"id":4,"state":"idle"}]});
        let seen = tasks_part(
            &directory,
            vec![
                (
                    "request.sock",
                    json!({"ok":true,"store_id":"s1","items":[
                    {"id":1,"dispatch":{"state":"accepted"}},{"id":2,"dispatch":{"state":"accepted"}},
                    {"id":3,"dispatch":{"state":"invalidated"}}]}),
                ),
                ("request.sock", task),
                (
                    "request.sock",
                    json!({"ok":true,"data":{"state":"idle","answer":"Lumi, Sunny and Archie.","truncated":false}}),
                ),
            ],
        );
        let (told_model, answered) = check(&place, &told, Duration::from_secs(2));
        assert!(answered);
        assert_eq!(told_model["who"], "Claude Code");
        assert_eq!(told_model["state"], "idle");
        assert_eq!(told_model["answer"], "Lumi, Sunny and Archie.");
        // The newest task handed over, not the newest prepared.
        assert_eq!(seen.recv().unwrap()["op"], "list");
        assert_eq!(seen.recv().unwrap(), json!({"op":"get_task","id":2}));
        assert_eq!(
            seen.recv().unwrap(),
            json!({"op":"get_observation","id":2,"observation_id":4})
        );
        // Heard now, so not told again unasked.
        assert_eq!(std::fs::read_to_string(&told).unwrap(), "s1:2:4\n");
        std::fs::remove_dir_all(directory).unwrap();
    }
}
