//! `lamp-skills`: the physical skills part (`docs/skills.md`). Loads every
//! skill folder, reloads them when one changes, listens to the topics their
//! triggers name, and runs their steps through the body's sockets, the
//! actions part and the voice's say service.
use lamp_bus::{Bus, Declaration, Kind, Publisher, Subscriber};
use lamp_msgs::{
    Decision, Hand, JointState, Light, LightRequest, Pause, SkillEvent, SkillStage, Speech, Voice,
    VoiceState,
};
use lamp_skills::glow::{GLOW_EVERY, Glow, Glows, REQUESTS_ENVIRONMENT};
use lamp_skills::{
    Admission, CLIP_GAP_US, Call, Engine, clip_us, clock, duration_us, load_all, run_tests, told,
    tools_file,
};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::{
        fs::FileTypeExt,
        net::{UnixDatagram, UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant, SystemTime},
};

const USAGE: &str = "usage: lamp-skills [--dir DIR] [--bus-root DIR] [--gaze-socket PATH] \
                     [--mood-socket PATH] [--actions-socket PATH] [--say-socket PATH] \
                     [--tools-dir DIR] [--boot-id ID] [--clock-control PATH]\n       \
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
/// The longest a call with a look waits for its body and light to be made
/// before it is answered (#1146): within `TOOL_TIMEOUT`, which the caller
/// waits again once the answer is committed.
const MADE_BEFORE_ANSWER: Duration = Duration::from_millis(800);
/// How often a held pose is said again: the body lets one go after 1.5 s.
const MOOD_EVERY: Duration = Duration::from_secs(1);
/// How long an ask waits for the Policy: its own 2 s from the receipt
/// (#958), and a little for the bus each way.
const ASK_WAIT: Duration = Duration::from_millis(2_500);
/// The longest a tool's run waits for the voice to begin the model's line,
/// for steps after the say.
const SPEAK_BEGIN: Duration = Duration::from_secs(5);
/// The longest a tool's line is waited for once the voice has stopped
/// speaking but not ended its turn (`thinking`).
const LINE_LONGEST: Duration = Duration::from_secs(4);
/// How often the voice's state, the hand and the pause are read while a run
/// waits on them.
const POLL: Duration = Duration::from_millis(20);

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
    let mut boot_id = None;
    let mut clock_control = None;
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
            "--boot-id" => boot_id = value.to_str().map(str::to_owned),
            "--clock-control" => clock_control = Some(value),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let ask = std::env::var("LAMP_SKILLS_ASK").unwrap_or_default();
    let options = Options {
        tools,
        ask,
        light_requests: std::env::var(REQUESTS_ENVIRONMENT).is_ok_and(|value| value == "on"),
        boot_id,
        clock_control,
    };
    match serve(&dir, root, &sockets, &options) {
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
    // Stamped on the host's clock, as every part stamps the bus.
    if let Err(error) = out.publish_json(lamp_ipc::monotonic_us(), &event) {
        eprintln!("lamp-skills: skills/events not published: {error}");
    }
}

/// The light shown now, if the ring's owner has said within 3 s (as the
/// actions part counts it).
fn light_now(bus: &Bus) -> Option<Light> {
    said_lately(bus, Light::TOPICS[1])
}

/// The newest value of `topic`, if it was said within 3 s: on the host's
/// wall clock, as its publisher stamped it, never the program's own.
fn said_lately<T: serde::de::DeserializeOwned>(bus: &Bus, topic: &str) -> Option<T> {
    let message = bus.latest(topic).ok()??;
    let now_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64);
    (now_ms.saturating_sub(message.header.at_unix_ms) <= 3_000)
        .then(|| message.parse().ok())
        .flatten()
}

/// The newest message on `topic`, if it was said within the last second.
fn said_lately_message(bus: &Bus, topic: &str) -> Option<lamp_bus::Message> {
    let message = bus.latest(topic).ok()??;
    let now_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64);
    (now_ms.saturating_sub(message.header.at_unix_ms) <= 1_000).then_some(message)
}

/// How the program serves, beyond its sockets.
struct Options {
    tools: Option<PathBuf>,
    /// `LAMP_SKILLS_ASK`: `skip`, `policy`, or anything else for nothing done.
    ask: String,
    /// `LAMP_LIGHT_REQUESTS=on`: a skill's light is asked of the ring.
    light_requests: bool,
    /// `--boot-id`: the boot a light request's claim is on, in place of the
    /// host's own, which only Linux has.
    boot_id: Option<String>,
    /// `--clock-control`: a test's socket to skip the program's clock
    /// ([`clock_control`]).
    clock_control: Option<PathBuf>,
}

/// A skip of the clock asked on `--clock-control`, and where to say it is
/// done.
type Skip = (Duration, mpsc::Sender<Result<(), String>>);

/// Listens on `path` for a test: each line `{"skip_us": N}` skips the
/// program's clock N µs, which the main loop does, and is answered with
/// `{"now_us": ...}` once it is done, or `{"error": ...}`. Production has
/// no such socket.
fn clock_control(path: &Path, skips: mpsc::Sender<Skip>) -> io::Result<()> {
    // An old socket of its own is taken over; anything else at `path` is
    // left, and the bind fails on it.
    if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_socket()) {
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    clock::stepped();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut lines = BufReader::new((&stream).take(8_192));
            let mut line = String::new();
            // Up to 8 KiB a line.
            while {
                lines.get_mut().set_limit(8_192);
                lines.read_line(&mut line).is_ok_and(|read| read > 0)
            } {
                let by = serde_json::from_str::<serde_json::Value>(&line)
                    .ok()
                    .and_then(|asked| asked["skip_us"].as_u64());
                line.clear();
                let answer = match by {
                    Some(by) => {
                        let (done, taken) = mpsc::channel();
                        if skips.send((Duration::from_micros(by), done)).is_err() {
                            return;
                        }
                        match taken.recv() {
                            Ok(Ok(())) => json!({ "now_us": clock::now_us() }),
                            Ok(Err(why)) => json!({ "error": why }),
                            Err(_) => return,
                        }
                    }
                    None => json!({ "error": "a line is {\"skip_us\": N}" }),
                };
                if writeln!(&stream, "{answer}").is_err() {
                    break;
                }
            }
        }
    });
    Ok(())
}

/// The skips a test asked for: what was published before each is taken in
/// first, on the clock as it was, and the Policy's answers with it; then the
/// clock moves on, and an ask it made late is given up, before the skip is
/// answered.
fn take_skips(
    skips: &mpsc::Receiver<Skip>,
    subscribers: &mut [Subscriber],
    engine: &mut Engine,
    busy: &mut HashSet<String>,
    out: &mut Publisher,
    runner: &Runner,
    asking: &mut Asking,
) {
    while let Ok((by, done)) = skips.try_recv() {
        for subscriber in subscribers.iter_mut() {
            while let Some(message) = receive(subscriber, Duration::ZERO) {
                observe(&message, engine, busy, out, runner, asking);
            }
        }
        if let Some((_, answers)) = &mut asking.gate {
            answer_asks(answers, &mut asking.waiting, engine, busy, out, runner);
        }
        let skipped = clock::skip(by);
        // The asks now late are given up before any answer that came since.
        give_up_late(&mut asking.waiting, out);
        if let Some((_, answers)) = &mut asking.gate {
            answer_asks(answers, &mut asking.waiting, engine, busy, out, runner);
        }
        let _ = done.send(skipped);
    }
}

/// The boot the claims are on: `given`, or the host's on Linux.
fn boot_clock(given: Option<&str>) -> io::Result<String> {
    match given {
        Some(id) => Ok(id.to_owned()),
        None if cfg!(target_os = "linux") => {
            Ok(std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?
                .trim()
                .to_owned())
        }
        None => Err(io::Error::other(
            "with LAMP_LIGHT_REQUESTS=on, supply --boot-id on this development host",
        )),
    }
}

fn serve(
    dir: &Path,
    root: Option<PathBuf>,
    sockets: &Sockets,
    options: &Options,
) -> io::Result<()> {
    let tools = options.tools.as_deref();
    let (skip_ask, ask_policy) = (options.ask == "skip", options.ask == "policy");
    let bus = match root {
        Some(root) => Bus::new(root)?,
        None => Bus::from_environment()?,
    };
    let mut out =
        bus.publisher_declared(SkillEvent::TOPIC, &SkillEvent::declaration(), "skills")?;
    let glows = if options.light_requests {
        Some(Glows {
            bus: bus.clone(),
            clock_id: boot_clock(options.boot_id.as_deref())?,
            out: Arc::new(Mutex::new(bus.publisher_declared(
                LightRequest::TOPIC,
                &LightRequest::declaration(),
                "skills",
            )?)),
            every: GLOW_EVERY,
        })
    } else {
        None
    };
    let (finished, done) = mpsc::channel::<SkillEvent>();
    let mut engine = Engine::new(Vec::new(), skip_ask);
    // The Policy's gate (#958): asks out on `skills/ask`, its answers in on
    // `policy/decisions`, by the ask's number. The ask's wire is #958's
    // `lamp_msgs::SkillAsk`, written here as JSON until it is on lamp-voice.
    let mut gate = None;
    if ask_policy {
        engine = engine.asking_the_policy();
        gate = Some((
            bus.publisher_declared(
                "skills/ask",
                &Declaration::new("lamp_msgs/SkillAsk", 1, Kind::Event),
                "skills",
            )?,
            bus.subscribe(Decision::TOPIC, "skills")?,
        ));
    }
    // Runs waiting for the Policy's answer, by the ask's number.
    let mut asking = Asking {
        gate,
        waiting: HashMap::new(),
        last: 0,
    };
    let runner = Runner {
        sockets: sockets.clone(),
        finished,
        bus: bus.clone(),
        glows,
    };
    let (ask, asks) = mpsc::channel::<ToolAsk>();
    if let Some(tools) = tools {
        listen(tools, ask)?;
    }
    let (skip, skips) = mpsc::channel::<Skip>();
    if let Some(path) = &options.clock_control {
        clock_control(path, skip)?;
    }
    // One run a skill at a time: what is under way, by skill.
    let mut busy = HashSet::new();
    let mut subscribers: Vec<Subscriber> = Vec::new();
    let mut seen = None;
    let mut looked: Option<Instant> = None;
    eprintln!(
        "lamp-skills: skills from {}, the Policy's ask {}, a skill's light {}",
        dir.display(),
        if skip_ask {
            "skipped"
        } else if ask_policy {
            "asked on skills/ask"
        } else {
            "not asked: nothing is done"
        },
        if runner.glows.is_some() {
            "asked of the ring on light/request"
        } else {
            "set through the actions part and given back"
        }
    );
    loop {
        while let Ok(event) = done.try_recv() {
            busy.remove(&event.skill);
            publish(&mut out, event);
        }
        take_skips(
            &skips,
            &mut subscribers,
            &mut engine,
            &mut busy,
            &mut out,
            &runner,
            &mut asking,
        );
        if let Some((_, answers)) = &mut asking.gate {
            answer_asks(
                answers,
                &mut asking.waiting,
                &engine,
                &mut busy,
                &mut out,
                &runner,
            );
        }
        if looked.is_none_or(|at| clock::now().saturating_duration_since(at) >= RELOAD_EVERY) {
            looked = Some(clock::now());
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
            if let Ok(call) = asks.recv_timeout(clock::poll(Duration::from_millis(250))) {
                tool_call(call, &mut engine, &mut busy, &mut out, &runner);
            }
            continue;
        }
        let count = subscribers.len() as u64;
        for index in 0..subscribers.len() {
            take_skips(
                &skips,
                &mut subscribers,
                &mut engine,
                &mut busy,
                &mut out,
                &runner,
                &mut asking,
            );
            while let Ok(call) = asks.try_recv() {
                // What was heard before the call, read first: `[again]`
                // asks whether it asked for the call.
                for subscriber in subscribers
                    .iter_mut()
                    .filter(|subscriber| subscriber.topic() == Speech::TOPICS[0])
                {
                    while let Some(message) = receive(subscriber, Duration::ZERO) {
                        observe(
                            &message,
                            &mut engine,
                            &mut busy,
                            &mut out,
                            &runner,
                            &mut asking,
                        );
                    }
                }
                tool_call(call, &mut engine, &mut busy, &mut out, &runner);
            }
            if let Some((_, answers)) = &mut asking.gate {
                answer_asks(
                    answers,
                    &mut asking.waiting,
                    &engine,
                    &mut busy,
                    &mut out,
                    &runner,
                );
            }
            // While the Policy is asked, look for its answer every 5 ms.
            let wait = if asking.waiting.is_empty() {
                Duration::from_millis(250 / count).max(Duration::from_millis(10))
            } else {
                Duration::from_millis(5)
            };
            if let Some(message) = receive(&mut subscribers[index], clock::poll(wait)) {
                observe(
                    &message,
                    &mut engine,
                    &mut busy,
                    &mut out,
                    &runner,
                    &mut asking,
                );
            }
        }
    }
}

/// The Policy's gate, when it is asked: where the asks go and its answers
/// come from, the runs waiting for an answer by the ask's number, and the
/// last ask's number.
struct Asking {
    gate: Option<(Publisher, Subscriber)>,
    waiting: HashMap<u64, (lamp_skills::Run, Instant)>,
    last: u64,
}

/// The next message of one subscriber, within `wait`.
fn receive(subscriber: &mut Subscriber, wait: Duration) -> Option<lamp_bus::Message> {
    match subscriber.recv(wait) {
        Ok(message) => message,
        Err(error) => {
            eprintln!("lamp-skills: {} not received: {error}", subscriber.topic());
            None
        }
    }
}

/// One message to the skills, and the runs it starts, or asks the Policy
/// for.
fn observe(
    message: &lamp_bus::Message,
    engine: &mut Engine,
    busy: &mut HashSet<String>,
    out: &mut Publisher,
    runner: &Runner,
    asking: &mut Asking,
) {
    // Received now, on the program's clock: the publisher's own stamps are
    // on its clock, not this one.
    let runs = engine.observe(
        &message.header.topic,
        clock::now_us(),
        &message.payload,
        message.header.expires_ms,
    );
    for mut run in runs {
        for event in std::mem::take(&mut run.events) {
            publish(out, event);
        }
        if let (Some(question), Some((asks, _))) = (run.ask.take(), &mut asking.gate) {
            ask_policy_for(
                question,
                run,
                asks,
                &runner.bus,
                &mut asking.last,
                &mut asking.waiting,
                out,
            );
        } else if !run.calls.is_empty() {
            let _ = start(run.skill, run.calls, busy, out, runner);
        }
    }
}

/// What a run needs from the program.
struct Runner {
    sockets: Sockets,
    finished: mpsc::Sender<SkillEvent>,
    bus: Bus,
    /// With `LAMP_LIGHT_REQUESTS=on`, how a skill's light is asked of the ring.
    glows: Option<Glows>,
}

/// Starts one run of `skill`, unless its last is still under way.
/// Starts a run on its own thread: what it gives says when its body and
/// light have been made (or the run is over), for a call's answer to wait on.
fn start(
    skill: String,
    calls: Vec<Call>,
    busy: &mut HashSet<String>,
    out: &mut Publisher,
    runner: &Runner,
) -> mpsc::Receiver<()> {
    let (made, made_rx) = mpsc::channel();
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
        return made_rx;
    }
    // A run waits on the voice for as long as the say takes (at most
    // SAY_TIMEOUT a line) or a light's for (at most 10 min): its own
    // thread, one a skill, so the skills go on listening.
    // ponytail: a skill removed meanwhile still finishes its run.
    let (sockets, finished, bus, glows) = (
        runner.sockets.clone(),
        runner.finished.clone(),
        runner.bus.clone(),
        runner.glows.clone(),
    );
    let before = light_now(&bus);
    std::thread::spawn(move || {
        let why = perform(&skill, &calls, &sockets, &bus, before, glows.as_ref(), made);
        let _ = finished.send(SkillEvent {
            skill,
            stage: SkillStage::Done,
            why,
            allowed: None,
        });
    });
    made_rx
}

/// Puts a run's question to the Policy and keeps the run until it answers.
fn ask_policy_for(
    mut question: serde_json::Value,
    run: lamp_skills::Run,
    asks: &mut Publisher,
    bus: &Bus,
    last: &mut u64,
    waiting: &mut HashMap<u64, (lamp_skills::Run, Instant)>,
    out: &mut Publisher,
) {
    // Nobody to answer (lamp-policy down, or without --gate): no at once.
    if bus
        .subscribers("skills/ask")
        .is_ok_and(|subs| subs.is_empty())
    {
        publish(
            out,
            SkillEvent {
                skill: run.skill,
                stage: SkillStage::Answered,
                why: "not asked: nobody is subscribed to skills/ask (lamp-policy --gate)".into(),
                allowed: Some(false),
            },
        );
        return;
    }
    // The ask's number: this clock in µs, never 0 and always above the last.
    let number = (*last + 1).max(clock::now_us());
    *last = number;
    question["ask"] = json!(number);
    match asks.publish_json(1, &question) {
        Ok(_) => {
            waiting.insert(number, (run, clock::now() + ASK_WAIT));
        }
        Err(error) => publish(
            out,
            SkillEvent {
                skill: run.skill,
                stage: SkillStage::Answered,
                why: format!("not asked: skills/ask was not written ({error})"),
                allowed: Some(false),
            },
        ),
    }
}

/// The Policy's answers to the asks waiting: a `say_now` not in shadow
/// starts the run, made again now, if its trigger still holds. Anything
/// else, or nothing within `ASK_WAIT`, leaves it undone.
fn answer_asks(
    answers: &mut Subscriber,
    waiting: &mut HashMap<u64, (lamp_skills::Run, Instant)>,
    engine: &Engine,
    busy: &mut HashSet<String>,
    out: &mut Publisher,
    runner: &Runner,
) {
    while let Ok(Some(message)) = answers.recv(Duration::ZERO) {
        let Ok(decision) = serde_json::from_slice::<serde_json::Value>(&message.payload) else {
            continue;
        };
        let Some((run, _)) = decision["ask"]
            .as_u64()
            .and_then(|number| waiting.remove(&number))
        else {
            continue;
        };
        let choice = decision["choice"].as_str().unwrap_or("");
        // A decision without `shadow` counts as one in shadow: "would say".
        let shadow = decision["shadow"].as_bool() != Some(false);
        let mut why = format!(
            "the Policy answered {choice} ({}, by {})",
            decision["about"].as_str().unwrap_or("?"),
            decision["by"].as_str().unwrap_or("?")
        );
        let mut calls = None;
        if choice == "say_now" && shadow {
            why += " in shadow: nothing is done";
        } else if choice == "say_now" {
            calls = engine.still(&run.skill, clock::now_us());
            if calls.is_none() {
                why += ", but its trigger no longer holds: nothing is done";
            }
        }
        publish(
            out,
            SkillEvent {
                skill: run.skill.clone(),
                stage: SkillStage::Answered,
                why,
                allowed: Some(calls.is_some()),
            },
        );
        if let Some(calls) = calls.filter(|calls| !calls.is_empty()) {
            let _ = start(run.skill, calls, busy, out, runner);
        }
    }
    give_up_late(waiting, out);
}

/// The asks waiting past their `ASK_WAIT`, given up: nothing is done.
fn give_up_late(waiting: &mut HashMap<u64, (lamp_skills::Run, Instant)>, out: &mut Publisher) {
    let now = clock::now();
    let late: Vec<u64> = waiting
        .iter()
        .filter(|(_, (_, until))| *until <= now)
        .map(|(number, _)| *number)
        .collect();
    for number in late {
        if let Some((run, _)) = waiting.remove(&number) {
            publish(
                out,
                SkillEvent {
                    skill: run.skill,
                    stage: SkillStage::Answered,
                    why: "the Policy did not answer in time: nothing is done".into(),
                    allowed: Some(false),
                },
            );
        }
    }
}

/// A call of a skill's tool from the voice, its arguments, and where its
/// answer goes.
type ToolAsk = (
    String,
    serde_json::Value,
    mpsc::Sender<serde_json::Value>,
    Admission,
);

/// The model called a skill's tool: its steps run, and the voice is told
/// what the model is to say: at once, or, with a look, once the body and
/// the light are made (#1146). The say is the model's and the look the
/// voice's, asked in the answer; the body and the light are made here.
fn tool_call(
    (name, args, reply, admission): ToolAsk,
    engine: &mut Engine,
    busy: &mut HashSet<String>,
    out: &mut Publisher,
    runner: &Runner,
) {
    let refuse = |why: &str| {
        if admission.commit() {
            let _ = reply.send(json!({ "error": why }));
        }
    };
    // A run with no pose to hold, under way: refused before it is a run,
    // so nothing moves, the line is not said again, and `[again]`'s while
    // still runs from the run under way (#999: "again!" mid-dance).
    if let Some((skill, still)) = engine.refused_under_way(&name)
        && busy.contains(skill)
    {
        eprintln!("lamp-skills: {name} with {args} not run: its run is under way");
        return refuse(still);
    }
    let run = match engine.tool(&name, &args, clock::now_us()) {
        Ok(run) => run,
        Err(why) => {
            eprintln!("lamp-skills: {name} with {args} not run: {why}");
            return refuse(&why);
        }
    };
    let made: Vec<Call> = run
        .calls
        .iter()
        .filter(|call| !matches!(call, Call::Look | Call::Say(_) | Call::SayFrom(_)))
        .cloned()
        .collect();
    // Nothing but the wait for the say is nothing to make.
    let nothing = made.iter().all(|call| *call == Call::AfterSay);
    // A run under way is left as it is: a call again meanwhile is told
    // again what to do next (a student who asks a minute later gets a new
    // look), and nothing more is made; the light already on is given back
    // when its `for` ends. A call that is nothing but made steps has
    // nothing to tell, and is refused.
    let under_way = !nothing && busy.contains(&run.skill);
    if under_way && made.len() == run.calls.len() {
        return refuse("I'm still doing that.");
    }
    // The caller told it timed out: nothing is done.
    if !admission.commit() {
        return;
    }
    // A call with a look is answered only once its body and light are made:
    // the voice asks the eyes as the answer's first audio plays, and by
    // then the head is on its way down over the page (#941, #1146). Any
    // other call is answered at once.
    let told = told(&run.calls);
    let looks = run.calls.contains(&Call::Look);
    if !looks {
        let _ = reply.send(told.clone());
    }
    for event in run.events {
        publish(out, event);
    }
    if under_way {
        // Each look comes with the head down over the page again: only the
        // run's poses are made again, never its light (#1051, Ten's P2).
        for call in made.iter().filter(|call| matches!(call, Call::Mood { .. })) {
            make(call, &runner.sockets, &runner.bus);
        }
    }
    if nothing || under_way {
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
        let made = start(run.skill, made, busy, out, runner);
        if looks {
            // Bounded: the caller waits at most `TOOL_TIMEOUT` once more.
            let _ = clock::recv_timeout(&made, MADE_BEFORE_ANSWER);
        }
    }
    if looks {
        let _ = reply.send(told);
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
/// (`{"op":"call","name":...,"args":{...}}`), one line out, the main loop
/// answering.
fn listen(dir: &Path, asks: mpsc::Sender<ToolAsk>) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let socket = dir.join("call.sock");
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket)?;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            // On the host's clock, as the socket timeouts to the other
            // parts are: the voice gives up on a call on its own.
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
                        asks.send((
                            (*name).to_owned(),
                            call["args"].clone(),
                            reply,
                            admission.clone(),
                        ))
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

/// What the body was told in this run: until when its last clip plays, and
/// why no more are sent, once the body was pushed or the lamp paused.
#[derive(Default)]
struct Played {
    /// The run's last clip sent.
    last: Option<Sent>,
    stopped: Option<String>,
}

/// A clip sent to the body, for the next to wait on: when it was sent, how
/// long the next waits from when it began (its length and `CLIP_GAP_US`),
/// and the body's `commanded` pose and its message's number just before.
struct Sent {
    at: Instant,
    rest: Duration,
    pose: Option<(u64, Vec<f64>)>,
}

impl Sent {
    /// When the next may be sent, the clip taken to begin as it was sent.
    fn until(&self) -> Instant {
        self.at + self.rest
    }
}

/// How long after a clip is sent the body may begin it: a gesture asked of
/// a lamp still rising waits up to 3 s for the rise (`crates/body/src/body.rs`,
/// `GESTURE_WAIT`), and `body/joint_states` comes every 100 ms.
const BEGIN_WITHIN: Duration = Duration::from_millis(3_200);
/// How far a commanded joint moves before the clip is taken to have begun:
/// about 3 of the servo's 4,096 steps a turn.
const BEGIN_RAD: f64 = 0.005;

/// The body's `commanded` pose now, and its message's number, if the body
/// said it within the last second.
fn commanded_now(bus: &Bus) -> Option<(u64, Vec<f64>)> {
    let message = said_lately_message(bus, JointState::TOPIC)?;
    let seq = message.header.seq;
    let state = message.parse::<JointState>().ok()?;
    (!state.commanded.is_empty()).then_some((seq, state.commanded))
}

/// Runs one skill's calls and says how each went. With a say, the others
/// are made when the voice begins it, so the look and the light come with
/// the words, and none if it is never begun. Those after `Call::AfterSay`
/// are made when the say ends: the skill's own, or in a tool's run, the
/// line the model says. A light changed is given back when the say ends,
/// after its `for`, or when the last clip ends, whichever is last. With
/// `glows`, the light is asked of the ring instead, and given back the same
/// way ([`Glow`]).
fn perform(
    skill: &str,
    calls: &[Call],
    sockets: &Sockets,
    bus: &Bus,
    before: Option<Light>,
    glows: Option<&Glows>,
    made: mpsc::Sender<()>,
) -> String {
    let is_light = |call: &Call| matches!(call, Call::Light { .. } | Call::Dim { .. });
    let mut said = Vec::new();
    // Asked of the ring, a light is never the person's to give back.
    let mut glow = glows
        .filter(|_| calls.iter().any(is_light))
        .map(|glows| Glow::new(glows, skill, before.clone()));
    if glow.is_none() && before.is_none() && calls.iter().any(is_light) {
        said.push(
            "light: left alone, since the light shown is not known and could not be given back"
                .into(),
        );
    }
    let (first, after) = match calls.iter().position(|call| *call == Call::AfterSay) {
        Some(at) => (&calls[..at], &calls[at + 1..]),
        None => (calls, &[][..]),
    };
    let keep = |calls: &[Call]| -> Vec<Call> {
        calls
            .iter()
            .filter(|call| {
                !matches!(call, Call::Say(_) | Call::SayFrom(_) | Call::Sound { .. })
                    && (glow.is_some() || before.is_some() || !is_light(call))
            })
            .cloned()
            .collect()
    };
    let (first, after) = (keep(first), keep(after));
    // What the voice is asked to say: a note in its own words, or words it
    // keeps itself (#939); or to play the skill's own sound (#938), which
    // it plays at once or refuses at once, with why.
    let asking = calls.iter().find_map(|call| match call {
        Call::Say(note) => Some(("say", json!({"note": note, "priority": 5}))),
        Call::SayFrom(source) => Some((
            "say",
            json!({"from": source.name(), "priority": source.priority()}),
        )),
        Call::Sound { path, .. } => Some(("sound", json!({"sound": path, "priority": 5}))),
        _ => None,
    });
    let mut changed = Changed::default();
    let mut played = Played::default();
    match asking {
        Some((what, asking)) => {
            let mut begun = Vec::new();
            let mut was_begun = false;
            let outcome = say(&sockets.say, what, skill, asking, || {
                was_begun = true;
                act(
                    &first,
                    sockets,
                    bus,
                    &mut changed,
                    &mut played,
                    &mut glow,
                    &mut begun,
                );
                let _ = made.send(());
            });
            said.extend(begun);
            said.push(outcome);
            // Nothing after a say never begun, as nothing with it.
            if was_begun {
                act(
                    &after,
                    sockets,
                    bus,
                    &mut changed,
                    &mut played,
                    &mut glow,
                    &mut said,
                );
            }
        }
        None => {
            act(
                &first,
                sockets,
                bus,
                &mut changed,
                &mut played,
                &mut glow,
                &mut said,
            );
            let _ = made.send(());
            if !after.is_empty() {
                let (how, cut) = spoken(bus);
                said.push(how);
                if cut {
                    // They spoke over the line: the lamp stops and listens.
                    said.push(
                        "the line was cut (the voice went from speaking to listening): nothing after it is made"
                            .into(),
                    );
                } else {
                    act(
                        &after,
                        sockets,
                        bus,
                        &mut changed,
                        &mut played,
                        &mut glow,
                        &mut said,
                    );
                }
            }
            clock::sleep(Duration::from_micros(changed.lasting_us));
        }
    }
    if let Some(last) = &played.last {
        clock::sleep(last.until().saturating_duration_since(clock::now()));
    }
    match glow {
        Some(glow) => said.extend(glow.give_back()),
        None => {
            if let Some(before) = before
                && (changed.coloured || changed.dimmed > 0)
            {
                said.push(give_back(&before, light_now(bus), sockets));
            }
        }
    }
    said.join("; ")
}

/// Makes `calls` in order. A clip after another waits for that one to end,
/// and is not sent, nor any after it, if meanwhile the body was pushed or
/// the lamp paused: the body's own stops hold for the whole run.
fn act(
    calls: &[Call],
    sockets: &Sockets,
    bus: &Bus,
    changed: &mut Changed,
    played: &mut Played,
    glow: &mut Option<Glow>,
    said: &mut Vec<String>,
) {
    for call in calls {
        match call {
            Call::Light { lasting, .. } => {
                changed.coloured = true;
                changed.lasting_us = changed.lasting_us.max(
                    lasting
                        .as_deref()
                        .and_then(|text| duration_us(text).ok())
                        .unwrap_or(0),
                );
            }
            Call::Dim { steps, .. } => changed.dimmed += steps,
            // The run lasts while the pose is held.
            Call::Mood { lasting_us, .. } => {
                changed.lasting_us = changed.lasting_us.max(*lasting_us);
            }
            Call::Gesture(_) | Call::Move(_) => {
                let (what, length_us) = match call {
                    Call::Gesture(name) => (format!("gesture {name}"), clip_us(name)),
                    Call::Move(clip) => (
                        format!("move {}", clip.name),
                        Some(
                            (clip.frames.len().saturating_sub(1) as u64)
                                * u64::from(clip.step_ms)
                                * 1_000,
                        ),
                    ),
                    _ => unreachable!(),
                };
                if let Some(last) = played.last.take()
                    && played.stopped.is_none()
                {
                    played.stopped = held_back(&last, bus);
                }
                if let Some(why) = &played.stopped {
                    said.push(format!("{what}: not sent, {why}"));
                    continue;
                }
                // The next clip waits for this one's length and a few of
                // the body's steps more, from when the body began it, until
                // the body can take it.
                played.last = length_us.map(|us| Sent {
                    at: clock::now(),
                    rest: Duration::from_micros(us + CLIP_GAP_US),
                    pose: commanded_now(bus),
                });
            }
            _ => {}
        }
        said.push(match glow {
            Some(glow) if matches!(call, Call::Light { .. } | Call::Dim { .. }) => glow.ask(call),
            _ => make(call, sockets, bus),
        });
    }
}

/// Waits while the clip `sent` plays, until its length and `CLIP_GAP_US`
/// after the body began it: when `body/joint_states` first shows its
/// `commanded` pose leave the one before the send, within `BEGIN_WITHIN`,
/// as a lamp rising at the line's end begins it late. Without that, from
/// when it was sent. A folded lamp the body stands up for the clip (#1080)
/// moves as it rises, so its clip is timed from the rise's start: as from
/// the send. Why the next is not sent, if the body was pushed or the lamp
/// paused meanwhile.
fn held_back(sent: &Sent, bus: &Bus) -> Option<String> {
    let mut until = sent.until();
    let mut began = false;
    loop {
        if !began
            && clock::now().saturating_duration_since(sent.at) <= BEGIN_WITHIN
            && let (Some((seq, pose)), Some((seq_now, now))) = (&sent.pose, commanded_now(bus))
            && seq_now != *seq
            && now.len() == pose.len()
            && now
                .iter()
                .zip(pose)
                .any(|(now, was)| (now - was).abs() > BEGIN_RAD)
        {
            began = true;
            until = until.max(clock::now() + sent.rest);
        }
        if bus
            .latest(Hand::TOPIC)
            .ok()
            .flatten()
            .and_then(|message| message.parse::<Hand>().ok())
            .is_some_and(|hand| hand.pushed)
        {
            return Some("the body was pushed".into());
        }
        if bus
            .latest(Pause::TOPIC)
            .ok()
            .flatten()
            .and_then(|message| message.parse::<Pause>().ok())
            .is_some_and(|pause| pause.paused)
        {
            return Some("the lamp was paused".into());
        }
        let left = until.saturating_duration_since(clock::now());
        if left.is_zero() {
            return None;
        }
        clock::sleep(left.min(POLL));
    }
}

/// Waits for the model's line after a tool's call: for the voice to begin
/// speaking, at most SPEAK_BEGIN, and then for its turn to end: `resting` or
/// `listening`. `thinking` after `speaking` is still the turn, since the
/// sound plays on (lamp-06c1, 2026-10-16: `speaking` for 20 ms, then
/// `thinking` for 15 s, through a line of about 2 s). So the wait ends at
/// the latest LINE_LONGEST after the speaking began, unless the voice is
/// speaking then. At most SAY_TIMEOUT in all. What it waited for, in words,
/// and whether the line was cut: a line said to its end leaves the voice
/// `resting`, one the person spoke over leaves it `listening`.
fn spoken(bus: &Bus) -> (String, bool) {
    let state = || said_lately::<VoiceState>(bus, VoiceState::TOPIC).map(|now| now.state);
    let asked = clock::now();
    let elapsed = |since: Instant| clock::now().saturating_duration_since(since);
    loop {
        match state() {
            Some(Voice::Speaking) => break,
            None => {
                return (
                    "after the say: the voice's state is not known, so at once".into(),
                    false,
                );
            }
            Some(_) if elapsed(asked) >= SPEAK_BEGIN => {
                return (
                    format!(
                        "after the say: the voice did not begin within {} s",
                        SPEAK_BEGIN.as_secs()
                    ),
                    false,
                );
            }
            Some(_) => clock::sleep(POLL),
        }
    }
    let begun = clock::now();
    let cut = loop {
        let now = state();
        let over = match now {
            Some(Voice::Speaking) => false,
            Some(Voice::Thinking) => elapsed(begun) >= LINE_LONGEST,
            _ => true,
        };
        if over {
            break now == Some(Voice::Listening);
        }
        if elapsed(begun) >= SAY_TIMEOUT {
            return ("after the say: the voice was still speaking".into(), false);
        }
        clock::sleep(POLL);
    };
    (
        format!(
            "after the say: spoken from {} ms after the call, the turn over {} ms later",
            (begun - asked).as_millis(),
            elapsed(begun).as_millis()
        ),
        cut,
    )
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
            Some(now) => done.extend(step(now.level, before.level, sockets)),
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

/// Steps the light from level `from` to `to`, a `light_brighter` or
/// `light_dimmer` call a level, and says how each went.
fn step(from: u8, to: u8, sockets: &Sockets) -> Vec<String> {
    let (name, steps) = if from < to {
        ("light_brighter", to - from)
    } else {
        ("light_dimmer", from - to)
    };
    (0..steps)
        .map(|_| answer(action(&sockets.actions, name, json!({}))))
        .collect()
}

fn answer(result: io::Result<String>) -> String {
    result.unwrap_or_else(|error| format!("the actions part was not reached ({error})"))
}

/// Asks the voice to say `asking`, a `note` or what it keeps `from`, or to
/// play a `sound`, with its `priority` (#939's say service: one line in, a
/// `begun` line when the turn starts, then the outcome). `begun` is called
/// with the first. What became of it, in words, after `what`.
fn say(
    path: &Path,
    what: &str,
    skill: &str,
    asking: serde_json::Value,
    mut begun: impl FnMut(),
) -> String {
    let mut attempt = || -> io::Result<String> {
        let mut stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(SAY_TIMEOUT))?;
        stream.set_write_timeout(Some(ACTIONS_TIMEOUT))?;
        let mut ask = asking.clone();
        ask["skill"] = json!(skill);
        ask["expires_in_ms"] = json!(SAY_EXPIRES_MS);
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
        Ok(outcome) => format!("{what}: {outcome}"),
        Err(error) => format!("{what}: the voice's say service was not reached ({error})"),
    }
}

/// Makes one call and says how it went.
fn make(call: &Call, sockets: &Sockets, bus: &Bus) -> String {
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
        Call::Move(clip) => tell(
            &format!("move {}", clip.name),
            json!({ "move": clip }).to_string(),
            &sockets.mood,
        ),
        Call::Light { colour, level, .. } => {
            let mut said = actions(
                &format!("light {colour}"),
                "light_colour",
                json!({ "colour": colour }),
            );
            // No call sets a level: it is stepped to, from the level shown.
            if let Some(level) = level {
                match light_now(bus) {
                    Some(now) if now.level == *level => {}
                    Some(now) => {
                        said.push_str(&format!(", to level {level}: "));
                        said.push_str(&step(now.level, *level, sockets).join(", "));
                    }
                    None => said.push_str(", its level is not known, so it is left"),
                }
            }
            said
        }
        // Held until the run ends, `until = "ends"` or not: `perform` gives
        // it back then.
        Call::Dim { steps, .. } => (0..*steps)
            .map(|_| actions("dim", "light_dimmer", json!({})))
            .collect::<Vec<_>>()
            .join(", "),
        Call::Mood { mood, lasting_us } => {
            // Said again every second, as the body needs it at least every
            // 1.5 s (#1011); when it stops being said, the body comes back.
            let (path, text) = (
                sockets.mood.clone(),
                json!({ "mood": mood, "level": 0 }).to_string(),
            );
            let first = tell(&format!("mood {mood}"), text.clone(), &path);
            let until = clock::now() + Duration::from_micros(*lasting_us);
            std::thread::spawn(move || {
                while clock::now() + MOOD_EVERY < until {
                    clock::sleep(MOOD_EVERY);
                    let _ = UnixDatagram::unbound()
                        .and_then(|body| body.send_to(text.as_bytes(), &path));
                }
            });
            format!("{first}, held {} s", lasting_us / 1_000_000)
        }
        Call::Say(_) | Call::SayFrom(_) => "say: made by its own step".into(),
        Call::Sound { .. } => "sound: made by its own step".into(),
        Call::Look => "look: the voice's, asked in the call's answer".into(),
        Call::AfterSay => "after the say".into(),
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
