//! Bounded input candidates before destructive turn replacement.
//!
//! This is sequencing and retention, not an echo/addressee classifier. The
//! directed harness explicitly accepts VAD-only evidence. No provider event or
//! session-scoped transcript is converted into a candidate decision here.
use crate::activity::{Activity, ObservedAudio, PRE_ROLL_BLOCKS};
use lamp_interaction::{BootId, MonoTime, Snapshot, TurnOwner};
use serde::Serialize;
use std::io;

pub const DECISION_BUDGET_US: u64 = 200_000;
pub const MAX_RETAINED_AGE_US: u64 = 600_000;
pub const MAX_EVIDENCE_AGE_US: u64 = 100_000;
pub const MAX_DECISION_BLOCKS: usize = 20;
pub const MAX_CANDIDATE_BLOCKS: usize = PRE_ROLL_BLOCKS + MAX_DECISION_BLOCKS;

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

/// Owns at most 50 original 10 ms PCM blocks. It never calls the controller,
/// cancels speech, sends provider input, or grants a listening/actuator cue.
pub struct InputAdmission {
    phase: Phase,
    serial: u64,
    last_now_us: u64,
}
impl Default for InputAdmission {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            serial: 0,
            last_now_us: 0,
        }
    }
}

impl InputAdmission {
    /// Reset alongside the detector after cancellation/failure. Keep identity
    /// high-water marks so a delayed decision cannot target a later candidate.
    pub fn reset(&mut self) {
        self.phase = Phase::Idle;
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
                if !context.ready(at_us) {
                    return Err(io::Error::other("candidate input is unavailable"));
                }
                validate_prefix(&audio, at_us)?;
                let first = &audio[0];
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
                        .saturating_add(DECISION_BUDGET_US)
                        .min(first.captured_at_us.saturating_add(MAX_RETAINED_AGE_US)),
                };
                let mut retained = Vec::with_capacity(MAX_CANDIDATE_BLOCKS);
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
                if pending.decision_blocks >= MAX_DECISION_BLOCKS
                    || pending.audio.len() >= MAX_CANDIDATE_BLOCKS
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

fn validate_prefix(audio: &[ObservedAudio], at_us: u64) -> io::Result<()> {
    if audio.is_empty()
        || audio.len() > PRE_ROLL_BLOCKS
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
    use lamp_interaction::{AdmissionState, CaptureState, Controller, Permission};

    const START: u64 = 1_000_000;

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
}
