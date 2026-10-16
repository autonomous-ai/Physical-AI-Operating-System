//! `lamp-skills`: the physical skills part (`docs/skills.md`). Loads every
//! skill folder, reloads them when one changes, listens to the topics their
//! triggers name, and runs their steps through the body's sockets, the
//! actions part and the voice's say service.
use lamp_bus::{Bus, Publisher, Subscriber};
use lamp_msgs::{Light, SkillEvent, SkillStage};
use lamp_skills::{Call, Engine, duration_us, load_all, run_tests};
use serde_json::json;
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::net::{UnixDatagram, UnixStream},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::mpsc,
    time::{Duration, Instant, SystemTime},
};

const USAGE: &str = "usage: lamp-skills [--dir DIR] [--bus-root DIR] [--gaze-socket PATH] \
                     [--mood-socket PATH] [--actions-socket PATH] [--say-socket PATH]\n       \
                     lamp-skills test SKILL_DIR...";
/// How often the folder is looked at for a change.
const RELOAD_EVERY: Duration = Duration::from_secs(2);
const ACTIONS_TIMEOUT: Duration = Duration::from_secs(2);
/// A say not begun by then is of no use: the moment has passed.
/// ponytail: one value for every skill; a `say` field when one needs another.
const SAY_EXPIRES_MS: u64 = 30_000;
/// The longest the voice may take between two lines of an answer.
const SAY_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone)]
struct Sockets {
    gaze: PathBuf,
    mood: PathBuf,
    actions: PathBuf,
    say: PathBuf,
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
        say: "/run/lampos-say/say.sock".into(),
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
            "--say-socket" => sockets.say = value,
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

/// The light shown now, if the ring's owner has said.
fn light_now(bus: &Bus) -> Option<Light> {
    bus.latest(Light::TOPICS[1]).ok()??.parse().ok()
}

fn serve(dir: &Path, root: Option<PathBuf>, sockets: &Sockets, skip_ask: bool) -> io::Result<()> {
    let bus = match root {
        Some(root) => Bus::new(root)?,
        None => Bus::from_environment()?,
    };
    let mut out =
        bus.publisher_declared(SkillEvent::TOPIC, &SkillEvent::declaration(), "skills")?;
    let (finished, done) = mpsc::channel::<SkillEvent>();
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
        while let Ok(event) = done.try_recv() {
            publish(&mut out, event);
        }
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
                // A run waits on the voice for as long as the say takes: its
                // own thread, so the skills go on listening.
                let (sockets, finished) = (sockets.clone(), finished.clone());
                let before = light_now(&bus);
                std::thread::spawn(move || {
                    let why = perform(&run.skill, &run.calls, &sockets, before);
                    let _ = finished.send(SkillEvent {
                        skill: run.skill,
                        stage: SkillStage::Done,
                        why,
                        allowed: None,
                    });
                });
            }
        }
    }
}

/// How the light was changed, to give it back.
#[derive(Default)]
struct Changed {
    coloured: bool,
    dimmed: u8,
    lasting_us: u64,
}

/// Runs one skill's calls and says how each went. With a say, the others
/// are made when the voice begins it, so the look and the light come with
/// the words, and none if it is never begun. A light changed is given back
/// when the say ends, or after its `for`.
fn perform(skill: &str, calls: &[Call], sockets: &Sockets, before: Option<Light>) -> String {
    let is_light = |call: &Call| matches!(call, Call::Light { .. } | Call::Dim { .. });
    let mut said = Vec::new();
    if before.is_none() && calls.iter().any(is_light) {
        said.push(
            "light: left alone, since the light shown is not known and could not be given back"
                .into(),
        );
    }
    let others: Vec<&Call> = calls
        .iter()
        .filter(|call| !matches!(call, Call::Say(_)) && (before.is_some() || !is_light(call)))
        .collect();
    let note = calls.iter().find_map(|call| match call {
        Call::Say(note) => Some(note.clone()),
        _ => None,
    });
    let mut changed = Changed::default();
    let mut act = |said: &mut Vec<String>| {
        for call in &others {
            match call {
                Call::Light { lasting, .. } => {
                    changed.coloured = true;
                    changed.lasting_us = lasting
                        .as_deref()
                        .and_then(|text| duration_us(text).ok())
                        .unwrap_or(0);
                }
                Call::Dim { steps, .. } => changed.dimmed += steps,
                _ => {}
            }
            said.push(make(call, sockets));
        }
    };
    match note {
        Some(note) => {
            let mut begun = Vec::new();
            let outcome = say(&sockets.say, skill, &note, || act(&mut begun));
            said.extend(begun);
            said.push(outcome);
        }
        None => {
            act(&mut said);
            std::thread::sleep(Duration::from_micros(changed.lasting_us));
        }
    }
    if let Some(before) = before
        && (changed.coloured || changed.dimmed > 0)
    {
        said.push(give_back(&before, &changed, sockets));
    }
    said.join("; ")
}

/// The light as it was: off, or its colour again and the steps dimmed
/// brightened back.
fn give_back(before: &Light, changed: &Changed, sockets: &Sockets) -> String {
    let mut done = Vec::new();
    if !before.on {
        done.push(answer(action(&sockets.actions, "light_off", json!({}))));
    } else {
        if changed.coloured {
            done.push(answer(action(
                &sockets.actions,
                "light_colour",
                json!({ "colour": before.colour }),
            )));
        }
        for _ in 0..changed.dimmed {
            done.push(answer(action(
                &sockets.actions,
                "light_brighter",
                json!({}),
            )));
        }
    }
    format!(
        "light given back to {} {} at {}: {}",
        if before.on { "on" } else { "off" },
        before.colour,
        before.level,
        done.join(", ")
    )
}

fn answer(result: io::Result<String>) -> String {
    result.unwrap_or_else(|error| format!("the actions part was not reached ({error})"))
}

/// Asks the voice to say `note` (#939's say service: one line in, a `begun`
/// line when the turn starts, then the outcome). `begun` is called with the
/// first. What became of it, in words.
fn say(path: &Path, skill: &str, note: &str, mut begun: impl FnMut()) -> String {
    let mut attempt = || -> io::Result<String> {
        let mut stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(SAY_TIMEOUT))?;
        stream.set_write_timeout(Some(ACTIONS_TIMEOUT))?;
        let ask =
            json!({"skill": skill, "note": note, "priority": 5, "expires_in_ms": SAY_EXPIRES_MS});
        writeln!(stream, "{ask}")?;
        let mut lines = BufReader::new(stream.take(64 * 1_024));
        loop {
            let mut line = String::new();
            if lines.read_line(&mut line)? == 0 {
                return Ok("the voice ended without an outcome".into());
            }
            let answer: serde_json::Value = serde_json::from_str(&line)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            if answer["event"] == "begun" {
                begun();
            } else if let Some(outcome) = answer["outcome"].as_str() {
                return Ok(match answer["why"].as_str() {
                    Some(why) => format!("{outcome} ({why})"),
                    None => outcome.to_owned(),
                });
            }
        }
    };
    match attempt() {
        Ok(outcome) => format!("say: {outcome}"),
        Err(error) => format!("say: the voice's say service was not reached ({error})"),
    }
}

/// Makes one call and says how it went.
fn make(call: &Call, sockets: &Sockets) -> String {
    let tell = |what: &str, text: String, path: &Path| {
        let sent = UnixDatagram::unbound().and_then(|body| body.send_to(text.as_bytes(), path));
        match sent {
            Ok(_) => format!("{what}: told the body"),
            Err(error) => format!("{what}: the body was not reached ({error})"),
        }
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
            json!({"face": {"x": x, "y": y, "size": size, "turned": 0.0}}).to_string(),
            &sockets.gaze,
        ),
        Call::Gesture(name) => tell(
            &format!("gesture {name}"),
            json!({ "gesture": name }).to_string(),
            &sockets.mood,
        ),
        Call::Light { colour, level, .. } => {
            let mut said = actions(
                &format!("light {colour}"),
                "light_colour",
                json!({ "colour": colour }),
            );
            // No call sets a level.
            if level.is_some() {
                said.push_str(" (its level is not carried out yet)");
            }
            said
        }
        Call::Dim { steps, until_ends } => {
            let mut said: Vec<String> = (0..*steps)
                .map(|_| actions("dim", "light_dimmer", json!({})))
                .collect();
            if *until_ends {
                said.push("until it ends is not carried out yet".into());
            }
            said.join(", ")
        }
        Call::Say(_) => "say: made by its own step".into(),
    }
}

/// One action on the actions part's socket: one line each way.
fn action(path: &Path, name: &str, args: serde_json::Value) -> io::Result<String> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(ACTIONS_TIMEOUT))?;
    stream.set_write_timeout(Some(ACTIONS_TIMEOUT))?;
    let call = json!({"op": "call", "name": name, "args": args});
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
