//! Bounded, deterministic offline fitting from independently labelled rows.
//!
//! Question heads see only training sessions. The local choice head sees
//! frozen question predictions on separate calibration sessions. Test rows
//! are used only for the report; the fixed judge mixture is never fitted.
//! Session isolation is checked mechanically; honest session identity and
//! lawful collection/labels remain the recording author's responsibility.
use super::{
    baseline::{Counts, Snapshot},
    local::{
        Cues, Head, Labels, Provenance, QUESTION_COUNT, QUESTION_NAMES, State, Weights, invalid,
        predict, short_text, sigmoid,
    },
};
use crate::attention::Choice;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, BufRead, Read},
};

pub const MOST_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MOST_ROWS: usize = 10_000;
pub const MOST_LINE_BYTES: usize = 16 * 1024;
pub const MOST_EPOCHS: u16 = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Split {
    Train,
    Calibration,
    Test,
}

/// Independent answers to the semantic questions, not labels derived from
/// either judge's output. An unlabelled question is explicitly nullable.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionLabels {
    pub calls_lamp: Option<bool>,
    pub names_someone_else: Option<bool>,
    pub others_business: Option<bool>,
    pub lamp_could_answer: Option<bool>,
    pub follows_lamp: Option<bool>,
    pub listening_noise: Option<bool>,
    pub asks_to_stop: Option<bool>,
}

impl QuestionLabels {
    fn values(&self) -> [Option<bool>; QUESTION_COUNT] {
        [
            self.calls_lamp,
            self.names_someone_else,
            self.others_business,
            self.lamp_could_answer,
            self.follows_lamp,
            self.listening_noise,
            self.asks_to_stop,
        ]
    }
}

/// One JSONL line. `source` and `rights` must identify the authorized source
/// and basis for use; they are declarations, not automatic license checks.
/// A session must include all related turns from that recording encounter.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub schema: u8,
    pub id: String,
    pub session: String,
    pub split: Split,
    pub source: String,
    pub rights: String,
    pub cues: Cues,
    pub labels: QuestionLabels,
    pub directed: Option<bool>,
    #[serde(default)]
    pub baseline: Option<Snapshot>,
}

/// Full-batch gradient descent, fixed order, bounded steps, L2 on weights
/// (not the intercept). Coefficients are clipped to [-16,16] each step.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub epochs: u16,
    pub learning_rate: f64,
    pub l2: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            epochs: 300,
            learning_rate: 0.25,
            l2: 0.01,
        }
    }
}

impl Options {
    fn validate(&self) -> io::Result<()> {
        if self.epochs == 0
            || self.epochs > MOST_EPOCHS
            || !self.learning_rate.is_finite()
            || !(0.0..=1.0).contains(&self.learning_rate)
            || self.learning_rate == 0.0
            || !self.l2.is_finite()
            || !(0.0..=1.0).contains(&self.l2)
        {
            return Err(invalid("invalid bounded optimizer settings"));
        }
        Ok(())
    }
}

/// Reject an oversized line before growing a buffer past 16 KiB + 1.
/// Blank lines and malformed rows are rejected; errors contain no row data.
pub fn read_rows(mut reader: impl BufRead) -> io::Result<Vec<Row>> {
    let mut rows = Vec::new();
    let mut total = 0usize;
    loop {
        let mut line = Vec::new();
        let read = reader
            .by_ref()
            .take((MOST_LINE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read);
        if read > MOST_LINE_BYTES {
            return Err(invalid("fitting line exceeds 16 KiB"));
        }
        if total > MOST_INPUT_BYTES {
            return Err(invalid("fitting input exceeds 16 MiB"));
        }
        if rows.len() == MOST_ROWS {
            return Err(invalid("fitting input exceeds 10000 rows"));
        }
        let row: Row =
            serde_json::from_slice(&line).map_err(|_| invalid("malformed fitting row"))?;
        rows.push(row);
    }
    validate_rows(&rows)?;
    Ok(rows)
}

fn validate_rows(rows: &[Row]) -> io::Result<()> {
    if rows.is_empty() || rows.len() > MOST_ROWS {
        return Err(invalid("invalid fitting row count"));
    }
    let mut ids = BTreeSet::new();
    let mut sessions = BTreeMap::new();
    let mut splits = [0usize; 3];
    let mut sources = BTreeSet::new();
    let mut rights = BTreeSet::new();
    // Check aggregate serialized bounds as well for callers bypassing read_rows.
    let mut total = 0usize;
    for row in rows {
        if row.schema != 1
            || !short_text(&row.id, 128)
            || !short_text(&row.session, 128)
            || !short_text(&row.source, 256)
            || !short_text(&row.rights, 256)
        {
            return Err(invalid("invalid fitting identity or provenance"));
        }
        row.cues.validate().map_err(invalid)?;
        if row
            .baseline
            .as_ref()
            .is_some_and(|snapshot| snapshot.validate().is_err())
        {
            return Err(invalid("invalid fitting baseline snapshot"));
        }
        if !ids.insert(&row.id) {
            return Err(invalid("duplicate fitting row id"));
        }
        if sessions
            .insert(&row.session, row.split)
            .is_some_and(|split| split != row.split)
        {
            return Err(invalid("a session crosses fitting splits"));
        }
        if row.split != Split::Train && row.directed.is_none() {
            return Err(invalid(
                "calibration and test rows need independent directed labels",
            ));
        }
        splits[match row.split {
            Split::Train => 0,
            Split::Calibration => 1,
            Split::Test => 2,
        }] += 1;
        sources.insert(&row.source);
        rights.insert(&row.rights);
        if sources.len() > 64 || rights.len() > 64 {
            return Err(invalid("too many fitting sources or rights declarations"));
        }
        let size = serde_json::to_vec(row)
            .map_err(|_| invalid("invalid fitting row"))?
            .len()
            + 1;
        if size > MOST_LINE_BYTES {
            return Err(invalid("fitting line exceeds 16 KiB"));
        }
        total = total.saturating_add(size);
        if total > MOST_INPUT_BYTES {
            return Err(invalid("fitting input exceeds 16 MiB"));
        }
    }
    if splits.contains(&0) {
        return Err(invalid("train, calibration and test sessions are required"));
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct HeadReport {
    pub question: String,
    pub state: &'static str,
    pub positive_labels: u32,
    pub negative_labels: u32,
    pub unknown_labels: u32,
}

/// Threshold counts describe permissions, not observed speech. A miss here is
/// a directed utterance for which the model did not choose Answer.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Metrics {
    pub labelled: u64,
    pub for_lamp: u64,
    pub not_for_lamp: u64,
    pub answers: u64,
    pub looks_up: u64,
    pub stays_quiet: u64,
    pub false_answers: u64,
    pub missed_calls: u64,
    pub brier: f64,
    pub log_loss: f64,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u8,
    pub method: &'static str,
    pub optimizer: Options,
    pub train_rows: usize,
    pub calibration_rows: usize,
    pub test_rows: usize,
    pub train_sessions: usize,
    pub calibration_sessions: usize,
    pub test_sessions: usize,
    pub question_heads: Vec<HeadReport>,
    pub choice_positive_labels: u32,
    pub choice_negative_labels: u32,
    pub mixture: &'static str,
    pub local_share: f32,
    pub test: Metrics,
    pub test_questions: BTreeMap<String, QuestionMetric>,
    pub baseline_source: &'static str,
    pub baseline: Counts,
    pub sources: Vec<String>,
    pub rights: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct QuestionMetric {
    pub labelled: u64,
    pub positive: u64,
    pub negative: u64,
    pub brier: Option<f64>,
    pub log_loss: Option<f64>,
}

pub struct FitResult {
    pub weights: Weights,
    pub report: Report,
}

pub fn fit(rows: &[Row], options: &Options) -> io::Result<FitResult> {
    options.validate()?;
    validate_rows(rows)?;
    let train: Vec<_> = rows.iter().filter(|r| r.split == Split::Train).collect();
    let calibration: Vec<_> = rows
        .iter()
        .filter(|r| r.split == Split::Calibration)
        .collect();
    let test: Vec<_> = rows.iter().filter(|r| r.split == Split::Test).collect();
    let mut weights = Weights::seed();
    let mut heads = Vec::new();
    for (i, name) in QUESTION_NAMES.iter().enumerate() {
        let examples: Vec<_> = train
            .iter()
            .filter_map(|r| r.labels.values()[i].map(|y| (r.cues.features(), y)))
            .collect();
        let counts = count(&examples);
        let head = &mut weights.questions[i].head;
        head.labels = counts;
        head.weights.fill(0.0);
        let sufficient = counts.yes > 0 && counts.no > 0;
        if sufficient {
            optimize(head, &examples, options);
        }
        head.state = if sufficient {
            State::Fitted
        } else {
            State::PriorOnly
        };
        heads.push(HeadReport {
            question: (*name).to_owned(),
            state: if sufficient {
                "fitted_on_train"
            } else {
                "prior_only_insufficient_classes"
            },
            positive_labels: counts.yes,
            negative_labels: counts.no,
            unknown_labels: train.len() as u32 - counts.yes - counts.no,
        });
    }
    let examples: Vec<_> = calibration
        .iter()
        .map(|row| {
            let probabilities = weights.probabilities(&row.cues.features());
            (
                weights.center(probabilities),
                row.directed.expect("validated calibration label"),
            )
        })
        .collect();
    let choice_labels = count(&examples);
    if choice_labels.yes == 0 || choice_labels.no == 0 {
        return Err(invalid("calibration requires both directed label classes"));
    }
    weights.local_choice.weights.fill(0.0);
    weights.local_choice.labels = choice_labels;
    weights.local_choice.state = State::Fitted;
    optimize(&mut weights.local_choice, &examples, options);
    // Unsupported semantic heads remain zero-effect in the choice model too.
    for i in 0..QUESTION_COUNT {
        if weights.questions[i].head.state == State::PriorOnly {
            weights.local_choice.weights[i] = 0.0;
        }
    }
    weights.name = "lamp-local-session-fit-v1".to_owned();
    let sources: Vec<_> = rows
        .iter()
        .map(|r| r.source.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let rights: Vec<_> = rows
        .iter()
        .map(|r| r.rights.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    weights.provenance = Provenance {
        method:
            "question heads: train; local choice: calibration; test: report only; mixture: unfitted"
                .to_owned(),
        sources: sources.clone(),
        rights: rights.clone(),
        rows: rows.len() as u32,
    };
    weights.validate()?;
    let mut metrics = Metrics::default();
    let mut question_metrics: [QuestionMetric; QUESTION_COUNT] =
        std::array::from_fn(|_| QuestionMetric::default());
    let mut baseline = Counts::default();
    for row in &test {
        let probabilities = weights.probabilities(&row.cues.features());
        let p = predict(&weights.local_choice, &weights.center(probabilities));
        let choice = Choice::at(p);
        let label = row.directed.expect("validated test label");
        metrics.labelled += 1;
        if label {
            metrics.for_lamp += 1;
        } else {
            metrics.not_for_lamp += 1;
        }
        match choice {
            Choice::Answer => metrics.answers += 1,
            Choice::LookUp => metrics.looks_up += 1,
            Choice::StayQuiet => metrics.stays_quiet += 1,
        }
        metrics.false_answers += u64::from(!label && choice == Choice::Answer);
        metrics.missed_calls += u64::from(label && choice != Choice::Answer);
        let (brier, loss) = loss(p, label);
        metrics.brier += brier;
        metrics.log_loss += loss;
        for (i, label) in row.labels.values().into_iter().enumerate() {
            if let Some(label) = label {
                let m = &mut question_metrics[i];
                m.labelled += 1;
                m.positive += u64::from(label);
                m.negative += u64::from(!label);
                let (brier, loss) = self::loss(probabilities[i], label);
                *m.brier.get_or_insert(0.0) += brier;
                *m.log_loss.get_or_insert(0.0) += loss;
            }
        }
        baseline.note(row.baseline.as_ref(), choice, Some(label));
    }
    metrics.brier /= metrics.labelled as f64;
    metrics.log_loss /= metrics.labelled as f64;
    for m in &mut question_metrics {
        if m.labelled > 0 {
            m.brier = m.brier.map(|sum| sum / m.labelled as f64);
            m.log_loss = m.log_loss.map(|sum| sum / m.labelled as f64);
        }
    }
    let sessions = |rows: &[&Row]| {
        rows.iter()
            .map(|row| &row.session)
            .collect::<BTreeSet<_>>()
            .len()
    };
    let report = Report {
        schema: 1,
        method: "deterministic logistic fitting; session-disjoint train/calibration/test",
        optimizer: options.clone(),
        train_rows: train.len(),
        calibration_rows: calibration.len(),
        test_rows: test.len(),
        train_sessions: sessions(&train),
        calibration_sessions: sessions(&calibration),
        test_sessions: sessions(&test),
        question_heads: heads,
        choice_positive_labels: choice_labels.yes,
        choice_negative_labels: choice_labels.no,
        mixture: "fixed seed; not fitted or evaluated as a fused runtime policy",
        local_share: weights.local_share(),
        test: metrics,
        test_questions: QUESTION_NAMES
            .into_iter()
            .map(str::to_owned)
            .zip(question_metrics)
            .collect(),
        baseline_source: super::baseline::SOURCE,
        baseline,
        sources,
        rights,
    };
    Ok(FitResult { weights, report })
}

fn count<const N: usize>(examples: &[([f64; N], bool)]) -> Labels {
    let yes = examples.iter().filter(|(_, y)| *y).count() as u32;
    Labels {
        yes,
        no: examples.len() as u32 - yes,
    }
}

fn optimize<const N: usize>(head: &mut Head<N>, examples: &[([f64; N], bool)], options: &Options) {
    let n = examples.len() as f64;
    for _ in 0..options.epochs {
        let mut gradient = [0.0; N];
        let mut bias = 0.0;
        for (x, label) in examples {
            let logit = head.bias + head.weights.iter().zip(x).map(|(w, x)| w * x).sum::<f64>();
            let error = sigmoid(logit) - f64::from(u8::from(*label));
            bias += error;
            for i in 0..N {
                gradient[i] += error * x[i];
            }
        }
        head.bias = (head.bias - options.learning_rate * bias / n).clamp(-16.0, 16.0);
        for (weight, gradient) in head.weights.iter_mut().zip(gradient) {
            *weight = (*weight - options.learning_rate * (gradient / n + options.l2 * *weight))
                .clamp(-16.0, 16.0);
        }
    }
}

fn loss(probability: f32, label: bool) -> (f64, f64) {
    let p = f64::from(probability);
    let y = f64::from(u8::from(label));
    let bounded = p.clamp(1e-7, 1.0 - 1e-7);
    (
        (p - y).powi(2),
        -(y * bounded.ln() + (1.0 - y) * (1.0 - bounded).ln()),
    )
}
