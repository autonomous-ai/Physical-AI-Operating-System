//! The Policy's logged attention decisions asked again of other decision
//! models (#871, `lamp-policy-replay`): the same question as on the day,
//! rebuilt from what was known ([`attention::state_text`] of the logged
//! context, the text that was sent), asked of each model in turn and made a
//! choice by the same weights ([`attention::combine`]), so that the models
//! can be laid side by side, and beside what should have happened where
//! someone has said.
//!
//! It runs where the logs are, since they hold what was heard. What it
//! gives back holds no words: for each decision its kinds (how many words,
//! whether the lamp was named in them, whether another language was heard),
//! and each model's choice, probability and time; then counts. The weights
//! were fitted to `typesafe/jev-1.13`'s answers: another model's answers go
//! through them as they are, so a model that answers on another scale is
//! judged by weights that were not fitted to it.
use crate::{
    attention::{self, Context, Room},
    decide::{Decider, Failed},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, BufRead, BufReader},
    path::Path,
    time::Duration,
};

/// One attention decision read back from the log.
#[derive(Clone, Debug, PartialEq)]
pub struct Logged {
    /// `2026-10-15#139`: the day's file and the line's number in it, from 1.
    pub id: String,
    pub at_unix_ms: u64,
    pub context: Context,
    /// What was chosen on the day, by whom.
    pub choice: String,
    pub probability: f64,
    pub by: String,
}

/// The attention decisions in `dir`'s files for `days` (`2026-10-15`), in
/// order, and how many lines could not be read as one.
pub fn read(dir: &Path, days: &[String]) -> io::Result<(Vec<Logged>, usize)> {
    let mut read = Vec::new();
    let mut unread = 0;
    for day in days {
        let path = dir.join(format!("decisions-{day}.jsonl"));
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        for (index, line) in BufReader::new(file).lines().enumerate() {
            let line = line?;
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                unread += 1;
                continue;
            };
            if value["about"] != attention::ABOUT {
                continue;
            }
            match logged(&value, format!("{day}#{}", index + 1)) {
                Some(logged) => read.push(logged),
                None => unread += 1,
            }
        }
    }
    Ok((read, unread))
}

fn logged(value: &Value, id: String) -> Option<Logged> {
    Some(Logged {
        id,
        at_unix_ms: value["at_unix_ms"].as_u64()?,
        context: serde_json::from_value(value["context"].clone()).ok()?,
        choice: value["judgement"]["choice"].as_str()?.to_owned(),
        probability: value["judgement"]["probability"].as_f64()?,
        by: value["by"].as_str().unwrap_or("rule").to_owned(),
    })
}

/// What should have happened, said by someone who knows: answered (or
/// looked up at), or left alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Should {
    Answer,
    StayQuiet,
}

impl Should {
    fn as_str(self) -> &'static str {
        match self {
            Self::Answer => "answer",
            Self::StayQuiet => "stay_quiet",
        }
    }

    /// Whether `choice` does what should have been done: `answer` and
    /// `look_up` both take the person up.
    fn met_by(self, choice: &str) -> bool {
        (choice == "stay_quiet") == (self == Self::StayQuiet)
    }
}

/// Labels, one JSON line each: `{"at_unix_ms": N, "should": "answer"}` or
/// `"stay_quiet"`, the moment as the replay's lines give it.
pub fn labels(text: &str) -> Result<BTreeMap<u64, Should>, String> {
    let mut labels = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .map_err(|_| format!("label line {} is not JSON", index + 1))?;
        let at = value["at_unix_ms"]
            .as_u64()
            .ok_or_else(|| format!("label line {} has no at_unix_ms", index + 1))?;
        let should = match value["should"].as_str() {
            Some("answer") => Should::Answer,
            Some("stay_quiet") => Should::StayQuiet,
            _ => {
                return Err(format!(
                    "label line {}: should is answer or stay_quiet",
                    index + 1
                ));
            }
        };
        // The same label twice at one moment is one label; two that
        // disagree at one moment are refused, whichever comes first (the
        // reviewer on #873): neither may become ground truth by its place.
        if let Some(earlier) = labels.insert(at, should)
            && earlier != should
        {
            return Err(format!(
                "label line {} says {} at {at}, where an earlier line says {}",
                index + 1,
                should.as_str(),
                earlier.as_str()
            ));
        }
    }
    Ok(labels)
}

/// What was heard, as kinds only: never its words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kinds {
    pub words: usize,
    /// The lamp was named in the words ([`attention::names_the_lamp`]).
    pub names_lamp: bool,
    /// Likely another language than English: a letter outside English's,
    /// or `¿` or `¡`, beyond the Vietnamese call "Lamp ơi" itself; or three
    /// words or more with none of [`ENGLISH`] among them ("Voglio mangiare
    /// sushi."). Rough, for counting apart; it decides nothing.
    pub other_language: bool,
}

/// Words nearly every English sentence said to a lamp has one of.
const ENGLISH: &[&str] = &[
    "a", "about", "all", "am", "an", "and", "are", "as", "at", "be", "but", "by", "can", "could",
    "did", "do", "does", "for", "from", "get", "go", "good", "had", "has", "have", "he", "her",
    "here", "hey", "hi", "him", "his", "how", "i", "i'm", "if", "in", "is", "it", "it's", "just",
    "know", "lamp", "let", "like", "me", "my", "no", "not", "now", "of", "ok", "okay", "on", "one",
    "or", "please", "say", "she", "so", "some", "stop", "tell", "that", "the", "their", "them",
    "then", "there", "they", "this", "time", "to", "too", "up", "us", "was", "we", "what", "when",
    "where", "which", "who", "why", "will", "with", "would", "yeah", "yes", "you", "your",
];

pub fn kinds(heard: &str) -> Kinds {
    let lowered = heard.to_lowercase().replace("lamp ơi", "lamp");
    let words: Vec<&str> = lowered
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|word| !word.is_empty())
        .collect();
    let letters = lowered
        .chars()
        .any(|c| matches!(c, '¿' | '¡') || (c.is_alphabetic() && !c.is_ascii()));
    let no_english = words.len() >= 3 && !words.iter().any(|word| ENGLISH.contains(word));
    Kinds {
        words: heard.split_whitespace().count(),
        names_lamp: attention::names_the_lamp(heard),
        other_language: letters || no_english,
    }
}

/// One model's answer to one decision.
#[derive(Clone, Debug, PartialEq)]
pub enum Replayed {
    Chose {
        /// The model as its service named it.
        by: String,
        choice: String,
        probability: f64,
        took_ms: u64,
        /// Its seven answers, by the question's name: numbers only, for
        /// fitting the weights to this model if they do not fit it.
        answers: BTreeMap<String, f64>,
    },
    Failed(&'static str),
}

/// Ask `logged`'s question of `judge`, as it was asked on the day.
pub fn ask(judge: &mut dyn Decider, logged: &Logged, deadline: Duration) -> Replayed {
    let context = &logged.context;
    let asked = judge.ask(
        &attention::state_text(context),
        &attention::questions(),
        deadline,
    );
    match asked {
        Ok(asked) => match attention::Answers::of(&asked.answers) {
            Ok(answers) => {
                let judgement = attention::combine(&answers, context);
                Replayed::Chose {
                    by: asked.model,
                    choice: judgement.choice.as_str().to_owned(),
                    probability: f64::from(judgement.probability),
                    took_ms: u64::try_from(asked.took.as_millis()).unwrap_or(u64::MAX),
                    answers: answers
                        .by_name()
                        .into_iter()
                        .map(|(name, yes)| (name, round(f64::from(yes))))
                        .collect(),
                }
            }
            Err(_) => Replayed::Failed("missing"),
        },
        Err(failed) => Replayed::Failed(Failed::kind(&failed)),
    }
}

/// The line for one decision: its kinds, what was chosen on the day, what
/// should have happened if known, and each model's answer, in the order the
/// models were given. No words.
pub fn line(logged: &Logged, should: Option<Should>, replayed: &[(String, Replayed)]) -> Value {
    let kinds = kinds(&logged.context.heard);
    let judges: Vec<Value> = replayed
        .iter()
        .map(|(asked, replayed)| match replayed {
            Replayed::Chose {
                by,
                choice,
                probability,
                took_ms,
                answers,
            } => json!({"asked": asked, "by": by, "choice": choice,
                        "probability": round(*probability), "took_ms": took_ms,
                        "answers": answers}),
            Replayed::Failed(kind) => json!({"asked": asked, "failed": kind}),
        })
        .collect();
    json!({
        "id": logged.id,
        "at_unix_ms": logged.at_unix_ms,
        "room": logged.context.room,
        "in_conversation": logged.context.in_conversation(),
        "name_heard": logged.context.name_heard.map(|_| logged.context.was_called()),
        "words": kinds.words,
        "names_lamp": kinds.names_lamp,
        "other_language": kinds.other_language,
        "logged": {"by": logged.by, "choice": logged.choice, "probability": round(logged.probability)},
        "should": should.map(Should::as_str),
        "judges": judges,
    })
}

fn round(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// An earlier run's lines (`lamp-policy-replay` output, its summary line
/// left out) with what should have happened set again from `labels`, and
/// the models in them, in their order: so a run can be labelled after it is
/// seen, and counted again without asking anything.
pub fn relabelled(
    text: &str,
    labels: &BTreeMap<u64, Should>,
) -> Result<(Vec<Value>, Vec<String>), String> {
    let mut lines = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut value: Value =
            serde_json::from_str(line).map_err(|_| format!("line {} is not JSON", index + 1))?;
        if value["status"] == "replay_summary" {
            continue;
        }
        let at = value["at_unix_ms"]
            .as_u64()
            .ok_or_else(|| format!("line {} is not a replayed decision", index + 1))?;
        value["should"] = json!(labels.get(&at).map(|should| should.as_str()));
        lines.push(value);
    }
    let models = lines
        .first()
        .and_then(|line| line["judges"].as_array())
        .map(|judges| {
            judges
                .iter()
                .filter_map(|judge| judge["asked"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    Ok((lines, models))
}

/// The counts over every line: for each model asked, how often it answered
/// and how fast (p50 and p95 of `took_ms`), what it chose, and, where what
/// should have happened is known, how often it did that: over all, for the
/// named requests, in a busy room and for another language. Then how often
/// each two models chose alike.
pub fn summary(lines: &[Value], models: &[String]) -> Value {
    let mut each = serde_json::Map::new();
    for (index, model) in models.iter().enumerate() {
        let of = |line: &Value| line["judges"].get(index).cloned().unwrap_or(Value::Null);
        let mut took: Vec<u64> = lines
            .iter()
            .filter_map(|line| of(line)["took_ms"].as_u64())
            .collect();
        took.sort_unstable();
        let mut failed = BTreeMap::new();
        let mut chose = BTreeMap::new();
        for line in lines {
            let answer = of(line);
            if let Some(kind) = answer["failed"].as_str() {
                *failed.entry(kind.to_owned()).or_insert(0_u64) += 1;
            }
            if let Some(choice) = answer["choice"].as_str() {
                *chose.entry(choice.to_owned()).or_insert(0_u64) += 1;
            }
        }
        let right = |keep: &dyn Fn(&Value) -> bool| {
            let labelled: Vec<&Value> = lines
                .iter()
                .filter(|line| keep(line) && line["should"].is_string())
                .filter(|line| of(line)["choice"].is_string())
                .collect();
            let met = labelled
                .iter()
                .filter(|line| {
                    let should = if line["should"] == "stay_quiet" {
                        Should::StayQuiet
                    } else {
                        Should::Answer
                    };
                    should.met_by(of(line)["choice"].as_str().unwrap_or_default())
                })
                .count();
            json!({"labelled": labelled.len(), "right": met})
        };
        let same_as_logged = lines
            .iter()
            .filter(|line| line["logged"]["by"] != "rule")
            .filter(|line| of(line)["choice"].is_string())
            .fold((0, 0), |(n, same), line| {
                (
                    n + 1,
                    same + usize::from(of(line)["choice"] == line["logged"]["choice"]),
                )
            });
        each.insert(
            model.clone(),
            json!({
                "asked": lines.len(),
                "answered": took.len(),
                "failed": failed,
                "took_ms": {"p50": percentile(&took, 50), "p95": percentile(&took, 95), "max": took.last()},
                "chose": chose,
                "right": right(&|_| true),
                "right_named": right(&|line| line["names_lamp"] == true),
                "right_busy": right(&|line| line["room"] == json!(Room::Busy)),
                "right_other_language": right(&|line| line["other_language"] == true),
                "same_as_logged": {"n": same_as_logged.0, "same": same_as_logged.1},
            }),
        );
    }
    let mut pairs = Vec::new();
    for first in 0..models.len() {
        for second in first + 1..models.len() {
            let both: Vec<(&Value, &Value)> = lines
                .iter()
                .map(|line| {
                    (
                        &line["judges"][first]["choice"],
                        &line["judges"][second]["choice"],
                    )
                })
                .filter(|(a, b)| a.is_string() && b.is_string())
                .collect();
            let same = both.iter().filter(|(a, b)| a == b).count();
            let same_side = both
                .iter()
                .filter(|(a, b)| (*a == "stay_quiet") == (*b == "stay_quiet"))
                .count();
            pairs.push(
                json!({"models": [models[first], models[second]], "both_answered": both.len(),
                              "same_choice": same, "same_side": same_side}),
            );
        }
    }
    json!({
        "status": "replay_summary",
        "decisions": lines.len(),
        "models": Value::Object(each),
        "pairs": pairs,
        "words": "none: kinds and counts only",
        "weights": "fitted to typesafe/jev-1.13's answers on 2026-10-13; every model's answers go through them",
    })
}

/// The lines of `pictures-YYYY-MM-DD.jsonl` for `days` (`lamp-policy
/// --images`, #871), in order.
pub fn read_pictures(dir: &Path, days: &[String]) -> io::Result<Vec<Value>> {
    let mut read = Vec::new();
    for day in days {
        let text = match fs::read_to_string(dir.join(format!("pictures-{day}.jsonl"))) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        read.extend(
            text.lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .filter(|line| line["about"] == "attention_picture"),
        );
    }
    Ok(read)
}

/// How far a label's moment may be from a picture line's: the line is
/// written at the moment the question was sent, and a label is written by a
/// scene's clock.
pub const LABEL_WITHIN_MS: u64 = 3_000;

/// Each label given to the one line nearest it within [`LABEL_WITHIN_MS`],
/// whatever that line is, a picture sent or not (the reviewer on #873): a
/// label is never given to more than one line, so a decision nobody labelled
/// is never taken for one somebody did. A label equally near two lines is
/// given to neither; two labels given to one line count only if they agree.
/// The labels per line, in order, and how they were matched.
fn assign(lines: &[Value], labels: &BTreeMap<u64, Should>) -> (Vec<Option<Should>>, Value) {
    let moments: Vec<Option<u64>> = lines
        .iter()
        .map(|line| line["at_unix_ms"].as_u64())
        .collect();
    let mut given: Vec<Vec<Should>> = vec![Vec::new(); lines.len()];
    let (mut unmatched, mut ambiguous) = (0_usize, 0_usize);
    for (at, should) in labels {
        let near: Vec<(usize, u64)> = moments
            .iter()
            .enumerate()
            .filter_map(|(index, moment)| Some((index, (*moment)?.abs_diff(*at))))
            .filter(|(_, distance)| *distance <= LABEL_WITHIN_MS)
            .collect();
        let Some(nearest) = near.iter().map(|(_, distance)| *distance).min() else {
            unmatched += 1;
            continue;
        };
        let at_nearest: Vec<usize> = near
            .iter()
            .filter(|(_, distance)| *distance == nearest)
            .map(|(index, _)| *index)
            .collect();
        match at_nearest.as_slice() {
            [one] => given[*one].push(*should),
            _ => ambiguous += 1,
        }
    }
    let mut conflicting = 0;
    let assigned: Vec<Option<Should>> = given
        .iter()
        .map(|labels| match labels.split_first() {
            Some((first, rest)) if rest.iter().all(|other| other == first) => Some(*first),
            Some(_) => {
                conflicting += labels.len();
                None
            }
            None => None,
        })
        .collect();
    let matched = given.iter().map(Vec::len).sum::<usize>() - conflicting;
    (
        assigned,
        json!({"given": labels.len(), "matched": matched, "unmatched": unmatched,
               "ambiguous": ambiguous, "conflicting": conflicting}),
    )
}

/// The counts over the picture lines, with no words: how often a frame was
/// sent and why not; for the asking without the picture and with it, how
/// often each answered, how fast and how often it did what should have
/// happened; and for each picture question, and for the eyes' own count of
/// faces turned to the lamp, how often its yes would have been right. The
/// labels are matched by [`assign`].
pub fn pictures_summary(lines: &[Value], labels: &BTreeMap<u64, Should>) -> Value {
    let (assigned, matching) = assign(lines, labels);
    let mut why_not = BTreeMap::new();
    for line in lines {
        if let Some(why) = line["picture"]["why_not"].as_str() {
            *why_not.entry(why.to_owned()).or_insert(0_u64) += 1;
        }
    }
    let sent: Vec<(&Value, Option<Should>)> = lines
        .iter()
        .zip(assigned.iter().copied())
        .filter(|(line, _)| line["picture"]["sent"] == true)
        .collect();
    let asking = |which: &str| {
        let mut took: Vec<u64> = sent
            .iter()
            .filter_map(|(line, _)| line[which]["took_ms"].as_u64())
            .collect();
        took.sort_unstable();
        let labelled: Vec<(Should, &str)> = sent
            .iter()
            .filter_map(|(line, should)| Some(((*should)?, line[which]["choice"].as_str()?)))
            .collect();
        let right = labelled
            .iter()
            .filter(|(should, choice)| should.met_by(choice))
            .count();
        json!({
            "answered": took.len(),
            "took_ms": {"p50": percentile(&took, 50), "p95": percentile(&took, 95), "max": took.last()},
            "right": {"labelled": labelled.len(), "right": right},
        })
    };
    // A sign is right when its yes goes with `yes_means`.
    let sign = |yes: &dyn Fn(&Value) -> Option<bool>, yes_means: Should| {
        let judged: Vec<(Should, bool)> = sent
            .iter()
            .filter_map(|(line, should)| Some(((*should)?, yes(line)?)))
            .collect();
        let right = judged
            .iter()
            .filter(|(should, yes)| *yes == (*should == yes_means))
            .count();
        json!({"labelled": judged.len(), "right": right,
               "yes": judged.iter().filter(|(_, yes)| *yes).count()})
    };
    let mut signs = serde_json::Map::new();
    for (name, means) in [
        ("picture_faces_lamp", Should::Answer),
        ("picture_asks_with_a_look", Should::Answer),
        ("picture_absorbed", Should::StayQuiet),
        ("picture_talking_to_another", Should::StayQuiet),
        ("picture_away", Should::StayQuiet),
    ] {
        let yes = |line: &Value| line["with"]["answers"][name].as_f64().map(|yes| yes >= 0.5);
        signs.insert(name.to_owned(), sign(&yes, means));
    }
    let facing = |line: &Value| line["seen"]["facing"].as_u64().map(|facing| facing > 0);
    signs.insert("eyes_facing".to_owned(), sign(&facing, Should::Answer));
    json!({
        "status": "pictures_summary",
        "questions": lines.len(),
        "picture_sent": sent.len(),
        "why_not": why_not,
        "without": asking("without"),
        "with": asking("with"),
        "signs": signs,
        "label_within_ms": LABEL_WITHIN_MS,
        "labels": matching,
        "words": "none: kinds and counts only",
    })
}

/// The `percent`th percentile of sorted `values`, by the nearest rank.
fn percentile(values: &[u64], percent: usize) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let rank = (percent * values.len()).div_ceil(100).max(1);
    values.get(rank - 1).copied()
}

#[cfg(test)]
mod tests;
