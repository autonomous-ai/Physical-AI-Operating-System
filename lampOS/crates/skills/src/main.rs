//! `lamp-skills`: the physical skills part (`docs/skills.md`). Loads every
//! skill folder, reloads them when one changes, listens to the topics their
//! triggers name, and runs their steps through the body's sockets, the
//! actions part and, until the voice has a say service, a stub.
use lamp_bus::{Bus, Publisher, Subscriber};
use lamp_msgs::{SkillEvent, SkillStage};
use lamp_skills::{Call, Engine, load_all, run_tests};
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::net::{UnixDatagram, UnixStream},
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant, SystemTime},
};

const USAGE: &str = "usage: lamp-skills [--dir DIR] [--bus-root DIR] [--gaze-socket PATH] \
                     [--mood-socket PATH] [--actions-socket PATH]\n       \
                     lamp-skills test SKILL_DIR...";
/// How often the folder is looked at for a change.
const RELOAD_EVERY: Duration = Duration::from_secs(2);
const ACTIONS_TIMEOUT: Duration = Duration::from_secs(2);

struct Sockets {
    gaze: PathBuf,
    mood: PathBuf,
    actions: PathBuf,
}

fn main() -> ExitCode {
    let given: Vec<String> = std::env::args().skip(1).collect();
    if given.first().map(String::as_str) == Some("test") {
        return test(&given[1..]);
    }
    let mut dir = PathBuf::from("/var/lib/lampOS/skills");
    let mut root = None;
    let mut sockets = Sockets {
        gaze: "/run/lampos-body/gaze.sock".into(),
        mood: "/run/lampos-body/mood.sock".into(),
        actions: "/run/lampos-actions/call.sock".into(),
    };
    let mut pairs = given.chunks(2);
    for pair in &mut pairs {
        let [flag, value] = pair else {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        };
        let value = PathBuf::from(value);
        match flag.as_str() {
            "--dir" => dir = value,
            "--bus-root" => root = Some(value),
            "--gaze-socket" => sockets.gaze = value,
            "--mood-socket" => sockets.mood = value,
            "--actions-socket" => sockets.actions = value,
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let skip_ask = std::env::var("LAMP_SKILLS_ASK").is_ok_and(|value| value == "skip");
    match serve(&dir, root, &sockets, skip_ask) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lamp-skills: {error}");
            ExitCode::FAILURE
        }
    }
}

fn test(dirs: &[String]) -> ExitCode {
    if dirs.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let mut failed = false;
    for dir in dirs {
        match run_tests(Path::new(dir)) {
            Ok((passed, unrecorded)) => {
                println!("{dir}: {passed} cases pass, {unrecorded} have no recording yet")
            }
            Err(why) => {
                println!("{dir}: {why}");
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// What the folder holds, to see that it changed: every file but the tests,
/// with its size and when it was written.
fn fingerprint(dir: &Path) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
    let mut found = Vec::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let path = entry.path();
            // A Mac's `._NAME` files are not the skill's: no reload for them.
            if lamp_skills::hidden(&path) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                if entry.file_name() != "tests" {
                    folders.push(path);
                }
            } else {
                found.push((path, meta.len(), meta.modified().ok()));
            }
        }
    }
    found.sort();
    found
}

fn publish(out: &mut Publisher, mut event: SkillEvent) {
    if event.why.len() > 2_000 {
        let mut end = 2_000;
        while !event.why.is_char_boundary(end) {
            end -= 1;
        }
        event.why.truncate(end);
    }
    eprintln!(
        "lamp-skills: {} {:?}: {}",
        event.skill, event.stage, event.why
    );
    if let Err(error) = out.publish_json(lamp_ipc::monotonic_us(), &event) {
        eprintln!("lamp-skills: skills/events not published: {error}");
    }
}

fn serve(dir: &Path, root: Option<PathBuf>, sockets: &Sockets, skip_ask: bool) -> io::Result<()> {
    let bus = match root {
        Some(root) => Bus::new(root)?,
        None => Bus::from_environment()?,
    };
    let mut out =
        bus.publisher_declared(SkillEvent::TOPIC, &SkillEvent::declaration(), "skills")?;
    let body = UnixDatagram::unbound()?;
    body.set_nonblocking(true)?;
    let mut engine = Engine::new(Vec::new(), skip_ask);
    let mut subscribers: Vec<Subscriber> = Vec::new();
    let mut seen = None;
    let mut looked: Option<Instant> = None;
    eprintln!(
        "lamp-skills: skills from {}, the Policy's ask {}",
        dir.display(),
        if skip_ask {
            "skipped"
        } else {
            "not built: nothing is done"
        }
    );
    loop {
        if looked.is_none_or(|at| at.elapsed() >= RELOAD_EVERY) {
            looked = Some(Instant::now());
            let print = fingerprint(dir);
            if seen.as_ref() != Some(&print) {
                seen = Some(print);
                let (skills, errors) = load_all(dir);
                for error in errors {
                    eprintln!("lamp-skills: not loaded: {error}");
                }
                let names: Vec<&str> = skills.iter().map(|skill| skill.name.as_str()).collect();
                eprintln!("lamp-skills: loaded {names:?}");
                engine.replace(skills);
                let topics = engine.topics();
                subscribers
                    .retain(|subscriber| topics.iter().any(|topic| topic == subscriber.topic()));
                for topic in topics {
                    if !subscribers
                        .iter()
                        .any(|subscriber| subscriber.topic() == topic)
                    {
                        match bus.subscribe(&topic, "skills") {
                            Ok(subscriber) => subscribers.push(subscriber),
                            Err(error) => eprintln!("lamp-skills: {topic} not subscribed: {error}"),
                        }
                    }
                }
            }
            let _ = out.alive();
        }
        if subscribers.is_empty() {
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }
        let wait =
            Duration::from_millis(250 / subscribers.len() as u64).max(Duration::from_millis(10));
        for subscriber in &mut subscribers {
            let message = match subscriber.recv(wait) {
                Ok(Some(message)) => message,
                Ok(None) => continue,
                Err(error) => {
                    eprintln!("lamp-skills: {} not received: {error}", subscriber.topic());
                    continue;
                }
            };
            let runs = engine.observe(
                &message.header.topic,
                lamp_ipc::monotonic_us(),
                &message.payload,
                message.header.expires_ms,
            );
            for run in runs {
                for event in run.events {
                    publish(&mut out, event);
                }
                if run.calls.is_empty() {
                    continue;
                }
                // ponytail: steps run in this loop; a slow actions part holds
                // the next message up to 2 s. A thread per run if that matters.
                let done: Vec<String> = run
                    .calls
                    .iter()
                    .map(|call| make(call, sockets, &body))
                    .collect();
                publish(
                    &mut out,
                    SkillEvent {
                        skill: run.skill,
                        stage: SkillStage::Done,
                        why: done.join("; "),
                        allowed: None,
                    },
                );
            }
        }
    }
}

/// Makes one call and says how it went.
fn make(call: &Call, sockets: &Sockets, body: &UnixDatagram) -> String {
    let tell = |what: &str, text: String, path: &Path| match body.send_to(text.as_bytes(), path) {
        Ok(_) => format!("{what}: told the body"),
        Err(error) => format!("{what}: the body was not reached ({error})"),
    };
    let actions = |what: &str, name: &str, args: serde_json::Value| match action(
        &sockets.actions,
        name,
        args,
    ) {
        Ok(answer) => format!("{what}: {answer}"),
        Err(error) => format!("{what}: the actions part was not reached ({error})"),
    };
    match call {
        Call::Gaze { face: None } => "look at the face: no face in view".into(),
        Call::Gaze {
            face: Some((x, y, size)),
        } => tell(
            "look at the face",
            serde_json::json!({"face": {"x": x, "y": y, "size": size, "turned": 0.0}}).to_string(),
            &sockets.gaze,
        ),
        Call::Gesture(name) => tell(
            &format!("gesture {name}"),
            serde_json::json!({ "gesture": name }).to_string(),
            &sockets.mood,
        ),
        Call::Light {
            colour,
            level,
            lasting,
        } => {
            let mut said = actions(
                &format!("light {colour}"),
                "light_colour",
                serde_json::json!({ "colour": colour }),
            );
            // No call sets a level, and nothing gives the light back yet.
            if level.is_some() || lasting.is_some() {
                said.push_str(" (its level and for are not carried out yet)");
            }
            said
        }
        Call::Dim { steps, until_ends } => {
            let mut said: Vec<String> = (0..*steps)
                .map(|_| actions("dim", "light_dimmer", serde_json::json!({})))
                .collect();
            if *until_ends {
                said.push("until it ends is not carried out yet".into());
            }
            said.join(", ")
        }
        // The stub until the voice's say service (#939): nothing is spoken.
        Call::Say(_) => {
            "say: a stub, nothing spoken (the voice has no say service yet, #939)".into()
        }
    }
}

/// One action on the actions part's socket: one line each way.
fn action(path: &Path, name: &str, args: serde_json::Value) -> io::Result<String> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(ACTIONS_TIMEOUT))?;
    stream.set_write_timeout(Some(ACTIONS_TIMEOUT))?;
    let call = serde_json::json!({"op": "call", "name": name, "args": args});
    writeln!(stream, "{call}")?;
    let mut line = String::new();
    BufReader::new(stream.take(8_193)).read_line(&mut line)?;
    let answer: serde_json::Value = serde_json::from_str(&line)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(if answer["ok"] == true {
        "done".into()
    } else {
        answer["error"].as_str().unwrap_or("refused").to_owned()
    })
}
