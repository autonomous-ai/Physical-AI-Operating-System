//! Every skill folder in `lampOS/skills/` against its own recordings, the
//! loader's refusals, and the program on a real bus with the body's and the
//! actions part's sockets stood in for.
use lamp_bus::Bus;
use lamp_msgs::{Faces, Light, SkillEvent, SkillStage};
use lamp_skills::{Call, Engine, load, load_all, read_recordings, run_tests};
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
    // Short: a socket's whole path has to fit in 103 bytes.
    let dir = tempfile::Builder::new()
        .prefix("sk")
        .tempdir_in("/tmp")
        .unwrap();
    let root = dir.path();
    fs::create_dir(root.join("skills")).unwrap();
    let bus = Bus::new(root.join("bus")).unwrap();
    let mut events = bus.subscribe(SkillEvent::TOPIC, "test").unwrap();
    let mut shown = bus
        .publisher_declared("light/state", &Light::declaration(), "ring")
        .unwrap();
    shown
        .publish_json(1, &Light::new(true, "blue", 3).unwrap())
        .unwrap();
    let gaze = UnixDatagram::bind(root.join("gaze.sock")).unwrap();
    gaze.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let actions = UnixListener::bind(root.join("actions.sock")).unwrap();
    let (called, lights) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in actions.incoming().flatten() {
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
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
    skill_with(&root.join("skills"), "hello", AT_ONCE);
    let deadline = Instant::now() + Duration::from_secs(10);
    while bus.subscribers(Faces::TOPIC).unwrap().is_empty() {
        assert!(Instant::now() < deadline, "the skill was never loaded");
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut faces = bus
        .publisher_declared(Faces::TOPIC, &Faces::declaration(), "eyes")
        .unwrap();
    faces
        .publish_json(
            1,
            &serde_json::json!({"faces": [{"id": "a1", "xmin": 0.25, "ymin": 0.25,
                "xmax": 0.75, "ymax": 0.5, "confidence": 0.9}]}),
        )
        .unwrap();
    let mut seen = Vec::new();
    let wanted = if skip { 4 } else { 3 };
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
    Seen {
        events: seen,
        looked,
        lights: lights.try_iter().collect(),
        asked: asked.try_recv().ok(),
    }
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

#[test]
fn a_folder_beginning_with_a_dot_is_passed_over() {
    let dir = tempfile::tempdir().unwrap();
    skill_with(dir.path(), ".draft", "not toml at all");
    welcome_back_with(dir.path(), "needs", "needs");
    let (skills, errors) = load_all(dir.path());
    assert_eq!(skills.len(), 1);
    assert!(errors.is_empty(), "{errors:?}");
}
