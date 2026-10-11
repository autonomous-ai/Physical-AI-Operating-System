//! One fixed position-only STS3215 broadcast. This codec is not authorization.

use crate::{JOINT_COUNT, Joint, units::EncoderCounts};

pub const POSITION_FRAME_BYTES: usize = 8 + JOINT_COUNT * 3;

/// Register 42, two little-endian position bytes, exactly five distinct IDs.
/// No goal-time, speed, acceleration, torque, EEPROM, or arbitrary-register API.
/// The broadcast has no status ACK. Host acceptance is not actuator acceptance.
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
