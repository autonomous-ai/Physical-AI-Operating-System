//! Every skill folder in `lampOS/skills/` against its own recordings, the
//! loader's refusals, and the program on a real bus with the body's and the
//! actions part's sockets stood in for.
use lamp_bus::Bus;
use lamp_msgs::{Faces, Light, SkillEvent, SkillStage};
use lamp_skills::{Admission, Call, Engine, load, load_all, read_recordings, run_tests};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixDatagram, UnixListener},
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::mpsc,
    time::{Duration, Instant},
};

fn skills_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills")
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A copy of Welcome back in `dir`, with one line of its `skill.toml` changed.
fn welcome_back_with(dir: &Path, from: &str, to: &str) -> PathBuf {
    let skill = dir.join("welcome-back");
    copy(&skills_root().join("welcome-back"), &skill);
    let toml = fs::read_to_string(skill.join("skill.toml")).unwrap();
    assert!(toml.contains(from), "{from}");
    fs::write(skill.join("skill.toml"), toml.replace(from, to)).unwrap();
    skill
}

#[test]
fn every_skill_folder_passes_its_own_recorded_tests() {
    let mut skills = 0;
    for entry in fs::read_dir(skills_root()).unwrap().flatten() {
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        // Every folder loads; a case with no recording yet is not run.
        load(&entry.path()).unwrap_or_else(|why| panic!("{}: {why}", entry.path().display()));
        let (passed, _unrecorded) = run_tests(&entry.path())
            .unwrap_or_else(|why| panic!("{}: {why}", entry.path().display()));
        skills += passed;
    }
    assert!(skills >= 1);
}

#[test]
fn a_recorded_case_fails_when_the_skill_no_longer_does_what_it_expects() {
    let dir = tempfile::tempdir().unwrap();
    // Without the cooldown the lamp greets again at 1,400 s, which the
    // recording says it must not.
    let skill = welcome_back_with(dir.path(), "cooldown = \"20m\"", "cooldown = \"0s\"");
    let why = run_tests(&skill).unwrap_err();
    assert!(why.contains("back-after-ten-minutes"), "{why}");
    // A red light where warm was recorded.
    let skill = welcome_back_with(&dir.path().join("red"), "needs", "needs");
    fs::write(skill.join("lights/warm_glow.toml"), "colour = \"red\"\n").unwrap();
    let why = run_tests(&skill).unwrap_err();
    assert!(why.contains("call 2 is not \"light\""), "{why}");
}

#[test]
fn without_the_skip_nothing_the_skill_starts_is_done() {
    let skill = load(&skills_root().join("welcome-back")).unwrap();
    let mut engine = Engine::new(vec![skill], false);
    let case = skills_root().join("welcome-back/tests/back-after-ten-minutes");
    let mut runs = Vec::new();
    for message in read_recordings(&case).unwrap() {
        runs.extend(engine.observe(
            &message.header.topic,
            message.header.at_us,
            &message.payload,
            message.header.expires_ms,
        ));
    }
    // Each arrival after five minutes away, refused. A refusal does not
    // start the cooldown, so the one at 1,400 s is asked too.
    assert_eq!(runs.len(), 3);
    for run in runs {
        assert!(run.calls.is_empty());
        let stages: Vec<_> = run.events.iter().map(|event| event.stage).collect();
        assert_eq!(
            stages,
            [
                SkillStage::Triggered,
                SkillStage::Asked,
                SkillStage::Answered
            ]
        );
        assert_eq!(run.events[2].allowed, Some(false));
        run.events.iter().for_each(|event| event.check().unwrap());
    }
}

#[test]
fn the_loader_refuses_what_it_cannot_do() {
    let dir = tempfile::tempdir().unwrap();
    for (from, to, why) in [
        ("when = \"appears\"", "when = \"stays\"", "only \"appears\""),
        (
            "topic = \"humans/faces/tracked\"",
            "topic = \"air/state\"",
            "a field and one of",
        ),
        (
            "absent_for = \"5m\"",
            "absent_for = \"five minutes\"",
            "not a duration",
        ),
        (
            "cooldown = \"20m\"",
            "cooldown = \"20m\"\nsound = \"chime\"",
            "unknown field",
        ),
        ("look_at = \"face\"", "gesture = \"dance\"", "not one of"),
        ("[ask]", "[later]", "unknown field"),
        (
            "[ask]",
            "[[do]]\nsay = \"And again.\"\n[ask]",
            "one say a skill",
        ),
    ] {
        let skill = welcome_back_with(&dir.path().join(why.replace(' ', "_")), from, to);
        let error = load(&skill).unwrap_err();
        assert!(error.contains(why), "{to}: {error}");
    }
    let skill = welcome_back_with(&dir.path().join("light"), "needs", "needs");
    fs::write(skill.join("lights/warm_glow.toml"), "colour = \"gold\"\n").unwrap();
    assert!(load(&skill).unwrap_err().contains("not one of"));
    let skill = welcome_back_with(&dir.path().join("run"), "needs", "needs");
    fs::write(skill.join("run.py"), "").unwrap();
    assert!(load(&skill).unwrap_err().contains("run.py"));
    let skill = dir.path().join("renamed/hello");
    copy(&skills_root().join("welcome-back"), &skill);
    assert!(load(&skill).unwrap_err().contains("folder"));
}

struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// What the program did with one face.
struct Seen {
    events: Vec<SkillEvent>,
    looked: Option<String>,
    /// Each call to the actions part, in order.
    lights: Vec<String>,
    /// What the voice was asked to say.
    asked: Option<String>,
    /// What the body's mood socket was told, for a skill that holds a pose.
    moods: Vec<String>,
}

/// A hello that greets at once, with the light and the say of Welcome back.
const AT_ONCE: &str = r#"
[trigger]
topic = "humans/faces/tracked"
when = "appears"
near = 0.14
[ask]
policy = "hello"
[[do]]
body = { look_at = "face" }
light = { colour = "warm" }
say = "Greet them."
"#;

/// The program on a bus of its own, its skills folder empty until a hello
/// that greets at once is put in it. The light shows blue at 3. One face is
/// published, and the voice's say service answers with `answers`, a line
/// each, 200 ms apart.
fn the_program(skip: bool, answers: &'static [&'static str]) -> Seen {
    the_program_with(AT_ONCE, skip, answers, 3, 1, if skip { 4 } else { 3 })
}

/// The same with this skill, the light blue at `level`, `faces` lists with a
/// face 50 ms apart, until `wanted` events have come. The actions part
/// keeps a light as the real one does, and says it on `light/state`.
fn the_program_with(
    skill: &str,
    skip: bool,
    answers: &'static [&'static str],
    level: u8,
    faces: usize,
    wanted: usize,
) -> Seen {
    // Short: a socket's whole path has to fit in 103 bytes.
    let dir = tempfile::Builder::new()
        .prefix("sk")
        .tempdir_in("/tmp")
        .unwrap();
    let root = dir.path();
    fs::create_dir(root.join("skills")).unwrap();
    let bus = Bus::new(root.join("bus")).unwrap();
    let mut events = bus.subscribe(SkillEvent::TOPIC, "test").unwrap();
    let gaze = UnixDatagram::bind(root.join("gaze.sock")).unwrap();
    gaze.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let mood = UnixDatagram::bind(root.join("mood.sock")).unwrap();
    mood.set_read_timeout(Some(Duration::from_millis(1_500)))
        .unwrap();
    let actions = UnixListener::bind(root.join("actions.sock")).unwrap();
    let (called, lights) = mpsc::channel();
    let ring = bus.clone();
    std::thread::spawn(move || {
        let mut shown = ring
            .publisher_declared("light/state", &Light::declaration(), "ring")
            .unwrap();
        let mut light = Light::new(true, "blue", level).unwrap();
        for stream in actions.incoming().flatten() {
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let call: serde_json::Value = serde_json::from_str(&line).unwrap();
            match call["name"].as_str().unwrap() {
                "light_colour" => {
                    light.on = true;
                    light.colour = call["args"]["colour"].as_str().unwrap().into();
                }
                "light_dimmer" => light.level = light.level.saturating_sub(1).max(1),
                "light_brighter" => light.level = (light.level + 1).min(5),
                "light_off" => light.on = false,
                _ => {}
            }
            shown.publish_json(1, &light).unwrap();
            (&stream)
                .write_all(b"{\"ok\":true,\"result\":{}}\n")
                .unwrap();
            let _ = called.send(line);
        }
    });
    let voice = UnixListener::bind(root.join("say.sock")).unwrap();
    let (heard, asked) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in voice.incoming().flatten() {
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let _ = heard.send(line);
            for answer in answers {
                std::thread::sleep(Duration::from_millis(200));
                (&stream)
                    .write_all(format!("{answer}\n").as_bytes())
                    .unwrap();
            }
        }
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_lamp-skills"));
    command.args([
        "--dir".as_ref(),
        root.join("skills").as_os_str(),
        "--bus-root".as_ref(),
        root.join("bus").as_os_str(),
        "--gaze-socket".as_ref(),
        root.join("gaze.sock").as_os_str(),
        "--mood-socket".as_ref(),
        root.join("mood.sock").as_os_str(),
        "--actions-socket".as_ref(),
        root.join("actions.sock").as_os_str(),
        "--say-socket".as_ref(),
        root.join("say.sock").as_os_str(),
    ]);
    command.env_remove("LAMP_SKILLS_ASK");
    if skip {
        command.env("LAMP_SKILLS_ASK", "skip");
    }
    let _running = Running(command.spawn().unwrap());
    std::thread::sleep(Duration::from_millis(300));
    // Put in after the start: found by the reload.
    skill_with(&root.join("skills"), "hello", skill);
    let deadline = Instant::now() + Duration::from_secs(10);
    while bus.subscribers(Faces::TOPIC).unwrap().is_empty() {
        assert!(Instant::now() < deadline, "the skill was never loaded");
        std::thread::sleep(Duration::from_millis(50));
    }
    // The light as shown just before the face, fresh.
    bus.publisher_declared("light/state", &Light::declaration(), "ring")
        .unwrap()
        .publish_json(1, &Light::new(true, "blue", level).unwrap())
        .unwrap();
    let mut eyes = bus
        .publisher_declared(Faces::TOPIC, &Faces::declaration(), "eyes")
        .unwrap();
    for _ in 0..faces {
        eyes.publish_json(
            1,
            &serde_json::json!({"faces": [{"id": "a1", "xmin": 0.25, "ymin": 0.25,
                "xmax": 0.75, "ymax": 0.5, "confidence": 0.9}]}),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut seen = Vec::new();
    while seen.len() < wanted && Instant::now() < deadline {
        if let Some(message) = events.recv(Duration::from_millis(100)).unwrap() {
            seen.push(message.parse::<SkillEvent>().unwrap());
        }
    }
    let mut buffer = [0u8; 512];
    let looked = gaze
        .recv(&mut buffer)
        .ok()
        .map(|length| String::from_utf8_lossy(&buffer[..length]).into_owned());
    // A held pose is said again every second until its for ends.
    let mut moods = Vec::new();
    while skill.contains("mood =")
        && let Ok(length) = mood.recv(&mut buffer)
    {
        moods.push(String::from_utf8_lossy(&buffer[..length]).into_owned());
    }
    Seen {
        events: seen,
        looked,
        lights: lights.try_iter().collect(),
        asked: asked.try_recv().ok(),
        moods,
    }
}

/// A light that dims a step while it greets.
const DIMS: &str = r#"
[trigger]
topic = "humans/faces/tracked"
when = "appears"
[ask]
policy = "hello"
[[do]]
light = { dim = 1 }
say = "Greet them."
"#;

#[test]
fn a_dim_at_the_lowest_level_changes_nothing_and_is_given_back_as_nothing() {
    let seen = the_program_with(
        DIMS,
        true,
        &[r#"{"event":"begun","at_us":5}"#, r#"{"outcome":"spoken"}"#],
        1,
        1,
        4,
    );
    let done = &seen.events[3].why;
    assert!(
        done.contains("light given back to on blue at 1: it already was"),
        "{done}"
    );
    assert_eq!(seen.lights.len(), 1, "{:?}", seen.lights);
    // At 3 the dim is given back with one step brighter.
    let seen = the_program_with(
        DIMS,
        true,
        &[r#"{"event":"begun","at_us":5}"#, r#"{"outcome":"spoken"}"#],
        3,
        1,
        4,
    );
    assert!(
        seen.lights[1].contains("light_brighter"),
        "{:?}",
        seen.lights
    );
    assert_eq!(seen.lights.len(), 2, "{:?}", seen.lights);
}

#[test]
fn a_skill_runs_once_at_a_time_and_says_so_when_it_cannot() {
    // Present and no cooldown: every list with a face triggers it, while the
    // first say still waits on the voice.
    let present = AT_ONCE.replace("when = \"appears\"", "when = \"present\"");
    let seen = the_program_with(
        &present,
        true,
        &[r#"{"event":"begun","at_us":5}"#, r#"{"outcome":"spoken"}"#],
        3,
        3,
        12,
    );
    let refused = seen
        .events
        .iter()
        .filter(|event| event.why == "not made: its last run is still under way")
        .count();
    assert_eq!(refused, 2, "{:?}", seen.events);
}

fn stages(events: &[SkillEvent]) -> Vec<SkillStage> {
    events.iter().map(|event| event.stage).collect()
}

#[test]
fn the_program_looks_and_lights_with_the_words_and_gives_the_light_back() {
    let seen = the_program(
        true,
        &[r#"{"event":"begun","at_us":5}"#, r#"{"outcome":"spoken"}"#],
    );
    assert_eq!(
        stages(&seen.events),
        [
            SkillStage::Triggered,
            SkillStage::Asked,
            SkillStage::Answered,
            SkillStage::Done
        ]
    );
    assert_eq!(seen.events[2].allowed, Some(true));
    let done = &seen.events[3].why;
    assert!(done.contains("look at the face: told the body"), "{done}");
    assert!(done.contains("light warm: done"), "{done}");
    assert!(done.contains("say: spoken"), "{done}");
    assert!(done.contains("light given back to on blue at 3"), "{done}");
    assert_eq!(
        seen.asked.as_deref(),
        Some(
            "{\"expires_in_ms\":30000,\"note\":\"Greet them.\",\"priority\":5,\"skill\":\"hello\"}\n"
        )
    );
    assert_eq!(
        seen.looked.as_deref(),
        Some(r#"{"face":{"size":0.25,"turned":0.0,"x":0.0,"y":-0.25}}"#)
    );
    assert_eq!(
        seen.lights,
        [
            "{\"args\":{\"colour\":\"warm\"},\"name\":\"light_colour\",\"op\":\"call\"}\n",
            "{\"args\":{\"colour\":\"blue\"},\"name\":\"light_colour\",\"op\":\"call\"}\n"
        ]
    );
}

#[test]
fn a_say_never_begun_moves_nothing_and_changes_no_light() {
    let seen = the_program(true, &[r#"{"outcome":"expired"}"#]);
    assert_eq!(stages(&seen.events).last(), Some(&SkillStage::Done));
    let done = &seen.events[3].why;
    assert_eq!(done, "say: expired");
    assert!(seen.asked.is_some());
    assert_eq!((seen.looked, seen.lights.len()), (None, 0));
}

#[test]
fn the_program_does_nothing_without_the_skip() {
    let seen = the_program(false, &[]);
    assert_eq!(
        stages(&seen.events),
        [
            SkillStage::Triggered,
            SkillStage::Asked,
            SkillStage::Answered
        ]
    );
    assert_eq!(seen.events[2].allowed, Some(false));
    assert_eq!(
        (seen.looked, seen.lights.len(), seen.asked),
        (None, 0, None)
    );
}

/// A skill folder named `name` in `dir` with this `skill.toml`.
fn skill_with(dir: &Path, name: &str, toml: &str) -> PathBuf {
    let skill = dir.join(name);
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: A test.\n---\n"),
    )
    .unwrap();
    fs::write(skill.join("skill.toml"), toml).unwrap();
    skill
}

const FACE: &str =
    r#"{"faces":[{"id":"a","xmin":0.25,"ymin":0.25,"xmax":0.75,"ymax":0.5,"confidence":0.9}]}"#;
const NOBODY: &str = r#"{"faces":[]}"#;

/// Stretch break as the skill creator wrote it (#947), its condition with
/// the field named.
const STRETCH: &str = r#"
needs = ["eyes", "body", "light"]
[trigger]
topic = "humans/faces/tracked"
when = "present"
for = "50m"
gap = "2m"
rearm = "leaves"
cooldown = "90m"
[[while]]
topic = "voice/state"
field = "state"
equals = "resting"
[ask]
policy = "nudge"
[[do]]
body = { look_at = "face" }
[[do]]
body = { gesture = "stretching" }
[[do]]
light = { colour = "warm", level = 2, for = "4s" }
"#;

/// Messages as (seconds, topic, payload); the seconds of each run and its calls.
fn runs_at(skill: &Path, messages: &[(u64, &str, &str)]) -> Vec<(u64, Vec<Call>)> {
    let mut engine = Engine::new(vec![load(skill).unwrap()], true);
    let mut runs = Vec::new();
    for (at, topic, payload) in messages {
        for run in engine.observe(topic, at * 1_000_000, payload.as_bytes(), None) {
            runs.push((*at, run.calls));
        }
    }
    runs
}

/// Someone at the desk each minute from `from` to `to` seconds, gone for
/// 30 s after each of `away`.
fn sitting(from: u64, to: u64, away: &[u64]) -> Vec<(u64, &'static str, &'static str)> {
    let mut messages = Vec::new();
    for at in (from..=to).step_by(60) {
        messages.push((at, Faces::TOPIC, FACE));
        if away.contains(&at) {
            messages.push((at + 30, Faces::TOPIC, NOBODY));
        }
    }
    messages
}

#[test]
fn present_for_a_while_through_short_dropouts_once_a_sitting_and_only_while_resting() {
    let dir = tempfile::tempdir().unwrap();
    let skill = skill_with(dir.path(), "stretch-break", STRETCH);
    let resting = (0, "voice/state", r#"{"state":"resting"}"#);
    let mut messages = vec![resting];
    messages.extend(sitting(0, 3300, &[600, 1800]));
    // They leave for five minutes and sit 55 minutes more: within the
    // cooldown, nothing.
    messages.push((3400, Faces::TOPIC, NOBODY));
    messages.extend(sitting(3700, 7000, &[]));
    messages.sort_by_key(|message| message.0);
    let runs = runs_at(&skill, &messages);
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].0, 3000);
    assert_eq!(
        runs[0].1,
        [
            Call::Gaze {
                face: Some((0.0, -0.25, 0.25))
            },
            Call::Gesture("stretching".into()),
            Call::Light {
                colour: "warm".into(),
                level: Some(2),
                lasting: Some("4s".into())
            }
        ]
    );
    // Talking at the fiftieth minute: its chance this sitting is gone, and
    // the next sitting, with no cooldown begun, has its own.
    let mut talking = messages.clone();
    talking.push((2950, "voice/state", r#"{"state":"speaking"}"#));
    talking.push((3100, "voice/state", r#"{"state":"resting"}"#));
    talking.sort_by_key(|message| message.0);
    let runs = runs_at(&skill, &talking);
    assert_eq!(runs.iter().map(|run| run.0).collect::<Vec<_>>(), [6700]);
    // A dropout longer than the gap begins the sitting again: 50 minutes
    // from 1,320 s is past the end.
    let mut broken: Vec<_> = sitting(0, 3300, &[])
        .into_iter()
        .filter(|message| !(1000..1300).contains(&message.0))
        .collect();
    broken.push(resting);
    broken.push((1001, Faces::TOPIC, NOBODY));
    broken.sort_by_key(|message| message.0);
    assert!(runs_at(&skill, &broken).is_empty());
}

#[test]
fn a_field_above_a_line_for_a_while_once_a_spell_and_only_with_someone_there() {
    let dir = tempfile::tempdir().unwrap();
    let skill = skill_with(
        dir.path(),
        "stuffy-air",
        r#"
needs = ["air", "eyes", "light"]
[trigger]
topic = "air/state"
field = "co2_ppm"
above = 1200
for = "10m"
rearm = "ends"
cooldown = "1h"
[[while]]
topic = "humans/faces/tracked"
present = true
[ask]
policy = "nudge"
[[do]]
light = { dim = 1, until = "ends" }
"#,
    );
    let high = r#"{"co2_ppm":1300,"feel":"stuffy"}"#;
    let low = r#"{"co2_ppm":800,"feel":"fresh"}"#;
    let mut messages = Vec::new();
    for at in (0..=1500).step_by(60) {
        messages.push((at, Faces::TOPIC, FACE));
        messages.push((at, "air/state", high));
    }
    let runs = runs_at(&skill, &messages);
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0].0, 600);
    assert_eq!(
        runs[0].1,
        [Call::Dim {
            steps: 1,
            until_ends: true
        }]
    );
    // Nobody at the desk: nothing.
    let alone: Vec<_> = messages
        .iter()
        .filter(|message| message.1 != Faces::TOPIC)
        .copied()
        .collect();
    assert!(runs_at(&skill, &alone).is_empty());
    // A rise of five minutes: nothing.
    let mut short = Vec::new();
    for at in (0..=1500).step_by(60) {
        short.push((at, Faces::TOPIC, FACE));
        short.push((at, "air/state", if at < 300 { high } else { low }));
    }
    assert!(runs_at(&skill, &short).is_empty());
}

#[test]
fn an_expired_face_is_not_looked_at_and_the_other_steps_still_run() {
    let dir = tempfile::tempdir().unwrap();
    let skill = skill_with(
        dir.path(),
        "stuffy-look",
        r#"
[trigger]
topic = "air/state"
field = "co2_ppm"
above = 1200
for = "10m"
rearm = "ends"
[ask]
policy = "nudge"
[[do]]
body = { look_at = "face" }
light = { dim = 1 }
"#,
    );
    let high = r#"{"co2_ppm":1300,"feel":"stuffy"}"#;
    let mut messages: Vec<_> = (0..=600)
        .step_by(60)
        .map(|at| (at, "air/state", high))
        .collect();
    // A face seen once, at the start: its two seconds are long over at 600 s.
    messages.insert(0, (0, Faces::TOPIC, FACE));
    let dim = Call::Dim {
        steps: 1,
        until_ends: false,
    };
    assert_eq!(runs_at(&skill, &messages), [(600, vec![dim.clone()])]);
    // The same face seen at 600 s, just before the air: looked at.
    messages.insert(messages.len() - 1, (600, Faces::TOPIC, FACE));
    let gaze = Call::Gaze {
        face: Some((0.0, -0.25, 0.25)),
    };
    assert_eq!(runs_at(&skill, &messages), [(600, vec![gaze, dim])]);
}

#[test]
fn a_look_at_a_face_subscribes_to_the_faces_whatever_the_trigger() {
    let dir = tempfile::tempdir().unwrap();
    let skill = |name: &str, step: &str| {
        let toml = format!(
            "[trigger]\ntopic = \"air/state\"\nfield = \"co2_ppm\"\nabove = 1200\n\
             [ask]\npolicy = \"nudge\"\n[[do]]\n{step}\n"
        );
        load(&skill_with(dir.path(), name, &toml)).unwrap()
    };
    let looks = Engine::new(vec![skill("looks", "body = { look_at = \"face\" }")], true);
    assert_eq!(looks.topics(), ["air/state", Faces::TOPIC]);
    let dims = Engine::new(vec![skill("dims", "light = { dim = 1 }")], true);
    assert_eq!(dims.topics(), ["air/state"]);
}

/// Air above the line, with a condition that someone has been at the desk
/// for 30 s.
fn sat_for_thirty(dir: &Path, name: &str, condition: &str) -> PathBuf {
    skill_with(
        dir,
        name,
        &format!(
            "[trigger]\ntopic = \"air/state\"\nfield = \"co2_ppm\"\nabove = 1200\n\
             [[while]]\n{condition}\nfor = \"30s\"\n\
             [ask]\npolicy = \"nudge\"\n[[do]]\nlight = {{ dim = 1 }}\n"
        ),
    )
}

#[test]
fn a_timed_condition_begins_again_after_its_value_expired() {
    let dir = tempfile::tempdir().unwrap();
    let skill = sat_for_thirty(
        dir.path(),
        "sat",
        "topic = \"humans/faces/tracked\"\npresent = true",
    );
    let high = r#"{"co2_ppm":1300,"feel":"stuffy"}"#;
    // A face at 0 s, its list lasting 2 s, then nothing until 31 s: the 29 s
    // between are not 30 s of someone there.
    let gap = [
        (0, Faces::TOPIC, FACE),
        (31, Faces::TOPIC, FACE),
        (31, "air/state", high),
    ];
    assert!(runs_at(&skill, &gap).is_empty());
    // A face every second for 31 s is.
    let mut held: Vec<_> = (0..=31).map(|at| (at, Faces::TOPIC, FACE)).collect();
    held.push((31, "air/state", high));
    assert_eq!(runs_at(&skill, &held).len(), 1);
}

#[test]
fn a_condition_changed_by_a_reload_begins_again() {
    let dir = tempfile::tempdir().unwrap();
    let speaking = sat_for_thirty(
        dir.path(),
        "talk",
        "topic = \"voice/state\"\nfield = \"state\"\nequals = \"speaking\"",
    );
    let mut engine = Engine::new(vec![load(&speaking).unwrap()], true);
    let state = |word: &str| format!(r#"{{"state":"{word}"}}"#);
    let high = br#"{"co2_ppm":1300,"feel":"stuffy"}"#;
    // Speaking from 0 s: the same condition, reloaded, keeps its time.
    engine.observe("voice/state", 0, state("speaking").as_bytes(), None);
    engine.replace(vec![load(&speaking).unwrap()]);
    assert_eq!(engine.observe("air/state", 31_000_000, high, None).len(), 1);
    // The same slot edited to resting: its first resting sample at 40 s
    // does not inherit 0 s.
    let mut engine = Engine::new(vec![load(&speaking).unwrap()], true);
    engine.observe("voice/state", 0, state("speaking").as_bytes(), None);
    let resting = sat_for_thirty(
        &dir.path().join("edited"),
        "talk",
        "topic = \"voice/state\"\nfield = \"state\"\nequals = \"resting\"",
    );
    engine.replace(vec![load(&resting).unwrap()]);
    engine.observe("voice/state", 40_000_000, state("resting").as_bytes(), None);
    assert!(
        engine
            .observe("air/state", 41_000_000, high, None)
            .is_empty()
    );
    assert_eq!(engine.observe("air/state", 71_000_000, high, None).len(), 1);
}

#[test]
fn a_folder_beginning_with_a_dot_is_passed_over() {
    let dir = tempfile::tempdir().unwrap();
    skill_with(dir.path(), ".draft", "not toml at all");
    welcome_back_with(dir.path(), "needs", "needs");
    let (skills, errors) = load_all(dir.path());
    assert_eq!(skills.len(), 1);
    assert!(errors.is_empty(), "{errors:?}");
}

/// A skill folder copied from a Mac (`tar` without `COPYFILE_DISABLE=1`)
/// has a binary `._NAME` beside each file, and `lamp-skills test` failed on
/// them on lamp-4ace (2026-10-16): they are passed over, and the skill loads
/// and passes as before.
#[test]
fn a_macs_appledouble_files_are_passed_over() {
    let dir = tempfile::tempdir().unwrap();
    let skill = welcome_back_with(dir.path(), "cooldown = \"20m\"", "cooldown = \"20m\"");
    let clean = run_tests(&skill).unwrap();
    let binary = [0u8, 5, 22, 7, 0, 2, 0, 0, 0xff, 0xfe, 0x80];
    let mut beside = Vec::new();
    let mut folders = vec![skill.clone()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path.clone());
            }
            let name = format!("._{}", entry.file_name().to_string_lossy());
            beside.push(folder.join(name));
        }
    }
    for path in &beside {
        fs::write(path, binary).unwrap();
    }
    assert!(
        beside
            .iter()
            .any(|path| path.to_string_lossy().ends_with(".jsonl"))
    );
    load(&skill).unwrap();
    assert_eq!(run_tests(&skill).unwrap(), clean);
}

/// Desk tutor's shape (#941): started by its tool alone, it lights the
/// page and has the voice take its full look.
const TUTOR: &str = r#"
needs = ["eyes", "voice", "light"]
tool = "check_my_work"
[[do]]
light = { colour = "white", for = "1s" }
look = true
say = "Tutor: a hint or a question, never the answer."
"#;

#[test]
fn a_reading_pose_is_held_on_the_mood_socket_for_its_for_and_then_let_go() {
    let reading = AT_ONCE.replace(
        "body = { look_at = \"face\" }",
        "body = { mood = \"reading\", for = \"3s\" }",
    );
    let seen = the_program_with(
        &reading,
        true,
        &[r#"{"event":"begun","at_us":5}"#, r#"{"outcome":"spoken"}"#],
        3,
        1,
        4,
    );
    // Told at once when the say begins, then every second while it lasts:
    // three in all for 3 s, and then nothing, so the body comes back.
    assert_eq!(
        seen.moods,
        vec![r#"{"level":0,"mood":"reading"}"#; 3],
        "{:?}",
        seen.events
    );
    let dir = tempfile::tempdir().unwrap();
    for (step, why) in [
        ("body = { mood = \"dance\", for = \"3s\" }", "not one of"),
        ("body = { mood = \"reading\" }", "goes with its for"),
        ("body = { mood = \"reading\", for = \"3m\" }", "at most 2m"),
    ] {
        let toml = AT_ONCE.replace("body = { look_at = \"face\" }", step);
        let error = load(&skill_with(&dir.path().join("bad"), "hello", &toml)).unwrap_err();
        assert!(error.contains(why), "{step}: {error}");
    }
}

#[test]
fn a_tool_needs_no_trigger_and_a_look_needs_a_tool() {
    let dir = tempfile::tempdir().unwrap();
    let skill = load(&skill_with(dir.path(), "tutor", TUTOR)).unwrap();
    assert_eq!(skill.tool.as_deref(), Some("check_my_work"));
    assert!(skill.triggers.is_empty());
    for (toml, why) in [
        (TUTOR.replace("check_my_work", "Check My Work"), "tool \""),
        (TUTOR.replace("tool = \"check_my_work\"", ""), "a trigger or a tool"),
        (
            TUTOR.replace(
                "tool = \"check_my_work\"",
                "tool = \"check_my_work\"\n[trigger]\ntopic = \"humans/faces/tracked\"\n[ask]\npolicy = \"nudge\"",
            ),
            "started by its tool alone",
        ),
    ] {
        let error = load(&skill_with(&dir.path().join("bad"), "tutor", &toml)).unwrap_err();
        assert!(error.contains(why), "{toml}: {error}");
    }
}

#[test]
fn a_tools_call_runs_only_while_its_conditions_hold_on_values_that_last() {
    let dir = tempfile::tempdir().unwrap();
    let toml = TUTOR.replace(
        "[[do]]",
        "[[while]]\ntopic = \"voice/state\"\nfield = \"state\"\nequals = \"resting\"\nfor = \"30s\"\n[[do]]",
    );
    let mut engine = Engine::new(
        vec![load(&skill_with(dir.path(), "tutor", &toml)).unwrap()],
        false,
    );
    let s = 1_000_000;
    let resting = br#"{"state":"resting"}"#;
    // Never seen.
    assert!(engine.tool("check_my_work", 0).is_err());
    engine.observe("voice/state", 0, resting, None);
    // Not yet for its 30 s, then it has.
    assert!(engine.tool("check_my_work", 10 * s).is_err());
    assert!(engine.tool("check_my_work", 31 * s).is_ok());
    // False.
    engine.observe("voice/state", 40 * s, br#"{"state":"speaking"}"#, None);
    assert!(engine.tool("check_my_work", 41 * s).is_err());
    // A value lasting 60 s: held 30 s within it, then expired.
    engine.observe("voice/state", 100 * s, resting, Some(60_000));
    assert!(engine.tool("check_my_work", 131 * s).is_ok());
    assert!(engine.tool("check_my_work", 161 * s).is_err());
}

#[test]
fn an_expired_queued_call_is_never_committed() {
    // The caller gave up while the call waited in the queue: the loop
    // cannot commit it after, and a committed call cannot be given up.
    let late = Admission::default();
    assert!(late.cancel());
    assert!(!late.commit());
    let taken = Admission::default();
    assert!(taken.commit());
    assert!(!taken.cancel());
}

#[test]
fn a_declared_tool_is_in_the_tools_file_and_a_call_runs_the_skills_steps() {
    // Short: a socket's whole path has to fit in 103 bytes.
    let dir = tempfile::Builder::new()
        .prefix("st")
        .tempdir_in("/tmp")
        .unwrap();
    let root = dir.path();
    fs::create_dir(root.join("skills")).unwrap();
    let bus = Bus::new(root.join("bus")).unwrap();
    let mut events = bus.subscribe(SkillEvent::TOPIC, "test").unwrap();
    let mut ring = bus
        .publisher_declared("light/state", &Light::declaration(), "ring")
        .unwrap();
    let actions = UnixListener::bind(root.join("actions.sock")).unwrap();
    let (called, lights) = mpsc::channel();
    // The actions part, saying the colour it shows as the real one does.
    let shown = bus.clone();
    std::thread::spawn(move || {
        let mut shown = shown
            .publisher_declared("light/state", &Light::declaration(), "ring")
            .unwrap();
        for stream in actions.incoming().flatten() {
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let call: serde_json::Value = serde_json::from_str(&line).unwrap();
            if let Some(colour) = call["args"]["colour"].as_str() {
                shown
                    .publish_json(1, &Light::new(true, colour, 3).unwrap())
                    .unwrap();
            }
            (&stream)
                .write_all(b"{\"ok\":true,\"result\":{}}\n")
                .unwrap();
            let _ = called.send(line);
        }
    });
    let tools = root.join("tools");
    let _running = Running(
        Command::new(env!("CARGO_BIN_EXE_lamp-skills"))
            .args([
                "--dir".as_ref(),
                root.join("skills").as_os_str(),
                "--bus-root".as_ref(),
                root.join("bus").as_os_str(),
                "--actions-socket".as_ref(),
                root.join("actions.sock").as_os_str(),
                "--tools-dir".as_ref(),
                tools.as_os_str(),
            ])
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !tools.join("call.sock").exists() {
        assert!(Instant::now() < deadline, "no call.sock");
        std::thread::sleep(Duration::from_millis(50));
    }
    // No skill with a tool: no tools file.
    std::thread::sleep(Duration::from_millis(300));
    assert!(!tools.join("tools.json").exists());
    skill_with(&root.join("skills"), "tutor", TUTOR);
    let file = loop {
        if let Ok(text) = fs::read_to_string(tools.join("tools.json")) {
            break serde_json::from_str::<serde_json::Value>(&text).unwrap();
        }
        assert!(Instant::now() < deadline, "no tools file");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(
        file,
        serde_json::json!({"tools":[{"name":"check_my_work","description":"A test."}]})
    );
    // The voice's call, as it asks the actions part.
    let call = |name: &str| {
        let stream = std::os::unix::net::UnixStream::connect(tools.join("call.sock")).unwrap();
        writeln!(&stream, r#"{{"op":"call","name":"{name}","args":{{}}}}"#).unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        serde_json::from_str::<serde_json::Value>(&line).unwrap()
    };
    // The light as shown just before the call, fresh.
    ring.publish_json(1, &Light::new(true, "blue", 3).unwrap())
        .unwrap();
    let asked = Instant::now();
    let answer = call("check_my_work");
    assert!(
        asked.elapsed() < Duration::from_millis(500),
        "{:?}",
        asked.elapsed()
    );
    assert_eq!(answer["ok"], true, "{answer}");
    let next = answer["result"]["next"].as_str().unwrap();
    assert!(
        next.starts_with("Say only \"Let me take a look.\""),
        "{next}"
    );
    assert!(next.ends_with("never the answer."), "{next}");
    // Again while the page is still lit, as a student asks once more after
    // fixing a line (#941): told to look again, and the light left as it is.
    let again = call("check_my_work");
    assert_eq!(again["ok"], true, "{again}");
    assert_eq!(again["result"]["next"], answer["result"]["next"]);
    assert_eq!(call("nobody_has_this")["ok"], false);
    let mut seen = Vec::new();
    while seen
        .iter()
        .filter(|event: &&SkillEvent| event.stage == SkillStage::Done)
        .count()
        < 2
        && Instant::now() < deadline
    {
        if let Some(message) = events.recv(Duration::from_millis(100)).unwrap() {
            seen.push(message.parse().unwrap());
        }
    }
    assert_eq!(
        stages(&seen),
        [
            SkillStage::Triggered,
            SkillStage::Triggered,
            SkillStage::Done,
            SkillStage::Done
        ]
    );
    assert!(seen[2].why.contains("still under way"), "{seen:?}");
    let lights: Vec<String> = lights.try_iter().collect();
    // The page lit once, and after its second given back: nothing made for
    // the second call.
    assert_eq!(lights.len(), 2, "{lights:?}");
    assert!(
        lights[0].contains("\"light_colour\"") && lights[0].contains("white"),
        "{lights:?}"
    );
    assert!(lights[1].contains("blue"), "{lights:?}");
}

/// A skill whose tool only makes things: nothing to tell the model, so a
/// call while its run is under way is still refused.
#[test]
fn a_repeat_call_with_only_made_steps_is_refused_while_its_run_is_under_way() {
    // Short: a socket's whole path has to fit in 103 bytes.
    let dir = tempfile::Builder::new()
        .prefix("st")
        .tempdir_in("/tmp")
        .unwrap();
    let root = dir.path();
    fs::create_dir(root.join("skills")).unwrap();
    let bus = Bus::new(root.join("bus")).unwrap();
    let mut ring = bus
        .publisher_declared("light/state", &Light::declaration(), "ring")
        .unwrap();
    let actions = UnixListener::bind(root.join("actions.sock")).unwrap();
    std::thread::spawn(move || {
        for stream in actions.incoming().flatten() {
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            (&stream)
                .write_all(b"{\"ok\":true,\"result\":{}}\n")
                .unwrap();
        }
    });
    let tools = root.join("tools");
    let _running = Running(
        Command::new(env!("CARGO_BIN_EXE_lamp-skills"))
            .args([
                "--dir".as_ref(),
                root.join("skills").as_os_str(),
                "--bus-root".as_ref(),
                root.join("bus").as_os_str(),
                "--actions-socket".as_ref(),
                root.join("actions.sock").as_os_str(),
                "--tools-dir".as_ref(),
                tools.as_os_str(),
            ])
            .spawn()
            .unwrap(),
    );
    skill_with(
        &root.join("skills"),
        "glow",
        &TUTOR
            .replace("check_my_work", "glow")
            .replace("look = true\n", "")
            .replace(
                "say = \"Tutor: a hint or a question, never the answer.\"\n",
                "",
            )
            .replace("for = \"1s\"", "for = \"3s\""),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !tools.join("tools.json").exists() {
        assert!(Instant::now() < deadline, "no tools file");
        std::thread::sleep(Duration::from_millis(50));
    }
    let call = || {
        let stream = std::os::unix::net::UnixStream::connect(tools.join("call.sock")).unwrap();
        writeln!(&stream, r#"{{"op":"call","name":"glow","args":{{}}}}"#).unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        serde_json::from_str::<serde_json::Value>(&line).unwrap()
    };
    ring.publish_json(1, &Light::new(true, "blue", 3).unwrap())
        .unwrap();
    let first = call();
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(first["result"]["next"], "Done.");
    let again = call();
    assert_eq!(again["ok"], false, "{again}");
    assert_eq!(again["error"], "I'm still doing that.");
}
