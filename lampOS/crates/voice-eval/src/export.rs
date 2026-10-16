//! `lamp-voice-eval export`: a window of a lamp's session, with no words, for
//! test cases (#1119).
//!
//! Builders without the lamp make test cases from what the lamp did. Until
//! this, the first builder made them by hand from the journal and the voice's
//! trace, taking out every word anyone said. This does the same by
//! construction: it never copies a field it does not know holds no words.
//!
//! - **The voice's trace** (`<session>/metadata/events-*.jsonl`): each event
//!   keeps its `kind`, `at_us`, and only its top-level numbers and booleans,
//!   and the strings of a few fields whose values the voice's code picks from
//!   a closed set (`outcome`, `reason`, `phase`, ...), when the value is one
//!   lower-case word of letters, digits and `_`. Nested objects, arrays, any
//!   other string, and every field of a kind whose name is not a plain word
//!   are left out and counted. A `transcript` keeps only who (`heard` or
//!   `lamp`), its length in characters, its turn and whether it was final.
//! - **The parts' journal** (`journalctl -o json`): only the lines whose
//!   shape is known, read into numbers and plain words (the Policy's
//!   decisions and outcomes, the body's gestures, faces and goals, a skill's
//!   stage). Every other line is left out and counted by its part.
//! - **The bus** (`--bus`, `lamp-bus record` files): only
//!   `humans/faces/tracked` and `voice/state`. A face keeps its box, its
//!   confidence, its yaw and whether it faced the lamp; its passing id is
//!   replaced by the export's own (`f1`, `f2`, ...) and every other field is
//!   left out and counted. A state keeps one of the four `lamp_msgs::Voice`
//!   words. Every other topic is left out and counted. The eyes write no face
//!   boxes to the journal, so a recording is the only source of faces.
//!
//! **The trust boundary.** A string is kept only from a field this file
//! names: the voice's [`WORD_FIELDS`] and the Policy's [`POLICY_KEYS`], and
//! only as one lower-case word. Those fields are trusted to be filled by
//! code from a closed set; checked against today's producers (a `reason` is
//! a code value or an error kind, never what a person said: a person's words
//! that become a reason go to `marked/`, not the trace). Whoever adds a field
//! there, or makes one of them echo a person's word, makes this tool export
//! it: add it here only when its values come from code.
//!
//! What it writes, into a new folder: `events.jsonl` (both, in time order,
//! each with `at_us` and the local wall time), `summary.json` (counts by kind,
//! and by kind and plain-word value, such as
//! `turn_finished.outcome=user_interrupted`), `left-out.json` (what was left
//! out, counted), and `case/voice_state.jsonl`, `voice/state` as a
//! `lamp-bus record` file a skill test replays (as recorded on `--bus`, else
//! derived from the turn's events, below), and `case/humans_faces_tracked.jsonl`
//! when `--bus` held faces. The case's files share one start, timed by the
//! monotonic `at_us` the voice's trace and the bus both carry, so a replay
//! keeps their order and gaps.
use crate::{Result, invalid};
use lamp_local_time::LocalTime;
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

/// A string field the voice's code fills from a closed set of words. Its
/// value is kept only when it is one plain word ([`plain_word`]).
const WORD_FIELDS: &[&str] = &[
    "outcome",
    "provider_outcome",
    "reason",
    "phase",
    "link",
    "provider_kind",
    "admission_basis",
    "basis",
    "mode",
    "scope",
    "state",
    "trigger",
    "choice",
    "fault",
    "gesture",
    "cause",
    "result",
    "trace_role",
    "policy",
];

/// The keys the Policy's four decision lines write (`lamp-policy`'s
/// `Note` in `crates/policy/src/policy.rs`): each a number, `true`/`false`
/// or a value its code picks. Any other key is left out and counted, even
/// when its value looks like one plain word.
const POLICY_KEYS: &[&str] = &[
    "task",
    "change",
    "arrival",
    "about",
    "choice",
    "probability",
    "by",
    "took_ms",
    "waited_ms",
    "guard",
    "on",
    "pieces",
    "bytes",
    "lamp",
    "spoke_after_ms",
    "heard_to_sound_ms",
    "overtaken",
    "logged",
];

/// The biggest input read: a day of a lamp's traces is well under this.
const MAX_INPUT_BYTES: u64 = 256 * 1024 * 1024;

/// What to export.
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Local wall time, `HH:MM:SS`, on [`ExportOptions::date`].
    pub since: String,
    pub until: String,
    /// `YYYY-MM-DD` in the lamp's zone; today's when not given.
    pub date: Option<String>,
    /// The voice's session folders (any depth): `/var/lib/lampOS/voice/sessions`.
    pub sessions: Option<PathBuf>,
    /// `journalctl -o json` output.
    pub journal: Option<PathBuf>,
    /// `lamp-bus record` files (`humans_faces_tracked.jsonl`,
    /// `voice_state.jsonl`): the faces in view and the voice's state as the
    /// bus carried them.
    pub bus: Vec<PathBuf>,
    /// `system` or an IANA zone name.
    pub zone: String,
    /// Wall clock minus the monotonic clock, in microseconds, for the boot
    /// the sessions were written in. Taken from the journal when it is given,
    /// else from this machine's clocks now (right only on the lamp, in the
    /// same boot).
    pub wall_offset_us: Option<i64>,
    pub out: PathBuf,
}

/// One lower-case word of letters, digits and `_`, at most 48: the shape of
/// every value the voice's code picks for a [`WORD_FIELDS`] field and of
/// every event kind. A sentence, a name with a capital, or a word with a
/// space never is.
pub fn plain_word(text: &str) -> bool {
    (1..=48).contains(&text.len())
        && text.as_bytes()[0].is_ascii_lowercase()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// A Rust enum's `Debug` name (`Calm`, `Started`): a capital, then letters.
fn enum_name(text: &str) -> bool {
    (1..=24).contains(&text.len())
        && text.as_bytes()[0].is_ascii_uppercase()
        && text.bytes().all(|byte| byte.is_ascii_alphabetic())
}

/// A skill's folder name or a gesture's: lower-case words joined by `-`/`_`.
fn name_word(text: &str) -> bool {
    (1..=48).contains(&text.len())
        && text.as_bytes()[0].is_ascii_lowercase()
        && text.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
}

/// What was left out, counted: never what it said.
#[derive(Default)]
pub struct LeftOut {
    /// Voice events whose kind is not a plain word.
    pub voice_kinds_unreadable: u64,
    /// Fields left out of voice events, by event kind.
    pub voice_fields: BTreeMap<String, u64>,
    /// Voice lines that are not JSON objects.
    pub voice_lines_unreadable: u64,
    /// Journal lines left out, by part (`other` when the part is not a plain word).
    pub journal_lines: BTreeMap<String, u64>,
    /// `key=value` pairs left out of a known journal line, by its kind.
    pub journal_pairs: BTreeMap<String, u64>,
    /// Recorded bus messages left out, by topic (`other` when the topic's
    /// name is not a plain topic name), declaration lines not counted.
    pub bus_messages: BTreeMap<String, u64>,
    /// Fields left out of a kept bus message, by topic.
    pub bus_fields: BTreeMap<String, u64>,
}

impl LeftOut {
    fn json(&self) -> Value {
        json!({
            "voice_kinds_unreadable": self.voice_kinds_unreadable,
            "voice_lines_unreadable": self.voice_lines_unreadable,
            "voice_fields_by_kind": self.voice_fields,
            "journal_lines_by_part": self.journal_lines,
            "journal_pairs_by_kind": self.journal_pairs,
            "bus_messages_by_topic": self.bus_messages,
            "bus_fields_by_topic": self.bus_fields,
        })
    }
}

/// One voice trace event, with only what holds no words. `None` when its
/// kind is not a plain word.
pub fn voice_event(event: &Value, left: &mut LeftOut) -> Option<Map<String, Value>> {
    let Some(object) = event.as_object() else {
        left.voice_lines_unreadable += 1;
        return None;
    };
    let Some(kind) = object
        .get("kind")
        .and_then(Value::as_str)
        .filter(|kind| plain_word(kind))
    else {
        left.voice_kinds_unreadable += 1;
        return None;
    };
    let at_us = object.get("at_us").and_then(Value::as_u64)?;
    let mut kept = Map::new();
    kept.insert("kind".into(), kind.into());
    kept.insert("at_us".into(), at_us.into());
    let mut dropped = 0;
    if kind == "transcript" {
        // Who said it, how long, which turn: a heard line has no turn (the
        // provider's input transcript is not tied to a request).
        let turn = object.get("turn").and_then(Value::as_u64);
        kept.insert(
            "who".into(),
            if turn.is_some() { "lamp" } else { "heard" }.into(),
        );
        if let Some(turn) = turn {
            kept.insert("turn".into(), turn.into());
        }
        let chars = object
            .get("text")
            .and_then(Value::as_str)
            .map_or(0, |text| text.chars().count());
        kept.insert("chars".into(), chars.into());
        if let Some(finished) = object.get("finished").and_then(Value::as_bool) {
            kept.insert("finished".into(), finished.into());
        }
        dropped += object
            .keys()
            .filter(|key| !matches!(key.as_str(), "kind" | "at_us" | "turn" | "finished"))
            .count() as u64;
    } else {
        for (key, value) in object {
            if key == "kind" || key == "at_us" {
                continue;
            }
            let keep = plain_word(key)
                && match value {
                    Value::Number(_) | Value::Bool(_) => true,
                    Value::String(text) => WORD_FIELDS.contains(&key.as_str()) && plain_word(text),
                    _ => false,
                };
            if keep {
                kept.insert(key.clone(), value.clone());
            } else if !value.is_null() {
                dropped += 1;
            }
        }
    }
    if dropped > 0 {
        *left.voice_fields.entry(kind.to_owned()).or_default() += dropped;
    }
    Some(kept)
}

/// A number written by the parts: `412`, `0.214`, `-3`.
fn number(text: &str) -> Option<Value> {
    if let Ok(whole) = text.parse::<i64>() {
        return Some(whole.into());
    }
    text.parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .and_then(serde_json::Number::from_f64)
        .map(Value::Number)
}

/// The numbers of a `[1, 2, 3]` list the body writes.
fn numbers(text: &str) -> Option<Vec<Value>> {
    let inner = text.strip_prefix('[')?.strip_suffix(']')?;
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    inner.split(',').map(|part| number(part.trim())).collect()
}

/// A Policy line's `key=value` pairs, kept when the key is one the Policy
/// writes ([`POLICY_KEYS`]) and the value a number, `true`/`false` or a
/// plain word.
fn pairs(kind: &str, rest: &str, kept: &mut Map<String, Value>, left: &mut LeftOut) {
    for token in rest.split_whitespace() {
        let value = token.split_once('=').and_then(|(key, value)| {
            if !POLICY_KEYS.contains(&key) {
                return None;
            }
            let value = match value {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => number(value).or_else(|| plain_word(value).then(|| value.into()))?,
            };
            Some((key, value))
        });
        match value {
            Some((key, value)) => {
                kept.insert(key.to_owned(), value);
            }
            None => *left.journal_pairs.entry(kind.to_owned()).or_default() += 1,
        }
    }
}

/// One part's journal message, read into a word-free event when its shape
/// is known. `part` is the part's name (`lamp-policy`).
pub fn journal_event(part: &str, message: &str, left: &mut LeftOut) -> Option<Map<String, Value>> {
    let mut kept = Map::new();
    let kind = match part {
        "lamp-policy" => {
            let (kind, rest) = [
                ("outcome of decision ", "policy_outcome"),
                ("progress decision ", "policy_progress_decision"),
                ("hello decision ", "policy_hello_decision"),
                ("decision ", "policy_decision"),
            ]
            .into_iter()
            .find_map(|(prefix, kind)| message.strip_prefix(prefix).map(|rest| (kind, rest)))?;
            let (decision, rest) = rest.split_once(' ').unwrap_or((rest, ""));
            kept.insert("decision".into(), decision.parse::<u64>().ok()?.into());
            pairs(kind, rest, &mut kept, left);
            kind
        }
        "lamp-body" => {
            let rest = message.strip_prefix("step ")?;
            let (step, rest) = rest.split_once(' ')?;
            kept.insert("step".into(), step.parse::<u64>().ok()?.into());
            if let Some(gesture) = rest.strip_prefix("gesture ") {
                if gesture != "over" && !name_word(gesture) {
                    return None;
                }
                kept.insert("gesture".into(), gesture.into());
                "body_gesture"
            } else {
                let (mood, rest) = rest.split_once(' ')?;
                if !enum_name(mood) {
                    return None;
                }
                kept.insert("mood".into(), mood.into());
                if let Some(face) = rest.strip_prefix("face at ") {
                    // `{across} across {down} down, size {size}; looking [..]`
                    let (across, face) = face.split_once(" across ")?;
                    let (down, face) = face.split_once(" down, size ")?;
                    let size = face.split(';').next()?;
                    kept.insert("across".into(), number(across)?);
                    kept.insert("down".into(), number(down)?);
                    kept.insert("size".into(), number(size.trim())?);
                    "body_face"
                } else {
                    let goals = rest.strip_prefix("goals ")?;
                    let goals = &goals[..goals.find(']')? + 1];
                    kept.insert("goals".into(), Value::Array(numbers(goals)?));
                    "body_goals"
                }
            }
        }
        "lamp-skills" => {
            // `{skill} {stage:?}: {why}`: the why is left out, always.
            let (head, _why) = message.split_once(": ")?;
            let (skill, stage) = head.split_once(' ')?;
            if !name_word(skill) || !enum_name(stage) {
                return None;
            }
            kept.insert("skill".into(), skill.into());
            kept.insert("stage".into(), stage.into());
            "skill_stage"
        }
        _ => return None,
    };
    kept.insert("kind".into(), kind.into());
    Some(kept)
}

/// A journal entry's part, time and message, from `journalctl -o json`.
struct Entry {
    part: String,
    realtime_us: u64,
    monotonic_us: u64,
    boot: String,
    message: String,
}

fn journal_entry(line: &Value) -> Option<Entry> {
    let text = |key: &str| line.get(key).and_then(Value::as_str);
    let micros = |key: &str| text(key).and_then(|value| value.parse::<u64>().ok());
    // A message that is not text comes as an array of bytes: left out.
    let message = text("MESSAGE")?;
    // The part writes `lamp-policy: ...`; the identifier says the same.
    let (part, message) = match message.split_once(": ") {
        Some((part, rest)) if part.starts_with("lamp-") && name_word(part) => (part, rest),
        _ => (text("SYSLOG_IDENTIFIER").unwrap_or("other"), message),
    };
    Some(Entry {
        part: part.to_owned(),
        realtime_us: micros("__REALTIME_TIMESTAMP")?,
        monotonic_us: micros("__MONOTONIC_TIMESTAMP")?,
        boot: text("_BOOT_ID").unwrap_or_default().to_owned(),
        message: message.to_owned(),
    })
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn parse_clock(text: &str) -> Result<i64> {
    let parts: Vec<&str> = text.split(':').collect();
    let [hours, minutes, seconds] = parts.as_slice() else {
        return Err(invalid("a time is HH:MM:SS"));
    };
    let (hours, minutes, seconds): (i64, i64, i64) =
        (hours.parse()?, minutes.parse()?, seconds.parse()?);
    if !(0..24).contains(&hours) || !(0..60).contains(&minutes) || !(0..60).contains(&seconds) {
        return Err(invalid("a time is HH:MM:SS"));
    }
    Ok((hours * 60 + minutes) * 60 + seconds)
}

fn parse_date(text: &str) -> Result<i64> {
    let parts: Vec<&str> = text.split('-').collect();
    let [year, month, day] = parts.as_slice() else {
        return Err(invalid("a date is YYYY-MM-DD"));
    };
    let (year, month, day): (i64, i64, i64) = (year.parse()?, month.parse()?, day.parse()?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(invalid("a date is YYYY-MM-DD"));
    }
    Ok(days_from_civil(year, month, day))
}

/// The unix milliseconds of a local `seconds` of the local `day` in `zone`.
pub fn local_to_unix_ms(zone: &LocalTime, day: i64, seconds: i64) -> u64 {
    let local_ms = (day * 86_400 + seconds) * 1000;
    // The offset at the moment itself: guess with the offset at the local
    // reading taken as UTC, then once more at the guess.
    let guess = (local_ms - i64::from(zone.offset_minutes_at(local_ms.max(0) as u64)) * 60_000)
        .max(0) as u64;
    (local_ms - i64::from(zone.offset_minutes_at(guess)) * 60_000).max(0) as u64
}

/// `HH:MM:SS.mmm` local.
fn wall_text(zone: &LocalTime, unix_ms: u64) -> String {
    let local = unix_ms as i64 + i64::from(zone.offset_minutes_at(unix_ms)) * 60_000;
    let of_day = local.rem_euclid(86_400_000);
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        of_day / 3_600_000,
        of_day / 60_000 % 60,
        of_day / 1000 % 60,
        of_day % 1000
    )
}

fn read_bounded(path: &Path) -> Result<String> {
    if fs::metadata(path)?.len() > MAX_INPUT_BYTES {
        return Err(invalid(&format!("{} is over 256 MiB", path.display())));
    }
    Ok(fs::read_to_string(path)?)
}

/// Every `events-*.jsonl` file under `root`, at any depth.
fn trace_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_owned()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type()?.is_dir() {
                folders.push(path);
            } else if name.starts_with("events-") && name.ends_with(".jsonl") {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// The voice's `voice/state` from the turn's events, as `bus_out` publishes
/// it from the ring's phase: listening from the admitted input, thinking from
/// its end, speaking from the first sound, resting when the turn is over.
/// Derived, not recorded: the voice does not write its state to its trace.
fn voice_state(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "input_admitted" => "listening",
        "local_endpoint" | "input_discarded_endpoint" => "thinking",
        "speaker_first_write" => "speaking",
        "turn_finished" | "speech_final_sample_retired" | "provider_input_abandoned" => "resting",
        _ => return None,
    })
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::new();
    for chunk in bytes.chunks(3) {
        let group = chunk.iter().enumerate().fold(0u32, |group, (index, byte)| {
            group | u32::from(*byte) << (16 - 8 * index)
        });
        for index in 0..4 {
            if index <= chunk.len() {
                text.push(ALPHABET[(group >> (18 - 6 * index) & 63) as usize] as char);
            } else {
                text.push('=');
            }
        }
    }
    text
}

/// The topics read from a `lamp-bus record` file. Every other topic is left
/// out and counted.
const FACES_TOPIC: &str = "humans/faces/tracked";
const VOICE_STATE_TOPIC: &str = "voice/state";
/// `lamp_msgs::Voice`, as the bus writes it.
const VOICE_STATES: [&str; 4] = ["resting", "listening", "thinking", "speaking"];

/// A topic's name, when it is one: lower-case words joined by `/`, `_`, `-`.
fn topic_name(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && text.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'/' | b'_' | b'-')
        })
}

/// Standard base64, as `lamp-bus record` writes payloads.
fn unbase64(text: &str) -> Option<Vec<u8>> {
    let value = |byte: u8| -> Option<u32> {
        Some(match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let text = text.trim_end_matches('=').as_bytes();
    let mut bytes = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.chunks(4) {
        if chunk.len() == 1 {
            return None;
        }
        let mut word = 0u32;
        for (index, byte) in chunk.iter().enumerate() {
            word |= value(*byte)? << (18 - 6 * index);
        }
        bytes.push((word >> 16) as u8);
        if chunk.len() > 2 {
            bytes.push((word >> 8) as u8);
        }
        if chunk.len() > 3 {
            bytes.push(word as u8);
        }
    }
    Some(bytes)
}

/// A number from 0 to 1, as a face's edges and confidence are.
fn unit(value: Option<&Value>) -> Option<Value> {
    let value = value?;
    (0.0..=1.0)
        .contains(&value.as_f64()?)
        .then(|| value.clone())
}

/// A `lamp_msgs::Faces` payload with only its boxes, confidences, yaws and
/// whether each face was turned to the lamp. A face's passing id is replaced
/// by one of the export's own (`f1`, `f2`, ...), the same for the same id:
/// the recorded id never leaves. A face without a whole box is left out.
fn faces(payload: &Value, ids: &mut BTreeMap<String, String>, left: &mut LeftOut) -> Option<Value> {
    let mut dropped = 0;
    let mut kept = Vec::new();
    for face in payload.get("faces")?.as_array()? {
        let Some(face) = face.as_object() else {
            dropped += 1;
            continue;
        };
        let mut out = Map::new();
        let edges = ["xmin", "ymin", "xmax", "ymax", "confidence"]
            .into_iter()
            .map(|key| unit(face.get(key)).map(|value| (key, value)))
            .collect::<Option<Vec<_>>>();
        let (Some(id), Some(edges)) = (face.get("id").and_then(Value::as_str), edges) else {
            dropped += 1;
            continue;
        };
        let next = format!("f{}", ids.len() + 1);
        let id = ids.entry(id.to_owned()).or_insert(next).clone();
        out.insert("id".into(), id.into());
        for (key, value) in edges {
            out.insert(key.into(), value);
        }
        if let Some(yaw) = face
            .get("yaw")
            .and_then(Value::as_f64)
            .filter(|yaw| yaw.is_finite())
        {
            out.insert("yaw".into(), json!(yaw));
        }
        if let Some(facing) = face.get("facing").and_then(Value::as_bool) {
            out.insert("facing".into(), facing.into());
        }
        dropped += face
            .keys()
            .filter(|key| {
                !matches!(
                    key.as_str(),
                    "id" | "xmin" | "ymin" | "xmax" | "ymax" | "confidence" | "yaw" | "facing"
                )
            })
            .count() as u64;
        kept.push(Value::Object(out));
    }
    if dropped > 0 {
        *left.bus_fields.entry(FACES_TOPIC.into()).or_default() += dropped;
    }
    Some(json!({ "faces": kept }))
}

/// One recorded bus message kept: its topic, the publisher's monotonic
/// `at_us` (the clock the voice's trace has too, on the same lamp), its
/// wall `at_unix_ms`, `expires_ms`, and its word-free payload.
struct BusMessage {
    topic: &'static str,
    at_us: u64,
    at_unix_ms: u64,
    expires_ms: Option<u64>,
    payload: Value,
}

/// One message of a case: monotonic `at_us`, wall `at_unix_ms`,
/// `expires_ms`, payload.
type CaseMessage = (u64, u64, Option<u64>, Value);

/// The kept messages of `lamp-bus record` lines within the window.
fn bus_messages(
    text: &str,
    since_ms: u64,
    until_ms: u64,
    ids: &mut BTreeMap<String, String>,
    left: &mut LeftOut,
) -> Vec<BusMessage> {
    let mut kept = Vec::new();
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            *left.bus_messages.entry("unreadable".into()).or_default() += 1;
            continue;
        };
        if value.get("declaration").is_some() {
            continue;
        }
        let header = &value["header"];
        let topic = header["topic"].as_str().unwrap_or_default();
        let counted = if topic_name(topic) { topic } else { "other" };
        let (Some(at_us), Some(at_unix_ms)) =
            (header["at_us"].as_u64(), header["at_unix_ms"].as_u64())
        else {
            *left.bus_messages.entry(counted.into()).or_default() += 1;
            continue;
        };
        if !(since_ms..=until_ms).contains(&at_unix_ms) {
            continue;
        }
        let payload = value["payload_b64"]
            .as_str()
            .and_then(unbase64)
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        let message = match (topic, payload) {
            (FACES_TOPIC, Some(payload)) => {
                faces(&payload, ids, left).map(|payload| (FACES_TOPIC, payload))
            }
            (VOICE_STATE_TOPIC, Some(payload)) => payload["state"]
                .as_str()
                .and_then(|state| VOICE_STATES.into_iter().find(|known| *known == state))
                .map(|state| (VOICE_STATE_TOPIC, json!({ "state": state }))),
            _ => None,
        };
        match message {
            Some((topic, payload)) => kept.push(BusMessage {
                topic,
                at_us,
                at_unix_ms,
                expires_ms: header["expires_ms"].as_u64(),
                payload,
            }),
            None => *left.bus_messages.entry(counted.into()).or_default() += 1,
        }
    }
    kept
}

/// A topic's messages as `lamp-bus record` writes them, a declaration
/// first, each message's `at_us` from `base_us`, the case's start: one
/// monotonic clock for every topic, so a replay keeps their order and gaps.
fn case_file(topic: &str, type_name: &str, messages: &[CaseMessage], base_us: u64) -> String {
    let mut case =
        json!({"topic": topic, "declaration": {"type": type_name, "version": 1, "kind": "latest"}})
            .to_string();
    case.push('\n');
    for (seq, (at_us, at_unix_ms, expires_ms, payload)) in messages.iter().enumerate() {
        let mut header = json!({"topic": topic, "kind": "latest", "enc": "json", "type": type_name,
            "source": "lamp_voice_eval_export", "boot": "0000000000001119", "seq": seq + 1,
            "at_us": at_us.saturating_sub(base_us), "at_unix_ms": at_unix_ms});
        if let Some(expires_ms) = expires_ms {
            header["expires_ms"] = json!(expires_ms);
        }
        case.push_str(
            &json!({"header": header, "payload_b64": base64(payload.to_string().as_bytes())})
                .to_string(),
        );
        case.push('\n');
    }
    case
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    Ok(())
}

/// Export the window. Returns the summary it wrote.
pub fn export(options: &ExportOptions) -> Result<Value> {
    if options.sessions.is_none() && options.journal.is_none() && options.bus.is_empty() {
        return Err(invalid("export needs --sessions, --journal or --bus"));
    }
    let zone = LocalTime::zone(&options.zone)?;
    let now_ms = crate::ledger::unix_ms();
    let day = match &options.date {
        Some(date) => parse_date(date)?,
        None => zone.day(now_ms),
    };
    let since_ms = local_to_unix_ms(&zone, day, parse_clock(&options.since)?);
    let until_ms = local_to_unix_ms(&zone, day, parse_clock(&options.until)?);
    if until_ms <= since_ms {
        return Err(invalid("--until is not after --since"));
    }
    let mut left = LeftOut::default();
    let mut rows: Vec<(u64, Map<String, Value>)> = Vec::new();

    // The journal: its own wall time, and the boot's offset for the voice.
    let mut offsets: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    if let Some(journal) = &options.journal {
        for line in read_bounded(journal)?.lines() {
            let Some(entry) = serde_json::from_str::<Value>(line)
                .ok()
                .as_ref()
                .and_then(journal_entry)
            else {
                *left.journal_lines.entry("unreadable".into()).or_default() += 1;
                continue;
            };
            offsets
                .entry(entry.boot.clone())
                .or_default()
                .push(entry.realtime_us as i64 - entry.monotonic_us as i64);
            let wall_ms = entry.realtime_us / 1000;
            if !(since_ms..=until_ms).contains(&wall_ms) {
                continue;
            }
            match journal_event(&entry.part, &entry.message, &mut left) {
                Some(mut event) => {
                    event.insert("source".into(), "journal".into());
                    event.insert("at_us".into(), entry.monotonic_us.into());
                    rows.push((wall_ms, event));
                }
                None => {
                    let part = if name_word(&entry.part) {
                        entry.part
                    } else {
                        "other".into()
                    };
                    *left.journal_lines.entry(part).or_default() += 1;
                }
            }
        }
    }
    // The boot with the most journal lines is the one the window is in.
    let journal_offset = offsets
        .values()
        .max_by_key(|values| values.len())
        .map(|values| {
            let mut values = values.clone();
            values.sort_unstable();
            values[values.len() / 2]
        });
    let (offset_us, offset_from) = match (options.wall_offset_us, journal_offset) {
        (Some(offset), _) => (offset, "given"),
        (None, Some(offset)) => (offset, "journal"),
        (None, None) => (
            (now_ms * 1000) as i64 - lamp_ipc::monotonic_us() as i64,
            "this machine's clocks now",
        ),
    };

    let mut states = Vec::new();
    if let Some(sessions) = &options.sessions {
        for file in trace_files(sessions)? {
            for line in read_bounded(&file)?.lines() {
                let Ok(value) = serde_json::from_str::<Value>(line) else {
                    left.voice_lines_unreadable += 1;
                    continue;
                };
                let Some(mut event) = voice_event(&value, &mut left) else {
                    continue;
                };
                let at_us = event["at_us"].as_u64().unwrap_or_default();
                let wall_ms = ((at_us as i64 + offset_us) / 1000).max(0) as u64;
                if !(since_ms..=until_ms).contains(&wall_ms) {
                    continue;
                }
                if let Some(state) = event["kind"].as_str().and_then(voice_state) {
                    states.push((at_us, wall_ms, state));
                }
                event.insert("source".into(), "voice".into());
                rows.push((wall_ms, event));
            }
        }
    }
    // The bus: the faces in view, and the voice's state as it was published.
    let mut ids = BTreeMap::new();
    let mut recorded_faces = Vec::new();
    let mut recorded_states = Vec::new();
    for file in &options.bus {
        for message in bus_messages(
            &read_bounded(file)?,
            since_ms,
            until_ms,
            &mut ids,
            &mut left,
        ) {
            let wall_ms = message.at_unix_ms;
            let mut event = Map::new();
            match message.topic {
                FACES_TOPIC => {
                    let count = message.payload["faces"].as_array().map_or(0, Vec::len);
                    event.insert("kind".into(), "faces_tracked".into());
                    event.insert("faces".into(), count.into());
                    event.insert("boxes".into(), message.payload["faces"].clone());
                    recorded_faces.push((
                        message.at_us,
                        message.at_unix_ms,
                        message.expires_ms,
                        message.payload,
                    ));
                }
                _ => {
                    event.insert("kind".into(), "voice_state".into());
                    event.insert("state".into(), message.payload["state"].clone());
                    recorded_states.push((
                        message.at_us,
                        message.at_unix_ms,
                        message.expires_ms,
                        message.payload,
                    ));
                }
            }
            event.insert("at_us".into(), message.at_us.into());
            event.insert("source".into(), "bus".into());
            rows.push((wall_ms, event));
        }
    }
    rows.sort_by_key(|(wall, event)| (*wall, event["at_us"].as_u64()));
    states.sort_unstable();
    recorded_faces.sort_by_key(|(at_us, ..)| *at_us);
    recorded_states.sort_by_key(|(at_us, ..)| *at_us);

    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut lines = String::new();
    for (wall_ms, event) in &mut rows {
        let kind = event["kind"].as_str().unwrap_or_default().to_owned();
        *counts.entry(kind.clone()).or_default() += 1;
        if kind == "voice_state"
            && let Some(state) = event.get("state").and_then(Value::as_str)
        {
            *counts
                .entry(format!("voice_state.state={state}"))
                .or_default() += 1;
        }
        for key in WORD_FIELDS {
            if let Some(value) = event.get(*key).and_then(Value::as_str) {
                *counts.entry(format!("{kind}.{key}={value}")).or_default() += 1;
            }
        }
        event.insert("wall".into(), wall_text(&zone, *wall_ms).into());
        lines.push_str(&Value::Object(event.clone()).to_string());
        lines.push('\n');
    }

    // The case: each topic as `lamp-bus record` writes it, all from one
    // start, the first message of any of them, as the skills' recorded cases
    // have it. `voice/state` as the bus carried it when it was recorded,
    // else derived from the turn's events.
    let (state_messages, state_from) = if recorded_states.is_empty() {
        let mut derived: Vec<CaseMessage> = Vec::new();
        let mut last = None;
        for (at_us, wall_ms, state) in &states {
            if last == Some(*state) {
                continue;
            }
            last = Some(*state);
            derived.push((*at_us, *wall_ms, None, json!({ "state": state })));
        }
        (
            derived,
            "derived from the turn's events: input_admitted listening, local_endpoint thinking, speaker_first_write speaking, turn_finished resting",
        )
    } else {
        (recorded_states, "recorded: voice/state from --bus")
    };
    let base_us = state_messages
        .iter()
        .chain(&recorded_faces)
        .map(|(at_us, ..)| *at_us)
        .min()
        .unwrap_or_default();
    let case = case_file(
        VOICE_STATE_TOPIC,
        "lamp_msgs/VoiceState",
        &state_messages,
        base_us,
    );
    let faces_case = (!recorded_faces.is_empty())
        .then(|| case_file(FACES_TOPIC, "lamp_msgs/Faces", &recorded_faces, base_us));
    let seq = state_messages.len();

    let summary = json!({
        "window": {"since": options.since, "until": options.until,
            "since_unix_ms": since_ms, "until_unix_ms": until_ms, "zone": options.zone},
        "wall_offset_us": offset_us,
        "wall_offset_from": offset_from,
        "events": rows.len(),
        "counts": counts,
        "voice_state_messages": seq,
        "voice_state_from": state_from,
        "faces_messages": recorded_faces.len(),
        "faces_seen": ids.len(),
    });

    fs::DirBuilder::new().mode(0o700).create(&options.out)?;
    fs::DirBuilder::new()
        .mode(0o700)
        .create(options.out.join("case"))?;
    write_new(&options.out.join("events.jsonl"), lines.as_bytes())?;
    write_new(
        &options.out.join("summary.json"),
        format!("{summary:#}\n").as_bytes(),
    )?;
    write_new(
        &options.out.join("left-out.json"),
        format!("{:#}\n", left.json()).as_bytes(),
    )?;
    write_new(&options.out.join("case/voice_state.jsonl"), case.as_bytes())?;
    if let Some(faces_case) = faces_case {
        write_new(
            &options.out.join("case/humans_faces_tracked.jsonl"),
            faces_case.as_bytes(),
        )?;
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Words that must never leave: in every text field, a name, a tool's
    /// arguments, an unknown kind, an unknown field, and journal lines.
    const SECRETS: &[&str] = &[
        "Marigold",
        "zebrafinch",
        "Quibbleton",
        "papayasecret",
        "Hollowmere",
        "tangerinepass",
        "oystercatcher",
        "Brindlewick",
        "lanternfjord",
        "velvetquokka",
        "Pemberly",
        "quillmouse",
        "Ottoline",
        "nightjarvane",
    ];

    struct Dir(PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn temp(name: &str) -> Dir {
        let path = std::env::temp_dir().join(format!(
            "lamp-export-{name}-{}-{}",
            std::process::id(),
            crate::ledger::unix_ms()
        ));
        fs::create_dir_all(&path).unwrap();
        Dir(path)
    }

    fn journal_line(part: &str, message: &str, realtime_us: u64, monotonic_us: u64) -> String {
        json!({"MESSAGE": format!("{part}: {message}"), "SYSLOG_IDENTIFIER": part,
            "__REALTIME_TIMESTAMP": realtime_us.to_string(),
            "__MONOTONIC_TIMESTAMP": monotonic_us.to_string(), "_BOOT_ID": "b1"})
        .to_string()
    }

    /// 2026-10-16 17:00:00 +07 is 10:00:00 UTC.
    const BASE_MS: u64 = 1_792_144_800_000;
    /// The boot's offset: wall microseconds minus monotonic ones.
    const OFFSET_US: u64 = BASE_MS * 1000 - 5_000_000_000;

    fn fixture(dir: &Path) -> (PathBuf, PathBuf) {
        let session = dir.join("sessions/one/metadata");
        fs::create_dir_all(&session).unwrap();
        let at = |seconds: u64| 5_000_000_000 + seconds * 1_000_000;
        let events = [
            json!({"kind":"run_start","at_us":at(1),"provider_kind":"gemini","seconds":0,"note":SECRETS[0]}),
            json!({"kind":"input_admitted","at_us":at(2),"turn":1,"admission_basis":"speech","candidate":{"id":7,"label":SECRETS[1]}}),
            json!({"kind":"transcript","at_us":at(3),"turn":null,"text":format!("hey {} what time", SECRETS[2]),"finished":true,"input_correlation":"unreliable; session scoped"}),
            json!({"kind":"local_endpoint","at_us":at(3),"turn":1,"includes_silence_wait_ms":300}),
            json!({"kind":"function_named","at_us":at(4),"turn":1,"name":SECRETS[3],"args":{"city":SECRETS[4]}}),
            json!({"kind":"transcript","at_us":at(5),"turn":1,"text":format!("It is five, {}.", SECRETS[5]),"finished":true}),
            json!({"kind":"speaker_first_write","at_us":at(5),"turn":1}),
            // A closed-set field carrying a sentence is still left out.
            json!({"kind":"turn_finished","at_us":at(6),"turn":1,"outcome":"user_interrupted","reason":format!("because {}", SECRETS[6])}),
            json!({"kind":SECRETS[7],"at_us":at(7),"turn":2}),
            json!({"kind":"skill_says","at_us":at(8),"skill":"stretch-break","say":SECRETS[8]}),
            // Outside the window.
            json!({"kind":"turn_finished","at_us":at(4000),"turn":9,"outcome":"answered"}),
        ];
        let text: String = events.iter().map(|event| format!("{event}\n")).collect();
        fs::write(
            session.join("events-0.jsonl"),
            text + "not json at all " + SECRETS[9] + "\n",
        )
        .unwrap();
        let wall = |seconds: u64| OFFSET_US + at(seconds);
        let journal = [
            journal_line(
                "lamp-policy",
                "decision 12 about=attention choice=answer probability=0.214 by=model took_ms=561 waited_ms=0 on=new pieces=3 bytes=900",
                wall(2),
                at(2),
            ),
            journal_line(
                "lamp-policy",
                &format!(
                    "outcome of decision 12 choice=answer lamp=spoke spoke_after_ms=412 heard_to_sound_ms=1290 overtaken=false logged=true note={} heard={}",
                    SECRETS[0], SECRETS[1]
                ),
                wall(6),
                at(6),
            ),
            journal_line(
                "lamp-policy",
                &format!("could not reach the service: {}", SECRETS[1]),
                wall(6),
                at(6),
            ),
            journal_line("lamp-body", "step 812 gesture nod", wall(5), at(5)),
            journal_line(
                "lamp-body",
                "step 900 Calm face at 0.12 across -0.30 down, size 0.21; looking [3.0, -1.5]",
                wall(5),
                at(5),
            ),
            journal_line(
                "lamp-body",
                "step 950 Curious goals [2048, 2100, 1990, 2010, 2000] positions [1, 2, 3, 4, 5] loads [0, 0, 0, 0, 0]",
                wall(6),
                at(6),
            ),
            journal_line(
                "lamp-skills",
                &format!(
                    "stretch-break Started: {} has sat for 50 minutes",
                    SECRETS[2]
                ),
                wall(6),
                at(6),
            ),
            // A skill or gesture name is a folder's or the code's: one that is
            // not such a name is left out with its line.
            journal_line(
                "lamp-skills",
                &format!("{} Started: x", SECRETS[7]),
                wall(6),
                at(6),
            ),
            journal_line(
                "lamp-eyes",
                &format!("asked to remember {}", SECRETS[4]),
                wall(6),
                at(6),
            ),
            journal_line(
                "lamp-body",
                &format!("step 1 gesture {}", SECRETS[4]),
                wall(6),
                at(6),
            ),
        ];
        let journal_path = dir.join("journal.json");
        fs::write(&journal_path, journal.join("\n") + "\n").unwrap();
        (dir.join("sessions"), journal_path)
    }

    fn options(dir: &Path, sessions: PathBuf, journal: Option<PathBuf>) -> ExportOptions {
        ExportOptions {
            since: "17:00:00".into(),
            until: "17:30:00".into(),
            date: Some("2026-10-16".into()),
            sessions: Some(sessions),
            journal,
            bus: Vec::new(),
            zone: "Asia/Ho_Chi_Minh".into(),
            wall_offset_us: None,
            out: dir.join("out"),
        }
    }

    /// A `lamp-bus record` file: faces whose passing id, extra fields and
    /// labels hold words, a voice state that is not one, and topics the
    /// export does not read.
    fn bus_fixture(dir: &Path) -> PathBuf {
        let line = |topic: &str, seconds: u64, payload: Value| {
            json!({"header": {"topic": topic, "kind": "latest", "enc": "json", "source": "eyes",
                "boot": "0123456789abcdef", "seq": seconds, "at_us": 5_000_000_000 + seconds * 1_000_000,
                "at_unix_ms": BASE_MS + seconds * 1000, "expires_ms": 2000},
                "payload_b64": base64(payload.to_string().as_bytes())})
            .to_string()
        };
        let face = json!({"id": SECRETS[12], "xmin": 0.3, "ymin": 0.2, "xmax": 0.6, "ymax": 0.6,
            "confidence": 0.9, "yaw": 0.1, "facing": true, "name": SECRETS[10], "label": SECRETS[11]});
        let lines = [
            r#"{"topic":"humans/faces/tracked","declaration":{"type":"lamp_msgs/Faces","version":1,"kind":"latest"}}"#.to_owned(),
            line("humans/faces/tracked", 1, json!({"faces": [face.clone()]})),
            line("humans/faces/tracked", 2, json!({"faces": [face, {"id": "b", "xmin": 0.1}]})),
            line("voice/state", 2, json!({"state": "listening"})),
            line("voice/state", 3, json!({"state": SECRETS[13]})),
            line("humans/names", 3, json!({"name": SECRETS[10]})),
            line(SECRETS[12], 3, json!({})),
            format!("not a recording {}", SECRETS[13]),
        ];
        let path = dir.join("bus.jsonl");
        fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }

    /// Everything the export wrote, the case files' payloads decoded.
    fn all_output(out: &Path) -> String {
        let mut text = String::new();
        for name in [
            "events.jsonl",
            "summary.json",
            "left-out.json",
            "case/voice_state.jsonl",
            "case/humans_faces_tracked.jsonl",
        ] {
            let Ok(file) = fs::read_to_string(out.join(name)) else {
                continue;
            };
            for line in file.lines() {
                let value: Value = serde_json::from_str(line).unwrap_or(Value::Null);
                if let Some(payload) = value["payload_b64"].as_str() {
                    text.push_str(&String::from_utf8(unbase64(payload).unwrap()).unwrap());
                    text.push('\n');
                }
            }
            text.push_str(&file);
        }
        text
    }

    #[test]
    fn a_session_full_of_words_exports_none_of_them() {
        let dir = temp("words");
        let (sessions, journal) = fixture(&dir.0);
        let mut words = options(&dir.0, sessions, Some(journal));
        words.bus = vec![bus_fixture(&dir.0)];
        export(&words).unwrap();
        let out = dir.0.join("out");
        assert!(out.join("case/humans_faces_tracked.jsonl").exists());
        let text = all_output(&out);
        for secret in SECRETS {
            let lower = secret.to_lowercase();
            assert!(
                !text.to_lowercase().contains(&lower),
                "`{secret}` leaked into the export:\n{text}"
            );
        }
        // Nor any word of the sentences around them.
        for word in ["what time", "It is five", "because", "has sat", "remember"] {
            assert!(!text.contains(word), "`{word}` leaked:\n{text}");
        }
    }

    #[test]
    fn what_holds_no_words_is_kept_and_counted() {
        let dir = temp("kept");
        let (sessions, journal) = fixture(&dir.0);
        let summary = export(&options(&dir.0, sessions, Some(journal))).unwrap();
        let out = dir.0.join("out");
        let events: Vec<Value> = fs::read_to_string(out.join("events.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let find = |kind: &str| events.iter().find(|e| e["kind"] == kind).unwrap().clone();
        let heard = events
            .iter()
            .find(|e| e["kind"] == "transcript" && e["who"] == "heard")
            .unwrap();
        assert_eq!(heard["chars"], 24);
        let said = events
            .iter()
            .find(|e| e["kind"] == "transcript" && e["who"] == "lamp")
            .unwrap();
        assert_eq!(said["turn"], 1);
        assert_eq!(find("input_admitted")["admission_basis"], "speech");
        assert_eq!(find("turn_finished")["outcome"], "user_interrupted");
        assert!(find("turn_finished").get("reason").is_none());
        assert_eq!(find("policy_decision")["choice"], "answer");
        assert_eq!(find("policy_decision")["probability"], 0.214);
        assert_eq!(find("policy_outcome")["overtaken"], false);
        assert_eq!(find("body_gesture")["gesture"], "nod");
        assert_eq!(find("body_face")["mood"], "Calm");
        assert_eq!(find("body_face")["down"], -0.3);
        assert_eq!(find("body_goals")["goals"][0], 2048);
        assert_eq!(find("skill_stage")["skill"], "stretch-break");
        assert_eq!(find("local_endpoint")["wall"], "17:00:03.000");
        assert_eq!(
            summary["counts"]["turn_finished.outcome=user_interrupted"],
            1
        );
        assert_eq!(summary["wall_offset_from"], "journal");
        let left: Value =
            serde_json::from_str(&fs::read_to_string(out.join("left-out.json")).unwrap()).unwrap();
        assert_eq!(left["voice_kinds_unreadable"], 1);
        assert_eq!(left["voice_lines_unreadable"], 1);
        assert_eq!(left["journal_lines_by_part"]["lamp-eyes"], 1);
        assert_eq!(left["journal_lines_by_part"]["lamp-skills"], 1);
        // `note=` and `heard=`: keys the Policy does not write, one of them a
        // single lower-case word (Second's P3 on #1134).
        assert_eq!(left["journal_pairs_by_kind"]["policy_outcome"], 2);
    }

    #[test]
    fn the_window_keeps_only_its_own_minutes_and_the_case_replays() {
        let dir = temp("window");
        let (sessions, journal) = fixture(&dir.0);
        let mut narrow = options(&dir.0, sessions, Some(journal));
        narrow.until = "17:00:05".into();
        export(&narrow).unwrap();
        let out = dir.0.join("out");
        let events = fs::read_to_string(out.join("events.jsonl")).unwrap();
        assert!(!events.contains("\"turn\":9"), "an event after the window");
        assert!(!events.contains("17:00:06"), "an event after --until");
        // The case is what `lamp-skills test` reads: a declaration, then
        // messages with headers and base64 payloads, from 0.
        let case: Vec<Value> = fs::read_to_string(out.join("case/voice_state.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(case[0]["topic"], "voice/state");
        assert_eq!(case[1]["header"]["at_us"], 0);
        assert_eq!(case[1]["payload_b64"], base64(br#"{"state":"listening"}"#));
        assert_eq!(case.len(), 4, "listening, thinking, speaking: {case:?}");
    }

    #[test]
    fn without_a_journal_a_given_offset_maps_the_voice_to_the_wall() {
        let dir = temp("offset");
        let (sessions, _) = fixture(&dir.0);
        let mut given = options(&dir.0, sessions, None);
        given.wall_offset_us = Some(OFFSET_US as i64);
        let summary = export(&given).unwrap();
        assert_eq!(summary["wall_offset_from"], "given");
        let events = fs::read_to_string(dir.0.join("out/events.jsonl")).unwrap();
        assert!(events.contains("\"wall\":\"17:00:02.000\""), "{events}");
    }

    #[test]
    fn local_times_and_dates_turn_into_the_right_instant() {
        let zone = LocalTime::zone("Asia/Ho_Chi_Minh").unwrap();
        let day = parse_date("2026-10-16").unwrap();
        assert_eq!(
            local_to_unix_ms(&zone, day, parse_clock("17:00:00").unwrap()),
            BASE_MS
        );
        assert_eq!(wall_text(&zone, BASE_MS + 1_234), "17:00:01.234");
        assert!(parse_clock("25:00:00").is_err());
        assert!(parse_date("2026-13-01").is_err());
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(
            base64(br#"{"state":"resting"}"#),
            "eyJzdGF0ZSI6InJlc3RpbmcifQ=="
        );
    }

    #[test]
    fn plain_words_are_only_lower_case_words() {
        assert!(plain_word("user_interrupted"));
        assert!(!plain_word("User"));
        assert!(!plain_word("two words"));
        assert!(!plain_word("it's"));
        assert!(!plain_word(""));
        assert!(!plain_word(&"a".repeat(49)));
    }

    #[test]
    fn faces_from_the_bus_keep_boxes_and_lose_their_ids_and_words() {
        let dir = temp("faces");
        let (sessions, _) = fixture(&dir.0);
        let mut bus = options(&dir.0, sessions, None);
        bus.wall_offset_us = Some(OFFSET_US as i64);
        bus.bus = vec![bus_fixture(&dir.0)];
        let summary = export(&bus).unwrap();
        assert_eq!(summary["faces_messages"], 2);
        assert_eq!(
            summary["faces_seen"], 1,
            "the face without a box is left out"
        );
        assert_eq!(
            summary["voice_state_from"],
            "recorded: voice/state from --bus"
        );
        let out = dir.0.join("out");
        let case: Vec<Value> = fs::read_to_string(out.join("case/humans_faces_tracked.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(case[0]["declaration"]["type"], "lamp_msgs/Faces");
        let payload: Value =
            serde_json::from_slice(&unbase64(case[1]["payload_b64"].as_str().unwrap()).unwrap())
                .unwrap();
        assert_eq!(
            payload,
            json!({"faces": [{"id": "f1", "xmin": 0.3, "ymin": 0.2, "xmax": 0.6, "ymax": 0.6,
                "confidence": 0.9, "yaw": 0.1, "facing": true}]})
        );
        assert_eq!(case[1]["header"]["expires_ms"], 2000);
        // Faces and the recorded voice state share one start: the first
        // face at 0, the state a second later.
        assert_eq!(case[1]["header"]["at_us"], 0);
        let states: Vec<Value> = fs::read_to_string(out.join("case/voice_state.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(states.len(), 2, "the declaration and `listening` only");
        assert_eq!(states[1]["header"]["at_us"], 1_000_000);
        let left: Value =
            serde_json::from_str(&fs::read_to_string(out.join("left-out.json")).unwrap()).unwrap();
        assert_eq!(left["bus_fields_by_topic"]["humans/faces/tracked"], 5);
        assert_eq!(left["bus_messages_by_topic"]["humans/names"], 1);
        assert_eq!(left["bus_messages_by_topic"]["voice/state"], 1);
        assert_eq!(left["bus_messages_by_topic"]["other"], 1);
        assert_eq!(left["bus_messages_by_topic"]["unreadable"], 1);
    }

    /// #1119's "done when": a skill's case rebuilt from `export` output alone
    /// makes what the recording it came from made. Stretch break's cases are
    /// `lamp-bus record` files: exported through `--bus`, then replayed by
    /// `lamp-skills` as its own tests are.
    #[test]
    fn stretch_breaks_cases_rebuilt_from_the_export_replay_the_same() {
        let skill = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/stretch-break");
        for case in [
            "acts-after-50-minutes",
            "across-the-room",
            "waits-for-the-talk-to-end",
        ] {
            let recording = skill.join("tests").join(case);
            let dir = temp(case);
            let files = fs::read_dir(&recording)
                .unwrap()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == "jsonl")
                })
                .collect();
            let rebuilt = ExportOptions {
                since: "00:00:00".into(),
                until: "23:59:59".into(),
                date: Some("2026-10-16".into()),
                sessions: None,
                journal: None,
                bus: files,
                zone: "Etc/UTC".into(),
                wall_offset_us: Some(0),
                out: dir.0.join("out"),
            };
            export(&rebuilt).unwrap();
            let made = |from: &Path| -> Vec<String> {
                lamp_skills::replay(&skill, from)
                    .unwrap()
                    .iter()
                    .map(|made| format!("{} {:?}", made.at_us, made.call))
                    .collect()
            };
            let original = made(&recording);
            assert_eq!(made(&dir.0.join("out/case")), original, "{case}");
            if case == "acts-after-50-minutes" {
                assert!(!original.is_empty(), "the case acts");
            }
        }
    }
}
