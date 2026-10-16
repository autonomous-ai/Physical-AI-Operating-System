//! Finite head-only qualification sequence over the existing worker commands.
//! No authority is created here. Every action needs original caller evidence;
//! Stop/fault never schedules a return pose. The caller owns Supervisor cleanup.
use super::{
    config::invalid,
    wire::{Command, Control, Event, Motion, Observation},
};
use crate::{
    Joint,
    calibration::UnitCalibration,
    command::JointSelection,
    control::Readback,
    units::{EncoderCounts, NormalizedPosition},
};
use lamp_turn_owner::{
    BootId, BoundaryGuard, MonoTime, OutputKind, OutputPermit, Snapshot, TurnOwner,
};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadPlan {
    pub unit: String,
    pub qualification_record: [u8; 32],
    pub expected_initial_counts: [u16; 5],
    /// Operator-qualified raw count envelope; no built-in value or fallback.
    pub minimum_count: u16,
    pub maximum_count: u16,
    pub outbound_count: u16,
    pub segment_duration_us: u64,
    pub maximum_duration_us: u64,
    pub readback_age_us: u64,
}
#[derive(Clone, Copy)]
pub struct OriginalEvidence {
    pub permit: OutputPermit,
    pub placement: Observation,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    New,
    InspectingInitial,
    Initial,
    Outbound,
    AtOutbound,
    InspectingReturn,
    ReturnReady,
    Returning,
    Returned,
    Inhibited,
}
/// Exactly two possible segments and two explicit inspections. No motion queue,
/// retry, timer-generated authority, or implicit restoration after interruption.
pub struct HeadQualification {
    plan: HeadPlan,
    calibration: UnitCalibration,
    guard: BoundaryGuard,
    worker: BootId,
    placement_source: BootId,
    owner: Option<TurnOwner>,
    phase: Phase,
    started: MonoTime,
    last_now: MonoTime,
    request: u64,
    motion: Option<Motion>,
    last_readback: Option<Readback>,
    last_frame: u64,
}
impl HeadPlan {
    pub fn validate(&self, calibration: &UnitCalibration) -> io::Result<()> {
        let unit = crate::calibration::UnitId::new(&self.unit).map_err(invalid)?;
        calibration.bind(&unit).map_err(invalid)?;
        if self.qualification_record == [0; 32]
            || self.minimum_count >= self.maximum_count
            || self.maximum_count > 4095
            || self.outbound_count == self.expected_initial_counts[4]
            || !(self.minimum_count..=self.maximum_count).contains(&self.outbound_count)
            || !(self.minimum_count..=self.maximum_count).contains(&self.expected_initial_counts[4])
            || !(1_000..=200_000).contains(&self.segment_duration_us)
            || !(1_000..=5_000_000).contains(&self.maximum_duration_us)
            || self.segment_duration_us * 2 >= self.maximum_duration_us
            || !(1..=200_000).contains(&self.readback_age_us)
        {
            return Err(invalid("invalid explicit head qualification plan"));
        }
        for (joint, count) in Joint::ALL.into_iter().zip(self.expected_initial_counts) {
            exact_target(calibration, joint, count)?;
        }
        for count in [self.minimum_count, self.maximum_count, self.outbound_count] {
            exact_target(calibration, Joint::WristPitch, count)?;
        }
        Ok(())
    }
}
impl HeadQualification {
    pub fn new(
        plan: HeadPlan,
        calibration: UnitCalibration,
        controller: BootId,
        worker: BootId,
        placement_source: BootId,
        now: MonoTime,
    ) -> io::Result<Self> {
        plan.validate(&calibration)?;
        Ok(Self {
            plan,
            calibration,
            guard: BoundaryGuard::new(controller, now),
            worker,
            placement_source,
            owner: None,
            phase: Phase::New,
            started: now,
            last_now: now,
            request: 0,
            motion: None,
            last_readback: None,
            last_frame: 0,
        })
    }
    /// Continue one existing worker after its two explicit setup requests.
    /// The trusted caller must already have correlated inspection and profile
    /// preparation completion. This allocates request IDs, never authority.
    pub fn after_profile_preparation(mut self) -> io::Result<Self> {
        if self.phase != Phase::New || self.request != 0 {
            return Err(invalid("qualification request sequence already started"));
        }
        self.request = 2;
        Ok(self)
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    /// Uses the existing one-child Supervisor, never a second serial reader.
    /// A failed send is terminal; there is no retry or automatic return action.
    pub fn send_inspection(
        &mut self,
        supervisor: &mut super::supervisor::Supervisor,
        now: MonoTime,
        e: OriginalEvidence,
    ) -> io::Result<()> {
        self.require_worker(supervisor)?;
        let result = self
            .inspect(now, e)
            .and_then(|command| supervisor.send_command(&command));
        if result.is_err() {
            self.stop();
            let _ = supervisor.stop();
        }
        result
    }
    pub fn send_segment(
        &mut self,
        supervisor: &mut super::supervisor::Supervisor,
        now: MonoTime,
        e: OriginalEvidence,
    ) -> io::Result<()> {
        self.require_worker(supervisor)?;
        let result = self
            .segment(now, e)
            .and_then(|command| supervisor.send_command(&command));
        if result.is_err() {
            self.stop();
            let _ = supervisor.stop();
        }
        result
    }
    /// One priority authority update. Forward denial even when this controller
    /// inhibits; resource cleanup is still reported independently by Supervisor.
    pub fn send_authority(
        &mut self,
        supervisor: &mut super::supervisor::Supervisor,
        now: MonoTime,
        snapshot: Snapshot,
    ) -> io::Result<()> {
        self.require_worker(supervisor)?;
        let checked = self.authority(now, snapshot);
        let sent = supervisor.send_control(&Control::Authority { snapshot });
        let result = checked.and(sent);
        if result.is_err() {
            self.stop();
            let _ = supervisor.stop();
        }
        result
    }
    /// At most eight already-available messages, no wait. Continue Supervisor
    /// polling after an error/Stop until actual exit and cleanup are observed.
    pub fn poll(
        &mut self,
        supervisor: &mut super::supervisor::Supervisor,
        mut receipt: impl FnMut(&Event),
    ) -> io::Result<()> {
        self.require_worker(supervisor)?;
        let result = (|| {
            self.maintain(supervisor.now())?;
            for _ in 0..8 {
                let Some(event) = supervisor.poll()? else {
                    break;
                };
                receipt(&event);
                self.observe(supervisor.now(), &event)?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.stop();
            let _ = supervisor.stop();
        }
        result
    }
    /// Bind dispatch and observation to the original owned child before touching
    /// either channel. A wrong supplied Supervisor is not ours to Stop.
    fn require_worker(&mut self, supervisor: &super::supervisor::Supervisor) -> io::Result<()> {
        if supervisor.worker != self.worker {
            self.stop();
            return Err(invalid("qualification Supervisor worker changed"));
        }
        Ok(())
    }
    pub fn last_readback(&self) -> Option<Readback> {
        self.last_readback
    }
    pub fn stop(&mut self) -> Control {
        self.phase = Phase::Inhibited;
        self.motion = None;
        Control::Stop {}
    }
    /// Caller forwards this same original snapshot to Supervisor priority control.
    pub fn authority(&mut self, now: MonoTime, snapshot: Snapshot) -> io::Result<()> {
        match self.guard.install(now, snapshot) {
            Ok(()) => {}
            Err(lamp_turn_owner::Error::StaleSnapshot) => return self.maintain(now),
            Err(lamp_turn_owner::Error::ExpiredState)
                if self
                    .guard
                    .state()
                    .is_some_and(|s| snapshot.revision() <= s.revision()) =>
            {
                return self.maintain(now);
            }
            Err(error) => {
                self.stop();
                return Err(invalid(error));
            }
        }
        if snapshot.microphone_permission() != lamp_turn_owner::Permission::Allowed
            || snapshot.camera_permission() != lamp_turn_owner::Permission::Allowed
        {
            self.stop();
            return Err(invalid("privacy revoked"));
        }
        self.maintain(now)
    }
    pub fn maintain(&mut self, now: MonoTime) -> io::Result<()> {
        if now < self.last_now
            || now.as_micros().saturating_sub(self.started.as_micros())
                >= self.plan.maximum_duration_us
            || self.guard.state().is_none_or(|s| {
                now >= s.expires_at() || self.owner.is_some_and(|o| s.owner() != Some(o))
            })
        {
            self.stop();
            return Err(invalid(
                "qualification authority or finite deadline expired",
            ));
        }
        self.last_now = now;
        if self.phase == Phase::Inhibited {
            return Err(invalid("qualification inhibited"));
        }
        Ok(())
    }
    fn evidence(&mut self, now: MonoTime, e: OriginalEvidence) -> io::Result<()> {
        self.maintain(now)?;
        self.guard
            .check(now, e.permit, OutputKind::Motion)
            .map_err(invalid)?;
        let p = e.placement;
        if self.owner.is_some_and(|o| o != e.permit.owner())
            || p.source_boot != self.placement_source
            || p.qualification_record != self.plan.qualification_record
            || p.frame_sequence <= self.last_frame
            || p.acquired_at_us > now.as_micros()
            || p.valid_until_us <= now.as_micros()
            || p.acquired_at_us >= p.valid_until_us
            || p.camera != self.guard.current_camera_grant(now).map_err(invalid)?
            || Some(p.camera) != e.permit.camera()
        {
            self.stop();
            return Err(invalid("original movement evidence changed or stale"));
        }
        self.owner = Some(e.permit.owner());
        self.last_frame = p.frame_sequence;
        Ok(())
    }
    /// Only an explicit caller action can request either fresh inspection.
    pub fn inspect(&mut self, now: MonoTime, e: OriginalEvidence) -> io::Result<Command> {
        let result = self.inspect_inner(now, e);
        if result.is_err() {
            self.stop();
        }
        result
    }
    fn inspect_inner(&mut self, now: MonoTime, e: OriginalEvidence) -> io::Result<Command> {
        if !matches!(self.phase, Phase::New | Phase::AtOutbound) {
            return Err(invalid("inspection phase mismatch"));
        }
        self.evidence(now, e)?;
        self.request += 1;
        self.phase = if self.phase == Phase::New {
            Phase::InspectingInitial
        } else {
            Phase::InspectingReturn
        };
        Ok(Command::Inspect {
            request: self.request,
            requested_at_us: now.as_micros(),
        })
    }
    /// Only ID5 is serialized by the real selected-joint worker/core boundary.
    pub fn segment(&mut self, now: MonoTime, e: OriginalEvidence) -> io::Result<Command> {
        let result = self.segment_inner(now, e);
        if result.is_err() {
            self.stop();
        }
        result
    }
    fn segment_inner(&mut self, now: MonoTime, e: OriginalEvidence) -> io::Result<Command> {
        if !matches!(self.phase, Phase::Initial | Phase::ReturnReady) {
            return Err(invalid("segment phase mismatch"));
        }
        self.evidence(now, e)?;
        self.last_readback
            .ok_or_else(|| invalid("readback absent"))?
            .validate(now, self.plan.readback_age_us)
            .map_err(invalid)?;
        let end = now
            .as_micros()
            .checked_add(self.plan.segment_duration_us)
            .ok_or_else(|| invalid("deadline overflow"))?;
        if end >= e.permit.expires_at().as_micros()
            || end >= e.placement.valid_until_us
            || end.saturating_sub(self.started.as_micros()) >= self.plan.maximum_duration_us
        {
            return Err(invalid("segment exceeds original authority"));
        }
        let returning = self.phase == Phase::ReturnReady;
        let mut counts = self.plan.expected_initial_counts;
        if !returning {
            counts[4] = self.plan.outbound_count;
        }
        let mut target = [0.0; 5];
        for joint in Joint::ALL {
            target[joint.index()] =
                exact_target(&self.calibration, joint, counts[joint.index()])?.get();
        }
        self.request += 1;
        self.phase = if returning {
            Phase::Returning
        } else {
            Phase::Outbound
        };
        Ok(Command::SelectedSegment {
            request: self.request,
            requested_at_us: now.as_micros(),
            permit: e.permit,
            placement: e.placement,
            target,
            duration_us: self.plan.segment_duration_us,
            selection: JointSelection::from_mask(1 << 4).map_err(invalid)?,
        })
    }
    pub fn observe(&mut self, now: MonoTime, event: &Event) -> io::Result<()> {
        let result = self.observe_inner(now, event);
        if result.is_err() {
            self.stop();
        }
        result
    }
    fn observe_inner(&mut self, now: MonoTime, event: &Event) -> io::Result<()> {
        self.maintain(now)?;
        if now
            .as_micros()
            .checked_sub(event.at_us())
            .is_none_or(|age| age > super::channel::MAX_AGE_US)
        {
            return Err(invalid("stale or future worker event"));
        }
        match *event {
            Event::Ready { .. } | Event::Progress { .. } => {}
            Event::Inspected {
                request,
                at_us,
                readback,
            } => {
                if request != self.request
                    || !matches!(
                        self.phase,
                        Phase::InspectingInitial | Phase::InspectingReturn
                    )
                {
                    return Err(invalid("unowned inspection"));
                }
                self.validate_readback(
                    now,
                    at_us,
                    readback,
                    self.phase == Phase::InspectingReturn,
                )?;
                self.last_readback = Some(readback);
                self.phase = if self.phase == Phase::InspectingInitial {
                    Phase::Initial
                } else {
                    Phase::ReturnReady
                };
            }
            Event::Started {
                request, motion, ..
            } => {
                if request != self.request
                    || self.motion.is_some()
                    || !matches!(self.phase, Phase::Outbound | Phase::Returning)
                    || motion.worker != self.worker
                    || Some(motion.owner) != self.owner
                    || motion.sequence == 0
                {
                    return Err(invalid("unowned motion"));
                }
                self.motion = Some(motion);
            }
            Event::HostAccepted {
                motion,
                selection,
                counts,
                ..
            } => {
                if self.motion != Some(motion)
                    || selection.mask() != 16
                    || counts[..4] != self.plan.expected_initial_counts[..4]
                    || !(self.plan.minimum_count..=self.plan.maximum_count).contains(&counts[4])
                {
                    return Err(invalid("unexpected accepted motor target"));
                }
            }
            Event::ReadbackReached {
                motion,
                at_us,
                readback,
            } => {
                if self.motion != Some(motion)
                    || !matches!(self.phase, Phase::Outbound | Phase::Returning)
                {
                    return Err(invalid("unowned reached receipt"));
                }
                self.validate_readback(now, at_us, readback, self.phase == Phase::Outbound)?;
                self.last_readback = Some(readback);
                self.motion = None;
                self.phase = if self.phase == Phase::Outbound {
                    Phase::AtOutbound
                } else {
                    Phase::Returned
                };
            }
            _ => return Err(invalid("qualification worker stopped or rejected action")),
        }
        Ok(())
    }
    fn validate_readback(
        &self,
        now: MonoTime,
        at_us: u64,
        r: Readback,
        outbound: bool,
    ) -> io::Result<()> {
        r.validate(MonoTime::from_micros(at_us), self.plan.readback_age_us)
            .map_err(invalid)?;
        r.validate(now, self.plan.readback_age_us)
            .map_err(invalid)?;
        let mut expected = self.plan.expected_initial_counts;
        if outbound {
            expected[4] = self.plan.outbound_count;
        }
        if !r.stationary() || r.counts() != expected {
            return Err(invalid(
                "actual stationary encoder counts differ from required endpoint",
            ));
        }
        Ok(())
    }
}
/// Choose the center of the same integer encoder cell when a float round trip
/// would lose a count. Always check the exact inverse; this is not a raw writer.
fn exact_target(
    calibration: &UnitCalibration,
    joint: Joint,
    count: u16,
) -> io::Result<NormalizedPosition> {
    let c = calibration
        .bind(calibration.unit_id())
        .map_err(invalid)?
        .joint(joint);
    let raw = EncoderCounts::new(i32::from(count)).map_err(invalid)?;
    if !c.register_binding().contains(raw) {
        return Err(invalid("encoder outside bound registers"));
    }
    let value = c.normalize(raw).map_err(invalid)?;
    if c.denormalize(value) == raw {
        return Ok(value);
    }
    let direction = if matches!(c.drive_mode(), crate::calibration::DriveMode::Normal) {
        1.0
    } else {
        -1.0
    };
    let centered = NormalizedPosition::new(
        value.get() + direction * 100.0 / f64::from(c.range_max().get() - c.range_min().get()),
    )
    .map_err(invalid)?;
    if c.denormalize(centered) != raw {
        return Err(invalid("count does not round trip"));
    }
    Ok(centered)
}
