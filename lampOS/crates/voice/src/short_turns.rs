//! How often a short sound the lamp heard got an answer, from what the voice
//! already writes down (`lamp-voice short-turns SESSIONS`, #800).
//!
//! On lamp-52e6 on 2026-10-15 at 13:03 a room sound of about 0.7 s was
//! admitted, ended by the silence fallback, transcribed as a one-word
//! question in Spanish, and answered 1.9 s later; nobody had spoken to the
//! lamp. With `LAMP_SHORT_SOUNDS=quiet-room` an utterance of fewer than
//! [`crate::not_speech::SCRAP_BLOCKS`] speech blocks is sent on as a sound
//! when the room is not busy, so that a real short word ("yes", "stop") is
//! not thrown away.
//!
//! This reads the session records (`<session>/metadata/events-*.jsonl`)
//! and prints one line for every short turn: one let through as a sound,
//! one thrown out as a sound, and any other under
//! [`crate::not_speech::SHORT_BLOCKS`] speech blocks. Each line has how
//! loud and how long it was, how its end was decided, whether an answer
//! reached the speaker, and what was heard, as kinds only: how many words,
//! whether it was one of the short things people say to the lamp in
//! English, and whether it had the marks of another language. Never the
//! words themselves.
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

/// Short things a person says to the lamp, in English as it is spoken to:
/// a heard text that is one of these, whole, is a word, not a sound.
pub const SHORT_WORDS: &[&str] = &[
    "yes",
    "yeah",
    "yep",
    "yup",
    "no",
    "nope",
    "stop",
    "what",
    "okay",
    "ok",
    "sure",
    "go ahead",
    "yes go ahead",
    "go on",
    "thanks",
    "thank you",
    "hello",
    "hi",
    "hey",
    "lamp",
    "hey lamp",
    "please",
    "wait",
    "right",
    "cool",
    "great",
    "nice",
    "again",
    "sorry",
    "pardon",
    "huh",
    "really",
    "why",
    "how",
    "when",
    "who",
    "where",
    "done",
    "louder",
    "quieter",
    "good",
    "fine",
    "never mind",
    "nevermind",
    "bye",
    "goodbye",
    "good night",
    "morning",
    "good morning",
    "do it",
    "start",
    "cancel",
    "shush",
    "quiet",
    "be quiet",
    "enough",
    "that's it",
    "that's all",
    "all right",
    "alright",
];

/// What was heard, as kinds: no word of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Heard {
    pub words: usize,
    /// It is one of [`SHORT_WORDS`], whole.
    pub short_word: bool,
    /// It has marks English does not: `¿`, `¡`, or a letter with an accent.
    pub other_marks: bool,
}

/// The kinds of a heard text.
pub fn heard(text: &str) -> Heard {
    let other_marks = text
        .chars()
        .any(|c| c == '¿' || c == '¡' || (c.is_alphabetic() && !c.is_ascii()));
    let plain: String = text
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect();
    let words: Vec<&str> = plain.split_whitespace().collect();
    Heard {
        words: words.len(),
        short_word: !words.is_empty() && SHORT_WORDS.contains(&words.join(" ").as_str()),
        other_marks,
    }
}

/// Every JSON line of the `events-*.jsonl` files under `root`, at any
/// depth, with the folder each was in; a line that is not JSON is passed
/// over.
pub fn records_under(root: &Path) -> std::io::Result<Vec<(std::path::PathBuf, Value)>> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_owned()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                folders.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("events-") && name.ends_with(".jsonl"))
            {
                found.extend(
                    std::fs::read_to_string(&path)?
                        .lines()
                        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                        .map(|value| (folder.clone(), value)),
                );
            }
        }
    }
    Ok(found)
}

/// One short turn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Short {
    pub turn: u64,
    /// `let_through` (sent on as a sound), `thrown_out` (dropped as one) or
    /// `taken_as_said`.
    pub how: &'static str,
    pub speech_blocks: usize,
    pub median_level: Option<u64>,
    pub upper_tenth_level: Option<u64>,
    pub ended_by: Option<String>,
    pub heard: Option<Heard>,
    pub answered_aloud: bool,
    pub outcome: Option<String>,
}

impl Short {
    pub fn row(&self) -> Value {
        json!({
            "turn": self.turn,
            "how": self.how,
            "speech_ms": self.speech_blocks * 10,
            "median_level": self.median_level,
            "upper_tenth_level": self.upper_tenth_level,
            "ended_by": self.ended_by,
            "heard_words": self.heard.as_ref().map(|heard| heard.words),
            "heard_a_short_word": self.heard.as_ref().map(|heard| heard.short_word),
            "heard_other_marks": self.heard.as_ref().map(|heard| heard.other_marks),
            "answered_aloud": self.answered_aloud,
            "outcome": self.outcome,
        })
    }
}

/// The short turns in the records of one or more runs. A heard text (an
/// input transcript, which the record keeps for the session, not the turn)
/// belongs to the turn admitted last before it.
pub fn short_turns(records: &[(std::path::PathBuf, Value)]) -> Vec<Short> {
    let mut by_run: BTreeMap<&Path, Vec<&Value>> = BTreeMap::new();
    for (folder, record) in records {
        by_run.entry(folder.as_path()).or_default().push(record);
    }
    let mut found = Vec::new();
    for records in by_run.into_values() {
        let mut records = records;
        records.sort_by_key(|record| record["at_us"].as_u64().unwrap_or(0));
        // The turn admitted last, and the texts heard since.
        let mut admitted: Option<u64> = None;
        let mut shorts: BTreeMap<u64, Short> = BTreeMap::new();
        let mut texts: BTreeMap<u64, String> = BTreeMap::new();
        for record in &records {
            // Records without a time are not of a turn.
            if record["at_us"].as_u64().is_none() {
                continue;
            }
            let turn = record["turn"].as_u64();
            match record["kind"].as_str() {
                Some("input_admitted") => admitted = turn.or(admitted),
                Some("local_endpoint") => {
                    let Some(turn) = turn else { continue };
                    let ended_by = record["endpoint"]["basis"].as_str().map(|basis| {
                        match record["endpoint"]["fallback"].as_str() {
                            Some(fallback) => format!("{basis}:{fallback}"),
                            None => basis.to_owned(),
                        }
                    });
                    let short = if let Some(blocks) = record["not_speech"]["speech_blocks"].as_u64()
                    {
                        Some(Short {
                            turn,
                            how: "thrown_out",
                            speech_blocks: blocks as usize,
                            ended_by,
                            ..Short::default()
                        })
                    } else {
                        let blocks =
                            record["speech_level"]["blocks"].as_u64().unwrap_or(0) as usize;
                        let let_through = record["let_through_as_sound"] == json!(true);
                        (let_through || blocks < crate::not_speech::SHORT_BLOCKS).then(|| Short {
                            turn,
                            how: if let_through {
                                "let_through"
                            } else {
                                "taken_as_said"
                            },
                            speech_blocks: blocks,
                            median_level: record["speech_level"]["median"].as_u64(),
                            upper_tenth_level: record["speech_level"]["upper_tenth"].as_u64(),
                            ended_by,
                            ..Short::default()
                        })
                    };
                    if let Some(short) = short {
                        shorts.insert(turn, short);
                    }
                }
                Some("transcript") if turn.is_none() => {
                    // Heard, not said: the session's input transcription.
                    if let (Some(turn), Some(text)) = (admitted, record["text"].as_str()) {
                        texts.entry(turn).or_default().push_str(text);
                    }
                }
                Some("speaker_first_write") => {
                    if let Some(short) = turn.and_then(|turn| shorts.get_mut(&turn)) {
                        short.answered_aloud = true;
                    }
                }
                Some("turn_finished") => {
                    if let Some(short) = turn.and_then(|turn| shorts.get_mut(&turn)) {
                        short.outcome = record["outcome"].as_str().map(str::to_owned);
                    }
                }
                _ => {}
            }
        }
        for (turn, short) in &mut shorts {
            short.heard = texts.get(turn).map(|text| heard(text));
        }
        found.extend(shorts.into_values());
    }
    found
}

/// Counts over the short turns: how many of each kind were answered, and
/// how many answered let-through sounds were heard as one of the short
/// words, with other marks, or as nothing.
pub fn summary(shorts: &[Short]) -> Value {
    let count = |how: &str, test: &dyn Fn(&Short) -> bool| {
        shorts
            .iter()
            .filter(|short| short.how == how && test(short))
            .count()
    };
    let answered = |short: &Short| short.answered_aloud;
    json!({
        "status": "short_turns_summary",
        "let_through": count("let_through", &|_| true),
        "let_through_answered": count("let_through", &answered),
        "let_through_answered_heard_a_short_word": count("let_through", &|short| answered(short) && short.heard.as_ref().is_some_and(|heard| heard.short_word)),
        "let_through_answered_heard_other_marks": count("let_through", &|short| answered(short) && short.heard.as_ref().is_some_and(|heard| heard.other_marks)),
        "let_through_answered_heard_nothing": count("let_through", &|short| answered(short) && short.heard.as_ref().is_none_or(|heard| heard.words == 0)),
        "thrown_out": count("thrown_out", &|_| true),
        "taken_as_said": count("taken_as_said", &|_| true),
        "taken_as_said_answered": count("taken_as_said", &answered),
        "boundary": "the session records only; a heard text is joined to the turn admitted last before it; no words printed",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_heard_is_kinds_only_and_a_short_word_is_whole() {
        assert_eq!(
            heard("¿Qué?"),
            Heard {
                words: 1,
                short_word: false,
                other_marks: true
            }
        );
        assert_eq!(
            heard("Yes, go ahead."),
            Heard {
                words: 3,
                short_word: true,
                other_marks: false
            }
        );
        assert!(heard(" What? ").short_word);
        assert!(heard("Stop.").short_word);
        assert!(!heard("Stop the music").short_word);
        assert!(!heard("Claro.").short_word);
        assert!(heard("Hola.").words == 1 && !heard("Hola.").other_marks);
        assert_eq!(heard("").words, 0);
        assert!(heard("albero").words == 1 && !heard("albero").short_word);
    }

    fn at(folder: &str, record: Value) -> (std::path::PathBuf, Value) {
        (std::path::PathBuf::from(folder), record)
    }

    #[test]
    fn the_short_turns_are_found_with_what_was_heard_and_whether_answered() {
        let records = vec![
            // A sound let through, heard as a word in Spanish, answered.
            at(
                "/r/one",
                json!({"kind":"input_admitted","turn":6,"at_us":9_000_000}),
            ),
            at(
                "/r/one",
                json!({"kind":"local_endpoint","turn":6,"at_us":10_000_000,"let_through_as_sound":true,
                "endpoint":{"basis":"silence_fallback","fallback":"short"},"speech_level":{"blocks":22,"median":90,"upper_tenth":140}}),
            ),
            at(
                "/r/one",
                json!({"kind":"transcript","turn":null,"text":"¿Qué?","finished":true,"at_us":10_300_000}),
            ),
            at(
                "/r/one",
                json!({"kind":"speaker_first_write","turn":6,"at_us":11_900_000}),
            ),
            at(
                "/r/one",
                json!({"kind":"turn_finished","turn":6,"outcome":"audio_written_unscored","at_us":14_000_000}),
            ),
            // A sound thrown out.
            at(
                "/r/one",
                json!({"kind":"input_admitted","turn":7,"at_us":19_500_000}),
            ),
            at(
                "/r/one",
                json!({"kind":"local_endpoint","turn":7,"at_us":20_000_000,"not_speech":{"speech_blocks":9,"fewer_than":30}}),
            ),
            at(
                "/r/one",
                json!({"kind":"turn_finished","turn":7,"outcome":"not_speech","at_us":20_000_100}),
            ),
            // A short "Yes." taken as said, answered; and a long question, not short.
            at(
                "/r/one",
                json!({"kind":"input_admitted","turn":8,"at_us":29_400_000}),
            ),
            // Heard while it was still being said, before its end.
            at(
                "/r/one",
                json!({"kind":"transcript","turn":null,"text":"Yes.","finished":true,"at_us":29_800_000}),
            ),
            at(
                "/r/one",
                json!({"kind":"local_endpoint","turn":8,"at_us":30_000_000,"speech_level":{"blocks":45,"median":120,"upper_tenth":160}}),
            ),
            at(
                "/r/one",
                json!({"kind":"speaker_first_write","turn":8,"at_us":31_000_000}),
            ),
            at(
                "/r/one",
                json!({"kind":"input_admitted","turn":9,"at_us":37_000_000}),
            ),
            at(
                "/r/one",
                json!({"kind":"local_endpoint","turn":9,"at_us":40_000_000,"speech_level":{"blocks":300,"median":120,"upper_tenth":160}}),
            ),
            // What the lamp said is not heard.
            at(
                "/r/one",
                json!({"kind":"transcript","turn":9,"text":"Your code is 417.","finished":true,"at_us":40_500_000}),
            ),
        ];
        let shorts = short_turns(&records);
        assert_eq!(
            shorts
                .iter()
                .map(|short| (short.turn, short.how))
                .collect::<Vec<_>>(),
            vec![(6, "let_through"), (7, "thrown_out"), (8, "taken_as_said")]
        );
        let row = shorts[0].row();
        assert_eq!(row["speech_ms"], 220);
        assert_eq!(row["ended_by"], "silence_fallback:short");
        assert_eq!(
            (
                row["heard_words"].clone(),
                row["heard_a_short_word"].clone(),
                row["heard_other_marks"].clone()
            ),
            (json!(1), json!(false), json!(true))
        );
        assert_eq!(row["answered_aloud"], true);
        assert!(!row.to_string().contains("Qué"));
        assert_eq!(shorts[1].outcome.as_deref(), Some("not_speech"));
        assert!(
            shorts[2]
                .heard
                .as_ref()
                .is_some_and(|heard| heard.short_word)
        );
        let summary = summary(&shorts);
        assert_eq!(summary["let_through_answered"], 1);
        assert_eq!(summary["let_through_answered_heard_other_marks"], 1);
        assert_eq!(summary["let_through_answered_heard_a_short_word"], 0);
        assert_eq!(summary["thrown_out"], 1);
        assert_eq!(summary["taken_as_said_answered"], 1);
        assert!(!summary.to_string().contains("417"));
    }
}
