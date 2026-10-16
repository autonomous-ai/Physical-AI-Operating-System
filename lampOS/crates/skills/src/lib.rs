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
/// The pose a skill may hold on the body's mood socket (#1011): `reading`
/// looks down at the desk and leans forward while it is said again, at
/// least every 1.5 s. The voice's own moods are not a skill's.
pub const MOODS: [&str; 1] = ["reading"];
/// How long a skill may hold a pose.
pub const MOOD_AT_MOST_US: u64 = 120_000_000;
/// How long a face message counts when its header names no lifespan: the
/// eyes' two seconds. A value on another topic counts until it is replaced.
const FACE_LIFESPAN_MS: u64 = 2_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillFile {
    #[serde(default)]
    needs: Vec<String>,
    /// One `[trigger]` table, or `[[trigger]]` tables.
    trigger: Option<toml::Value>,
    #[serde(default, rename = "while")]
    conditions: Vec<TestFile>,
    ask: Option<AskFile>,
    /// A function the model may call, which runs the steps.
    tool: Option<String>,
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
    /// On `humans/faces/tracked`: only a face at least this share of the
    /// picture's height counts.
    near: Option<f32>,
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
    /// It has held this long.
    #[serde(rename = "for")]
    lasting: Option<String>,
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
    look: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BodyFile {
    look_at: Option<String>,
    gesture: Option<String>,
    mood: Option<String>,
    #[serde(rename = "for")]
    lasting: Option<String>,
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
    /// The Policy's question; empty for a skill started by its tool alone.
    pub policy: String,
    /// The function the model may call to run the steps, described to it
    /// with the skill's description.
    pub tool: Option<String>,
    steps: Vec<Call>,
}

/// What a message must show for a trigger or a condition to hold.
#[derive(Clone, Debug, PartialEq)]
enum Test {
    /// A face in the list of `humans/faces/tracked`, at least this share of
    /// the picture's height.
    Faces(f32),
    Equals(String, Value),
    Above(String, f64),
    Below(String, f64),
}

impl Test {
    fn holds(&self, value: &Value) -> bool {
        match self {
            Self::Faces(near) => value["faces"].as_array().is_some_and(|faces| {
                faces.iter().any(|face| {
                    face["ymax"].as_f64().unwrap_or(0.0) - face["ymin"].as_f64().unwrap_or(0.0)
                        >= f64::from(*near)
                })
            }),
            Self::Equals(field, wanted) => match (value[field].as_f64(), wanted.as_f64()) {
                (Some(got), Some(wanted)) => got == wanted,
                _ => &value[field] == wanted,
            },
            Self::Above(field, line) => value[field].as_f64().is_some_and(|got| got > *line),
            Self::Below(field, line) => value[field].as_f64().is_some_and(|got| got < *line),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Condition {
    topic: String,
    test: Test,
    for_us: u64,
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
    /// A pose held on the body's mood socket for `lasting_us`, said again
    /// every second; the body comes back when it is no longer said.
    Mood { mood: String, lasting_us: u64 },
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
    /// The voice's full look (#836), which it takes when the lamp says it
    /// will look. Only in a skill its tool starts.
    Look,
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
            Self::Mood { mood, lasting_us } => (
                "body.mood",
                json!({ "mood": mood, "for_ms": lasting_us / 1_000 }),
            ),
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
            Self::Look => ("look", json!({})),
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
        (None, None, None, None) if topic == Faces::TOPIC => Ok(Test::Faces(0.0)),
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
    let mut test = test(&file.topic, file.field, file.equals, file.above, file.below)?;
    match (file.near, &mut test) {
        (None, _) => {}
        (Some(near), Test::Faces(least)) if (0.0..=1.0).contains(&near) => *least = near,
        _ => {
            return Err(format!(
                "near is a share of the picture from 0 to 1, on {} alone",
                Faces::TOPIC
            ));
        }
    }
    Ok(Trigger {
        test,
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
        None => Vec::new(),
        Some(toml::Value::Array(many)) => many,
        Some(one) => vec![one],
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
    if (triggers.is_empty() && file.tool.is_none()) || file.steps.is_empty() {
        return Err("skill.toml needs a trigger or a tool, and at least one [[do]]".into());
    }
    let policy = file.ask.map(|ask| ask.policy).unwrap_or_default();
    if !triggers.is_empty() && policy.is_empty() {
        return Err("[ask] names the Policy's question".into());
    }
    if let Some(tool) = &file.tool {
        // What the provider link takes (`lamp_gemini::Tool::validate`).
        let mut letters = tool.bytes();
        let first = letters
            .next()
            .is_some_and(|first| first.is_ascii_lowercase());
        if !first
            || tool.len() > 48
            || !letters
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(format!(
                "tool {tool:?}: a lowercase letter, then at most 47 lowercase letters, digits and _"
            ));
        }
        if description.len() > 1_024 {
            return Err("a skill with a tool has a description of at most 1,024 bytes".into());
        }
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
                Test::Faces(0.0)
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
            for_us: condition.lasting.as_deref().map_or(Ok(0), duration_us)?,
        });
    }
    let mut steps = Vec::new();
    for step in file.steps {
        if step.body.is_none() && step.light.is_none() && step.say.is_none() && step.look.is_none()
        {
            return Err("a [[do]] step names body, light, say or look".into());
        }
        match step.look {
            None => {}
            Some(true) if file.tool.is_some() && triggers.is_empty() => steps.push(Call::Look),
            Some(_) => {
                return Err(
                    "look = true, in a skill started by its tool alone: the model takes the look"
                        .into(),
                );
            }
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
            match (body.mood, body.lasting) {
                (None, None) => {}
                (Some(mood), Some(lasting)) => {
                    if !MOODS.contains(&mood.as_str()) {
                        return Err(format!("mood {mood:?} is not one of {MOODS:?}"));
                    }
                    let lasting_us = duration_us(&lasting)?;
                    if lasting_us == 0 || lasting_us > MOOD_AT_MOST_US {
                        return Err("a mood's for is more than 0 and at most 2m".into());
                    }
                    steps.push(Call::Mood { mood, lasting_us });
                }
                _ => return Err("a body mood goes with its for, and a for with a mood".into()),
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
    if steps
        .iter()
        .filter(|step| matches!(step, Call::Say(_)))
        .count()
        > 1
    {
        return Err(
            "one say a skill: its other steps are made, in their order, when the voice begins it"
                .into(),
        );
    }
    Ok(Skill {
        name,
        description,
        needs: file.needs,
        triggers,
        conditions,
        policy,
        tool: file.tool,
        steps,
    })
}

fn light_call(file: LightFile) -> Result<Call, String> {
    let until_ends = match file.until.as_deref() {
        None => false,
        Some("ends") => true,
        Some(other) => return Err(format!("until = {other:?}: only \"ends\"")),
    };
    if let Some(lasting) = &file.lasting
        && duration_us(lasting)? > 600_000_000
    {
        return Err("a light's for is at most 10m".into());
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
    /// Since when each skill's condition has held, by skill and condition.
    holding: HashMap<(String, usize), u64>,
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
            holding: HashMap::new(),
            skip_ask,
        }
    }

    /// The skills loaded again. A timed condition keeps when it began to
    /// hold only if it is the same condition as before.
    pub fn replace(&mut self, skills: Vec<Skill>) {
        let old = std::mem::replace(&mut self.skills, skills);
        let new = &self.skills;
        let condition = |skills: &[Skill], name: &str, index: usize| {
            skills
                .iter()
                .find(|skill| skill.name == name)
                .and_then(|skill| skill.conditions.get(index).cloned())
        };
        self.holding.retain(|(name, index), _| {
            let before = condition(&old, name, *index);
            before.is_some() && before == condition(new, name, *index)
        });
    }

    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    /// Every topic a trigger or a condition names, and the faces a look at
    /// a face reads, once each.
    pub fn topics(&self) -> Vec<String> {
        let mut topics: Vec<String> = self
            .skills
            .iter()
            .flat_map(|skill| {
                let looks = skill
                    .steps
                    .iter()
                    .any(|step| matches!(step, Call::Gaze { .. }));
                skill
                    .triggers
                    .iter()
                    .map(|trigger| trigger.topic.clone())
                    .chain(skill.conditions.iter().map(|test| test.topic.clone()))
                    .chain(looks.then(|| Faces::TOPIC.to_owned()))
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

    /// The condition holds on its topic's newest value that still lasts, and
    /// has since `since_us` for its `for`.
    fn condition_holds(
        latest: &HashMap<String, (Value, u64, Option<u64>)>,
        condition: &Condition,
        since_us: Option<&u64>,
        now_us: u64,
    ) -> bool {
        Self::fresh(latest, &condition.topic, now_us)
            .is_some_and(|value| condition.test.holds(value))
            && since_us.is_some_and(|since| now_us - since >= condition.for_us)
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
        // A timed condition held only while its topic's value lasted: after a
        // value that expired, it begins again.
        let lasted = Self::fresh(&self.latest, topic, now_us).is_some();
        self.latest
            .insert(topic.to_owned(), (value.clone(), now_us, lifespan_us));
        for skill in &self.skills {
            for (index, condition) in skill.conditions.iter().enumerate() {
                if condition.topic != topic {
                    continue;
                }
                let key = (skill.name.clone(), index);
                if condition.test.holds(&value) && lasted {
                    self.holding.entry(key).or_insert(now_us);
                } else if condition.test.holds(&value) {
                    self.holding.insert(key, now_us);
                } else {
                    self.holding.remove(&key);
                }
            }
        }
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
                        .enumerate()
                        .all(|(index, condition)| {
                            Self::condition_holds(
                                &self.latest,
                                condition,
                                self.holding.get(&(skill.name.clone(), index)),
                                now_us,
                            )
                        })
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
                runs.push(Run {
                    skill: skill.name.clone(),
                    events,
                    calls: calls(&self.latest, skill, now_us),
                });
            }
        }
        runs
    }

    /// The model called `name`: the first skill whose tool it is runs its
    /// steps. A person asked, so the Policy is not asked.
    /// Its `[[while]]` conditions must hold as for a trigger: on a value
    /// that still lasts, for their `for`. `Err` says why not, in words the
    /// lamp can say.
    pub fn tool(&self, name: &str, now_us: u64) -> Result<Run, String> {
        let skill = self
            .skills
            .iter()
            .find(|skill| skill.tool.as_deref() == Some(name))
            .ok_or("No skill of mine does that just now.")?;
        let holds = skill
            .conditions
            .iter()
            .enumerate()
            .all(|(index, condition)| {
                Self::condition_holds(
                    &self.latest,
                    condition,
                    self.holding.get(&(skill.name.clone(), index)),
                    now_us,
                )
            });
        if !holds {
            return Err("I can't do that just now.".into());
        }
        Ok(Run {
            skill: skill.name.clone(),
            events: vec![event(
                &skill.name,
                SkillStage::Triggered,
                format!("the model called {name}"),
                None,
            )],
            calls: calls(&self.latest, skill, now_us),
        })
    }
}

/// One tool call's admission, shared by the socket that waits for it and
/// the loop that runs it: whichever comes first, the run committed or the
/// caller told it timed out, wins, and the other then cannot happen.
#[derive(Clone, Default)]
pub struct Admission(std::sync::Arc<std::sync::atomic::AtomicU8>);

impl Admission {
    const PENDING: u8 = 0;
    const COMMITTED: u8 = 1;
    const CANCELLED: u8 = 2;

    fn settle(&self, to: u8) -> bool {
        use std::sync::atomic::Ordering;
        self.0
            .compare_exchange(Self::PENDING, to, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// The loop is about to answer and run it: false if the caller gave up.
    pub fn commit(&self) -> bool {
        self.settle(Self::COMMITTED)
    }

    /// The caller gives up: false if the loop committed first, whose answer
    /// is then on its way.
    pub fn cancel(&self) -> bool {
        self.settle(Self::CANCELLED)
    }
}

/// A skill's steps as calls now. The face to look at: the largest in the
/// newest list of faces that still lasts. With none, the other steps go on.
fn calls(
    latest: &HashMap<String, (Value, u64, Option<u64>)>,
    skill: &Skill,
    now_us: u64,
) -> Vec<Call> {
    let face = Engine::fresh(latest, Faces::TOPIC, now_us).and_then(|faces| {
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
    skill
        .steps
        .iter()
        .filter_map(|step| match step {
            Call::Gaze { .. } => face.map(|face| Call::Gaze { face: Some(face) }),
            other => Some(other.clone()),
        })
        .collect()
}

/// The tools file the voice reads beside the actions part's `tools.json`:
/// each skill's tool, described with the skill's description. A name an
/// earlier skill took is left out, as [`Engine::tool`] runs the first.
pub fn tools_file(skills: &[Skill]) -> Value {
    let mut tools: Vec<Value> = Vec::new();
    for skill in skills {
        if let Some(name) = &skill.tool
            && !tools.iter().any(|tool| tool["name"] == *name)
        {
            tools.push(json!({"name": name, "description": skill.description}));
        }
    }
    json!({ "tools": tools })
}

/// What the model is told when its call ran these calls. With a look, to
/// say only that it will look: the voice takes its full look when the lamp
/// says so. With a say, the note, for its answer.
pub fn told(calls: &[Call]) -> Value {
    let note = calls.iter().find_map(|call| match call {
        Call::Say(note) => Some(note.as_str()),
        _ => None,
    });
    const LOOK: &str = "Say only \"Let me take a look.\" now, in those words.";
    let next = match (calls.contains(&Call::Look), note) {
        (true, Some(note)) => format!("{LOOK} When the picture comes: {note}"),
        (true, None) => format!("{LOOK} The picture comes after it."),
        (false, Some(note)) => note.to_owned(),
        (false, None) => "Done.".to_owned(),
    };
    json!({ "next": next })
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

/// Whether a file or folder is hidden: its name begins with a dot. A folder
/// copied from a Mac carries `._NAME` AppleDouble files beside each file,
/// binary and named like it (lamp-4ace, 2026-10-16); they are not the
/// skill's.
pub fn hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

/// Every message in the `.jsonl` files of `dir`, in the order of their
/// `at_us`. Declaration lines and hidden files are passed over.
pub fn read_recordings(dir: &Path) -> Result<Vec<Recorded>, String> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            !hidden(path)
                && path
                    .extension()
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
        .filter(|path| path.is_dir() && !hidden(path))
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
