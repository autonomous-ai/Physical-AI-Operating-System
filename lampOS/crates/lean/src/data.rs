use crate::{Error, JOINT_COUNT, invalid};
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

pub const MAX_GEOMETRY_BYTES: usize = 65_536;

/// Bus order, not the serial kinematic order. In the V1 description the last
/// two links are head_pitch then head_roll; Geometry.links names them explicitly.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Joint {
    BaseYaw,
    Shoulder,
    Elbow,
    HeadRoll,
    HeadPitch,
}

impl Joint {
    pub const ALL: [Self; JOINT_COUNT] = [
        Self::BaseYaw,
        Self::Shoulder,
        Self::Elbow,
        Self::HeadRoll,
        Self::HeadPitch,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::BaseYaw => 0,
            Self::Shoulder => 1,
            Self::Elbow => 2,
            Self::HeadRoll => 3,
            Self::HeadPitch => 4,
        }
    }
}

/// Five complete encoder-angle values, in radians about the servo midpoint.
/// These are neither legacy normalized positions nor physical link angles.
/// No angle wrapping or clamping conceals an invalid request.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Angles([f64; JOINT_COUNT]);

impl Angles {
    pub fn new(radians: [f64; JOINT_COUNT]) -> Result<Self, Error> {
        if radians
            .iter()
            .any(|q| !q.is_finite() || !(-PI..=PI).contains(q))
        {
            return Err(invalid(
                "all five angles must be finite radians within -pi..pi",
            ));
        }
        Ok(Self(radians))
    }

    /// Matches Body's JointState convention exactly. Present_Position already
    /// includes servo homing; subtracting its homing register again is wrong.
    /// The caller must establish position mode and a qualified register binding.
    pub fn from_encoder_counts(counts: [u16; JOINT_COUNT]) -> Result<Self, Error> {
        if counts.iter().any(|count| *count > 4095) {
            return Err(invalid("single-turn encoder counts must be 0..4095"));
        }
        Self::new(counts.map(|count| (f64::from(count) - 2048.0) * TAU / 4096.0))
    }

    pub const fn radians(self) -> [f64; JOINT_COUNT] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Measurement {
    StandIn,
    Measured,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Base {
    pub mass_kg: f64,
    /// Fixed base COM in the support-plane frame, metres.
    pub com_m: [f64; 3],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub joint: Joint,
    /// This joint's origin in the preceding link's frame, metres. The first
    /// joint uses the fixed support-plane frame. These offsets encode lengths.
    pub origin_m: [f64; 3],
    /// Fixed origin rotation Rz(yaw) * Ry(pitch) * Rx(roll), radians.
    pub rpy_rad: [f64; 3],
    /// Unit axis in that rotated joint frame, pointing so a positive encoder
    /// angle turns by the right-hand rule. Its sign must be measured.
    pub axis: [f64; 3],
    /// Encoder-angle radians at the model's zero pose for this joint.
    pub angle_zero_rad: f64,
    /// Mass of this rigid moving part, excluding every other modeled part.
    pub mass_kg: f64,
    /// Part COM in its own frame AFTER this joint rotates, metres.
    pub com_m: [f64; 3],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    pub schema_version: u32,
    pub measurement: Measurement,
    /// Human-readable measurement provenance. A label is not certification.
    pub label: String,
    /// Contact support polygon in counterclockwise order, metres, z = 0.
    pub footprint_m: Vec<[f64; 2]>,
    pub margin_m: f64,
    pub base: Base,
    /// Exactly five links in physical serial order; every Joint occurs once.
    pub links: [Link; JOINT_COUNT],
}

impl Geometry {
    /// Parsing is bounded; Model::new performs geometric/numeric validation.
    /// No embedded data are selected implicitly.
    pub fn from_json(text: &str) -> Result<Self, Error> {
        if text.len() > MAX_GEOMETRY_BYTES {
            return Err(invalid("geometry exceeds 64 KiB"));
        }
        serde_json::from_str(text).map_err(|error| invalid(format!("geometry JSON: {error}")))
    }
}
