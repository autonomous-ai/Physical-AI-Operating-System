//! Bounded input candidates before destructive turn replacement.
//!
//! This is sequencing and retention, not an echo/addressee classifier. The
//! directed harness explicitly accepts VAD-only evidence. No provider event or
//! session-scoped transcript is converted into a candidate decision here.
use crate::activity::{Activity, ObservedAudio, PRE_ROLL_BLOCKS, REACH_BACK_BLOCKS, START_BLOCKS};
use lamp_turn_owner::{BootId, MonoTime, Snapshot, TurnOwner};
use serde::Serialize;
use std::io;

pub const DECISION_BUDGET_US: u64 = 200_000;
pub const MAX_RETAINED_AGE_US: u64 = 600_000;
pub const MAX_EVIDENCE_AGE_US: u64 = 100_000;
pub const MAX_DECISION_BLOCKS: usize = 20;
/// The oldest a yield's first words may be when it is committed (#828):
/// the provider refuses audio older than [`lamp_gemini::MAX_INPUT_AGE`]
/// (2.5 s), and this leaves 0.5 s for the handoff. On a late but legal
/// decision the reviewer on #936 saw a lead 2.52 s old, and the provider
/// worker ended on it. Without first words kept nothing retained comes
/// near it.
pub const FIRST_WORDS_AGE_US: u64 = lamp_gemini::MAX_INPUT_AGE.as_micros() as u64 - 500_000;
pub const MAX_CANDIDATE_BLOCKS: usize = PRE_ROLL_BLOCKS + MAX_DECISION_BLOCKS;

/// How long a candidate may wait for its decision, and how much it may hold.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hold {
    pub decision_budget_us: u64,
    pub retained_age_us: u64,
    pub decision_blocks: usize,
    /// Blocks a candidate may hold before its 300 ms of pre-roll: a yield's
    /// first words, with `LAMP_FIRST_WORDS=kept` (#828). They count toward
    /// neither its deadline nor its retained age.
    pub lead_blocks: usize,
}
impl Hold {
    pub const STANDARD: Self = Self {
        decision_budget_us: DECISION_BUDGET_US,
        retained_age_us: MAX_RETAINED_AGE_US,
        decision_blocks: MAX_DECISION_BLOCKS,
        lead_blocks: 0,
    };
    /// Duplex holds a candidate that began over the lamp while the lamp
    /// falls silent and listens for more: at most 1.1 s after its trigger,
    /// with the 300 ms of pre-roll before it. Long enough for a breath after
    /// a first word ("Lamp, ... stop"), which 600 ms was not: on lamp-4ace
    /// the lamp went on speaking for 0.3 s between those two words. Still
    /// one candidate, one bounded block list.
    pub const DUPLEX: Self = Self {
        decision_budget_us: 1_200_000,
        retained_age_us: 1_600_000,
        decision_blocks: 120,
        lead_blocks: 0,
    };
    /// The same hold, with room for a yield's first words before its
    /// pre-roll, as far back as [`REACH_BACK_BLOCKS`] (#828).
    pub const fn keeping_first_words(self) -> Self {
        Self {
            lead_blocks: REACH_BACK_BLOCKS - PRE_ROLL_BLOCKS,
            ..self
        }
    }
    fn candidate_blocks(self) -> usize {
        self.lead_blocks + PRE_ROLL_BLOCKS + self.decision_blocks
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CaptureLineage {
    pub worker: BootId,
    pub epoch: u64,
    pub dsp_epoch: u64,
    pub privacy_generation: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub capture: CaptureLineage,
    pub authority: Snapshot,
}
impl Context {
    fn ready(self, at_us: u64) -> bool {
        self.capture.epoch > 0
            && self.capture.dsp_epoch > 0
            && self.capture.privacy_generation == self.authority.microphone_generation()
            && self.authority.listening_ready(MonoTime::from_micros(at_us))
    }
}

/// Immutable across additional captured blocks. Reset never reuses a serial.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CandidateId {
    controller: BootId,
    serial: u64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Candidate {
    pub id: CandidateId,
    pub capture: CaptureLineage,
    pub displaced_owner: Option<TurnOwner>,
    pub trigger_sequence: u64,
    pub trigger_read_at_us: u64,
    pub first_read_at_us: u64,
    pub decision_deadline_us: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptanceBasis {
    /// Explicitly opened qualification session; not social/echo recognition.
    DirectedSessionVadOnly,
    /// Opt-in own-playback suppression; still no addressee inference.
    DirectedSessionV1Baseline,
    /// Opt-in duplex hearing: echo evidence, a yield and speech that outlasts
    /// the lamp's own sound. Still no addressee inference.
    DirectedSessionDuplex,
    /// A caller's classifier decision, not a correctness guarantee. There is
    /// no production classifier wired to this variant in the directed runtime.
    ClassifierDecision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    Policy,
    Deadline,
    Capacity,
    CaptureChanged,
    AuthorityChanged,
    InputUnavailable,
    RetainedHistoryUnavailable,
}

#[derive(Clone, Copy, Debug)]
pub enum Verdict {
    Accept(AcceptanceBasis),
    Reject,
}

/// Evidence must be explicitly correlated before construction. `through_sequence`
/// identifies retained audio; receipt time cannot freshen old observations.
#[derive(Clone, Copy, Debug)]
pub struct Evidence {
    pub candidate: CandidateId,
    pub through_sequence: u64,
    pub produced_at_us: u64,
    pub verdict: Verdict,
}

#[derive(Debug)]
pub struct Accepted {
    pub candidate: Candidate,
    pub basis: AcceptanceBasis,
    pub audio: Vec<ObservedAudio>,
    pub ended: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Rejection {
    pub candidate: CandidateId,
    pub reason: RejectReason,
}

#[derive(Debug)]
pub enum Decision {
    Accepted(Accepted),
    Rejected(Rejection),
    /// Old, future, unbound or already consumed evidence has no authority.
    Ignored,
}

#[derive(Debug)]
pub enum Step {
    None,
    Candidate(Candidate),
    Audio(ObservedAudio),
    End(ObservedAudio),
    Rejected(Rejection),
}

/// At most one older candidate can expire before this activity is handled.
/// Retain both events when an ended candidate expires as a new utterance starts.
#[derive(Debug)]
pub struct Update {
    pub rejection: Option<Rejection>,
    pub step: Step,
}

struct Pending {
    candidate: Candidate,
    expected_generation: u64,
    expected_owner: Option<TurnOwner>,
    audio: Vec<ObservedAudio>,
    ended: bool,
    decision_blocks: usize,
}

#[derive(Clone, Copy)]
struct Streaming {
    capture: CaptureLineage,
    last_sequence: u64,
    last_read_at_us: u64,
}

enum Phase {
    Idle,
    Pending(Pending),
    Streaming(Streaming),
    // An accepted stream keeps its hard lineage/continuity checks even when
    // its PCM is intentionally discarded. Policy rejection retains no stream.
    Draining(Option<Streaming>),
}

/// Owns at most 50 original 10 ms PCM blocks, or 100 in duplex mode. It never calls the controller,
/// cancels speech, sends provider input, or grants a listening/actuator cue.
pub struct InputAdmission {
    phase: Phase,
    hand_abandoned: bool,
    serial: u64,
    last_now_us: u64,
    hold: Hold,
}
impl Default for InputAdmission {
    fn default() -> Self {
        Self::with_hold(Hold::STANDARD)
    }
}

impl InputAdmission {
    pub fn with_hold(hold: Hold) -> Self {
        Self {
            phase: Phase::Idle,
            hand_abandoned: false,
            serial: 0,
            last_now_us: 0,
            hold,
        }
    }

    /// Reset alongside the detector after cancellation/failure. Keep identity
    /// high-water marks so a delayed decision cannot target a later candidate.
    pub fn reset(&mut self) {
        self.phase = Phase::Idle;
        self.hand_abandoned = false;
    }

    /// Discard an accepted, unsubmitted utterance through its detector endpoint.
    /// The caller owns and must drop `Accepted.audio`; this state keeps no PCM.
    /// Recovery must not reset the detector or rearm halfway through this input.
    /// Pending candidates require an explicit decision and are left unchanged.
    pub fn discard_stream_until_end(&mut self, at_us: u64) -> io::Result<()> {
        self.advance(at_us)?;
        match &self.phase {
            Phase::Streaming(stream) => self.phase = Phase::Draining(Some(*stream)),
            Phase::Pending(_) => {
                return Err(io::Error::other("cannot discard an undecided candidate"));
            }
            Phase::Idle | Phase::Draining(_) => {}
        }
        Ok(())
    }

    /// A local Stop abandons even an undecided utterance. Preserve its endpoint
    /// boundary, so the remainder cannot become another request after the Stop.
    pub(crate) fn abandon_until_end(&mut self, at_us: u64) -> io::Result<()> {
        self.advance(at_us)?;
        match &self.phase {
            Phase::Pending(_) => {
                self.reject(RejectReason::Policy);
            }
            Phase::Streaming(stream) => self.phase = Phase::Draining(Some(*stream)),
            Phase::Idle | Phase::Draining(_) => {}
        }
        self.hand_abandoned = matches!(self.phase, Phase::Draining(_));
        Ok(())
    }

    /// Discard only prospective input. Never erase an already accepted stream.
    pub fn suppress_prospective(
        &mut self,
        context: Context,
        at_us: u64,
    ) -> io::Result<Option<Rejection>> {
        self.advance(at_us)?;
        if !context.ready(at_us) {
            return Err(io::Error::other(
                "suppressed input lost hard capture authority",
            ));
        }
        match &self.phase {
            Phase::Streaming(_) | Phase::Draining(Some(_)) => {
                return Err(io::Error::other("cannot suppress admitted input"));
            }
            _ => {}
        }
        let previous = std::mem::replace(&mut self.phase, Phase::Idle);
        Ok(match previous {
            Phase::Pending(p) => Some(Rejection {
                candidate: p.candidate.id,
                reason: RejectReason::Policy,
            }),
            _ => None,
        })
    }
    /// Let a prospective candidate go because the caller is keeping the turn
    /// it has. Unlike [`Self::suppress_prospective`] this consults no
    /// authority: nothing is admitted and nothing accepted is touched, so
    /// there is nothing for a stale snapshot to invalidate. (The first use
    /// of the suppressing call for this, with a snapshot issued a moment
    /// after the time it was checked against, ended the service twice on
    /// lamp-4ace.)
    /// Whether the candidate in hand has ended: its sound is over, and it
    /// waits only for its decision (#1123).
    pub fn pending_ended(&self) -> bool {
        matches!(&self.phase, Phase::Pending(pending) if pending.ended)
    }

    pub fn release_prospective(&mut self, at_us: u64) -> io::Result<Option<Rejection>> {
        self.advance(at_us)?;
        match &self.phase {
            Phase::Streaming(_) | Phase::Draining(Some(_)) => {
                Err(io::Error::other("cannot release admitted input"))
            }
            Phase::Pending(_) => {
                let Phase::Pending(pending) = std::mem::replace(&mut self.phase, Phase::Idle)
                else {
                    unreachable!("matched pending above");
                };
                Ok(Some(Rejection {
                    candidate: pending.candidate.id,
                    reason: RejectReason::Policy,
                }))
            }
            Phase::Idle | Phase::Draining(None) => Ok(None),
        }
    }
    pub fn owns_stream(&self) -> bool {
        matches!(self.phase, Phase::Streaming(_) | Phase::Draining(Some(_)))
    }
    /// Due reminders must yield even to a prospective utterance that
    /// has not acquired a turn yet. Draining is not idle input either.
    pub(crate) fn idle_for_reminder(&self) -> bool {
        matches!(self.phase, Phase::Idle) && !self.hand_abandoned
    }
    pub(crate) fn draining(&self) -> bool {
        self.hand_abandoned && matches!(self.phase, Phase::Draining(_))
    }
    /// Original bounded prospective audio, never an accepted stream.
    pub(crate) fn pending_audio(&self, candidate: CandidateId) -> Option<&[ObservedAudio]> {
        match &self.phase {
            Phase::Pending(p) if p.candidate.id == candidate => Some(&p.audio),
            _ => None,
        }
    }
    /// The last block of `candidate`'s lead of first words captured before
    /// `oldest_us`, if any: never one of its usual pre-roll or onset. What
    /// [`FIRST_WORDS_AGE_US`] drops before a commit (#828, #936).
    pub(crate) fn lead_older_than(&self, candidate: CandidateId, oldest_us: u64) -> Option<u64> {
        let Phase::Pending(pending) = &self.phase else {
            return None;
        };
        if pending.candidate.id != candidate {
            return None;
        }
        let lead = pending
            .audio
            .len()
            .saturating_sub(PRE_ROLL_BLOCKS + pending.decision_blocks);
        pending.audio[..lead]
            .iter()
            .take_while(|frame| frame.captured_at_us < oldest_us)
            .last()
            .map(|frame| frame.sequence)
    }
    /// Keep one contiguous safe suffix without renewing identity, deadline,
    /// retention age or decision-block budget. The entire triggering onset
    /// must survive; callers reject the candidate if its onset is masked.
    pub(crate) fn discard_pending_prefix(
        &mut self,
        candidate: CandidateId,
        through_sequence: u64,
    ) -> io::Result<usize> {
        let Phase::Pending(pending) = &mut self.phase else {
            return Err(io::Error::other("prefix has no pending candidate"));
        };
        let first_onset = pending
            .candidate
            .trigger_sequence
            .checked_sub(u64::from(START_BLOCKS) - 1)
            .ok_or_else(|| io::Error::other("candidate has no complete onset"))?;
        if pending.candidate.id != candidate || through_sequence >= first_onset {
            return Err(io::Error::other(
                "cannot discard candidate onset or stale prefix",
            ));
        }
        let count = pending
            .audio
            .iter()
            .position(|frame| frame.sequence == through_sequence)
            .map(|index| index + 1)
            .ok_or_else(|| io::Error::other("prefix cutoff is not retained"))?;
        if pending.audio.len() - count < usize::from(START_BLOCKS) {
            return Err(io::Error::other("prefix removal would lose fresh onset"));
        }
        pending.audio.drain(..count);
        Ok(count)
    }
    /// Original latest retained block, not a freshened trigger timestamp.
    pub fn pending_evidence(
        &self,
        at_us: u64,
        basis: AcceptanceBasis,
    ) -> Option<(Candidate, Evidence)> {
        let Phase::Pending(pending) = &self.phase else {
            return None;
        };
        let last = pending.audio.last()?;
        Some((
            pending.candidate,
            Evidence {
                candidate: pending.candidate.id,
                through_sequence: last.sequence,
                produced_at_us: at_us,
                verdict: Verdict::Accept(basis),
            },
        ))
    }

    pub fn observe(
        &mut self,
        activity: Activity,
        context: Context,
        at_us: u64,
    ) -> io::Result<Update> {
        let result = self.observe_inner(activity, context, at_us);
        if result.is_err() {
            self.reset();
        }
        result
    }

    fn observe_inner(
        &mut self,
        activity: Activity,
        context: Context,
        at_us: u64,
    ) -> io::Result<Update> {
        let rejected = self.maintain(context, at_us)?;
        let step = match activity {
            Activity::Fault(reason) => {
                self.reset();
                return Err(io::Error::other(reason));
            }
            Activity::Quiet if matches!(self.phase, Phase::Idle) => Step::None,
            Activity::Quiet if matches!(&self.phase, Phase::Pending(pending) if pending.ended) => {
                Step::None
            }
            Activity::Start(audio) if matches!(self.phase, Phase::Idle) => {
                self.hand_abandoned = false;
                if !context.ready(at_us) {
                    return Err(io::Error::other("candidate input is unavailable"));
                }
                validate_prefix(&audio, self.hold.lead_blocks, at_us)?;
                let first = &audio[0];
                // A lead of first words counts toward neither the deadline
                // nor the retained age: those run from the usual pre-roll.
                let onset = &audio[audio.len().saturating_sub(PRE_ROLL_BLOCKS)];
                let trigger = audio.last().expect("validated nonempty prefix");
                self.serial = self
                    .serial
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("candidate serial exhausted"))?;
                let candidate = Candidate {
                    id: CandidateId {
                        controller: context.authority.boot(),
                        serial: self.serial,
                    },
                    capture: context.capture,
                    displaced_owner: context.authority.owner(),
                    trigger_sequence: trigger.sequence,
                    trigger_read_at_us: trigger.captured_at_us,
                    first_read_at_us: first.captured_at_us,
                    decision_deadline_us: trigger
                        .captured_at_us
                        .saturating_add(self.hold.decision_budget_us)
                        .min(
                            onset
                                .captured_at_us
                                .saturating_add(self.hold.retained_age_us),
                        ),
                };
                let mut retained = Vec::with_capacity(self.hold.candidate_blocks());
                retained.extend(audio);
                self.phase = Phase::Pending(Pending {
                    candidate,
                    expected_generation: context.authority.generation(),
                    expected_owner: context.authority.owner(),
                    audio: retained,
                    ended: false,
                    decision_blocks: 0,
                });
                if let Some(rejection) = self.maintain(context, at_us)? {
                    Step::Rejected(rejection)
                } else {
                    Step::Candidate(candidate)
                }
            }
            Activity::Continue(audio) => self.continue_audio(audio, false, context, at_us)?,
            Activity::End(audio) => self.continue_audio(audio, true, context, at_us)?,
            _ => return Err(io::Error::other("activity contradicts admission state")),
        };
        Ok(Update {
            rejection: rejected,
            step,
        })
    }

    /// Call every control tick, even when capture/evidence is quiet. Incoming
    /// frames and evidence never extend the original decision deadline.
    pub fn maintain(&mut self, context: Context, at_us: u64) -> io::Result<Option<Rejection>> {
        self.advance(at_us)?;
        if self.draining() && !context.ready(at_us) {
            self.reset();
            return Err(io::Error::other(
                "hand-abandoned input lost capture authority",
            ));
        }
        if let Phase::Streaming(stream) | Phase::Draining(Some(stream)) = &self.phase {
            // A planned AEC reset may advance DSP epoch without dropping the
            // accepted utterance. Capture/privacy incarnation must not change.
            if !context.ready(at_us)
                || context.capture.worker != stream.capture.worker
                || context.capture.epoch != stream.capture.epoch
                || context.capture.privacy_generation != stream.capture.privacy_generation
                || context.capture.dsp_epoch < stream.capture.dsp_epoch
            {
                self.reset();
                return Err(io::Error::other("accepted capture lineage changed"));
            }
        }
        let Phase::Pending(pending) = &self.phase else {
            return Ok(None);
        };
        let reason = if !context.ready(at_us) {
            Some(RejectReason::InputUnavailable)
        } else if pending.candidate.capture != context.capture {
            Some(RejectReason::CaptureChanged)
        } else if pending.candidate.id.controller != context.authority.boot()
            || pending.expected_generation != context.authority.generation()
            || pending.expected_owner != context.authority.owner()
        {
            Some(RejectReason::AuthorityChanged)
        } else if at_us >= pending.candidate.decision_deadline_us {
            Some(RejectReason::Deadline)
        } else {
            None
        };
        Ok(reason.map(|reason| self.reject(reason)))
    }

    /// Retire only this still-prospective candidate after original read history
    /// expires or is evicted internally. Hard authority and deadline checks retain priority;
    /// stale identities cannot discard a successor or erase admitted audio.
    pub(crate) fn reject_retained_history(
        &mut self,
        candidate: CandidateId,
        context: Context,
        at_us: u64,
    ) -> io::Result<Option<Rejection>> {
        if let Some(rejected) = self.maintain(context, at_us)? {
            return Ok(Some(rejected));
        }
        if matches!(&self.phase, Phase::Pending(p) if p.candidate.id == candidate) {
            Ok(Some(self.reject(RejectReason::RetainedHistoryUnavailable)))
        } else {
            Ok(None)
        }
    }

    /// Natural completion is an explicit coordinator fact, not an inference
    /// from an absent owner. It can turn a pending interruption into a new turn.
    pub fn owner_completed(
        &mut self,
        completed: TurnOwner,
        authority: Snapshot,
        at_us: u64,
    ) -> io::Result<()> {
        self.advance(at_us)?;
        if let Phase::Pending(pending) = &mut self.phase
            && pending.expected_owner == Some(completed)
            && authority.boot() == pending.candidate.id.controller
            && authority.generation().checked_sub(1) == Some(pending.expected_generation)
            && authority.owner().is_none()
            && authority.microphone_generation() == pending.candidate.capture.privacy_generation
        {
            pending.expected_generation = authority.generation();
            pending.expected_owner = authority.owner();
        }
        Ok(())
    }

    pub fn decide(
        &mut self,
        evidence: Evidence,
        context: Context,
        at_us: u64,
    ) -> io::Result<Decision> {
        if let Some(rejected) = self.maintain(context, at_us)? {
            return Ok(Decision::Rejected(rejected));
        }
        let Phase::Pending(pending) = &self.phase else {
            return Ok(Decision::Ignored);
        };
        if evidence.candidate != pending.candidate.id
            || evidence.through_sequence < pending.candidate.trigger_sequence
            || evidence.produced_at_us > at_us
        {
            return Ok(Decision::Ignored);
        }
        let Some(frame) = pending
            .audio
            .iter()
            .find(|frame| frame.sequence == evidence.through_sequence)
        else {
            return Ok(Decision::Ignored);
        };
        if evidence.produced_at_us < frame.captured_at_us
            || at_us
                .checked_sub(frame.captured_at_us)
                .is_none_or(|age| age >= MAX_EVIDENCE_AGE_US)
        {
            return Ok(Decision::Ignored);
        }
        let Verdict::Accept(basis) = evidence.verdict else {
            return Ok(Decision::Rejected(self.reject(RejectReason::Policy)));
        };
        let Phase::Pending(pending) = std::mem::replace(&mut self.phase, Phase::Idle) else {
            unreachable!()
        };
        if !pending.ended {
            let last = pending.audio.last().expect("nonempty candidate");
            self.phase = Phase::Streaming(Streaming {
                capture: pending.candidate.capture,
                last_sequence: last.sequence,
                last_read_at_us: last.captured_at_us,
            });
        }
        Ok(Decision::Accepted(Accepted {
            candidate: pending.candidate,
            basis,
            audio: pending.audio,
            ended: pending.ended,
        }))
    }

    fn continue_audio(
        &mut self,
        audio: ObservedAudio,
        ended: bool,
        context: Context,
        at_us: u64,
    ) -> io::Result<Step> {
        let discard = matches!(self.phase, Phase::Draining(_));
        let hold = self.hold;
        match &mut self.phase {
            Phase::Pending(pending) => {
                if pending.ended {
                    return Err(io::Error::other("audio after candidate endpoint"));
                }
                validate_next(
                    pending.audio.last().expect("nonempty candidate"),
                    &audio,
                    at_us,
                )?;
                if pending.decision_blocks >= hold.decision_blocks
                    || pending.audio.len() >= hold.candidate_blocks()
                {
                    let rejection = self.reject(RejectReason::Capacity);
                    if ended {
                        self.phase = Phase::Idle;
                    }
                    return Ok(Step::Rejected(rejection));
                }
                pending.audio.push(audio);
                pending.ended = ended;
                pending.decision_blocks += 1;
                Ok(Step::None)
            }
            Phase::Streaming(stream) | Phase::Draining(Some(stream)) => {
                if stream.last_sequence.checked_add(1) != Some(audio.sequence)
                    || audio.captured_at_us < stream.last_read_at_us
                    || audio.captured_at_us.saturating_sub(stream.last_read_at_us) > 50_000
                    || at_us
                        .checked_sub(audio.captured_at_us)
                        .is_none_or(|age| age >= MAX_EVIDENCE_AGE_US)
                {
                    return Err(io::Error::other("accepted capture discontinuity"));
                }
                stream.last_sequence = audio.sequence;
                stream.last_read_at_us = audio.captured_at_us;
                stream.capture = context.capture;
                if ended {
                    self.phase = Phase::Idle;
                }
                if discard {
                    Ok(Step::None)
                } else if ended {
                    Ok(Step::End(audio))
                } else {
                    Ok(Step::Audio(audio))
                }
            }
            Phase::Draining(None) => {
                if ended {
                    self.phase = Phase::Idle;
                }
                Ok(Step::None)
            }
            Phase::Idle => Err(io::Error::other("audio has no candidate or admitted owner")),
        }
    }

    fn reject(&mut self, reason: RejectReason) -> Rejection {
        let Phase::Pending(pending) = std::mem::replace(&mut self.phase, Phase::Draining(None))
        else {
            unreachable!()
        };
        if pending.ended {
            self.phase = Phase::Idle;
        }
        Rejection {
            candidate: pending.candidate.id,
            reason,
        }
    }

    fn advance(&mut self, at_us: u64) -> io::Result<()> {
        if at_us < self.last_now_us {
            self.reset();
            return Err(io::Error::other("admission clock moved backwards"));
        }
        self.last_now_us = at_us;
        Ok(())
    }
}

fn validate_prefix(audio: &[ObservedAudio], lead_blocks: usize, at_us: u64) -> io::Result<()> {
    if audio.is_empty()
        || audio.len() > PRE_ROLL_BLOCKS + lead_blocks
        || audio[0].sequence == 0
        || audio[0].captured_at_us == 0
    {
        return Err(io::Error::other("invalid candidate prefix"));
    }
    for frames in audio.windows(2) {
        validate_next(&frames[0], &frames[1], at_us)?;
    }
    if at_us
        .checked_sub(audio.last().expect("nonempty prefix").captured_at_us)
        .is_none_or(|age| age >= MAX_EVIDENCE_AGE_US)
    {
        return Err(io::Error::other("candidate trigger is stale or future"));
    }
    Ok(())
}

fn validate_next(previous: &ObservedAudio, next: &ObservedAudio, at_us: u64) -> io::Result<()> {
    if previous.sequence.checked_add(1) != Some(next.sequence)
        || next.captured_at_us < previous.captured_at_us
        || next.captured_at_us.saturating_sub(previous.captured_at_us) > 50_000
        || next.captured_at_us > at_us
    {
        return Err(io::Error::other("candidate capture discontinuity"));
    }
    Ok(())
}

#[cfg(test)]
mod outage_drain_tests {
    use super::*;
    use crate::activity::{END_SILENCE_BLOCKS, START_BLOCKS, TurnDetector};
    use lamp_turn_owner::{AdmissionState, CaptureState, Controller, Permission};

    const START: u64 = 1_000_000;

    #[test]
    fn prospective_speech_blocks_a_reminder_before_it_owns_a_stream() {
        let mut gate = InputAdmission::default();
        let mut local = LocalInput::new();
        assert!(gate.idle_for_reminder());
        let _ = candidate(&mut gate, local.context(START), 1, START);
        assert!(
            !gate.owns_stream(),
            "a pending candidate is not admitted audio"
        );
        assert!(
            !gate.idle_for_reminder(),
            "a due reminder must yield to that candidate"
        );
        gate.abandon_until_end(START + 1).unwrap();
        assert!(
            !gate.idle_for_reminder(),
            "draining for a hand stop cannot admit a reminder"
        );
    }

    fn at(us: u64) -> MonoTime {
        MonoTime::from_micros(us)
    }

    struct LocalInput {
        controller: Controller,
        capture: CaptureLineage,
    }

    impl LocalInput {
        fn new() -> Self {
            let mut controller = Controller::new(BootId::new([1; 16]).unwrap(), at(START));
            controller
                .set_microphone_permission(at(START), Permission::Allowed)
                .unwrap();
            let privacy_generation = controller
                .snapshot(at(START))
                .unwrap()
                .microphone_generation();
            Self {
                controller,
                capture: CaptureLineage {
                    worker: BootId::new([2; 16]).unwrap(),
                    epoch: 1,
                    dsp_epoch: 2,
                    privacy_generation,
                },
            }
        }

        fn context(&mut self, now: u64) -> Context {
            self.controller
                .set_capture(at(now), CaptureState::RetainingUntil(at(now + 100_000)))
                .unwrap();
            self.controller
                .set_admission(at(now), AdmissionState::OpenUntil(at(now + 100_000)))
                .unwrap();
            Context {
                capture: self.capture,
                authority: self.controller.snapshot(at(now)).unwrap(),
            }
        }
    }

    fn audio(sequence: u64, now: u64) -> ObservedAudio {
        ObservedAudio {
            sequence,
            captured_at_us: now,
            samples: [7; lamp_audio::CAPTURE_SAMPLES],
        }
    }

    fn candidate(
        gate: &mut InputAdmission,
        context: Context,
        sequence: u64,
        now: u64,
    ) -> Candidate {
        let update = gate
            .observe(Activity::Start(vec![audio(sequence, now)]), context, now)
            .unwrap();
        assert!(update.rejection.is_none());
        let Step::Candidate(candidate) = update.step else {
            panic!("expected a new candidate, got {:?}", update.step);
        };
        candidate
    }

    fn evidence(candidate: Candidate, sequence: u64, now: u64) -> Evidence {
        Evidence {
            candidate: candidate.id,
            through_sequence: sequence,
            produced_at_us: now,
            verdict: Verdict::Accept(AcceptanceBasis::DirectedSessionVadOnly),
        }
    }

    fn accept(
        gate: &mut InputAdmission,
        context: Context,
        evidence: Evidence,
        now: u64,
    ) -> Accepted {
        let Decision::Accepted(accepted) = gate.decide(evidence, context, now).unwrap() else {
            panic!("expected accepted input");
        };
        accepted
    }

    fn accepted_stream() -> (InputAdmission, LocalInput, Evidence) {
        let mut local = LocalInput::new();
        let mut gate = InputAdmission::default();
        let first = candidate(&mut gate, local.context(START), 1, START);
        let decision = evidence(first, 1, START);
        drop(accept(&mut gate, local.context(START), decision, START));
        (gate, local, decision)
    }

    fn no_output(update: Update) {
        assert!(update.rejection.is_none());
        assert!(matches!(update.step, Step::None), "{:?}", update.step);
    }

    #[test]
    fn a_pending_candidate_can_be_let_go_without_authority_and_an_accepted_stream_cannot() {
        let mut local = LocalInput::new();
        let mut gate = InputAdmission::default();
        let first = candidate(&mut gate, local.context(START), 1, START);
        // No snapshot is asked for: one issued after this instant, as the
        // coordinator's was, cannot make letting go a fault.
        let let_go = gate.release_prospective(START + 1_000).unwrap().unwrap();
        assert_eq!(let_go.candidate, first.id);
        assert_eq!(let_go.reason, RejectReason::Policy);
        assert!(gate.release_prospective(START + 2_000).unwrap().is_none());
        // The next start is a new candidate with its own identity.
        let later = START + 500_000;
        let second = candidate(&mut gate, local.context(later), 60, later);
        assert_ne!(second.id, first.id);
        // What has been accepted is not prospective.
        let (mut streaming, _local, _) = accepted_stream();
        assert!(streaming.release_prospective(START + 1_000).is_err());
        // A clock that went backwards is still a fault.
        assert!(gate.release_prospective(START).is_err());
    }

    #[test]
    fn baseline_streaming_or_reset_cannot_silently_discard_an_accepted_tail() {
        let (mut gate, mut local, _) = accepted_stream();
        let now = START + 10_000;
        let update = gate
            .observe(Activity::Continue(audio(2, now)), local.context(now), now)
            .unwrap();
        assert!(matches!(update.step, Step::Audio(_)));
        gate.reset();
        let now = now + 10_000;
        assert!(
            gate.observe(Activity::Continue(audio(3, now)), local.context(now), now)
                .is_err()
        );
    }

    #[test]
    fn discarded_stream_waits_for_detector_end_across_recovery_and_keeps_fresh_start() {
        let mut gate = InputAdmission::default();
        let mut local = LocalInput::new();
        let mut detector = TurnDetector::default();
        let mut first = None;
        // The detector, including its active utterance, is never reset here.
        for sequence in 1..=300 {
            let now = START + (sequence - 1) * 10_000;
            let update = gate
                .observe(
                    detector.push(audio(sequence, now), 0.95),
                    local.context(now),
                    now,
                )
                .unwrap();
            if sequence == u64::from(START_BLOCKS) {
                let Step::Candidate(initial) = update.step else {
                    panic!("missing first candidate");
                };
                let decision = evidence(initial, sequence, now);
                let accepted = accept(&mut gate, local.context(now), decision, now);
                assert_eq!(accepted.audio.len(), usize::from(START_BLOCKS));
                assert!(!accepted.ended);
                // This prefix belongs to the caller, not to the drain state.
                drop(accepted);
                gate.discard_stream_until_end(now).unwrap();
                first = Some(decision);
            } else {
                no_output(update);
            }
            if sequence > u64::from(START_BLOCKS) {
                assert!(matches!(
                    gate.decide(first.unwrap(), local.context(now), now)
                        .unwrap(),
                    Decision::Ignored
                ));
                // A coordinator may now be recovered; repeated calls must not
                // rearm admission or extend any capture/authority lease.
                if sequence == 100 || sequence == 200 {
                    gate.discard_stream_until_end(now).unwrap();
                }
            }
        }
        for index in 1..=u64::from(END_SILENCE_BLOCKS) {
            let sequence = 300 + index;
            let now = START + (sequence - 1) * 10_000;
            let activity = detector.push(audio(sequence, now), 0.0);
            assert_eq!(
                matches!(activity, Activity::End(_)),
                index == u64::from(END_SILENCE_BLOCKS)
            );
            no_output(gate.observe(activity, local.context(now), now).unwrap());
        }
        let mut fresh = None;
        for index in 1..=u64::from(START_BLOCKS) {
            let sequence = 300 + u64::from(END_SILENCE_BLOCKS) + index;
            let now = START + (sequence - 1) * 10_000;
            let update = gate
                .observe(
                    detector.push(audio(sequence, now), 0.95),
                    local.context(now),
                    now,
                )
                .unwrap();
            if index == u64::from(START_BLOCKS) {
                let Step::Candidate(next) = update.step else {
                    panic!("fresh beginning was lost");
                };
                assert_ne!(next.id, first.unwrap().candidate);
                assert!(matches!(
                    gate.decide(first.unwrap(), local.context(now), now)
                        .unwrap(),
                    Decision::Ignored
                ));
                fresh = Some(accept(
                    &mut gate,
                    local.context(now),
                    evidence(next, sequence, now),
                    now,
                ));
            } else {
                no_output(update);
            }
            assert!(local.context(now).authority.owner().is_none());
        }
        let accepted = fresh.unwrap();
        assert_eq!(accepted.audio.len(), usize::from(START_BLOCKS));
        assert_eq!(
            accepted.audio[0].sequence,
            301 + u64::from(END_SILENCE_BLOCKS)
        );
    }

    #[test]
    fn already_ended_input_and_idle_discard_are_noops() {
        let mut gate = InputAdmission::default();
        let mut local = LocalInput::new();
        gate.discard_stream_until_end(START).unwrap();
        let first = candidate(&mut gate, local.context(START), 1, START);
        let now = START + 10_000;
        no_output(
            gate.observe(Activity::End(audio(2, now)), local.context(now), now)
                .unwrap(),
        );
        let accepted = accept(&mut gate, local.context(now), evidence(first, 2, now), now);
        assert!(accepted.ended);
        assert_eq!(accepted.audio.len(), 2);
        drop(accepted);
        gate.discard_stream_until_end(now).unwrap();
        gate.discard_stream_until_end(now).unwrap();
        let next = candidate(&mut gate, local.context(now + 10_000), 3, now + 10_000);
        assert_ne!(next.id, first.id);
    }

    #[test]
    fn pending_discard_errors_without_consuming_candidate_or_renewing_deadline() {
        let mut gate = InputAdmission::default();
        let mut local = LocalInput::new();
        let first = candidate(&mut gate, local.context(START), 1, START);
        assert!(gate.discard_stream_until_end(START + 1).is_err());
        let Phase::Pending(pending) = &gate.phase else {
            panic!("discard silently consumed an undecided candidate");
        };
        assert_eq!(pending.audio.len(), 1);
        assert_eq!(
            pending.candidate.decision_deadline_us,
            first.decision_deadline_us
        );
        let deadline = first.decision_deadline_us;
        assert!(gate.discard_stream_until_end(deadline).is_err());
        assert!(matches!(
            gate.maintain(local.context(deadline), deadline).unwrap(),
            Some(Rejection {
                reason: RejectReason::Deadline,
                ..
            })
        ));
        assert!(matches!(
            gate.decide(
                evidence(first, 1, deadline),
                local.context(deadline),
                deadline
            )
            .unwrap(),
            Decision::Ignored
        ));
    }

    #[test]
    fn policy_rejection_remains_draining_and_idempotent() {
        let mut gate = InputAdmission::default();
        let mut local = LocalInput::new();
        let first = candidate(&mut gate, local.context(START), 1, START);
        assert!(matches!(
            gate.decide(
                Evidence {
                    verdict: Verdict::Reject,
                    ..evidence(first, 1, START)
                },
                local.context(START),
                START
            )
            .unwrap(),
            Decision::Rejected(_)
        ));
        gate.discard_stream_until_end(START).unwrap();
        gate.discard_stream_until_end(START + 1).unwrap();
        let now = START + 10_000;
        no_output(
            gate.observe(Activity::Continue(audio(2, now)), local.context(now), now)
                .unwrap(),
        );
        no_output(
            gate.observe(
                Activity::End(audio(3, now + 10_000)),
                local.context(now + 10_000),
                now + 10_000,
            )
            .unwrap(),
        );
        let next = candidate(&mut gate, local.context(now + 20_000), 4, now + 20_000);
        assert_ne!(first.id, next.id);
    }

    #[test]
    fn discarded_stream_preserves_hard_authority_and_capture_lineage_checks() {
        for case in 0..7 {
            let (mut gate, mut local, old) = accepted_stream();
            gate.discard_stream_until_end(START).unwrap();
            let now = START + 10_000;
            let mut context = local.context(now);
            match case {
                0 => context.capture.worker = BootId::new([3; 16]).unwrap(),
                1 => context.capture.epoch += 1,
                2 => context.capture.privacy_generation += 1,
                3 => context.capture.dsp_epoch -= 1,
                4 => {
                    local
                        .controller
                        .set_microphone_permission(at(now), Permission::Denied)
                        .unwrap();
                    context.authority = local.controller.snapshot(at(now)).unwrap();
                }
                5 => {
                    local
                        .controller
                        .set_capture(at(now), CaptureState::Unavailable)
                        .unwrap();
                    context.authority = local.controller.snapshot(at(now)).unwrap();
                }
                6 => {
                    local
                        .controller
                        .set_admission(at(now), AdmissionState::Unavailable)
                        .unwrap();
                    context.authority = local.controller.snapshot(at(now)).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                gate.observe(Activity::Continue(audio(2, now)), context, now)
                    .is_err(),
                "case {case}"
            );
            assert!(matches!(gate.phase, Phase::Idle));
            assert!(matches!(
                gate.decide(old, context, now).unwrap(),
                Decision::Ignored
            ));
        }
    }

    #[test]
    fn planned_dsp_advance_is_allowed_but_its_drain_cannot_regress() {
        let (mut gate, mut local, _) = accepted_stream();
        gate.discard_stream_until_end(START).unwrap();
        local.capture.dsp_epoch += 1;
        let now = START + 10_000;
        no_output(
            gate.observe(Activity::Continue(audio(2, now)), local.context(now), now)
                .unwrap(),
        );
        local.capture.dsp_epoch -= 1;
        assert!(
            gate.observe(
                Activity::Continue(audio(3, now + 10_000)),
                local.context(now + 10_000),
                now + 10_000
            )
            .is_err()
        );
    }

    #[test]
    fn discarded_stream_keeps_original_sequence_and_capture_time_bounds() {
        for case in 0..5 {
            let (mut gate, mut local, _) = accepted_stream();
            gate.discard_stream_until_end(START).unwrap();
            let now = START + 10_000;
            let (block, receipt) = match case {
                0 => (audio(1, now), now),
                1 => (audio(3, now), now),
                2 => (audio(2, START + 50_001), START + 50_001),
                3 => (audio(2, now + 1), now),
                4 => (audio(2, now), now + MAX_EVIDENCE_AGE_US),
                _ => unreachable!(),
            };
            assert!(
                gate.observe(Activity::Continue(block), local.context(receipt), receipt)
                    .is_err(),
                "case {case}"
            );
            assert!(matches!(gate.phase, Phase::Idle));
        }
    }

    #[test]
    fn discard_clock_regression_and_explicit_reset_do_not_reuse_candidate_identity() {
        let (mut gate, mut local, old) = accepted_stream();
        gate.discard_stream_until_end(START + 1).unwrap();
        assert!(gate.discard_stream_until_end(START).is_err());
        assert!(matches!(gate.phase, Phase::Idle));
        gate.reset();
        let now = START + 10_000;
        let next = candidate(&mut gate, local.context(now), 2, now);
        assert_ne!(next.id, old.candidate);
        assert!(matches!(
            gate.decide(old, local.context(now), now).unwrap(),
            Decision::Ignored
        ));
        let accepted = accept(&mut gate, local.context(now), evidence(next, 2, now), now);
        assert_eq!(accepted.audio.len(), 1);
        assert_eq!(accepted.audio[0].sequence, 2);
    }

    #[test]
    fn prefix_sanitation_keeps_onset_identity_deadline_and_stale_decision_guards() {
        let mut local = LocalInput::new();
        let mut gate = InputAdmission::default();
        let old = candidate(&mut gate, local.context(START), 1, START);
        gate.reset();
        let now = START + 100_000;
        let frames = (2..=11)
            .map(|sequence| audio(sequence, START + (sequence - 1) * 10_000))
            .collect();
        let Step::Candidate(current) = gate
            .observe(Activity::Start(frames), local.context(now), now)
            .unwrap()
            .step
        else {
            panic!("expected current candidate");
        };
        assert!(gate.pending_audio(old.id).is_none());
        assert!(gate.discard_pending_prefix(old.id, 5).is_err());
        assert!(gate.discard_pending_prefix(current.id, 6).is_err());
        assert_eq!(gate.pending_audio(current.id).unwrap().len(), 10);
        assert_eq!(gate.discard_pending_prefix(current.id, 5).unwrap(), 4);
        let retained = gate.pending_audio(current.id).unwrap();
        assert_eq!(
            retained.iter().map(|f| f.sequence).collect::<Vec<_>>(),
            vec![6, 7, 8, 9, 10, 11]
        );
        let (after, latest) = gate
            .pending_evidence(now, AcceptanceBasis::DirectedSessionV1Baseline)
            .unwrap();
        assert_eq!(after.id, current.id);
        assert_eq!(after.first_read_at_us, current.first_read_at_us);
        assert_eq!(after.decision_deadline_us, current.decision_deadline_us);
        assert_eq!(latest.through_sequence, 11);
        assert!(matches!(
            gate.decide(evidence(old, 1, now), local.context(now), now)
                .unwrap(),
            Decision::Ignored
        ));
        let expired = current.decision_deadline_us;
        assert!(matches!(
            gate.decide(latest, local.context(expired), expired)
                .unwrap(),
            Decision::Rejected(Rejection {
                reason: RejectReason::Deadline,
                ..
            })
        ));
    }

    /// #828: a duplex hold keeping first words takes a candidate whose
    /// audio begins before its 300 ms of pre-roll, as far as 1.6 s before
    /// its trigger, and counts its deadline from the trigger and its
    /// retained age from that pre-roll, as before. A longer lead, or any
    /// lead without first words kept, is refused.
    #[test]
    fn a_lead_of_first_words_is_held_without_shortening_the_decision() {
        let started = |hold: Hold, blocks: u64| {
            let mut gate = InputAdmission::with_hold(hold);
            let mut local = LocalInput::new();
            let now = START + blocks * 10_000;
            let prefix = (1..=blocks)
                .map(|n| audio(n, START + n * 10_000))
                .collect::<Vec<_>>();
            gate.observe(Activity::Start(prefix), local.context(now), now)
                .map(|update| (update.step, now))
        };
        let kept = Hold::DUPLEX.keeping_first_words();
        assert_eq!(kept.lead_blocks + PRE_ROLL_BLOCKS, REACH_BACK_BLOCKS);
        let (Step::Candidate(candidate), now) = started(kept, 160).unwrap() else {
            panic!("a lead of first words was not taken");
        };
        assert_eq!(candidate.first_read_at_us, START + 10_000);
        assert_eq!(candidate.trigger_read_at_us, now);
        assert_eq!(
            candidate.decision_deadline_us,
            now + Hold::DUPLEX.decision_budget_us
        );
        assert!(started(kept, 161).is_err());
        assert!(started(Hold::DUPLEX, 31).is_err());
        let (Step::Candidate(usual), now) = started(Hold::DUPLEX, 30).unwrap() else {
            panic!("the usual pre-roll was not taken");
        };
        assert_eq!(
            usual.decision_deadline_us,
            now + Hold::DUPLEX.decision_budget_us
        );
    }

    /// #936, the reviewer at 08:02: what may be dropped before a commit is
    /// only the lead, the blocks before the usual 300 ms of pre-roll, and of
    /// those only the ones captured before the cutoff. The pre-roll, the
    /// onset and what was said after it are never named.
    #[test]
    fn only_a_lead_older_than_the_cutoff_may_be_dropped() {
        let mut gate = InputAdmission::with_hold(Hold::DUPLEX.keeping_first_words());
        let mut local = LocalInput::new();
        let now = START + 160 * 10_000;
        let prefix = (1..=160)
            .map(|n| audio(n, START + n * 10_000))
            .collect::<Vec<_>>();
        let Step::Candidate(candidate) = gate
            .observe(Activity::Start(prefix), local.context(now), now)
            .unwrap()
            .step
        else {
            panic!("no candidate");
        };
        // Ten more blocks said after the onset.
        for n in 161..=170 {
            let at = START + n * 10_000;
            gate.observe(Activity::Continue(audio(n, at)), local.context(at), at)
                .unwrap();
        }
        let block = |n: u64| START + n * 10_000;
        assert_eq!(gate.lead_older_than(candidate.id, block(1)), None);
        assert_eq!(gate.lead_older_than(candidate.id, block(41)), Some(40));
        // The lead is blocks 1 to 130; the cutoff past it names its last.
        assert_eq!(gate.lead_older_than(candidate.id, block(170)), Some(130));
        assert_eq!(gate.discard_pending_prefix(candidate.id, 40).unwrap(), 40);
        assert_eq!(gate.lead_older_than(candidate.id, block(41)), None);
        assert_eq!(gate.lead_older_than(candidate.id, block(170)), Some(130));
        // Without a lead nothing may be.
        let mut local = LocalInput::new();
        let mut usual = InputAdmission::with_hold(Hold::DUPLEX);
        let prefix = (1..=30)
            .map(|n| audio(n, START + n * 10_000))
            .collect::<Vec<_>>();
        let at = START + 30 * 10_000;
        let Step::Candidate(candidate) = usual
            .observe(Activity::Start(prefix), local.context(at), at)
            .unwrap()
            .step
        else {
            panic!("no candidate");
        };
        assert_eq!(usual.lead_older_than(candidate.id, at + 10_000_000), None);
        assert!(FIRST_WORDS_AGE_US < lamp_gemini::MAX_INPUT_AGE.as_micros() as u64);
    }

    #[test]
    fn a_longer_hold_keeps_one_candidate_longer_and_the_standard_hold_is_unchanged() {
        assert_eq!(
            Hold::STANDARD,
            Hold {
                decision_budget_us: DECISION_BUDGET_US,
                retained_age_us: MAX_RETAINED_AGE_US,
                decision_blocks: MAX_DECISION_BLOCKS,
                lead_blocks: 0,
            }
        );
        assert_eq!(Hold::STANDARD.candidate_blocks(), MAX_CANDIDATE_BLOCKS);
        for hold in [Hold::STANDARD, Hold::DUPLEX] {
            let mut gate = InputAdmission::with_hold(hold);
            let mut local = LocalInput::new();
            let first = candidate(&mut gate, local.context(START), 1, START);
            assert_eq!(first.decision_deadline_us, START + hold.decision_budget_us);
            // A block every 10 ms while the decision is awaited: all kept, in
            // one list, under the one identity and the one deadline.
            let last = hold.decision_budget_us / 10_000 - 1;
            for n in 1..=last {
                let now = START + n * 10_000;
                no_output(
                    gate.observe(
                        Activity::Continue(audio(1 + n, now)),
                        local.context(now),
                        now,
                    )
                    .unwrap(),
                );
            }
            let now = START + last * 10_000;
            assert_eq!(gate.pending_audio(first.id).unwrap().len() as u64, 1 + last);
            let (same, _) = gate
                .pending_evidence(now, AcceptanceBasis::DirectedSessionDuplex)
                .unwrap();
            assert_eq!(same.id, first.id);
            assert_eq!(same.decision_deadline_us, first.decision_deadline_us);
            // A decision in the last block before the deadline takes them all.
            let mut decided = InputAdmission::with_hold(hold);
            let mut other = LocalInput::new();
            let again = candidate(&mut decided, other.context(START), 1, START);
            for n in 1..=last {
                let at_us = START + n * 10_000;
                decided
                    .observe(
                        Activity::Continue(audio(1 + n, at_us)),
                        other.context(at_us),
                        at_us,
                    )
                    .unwrap();
            }
            let accepted = accept(
                &mut decided,
                other.context(now),
                evidence(again, 1 + last, now),
                now,
            );
            assert_eq!(accepted.audio.len() as u64, 1 + last);
            // At the deadline the candidate is given up, whatever arrives.
            let deadline = first.decision_deadline_us;
            let update = gate
                .observe(
                    Activity::Continue(audio(2 + last, deadline)),
                    local.context(deadline),
                    deadline,
                )
                .unwrap();
            assert!(matches!(
                update.rejection,
                Some(Rejection {
                    reason: RejectReason::Deadline,
                    ..
                })
            ));
        }
        // Blocks that arrive faster than they were captured run into the
        // block bound first: still one bounded list.
        let mut gate = InputAdmission::with_hold(Hold::DUPLEX);
        let mut local = LocalInput::new();
        candidate(&mut gate, local.context(START), 1, START);
        for n in 1..=Hold::DUPLEX.decision_blocks as u64 {
            no_output(
                gate.observe(
                    Activity::Continue(audio(1 + n, START + n)),
                    local.context(START + n),
                    START + n,
                )
                .unwrap(),
            );
        }
        let n = Hold::DUPLEX.decision_blocks as u64 + 1;
        let update = gate
            .observe(
                Activity::Continue(audio(1 + n, START + n)),
                local.context(START + n),
                START + n,
            )
            .unwrap();
        assert!(matches!(
            update.step,
            Step::Rejected(Rejection {
                reason: RejectReason::Capacity,
                ..
            })
        ));
    }

    #[test]
    fn a_dsp_reseed_ends_a_candidate_still_being_decided_and_spares_an_accepted_stream() {
        // Whatever raised the candidate was heard through a canceller that
        // has since been reset: it is rejected, and old evidence for it
        // decides nothing.
        let mut gate = InputAdmission::default();
        let mut local = LocalInput::new();
        let first = candidate(&mut gate, local.context(START), 1, START);
        local.capture.dsp_epoch += 1;
        let now = START + 10_000;
        let update = gate
            .observe(Activity::Continue(audio(2, now)), local.context(now), now)
            .unwrap();
        assert!(matches!(
            update.rejection,
            Some(Rejection {
                reason: RejectReason::CaptureChanged,
                ..
            })
        ));
        assert!(matches!(
            gate.decide(evidence(first, 1, now), local.context(now), now)
                .unwrap(),
            Decision::Ignored
        ));
        assert!(!gate.owns_stream());
        // A person already admitted is not cut off by it.
        let (mut gate, mut local, _) = accepted_stream();
        local.capture.dsp_epoch += 1;
        let update = gate
            .observe(Activity::Continue(audio(2, now)), local.context(now), now)
            .unwrap();
        assert!(update.rejection.is_none());
        assert!(gate.owns_stream());
    }

    #[test]
    fn expired_history_rejection_keeps_authority_and_identity_checks_first() {
        for kind in 0..4 {
            let mut gate = InputAdmission::with_hold(Hold::DUPLEX);
            let mut local = LocalInput::new();
            let first = candidate(&mut gate, local.context(START), 1, START);
            let when = if kind == 3 {
                first.decision_deadline_us
            } else {
                START + 1
            };
            let mut context = local.context(when);
            let reason = match kind {
                0 => {
                    context.capture.dsp_epoch += 1;
                    RejectReason::CaptureChanged
                }
                1 => {
                    local
                        .controller
                        .set_microphone_permission(at(when), Permission::Denied)
                        .unwrap();
                    context.authority = local.controller.snapshot(at(when)).unwrap();
                    RejectReason::InputUnavailable
                }
                2 => {
                    local
                        .controller
                        .admit(at(when), lamp_turn_owner::AdmittedInput::NewTurn)
                        .unwrap();
                    context.authority = local.controller.snapshot(at(when)).unwrap();
                    RejectReason::AuthorityChanged
                }
                _ => RejectReason::Deadline,
            };
            let rejected = gate
                .reject_retained_history(first.id, context, when)
                .unwrap()
                .unwrap();
            assert_eq!(rejected.reason, reason);
            assert!(!gate.owns_stream());
        }
    }

    #[test]
    fn expired_history_for_an_old_identity_cannot_reject_a_new_candidate() {
        let mut gate = InputAdmission::with_hold(Hold::DUPLEX);
        let mut local = LocalInput::new();
        let first = candidate(&mut gate, local.context(START), 1, START);
        gate.reset();
        let next = candidate(&mut gate, local.context(START + 1), 1, START + 1);
        assert_ne!(first.id, next.id);
        assert!(
            gate.reject_retained_history(first.id, local.context(START + 1), START + 1)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            gate.pending_evidence(START + 1, AcceptanceBasis::DirectedSessionDuplex)
                .unwrap()
                .0
                .id,
            next.id
        );
    }
}
