//! Where each of the lamp's turns spent its time, from what the voice
//! already writes down (`lamp-voice turn-times SESSIONS [FIRST_OUTPUT]`,
//! #799): for every request, from the moment the lamp decided the person had
//! finished, when the model's first output came and what it was, when a
//! function was called and our answer sent, when the model's first audio
//! came, and when the first sound was handed to the speaker. Then medians
//! for turns with a function call and for plain ones.
//!
//! It reads the session records (`<session>/metadata/events-*.jsonl`) and,
//! if given, the provider worker's first-output files
//! (`first-output-*.jsonl`, `LAMP_FIRST_OUTPUT`). Every time in both is the
//! lamp's monotonic clock in microseconds. A request number is the
//! coordinator's turn; turns are counted afresh each time the voice starts,
//! so a session folder is one run, and a first-output line is matched to a
//! turn of the same number whose end of speech was decided at most a minute
//! before it. What it prints is numbers and function names: no words that
//! were heard or said.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Calls after which the model says nothing more: they tell whom it heard,
/// not something to do, and do not make a turn one with a function call.
const SIGNALS: &[&str] = &[crate::tools::NOT_FOR_ME, crate::attention::HEARD_MY_NAME];
/// A first-output line belongs to a turn whose end of speech was decided at
/// most this long before it.
const MATCH_WITHIN_US: u64 = 60_000_000;

/// Every JSON line of the files under `root`, at any depth, whose names
/// begin with `prefix` and end `.jsonl`, each with the folder it was in.
/// A line that is not JSON is passed over.
pub fn lines_under(root: &Path, prefix: &str) -> std::io::Result<Vec<(PathBuf, Value)>> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_owned()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                folders.push(path);
            } else if kind.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(prefix) && name.ends_with(".jsonl"))
            {
                let text = std::fs::read_to_string(&path)?;
                found.extend(
                    text.lines()
                        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                        .map(|value| (folder.clone(), value)),
                );
            }
        }
    }
    Ok(found)
}

/// One function call and our answer, as the first-output file says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub name: String,
    pub called_us: u64,
    pub sent_us: u64,
}

/// One request, its times on the lamp's monotonic clock.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Turn {
    pub run: PathBuf,
    pub turn: u64,
    pub endpoint_us: u64,
    pub first_output: Option<(String, u64)>,
    pub calls: Vec<Call>,
    pub first_audio_us: Option<u64>,
    pub first_sound_us: Option<u64>,
    pub outcome: Option<String>,
}

impl Turn {
    /// A turn whose model called something to be done, not only a signal.
    pub fn with_function(&self) -> bool {
        self.calls
            .iter()
            .any(|call| !SIGNALS.contains(&call.name.as_str()))
            || self
                .first_output
                .as_ref()
                .is_some_and(|(trigger, _)| trigger == "tool_call")
    }

    fn after(&self, at_us: Option<u64>) -> Option<f64> {
        at_us
            .filter(|at| *at >= self.endpoint_us)
            .map(|at| (at - self.endpoint_us) as f64 / 1_000.0)
    }

    /// The row printed for it, every time in milliseconds after the end of
    /// speech was decided.
    pub fn row(&self) -> Value {
        let call = self
            .calls
            .iter()
            .find(|call| !SIGNALS.contains(&call.name.as_str()));
        json!({
            "turn": self.turn,
            "kind": if self.with_function() { "function" } else { "plain" },
            "functions": self.calls.iter().map(|call| call.name.as_str()).collect::<Vec<_>>(),
            "first_output": self.first_output.as_ref().map(|(trigger, _)| trigger.as_str()),
            "first_output_ms": self.after(self.first_output.as_ref().map(|(_, at)| *at)),
            "call_ms": self.after(call.map(|call| call.called_us)),
            "answer_sent_ms": self.after(call.map(|call| call.sent_us)),
            "first_audio_ms": self.after(self.first_audio_us),
            "first_sound_ms": self.after(self.first_sound_us),
            "outcome": self.outcome,
        })
    }
}

/// The turns in the session records `events`, with the first-output lines
/// `first` matched to them.
pub fn turns(events: &[(PathBuf, Value)], first: &[(PathBuf, Value)]) -> Vec<Turn> {
    let mut by_turn: BTreeMap<(PathBuf, u64), Turn> = BTreeMap::new();
    let number = |value: &Value, field: &str| value[field].as_u64();
    for (folder, event) in events {
        let Some(turn) = number(event, "turn") else {
            continue;
        };
        let at = number(event, "at_us");
        let key = (folder.clone(), turn);
        match event["kind"].as_str() {
            Some("local_endpoint") => {
                if let Some(at) = at {
                    let entry = by_turn.entry(key).or_insert_with(|| Turn {
                        run: folder.clone(),
                        turn,
                        endpoint_us: at,
                        ..Turn::default()
                    });
                    entry.endpoint_us = entry.endpoint_us.min(at);
                }
            }
            Some("provider_first_audio") => {
                if let (Some(entry), Some(at)) = (by_turn.get_mut(&key), at) {
                    entry.first_audio_us.get_or_insert(at);
                }
            }
            Some("speaker_first_write") => {
                if let (Some(entry), Some(at)) = (by_turn.get_mut(&key), at) {
                    entry.first_sound_us.get_or_insert(at);
                }
            }
            Some("turn_finished") => {
                if let Some(entry) = by_turn.get_mut(&key) {
                    entry.outcome = event["outcome"].as_str().map(str::to_owned);
                }
            }
            _ => {}
        }
    }
    let mut turns: Vec<Turn> = by_turn.into_values().collect();
    // Each first-output line goes to the turn of its number decided most
    // recently before it, within a minute.
    for (_, line) in first {
        let Some(request) = number(line, "request") else {
            continue;
        };
        let (at, call) = match line["event"].as_str() {
            Some("voice_first_output") => (number(line, "sampled_start_us"), None),
            Some("voice_tool_call") => {
                let (Some(called_us), Some(sent_us), Some(name)) = (
                    number(line, "called_us"),
                    number(line, "sent_us"),
                    line["name"].as_str(),
                ) else {
                    continue;
                };
                (
                    Some(called_us),
                    Some(Call {
                        name: name.to_owned(),
                        called_us,
                        sent_us,
                    }),
                )
            }
            _ => continue,
        };
        let Some(at) = at else { continue };
        let Some(turn) = turns
            .iter_mut()
            .filter(|turn| {
                turn.turn == request
                    && turn.endpoint_us <= at
                    && at - turn.endpoint_us <= MATCH_WITHIN_US
            })
            .max_by_key(|turn| turn.endpoint_us)
        else {
            continue;
        };
        match call {
            Some(call) => turn.calls.push(call),
            None => {
                if let Some(trigger) = line["trigger"].as_str() {
                    turn.first_output.get_or_insert((trigger.to_owned(), at));
                }
            }
        }
    }
    turns
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len() % 2 == 1 {
        values[middle]
    } else {
        (values[middle - 1] + values[middle]) / 2.0
    })
}

/// Medians for turns with a function call and for plain ones, of the turns
/// that reached the speaker; each step of a function turn, too.
pub fn summary(turns: &[Turn]) -> Value {
    let rows: Vec<Value> = turns.iter().map(Turn::row).collect();
    let of = |kind: &str, field: &str| {
        median(
            rows.iter()
                .filter(|row| row["kind"] == kind && !row["first_sound_ms"].is_null())
                .filter_map(|row| row[field].as_f64())
                .collect(),
        )
    };
    let between = |from: &str, to: &str| {
        median(
            rows.iter()
                .filter(|row| row["kind"] == "function" && !row["first_sound_ms"].is_null())
                .filter_map(|row| Some(row[to].as_f64()? - row[from].as_f64()?))
                .collect(),
        )
    };
    let count = |kind: &str| {
        rows.iter()
            .filter(|row| row["kind"] == kind && !row["first_sound_ms"].is_null())
            .count()
    };
    json!({
        "status": "turn_times_summary",
        "turns": rows.len(),
        "plain": {
            "n": count("plain"),
            "first_output_ms": of("plain", "first_output_ms"),
            "first_audio_ms": of("plain", "first_audio_ms"),
            "first_sound_ms": of("plain", "first_sound_ms"),
        },
        "function": {
            "n": count("function"),
            "call_ms": of("function", "call_ms"),
            "answer_sent_after_call_ms": between("call_ms", "answer_sent_ms"),
            "first_audio_after_answer_ms": between("answer_sent_ms", "first_audio_ms"),
            "first_audio_ms": of("function", "first_audio_ms"),
            "first_sound_ms": of("function", "first_sound_ms"),
        },
        "boundary": "from the voice's end-of-speech decision (local_endpoint) to the first write the speaker accepted (speaker_first_write); acoustic onset unmeasured; calls only where a first-output file has voice_tool_call lines",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: &str, turn: u64, at_us: u64) -> (PathBuf, Value) {
        (
            PathBuf::from("/s/one/metadata"),
            json!({"kind": kind, "turn": turn, "at_us": at_us}),
        )
    }
    fn first(request: u64, trigger: &str, at_us: u64) -> (PathBuf, Value) {
        (
            PathBuf::from("/f"),
            json!({"event":"voice_first_output","request":request,"trigger":trigger,"sampled_start_us":at_us}),
        )
    }
    fn call(request: u64, name: &str, called_us: u64, sent_us: u64) -> (PathBuf, Value) {
        (
            PathBuf::from("/f"),
            json!({"event":"voice_tool_call","request":request,"name":name,"called_us":called_us,"sent_us":sent_us}),
        )
    }

    #[test]
    fn each_turn_is_timed_from_its_end_of_speech_and_a_call_makes_it_a_function_turn() {
        let events = vec![
            // A plain answer: 0.9 s to the model's audio, 1.25 s to the speaker.
            event("local_endpoint", 4, 10_000_000),
            event("provider_first_audio", 4, 10_900_000),
            event("speaker_first_write", 4, 11_250_000),
            event("turn_finished", 4, 13_000_000),
            // A turn with a call: the call at 1.0 s, the answer 45 ms after,
            // the audio 0.95 s after the answer, the sound 0.35 s after.
            event("local_endpoint", 5, 20_000_000),
            event("provider_first_audio", 5, 21_995_000),
            event("speaker_first_write", 5, 22_345_000),
            // A turn the model said was not for it: a signal, not a function.
            event("local_endpoint", 6, 30_000_000),
        ];
        let lines = vec![
            first(4, "audio", 10_850_000),
            first(5, "tool_call", 21_000_000),
            call(5, "confirm_agent_task", 21_000_000, 21_045_000),
            call(6, "not_for_me", 30_800_000, 30_800_100),
            // The same number from a run long after: not this turn's.
            first(4, "tool_call", 500_000_000),
        ];
        let turns = turns(&events, &lines);
        assert_eq!(turns.len(), 3);
        let plain = turns[0].row();
        assert_eq!(
            (plain["kind"].clone(), plain["first_output"].clone()),
            (json!("plain"), json!("audio"))
        );
        assert_eq!(plain["first_audio_ms"], 900.0);
        assert_eq!(plain["first_sound_ms"], 1_250.0);
        let function = turns[1].row();
        assert_eq!(function["kind"], "function");
        assert_eq!(function["functions"], json!(["confirm_agent_task"]));
        assert_eq!(
            (
                function["call_ms"].clone(),
                function["answer_sent_ms"].clone()
            ),
            (json!(1_000.0), json!(1_045.0))
        );
        assert_eq!(function["first_sound_ms"], 2_345.0);
        assert_eq!(turns[2].row()["kind"], "plain");
        let summary = summary(&turns);
        assert_eq!(summary["plain"]["n"], 1);
        assert_eq!(summary["plain"]["first_sound_ms"], 1_250.0);
        assert_eq!(summary["function"]["n"], 1);
        assert_eq!(summary["function"]["answer_sent_after_call_ms"], 45.0);
        assert_eq!(summary["function"]["first_audio_after_answer_ms"], 950.0);
        assert_eq!(summary["function"]["first_sound_ms"], 2_345.0);
        // Numbers and names only: no words.
        let text = summary.to_string();
        assert!(!text.contains("said") && !text.contains("text"));
    }

    #[test]
    fn records_are_read_from_every_folder_under_the_root_and_broken_lines_are_passed_over() {
        let root = std::env::temp_dir().join(format!("turn-times-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let metadata = root.join("run-1/metadata");
        std::fs::create_dir_all(&metadata).unwrap();
        std::fs::write(
            metadata.join("events-0.jsonl"),
            "{\"kind\":\"local_endpoint\",\"turn\":1,\"at_us\":5}\nnot json\n",
        )
        .unwrap();
        std::fs::write(metadata.join("health.json"), "{}").unwrap();
        std::fs::write(
            root.join("first-output-1.jsonl"),
            "{\"event\":\"capture_start\"}\n",
        )
        .unwrap();
        let events = lines_under(&root, "events-").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, metadata);
        assert_eq!(lines_under(&root, "first-output-").unwrap().len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
