//! Bounded measured-pose hold after ordinary motion is irreversibly inhibited.
//! This is separately provisioned stopping behavior, never a new interaction goal.
use super::*;
use serde::{Deserialize, Serialize};

pub const MAX_STOP_US: u64 = 100_000;

/// Provisioned software deadlines, not a claim about mechanical stopping time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StopHoldPolicy {
    deadline_us: u64,
    sample_age_us: u64,
}
impl StopHoldPolicy {
    pub fn new(deadline_us: u64, sample_age_us: u64) -> Result<Self, Error> {
        if !(1..=MAX_STOP_US).contains(&deadline_us)
            || !(1..=20_000).contains(&sample_age_us)
            || sample_age_us > deadline_us
        {
            return Err(Error::Configuration);
        }
        Ok(Self {
            deadline_us,
            sample_age_us,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldOutcome {
    Pending,
    NoAcceptedGoal,
    ObservedAtRest,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldFailure {
    BusUncertain,
    PartialPosition,
    Clock,
    Deadline,
    OperationLimit,
    Read,
    Write,
    IoOverrun,
    Protocol,
    UnexpectedTraffic,
    Profile,
    InvalidPosition,
    DeviceStatus,
    StaleSample,
    Clearance,
    Abandoned,
}

/// Host observations only. A position broadcast has no servo ACK. Even stationary
/// readback does not establish collision clearance, force, or safe torque release.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldReport {
    pub outcome: HoldOutcome,
    pub failure: Option<HoldFailure>,
    pub selection: Option<JointSelection>,
    pub started_at_us: u64,
    pub finished_at_us: Option<u64>,
    pub source: Option<Readback>,
    pub goal_accepted_bytes: usize,
    pub goal_host_accepted_at_us: Option<u64>,
    pub observed: Option<Readback>,
    pub discard_attempted: bool,
    pub discard_failed: bool,
}
impl HoldReport {
    /// Validate receipt structure, not mechanical safety. The supervisor cannot
    /// reconstruct servo acquisition times or independently certify the pose.
    pub fn validate(self, at: MonoTime) -> Result<(), Error> {
        let invalid = Error::Configuration;
        if self.started_at_us > at.as_micros()
            || self
                .finished_at_us
                .is_some_and(|v| v < self.started_at_us || v > at.as_micros())
            || self.discard_failed && !self.discard_attempted
            || (self.outcome == HoldOutcome::Failed) != self.failure.is_some()
            || (self.outcome == HoldOutcome::Pending) != self.finished_at_us.is_none()
        {
            return Err(invalid);
        }
        let bytes = self
            .selection
            .map_or(0, |s| 8 + 3 * s.mask().count_ones() as usize);
        let end = self.finished_at_us.unwrap_or(at.as_micros());
        if self.goal_accepted_bytes > bytes {
            return Err(invalid);
        }
        if let Some(accepted) = self.goal_host_accepted_at_us {
            if accepted < self.started_at_us
                || accepted > end
                || bytes == 0
                || self.goal_accepted_bytes != bytes
            {
                return Err(invalid);
            }
            let source = self.source.ok_or(invalid)?;
            source.validate(
                MonoTime::from_micros(accepted),
                if self.outcome == HoldOutcome::ObservedAtRest {
                    20_000
                } else {
                    u64::MAX
                },
            )?;
        }
        for readback in [self.source, self.observed].into_iter().flatten() {
            readback.validate(MonoTime::from_micros(end), u64::MAX)?;
            if readback
                .joints
                .iter()
                .any(|s| s.received_at_us < self.started_at_us)
            {
                return Err(invalid);
            }
        }
        match self.outcome {
            HoldOutcome::NoAcceptedGoal
                if self.selection.is_some()
                    || self.source.is_some()
                    || self.observed.is_some()
                    || self.goal_accepted_bytes != 0 =>
            {
                return Err(invalid);
            }
            HoldOutcome::ObservedAtRest => {
                let accepted = self.goal_host_accepted_at_us.ok_or(invalid)?;
                let observed = self.observed.ok_or(invalid)?;
                if !observed.stationary()
                    || observed.joints.iter().any(|s| s.received_at_us < accepted)
                    || !self.discard_attempted
                {
                    return Err(invalid);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Drain,
    Profile,
    Source,
    Goal,
    Verify,
    Wait,
    Done,
}

pub(super) struct Holding {
    policy: StopHoldPolicy,
    qualification: QualifiedEnvelope,
    phase: Phase,
    transaction: Option<Transaction>,
    samples: [Option<Sample>; JOINT_COUNT],
    next_at: MonoTime,
    last_now: MonoTime,
    operations: u16,
    steps: u16,
    pub(super) report: HoldReport,
}

impl Holding {
    pub(super) fn begin(
        qualification: QualifiedEnvelope,
        selection: Option<JointSelection>,
        transaction: Option<Transaction>,
        reason: Error,
        now: MonoTime,
    ) -> Self {
        let policy = qualification.stop_hold.expect("provisioned stop policy");
        let mut value = Self {
            policy,
            qualification,
            phase: Phase::Profile,
            transaction: None,
            samples: [None; JOINT_COUNT],
            next_at: now,
            last_now: now,
            operations: 0,
            steps: 0,
            report: HoldReport {
                outcome: HoldOutcome::Pending,
                failure: None,
                selection,
                started_at_us: now.as_micros(),
                finished_at_us: None,
                source: None,
                goal_accepted_bytes: 0,
                goal_host_accepted_at_us: None,
                observed: None,
                discard_attempted: false,
                discard_failed: false,
            },
        };
        if selection.is_none() {
            value.phase = Phase::Done;
            value.report.outcome = HoldOutcome::NoAcceptedGoal;
            value.report.finished_at_us = Some(now.as_micros());
            return value;
        }
        // A damaged/unknown stream must never receive a guessed recovery packet.
        if !matches!(
            reason,
            Error::Cancelled
                | Error::Shutdown
                | Error::ControlLost
                | Error::Authority(_)
                | Error::Load(_)
                | Error::Velocity(_)
                | Error::Tracking(_)
                | Error::ReadbackExpired
                | Error::SegmentExpired
                | Error::Clearance(_)
        ) {
            value.fail(HoldFailure::BusUncertain, now);
            return value;
        }
        if let Some(tx) = transaction {
            if tx.counts.is_some() {
                if tx.written != 0 && tx.written != tx.length {
                    value.fail(HoldFailure::PartialPosition, now);
                    return value;
                }
                // Zero bytes: discard the unsent old goal. Full frame: bytes are
                // already accepted; ordered reads below follow that frame.
            } else if tx.stage.is_some() && tx.profile.is_none() {
                // Finishing an existing READ is benign. Keep its original tracker
                // and partial parser; do not reinterpret late bytes as a new reply.
                value.phase = Phase::Drain;
                value.transaction = Some(tx);
            } else if tx.written != 0 {
                value.fail(HoldFailure::BusUncertain, now);
                return value;
            }
        }
        if value.phase == Phase::Profile {
            value.transaction = Some(Transaction::read(ReadStage::Profile, now));
        }
        value
    }

    pub(super) fn pending(&self) -> bool {
        self.phase != Phase::Done
    }

    fn fail(&mut self, reason: HoldFailure, now: MonoTime) {
        self.phase = Phase::Done;
        self.transaction = None;
        self.report.outcome = HoldOutcome::Failed;
        self.report.failure = Some(reason);
        // Never manufacture a regressed completion timestamp.
        self.report.finished_at_us = Some(now.as_micros().max(self.last_now.as_micros()));
    }

    fn time(&mut self, clock: &impl Clock) -> Result<MonoTime, HoldFailure> {
        let now = clock.now();
        if now < self.last_now {
            return Err(HoldFailure::Clock);
        }
        self.last_now = now;
        if now.as_micros() - self.report.started_at_us >= self.policy.deadline_us {
            return Err(HoldFailure::Deadline);
        }
        Ok(now)
    }

    fn after_io(
        &mut self,
        before: MonoTime,
        clock: &impl Clock,
        io_us: u64,
    ) -> Result<MonoTime, HoldFailure> {
        let now = self.time(clock)?;
        if now.as_micros() - before.as_micros() > io_us {
            return Err(HoldFailure::IoOverrun);
        }
        self.operations = self
            .operations
            .checked_add(1)
            .ok_or(HoldFailure::OperationLimit)?;
        if self.operations > MAX_OPERATIONS {
            return Err(HoldFailure::OperationLimit);
        }
        Ok(now)
    }

    pub(super) fn step<P: BusPort, C: Clock>(&mut self, port: &mut P, clock: &C, timing: Timing) {
        if !self.pending() {
            return;
        }
        if let Err(failure) = self.step_inner(port, clock, timing) {
            self.fail(failure, clock.now());
        }
        if !self.pending() {
            self.discard(port, clock, timing);
        }
    }
    pub(super) fn abandon<P: BusPort, C: Clock>(
        &mut self,
        port: &mut P,
        clock: &C,
        timing: Timing,
    ) {
        if self.pending() {
            self.fail(HoldFailure::Abandoned, clock.now());
            self.discard(port, clock, timing);
        }
    }
    fn discard<P: BusPort, C: Clock>(&mut self, port: &mut P, clock: &C, timing: Timing) {
        if !self.report.discard_attempted {
            // One best-effort flush only. Accepted bytes stay in the report and
            // no subsequent attempt/retry can append to a partial hold frame.
            self.report.discard_attempted = true;
            let before = clock.now();
            let result = port.discard_output();
            let after = clock.now();
            self.report.discard_failed = result.is_err()
                || after < before
                || after.as_micros().saturating_sub(before.as_micros()) > timing.io_us;
        }
    }

    fn step_inner<P: BusPort, C: Clock>(
        &mut self,
        port: &mut P,
        clock: &C,
        timing: Timing,
    ) -> Result<(), HoldFailure> {
        self.steps = self
            .steps
            .checked_add(1)
            .ok_or(HoldFailure::OperationLimit)?;
        if self.steps > MAX_OPERATIONS {
            return Err(HoldFailure::OperationLimit);
        }
        let now = self.time(clock)?;
        if self.phase == Phase::Wait {
            if now < self.next_at {
                return Ok(());
            }
            self.queue_read(Phase::Verify, ReadStage::Telemetry, now);
        }
        let tx = self.transaction.as_ref().ok_or(HoldFailure::Protocol)?;
        if tx.written < tx.length {
            let mut buffer = [0; READ_BYTES];
            let result = port.read(&mut buffer);
            let now = self.after_io(now, clock, timing.io_us)?;
            match result {
                Err(e) if transient(&e) => {}
                Ok(_) => return Err(HoldFailure::UnexpectedTraffic),
                Err(_) => return Err(HoldFailure::Read),
            }
            if self.phase == Phase::Goal {
                self.source_fresh(now)?;
            }
            let tx = self.transaction.as_mut().ok_or(HoldFailure::Protocol)?;
            let remaining = tx.length - tx.written;
            let result = port.write(&tx.bytes[tx.written..tx.length]);
            match result {
                Ok(count) if count > 0 && count <= remaining => {
                    tx.written += count;
                    if self.phase == Phase::Goal {
                        self.report.goal_accepted_bytes = tx.written;
                        if tx.written == tx.length {
                            self.report.goal_host_accepted_at_us = Some(clock.now().as_micros());
                        }
                    }
                }
                Err(e) if transient(&e) => {}
                _ => return Err(HoldFailure::Write),
            }
            let after = self.after_io(now, clock, timing.io_us)?;
            if self.phase == Phase::Goal {
                self.source_fresh(after)?;
            }
            if self.phase == Phase::Goal
                && self
                    .transaction
                    .as_ref()
                    .is_some_and(|t| t.written == t.length)
            {
                self.queue_read(Phase::Verify, ReadStage::Telemetry, after);
            }
            return Ok(());
        }
        if tx.replies.as_ref().is_some_and(ReplyTracker::is_complete) {
            if tx.parser.buffered_bytes() != 0 {
                return Err(HoldFailure::UnexpectedTraffic);
            }
            return self.read_finished(now, timing);
        }
        let mut buffer = [0; READ_BYTES];
        let result = port.read(&mut buffer);
        let after = self.after_io(now, clock, timing.io_us)?;
        let count = match result {
            Err(e) if transient(&e) => return Ok(()),
            Ok(count) if count > 0 && count <= READ_BYTES => count,
            _ => return Err(HoldFailure::Read),
        };
        let mut replies = [const { None }; JOINT_COUNT];
        let mut fault = false;
        let tx = self.transaction.as_mut().ok_or(HoldFailure::Protocol)?;
        let tracker = tx.replies.as_mut().ok_or(HoldFailure::Protocol)?;
        let decoded = tx
            .parser
            .feed(&buffer[..count], |event| match tracker.accept(event) {
                Ok(reply) => {
                    let index = reply.joint().index();
                    replies[index] = Some(reply);
                }
                Err(_) => fault = true,
            });
        if fault {
            return Err(HoldFailure::Protocol);
        }
        if decoded.discarded_bytes != 0 {
            return Err(HoldFailure::UnexpectedTraffic);
        }
        for reply in replies.into_iter().flatten() {
            let joint = reply.joint();
            let index = joint.index();
            match self.phase {
                Phase::Drain => {}
                Phase::Profile => {
                    let raw = reply
                        .torque_and_profile()
                        .map_err(|_| HoldFailure::Profile)?;
                    if raw.bytes()[0] != 1
                        || !self.qualification.limits[index]
                            .profile
                            .matches(&raw.bytes()[1..])
                    {
                        return Err(HoldFailure::Profile);
                    }
                }
                Phase::Source | Phase::Verify => {
                    let raw = reply.telemetry().map_err(|_| HoldFailure::Protocol)?;
                    if raw.status_raw() != 0
                        || raw.moving_raw() > 1
                        || raw.load().uninterpreted_bits() != 0
                    {
                        return Err(HoldFailure::DeviceStatus);
                    }
                    self.checked_position(joint, raw)?;
                    self.samples[index] = Some(Sample {
                        value: raw,
                        received_at: after,
                    });
                }
                _ => return Err(HoldFailure::Protocol),
            }
        }
        let tx = self.transaction.as_ref().ok_or(HoldFailure::Protocol)?;
        if tx.replies.as_ref().is_some_and(ReplyTracker::is_complete) {
            if tx.parser.buffered_bytes() != 0 {
                return Err(HoldFailure::UnexpectedTraffic);
            }
            self.read_finished(after, timing)?;
        }
        Ok(())
    }

    fn queue_read(&mut self, phase: Phase, stage: ReadStage, now: MonoTime) {
        self.phase = phase;
        self.samples.fill(None);
        self.transaction = Some(Transaction::read(stage, now));
    }

    fn checked_position(
        &self,
        joint: Joint,
        value: RawTelemetry,
    ) -> Result<NormalizedPosition, HoldFailure> {
        let count = value
            .position()
            .single_turn_counts()
            .map_err(|_| HoldFailure::InvalidPosition)?;
        let bound = self
            .qualification
            .calibration
            .bind(self.qualification.calibration.unit_id())
            .map_err(|_| HoldFailure::InvalidPosition)?;
        let calibration = bound.joint(joint);
        if !calibration.register_binding().contains(count) {
            return Err(HoldFailure::InvalidPosition);
        }
        let position = calibration
            .normalize(count)
            .map_err(|_| HoldFailure::InvalidPosition)?;
        if !self.qualification.limits[joint.index()].contains(position) {
            return Err(HoldFailure::InvalidPosition);
        }
        Ok(position)
    }

    fn observation(&self, now: MonoTime) -> Result<Readback, HoldFailure> {
        let mut joints = [EncoderReadback {
            counts: 0,
            received_at_us: 0,
            velocity_raw: 0,
            load_magnitude_raw: 0,
            moving_raw: 0,
            status_raw: 0,
        }; JOINT_COUNT];
        let mut pose = [NormalizedPosition::new(0.0).expect("zero"); JOINT_COUNT];
        for joint in Joint::ALL {
            let sample = self.samples[joint.index()].ok_or(HoldFailure::Protocol)?;
            if age(now, sample.received_at).map_err(|_| HoldFailure::Clock)?
                > self.policy.sample_age_us
            {
                return Err(HoldFailure::StaleSample);
            }
            pose[joint.index()] = self.checked_position(joint, sample.value)?;
            joints[joint.index()] = EncoderReadback {
                counts: sample
                    .value
                    .position()
                    .single_turn_counts()
                    .map_err(|_| HoldFailure::InvalidPosition)?
                    .get(),
                received_at_us: sample.received_at.as_micros(),
                velocity_raw: sample.value.velocity().signed_raw(),
                load_magnitude_raw: sample.value.load().magnitude_raw(),
                moving_raw: sample.value.moving_raw(),
                status_raw: sample.value.status_raw(),
            };
        }
        // The read position is never clipped into a different, possibly moving,
        // goal. Unknown/outside clearance is a truthful failed hold.
        self.qualification
            .clearance
            .as_ref()
            .ok_or(HoldFailure::Clearance)?
            .check_pose(pose, [0.0; JOINT_COUNT])
            .map_err(|_| HoldFailure::Clearance)?;
        Ok(Readback { joints })
    }

    fn source_fresh(&self, now: MonoTime) -> Result<(), HoldFailure> {
        let source = self.report.source.ok_or(HoldFailure::Protocol)?;
        source
            .validate(now, self.policy.sample_age_us)
            .map_err(|_| HoldFailure::StaleSample)
    }

    fn read_finished(&mut self, now: MonoTime, timing: Timing) -> Result<(), HoldFailure> {
        match self.phase {
            Phase::Drain => self.queue_read(Phase::Profile, ReadStage::Profile, now),
            Phase::Profile => self.queue_read(Phase::Source, ReadStage::Telemetry, now),
            Phase::Source => {
                let source = self.observation(now)?;
                let counts = source.joints.map(|sample| {
                    EncoderCounts::new(i32::from(sample.counts)).expect("validated counts")
                });
                self.report.source = Some(source);
                self.transaction = Some(Transaction::position(
                    self.report.selection.expect("accepted selection"),
                    counts,
                    now,
                ));
                self.phase = Phase::Goal;
            }
            Phase::Verify => {
                let observed = self.observation(now)?;
                self.report.observed = Some(observed);
                let source = self.report.source.ok_or(HoldFailure::Protocol)?;
                let bound = self
                    .qualification
                    .calibration
                    .bind(self.qualification.calibration.unit_id())
                    .map_err(|_| HoldFailure::InvalidPosition)?;
                let mut settled = observed.stationary();
                for joint in Joint::ALL {
                    let i = joint.index();
                    let a = bound
                        .joint(joint)
                        .normalize(
                            EncoderCounts::new(i32::from(source.joints[i].counts))
                                .expect("validated source"),
                        )
                        .map_err(|_| HoldFailure::InvalidPosition)?;
                    let b = bound
                        .joint(joint)
                        .normalize(
                            EncoderCounts::new(i32::from(observed.joints[i].counts))
                                .expect("validated observed"),
                        )
                        .map_err(|_| HoldFailure::InvalidPosition)?;
                    settled &= (a.get() - b.get()).abs()
                        <= self.qualification.limits[i].settling_error
                        && observed.joints[i].load_magnitude_raw
                            <= self.qualification.limits[i].maximum_load_raw;
                }
                self.transaction = None;
                if settled {
                    self.report.outcome = HoldOutcome::ObservedAtRest;
                    self.report.finished_at_us = Some(now.as_micros());
                    self.phase = Phase::Done;
                } else {
                    self.next_at = now
                        .checked_add(timing.interval_us)
                        .ok_or(HoldFailure::Clock)?;
                    self.phase = Phase::Wait;
                }
            }
            _ => return Err(HoldFailure::Protocol),
        }
        Ok(())
    }
}
