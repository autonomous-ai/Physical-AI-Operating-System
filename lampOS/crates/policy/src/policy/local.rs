//! A local implementation of the same seven-question decision contract.
//!
//! Missing signs are zero contributions, never negative observations. These
//! small models cannot recover semantic evidence which the signs do not hold.
//! The shipped numbers are authored, unfitted priors, not measured accuracy.
//! Jev's independently fitted combining coefficients are never used here.
use crate::{
    attention::{self, Answers, Judgement},
    decide::{Answer, Asked, Decider, Failed, Question},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Read},
    path::Path,
    time::{Duration, Instant},
};

pub const MOST_CUES_BYTES: usize = 4096;
pub const MOST_WEIGHTS_BYTES: usize = 64 * 1024;
pub const FEATURE_COUNT: usize = 8;
pub const QUESTION_COUNT: usize = 7;
pub const QUESTION_NAMES: [&str; QUESTION_COUNT] = [
    "calls_lamp",
    "names_someone_else",
    "others_business",
    "lamp_could_answer",
    "follows_lamp",
    "listening_noise",
    "asks_to_stop",
];
const MOST_COEFFICIENT: f64 = 32.0;

fn schema_one() -> u8 {
    1
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomCue {
    Quiet,
    Busy,
}

/// Receipt-time signs, not a transcript. Speech duration must remain `None`
/// when only transcript receipt times are available. An unknown head pose is
/// different from a measured face turned away, and a head pose is not gaze.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cues {
    pub schema: u8,
    pub name_score: Option<f32>,
    pub conversation: Option<bool>,
    pub room: Option<RoomCue>,
    pub facing: Option<bool>,
    pub face_age_ms: Option<u32>,
    pub since_lamp_spoke_ms: Option<u32>,
    pub word_count: Option<u32>,
    pub duration_ms: Option<u32>,
    pub text_called: Option<bool>,
}

impl Default for Cues {
    fn default() -> Self {
        Self {
            schema: schema_one(),
            name_score: None,
            conversation: None,
            room: None,
            facing: None,
            face_age_ms: None,
            since_lamp_spoke_ms: None,
            word_count: None,
            duration_ms: None,
            text_called: None,
        }
    }
}

impl Cues {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != 1 {
            return Err("unsupported local cues schema");
        }
        if self
            .name_score
            .is_some_and(|p| !p.is_finite() || !(0.0..=1.0).contains(&p))
        {
            return Err("local name score must be a finite probability");
        }
        Ok(())
    }

    /// Version-one feature order: name, conversation, room, fresh facing,
    /// speech recency, word length, audio duration, explicit text call.
    /// Values are bounded to [-1, 1]; absent signs contribute exactly zero.
    pub(super) fn features(&self) -> [f64; FEATURE_COUNT] {
        let signed = |value: bool| if value { 1.0 } else { -1.0 };
        let facing = match (self.facing, self.face_age_ms) {
            (Some(pose), Some(age)) if age <= 2000 => {
                signed(pose) * (1.0 - f64::from(age) / 2000.0)
            }
            _ => 0.0,
        };
        [
            self.name_score.map_or(0.0, |p| 2.0 * f64::from(p) - 1.0),
            self.conversation.map_or(0.0, signed),
            self.room.map_or(0.0, |room| match room {
                RoomCue::Quiet => 1.0,
                RoomCue::Busy => -1.0,
            }),
            facing,
            self.since_lamp_spoke_ms
                .map_or(0.0, |ms| 1.0 - f64::from(ms.min(40_000)) / 20_000.0),
            self.word_count
                .map_or(0.0, |words| f64::from(words.min(12)) / 6.0 - 1.0),
            self.duration_ms
                .map_or(0.0, |ms| f64::from(ms.min(6000)) / 3000.0 - 1.0),
            f64::from(u8::from(self.text_called == Some(true))),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum State {
    SeededUnfitted,
    PriorOnly,
    Fitted,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Labels {
    pub yes: u32,
    pub no: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Head<const N: usize> {
    pub bias: f64,
    // Serde's array implementation is fixed-size; every model uses N=7 or 8.
    #[serde(with = "fixed")]
    pub weights: [f64; N],
    pub state: State,
    pub labels: Labels,
}

mod fixed {
    use serde::{Deserializer, Serializer, ser::SerializeSeq};
    pub fn serialize<S: Serializer, const N: usize>(
        value: &[f64; N],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(N))?;
        for item in value {
            seq.serialize_element(item)?;
        }
        seq.end()
    }
    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
        deserializer: D,
    ) -> Result<[f64; N], D::Error> {
        struct Visitor<const N: usize>;
        impl<'de, const N: usize> serde::de::Visitor<'de> for Visitor<N> {
            type Value = [f64; N];
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "exactly {N} coefficients")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = [0.0; N];
                for value in &mut values {
                    *value = seq
                        .next_element()?
                        .ok_or_else(|| serde::de::Error::custom("too few coefficients"))?;
                }
                if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                    return Err(serde::de::Error::custom("too many coefficients"));
                }
                Ok(values)
            }
        }
        deserializer.deserialize_seq(Visitor::<N>)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QuestionHead {
    pub name: String,
    pub head: Head<FEATURE_COUNT>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Provenance {
    pub method: String,
    pub sources: Vec<String>,
    pub rights: Vec<String>,
    pub rows: u32,
}

/// Validated at construction; ask performs no filesystem or network work.
/// No public deserializer bypasses validation. `judge` is this model's own
/// separately fitted combination, never the Jev attention fit.
#[derive(Clone, Debug, Serialize)]
pub struct Weights {
    pub(super) schema: u8,
    pub(super) name: String,
    pub(super) feature_schema: u8,
    pub(super) local_share: f32,
    pub(super) questions: [QuestionHead; QUESTION_COUNT],
    pub(super) local_choice: Head<QUESTION_COUNT>,
    pub(super) provenance: Provenance,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWeights {
    schema: u8,
    name: String,
    feature_schema: u8,
    local_share: f32,
    questions: [QuestionHead; QUESTION_COUNT],
    local_choice: Head<QUESTION_COUNT>,
    provenance: Provenance,
}

impl Weights {
    /// The checked-in file is the sole source of the initial numbers.
    pub fn seed() -> Self {
        Self::from_bytes(include_bytes!(
            "../../../../models/policy/local-weights.json"
        ))
        .expect("the authored local weights are valid")
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let mut bytes = Vec::new();
        File::open(path)?
            .take((MOST_WEIGHTS_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        Self::from_bytes(&bytes)
    }

    fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > MOST_WEIGHTS_BYTES {
            return Err(invalid("local weights exceed 64 KiB"));
        }
        let raw: RawWeights =
            serde_json::from_slice(bytes).map_err(|_| invalid("malformed local weights"))?;
        let weights = Self {
            schema: raw.schema,
            name: raw.name,
            feature_schema: raw.feature_schema,
            local_share: raw.local_share,
            questions: raw.questions,
            local_choice: raw.local_choice,
            provenance: raw.provenance,
        };
        weights.validate()?;
        Ok(weights)
    }

    pub(super) fn validate(&self) -> io::Result<()> {
        if self.schema != 1 || self.feature_schema != 1 {
            return Err(invalid("unsupported local weights schema"));
        }
        if !short_text(&self.name, 200)
            || !self.local_share.is_finite()
            || !(0.0..=1.0).contains(&self.local_share)
        {
            return Err(invalid("invalid local model name or mixture"));
        }
        for (head, name) in self.questions.iter().zip(QUESTION_NAMES) {
            if head.name != name {
                return Err(invalid("local questions have wrong names or order"));
            }
            check_head(&head.head)?;
        }
        check_head(&self.local_choice)?;
        let p = &self.provenance;
        if !short_text(&p.method, 256)
            || p.sources.is_empty()
            || p.sources.len() > 64
            || p.rights.is_empty()
            || p.rights.len() > 64
            || p.sources
                .iter()
                .chain(&p.rights)
                .any(|s| !short_text(s, 256))
            || p.rows > 10_000
        {
            return Err(invalid("invalid local weights provenance"));
        }
        Ok(())
    }

    pub fn local_share(&self) -> f32 {
        self.local_share
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn judge(&self, answers: &Answers) -> Judgement {
        let by_name = answers.by_name();
        let probabilities = std::array::from_fn(|i| by_name[QUESTION_NAMES[i]]);
        let features = self.center(probabilities);
        let probability = predict(&self.local_choice, &features);
        Judgement {
            probability,
            choice: attention::Choice::at(probability),
            from_rule: false,
        }
    }

    pub(super) fn probabilities(&self, features: &[f64; FEATURE_COUNT]) -> [f32; QUESTION_COUNT] {
        std::array::from_fn(|i| predict(&self.questions[i].head, features))
    }

    pub(super) fn center(&self, values: [f32; QUESTION_COUNT]) -> [f64; QUESTION_COUNT] {
        // Round both the answer and its neutral prior through the same f32
        // boundary. All-unknown signs therefore subtract to exactly zero.
        std::array::from_fn(|i| f64::from(values[i] - sigmoid(self.questions[i].head.bias) as f32))
    }
}

fn check_head<const N: usize>(head: &Head<N>) -> io::Result<()> {
    if !head.bias.is_finite()
        || head.bias.abs() > MOST_COEFFICIENT
        || head
            .weights
            .iter()
            .any(|w| !w.is_finite() || w.abs() > MOST_COEFFICIENT)
        || head.labels.yes.saturating_add(head.labels.no) > 10_000
        || (head.state == State::PriorOnly && head.weights.iter().any(|w| *w != 0.0))
        || (head.state == State::Fitted && (head.labels.yes == 0 || head.labels.no == 0))
        || (head.state == State::SeededUnfitted && (head.labels.yes != 0 || head.labels.no != 0))
    {
        return Err(invalid("invalid local coefficients or label counts"));
    }
    Ok(())
}

pub(super) fn short_text(value: &str, limit: usize) -> bool {
    !value.trim().is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}

pub(super) fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub(super) fn sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let exp = x.exp();
        exp / (1.0 + exp)
    }
}

pub(super) fn predict<const N: usize>(head: &Head<N>, features: &[f64; N]) -> f32 {
    sigmoid(
        head.bias
            + head
                .weights
                .iter()
                .zip(features)
                .map(|(w, x)| w * x)
                .sum::<f64>(),
    ) as f32
}

pub struct Local {
    weights: Weights,
    questions: Vec<(String, Question)>,
}

impl Local {
    pub fn new(weights: Weights) -> Self {
        Self {
            weights,
            questions: attention::questions(),
        }
    }

    pub fn weights(&self) -> &Weights {
        &self.weights
    }
}

impl Decider for Local {
    fn ask(
        &mut self,
        state: &str,
        questions: &[(String, Question)],
        deadline: Duration,
    ) -> Result<Asked, Failed> {
        let started = Instant::now();
        if deadline.is_zero() {
            return Err(Failed::TimedOut);
        }
        if questions != self.questions {
            return Err(Failed::Unreadable(
                "local judge requires the seven attention questions",
            ));
        }
        if state.len() > MOST_CUES_BYTES {
            return Err(Failed::Unreadable("local cues exceed 4 KiB"));
        }
        let cues: Cues =
            serde_json::from_str(state).map_err(|_| Failed::Unreadable("malformed local cues"))?;
        cues.validate().map_err(Failed::Unreadable)?;
        if started.elapsed() >= deadline {
            return Err(Failed::TimedOut);
        }
        let probabilities = self.weights.probabilities(&cues.features());
        let answers: BTreeMap<String, Answer> = QUESTION_NAMES
            .into_iter()
            .zip(probabilities)
            .map(|(name, p)| (name.to_owned(), Answer::Noul(p)))
            .collect();
        let model = self.weights.name.clone();
        let took = started.elapsed();
        if took >= deadline {
            return Err(Failed::TimedOut);
        }
        Ok(Asked {
            answers,
            model,
            took,
            usage: None,
            connection: None,
        })
    }

    fn name(&self) -> &str {
        self.weights.name()
    }
}
