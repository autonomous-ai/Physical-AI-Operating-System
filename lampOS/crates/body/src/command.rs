//! Bounded position-only STS3215 broadcasts. This codec is not authorization.

use crate::{JOINT_COUNT, Joint, units::EncoderCounts};
use serde::{Deserialize, Serialize};

pub const POSITION_FRAME_BYTES: usize = 8 + JOINT_COUNT * 3;

/// Nonempty subset of the five known joints. Bit zero is servo ID 1.
/// Deserialization validates the same bounds as construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct JointSelection(u8);
impl JointSelection {
    pub const ALL: Self = Self(0x1f);
    pub fn from_mask(mask: u8) -> Result<Self, InvalidSelection> {
        if mask == 0 || mask & !Self::ALL.0 != 0 {
            Err(InvalidSelection)
        } else {
            Ok(Self(mask))
        }
    }
    pub const fn contains(self, joint: Joint) -> bool {
        self.0 & joint.mask() != 0
    }
    pub const fn mask(self) -> u8 {
        self.0
    }
}
impl TryFrom<u8> for JointSelection {
    type Error = InvalidSelection;
    fn try_from(mask: u8) -> Result<Self, Self::Error> {
        Self::from_mask(mask)
    }
}
impl From<JointSelection> for u8 {
    fn from(selection: JointSelection) -> Self {
        selection.mask()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidSelection;
impl std::fmt::Display for InvalidSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("joint selection must be a nonempty subset of IDs 1..=5")
    }
}
impl std::error::Error for InvalidSelection {}

/// Register 42, two little-endian position bytes, exactly five distinct IDs.
/// Existing callers retain byte-for-byte all-joint behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PositionFrame([u8; POSITION_FRAME_BYTES]);
impl PositionFrame {
    pub fn encode(positions: [EncoderCounts; JOINT_COUNT]) -> Self {
        let mut bytes = [0; POSITION_FRAME_BYTES];
        bytes[..7].copy_from_slice(&[0xff, 0xff, 0xfe, 19, 0x83, 42, 2]);
        for joint in Joint::ALL {
            let offset = 7 + joint.index() * 3;
            bytes[offset] = joint.id();
            bytes[offset + 1..offset + 3]
                .copy_from_slice(&positions[joint.index()].get().to_le_bytes());
        }
        bytes[POSITION_FRAME_BYTES - 1] = !bytes[2..POSITION_FRAME_BYTES - 1]
            .iter()
            .fold(0u8, |sum, &byte| sum.wrapping_add(byte));
        Self(bytes)
    }
    pub fn bytes(&self) -> &[u8; POSITION_FRAME_BYTES] {
        &self.0
    }
}

/// Only selected IDs are present, in ascending order. Unselected array entries
/// are not transmitted. No time, speed, acceleration, torque or EEPROM writes.
/// The broadcast has no status ACK; host acceptance is not actuator acceptance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedPositionFrame {
    bytes: [u8; POSITION_FRAME_BYTES],
    length: usize,
}
impl SelectedPositionFrame {
    pub fn encode(selection: JointSelection, positions: [EncoderCounts; JOINT_COUNT]) -> Self {
        let length = 8 + selection.mask().count_ones() as usize * 3;
        let mut bytes = [0; POSITION_FRAME_BYTES];
        bytes[..7].copy_from_slice(&[0xff, 0xff, 0xfe, (length - 4) as u8, 0x83, 42, 2]);
        let mut offset = 7;
        for joint in Joint::ALL {
            if selection.contains(joint) {
                bytes[offset] = joint.id();
                bytes[offset + 1..offset + 3]
                    .copy_from_slice(&positions[joint.index()].get().to_le_bytes());
                offset += 3;
            }
        }
        bytes[length - 1] = !bytes[2..length - 1]
            .iter()
            .fold(0u8, |sum, &byte| sum.wrapping_add(byte));
        Self { bytes, length }
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}
