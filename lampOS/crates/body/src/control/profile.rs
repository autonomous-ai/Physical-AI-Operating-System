//! Explicit ID5-only SRAM preparation. No position, torque, EEPROM or retries.
use super::*;

/// Host acceptance, ACK and readback are distinct observations. A missing field
/// is unknown/not reached, never proof the device kept its original setting.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileProgress {
    pub before: [[u8; 8]; JOINT_COUNT],
    pub speed_raw: u16,
    pub acceleration_raw: u8,
    pub speed_request_bytes: usize,
    pub acceleration_request_bytes: usize,
    pub speed_ack_at_us: Option<u64>,
    pub speed_readback_at_us: Option<u64>,
    pub acceleration_ack_at_us: Option<u64>,
    pub final_readback_at_us: Option<u64>,
    /// Last complete observed profiles, including a rejected mismatch.
    pub after: Option<[[u8; 8]; JOINT_COUNT]>,
    pub after_observed_at_us: Option<u64>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Register {
    Speed,
    Acceleration,
}
#[derive(Clone, Copy)]
pub(super) struct Preparing {
    pub id: MotionId,
    permit: OutputPermit,
    placement: Placement,
    desired: RawProfile,
    original_counts: [u16; JOINT_COUNT],
    register: Register,
}
impl Transaction {
    fn profile(register: Register, desired: RawProfile, now: MonoTime) -> Self {
        let mut bytes = [0; POSITION_FRAME_BYTES];
        let length = match register {
            Register::Speed => {
                bytes[..8].copy_from_slice(&[
                    255,
                    255,
                    5,
                    5,
                    3,
                    46,
                    desired.speed_raw() as u8,
                    (desired.speed_raw() >> 8) as u8,
                ]);
                9
            }
            Register::Acceleration => {
                bytes[..7].copy_from_slice(&[255, 255, 5, 4, 3, 41, desired.acceleration_raw()]);
                8
            }
        };
        bytes[length - 1] = !bytes[2..length - 1]
            .iter()
            .fold(0u8, |s, b| s.wrapping_add(*b));
        // Ping and WRITE both require exactly one zero-parameter status reply.
        Self {
            bytes,
            length,
            written: 0,
            stage: None,
            parser: StatusParser::default(),
            replies: Some(ReadOnlyRequest::ping(Joint::WristPitch).track_replies()),
            entered_at: now,
            operations: 0,
            counts: None,
            profile: Some(register),
        }
    }
}
impl<P: BusPort, C: Clock> MotorControl<P, C> {
    /// Explicit two-write preparation, never part of normal motion admission.
    /// Both caller values must equal the provisioned nonzero target. The initial
    /// all-five profile binding must also be explicit and match fresh readback.
    pub fn prepare_head_profile(
        &mut self,
        permit: OutputPermit,
        placement: Placement,
        speed_raw: u16,
        acceleration_raw: u8,
    ) -> Result<MotionId, Error> {
        let result = self.prepare_head_inner(permit, placement, speed_raw, acceleration_raw);
        if let Err(
            e @ (Error::Profile(_)
            | Error::Torque(_)
            | Error::Load(_)
            | Error::Velocity(_)
            | Error::Position(_)
            | Error::Readback(_)
            | Error::NotAtRest(_)),
        ) = result
        {
            self.stop(e, self.clock.now());
        }
        result
    }
    fn prepare_head_inner(
        &mut self,
        permit: OutputPermit,
        placement: Placement,
        speed_raw: u16,
        acceleration_raw: u8,
    ) -> Result<MotionId, Error> {
        let now = self.now()?;
        if self.active.is_some()
            || self.preparing.is_some()
            || self.transaction.is_some()
            || self.inspect_next.is_some()
            || self.quiet.is_some()
        {
            return Err(Error::Busy);
        }
        self.guard
            .check(now, permit, OutputKind::Motion)
            .map_err(Error::Authority)?;
        self.check_placement(now, permit, placement)?;
        let q = self.qualification.as_ref().ok_or(Error::Qualification)?;
        if q.allowed.mask() != 16 || self.profile_progress.is_some() {
            return Err(Error::ProfilePreparation);
        }
        let initial = q.prepare_from.ok_or(Error::ProfilePreparation)?;
        let desired = q.limits[4].profile;
        if !desired.bounded()
            || desired.speed_raw() != speed_raw
            || desired.acceleration_raw() != acceleration_raw
        {
            return Err(Error::ProfilePreparation);
        }
        if age(now, self.inspected_at.ok_or(Error::NotInspected)?)? > self.timing.readback_age_us {
            return Err(Error::ReadbackExpired);
        }
        let before = self.profile_array()?;
        let original_counts = self.check_stationary_profiles(now, initial, None)?;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        let id = MotionId {
            worker_boot: self.worker_boot,
            owner: permit.owner(),
            sequence: self.sequence,
        };
        self.profile_progress = Some(ProfileProgress {
            before,
            speed_raw,
            acceleration_raw,
            speed_request_bytes: 0,
            acceleration_request_bytes: 0,
            speed_ack_at_us: None,
            speed_readback_at_us: None,
            acceleration_ack_at_us: None,
            final_readback_at_us: None,
            after: None,
            after_observed_at_us: None,
        });
        self.preparing = Some(Preparing {
            id,
            permit,
            placement,
            desired,
            original_counts,
            register: Register::Speed,
        });
        Ok(id)
    }
    pub(super) fn check_preparing(&mut self, now: MonoTime) -> Result<(), Error> {
        if let Some(p) = self.preparing {
            self.guard
                .check(now, p.permit, OutputKind::Motion)
                .map_err(Error::Authority)?;
            self.check_placement(now, p.permit, p.placement)?;
            let q = self.qualification.as_ref().ok_or(Error::Qualification)?;
            if q.allowed.mask() != 16 || q.limits[4].profile != p.desired || !p.desired.bounded() {
                return Err(Error::ProfilePreparation);
            }
            for j in Joint::ALL {
                let s = self.samples[j.index()].ok_or(Error::NotInspected)?;
                self.check_sample(j, s, now, None)?;
                if s.value.moving_raw() != 0
                    || s.value.velocity().signed_raw() != 0
                    || s.value
                        .position()
                        .single_turn_counts()
                        .map_err(|_| Error::Position(j))?
                        .get()
                        != p.original_counts[j.index()]
                {
                    return Err(Error::NotAtRest(j));
                }
            }
        }
        Ok(())
    }
    pub(super) fn queue_profile_write(&mut self, now: MonoTime) -> Result<Tick, Error> {
        self.check_preparing(now)?;
        let p = self.preparing.ok_or(Error::ProfilePreparation)?;
        self.transaction = Some(Transaction::profile(p.register, p.desired, now));
        Ok(Tick::Progress)
    }
    pub(super) fn profile_accepted_bytes(&mut self) {
        if let Some(tx) = &self.transaction
            && let Some(reg) = tx.profile
            && let Some(progress) = &mut self.profile_progress
        {
            match reg {
                Register::Speed => progress.speed_request_bytes = tx.written,
                Register::Acceleration => progress.acceleration_request_bytes = tx.written,
            }
        }
    }
    pub(super) fn profile_ack(&mut self, now: MonoTime) -> Result<Tick, Error> {
        let reg = self
            .transaction
            .as_ref()
            .and_then(|t| t.profile)
            .ok_or(Error::ProfilePreparation)?;
        let p = self.preparing.ok_or(Error::ProfilePreparation)?;
        if p.register != reg {
            return Err(Error::ProfilePreparation);
        }
        let progress = self
            .profile_progress
            .as_mut()
            .ok_or(Error::ProfilePreparation)?;
        match reg {
            Register::Speed => progress.speed_ack_at_us = Some(now.as_micros()),
            Register::Acceleration => progress.acceleration_ack_at_us = Some(now.as_micros()),
        }
        self.transaction = None;
        self.inspect_next = Some(ReadStage::Profile);
        Ok(Tick::Progress)
    }
    pub(super) fn profile_readback(&mut self, now: MonoTime) -> Result<Tick, Error> {
        let p = self.preparing.ok_or(Error::ProfilePreparation)?;
        let q = self.qualification.as_ref().ok_or(Error::Qualification)?;
        let mut expected = q.prepare_from.ok_or(Error::ProfilePreparation)?;
        expected[4] = match p.register {
            Register::Speed => RawProfile::observed_binding(
                expected[4].acceleration_raw(),
                p.desired.goal_time_raw(),
                p.desired.speed_raw(),
            )
            .map_err(Error::Trajectory)?,
            Register::Acceleration => p.desired,
        };
        let after = self.profile_array()?;
        let progress = self
            .profile_progress
            .as_mut()
            .ok_or(Error::ProfilePreparation)?;
        progress.after = Some(after);
        progress.after_observed_at_us = Some(now.as_micros());
        self.check_stationary_profiles(now, expected, Some(p.original_counts))?;
        let progress = self
            .profile_progress
            .as_mut()
            .ok_or(Error::ProfilePreparation)?;
        match p.register {
            Register::Speed => {
                progress.speed_readback_at_us = Some(now.as_micros());
                self.preparing
                    .as_mut()
                    .expect("checked preparation")
                    .register = Register::Acceleration;
                Ok(Tick::Progress)
            }
            Register::Acceleration => {
                progress.final_readback_at_us = Some(now.as_micros());
                let progress = *progress;
                self.preparing = None;
                Ok(Tick::ProfilePrepared {
                    motion: p.id,
                    at: now,
                    progress,
                })
            }
        }
    }
    fn profile_array(&self) -> Result<[[u8; 8]; JOINT_COUNT], Error> {
        let mut result = [[0; 8]; JOINT_COUNT];
        for j in Joint::ALL {
            result[j.index()] = self.profiles[j.index()].ok_or(Error::NotInspected)?;
        }
        Ok(result)
    }
    fn check_stationary_profiles(
        &self,
        now: MonoTime,
        expected: [RawProfile; JOINT_COUNT],
        original: Option<[u16; JOINT_COUNT]>,
    ) -> Result<[u16; JOINT_COUNT], Error> {
        let mut counts = [0; JOINT_COUNT];
        for j in Joint::ALL {
            let i = j.index();
            let p = self.profiles[i].ok_or(Error::NotInspected)?;
            let s = self.samples[i].ok_or(Error::NotInspected)?;
            self.check_sample(j, s, now, None)?;
            counts[i] = s
                .value
                .position()
                .single_turn_counts()
                .map_err(|_| Error::Position(j))?
                .get();
            if p[0] != 1 {
                return Err(Error::Torque(j));
            }
            if !expected[i].matches(&p[1..]) {
                return Err(Error::Profile(j));
            }
            if s.value.moving_raw() != 0
                || s.value.velocity().signed_raw() != 0
                || u16::from_le_bytes([p[2], p[3]]) != counts[i]
                || original.is_some_and(|v| v[i] != counts[i])
            {
                return Err(Error::NotAtRest(j));
            }
        }
        Ok(counts)
    }
}
