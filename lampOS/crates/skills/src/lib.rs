//! Physical skills (`docs/skills.md`). A skill is a folder; this crate loads
//! one, decides from what the bus carries when it runs, and says which calls
//! its steps make. Sending them is the program's (`main.rs`).
//!
//! Time here is microseconds of one monotonic clock, given by the caller:
//! the program's own clock when a message arrives, a message's `at_us` when
//! a recording is replayed.
use lamp_bus::Header;
use lamp_msgs::{Faces, Light, SkillEvent, SkillStage};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

/// What the body plays by name on its mood socket (`crates/body/src/clips/data.rs`;
/// `idle` is its own).
pub const GESTURES: [&str; 6] = ["nod", "headshake", "greeting", "laugh", "shy", "stretching"];
/// How long a face message counts when its header names no lifespan: the
/// eyes' two seconds. A value on another topic counts until it is replaced.
const FACE_LIFESPAN_MS: u64 = 2_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillFile {
    #[serde(default)]
    needs: Vec<String>,
    /// One `[trigger]` table, or `[[trigger]]` tables.
    trigger: toml::Value,
    #[serde(default, rename = "while")]
    conditions: Vec<TestFile>,
    ask: AskFile,
    #[serde(rename = "do")]
    steps: Vec<StepFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TriggerFile {
    topic: String,
    when: Option<String>,
    field: Option<String>,
    equals: Option<toml::Value>,
    above: Option<f64>,
    below: Option<f64>,
    absent_for: Option<String>,
    #[serde(rename = "for")]
    lasting: Option<String>,
    gap: Option<String>,
    rearm: Option<String>,
    cooldown: Option<String>,
}

/// A `[[while]]` condition.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TestFile {
    topic: String,
    field: Option<String>,
    equals: Option<toml::Value>,
    above: Option<f64>,
    below: Option<f64>,
    present: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AskFile {
    policy: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StepFile {
    body: Option<BodyFile>,
    light: Option<LightStep>,
    say: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BodyFile {
    look_at: Option<String>,
    gesture: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LightStep {
    /// A file in `lights/`.
    Named(String),
    Inline(LightFile),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LightFile {
    colour: Option<String>,
    level: Option<u8>,
    #[serde(rename = "for")]
    lasting: Option<String>,
    dim: Option<u8>,
    until: Option<String>,
}

/// A skill as loaded: everything in it checked.
#[derive(Clone, Debug)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub needs: Vec<String>,
    pub triggers: Vec<Trigger>,
    conditions: Vec<Condition>,
    pub policy: String,
    steps: Vec<Call>,
}

/// What a message must show for a trigger or a condition to hold.
#[derive(Clone, Debug)]
enum Test {
    /// A face in the list of `humans/faces/tracked`.
    Faces,
    Equals(String, Value),
    Above(String, f64),
    Below(String, f64),
}

impl Test {
    fn holds(&self, value: &Value) -> bool {
        match self {
            Self::Faces => value["faces"]
                .as_array()
                .is_some_and(|faces| !faces.is_empty()),
            Self::Equals(field, wanted) => match (value[field].as_f64(), wanted.as_f64()) {
                (Some(got), Some(wanted)) => got == wanted,
                _ => &value[field] == wanted,
            },
            Self::Above(field, line) => value[field].as_f64().is_some_and(|got| got > *line),
            Self::Below(field, line) => value[field].as_f64().is_some_and(|got| got < *line),
        }
    }
}

#[derive(Clone, Debug)]
struct Condition {
    topic: String,
    test: Test,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// Holds where it did not just before: once each time.
    Appears,
    /// Holds, and has for `for`.
    Present,
}

#[derive(Clone, Debug)]
pub struct Trigger {
    pub topic: String,
    test: Test,
    pub when: When,
    pub absent_for_us: u64,
    pub for_us: u64,
    pub gap_us: u64,
    /// Once each time it holds; again only after it stopped.
    pub rearm: bool,
    pub cooldown_us: u64,
}

/// One call a step makes, as the program sends it and a test expects it.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    /// To the body's gaze socket, as the eyes send a face: its centre from
    /// the middle of the picture (-1 to 1) and its height as a share of it.
    /// Before a run it names no face.
    Gaze { face: Option<(f32, f32, f32)> },
    /// A gesture by name, to the body's mood socket.
    Gesture(String),
    /// A colour, to the actions part's `light_colour`; a level and a while
    /// that it cannot carry out yet, kept as written.
    Light {
        colour: String,
        level: Option<u8>,
        lasting: Option<String>,
    },
    /// Steps dimmer, to the actions part's `light_dimmer`; "until it ends",
    /// which it cannot carry out yet.
    Dim { steps: u8, until_ends: bool },
    /// A note for the voice to say in its own words.
    Say(String),
}

impl Call {
    /// The call's name and its arguments, as a test names them.
    pub fn named(&self) -> (&'static str, Value) {
        match self {
            Self::Gaze { face } => {
                let mut args = json!({ "look_at": "face" });
                if let Some((x, y, size)) = face {
                    args["x"] = json!(x);
                    args["y"] = json!(y);
                    args["size"] = json!(size);
                }
                ("body.gaze", args)
            }
            Self::Gesture(name) => ("body.gesture", json!({ "gesture": name })),
            Self::Light {
                colour,
                level,
                lasting,
            } => {
                let mut args = json!({ "colour": colour });
                if let Some(level) = level {
                    args["level"] = json!(level);
                }
                if let Some(lasting) = lasting {
                    args["for"] = json!(lasting);
                }
                ("light", args)
            }
            Self::Dim { steps, until_ends } => {
                let mut args = json!({ "dim": steps });
                if *until_ends {
                    args["until"] = json!("ends");
                }
                ("light", args)
            }
            Self::Say(note) => ("say", json!({ "note": note })),
        }
    }
}

/// `"5m"` and the like: a whole number of `ms`, `s`, `m` or `h`.
pub fn duration_us(text: &str) -> Result<u64, String> {
    let digits = text.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let per_unit: u64 = match &text[digits.len()..] {
        "ms" => 1_000,
        "s" => 1_000_000,
        "m" => 60_000_000,
        "h" => 3_600_000_000,
        _ => {
            return Err(format!(
                "{text:?} is not a duration: a whole number of ms, s, m or h"
            ));
        }
    };
    digits
        .parse::<u64>()
        .ok()
        .and_then(|count| count.checked_mul(per_unit))
        .ok_or_else(|| format!("{text:?} is not a duration: a whole number of ms, s, m or h"))
}

fn plain_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
}

/// `name` and `description` from `SKILL.md`'s front matter (Agent Skills).
fn front_matter(text: &str) -> Result<(String, String), String> {
    let body = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---"))
        .ok_or("SKILL.md begins with front matter between two --- lines")?
        .0;
    let field = |key: &str| {
        body.lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .ok_or(format!("SKILL.md's front matter has no {key}"))
    };
    Ok((field("name")?, field("description")?))
}

/// A test from `field` and one of `equals`, `above`, `below`; or, on
/// `humans/faces/tracked` with none of them, a face in view.
fn test(
    topic: &str,
    field: Option<String>,
    equals: Option<toml::Value>,
    above: Option<f64>,
    below: Option<f64>,
) -> Result<Test, String> {
    let equals = equals
        .map(|value| serde_json::to_value(value).map_err(|error| error.to_string()))
        .transpose()?;
    match (field, equals, above, below) {
        (None, None, None, None) if topic == Faces::TOPIC => Ok(Test::Faces),
        (Some(field), Some(value), None, None) => Ok(Test::Equals(field, value)),
        (Some(field), None, Some(line), None) => Ok(Test::Above(field, line)),
        (Some(field), None, None, Some(line)) => Ok(Test::Below(field, line)),
        _ => Err(format!(
            "on {topic}: a field and one of equals, above or below \
             (only {} can do without, for a face in view)",
            Faces::TOPIC
        )),
    }
}

fn trigger(file: TriggerFile) -> Result<Trigger, String> {
    let optional = |text: &Option<String>| text.as_deref().map_or(Ok(0), duration_us);
    let when = match file.when.as_deref() {
        Some("appears") => When::Appears,
        None | Some("present") => When::Present,
        Some(other) => {
            return Err(format!(
                "when = {other:?}: only \"appears\" and \"present\" are known"
            ));
        }
    };
    let rearm = match file.rearm.as_deref() {
        None => false,
        Some("leaves" | "ends") => true,
        Some(other) => return Err(format!("rearm = {other:?}: \"leaves\" or \"ends\"")),
    };
    Ok(Trigger {
        test: test(&file.topic, file.field, file.equals, file.above, file.below)?,
        topic: file.topic,
        when,
        absent_for_us: optional(&file.absent_for)?,
        for_us: optional(&file.lasting)?,
        gap_us: optional(&file.gap)?,
        rearm,
        cooldown_us: optional(&file.cooldown)?,
    })
}

/// One skill folder, or why it cannot be loaded.
pub fn load(dir: &Path) -> Result<Skill, String> {
    let read =
        |file: &str| fs::read_to_string(dir.join(file)).map_err(|error| format!("{file}: {error}"));
    let (name, description) = front_matter(&read("SKILL.md")?)?;
    if dir.file_name().and_then(|folder| folder.to_str()) != Some(name.as_str()) {
        return Err(format!(
            "SKILL.md names {name:?}, which is not its folder's name"
        ));
    }
    if !plain_name(&name) || name.contains('_') {
        return Err(format!(
            "{name:?}: a name is 1 to 64 lowercase letters, digits and hyphens"
        ));
    }
    if dir.join("run.py").exists() {
        return Err("run.py is not run yet: a skill with one is not loaded".into());
    }
    let file: SkillFile =
        toml::from_str(&read("skill.toml")?).map_err(|error| format!("skill.toml: {error}"))?;
    let triggers = match file.trigger {
        toml::Value::Array(many) => many,
        one => vec![one],
    };
    let triggers = triggers
        .into_iter()
        .map(|value| {
            trigger(
                value
                    .try_into()
                    .map_err(|error| format!("skill.toml: trigger: {error}"))?,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    if triggers.is_empty() || file.steps.is_empty() {
        return Err("skill.toml needs a trigger and at least one [[do]]".into());
    }
    if file.ask.policy.is_empty() {
        return Err("[ask] names the Policy's question".into());
    }
    let mut conditions = Vec::new();
    for condition in file.conditions {
        let test = match condition.present {
            None => test(
                &condition.topic,
                condition.field,
                condition.equals,
                condition.above,
                condition.below,
            )?,
            Some(true) if condition.topic == Faces::TOPIC && condition.field.is_none() => {
                Test::Faces
            }
            Some(_) => {
                return Err(format!(
                    "present = true is known only on {}, alone",
                    Faces::TOPIC
                ));
            }
        };
        conditions.push(Condition {
            topic: condition.topic,
            test,
        });
    }
    let mut steps = Vec::new();
    for step in file.steps {
        if step.body.is_none() && step.light.is_none() && step.say.is_none() {
            return Err("a [[do]] step names body, light or say".into());
        }
        if let Some(body) = step.body {
            match body.look_at.as_deref() {
                None => {}
                Some("face") => steps.push(Call::Gaze { face: None }),
                Some(other) => return Err(format!("look_at = {other:?}: only \"face\"")),
            }
            if let Some(gesture) = body.gesture {
                if !GESTURES.contains(&gesture.as_str()) {
                    return Err(format!("gesture {gesture:?} is not one of {GESTURES:?}"));
                }
                steps.push(Call::Gesture(gesture));
            }
        }
        if let Some(light) = step.light {
            let (file, place) = match light {
                LightStep::Named(light) => {
                    if !plain_name(&light) {
                        return Err(format!("light {light:?} is not a file name in lights/"));
                    }
                    let place = format!("lights/{light}.toml");
                    let file: LightFile = toml::from_str(&read(&place)?)
                        .map_err(|error| format!("{place}: {error}"))?;
                    (file, place)
                }
                LightStep::Inline(file) => (file, "a light step".into()),
            };
            steps.push(light_call(file).map_err(|why| format!("{place}: {why}"))?);
        }
        if let Some(say) = step.say {
            if say.trim().is_empty() {
                return Err("say is a note in words".into());
            }
            steps.push(Call::Say(say));
        }
    }
    Ok(Skill {
        name,
        description,
        needs: file.needs,
        triggers,
        conditions,
        policy: file.ask.policy,
        steps,
    })
}

fn light_call(file: LightFile) -> Result<Call, String> {
    let until_ends = match file.until.as_deref() {
        None => false,
        Some("ends") => true,
        Some(other) => return Err(format!("until = {other:?}: only \"ends\"")),
    };
    if let Some(lasting) = &file.lasting {
        duration_us(lasting)?;
    }
    match (file.colour, file.dim) {
        (Some(colour), None) => {
            if !Light::COLOURS.contains(&colour.as_str()) {
                return Err(format!(
                    "colour {colour:?} is not one of {:?}",
                    Light::COLOURS
                ));
            }
            if file.level.is_some_and(|level| !(1..=5).contains(&level)) {
                return Err("level is 1 to 5".into());
            }
            if until_ends {
                return Err("until goes with dim".into());
            }
            Ok(Call::Light {
                colour,
                level: file.level,
                lasting: file.lasting,
            })
        }
        (None, Some(steps)) if (1..=4).contains(&steps) => {
            if file.level.is_some() || file.lasting.is_some() {
                return Err("dim takes no level and no for".into());
            }
            Ok(Call::Dim { steps, until_ends })
        }
        _ => Err("a light is a colour (with a level and a for) or dim = 1 to 4".into()),
    }
}

/// Every skill folder under `root` but those whose name begins with a dot,
/// and why each other one did not load.
pub fn load_all(root: &Path) -> (Vec<Skill>, Vec<String>) {
    let mut folders: Vec<PathBuf> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    folders.sort();
    let (mut skills, mut errors) = (Vec::new(), Vec::new());
    for folder in folders {
        match load(&folder) {
            Ok(skill) => skills.push(skill),
            Err(why) => errors.push(format!("{}: {why}", folder.display())),
        }
    }
    (skills, errors)
}

#[derive(Default)]
struct Watch {
    started_us: Option<u64>,
    last_held_us: Option<u64>,
    /// Since when it has held, through dropouts shorter than the gap.
    since_us: Option<u64>,
    /// How long it had not held before that.
    away_us: u64,
    /// This time it holds has had its chance.
    spent: bool,
    ran_us: Option<u64>,
}

/// A trigger that held: what was said of it, and the calls to make.
#[derive(Debug)]
pub struct Run {
    pub skill: String,
    pub events: Vec<SkillEvent>,
    pub calls: Vec<Call>,
}

pub struct Engine {
    skills: Vec<Skill>,
    /// By skill name and trigger, kept across reloads, so an edit does not
    /// forget a cooldown.
    watches: HashMap<(String, usize), Watch>,
    /// The newest value of each topic: when it came and how long it lasts.
    latest: HashMap<String, (Value, u64, Option<u64>)>,
    skip_ask: bool,
}

fn event(skill: &str, stage: SkillStage, why: String, allowed: Option<bool>) -> SkillEvent {
    SkillEvent {
        skill: skill.into(),
        stage,
        why,
        allowed,
    }
}

impl Engine {
    /// `skip_ask`: act without asking the Policy, whose gate is not built
    /// yet (`LAMP_SKILLS_ASK=skip`). Without it nothing a skill starts is done.
    pub fn new(skills: Vec<Skill>, skip_ask: bool) -> Self {
        Self {
            skills,
            watches: HashMap::new(),
            latest: HashMap::new(),
            skip_ask,
        }
    }

    pub fn replace(&mut self, skills: Vec<Skill>) {
        self.skills = skills;
    }

    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    /// Every topic a trigger or a condition names, once each.
    pub fn topics(&self) -> Vec<String> {
        let mut topics: Vec<String> = self
            .skills
            .iter()
            .flat_map(|skill| {
                skill
                    .triggers
                    .iter()
                    .map(|trigger| trigger.topic.clone())
                    .chain(skill.conditions.iter().map(|test| test.topic.clone()))
            })
            .collect();
        topics.sort();
        topics.dedup();
        topics
    }

    /// The newest value of `topic`, if it still lasts at `now_us`.
    fn fresh<'a>(
        latest: &'a HashMap<String, (Value, u64, Option<u64>)>,
        topic: &str,
        now_us: u64,
    ) -> Option<&'a Value> {
        latest
            .get(topic)
            .filter(|(_, at_us, lasting_us)| {
                lasting_us.is_none_or(|lasting| now_us < at_us + lasting)
            })
            .map(|(value, _, _)| value)
    }

    fn condition_holds(
        latest: &HashMap<String, (Value, u64, Option<u64>)>,
        condition: &Condition,
        now_us: u64,
    ) -> bool {
        Self::fresh(latest, &condition.topic, now_us)
            .is_some_and(|value| condition.test.holds(value))
    }

    /// A message on `topic`, at `now_us`. A payload that is not JSON is
    /// passed over.
    pub fn observe(
        &mut self,
        topic: &str,
        now_us: u64,
        payload: &[u8],
        expires_ms: Option<u64>,
    ) -> Vec<Run> {
        let Ok(value) = serde_json::from_slice::<Value>(payload) else {
            return Vec::new();
        };
        let lifespan_us = expires_ms
            .or((topic == Faces::TOPIC).then_some(FACE_LIFESPAN_MS))
            .map(|ms| ms * 1_000);
        self.latest
            .insert(topic.to_owned(), (value.clone(), now_us, lifespan_us));
        let mut runs = Vec::new();
        for skill in &self.skills {
            for (index, trigger) in skill.triggers.iter().enumerate() {
                if trigger.topic != topic {
                    continue;
                }
                let watch = self.watches.entry((skill.name.clone(), index)).or_default();
                let started = *watch.started_us.get_or_insert(now_us);
                if !trigger.test.holds(&value) {
                    if watch
                        .last_held_us
                        .is_none_or(|held| now_us - held >= trigger.gap_us)
                    {
                        watch.since_us = None;
                    }
                    continue;
                }
                let tolerated = lifespan_us.unwrap_or(u64::MAX).max(trigger.gap_us);
                if watch.since_us.is_none()
                    || watch
                        .last_held_us
                        .is_some_and(|held| now_us - held >= tolerated)
                {
                    watch.since_us = Some(now_us);
                    watch.away_us = now_us - watch.last_held_us.unwrap_or(started);
                    watch.spent = false;
                }
                watch.last_held_us = Some(now_us);
                let held_us = now_us - watch.since_us.unwrap_or(now_us);
                if held_us < trigger.for_us {
                    continue;
                }
                // Once each time it holds: a chance held back now waits for
                // the next time.
                let once = trigger.when == When::Appears || trigger.rearm;
                if once && watch.spent {
                    continue;
                }
                watch.spent = once;
                if (trigger.when == When::Appears && watch.away_us < trigger.absent_for_us)
                    || watch
                        .ran_us
                        .is_some_and(|ran| now_us - ran < trigger.cooldown_us)
                    || !skill
                        .conditions
                        .iter()
                        .all(|condition| Self::condition_holds(&self.latest, condition, now_us))
                {
                    continue;
                }
                let held = match trigger.when {
                    When::Appears => format!(
                        "it appeared on {topic} after {} s without",
                        watch.away_us / 1_000_000
                    ),
                    When::Present => format!("{topic} has held for {} s", held_us / 1_000_000),
                };
                let mut events = vec![
                    event(&skill.name, SkillStage::Triggered, held, None),
                    event(
                        &skill.name,
                        SkillStage::Asked,
                        format!("policy {}", skill.policy),
                        None,
                    ),
                ];
                if !self.skip_ask {
                    events.push(event(
                        &skill.name,
                        SkillStage::Answered,
                        "the Policy's gate is not built yet, and nothing the lamp starts \
                         itself is done without it (LAMP_SKILLS_ASK=skip acts without asking)"
                            .into(),
                        Some(false),
                    ));
                    runs.push(Run {
                        skill: skill.name.clone(),
                        events,
                        calls: Vec::new(),
                    });
                    continue;
                }
                events.push(event(
                    &skill.name,
                    SkillStage::Answered,
                    "the ask is skipped (LAMP_SKILLS_ASK=skip): the Policy's gate is not built yet"
                        .into(),
                    Some(true),
                ));
                watch.ran_us = Some(now_us);
                // The face to look at: the largest in the newest list of
                // faces that still lasts. With none, the other steps go on.
                let face = Self::fresh(&self.latest, Faces::TOPIC, now_us).and_then(|faces| {
                    serde_json::from_value::<Faces>(faces.clone())
                        .ok()?
                        .faces
                        .into_iter()
                        .max_by(|a, b| (a.ymax - a.ymin).total_cmp(&(b.ymax - b.ymin)))
                        .map(|face| {
                            (
                                face.xmin + face.xmax - 1.0,
                                face.ymin + face.ymax - 1.0,
                                face.ymax - face.ymin,
                            )
                        })
                });
                let calls = skill
                    .steps
                    .iter()
                    .filter_map(|step| match step {
                        Call::Gaze { .. } => face.map(|face| Call::Gaze { face: Some(face) }),
                        other => Some(other.clone()),
                    })
                    .collect();
                runs.push(Run {
                    skill: skill.name.clone(),
                    events,
                    calls,
                });
            }
        }
        runs
    }
}

/// One message of a recording made by `lamp-bus record`.
pub struct Recorded {
    pub header: Header,
    pub payload: Vec<u8>,
}

#[derive(Deserialize)]
struct Line {
    header: Option<Header>,
    payload_b64: Option<String>,
}

/// Every message in the `.jsonl` files of `dir`, in the order of their
/// `at_us`. Declaration lines are passed over.
pub fn read_recordings(dir: &Path) -> Result<Vec<Recorded>, String> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .collect();
    files.sort();
    let mut messages = Vec::new();
    for file in files {
        let text =
            fs::read_to_string(&file).map_err(|error| format!("{}: {error}", file.display()))?;
        for (number, line) in text.lines().enumerate() {
            let wrong = |why: &str| format!("{} line {}: {why}", file.display(), number + 1);
            let line: Line =
                serde_json::from_str(line).map_err(|error| wrong(&error.to_string()))?;
            let (Some(header), Some(payload)) = (line.header, line.payload_b64) else {
                continue;
            };
            let payload = unbase64(&payload).ok_or_else(|| wrong("payload_b64 is not base64"))?;
            messages.push(Recorded { header, payload });
        }
    }
    messages.sort_by_key(|message| message.header.at_us);
    Ok(messages)
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Padded base64, as `lamp-bus record` writes it (its own copy is private to
/// its program).
fn unbase64(text: &str) -> Option<Vec<u8>> {
    let text = text.as_bytes();
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() / 4 * 3);
    for (index, chunk) in text.chunks(4).enumerate() {
        let padding = chunk.iter().rev().take_while(|&&byte| byte == b'=').count();
        if padding > 2 || (padding > 0 && (index + 1) * 4 != text.len()) {
            return None;
        }
        let mut group = 0u32;
        for &byte in &chunk[..4 - padding] {
            group = group << 6 | ALPHABET.iter().position(|&letter| letter == byte)? as u32;
        }
        group <<= 6 * padding;
        bytes.extend_from_slice(&group.to_be_bytes()[1..4 - padding]);
    }
    Some(bytes)
}

/// A call made in a replay: when, from the recording's first message.
#[derive(Debug)]
pub struct Made {
    pub at_us: u64,
    pub call: Call,
}

/// The calls the skill in `dir` makes for one recording, with the ask skipped.
pub fn replay(dir: &Path, recording: &Path) -> Result<Vec<Made>, String> {
    let mut engine = Engine::new(vec![load(dir)?], true);
    let messages = read_recordings(recording)?;
    let first_us = messages.first().map_or(0, |message| message.header.at_us);
    let mut made = Vec::new();
    for message in messages {
        let at_us = message.header.at_us;
        for run in engine.observe(
            &message.header.topic,
            at_us,
            &message.payload,
            message.header.expires_ms,
        ) {
            made.extend(run.calls.into_iter().map(|call| Made {
                at_us: at_us - first_us,
                call,
            }));
        }
    }
    Ok(made)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectFile {
    #[serde(default)]
    none: bool,
    #[serde(default)]
    expect: Vec<ExpectedCall>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedCall {
    at: Option<String>,
    within: Option<String>,
    call: String,
    #[serde(default)]
    args: toml::Table,
}

/// Whether `made` is what one `[[expect]]` says: its name, every argument
/// it names, and, with `at`, when within `within`.
fn matches(expected: &ExpectedCall, made: &Made) -> Result<bool, String> {
    let (name, args) = made.call.named();
    if name != expected.call {
        return Ok(false);
    }
    for (key, wanted) in &expected.args {
        let wanted = serde_json::to_value(wanted).map_err(|error| error.to_string())?;
        let got = &args[key];
        let same = match (got.as_f64(), wanted.as_f64()) {
            (Some(got), Some(wanted)) => got == wanted,
            _ => *got == wanted,
        };
        if !same {
            return Ok(false);
        }
    }
    if let Some(at) = &expected.at {
        let at = duration_us(at)?;
        let within = expected.within.as_deref().map_or(Ok(0), duration_us)?;
        if made.at_us.abs_diff(at) > within {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Each case in the skill's `tests/`: a folder of recordings and
/// `expect.toml`, which lists every call the case may make, in order, or
/// says `none = true`. How many passed and how many have no recording yet,
/// or the first that did not pass.
pub fn run_tests(dir: &Path) -> Result<(usize, usize), String> {
    load(dir)?;
    let mut cases: Vec<PathBuf> = fs::read_dir(dir.join("tests"))
        .map_err(|error| format!("tests/: {error}"))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    cases.sort();
    let (mut passed, mut unrecorded) = (0, 0);
    for case in &cases {
        let wrong = |why: String| format!("{}: {why}", case.display());
        let expect: ExpectFile = toml::from_str(
            &fs::read_to_string(case.join("expect.toml"))
                .map_err(|error| wrong(format!("expect.toml: {error}")))?,
        )
        .map_err(|error| wrong(format!("expect.toml: {error}")))?;
        if expect.none != expect.expect.is_empty() {
            return Err(wrong(
                "expect.toml says none = true or lists [[expect]], one of them".into(),
            ));
        }
        if read_recordings(case)?.is_empty() {
            unrecorded += 1;
            continue;
        }
        let made = replay(dir, case)?;
        let names: Vec<_> = made
            .iter()
            .map(|made| (made.at_us, made.call.named()))
            .collect();
        if made.len() != expect.expect.len() {
            return Err(wrong(format!(
                "made {} calls, {} expected: {names:?}",
                made.len(),
                expect.expect.len()
            )));
        }
        for (index, (expected, made)) in expect.expect.iter().zip(&made).enumerate() {
            if !matches(expected, made).map_err(&wrong)? {
                return Err(wrong(format!(
                    "call {} is not {:?} {:?}: {names:?}",
                    index + 1,
                    expected.call,
                    expected.args
                )));
            }
        }
        passed += 1;
    }
    Ok((passed, unrecorded))
}
