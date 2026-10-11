//! Finite rest-to-rest segments in calibrated normalized units, never degrees.

use crate::{JOINT_COUNT, Joint, units::NormalizedPosition};
use std::fmt;

/// Exact prequalified register settings. Nonzero speed avoids the legacy zero
/// speed-cap trap. Nonzero acceleration is required without asserting its scale.
/// Goal time is matched exactly; this module neither interprets nor changes it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawProfile {
    acceleration: u8,
    goal_time: u16,
    speed: u16,
}

impl RawProfile {
    pub fn new(acceleration: u8, goal_time: u16, speed: u16) -> Result<Self, TrajectoryError> {
        if acceleration == 0 || speed == 0 || speed > 0x7fff {
            return Err(TrajectoryError::InvalidRawProfile);
        }
        Ok(Self {
            acceleration,
            goal_time,
            speed,
        })
    }

    pub fn matches(self, bytes: &[u8]) -> bool {
        bytes.len() == 7
            && bytes[0] == self.acceleration
            && u16::from_le_bytes([bytes[3], bytes[4]]) == self.goal_time
            && u16::from_le_bytes([bytes[5], bytes[6]]) == self.speed
    }
}

/// Explicit qualified limits; there are no inherited defaults. Velocity and
/// acceleration limit the generated goal sequence in normalized units/s and
/// normalized units/s². They do not establish measured actuator derivatives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointEnvelope {
    pub(crate) minimum: NormalizedPosition,
    pub(crate) maximum: NormalizedPosition,
    pub(crate) velocity: f64,
    pub(crate) acceleration: f64,
    pub(crate) profile: RawProfile,
    pub(crate) maximum_load_raw: u16,
    pub(crate) maximum_velocity_raw: u16,
    pub(crate) tracking_error: f64,
    pub(crate) settling_error: f64,
}

impl JointEnvelope {
    #[allow(clippy::too_many_arguments)] // Explicit independent qualified limits, no hidden defaults.
    pub fn new(
        minimum: NormalizedPosition,
        maximum: NormalizedPosition,
        velocity_units_per_second: f64,
        acceleration_units_per_second_squared: f64,
        profile: RawProfile,
        maximum_load_raw: u16,
        maximum_velocity_raw: u16,
        tracking_error_units: f64,
        settling_error_units: f64,
    ) -> Result<Self, TrajectoryError> {
        let positive = |v: f64| v.is_finite() && v > 0.0;
        if minimum.get() >= maximum.get()
            || !positive(velocity_units_per_second)
            || !positive(acceleration_units_per_second_squared)
            || !(1..=1023).contains(&maximum_load_raw)
            || !(1..=32767).contains(&maximum_velocity_raw)
            || !positive(tracking_error_units)
            || tracking_error_units > 200.0
            || !positive(settling_error_units)
            || settling_error_units > tracking_error_units
        {
            return Err(TrajectoryError::InvalidEnvelope);
        }
        Ok(Self {
            minimum,
            maximum,
            velocity: velocity_units_per_second,
            acceleration: acceleration_units_per_second_squared,
            profile,
            maximum_load_raw,
            maximum_velocity_raw,
            tracking_error: tracking_error_units,
            settling_error: settling_error_units,
        })
    }

    pub fn contains(self, position: NormalizedPosition) -> bool {
        position.get() >= self.minimum.get() && position.get() <= self.maximum.get()
    }
}

/// A single cubic smoothstep segment; no queue, curve parser, extrapolation, or
/// duration adaptation. Caller-supplied duration must satisfy all five bounds.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    start: [NormalizedPosition; JOINT_COUNT],
    end: [NormalizedPosition; JOINT_COUNT],
    duration_us: u64,
}

impl Segment {
    pub fn new(
        start: [NormalizedPosition; JOINT_COUNT],
        end: [NormalizedPosition; JOINT_COUNT],
        duration_us: u64,
        limits: &[JointEnvelope; JOINT_COUNT],
    ) -> Result<Self, TrajectoryError> {
        // A segment fits one original <=250 ms output permit, leaving time for
        // readback. This is a work bound, not a qualified physical duration.
        if !(1_000..=200_000).contains(&duration_us) {
            return Err(TrajectoryError::Duration);
        }
        let seconds = duration_us as f64 / 1_000_000.0;
        for joint in Joint::ALL {
            let i = joint.index();
            let limit = limits[i];
            if !limit.contains(start[i]) || !limit.contains(end[i]) {
                return Err(TrajectoryError::Range(joint));
            }
            let distance = (end[i].get() - start[i].get()).abs();
            if 1.5 * distance / seconds > limit.velocity {
                return Err(TrajectoryError::Velocity(joint));
            }
            if 6.0 * distance / (seconds * seconds) > limit.acceleration {
                return Err(TrajectoryError::Acceleration(joint));
            }
        }
        Ok(Self {
            start,
            end,
            duration_us,
        })
    }

    pub const fn duration_us(self) -> u64 {
        self.duration_us
    }

    pub fn sample(self, elapsed_us: u64) -> [NormalizedPosition; JOINT_COUNT] {
        if elapsed_us >= self.duration_us {
            return self.end;
        }
        let t = elapsed_us as f64 / self.duration_us as f64;
        let weight = t * t * (3.0 - 2.0 * t);
        std::array::from_fn(|i| {
            NormalizedPosition::new(
                self.start[i].get() + (self.end[i].get() - self.start[i].get()) * weight,
            )
            .expect("smoothstep is inside validated endpoints")
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrajectoryError {
    InvalidRawProfile,
    InvalidEnvelope,
    Duration,
    Range(Joint),
    Velocity(Joint),
    Acceleration(Joint),
}

impl fmt::Display for TrajectoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TrajectoryError {}
