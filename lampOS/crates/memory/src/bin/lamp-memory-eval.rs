//! Offline, retrieval-only LongMemEval evaluation. No model, credentials,
//! network, live memory directory, or lamp is used. See `--help` for the
//! distinction between this untruncated benchmark and the live service.

use lamp_memory::search::{self, SearchIndex};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{self, BufWriter, Write},
    path::PathBuf,
    process::ExitCode,
    time::Instant,
};

const DEPTHS: [usize; 3] = [1, 5, 10];
const USAGE: &str = "usage: lamp-memory-eval DATASET.json [--units session|turn|both] [--repeats N] [--limit N] [--machine LABEL] [--per-query NEW.jsonl]";
const HELP: &str = "Runs whole-word search and SQLite FTS5 BM25 on the same complete text.\n\
The official_user_only track follows LongMemEval's user-side has_answer labels;\n\
it excludes abstentions and questions without a user-side target (419 eligible\n\
questions in the pinned cleaned S release). both_speakers is a separate track:\n\
session targets use answer_session_ids; turn targets use both speakers'\n\
has_answer labels (470 eligible questions). Turn scores count actual target\n\
turns, not any turn from an answer session. No turn-to-session score is claimed.\n\
Reports recall_all, recall_any, fractional recall, and the upstream nDCG\n\
discount convention at 1, 5 and 10, overall and by question type. The full\n\
context reference returns everything: a retrieval ceiling, not answer accuracy.\n\
Defaults: session units and 3 repetitions of each query. --limit selects a\n\
prefix for a smoke test and is not a full benchmark. --per-query creates a\n\
new file; it never overwrites one. Summary JSON goes to stdout.\n\
Latency is local, in-memory query-to-ranked-IDs time, including normalization.\n\
Index construction is reported separately. First-query and repeated-query\n\
times are separate; neither claims a cold operating-system cache. Text bytes\n\
are UTF-8, not tokens or network traffic. There is no 600/800-byte truncation,\n\
30-day expiry, live store I/O, socket, or answer generation in this harness.";

#[derive(Debug, Deserialize)]
struct Turn {
    role: String,
    content: String,
    #[serde(default)]
    has_answer: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct Question {
    question_id: String,
    question_type: String,
    question: String,
    haystack_dates: Vec<String>,
    haystack_session_ids: Vec<String>,
    haystack_sessions: Vec<Vec<Turn>>,
    answer_session_ids: Vec<String>,
    // The answer field is intentionally not deserialized or indexed.
}

impl Question {
    fn abstention(&self) -> bool {
        self.question_id.contains("_abs")
    }

    fn validate(&self) -> Result<(), String> {
        let count = self.haystack_sessions.len();
        if count == 0
            || self.haystack_dates.len() != count
            || self.haystack_session_ids.len() != count
        {
            return Err("history arrays must be nonempty and have equal lengths".into());
        }
        let sessions: BTreeSet<&str> = self
            .haystack_session_ids
            .iter()
            .map(String::as_str)
            .collect();
        // The published cleaned S data contains duplicate distractor
        // session ids. Preserve their occurrences, as upstream does.
        if !self.abstention() {
            if self.answer_session_ids.is_empty() {
                return Err("answerable question has no answer_session_ids".into());
            }
            for answer in &self.answer_session_ids {
                if !sessions.contains(answer.as_str()) {
                    return Err(format!("answer session {answer:?} is missing from history"));
                }
            }
        }
        for (id, turns) in self
            .haystack_session_ids
            .iter()
            .zip(&self.haystack_sessions)
        {
            for turn in turns {
                if turn.role != "user" && turn.role != "assistant" {
                    return Err(format!("unsupported role {:?} in {id}", turn.role));
                }
                if id.contains("answer") && turn.role == "user" && turn.has_answer.is_none() {
                    return Err(format!(
                        "answer-session user turn in {id} has no has_answer label"
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Track {
    user_only: bool,
    session: bool,
}

impl Track {
    fn name(self) -> &'static str {
        match (self.user_only, self.session) {
            (true, true) => "official_user_only_session",
            (true, false) => "official_user_only_turn",
            (false, true) => "both_speakers_session",
            (false, false) => "both_speakers_turn",
        }
    }

    fn labels(self) -> &'static str {
        match (self.user_only, self.session) {
            (true, true) => "sessions with answer-prefixed ids and a user has_answer=true turn",
            (true, false) => "user has_answer=true turns in answer-prefixed sessions",
            (false, true) => "answer_session_ids",
            (false, false) => "user or assistant has_answer=true turns in answer_session_ids",
        }
    }
}

struct Document {
    id: String,
    date: String,
    body: String,
    target: bool,
}

fn corpus(question: &Question, track: Track) -> Vec<Document> {
    let mut documents = Vec::new();
    for ((id, date), turns) in question
        .haystack_session_ids
        .iter()
        .zip(&question.haystack_dates)
        .zip(&question.haystack_sessions)
    {
        let official_target = id.contains("answer")
            && turns
                .iter()
                .any(|turn| turn.role == "user" && turn.has_answer == Some(true));
        let annotated_session = question.answer_session_ids.contains(id);
        let included = |turn: &&Turn| !track.user_only || turn.role == "user";
        if track.session {
            documents.push(Document {
                id: id.clone(),
                date: date.clone(),
                body: turns
                    .iter()
                    .filter(included)
                    .map(|turn| turn.content.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                target: if track.user_only {
                    official_target
                } else {
                    annotated_session
                },
            });
        } else {
            for (position, turn) in turns.iter().enumerate() {
                if !included(&turn) {
                    continue;
                }
                documents.push(Document {
                    id: format!("{id}_{}", position + 1),
                    date: date.clone(),
                    body: turn.content.clone(),
                    target: turn.has_answer == Some(true)
                        && if track.user_only {
                            id.contains("answer")
                        } else {
                            annotated_session
                        },
                });
            }
        }
    }
    // Upstream dates have a year-first, fixed-width form. Stable ordering
    // retains message order when multiple documents have the same date.
    documents.sort_by(|left, right| left.date.cmp(&right.date));
    documents
}

fn legacy_rank(question: &str, documents: &[Document], limit: usize) -> Vec<usize> {
    let words = search::words(question);
    let mut ranked: Vec<_> = documents
        .iter()
        .enumerate()
        .filter_map(|(position, document)| {
            let score = search::matched(&words, &[&document.body]);
            (score > 0).then_some((score, position))
        })
        .collect();
    ranked.sort_unstable_by(|left, right| right.cmp(left));
    ranked.truncate(limit);
    ranked.into_iter().map(|(_, position)| position).collect()
}

#[derive(Clone, Copy, Default)]
struct Metric {
    recall_any: f64,
    recall_all: f64,
    recall_fraction: f64,
    ndcg: f64,
    documents: f64,
    bytes: f64,
}

impl Metric {
    fn add(&mut self, other: Self) {
        self.recall_any += other.recall_any;
        self.recall_all += other.recall_all;
        self.recall_fraction += other.recall_fraction;
        self.ndcg += other.ndcg;
        self.documents += other.documents;
        self.bytes += other.bytes;
    }

    fn json(self, denominator: usize) -> Value {
        if denominator == 0 {
            return Value::Null;
        }
        let n = denominator as f64;
        json!({
            "recall_any": self.recall_any / n,
            "recall_all": self.recall_all / n,
            "recall_fraction": self.recall_fraction / n,
            "ndcg_official_discount": self.ndcg / n,
            "returned_documents_mean": self.documents / n,
            "returned_utf8_bytes_mean": self.bytes / n,
        })
    }
}

// This deliberately matches upstream eval_utils.py, whose first and second
// ranks both have discount 1. Do not substitute log2(rank + 1).
fn discount(position: usize) -> f64 {
    if position == 0 {
        1.0
    } else {
        1.0 / ((position + 1) as f64).log2()
    }
}

fn metric(documents: &[Document], ranked: &[usize], depth: usize) -> Metric {
    let target_ids: BTreeSet<_> = documents
        .iter()
        .filter(|document| document.target)
        .map(|document| document.id.as_str())
        .collect();
    if target_ids.is_empty() {
        return Metric::default();
    }
    let selected = &ranked[..ranked.len().min(depth)];
    let recalled_ids: BTreeSet<_> = selected
        .iter()
        .filter(|&&position| documents[position].target)
        .map(|&position| documents[position].id.as_str())
        .collect();
    let hits = recalled_ids.len();
    let dcg: f64 = selected
        .iter()
        .enumerate()
        .filter(|&(_, &position)| documents[position].target)
        .map(|(position, _)| discount(position))
        .sum();
    // Upstream recall uses sets of document ids, but nDCG counts the
    // relevance of each corpus occurrence, including duplicated ids.
    let target_occurrences = documents.iter().filter(|document| document.target).count();
    let ideal: f64 = (0..target_occurrences.min(depth)).map(discount).sum();
    Metric {
        recall_any: f64::from(hits > 0),
        recall_all: f64::from(hits == target_ids.len()),
        recall_fraction: hits as f64 / target_ids.len() as f64,
        ndcg: if ideal == 0.0 { 0.0 } else { dcg / ideal },
        documents: selected.len() as f64,
        bytes: selected
            .iter()
            .map(|&position| documents[position].body.len())
            .sum::<usize>() as f64,
    }
}

struct Measured {
    ranked: Vec<usize>,
    first_ms: f64,
    repeated_ms: Vec<f64>,
    build_ms: Option<f64>,
}

fn measure(
    repeats: usize,
    build_ms: Option<f64>,
    mut retrieve: impl FnMut() -> Result<Vec<usize>, String>,
) -> Result<Measured, String> {
    let mut ranked = Vec::new();
    let mut times = Vec::with_capacity(repeats);
    for repetition in 0..repeats {
        let began = Instant::now();
        let current = retrieve()?;
        times.push(began.elapsed().as_secs_f64() * 1000.0);
        if repetition == 0 {
            ranked = current;
        } else if current != ranked {
            return Err("retriever changed its ranking across identical repetitions".into());
        }
    }
    Ok(Measured {
        ranked,
        first_ms: times[0],
        repeated_ms: times[1..].to_vec(),
        build_ms,
    })
}

fn statistics(samples: &[f64]) -> Value {
    if samples.is_empty() {
        return Value::Null;
    }
    let mut ordered = samples.to_vec();
    ordered.sort_by(f64::total_cmp);
    json!({
        "n": ordered.len(),
        "mean": ordered.iter().sum::<f64>() / ordered.len() as f64,
        "p50": ordered[ordered.len().div_ceil(2) - 1],
        "p95": ordered[(ordered.len() * 95).div_ceil(100) - 1],
        "max": ordered[ordered.len() - 1],
    })
}

#[derive(Default)]
struct Totals {
    questions: usize,
    nonempty: usize,
    corpus_documents: usize,
    corpus_bytes: usize,
    targets: usize,
    metrics: [Metric; 3],
    first_ms: Vec<f64>,
    repeated_ms: Vec<f64>,
    build_ms: Vec<f64>,
}

impl Totals {
    fn record(&mut self, documents: &[Document], measured: &Measured) {
        self.questions += 1;
        self.nonempty += usize::from(!measured.ranked.is_empty());
        self.corpus_documents += documents.len();
        self.corpus_bytes += documents
            .iter()
            .map(|document| document.body.len())
            .sum::<usize>();
        self.targets += documents.iter().filter(|document| document.target).count();
        for (total, depth) in self.metrics.iter_mut().zip(DEPTHS) {
            total.add(metric(documents, &measured.ranked, depth));
        }
        self.first_ms.push(measured.first_ms);
        self.repeated_ms.extend(&measured.repeated_ms);
        if let Some(build) = measured.build_ms {
            self.build_ms.push(build);
        }
    }

    fn json(&self) -> Value {
        let depths: BTreeMap<_, _> = DEPTHS
            .iter()
            .zip(&self.metrics)
            .map(|(depth, metric)| (depth.to_string(), metric.json(self.questions)))
            .collect();
        json!({
            "questions": self.questions,
            "queries_with_results": self.nonempty,
            "queries_without_results": self.questions - self.nonempty,
            "corpus_documents_total": self.corpus_documents,
            "corpus_utf8_bytes_total": self.corpus_bytes,
            "target_documents_total": self.targets,
            "depths": depths,
            "first_query_ms": statistics(&self.first_ms),
            "repeated_query_ms": statistics(&self.repeated_ms),
            "index_build_ms": statistics(&self.build_ms),
        })
    }
}

#[derive(Default)]
struct MethodTotals {
    overall: Totals,
    abilities: BTreeMap<String, Totals>,
}

impl MethodTotals {
    fn record(&mut self, ability: &str, documents: &[Document], measured: &Measured) {
        self.overall.record(documents, measured);
        self.abilities
            .entry(ability.to_owned())
            .or_default()
            .record(documents, measured);
    }

    fn json(&self) -> Value {
        let abilities: BTreeMap<_, _> = self
            .abilities
            .iter()
            .map(|(name, totals)| (name, totals.json()))
            .collect();
        json!({"overall": self.overall.json(), "by_question_type": abilities})
    }
}

#[derive(Default)]
struct TrackTotals {
    methods: BTreeMap<&'static str, MethodTotals>,
    skipped_no_target: Vec<String>,
    skipped_abstention: usize,
}

impl TrackTotals {
    fn json(&self, track: Track) -> Value {
        let methods: BTreeMap<_, _> = self
            .methods
            .iter()
            .map(|(name, totals)| (name, totals.json()))
            .collect();
        let reference = self.methods.get("whole_word").map(|method| {
            let total = &method.overall;
            json!({
                "questions": total.questions,
                "recall_any": 1.0,
                "recall_all": 1.0,
                "recall_fraction": 1.0,
                "returned_documents_mean": total.corpus_documents as f64 / total.questions as f64,
                "returned_utf8_bytes_mean": total.corpus_bytes as f64 / total.questions as f64,
                "meaning": "all indexed text; unbounded retrieval ceiling, no answer model or accuracy score",
            })
        });
        json!({
            "name": track.name(),
            "unit": if track.session { "session" } else { "individual_message_turn" },
            "labels": track.labels(),
            "excluded_abstentions": self.skipped_abstention,
            "excluded_no_target": self.skipped_no_target.len(),
            "excluded_no_target_ids": self.skipped_no_target,
            "methods": methods,
            "full_context_reference": reference,
        })
    }
}

struct Arguments {
    dataset: PathBuf,
    tracks: Vec<Track>,
    repeats: usize,
    limit: Option<usize>,
    machine: String,
    per_query: Option<PathBuf>,
}

fn arguments() -> Result<Option<Arguments>, String> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--help" {
        return Ok(None);
    }
    if args.is_empty() || args.len() % 2 != 1 {
        return Err(USAGE.into());
    }
    let mut parsed = Arguments {
        dataset: PathBuf::from(&args[0]),
        tracks: Vec::new(),
        repeats: 3,
        limit: None,
        machine: format!("{}-{}", env::consts::OS, env::consts::ARCH),
        per_query: None,
    };
    let mut units = "session".to_owned();
    let mut seen = BTreeSet::new();
    for pair in args[1..].chunks_exact(2) {
        let key = pair[0].to_str().ok_or("option is not UTF-8")?;
        if !seen.insert(key) {
            return Err(format!("duplicate option {key}"));
        }
        if key == "--per-query" {
            parsed.per_query = Some(PathBuf::from(&pair[1]));
            continue;
        }
        let value = pair[1].to_str().ok_or("option value is not UTF-8")?;
        match key {
            "--units" if matches!(value, "session" | "turn" | "both") => units = value.into(),
            "--repeats" => {
                parsed.repeats = value.parse().map_err(|_| "--repeats must be 1..=20")?;
                if !(1..=20).contains(&parsed.repeats) {
                    return Err("--repeats must be 1..=20".into());
                }
            }
            "--limit" => {
                let limit = value
                    .parse()
                    .map_err(|_| "--limit must be a positive integer")?;
                if limit == 0 {
                    return Err("--limit must be a positive integer".into());
                }
                parsed.limit = Some(limit);
            }
            "--machine" if !value.trim().is_empty() => parsed.machine = value.into(),
            _ => return Err(format!("unknown option or invalid value: {key} {value:?}")),
        }
    }
    for session in [true, false] {
        if (session && units == "turn") || (!session && units == "session") {
            continue;
        }
        for user_only in [true, false] {
            parsed.tracks.push(Track { user_only, session });
        }
    }
    Ok(Some(parsed))
}

fn per_query_result(documents: &[Document], measured: &Measured) -> Value {
    let metrics: BTreeMap<_, _> = DEPTHS
        .into_iter()
        .map(|depth| {
            (
                depth.to_string(),
                metric(documents, &measured.ranked, depth).json(1),
            )
        })
        .collect();
    json!({
        "ranked_ids": measured.ranked.iter().map(|&position| &documents[position].id).collect::<Vec<_>>(),
        "metrics": metrics,
        "first_query_ms": measured.first_ms,
        "repeated_query_ms": measured.repeated_ms,
        "index_build_ms": measured.build_ms,
    })
}

fn evaluate(arguments: &Arguments) -> Result<Value, String> {
    let input = fs::read(&arguments.dataset).map_err(|error| format!("read dataset: {error}"))?;
    let dataset_bytes = input.len();
    let digest = format!("{:x}", Sha256::digest(&input));
    let questions: Vec<Question> =
        serde_json::from_slice(&input).map_err(|error| format!("parse dataset: {error}"))?;
    drop(input);
    if questions.is_empty() {
        return Err("dataset contains no questions".into());
    }
    let mut question_ids = BTreeSet::new();
    for question in &questions {
        if !question_ids.insert(&question.question_id) {
            return Err(format!("duplicate question id {:?}", question.question_id));
        }
        question
            .validate()
            .map_err(|error| format!("question {}: {error}", question.question_id))?;
    }
    let selected = arguments
        .limit
        .unwrap_or(questions.len())
        .min(questions.len());
    let duplicate_history_ids: Vec<_> = questions
        .iter()
        .take(selected)
        .filter(|question| {
            question
                .haystack_session_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != question.haystack_session_ids.len()
        })
        .map(|question| question.question_id.as_str())
        .collect();
    let mut output = arguments
        .per_query
        .as_ref()
        .map(|path| {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map(BufWriter::new)
                .map_err(|error| format!("create per-query output {}: {error}", path.display()))
        })
        .transpose()?;
    let mut totals: Vec<_> = arguments
        .tracks
        .iter()
        .map(|_| TrackTotals::default())
        .collect();
    let began = Instant::now();
    for (number, question) in questions.iter().take(selected).enumerate() {
        for (&track, total) in arguments.tracks.iter().zip(&mut totals) {
            if question.abstention() {
                total.skipped_abstention += 1;
                continue;
            }
            let documents = corpus(question, track);
            let targets = documents.iter().filter(|document| document.target).count();
            if targets == 0 {
                if !track.user_only {
                    return Err(format!(
                        "question {} has no target labels for {}",
                        question.question_id,
                        track.name()
                    ));
                }
                total.skipped_no_target.push(question.question_id.clone());
                continue;
            }
            let legacy = measure(arguments.repeats, None, || {
                Ok(legacy_rank(&question.question, &documents, 10))
            })?;
            let build_began = Instant::now();
            let index =
                SearchIndex::new().map_err(|error| format!("create FTS5 index: {error}"))?;
            for (position, document) in documents.iter().enumerate() {
                // Only the body and an opaque integer reach the retriever.
                // No answer, has_answer, question type, date, or session id
                // is part of the FTS content or query.
                index
                    .add(position as i64 + 1, &document.body)
                    .map_err(|error| format!("populate FTS5 index: {error}"))?;
            }
            let build_ms = build_began.elapsed().as_secs_f64() * 1000.0;
            let ranked = measure(arguments.repeats, Some(build_ms), || {
                index
                    .rank(&question.question, 10)
                    .map(|ids| ids.into_iter().map(|id| id as usize - 1).collect())
                    .map_err(|error| format!("query FTS5 index: {error}"))
            })?;
            for (name, measured) in [("whole_word", &legacy), ("fts5_bm25", &ranked)] {
                total.methods.entry(name).or_default().record(
                    &question.question_type,
                    &documents,
                    measured,
                );
            }
            if let Some(output) = &mut output {
                let line = json!({
                    "question_id": question.question_id,
                    "question_type": question.question_type,
                    "track": track.name(),
                    "corpus_documents": documents.len(),
                    "target_documents": targets,
                    "whole_word": per_query_result(&documents, &legacy),
                    "fts5_bm25": per_query_result(&documents, &ranked),
                });
                serde_json::to_writer(&mut *output, &line)
                    .map_err(|error| format!("write per-query result: {error}"))?;
                output
                    .write_all(b"\n")
                    .map_err(|error| format!("write per-query newline: {error}"))?;
            }
        }
        if (number + 1) % 50 == 0 {
            eprintln!("evaluated {} of {selected} questions", number + 1);
        }
    }
    if let Some(output) = &mut output {
        output
            .flush()
            .map_err(|error| format!("flush per-query results: {error}"))?;
    }
    let results: Vec<_> = arguments
        .tracks
        .iter()
        .zip(&totals)
        .map(|(&track, total)| total.json(track))
        .collect();
    Ok(json!({
        "schema_version": 1,
        "dataset": {"path": arguments.dataset, "bytes": dataset_bytes, "sha256": digest},
        "machine": arguments.machine,
        "compile_target": format!("{}-{}", env::consts::OS, env::consts::ARCH),
        "sqlite_version": rusqlite::version(),
        "questions_in_file": questions.len(),
        "questions_selected": selected,
        "questions_with_duplicate_history_ids": duplicate_history_ids,
        "history_duplicates": "source occurrences are retained; recall uses unique target ids and nDCG counts corpus occurrences, like upstream",
        "complete_file_evaluated": selected == questions.len(),
        "repetitions_per_query": arguments.repeats,
        "evaluation_wall_ms": began.elapsed().as_secs_f64() * 1000.0,
        "ranking_ties": "newer timestamp, then later input position; neither is an indexed term",
        "query_latency_scope": "local in-memory normalization and ranking to IDs; excludes index build, file I/O, sockets, response encoding, and models; bytes are original UTF-8 text, not tokens or network egress",
        "index_build_scope": "open an in-memory SQLite index, normalize complete bodies, insert each document; excludes corpus construction and dataset parsing",
        "evaluation_wall_scope": "all selected queries, corpus/index construction, repetitions, metric calculation and optional per-query file writes; excludes initial dataset read/parse/validation and final summary encoding",
        "limits": "full untruncated sessions or individual messages; no live 600/800-byte input caps, answer-byte cap, or 30-day expiry; not production per-exchange or lamp latency",
        "ndcg_convention": "upstream eval_utils.py: discounts 1, 1, 1/log2(3), ...; not log2(rank+1)",
        "abstention": "question ids containing _abs are excluded, never scored as retrieval successes; answer abstention is not measured",
        "tracks": results,
    }))
}

fn main() -> ExitCode {
    let arguments = match arguments() {
        Ok(Some(arguments)) => arguments,
        Ok(None) => {
            println!("{USAGE}\n\n{HELP}");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match evaluate(&arguments) {
        Ok(result) => {
            let mut output = io::stdout().lock();
            if let Err(error) = serde_json::to_writer_pretty(&mut output, &result)
                .map_err(io::Error::other)
                .and_then(|()| output.write_all(b"\n"))
            {
                eprintln!("write summary: {error}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(body: &str, target: bool) -> Document {
        Document {
            id: body.into(),
            date: "2026/01/01 (Thu) 12:00".into(),
            body: body.into(),
            target,
        }
    }

    fn sample() -> Question {
        serde_json::from_value(json!({
            "question_id": "fixture",
            "question_type": "multi-session",
            "question": "coffee",
            "answer": 42,
            "haystack_dates": ["2026/01/01 (Thu) 12:00", "2026/01/02 (Fri) 12:00"],
            "haystack_session_ids": ["answer_one", "answer_two"],
            "answer_session_ids": ["answer_one", "answer_two"],
            "haystack_sessions": [
                [{"role":"user", "content":"coffee", "has_answer":true},
                 {"role":"assistant", "content":"tea", "has_answer":false}],
                [{"role":"user", "content":"water", "has_answer":false},
                 {"role":"assistant", "content":"cocoa", "has_answer":true}]
            ]
        }))
        .unwrap()
    }

    #[test]
    fn partial_evidence_is_not_all_evidence_and_ndcg_matches_upstream_discount() {
        let documents = [
            document("first", true),
            document("irrelevant", false),
            document("second", true),
        ];
        let result = metric(&documents, &[0, 1, 2], 2);
        assert_eq!(result.recall_any, 1.0);
        assert_eq!(result.recall_all, 0.0);
        assert_eq!(result.recall_fraction, 0.5);
        assert_eq!(result.ndcg, 0.5);
        assert_eq!(metric(&documents, &[2, 0, 1], 2).ndcg, 1.0);
        let third = metric(
            &[
                document("a", false),
                document("b", false),
                document("c", true),
            ],
            &[0, 1, 2],
            3,
        );
        assert!((third.ndcg - 0.6309297535714575).abs() < 1e-12);
        assert_eq!(metric(&documents, &[], 10).recall_all, 0.0);
    }

    #[test]
    fn no_labels_never_produces_vacuous_recall() {
        let result = metric(&[document("unknown", false)], &[0], 5);
        assert_eq!(result.recall_all, 0.0);
        assert_eq!(result.ndcg, 0.0);
    }

    #[test]
    fn repeated_session_occurrences_do_not_count_as_distinct_recalled_evidence() {
        let documents = [
            document("same-id", true),
            document("same-id", true),
            document("other-id", true),
        ];
        let result = metric(&documents, &[0, 1, 2], 2);
        assert_eq!(result.recall_all, 0.0);
        assert_eq!(result.recall_fraction, 0.5);
        assert_eq!(result.ndcg, 1.0);
        assert_eq!(metric(&documents, &[0, 1, 2], 3).recall_all, 1.0);
        let mut question = sample();
        question.haystack_session_ids.push("answer_one".into());
        question
            .haystack_dates
            .push(question.haystack_dates[0].clone());
        question.haystack_sessions.push(vec![Turn {
            role: "user".into(),
            content: "coffee".into(),
            has_answer: Some(true),
        }]);
        question.validate().unwrap();
        assert_eq!(
            corpus(
                &question,
                Track {
                    user_only: true,
                    session: true
                }
            )
            .len(),
            3
        );
    }

    #[test]
    fn roles_and_units_keep_the_two_label_conventions_separate() {
        let question = sample();
        question.validate().unwrap();
        let official = corpus(
            &question,
            Track {
                user_only: true,
                session: true,
            },
        );
        assert_eq!(
            official.iter().filter(|document| document.target).count(),
            1
        );
        assert_eq!(official[0].body, "coffee");
        let both = corpus(
            &question,
            Track {
                user_only: false,
                session: true,
            },
        );
        assert_eq!(both.iter().filter(|document| document.target).count(), 2);
        assert_eq!(both[0].body, "coffee tea");
        let turns = corpus(
            &question,
            Track {
                user_only: false,
                session: false,
            },
        );
        assert_eq!(turns.len(), 4);
        assert!(turns[0].target);
        assert!(!turns[1].target);
        assert!(!turns[2].target);
        assert!(turns[3].target);
    }

    #[test]
    fn evidence_labels_never_change_retrieved_text_or_ranking() {
        let mut question = sample();
        let track = Track {
            user_only: false,
            session: true,
        };
        let first = corpus(&question, track);
        question.answer_session_ids = vec!["answer_two".into()];
        for session in &mut question.haystack_sessions {
            for turn in session {
                turn.has_answer = Some(false);
            }
        }
        let second = corpus(&question, track);
        assert_eq!(
            first.iter().map(|d| &d.body).collect::<Vec<_>>(),
            second.iter().map(|d| &d.body).collect::<Vec<_>>()
        );
        assert_eq!(
            legacy_rank("coffee", &first, 10),
            legacy_rank("coffee", &second, 10)
        );
        assert_eq!(
            legacy_rank("answer one multi session", &first, 10),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn missing_or_unaligned_evidence_is_rejected() {
        let mut question = sample();
        question.answer_session_ids.push("missing".into());
        assert!(
            question
                .validate()
                .unwrap_err()
                .contains("missing from history")
        );
        question.question_id = "fixture_abs".into();
        assert!(question.validate().is_ok());
        question.haystack_dates.pop();
        assert!(question.validate().unwrap_err().contains("equal lengths"));
    }

    #[test]
    fn legacy_ties_are_newer_first_and_nonmatches_are_not_retrieved() {
        let documents = [
            document("coffee", true),
            document("tea", false),
            document("coffee", false),
        ];
        assert_eq!(legacy_rank("coffee", &documents, 10), vec![2, 0]);
        assert!(legacy_rank("?", &documents, 10).is_empty());
    }

    #[test]
    fn utf8_bytes_and_full_cutoff_are_measured_without_truncation() {
        let documents = [
            document("cà phê", true),
            document(&"long ".repeat(200), false),
        ];
        let result = metric(&documents, &[0, 1], 1);
        assert_eq!(result.bytes, 8.0);
        assert_eq!(result.documents, 1.0);
        assert_eq!(metric(&documents, &[0, 1], 5).bytes, 1008.0);
    }
}
