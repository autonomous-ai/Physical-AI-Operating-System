//! Correlation for the separately requested profile prerequisite. No defaults,
//! register bypass, second owner, retry, or implicit setup from an inspection.
use super::*;
use crate::head_qualification::{Action, Stage};
use lamp_turn_owner::TurnOwner;

pub(super) struct ProfileSetup {
    stage: Stage,
    pub complete: bool,
    pub owner: Option<TurnOwner>,
    pub evidence: Option<OriginalEvidence>,
    motion: Option<Motion>,
    readback: Option<lamp_body::control::Readback>,
}
impl ProfileSetup {
    pub fn new() -> Self {
        Self {
            stage: Stage::NeedsProfileInspection,
            complete: false,
            owner: None,
            evidence: None,
            motion: None,
            readback: None,
        }
    }
}
impl OptionalMotor {
    pub(crate) fn qualification_stage(&self) -> Stage {
        if self.disabled || self.stopping {
            return self.qualification_phase().into();
        }
        if let Some(setup) = &self.head_profile
            && !setup.complete
        {
            return setup.stage;
        }
        self.qualification_phase().into()
    }
    pub(super) fn profile_action(
        &mut self,
        action: Action,
        snapshot: Snapshot,
        permit: OutputPermit,
        placement: Observation,
    ) -> io::Result<()> {
        let prepared = self
            .prepared
            .as_ref()
            .ok_or_else(|| invalid("profile config absent"))?;
        let setup = self
            .head_profile
            .as_mut()
            .ok_or_else(|| invalid("profile preparation not provisioned"))?;
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| invalid("profile child absent"))?;
        if child.worker != prepared.identity.worker
            || setup.complete
            || setup.stage != action.expected()
            || setup.owner.is_some_and(|o| Some(o) != snapshot.owner())
        {
            return Err(invalid("profile request identity/phase changed"));
        }
        self.guard
            .check(time(), permit, OutputKind::Motion)
            .map_err(invalid)?;
        if placement.source_boot != prepared.identity.placement_source
            || placement.qualification_record != prepared.identity.qualification_record
            || placement.camera != self.guard.current_camera_grant(time()).map_err(invalid)?
            || Some(placement.camera) != permit.camera()
            || placement.acquired_at_us > monotonic_us()
            || placement.valid_until_us <= monotonic_us()
            || placement
                .valid_until_us
                .saturating_sub(placement.acquired_at_us)
                > prepared.config.timing.placement_age_us
        {
            return Err(invalid("profile original placement changed/expired"));
        }
        let command = match action {
            Action::InspectProfile => Command::Inspect {
                request: 1,
                requested_at_us: monotonic_us(),
            },
            Action::PrepareProfile => {
                setup
                    .readback
                    .ok_or_else(|| invalid("profile inspection missing"))?
                    .validate(time(), prepared.config.timing.readback_age_us)
                    .map_err(invalid)?;
                let raw = prepared.config.qualification.joints[4];
                Command::PrepareHeadProfile {
                    request: 2,
                    requested_at_us: monotonic_us(),
                    permit,
                    placement,
                    speed_raw: raw.raw_speed,
                    acceleration_raw: raw.raw_acceleration,
                }
            }
            _ => return Err(invalid("not a profile action")),
        };
        setup.owner = Some(permit.owner());
        setup.evidence = Some(OriginalEvidence { permit, placement });
        setup.stage = if action == Action::InspectProfile {
            Stage::InspectingProfile
        } else {
            Stage::PreparingProfile
        };
        child.send_command(&command)
    }
    pub(super) fn profile_observe(&mut self, event: &Event) -> io::Result<()> {
        let at = time();
        let expected = self
            .head_plan
            .as_ref()
            .ok_or_else(|| invalid("profile plan absent"))?
            .expected_initial_counts;
        let prepared = self
            .prepared
            .as_ref()
            .ok_or_else(|| invalid("profile config absent"))?;
        let setup = self
            .head_profile
            .as_mut()
            .ok_or_else(|| invalid("profile state absent"))?;
        if at
            .as_micros()
            .checked_sub(event.at_us())
            .is_none_or(|age| age > lamp_body::runtime::channel::MAX_AGE_US)
        {
            return Err(invalid("profile event stale/future"));
        }
        match *event {
            Event::Ready { .. } | Event::Progress { .. } => Ok(()),
            Event::Inspected {
                request,
                at_us,
                readback,
            } if request == 1 && setup.stage == Stage::InspectingProfile => {
                readback
                    .validate(
                        MonoTime::from_micros(at_us),
                        prepared.config.timing.readback_age_us,
                    )
                    .map_err(invalid)?;
                readback
                    .validate(at, prepared.config.timing.readback_age_us)
                    .map_err(invalid)?;
                if !readback.stationary() || readback.counts() != expected {
                    return Err(invalid("profile initial encoder counts changed"));
                }
                setup.readback = Some(readback);
                setup.evidence = None;
                setup.stage = Stage::ProfileInspected;
                Ok(())
            }
            Event::ProfileStarted {
                request, motion, ..
            } if request == 2
                && setup.stage == Stage::PreparingProfile
                && setup.motion.is_none()
                && motion.worker == prepared.identity.worker
                && Some(motion.owner) == setup.owner
                && motion.sequence != 0 =>
            {
                setup.motion = Some(motion);
                Ok(())
            }
            Event::ProfilePrepared {
                motion,
                at_us,
                progress,
                readback,
            } if setup.stage == Stage::PreparingProfile && setup.motion == Some(motion) => {
                readback
                    .validate(
                        MonoTime::from_micros(at_us),
                        prepared.config.timing.readback_age_us,
                    )
                    .map_err(invalid)?;
                readback
                    .validate(at, prepared.config.timing.readback_age_us)
                    .map_err(invalid)?;
                let raw = prepared.config.qualification.joints[4];
                let stamps = [
                    progress.speed_ack_at_us,
                    progress.speed_readback_at_us,
                    progress.acceleration_ack_at_us,
                    progress.final_readback_at_us,
                ];
                if !readback.stationary()
                    || readback.counts() != expected
                    || progress.speed_raw != raw.raw_speed
                    || progress.acceleration_raw != raw.raw_acceleration
                    || progress.speed_request_bytes != 9
                    || progress.acceleration_request_bytes != 8
                    || stamps.iter().any(Option::is_none)
                    || stamps.windows(2).any(|w| w[0] > w[1])
                    || progress.final_readback_at_us.is_none_or(|t| t > at_us)
                    || progress.after.is_none()
                    || progress.after_observed_at_us != progress.final_readback_at_us
                {
                    return Err(invalid(
                        "profile completion missing exact write/ACK/readback evidence",
                    ));
                }
                setup.readback = Some(readback);
                setup.evidence = None;
                setup.motion = None;
                setup.complete = true;
                setup.stage = Stage::New;
                Ok(())
            }
            _ => Err(invalid("unexpected or unowned profile event")),
        }
    }
}
