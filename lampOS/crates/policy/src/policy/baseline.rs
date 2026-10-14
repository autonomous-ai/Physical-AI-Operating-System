//! A comparison with Voice's strict busy-room permission, without linking Voice.
//!
//! Audited source: `e07449e13`, `crates/voice/src/attention.rs`:
//! `first_output_busy`, `called` and `decide_busy`. This mirrors the binary
//! result with `strict = true`, including the per-request sure-name latch.
//! Request freshness only changes the rule's explanation. Re-audit
//! this copy when the Voice rule changes. A permission does not establish that
//! the provider answered or that a speaker played anything.
//!
//! A Policy snapshot is a counterfactual application of the rule. Reproducing
//! the actual Voice gate instead needs its first-output transcript prefix,
//! its own-turn flag, sure-name latch, 45-second conversation and newest fresh
//! *any*-face result. Policy's completed text, 20-second conversation and
//! near-face summary are not interchangeable with those observations.
use crate::attention::Choice;
use serde::{Deserialize, Deserializer, Serialize, de};

/// Voice retains at most this many UTF-8 bytes of a request's transcript.
pub const MOST_TEXT_BYTES: usize = 2_048;
/// The upstream revision whose permission semantics are mirrored here.
pub const SOURCE: &str = "e07449e13:crates/voice/src/attention.rs";
/// Voice's threshold for a name heard plainly enough to open a busy room.
pub const SURE_SCORE: f32 = 0.999;

/// Apply the score threshold to a declared Policy snapshot. This does not
/// reconstruct Voice's timestamp window or per-request latch. Missing or
/// invalid scores supply no positive evidence, not a measured negative cue.
pub fn sure_name(score: Option<f32>) -> bool {
    score.is_some_and(|score| (SURE_SCORE..=1.0).contains(&score))
}

/// Which evidence was supplied. Counts from these tracks are never pooled.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    /// Apply both decisions to a declared shared snapshot, not the live gate.
    SameSnapshot,
    /// Inputs actually available at Voice's first proposed output. The caller
    /// must establish their provenance; deserializing this tag cannot do so.
    VoiceFirstOutput,
}

/// What the baseline knew of the room, including its fail-open unknown case.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Room {
    Quiet,
    Busy,
    Unknown,
}

/// A complete, explicitly supplied snapshot of the strict rule's inputs.
///
/// Every field is required in JSON. For a live gate, `facing: null` records
/// its actual None argument, including when eyes were not queried. A false
/// argument can include a fresh message with only unknown poses or no faces.
/// An unrecorded argument instead makes the snapshot unavailable. For a
/// live-gate comparison, `going_on` must be Voice's value,
/// `named_sure` must be its retained per-request latch, and `text` must be its
/// prefix at first output, not a later transcript. Missing latch evidence
/// cannot be converted to false; omit the snapshot instead.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub kind: SnapshotKind,
    pub room: Room,
    pub own_turn: bool,
    pub going_on: bool,
    pub named_sure: bool,
    #[serde(deserialize_with = "bounded_text")]
    pub text: String,
    #[serde(deserialize_with = "required_facing")]
    pub facing: Option<bool>,
}

impl Snapshot {
    /// Validate a snapshot constructed directly, as well as one read as JSON.
    /// No truncation is performed: that would change the evidence being scored.
    pub fn validate(&self) -> Result<(), String> {
        if self.text.len() > MOST_TEXT_BYTES {
            return Err(format!(
                "baseline text exceeds {MOST_TEXT_BYTES} UTF-8 bytes"
            ));
        }
        Ok(())
    }
}

fn bounded_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let text = String::deserialize(deserializer)?;
    if text.len() > MOST_TEXT_BYTES {
        return Err(de::Error::custom(format!(
            "baseline text exceeds {MOST_TEXT_BYTES} UTF-8 bytes"
        )));
    }
    Ok(text)
}

// A plain Option field would also accept an omitted field. deserialize_with
// makes omission an error while still allowing an explicit JSON null.
fn required_facing<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<bool>, D::Error> {
    Option::<bool>::deserialize(deserializer)
}

/// Voice's exact positive-only transcript-name predicate. Unicode determines
/// word boundaries; matching is ASCII case-insensitive, without normalization.
/// Repeated greetings are skipped only before the first other word. This uses
/// no allocation and does not consult the local judge's weights or names.
pub fn called(heard: &str) -> bool {
    let mut words = heard
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty());
    let is = |word: &str, list: &[&str]| list.iter().any(|known| word.eq_ignore_ascii_case(known));
    if words.clone().any(|word| is(word, &["lamp", "lamb"])) {
        return true;
    }
    let first = words.find(|word| !is(word, &["hey", "hi", "ok", "okay", "oh", "hello"]));
    first.is_some_and(|word| {
        is(
            word,
            &[
                "lam", "lan", "milan", "man", "plan", "helen", "mom", "amen", "weiland",
            ],
        )
    })
}

/// Whether the busy rule permits a proposed output, not an observed action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    Play,
    Hold,
}

/// The binary strict-busy rule. Call `validate` before judging a snapshot
/// constructed directly; JSON decoding and [`Counts::note`] already check it.
pub fn judge(snapshot: &Snapshot) -> Permission {
    if snapshot.own_turn
        || snapshot.room != Room::Busy
        || snapshot.going_on
        || snapshot.named_sure
        || called(&snapshot.text)
        || snapshot.facing == Some(true)
    {
        Permission::Play
    } else {
        Permission::Hold
    }
}

/// A table row retains all three Policy choices, including both silent ones.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Choices {
    pub answer: u64,
    pub look_up: u64,
    pub stay_quiet: u64,
}

impl Choices {
    fn note(&mut self, choice: Choice) {
        let count = match choice {
            Choice::Answer => &mut self.answer,
            Choice::LookUp => &mut self.look_up,
            Choice::StayQuiet => &mut self.stay_quiet,
        };
        increment(count);
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Table {
    pub play: Choices,
    pub hold: Choices,
}

/// Counterfactual permission errors against supplied addressee labels. These
/// are not measured audible false answers or evidence of an actual missed call.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Errors {
    /// Permitted an answer on a labelled not-for-lamp row.
    pub false_answers: u64,
    /// Did not permit an answer on a labelled for-lamp row.
    pub missed_calls: u64,
}

impl Errors {
    fn note(&mut self, answers: bool, for_lamp: bool) {
        match (answers, for_lamp) {
            (true, false) => increment(&mut self.false_answers),
            (false, true) => increment(&mut self.missed_calls),
            _ => {}
        }
    }
}

/// Denominators shared by the two decisions on eligible paired rows.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Labels {
    pub for_lamp: u64,
    pub not_for_lamp: u64,
    pub unknown: u64,
    pub policy: Errors,
    pub baseline: Errors,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Track {
    pub compared: u64,
    pub table: Table,
    /// Play versus a silent Policy choice, or Hold versus Answer.
    pub permission_disagreements: u64,
    pub labels: Labels,
}

/// Coverage and comparisons, separated by evidence kind. Counters saturate.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Counts {
    pub rows: u64,
    pub missing_snapshot: u64,
    pub invalid_snapshot: u64,
    pub same_snapshot: Track,
    pub voice_first_output: Track,
}

impl Counts {
    /// Add one row. A label must be an independent addressee annotation,
    /// never inferred from either decision or from the lamp's response.
    /// `None` is unknown and adds no error. Authored fixture labels test the
    /// arithmetic only; callers must keep them separate from measured data.
    /// Missing or invalid baseline evidence is excluded from *both* paired
    /// error counts, so their positive and negative denominators match.
    pub fn note(&mut self, snapshot: Option<&Snapshot>, choice: Choice, label: Option<bool>) {
        increment(&mut self.rows);
        let Some(snapshot) = snapshot else {
            increment(&mut self.missing_snapshot);
            return;
        };
        if snapshot.validate().is_err() {
            increment(&mut self.invalid_snapshot);
            return;
        }
        let permission = judge(snapshot);
        let track = match snapshot.kind {
            SnapshotKind::SameSnapshot => &mut self.same_snapshot,
            SnapshotKind::VoiceFirstOutput => &mut self.voice_first_output,
        };
        increment(&mut track.compared);
        match permission {
            Permission::Play => track.table.play.note(choice),
            Permission::Hold => track.table.hold.note(choice),
        }
        let baseline_answers = permission == Permission::Play;
        let policy_answers = choice == Choice::Answer;
        if baseline_answers != policy_answers {
            increment(&mut track.permission_disagreements);
        }
        match label {
            Some(for_lamp) => {
                if for_lamp {
                    increment(&mut track.labels.for_lamp);
                } else {
                    increment(&mut track.labels.not_for_lamp);
                }
                track.labels.policy.note(policy_answers, for_lamp);
                track.labels.baseline.note(baseline_answers, for_lamp);
            }
            None => increment(&mut track.labels.unknown),
        }
    }
}

fn increment(count: &mut u64) {
    *count = count.saturating_add(1);
}
