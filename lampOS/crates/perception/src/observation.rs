//! Perception worker output. Every event fits one runtime datagram.
use crate::image::Exposure;
use lamp_camera::frames::{FrameMeta, WireTimestamp};
use lamp_interaction::{BootId, CameraGrant, Observation};
use serde::{Deserialize, Serialize};

/// Faces reported per observation; `Presence::Faces::count` counts all.
pub const MAX_FACES: usize = 4;
/// Normalized coordinates are per ten thousand of the frame width/height.
pub const NORMALIZED: f32 = 10_000.0;

/// The camera frame an observation came from, with its original lineage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameRef {
    pub camera_worker: BootId,
    pub capture_epoch: u64,
    pub frame_sequence: u64,
    pub grant: CameraGrant,
    pub width: u32,
    pub height: u32,
    pub driver_timestamp: Option<WireTimestamp>,
    /// Host dequeue completion: bounds local age, not exposure time.
    pub dequeue_completed_at_us: u64,
}

impl From<&FrameMeta> for FrameRef {
    fn from(meta: &FrameMeta) -> Self {
        Self {
            camera_worker: meta.camera_worker,
            capture_epoch: meta.capture_epoch,
            frame_sequence: meta.frame_sequence,
            grant: meta.grant,
            width: meta.width,
            height: meta.height,
            driver_timestamp: meta.driver_timestamp,
            dequeue_completed_at_us: meta.dequeue_completed_at_us,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownReason {
    TooDark,
    Overexposed,
    NoFace,
    FaceTooSmall,
    LandmarksOutOfFrame,
    LandmarksDegenerate,
}

/// Whether faces were detected. `NoFaceDetected` is a statement about this
/// frame and model, not that the desk is empty: a person may be turned away,
/// occluded or outside the field of view.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Presence {
    Faces { count: u8, best_per_mille: u16 },
    NoFaceDetected,
    Unknown { reason: UnknownReason },
}

/// Head yaw of the largest face relative to the camera. Facing the camera is
/// not proof of being addressed, and facing away (for example toward the
/// monitor) is not a reason to reject clearly addressed speech.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Attention {
    Toward { face: u8, yaw_tenths: u16 },
    Away { face: u8, yaw_tenths: u16 },
    Unknown { reason: UnknownReason },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceEvidence {
    pub score_per_mille: u16,
    /// x, y, width, height normalized to the frame (clamped to it).
    pub bounds: [u16; 4],
    /// Subject's right eye, left eye, nose, right and left mouth corners.
    pub landmarks: [[u16; 2]; 5],
    /// Absolute yaw in tenths of a degree, when estimable.
    pub yaw_tenths: Option<u16>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisTiming {
    pub received_at_us: u64,
    pub decode_us: u32,
    pub inference_us: u32,
    pub analysed_at_us: u64,
    pub published_at_us: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    pub id: String,
    pub sha256_prefix: String,
    pub input_width: u32,
    pub input_height: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualObservation {
    /// Source boot is the perception worker; `acquired_at` is the frame's
    /// host dequeue completion; confidence is the best face score, or unknown
    /// when no face was detected or the frame was unusable.
    pub provenance: Observation,
    pub frame: FrameRef,
    pub timing: AnalysisTiming,
    pub exposure: Exposure,
    pub presence: Presence,
    pub faces: Vec<FaceEvidence>,
    pub attention: Attention,
    pub model: ModelRef,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscardReason {
    /// Older than the freshness bound on receipt or after analysis.
    Stale,
    /// The frame's camera grant is no longer the current one.
    PrivacyChanged,
    /// No current camera authority to judge the grant.
    NoAuthority,
    DecodeFailed,
    InferenceFailed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerceptionCounts {
    pub requested: u64,
    pub received: u64,
    pub published: u64,
    pub stale: u64,
    pub privacy_changed: u64,
    pub decode_failed: u64,
    pub inference_failed: u64,
    pub link_errors: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PerceptionEvent {
    Observation {
        observation: VisualObservation,
    },
    Discarded {
        frame: FrameRef,
        reason: DiscardReason,
        at_us: u64,
    },
    /// Frame link state changes. `error` is bounded diagnostic text.
    CameraLink {
        connected: bool,
        at_us: u64,
        error: Option<String>,
    },
    Progress {
        at_us: u64,
        counts: PerceptionCounts,
    },
}

/// Bound diagnostic text so every event fits one datagram.
pub fn bounded_text(text: &str) -> String {
    text.chars().take(160).collect()
}
