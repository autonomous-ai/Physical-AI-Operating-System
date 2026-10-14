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
//! Who gets the work (`docs/tasks.md`, "Who gets the work"), off unless
//! `LAMP_TASKS_WHO=on` (`--tasks-who`): the person is the boss and the
//! agents are the staff ("ask Codex to ...", "have Claude Code ..."). The
//! staff are read from the tasks part's targets (`{"op":"targets"}`, alias
//! and engine) when the provider worker starts and every 30 s after, never
//! written into the prompt. With two or more, [`START`] and [`CHECK`] take
//! `who`, one of their names, and nobody named is Claude Code. The model
//! may hear "Codex" as "codecs" and "Claude" as "cloud": `who` is an enum
//! of the names, its description says what each may sound like, and the
//! voice takes a close form of a name only when it is one member's and
//! nobody else's. A name that is none of them prepares nothing, and the
//! model is told to ask. With fewer than two, or the staff not yet read,
//! the functions and everything sent to the tasks part are as without the
//! switch.
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
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

/// Where the coordinator reads the tasks part's directory.
pub const ENVIRONMENT: &str = "LAMP_TASKS_DIR";
/// Where it reads the target's alias, if not [`DEFAULT_TARGET`].
pub const TARGET_ENVIRONMENT: &str = "LAMP_TASKS_TARGET";
/// Where it reads whether the person may name who gets the work. `on` only.
pub const WHO_ENVIRONMENT: &str = "LAMP_TASKS_WHO";
/// The provider worker's flags for them.
pub const FLAG: &str = "--tasks-dir";
pub const TARGET_FLAG: &str = "--tasks-target";
pub const WHO_FLAG: &str = "--tasks-who";
pub const DEFAULT_TARGET: &str = "office";
/// The model's two functions.
pub const START: &str = "start_agent_task";
pub const CONFIRM: &str = "confirm_agent_task";
pub const CHECK: &str = "check_agent_task";
/// A yes counts this long after the task was prepared.
pub const CONFIRM_WITHIN: Duration = Duration::from_secs(120);
/// The longest task text the tasks part takes.
const MOST_TASK_BYTES: usize = 1_000;
/// The longest the first read of the staff holds up the provider worker's
/// start, as for the memory's: the tasks part answered a request on
/// lamp-4ace in 35 ms (`docs/tasks.md`), and a socket nobody serves fails
/// at once.
const FIRST_READ: Duration = Duration::from_millis(300);
/// How often the staff are read again, so that a tasks part started after
/// the voice, or restarted with other targets, is seen by the next session.
const READ_AGAIN: Duration = Duration::from_secs(30);
/// Who gets the work when the switch is off: as it always was.
const CLAUDE_CODE: &str = "Claude Code";

/// The task prepared and not yet started.
#[derive(Clone, Debug)]
struct Pending {
    id: u64,
    digest: String,
    request: u64,
    at: Instant,
    /// Who it was prepared for, as the person calls them.
    who: &'static str,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

/// An agent the tasks part can start: what the person calls it, how the
/// model is told it may sound, and the close forms the voice takes for it
/// (lower case, letters and digits only).
#[derive(Debug, Eq, PartialEq)]
struct Kind {
    engine: &'static str,
    name: &'static str,
    sounds: &'static str,
    close: &'static [&'static str],
}

/// The engines `lamp-tasks` takes. "Cloud" and "codecs" are what the issue
/// (#733) expects the model to hear for Claude and Codex; none of these
/// forms is close to another engine's.
const KINDS: [Kind; 3] = [
    Kind {
        engine: "claude",
        name: CLAUDE_CODE,
        sounds: "\"Claude\" or \"Claude Code\", which can sound like \"cloud\" or \"cloud code\"",
        close: &["claudecode", "claude", "cloudcode", "cloud"],
    },
    Kind {
        engine: "codex",
        name: "Codex",
        sounds: "\"Codex\", which can sound like \"codecs\" or \"code X\"",
        close: &["codex", "codecs", "codec"],
    },
    Kind {
        engine: "opencode",
        name: "OpenCode",
        sounds: "\"OpenCode\" or \"Open Code\"",
        close: &["opencode"],
    },
];

/// One of the staff: a target of the tasks part, and its kind.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Member {
    alias: String,
    kind: &'static Kind,
}

/// The staff as last read from the tasks part; empty until it answers.
static STAFF: Mutex<Vec<Member>> = Mutex::new(Vec::new());
static READING: OnceLock<()> = OnceLock::new();

/// The functions a session is told of: with [`WHO_FLAG`], for the staff as
/// last read.
pub fn tools(place: &Place) -> Vec<Tool> {
    declared(place, &staff_now(place))
}

fn staff_now(place: &Place) -> Vec<Member> {
    if !place.who {
        return Vec::new();
    }
    STAFF.lock().map(|staff| staff.clone()).unwrap_or_default()
}

/// The person may name one of them: the switch is on and there is more
/// than one.
fn choosing(place: &Place, staff: &[Member]) -> bool {
    place.who && staff.len() >= 2
}

fn declared(place: &Place, staff: &[Member]) -> Vec<Tool> {
    let choosing = choosing(place, staff);
    // Who does it when nobody is named; without the switch, as always.
    let default = if place.who {
        default_member(staff, &place.target).map_or(CLAUDE_CODE, |member| member.kind.name)
    } else {
        CLAUDE_CODE
    };
    let task = json!({
        "type": "string",
        "description": "The work, in the person's own words, complete enough to do without asking."
    });
    let (start_description, start_parameters) = if choosing {
        let all = names(staff, "and");
        let sounds: Vec<String> = staff
            .iter()
            .map(|member| format!("{} (they may say {})", member.kind.name, member.kind.sounds))
            .collect();
        (
            format!(
                "Prepare a piece of computer work for one of the person's agents on their office \
                 computer ({all}): writing or changing code, documents or files there. Call it \
                 only when the person asks for such work to be done there. Nothing starts yet: \
                 then say the task back in one short sentence, with who will do it, and ask \
                 whether to start it. \
                 You can do this: if they ask you to start one of them, open a session or \
                 give it something, without saying what it should do, never say you cannot; \
                 ask in a few words what it should work on, and call this with their answer."
            ),
            json!({
                "type": "object",
                "properties": {
                    "task": task,
                    "who": {
                        "type": "string",
                        "enum": staff.iter().map(|member| member.kind.name).collect::<Vec<_>>(),
                        "description": format!(
                            "Who does it, if the person named one: {}. Leave it out if they named nobody: then {default} does it.",
                            sounds.join("; ")
                        )
                    }
                },
                "required": ["task"]
            }),
        )
    } else {
        (
            format!(
                "Prepare a piece of computer work for {default} on the person's office \
                 computer: writing or changing code, documents or files there. Call it only \
                 when the person asks for such work to be done there. Nothing starts yet: \
                 then say the task back in one short sentence and ask whether to start it. \
                 You can do this: if they ask you to start {default}, open a session or \
                 give it something, without saying what it should do, never say you cannot; \
                 ask in a few words what it should work on, and call this with their answer."
            ),
            json!({
                "type": "object",
                "properties": {"task": task},
                "required": ["task"]
            }),
        )
    };
    vec![
        Tool {
            name: START.into(),
            description: start_description,
            parameters: Some(start_parameters),
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
            description: format!(
                "How the latest work you handed over is going, and what the agent came \
                 back with if it has finished. Call it only when the person asks about that \
                 work (\"how's it going?\", \"what did {default} say?\"), never on your own: \
                 you are told by yourself when it finishes."
            ),
            parameters: choosing.then(|| {
                json!({
                    "type": "object",
                    "properties": {
                        "who": {
                            "type": "string",
                            "enum": staff.iter().map(|member| member.kind.name).collect::<Vec<_>>(),
                            "description": format!(
                                "Whose work, if the person named one: {}. Leave it out for the latest work handed over.",
                                names(staff, "or")
                            )
                        }
                    }
                })
            }),
            silent: false,
        },
    ]
}

/// "Claude Code, Codex and OpenCode", or "or".
fn names(staff: &[Member], and: &str) -> String {
    let names: Vec<&str> = staff.iter().map(|member| member.kind.name).collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} {and} {last}", rest.join(", ")),
        Some((last, _)) => (*last).to_owned(),
        None => String::new(),
    }
}

/// Who does the work when nobody is named: the target the voice was given
/// if it is one of the staff, else Claude Code, else the first.
fn default_member<'a>(staff: &'a [Member], target: &str) -> Option<&'a Member> {
    staff
        .iter()
        .find(|member| member.alias == target)
        .or_else(|| staff.iter().find(|member| member.kind.engine == "claude"))
        .or_else(|| staff.first())
}

/// Lower case, letters and digits only: "Code X" and "codex" are one.
fn plain(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|letter| letter.to_ascii_lowercase())
        .collect()
}

/// The one member a name the model gave is: by the member's name, a close
/// form of it, or its alias. A form two members could share names neither.
fn named<'a>(staff: &'a [Member], said: &str) -> Option<&'a Member> {
    let said = plain(said);
    let mut found = staff.iter().filter(|member| {
        member.kind.close.contains(&said.as_str()) || plain(&member.alias) == said
    });
    let one = found.next()?;
    found.next().is_none().then_some(one)
}

/// Who gets the work in a call: nobody in particular when the person
/// cannot name anyone (the switch off, or the staff not read), so it goes
/// where it always went; the one named; the default when nobody is named;
/// or, for a name that is none of the staff, what to tell the model.
fn chosen<'a>(
    place: &Place,
    staff: &'a [Member],
    args: &Value,
) -> Result<Option<&'a Member>, Value> {
    if !place.who || staff.is_empty() {
        return Ok(None);
    }
    // Nobody to choose between: the one there is, whatever was said.
    if !choosing(place, staff) {
        return Ok(default_member(staff, &place.target));
    }
    let said = args
        .get("who")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|said| !said.is_empty());
    match said {
        None => Ok(default_member(staff, &place.target)),
        Some(said) => named(staff, said).map(Some).ok_or_else(|| {
            json!({"error": format!(
                "Nothing was done: nobody by that name works here. Ask the person who should do it: {}.",
                names(staff, "or")
            )})
        }),
    }
}

/// The staff in the tasks part's answer to `targets`: the first alias of
/// each engine, in the tasks part's order, and none of an engine this
/// voice does not know.
fn staff_from(answer: &Value) -> Option<Vec<Member>> {
    if answer.get("ok") != Some(&json!(true)) {
        return None;
    }
    let mut staff: Vec<Member> = Vec::new();
    for target in answer.get("targets")?.as_array()? {
        let (Some(name), Some(engine)) = (target["alias"].as_str(), target["engine"].as_str())
        else {
            continue;
        };
        let Some(kind) = KINDS.iter().find(|kind| kind.engine == engine) else {
            continue;
        };
        if alias(name) && !staff.iter().any(|member| member.kind == kind) {
            staff.push(Member {
                alias: name.to_owned(),
                kind,
            });
        }
    }
    Some(staff)
}

/// Read the staff from the tasks part: once now, briefly, and then every
/// 30 s on a thread of its own, so that opening a session waits for
/// nobody. Only with [`WHO_FLAG`], once per process. A read that fails
/// keeps what was known.
pub fn start(place: &Place) {
    if !place.who || READING.set(()).is_err() {
        return;
    }
    let socket = place.directory.join("request.sock");
    read_staff(&socket, FIRST_READ);
    let spawned = std::thread::Builder::new()
        .name("lamp-voice-staff".into())
        .spawn(move || {
            loop {
                std::thread::sleep(READ_AGAIN);
                read_staff(&socket, crate::tools::ASK_DEADLINE);
            }
        });
    if spawned.is_err() {
        eprintln!("lamp-voice provider: no thread to read who work can go to; read once");
    }
}

fn read_staff(socket: &Path, deadline: Duration) {
    let Some(read) = crate::tools::exchange_up_to(
        socket,
        &json!({"op":"targets"}),
        deadline,
        crate::tools::MAX_TASKS_ANSWER_BYTES,
    )
    .as_ref()
    .and_then(staff_from) else {
        return;
    };
    if let Ok(mut known) = STAFF.lock()
        && *known != read
    {
        let each: Vec<String> = read
            .iter()
            .map(|member| format!("{} ({})", member.kind.name, member.alias))
            .collect();
        eprintln!(
            "lamp-voice provider: work can go to {}",
            if each.is_empty() {
                "nobody this voice knows".to_owned()
            } else {
                each.join(", ")
            }
        );
        *known = read;
    }
}

pub fn is_tool(name: &str) -> bool {
    name == START || name == CONFIRM || name == CHECK
}

/// What the tasks part is reached by.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Place {
    pub directory: PathBuf,
    pub target: String,
    /// The person may name who gets the work ([`WHO_FLAG`]).
    pub who: bool,
}

/// The provider worker's arguments for what the environment says.
pub fn arguments_from_environment() -> Vec<String> {
    arguments(
        std::env::var(ENVIRONMENT).ok().as_deref(),
        std::env::var(TARGET_ENVIRONMENT).ok().as_deref(),
        std::env::var(WHO_ENVIRONMENT).ok().as_deref(),
    )
}

fn arguments(directory: Option<&str>, target: Option<&str>, who: Option<&str>) -> Vec<String> {
    let Some(directory) = directory.filter(|path| Path::new(path).is_absolute()) else {
        return Vec::new();
    };
    let mut arguments = vec![FLAG.to_owned(), directory.to_owned()];
    if let Some(target) = target.filter(|target| alias(target)) {
        arguments.push(TARGET_FLAG.to_owned());
        arguments.push(target.to_owned());
    }
    if who == Some("on") {
        arguments.push(WHO_FLAG.to_owned());
    }
    arguments
}

fn alias(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Take a leading `--tasks-dir DIR [--tasks-target ALIAS] [--tasks-who]` off
/// the flags.
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
    let (who, rest) = match rest {
        [flag, rest @ ..] if flag == WHO_FLAG => (true, rest),
        _ => (false, rest),
    };
    Ok((
        Some(Place {
            directory: PathBuf::from(directory),
            target,
            who,
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
    run_for(place, &staff_now(place), call, request, heard, deadline)
}

/// [`run`] with the staff as known.
fn run_for(
    place: &Place,
    staff: &[Member],
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
            // Who is settled before the tasks part is asked anything.
            let member = match chosen(place, staff, &call.args) {
                Ok(member) => member,
                Err(told) => return (told, true),
            };
            let (target, who) = member.map_or((place.target.as_str(), CLAUDE_CODE), |member| {
                (member.alias.as_str(), member.kind.name)
            });
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
                &json!({"op":"prepare","guard":guard,"text":task,"target":target}),
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
                            who,
                        });
                    }
                    let next = if choosing(place, staff) {
                        format!(
                            "Say the task back in one short sentence, saying {who} will do it, and ask whether to start it. Start it only if the person says yes."
                        )
                    } else {
                        "Say the task back in one short sentence and ask whether to start it. Start it only if the person says yes.".to_owned()
                    };
                    (
                        json!({
                            "prepared": true,
                            "task": task,
                            "where": format!("{who} on the office computer"),
                            "next": next
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
                    let who = pending.who;
                    (
                        json!({
                            "handing_over": true,
                            "who": who,
                            "next": format!("Tell them in a few natural words that you are handing it to {who} now and will let them know when it is done, the way a chief of staff would (\"Sure, I'm giving that to {who}. I'll tell you when it's done.\"). It has not been taken up yet: do not say it has.")
                        }),
                        true,
                    )
                }
                Some(_) => (json!({"error": "It could not be started just now."}), true),
                None => (crate::tools::unavailable(), false),
            }
        }
        CHECK => {
            // Only someone named narrows it, and only when there is a choice.
            let asked_about = call
                .args
                .get("who")
                .is_some_and(|who| who.as_str().is_some_and(|who| !who.trim().is_empty()));
            let only = if choosing(place, staff) && asked_about {
                match chosen(place, staff, &call.args) {
                    Ok(member) => member,
                    Err(told) => return (told, true),
                }
            } else {
                None
            };
            check(place, only, &crate::task_news::told_file(), left())
        }
        _ => (crate::tools::unavailable(), false),
    }
}

/// How the newest task handed over is going (to `only`, if named), from
/// what the tasks part last saw: no new look at the agent, so the answer
/// comes at once. A finished one the person now hears about is kept as
/// told.
fn check(
    place: &Place,
    only: Option<&Member>,
    told_file: &Path,
    deadline: Duration,
) -> (Value, bool) {
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
                && only.is_none_or(|member| item["target_alias"] == json!(member.alias))
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
        let next = only.map_or_else(
            || "Tell them there is no work handed over just now.".to_owned(),
            |member| {
                format!(
                    "Tell them there is no work handed over to {} just now.",
                    member.kind.name
                )
            },
        );
        return (json!({"none": true, "next": next}), true);
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
                "next": format!("Tell them in a few words that {who} is still working on it and that you will tell them yourself when it is done. Do not check again unless they ask again.")}),
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
        _ => {
            "Tell them in a few words that it is still working on it and that you will tell them yourself when it is done. Do not check again unless they ask again."
        }
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
    crate::tools::exchange_up_to(socket, line, deadline, crate::tools::MAX_TASKS_ANSWER_BYTES)
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
            who: CLAUDE_CODE,
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
        assert!(arguments(None, None, None).is_empty());
        assert!(arguments(Some("relative"), None, None).is_empty());
        assert_eq!(arguments(Some("/run/t"), None, None), [FLAG, "/run/t"]);
        assert_eq!(
            arguments(Some("/run/t"), Some("m2"), None),
            [FLAG, "/run/t", TARGET_FLAG, "m2"]
        );
        assert_eq!(
            arguments(Some("/run/t"), Some("no spaces"), None),
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
                target: DEFAULT_TARGET.into(),
                who: false,
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

    /// Held by each test that prepares and confirms: they share [`PENDING`].
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    #[test]
    fn a_task_is_prepared_on_the_request_socket_and_started_only_on_the_voice_socket_after_a_yes() {
        let _one = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
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
            who: false,
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
            who: false,
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
            who: false,
        };
        let task = json!({"ok":true,"text":"Suggest three names","target":{"engine":"claude"},
            "observations":[{"id":4,"state":"idle"}]});
        let seen = tasks_part(
            &directory,
            vec![
                (
                    "request.sock",
                    // Longer than the executive's 8 KiB bound, as a page of
                    // eight tasks with their records was on lamp-4ace.
                    json!({"ok":true,"store_id":"s1","padding":"x".repeat(9_000),"items":[
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
        let (told_model, answered) = check(&place, None, &told, Duration::from_secs(2));
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

    /// The three functions exactly as they were declared before the person
    /// could name who gets the work (voice `fd1a273ee`).
    fn before() -> Vec<Tool> {
        vec![
            Tool {
                name: START.into(),
                description: "Prepare a piece of computer work for Claude Code on the person's office \
                              computer: writing or changing code, documents or files there. Call it only \
                              when the person asks for such work to be done there. Nothing starts yet: \
                              then say the task back in one short sentence and ask whether to start it. \
                              You can do this: if they ask you to start Claude Code, open a session or \
                              give it something, without saying what it should do, never say you cannot; \
                              ask in a few words what it should work on, and call this with their answer."
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
                              back with if it has finished. Call it only when the person asks about that \
                              work (\"how's it going?\", \"what did Claude Code say?\"), never on your own: \
                              you are told by yourself when it finishes."
                    .into(),
                parameters: None,
                silent: false,
            },
        ]
    }

    fn member(alias: &str, engine: &str) -> Member {
        Member {
            alias: alias.into(),
            kind: KINDS.iter().find(|kind| kind.engine == engine).unwrap(),
        }
    }
    /// lamp-4ace's two, as `docs/tasks.md` gives them.
    fn two() -> Vec<Member> {
        vec![
            member("office-claude", "claude"),
            member("office-codex", "codex"),
        ]
    }
    fn place(directory: &Path, who: bool) -> Place {
        Place {
            directory: directory.into(),
            target: DEFAULT_TARGET.into(),
            who,
        }
    }

    /// One task prepared and started through a tasks part of one socket
    /// pair: what the tasks part was sent (each guard's request id, made
    /// new each time, left out) and what the model was told.
    fn handed_over(
        tag: &str,
        place_who: bool,
        staff: Option<&[Member]>,
        start: Value,
    ) -> (Vec<Value>, Vec<Value>) {
        let directory =
            std::env::temp_dir().join(format!("agent-who-{tag}-{}", std::process::id()));
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
        let place = place(&directory, place_who);
        let call = |name: &str, args: Value| ToolCall {
            id: "c".into(),
            name: name.into(),
            args,
        };
        let deadline = Duration::from_secs(2);
        let calls = [(call(START, start), 3), (call(CONFIRM, json!({})), 4)];
        let told: Vec<Value> = calls
            .iter()
            .map(|(call, request)| match staff {
                // As the provider worker runs it, staff and all.
                None => run(&place, call, *request, true, deadline).0,
                Some(staff) => run_for(&place, staff, call, *request, true, deadline).0,
            })
            .collect();
        let sent = (0..4)
            .map(|_| {
                let mut line = seen.recv_timeout(Duration::from_secs(2)).unwrap();
                if let Some(guard) = line.get_mut("guard").and_then(Value::as_object_mut) {
                    assert!(guard.remove("request_id").is_some());
                }
                line
            })
            .collect();
        std::fs::remove_dir_all(directory).unwrap();
        (sent, told)
    }

    #[test]
    fn without_the_switch_or_anyone_else_to_name_the_functions_are_exactly_as_before() {
        let off = place(Path::new("/run/t"), false);
        assert_eq!(tools(&off), before());
        // Off, whatever the tasks part has.
        assert_eq!(declared(&off, &two()), before());
        // On, but nobody read yet, or one target `office` for Claude Code.
        let on = place(Path::new("/run/t"), true);
        assert_eq!(declared(&on, &[]), before());
        assert_eq!(declared(&on, &[member("office", "claude")]), before());
    }

    #[test]
    fn without_the_switch_or_anyone_else_to_name_the_tasks_part_and_the_model_hear_what_they_did() {
        let _one = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let task = json!({"task":"Write hello.txt"});
        let (sent, told) = handed_over("off", false, None, task.clone());
        assert_eq!(
            sent,
            [
                json!({"op":"status"}),
                json!({"op":"prepare","guard":{"store_id":"s1","revision":0},"text":"Write hello.txt","target":"office"}),
                json!({"op":"status"}),
                json!({"op":"confirm","guard":{"store_id":"s1","revision":1},"id":7,"prepared_digest":"abc","session_id":"sess"}),
            ]
        );
        assert_eq!(
            told,
            [
                json!({"prepared":true,"task":"Write hello.txt","where":"Claude Code on the office computer",
                    "next":"Say the task back in one short sentence and ask whether to start it. Start it only if the person says yes."}),
                json!({"handing_over":true,"who":"Claude Code",
                    "next":"Tell them in a few natural words that you are handing it to Claude Code now and will let them know when it is done, the way a chief of staff would (\"Sure, I'm giving that to Claude Code. I'll tell you when it's done.\"). It has not been taken up yet: do not say it has."}),
            ]
        );
        // Off with a `who` the model was never told of: ignored, as before.
        let named = json!({"task":"Write hello.txt","who":"Codex"});
        assert_eq!(
            handed_over("off-named", false, None, named.clone()),
            (sent.clone(), told.clone())
        );
        // On, with nobody read yet, or only `office`: the same.
        assert_eq!(
            handed_over("on-none", true, Some(&[]), task.clone()),
            (sent.clone(), told.clone())
        );
        let office = [member("office", "claude")];
        assert_eq!(
            handed_over("on-office", true, Some(&office), named),
            (sent, told)
        );
    }

    #[test]
    fn the_person_names_who_gets_the_work_and_nobody_named_is_claude_code() {
        let _one = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let on = place(Path::new("/run/t"), true);
        let told = declared(&on, &two());
        assert!(told.iter().all(|tool| tool.validate().is_ok()));
        let start = told[0].parameters.as_ref().unwrap();
        assert_eq!(
            start["properties"]["who"]["enum"],
            json!(["Claude Code", "Codex"])
        );
        assert_eq!(start["required"], json!(["task"]));
        let hint = start["properties"]["who"]["description"].as_str().unwrap();
        for heard in ["\"cloud\"", "\"codecs\"", "then Claude Code does it"] {
            assert!(hint.contains(heard), "{hint}");
        }
        assert!(told[0].description.contains("(Claude Code and Codex)"));
        assert_eq!(
            told[2].parameters.as_ref().unwrap()["properties"]["who"]["enum"],
            json!(["Claude Code", "Codex"])
        );
        // All three the tasks part takes still fit what the link carries.
        let three = [two(), vec![member("office-opencode", "opencode")]].concat();
        assert!(
            declared(&on, &three)
                .iter()
                .all(|tool| tool.validate().is_ok())
        );

        for (tag, who, target, name) in [
            ("codex", json!("Codex"), "office-codex", "Codex"),
            ("codecs", json!("codecs"), "office-codex", "Codex"),
            ("cloud", json!("cloud code"), "office-claude", "Claude Code"),
            ("nobody", Value::Null, "office-claude", "Claude Code"),
            ("blank", json!(" "), "office-claude", "Claude Code"),
        ] {
            let mut start = json!({"task":"Write hello.txt"});
            if !who.is_null() {
                start["who"] = who;
            }
            let (sent, told) = handed_over(tag, true, Some(&two()), start);
            assert_eq!(sent[1]["target"], target, "{tag}");
            assert_eq!(sent[1]["text"], "Write hello.txt");
            assert_eq!(told[0]["where"], format!("{name} on the office computer"));
            assert!(
                told[0]["next"]
                    .as_str()
                    .unwrap()
                    .contains(&format!("saying {name} will do it")),
                "{tag}"
            );
            assert_eq!(told[1]["who"], name);
            assert!(
                told[1]["next"]
                    .as_str()
                    .unwrap()
                    .contains(&format!("I'm giving that to {name}."))
            );
        }
    }

    #[test]
    fn a_name_that_is_none_of_the_staff_prepares_nothing_and_the_model_is_told_to_ask() {
        // Nothing listens there: a call that asked the tasks part anything
        // would come back unavailable, not answered.
        let on = place(Path::new("/nonexistent"), true);
        for said in ["Cursor", "code", "OpenCode", "Gemini"] {
            let call = ToolCall {
                id: "c".into(),
                name: START.into(),
                args: json!({"task":"Write a file","who":said}),
            };
            let (told, answered) = run_for(&on, &two(), &call, 1, true, Duration::from_secs(1));
            assert!(answered, "{said}");
            let error = told["error"].as_str().unwrap();
            assert!(error.contains("Claude Code or Codex"), "{error}");
        }
    }

    #[test]
    fn a_close_form_names_one_member_and_never_one_of_another_engine() {
        let staff = two();
        for (said, alias) in [
            ("Claude", "office-claude"),
            ("CLAUDE CODE", "office-claude"),
            ("Cloud", "office-claude"),
            ("cloud-code", "office-claude"),
            ("codex", "office-codex"),
            ("Code X", "office-codex"),
            ("codecs", "office-codex"),
            ("codec", "office-codex"),
            ("office-codex", "office-codex"),
        ] {
            assert_eq!(
                named(&staff, said).map(|member| member.alias.as_str()),
                Some(alias),
                "{said}"
            );
        }
        for said in ["code", "claw", "codes", "open code", "", "Claude and Codex"] {
            assert_eq!(named(&staff, said), None, "{said}");
        }
        // Codex is not here: what sounds like it names nobody.
        assert_eq!(named(&[member("office-claude", "claude")], "codecs"), None);
        // A form two members could share names neither.
        let muddled = [member("codex", "claude"), member("office-codex", "codex")];
        assert_eq!(named(&muddled, "codex"), None);
    }

    #[test]
    fn the_staff_are_the_first_target_of_each_engine_the_voice_knows() {
        let answer = json!({"ok":true,"store_id":"s1","targets":[
            {"alias":"home-claude","engine":"claude"},{"alias":"office-claude","engine":"claude"},
            {"alias":"office-codex","engine":"codex"},{"alias":"office-cursor","engine":"cursor"},
            {"alias":"no spaces","engine":"opencode"},{"alias":"office-opencode","engine":"opencode"}]});
        let staff = staff_from(&answer).unwrap();
        let aliases: Vec<&str> = staff.iter().map(|member| member.alias.as_str()).collect();
        assert_eq!(aliases, ["home-claude", "office-codex", "office-opencode"]);
        assert_eq!(names(&staff, "and"), "Claude Code, Codex and OpenCode");
        // An older tasks part that does not know `targets`, or nonsense.
        assert_eq!(staff_from(&json!({"ok":false,"error":"bad_request"})), None);
        assert_eq!(staff_from(&json!({"ok":true})), None);
        // Nobody named: the target the voice was given, else Claude Code,
        // else the first.
        let alias = |staff: &[Member], target: &str| {
            default_member(staff, target).map(|member| member.alias.clone())
        };
        assert_eq!(
            alias(&two(), "office-codex").as_deref(),
            Some("office-codex")
        );
        assert_eq!(
            alias(&two(), DEFAULT_TARGET).as_deref(),
            Some("office-claude")
        );
        let others = [
            member("office-codex", "codex"),
            member("office-opencode", "opencode"),
        ];
        assert_eq!(
            alias(&others, DEFAULT_TARGET).as_deref(),
            Some("office-codex")
        );
        assert_eq!(alias(&[], DEFAULT_TARGET), None);
        // The news names OpenCode too.
        assert_eq!(
            crate::task_news::who(&json!({"target":{"engine":"opencode"}})),
            "OpenCode"
        );
    }

    #[test]
    fn the_switch_to_name_who_is_on_only_and_follows_the_target() {
        assert_eq!(
            arguments(Some("/run/t"), Some("office-claude"), Some("on")),
            [FLAG, "/run/t", TARGET_FLAG, "office-claude", WHO_FLAG]
        );
        assert_eq!(
            arguments(Some("/run/t"), None, Some("on")),
            [FLAG, "/run/t", WHO_FLAG]
        );
        assert_eq!(
            arguments(Some("/run/t"), None, Some("yes")),
            [FLAG, "/run/t"]
        );
        assert!(arguments(None, None, Some("on")).is_empty());
        let flags: Vec<String> = [
            FLAG,
            "/run/t",
            TARGET_FLAG,
            "office-claude",
            WHO_FLAG,
            "--other",
        ]
        .map(String::from)
        .into();
        let (place, rest) = take(&flags).unwrap();
        assert_eq!(
            place,
            Some(Place {
                directory: "/run/t".into(),
                target: "office-claude".into(),
                who: true,
            })
        );
        assert_eq!(rest, ["--other"]);
        let alone = [FLAG.to_owned(), "/run/t".to_owned(), WHO_FLAG.to_owned()];
        let (place, rest) = take(&alone).unwrap();
        assert!(place.is_some_and(|place| place.who && place.target == DEFAULT_TARGET));
        assert!(rest.is_empty());
    }

    #[test]
    fn asked_about_one_of_them_the_lamp_answers_about_that_ones_newest_work() {
        let directory =
            std::env::temp_dir().join(format!("agent-check-who-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let on = place(&directory, true);
        let list = json!({"ok":true,"store_id":"s1","items":[
            {"id":1,"target_alias":"office-claude","dispatch":{"state":"accepted"}},
            {"id":2,"target_alias":"office-codex","dispatch":{"state":"accepted"}},
            {"id":3,"target_alias":"office-claude","dispatch":{"state":"accepted"}}]});
        let seen = tasks_part(
            &directory,
            vec![
                ("request.sock", list.clone()),
                (
                    "request.sock",
                    json!({"ok":true,"text":"Name a lamp","target":{"engine":"codex"},"observations":[]}),
                ),
                (
                    "request.sock",
                    json!({"ok":true,"store_id":"s1","items":[]}),
                ),
            ],
        );
        let call = |who: &str| ToolCall {
            id: "c".into(),
            name: CHECK.into(),
            args: json!({"who":who}),
        };
        let deadline = Duration::from_secs(2);
        let (told, answered) = run_for(&on, &two(), &call("codecs"), 5, true, deadline);
        assert!(answered);
        assert_eq!(told["who"], "Codex");
        assert_eq!(told["state"], "working");
        assert_eq!(seen.recv().unwrap()["op"], "list");
        assert_eq!(seen.recv().unwrap(), json!({"op":"get_task","id":2}));
        // Nothing handed to them yet.
        let (told, _) = run_for(&on, &two(), &call("Codex"), 6, true, deadline);
        assert_eq!(
            told["next"],
            "Tell them there is no work handed over to Codex just now."
        );
        // Nobody by that name: nothing is asked.
        let (told, answered) = run_for(&on, &two(), &call("Cursor"), 7, true, deadline);
        assert!(answered && told["error"].is_string(), "{told}");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn the_staff_read_from_the_tasks_part_are_what_the_next_session_is_told_of() {
        let directory = std::env::temp_dir().join(format!("agent-staff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let seen = tasks_part(
            &directory,
            vec![
                (
                    "request.sock",
                    json!({"ok":true,"targets":[{"alias":"office-claude","engine":"claude"},{"alias":"office-codex","engine":"codex"}]}),
                ),
                // An older tasks part: what was known stays.
                ("request.sock", json!({"ok":false,"error":"bad_request"})),
            ],
        );
        let socket = directory.join("request.sock");
        read_staff(&socket, Duration::from_secs(2));
        assert_eq!(seen.recv().unwrap(), json!({"op":"targets"}));
        read_staff(&socket, Duration::from_secs(2));
        assert_eq!(seen.recv().unwrap(), json!({"op":"targets"}));
        let on = place(&directory, true);
        let told = tools(&on);
        assert_eq!(
            told[0].parameters.as_ref().unwrap()["properties"]["who"]["enum"],
            json!(["Claude Code", "Codex"])
        );
        // Without the switch nothing read counts.
        assert_eq!(tools(&place(&directory, false)), before());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
