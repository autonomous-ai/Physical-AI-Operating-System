//! `lamp-skills`: the physical skills part (`docs/skills.md`). Loads every
//! skill folder, reloads them when one changes, listens to the topics their
//! triggers name, and runs their steps through the body's sockets, the
//! actions part and the voice's say service.
use lamp_bus::{Bus, Publisher, Subscriber};
use lamp_msgs::{Light, SkillEvent, SkillStage};
use lamp_skills::{Admission, Call, Engine, duration_us, load_all, run_tests, told, tools_file};
use serde_json::json;
use std::{
    collections::HashSet,
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::net::{UnixDatagram, UnixListener, UnixStream},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::mpsc,
    time::{Duration, Instant, SystemTime},
};

const USAGE: &str = "usage: lamp-skills [--dir DIR] [--bus-root DIR] [--gaze-socket PATH] \
                     [--mood-socket PATH] [--actions-socket PATH] [--say-socket PATH] \
                     [--tools-dir DIR]\n       \
                     lamp-skills test SKILL_DIR...";
/// How often the folder is looked at for a change.
const RELOAD_EVERY: Duration = Duration::from_secs(2);
const ACTIONS_TIMEOUT: Duration = Duration::from_secs(2);
/// A say not begun by then is of no use: the moment has passed.
/// ponytail: one value for every skill; a `say` field when one needs another.
const SAY_EXPIRES_MS: u64 = 30_000;
/// The longest the voice may take between two lines of an answer.
const SAY_TIMEOUT: Duration = Duration::from_secs(180);
/// The longest a tool call waits for the main loop, and for the voice's
/// line: the voice gives up on a call after 1.5 s.
const TOOL_TIMEOUT: Duration = Duration::from_secs(1);
/// How often a held pose is said again: the body lets one go after 1.5 s.
const MOOD_EVERY: Duration = Duration::from_secs(1);

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
    let mut tools = None;
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
            "--tools-dir" => tools = Some(value),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let skip_ask = std::env::var("LAMP_SKILLS_ASK").is_ok_and(|value| value == "skip");
    match serve(&dir, root, &sockets, tools.as_deref(), skip_ask) {
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
/// The light shown now, if the ring's owner has said within 3 s (as the
/// actions part counts it).
fn light_now(bus: &Bus) -> Option<Light> {
    let message = bus.latest(Light::TOPICS[1]).ok()??;
    let now_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64);
    (now_ms.saturating_sub(message.header.at_unix_ms) <= 3_000)
        .then(|| message.parse().ok())
        .flatten()
}

fn serve(
    dir: &Path,
    root: Option<PathBuf>,
    sockets: &Sockets,
    tools: Option<&Path>,
    skip_ask: bool,
) -> io::Result<()> {
    let bus = match root {
        Some(root) => Bus::new(root)?,
        None => Bus::from_environment()?,
    };
    let mut out =
        bus.publisher_declared(SkillEvent::TOPIC, &SkillEvent::declaration(), "skills")?;
    let (finished, done) = mpsc::channel::<SkillEvent>();
    let mut engine = Engine::new(Vec::new(), skip_ask);
    let runner = Runner {
        sockets: sockets.clone(),
        finished,
        bus: bus.clone(),
    };
    let (ask, asks) = mpsc::channel::<ToolAsk>();
    if let Some(tools) = tools {
        listen(tools, ask)?;
    }
    // One run a skill at a time: what is under way, by skill.
    let mut busy = HashSet::new();
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
            busy.remove(&event.skill);
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
                if let Some(tools) = tools {
                    write_tools(tools, &engine);
                }
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
            if let Ok(call) = asks.recv_timeout(Duration::from_millis(250)) {
                tool_call(call, &engine, &mut busy, &mut out, &runner);
            }
            continue;
        }
        let wait =
            Duration::from_millis(250 / subscribers.len() as u64).max(Duration::from_millis(10));
        for subscriber in &mut subscribers {
            while let Ok(call) = asks.try_recv() {
                tool_call(call, &engine, &mut busy, &mut out, &runner);
            }
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
                if !run.calls.is_empty() {
                    start(run.skill, run.calls, &mut busy, &mut out, &runner);
                }
            }
        }
    }
}

/// What a run needs from the program.
struct Runner {
    sockets: Sockets,
    finished: mpsc::Sender<SkillEvent>,
    bus: Bus,
}

/// Starts one run of `skill`, unless its last is still under way.
fn start(
    skill: String,
    calls: Vec<Call>,
    busy: &mut HashSet<String>,
    out: &mut Publisher,
    runner: &Runner,
) {
    if !busy.insert(skill.clone()) {
        publish(
            out,
            SkillEvent {
                skill,
                stage: SkillStage::Done,
                why: "not made: its last run is still under way".into(),
                allowed: None,
            },
        );
        return;
    }
    // A run waits on the voice for as long as the say takes (at most
    // SAY_TIMEOUT a line) or a light's for (at most 10 min): its own
    // thread, one a skill, so the skills go on listening.
    // ponytail: a skill removed meanwhile still finishes its run.
    let (sockets, finished, bus) = (
        runner.sockets.clone(),
        runner.finished.clone(),
        runner.bus.clone(),
    );
    let before = light_now(&bus);
    std::thread::spawn(move || {
        let why = perform(&skill, &calls, &sockets, &bus, before);
        let _ = finished.send(SkillEvent {
            skill,
            stage: SkillStage::Done,
            why,
            allowed: None,
        });
    });
}

/// A call of a skill's tool from the voice, and where its answer goes.
type ToolAsk = (String, mpsc::Sender<serde_json::Value>, Admission);

/// The model called a skill's tool: its steps run, and the voice is told
/// at once what the model is to say. The look and the say are the model's;
/// the body and the light are made here.
fn tool_call(
    (name, reply, admission): ToolAsk,
    engine: &Engine,
    busy: &mut HashSet<String>,
    out: &mut Publisher,
    runner: &Runner,
) {
    let refuse = |why: &str| {
        if admission.commit() {
            let _ = reply.send(json!({ "error": why }));
        }
    };
    let run = match engine.tool(&name, lamp_ipc::monotonic_us()) {
        Ok(run) => run,
        Err(why) => return refuse(&why),
    };
    let made: Vec<Call> = run
        .calls
        .iter()
        .filter(|call| !matches!(call, Call::Look | Call::Say(_)))
        .cloned()
        .collect();
    // A run under way is left as it is: a call again meanwhile is told
    // again what to do next (a student who asks a minute later gets a new
    // look), and nothing more is made; the light already on is given back
    // when its `for` ends. A call that is nothing but made steps has
    // nothing to tell, and is refused.
    let under_way = !made.is_empty() && busy.contains(&run.skill);
    if under_way && made.len() == run.calls.len() {
        return refuse("I'm still doing that.");
    }
    // The caller told it timed out: nothing is done.
    if !admission.commit() {
        return;
    }
    let _ = reply.send(told(&run.calls));
    for event in run.events {
        publish(out, event);
    }
    if under_way {
        // Each look comes with the head down over the page again: only the
        // run's poses are made again, never its light (#1051, Ten's P2).
        for call in made.iter().filter(|call| matches!(call, Call::Mood { .. })) {
            make(call, &runner.sockets);
        }
    }
    if made.is_empty() || under_way {
        publish(
            out,
            SkillEvent {
                skill: run.skill,
                stage: SkillStage::Done,
                why: if under_way {
                    "the model was told again what to do next; its last run is still under way, left as it is but for its poses, held again"
                } else {
                    "the model was told what to do next"
                }
                .into(),
                allowed: None,
            },
        );
    } else {
        start(run.skill, made, busy, out, runner);
    }
}

/// The skills' tools for the voice: `tools.json` written beside a
/// `call.sock` in `dir`, which the voice finds in the actions part's
/// directory (`docs/tools.md`). No tool, no file.
fn write_tools(dir: &Path, engine: &Engine) {
    let file = tools_file(engine.skills());
    let path = dir.join("tools.json");
    let written = if file["tools"].as_array().is_some_and(Vec::is_empty) {
        std::fs::remove_file(&path).or_else(|error| match error.kind() {
            io::ErrorKind::NotFound => Ok(()),
            _ => Err(error),
        })
    } else {
        // Whole or not at all: the voice may read it as it is written.
        let fresh = dir.join(".tools.json");
        std::fs::write(&fresh, file.to_string()).and_then(|()| std::fs::rename(&fresh, &path))
    };
    if let Err(error) = written {
        eprintln!("lamp-skills: {} not written: {error}", path.display());
    }
}

/// Listens on `dir/call.sock` as the actions part does: one line in
/// (`{"op":"call","name":...}`), one line out, the main loop answering.
fn listen(dir: &Path, asks: mpsc::Sender<ToolAsk>) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let socket = dir.join("call.sock");
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket)?;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let answer = || -> io::Result<()> {
                stream.set_read_timeout(Some(TOOL_TIMEOUT))?;
                stream.set_write_timeout(Some(TOOL_TIMEOUT))?;
                let mut line = String::new();
                BufReader::new((&stream).take(8_192)).read_line(&mut line)?;
                let call: serde_json::Value = serde_json::from_str(&line).unwrap_or_default();
                let (reply, told) = mpsc::channel();
                let admission = Admission::default();
                let result = (call["op"] == "call")
                    .then(|| call["name"].as_str())
                    .flatten()
                    .filter(|name| {
                        asks.send(((*name).to_owned(), reply, admission.clone()))
                            .is_ok()
                    })
                    .and_then(|_| match told.recv_timeout(TOOL_TIMEOUT) {
                        Ok(result) => Some(result),
                        // Late: given up, unless the loop committed first,
                        // whose answer is then being sent.
                        Err(_) if admission.cancel() => None,
                        Err(_) => told.recv_timeout(TOOL_TIMEOUT).ok(),
                    });
                let line = match result {
                    Some(result) => match result.get("error") {
                        Some(error) => json!({"ok": false, "error": error}),
                        None => json!({"ok": true, "result": result}),
                    },
                    None => json!({"ok": false, "error": "My skills did not answer in time."}),
                };
                writeln!(&stream, "{line}")
            };
            if let Err(error) = answer() {
                eprintln!("lamp-skills: a tool call was not answered: {error}");
            }
        }
    });
    Ok(())
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
fn perform(
    skill: &str,
    calls: &[Call],
    sockets: &Sockets,
    bus: &Bus,
    before: Option<Light>,
) -> String {
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
                // The run lasts while the pose is held.
                Call::Mood { lasting_us, .. } => {
                    changed.lasting_us = changed.lasting_us.max(*lasting_us);
                }
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
        said.push(give_back(&before, light_now(bus), sockets));
    }
    said.join("; ")
}

/// The light as it was before the run, from the light shown now: off, or
/// its colour again and its level stepped back. With the light now not
/// known, its colour only.
fn give_back(before: &Light, now: Option<Light>, sockets: &Sockets) -> String {
    let call = |name: &str, args: serde_json::Value| answer(action(&sockets.actions, name, args));
    let mut done = Vec::new();
    if !before.on {
        if now.as_ref().is_none_or(|now| now.on) {
            done.push(call("light_off", json!({})));
        }
    } else {
        if now
            .as_ref()
            .is_none_or(|now| !now.on || now.colour != before.colour)
        {
            done.push(call("light_colour", json!({ "colour": before.colour })));
        }
        match &now {
            Some(now) => {
                let (name, steps) = if now.level < before.level {
                    ("light_brighter", before.level - now.level)
                } else {
                    ("light_dimmer", now.level - before.level)
                };
                for _ in 0..steps {
                    done.push(call(name, json!({})));
                }
            }
            None => done.push("its level is not known, so it is left".into()),
        }
    }
    format!(
        "light given back to {} {} at {}: {}",
        if before.on { "on" } else { "off" },
        before.colour,
        before.level,
        if done.is_empty() {
            "it already was".into()
        } else {
            done.join(", ")
        }
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
        Call::Mood { mood, lasting_us } => {
            // Said again every second, as the body needs it at least every
            // 1.5 s (#1011); when it stops being said, the body comes back.
            let (path, text) = (
                sockets.mood.clone(),
                json!({ "mood": mood, "level": 0 }).to_string(),
            );
            let first = tell(&format!("mood {mood}"), text.clone(), &path);
            let until = Instant::now() + Duration::from_micros(*lasting_us);
            std::thread::spawn(move || {
                while Instant::now() + MOOD_EVERY < until {
                    std::thread::sleep(MOOD_EVERY);
                    let _ = UnixDatagram::unbound()
                        .and_then(|body| body.send_to(text.as_bytes(), &path));
                }
            });
            format!("{first}, held {} s", lasting_us / 1_000_000)
        }
        Call::Say(_) => "say: made by its own step".into(),
        Call::Look => "look: the model's, when it says it will look".into(),
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
