//! Strict bounded motor messages. No raw registers or arbitrary byte payloads.
use crate::command::JointSelection;
use crate::control::{MotionId, Placement, StopReport};
use lamp_interaction::{BootId, CameraGrant, MonoTime, OutputPermit, Snapshot, TurnOwner};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Motion {
    pub worker: BootId,
    pub owner: TurnOwner,
    pub sequence: u64,
}
impl From<MotionId> for Motion {
    fn from(m: MotionId) -> Self {
        Self {
            worker: m.worker_boot(),
            owner: m.owner(),
            sequence: m.sequence(),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub source_boot: BootId,
    pub frame_sequence: u64,
    pub acquired_at_us: u64,
    pub valid_until_us: u64,
    pub camera: CameraGrant,
    pub qualification_record: [u8; 32],
}
impl From<Observation> for Placement {
    fn from(o: Observation) -> Self {
        Self {
            source_boot: o.source_boot,
            frame_sequence: o.frame_sequence,
            acquired_at: MonoTime::from_micros(o.acquired_at_us),
            valid_until: MonoTime::from_micros(o.valid_until_us),
            camera: o.camera,
            qualification_record: o.qualification_record,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Control {
    Authority { snapshot: Snapshot },
    Cancel { motion: Motion },
    Stop {},
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
// Exactly one fixed-size pending command; boxing would add a hot-path allocation.
#[allow(clippy::large_enum_variant)]
pub enum Command {
    PrepareHeadProfile {
        request: u64,
        requested_at_us: u64,
        permit: OutputPermit,
        placement: Observation,
        speed_raw: u16,
        acceleration_raw: u8,
    },
    Inspect {
        request: u64,
        requested_at_us: u64,
    },
    Segment {
        request: u64,
        requested_at_us: u64,
        permit: OutputPermit,
        placement: Observation,
        target: [f64; 5],
        duration_us: u64,
    },
    SelectedSegment {
        request: u64,
        requested_at_us: u64,
        permit: OutputPermit,
        placement: Observation,
        target: [f64; 5],
        duration_us: u64,
        selection: JointSelection,
    },
}
impl Command {
    pub fn identity(&self) -> (u64, u64) {
        match self {
            Self::PrepareHeadProfile {
                request,
                requested_at_us,
                ..
            }
            | Self::Inspect {
                request,
                requested_at_us,
            }
            | Self::Segment {
                request,
                requested_at_us,
                ..
            }
            | Self::SelectedSegment {
                request,
                requested_at_us,
                ..
            } => (*request, *requested_at_us),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopEvidence {
    /// Present only when this exact fresh explicit denial was consumed. It is
    /// not a clean-operation or physical-stop claim; legacy absence stays None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_preparation: Option<crate::control::ProfileProgress>,
    pub reason: String,
    pub stopped_at_us: u64,
    pub accepted_request_bytes: usize,
    pub partial_request_bytes: usize,
    pub position_request: bool,
    pub discard_attempted: bool,
    pub discard_error: Option<String>,
}
impl From<StopReport> for StopEvidence {
    fn from(r: StopReport) -> Self {
        Self {
            privacy_revision: None,
            profile_preparation: r.profile_preparation,
            reason: bounded(r.reason),
            stopped_at_us: r.stopped_at.as_micros(),
            accepted_request_bytes: r.accepted_request_bytes,
            partial_request_bytes: r.partial_request_bytes,
            position_request: r.position_request,
            discard_attempted: r.discard_attempted,
            discard_error: r.discard_error.map(bounded),
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cleanup {
    /// Some only for the real owned serial adapter. False or errors fail cleanup.
    pub serial_closed: Option<bool>,
    pub settings_restored: Option<bool>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    ProfileStarted {
        request: u64,
        motion: Motion,
        at_us: u64,
    },
    ProfilePrepared {
        motion: Motion,
        at_us: u64,
        progress: crate::control::ProfileProgress,
        readback: crate::control::Readback,
    },
    Ready {
        at_us: u64,
    },
    Progress {
        at_us: u64,
    },
    Rejected {
        request: u64,
        at_us: u64,
        reason: String,
    },
    CancelRejected {
        motion: Motion,
        at_us: u64,
    },
    Inspected {
        request: u64,
        at_us: u64,
        readback: crate::control::Readback,
    },
    Started {
        request: u64,
        motion: Motion,
        at_us: u64,
    },
    HostAccepted {
        /// Only these IDs were transmitted; other counts are retained readbacks.
        selection: JointSelection,
        motion: Motion,
        counts: [u16; 5],
        at_us: u64,
    },
    ReadbackReached {
        motion: Motion,
        at_us: u64,
        readback: crate::control::Readback,
    },
    Inhibited {
        /// Publication observation; report.stopped_at_us is the original core fault/stop time.
        at_us: u64,
        motion: Option<Motion>,
        report: StopEvidence,
    },
    Stopped {
        at_us: u64,
        cleanup: Cleanup,
        /// Separate from command inhibition and serial-resource cleanup.
        physical_stop: Option<crate::control::HoldReport>,
    },
    Fault {
        at_us: u64,
        reason: String,
    },
}
impl Event {
    pub fn at_us(&self) -> u64 {
        match self {
            Self::ProfileStarted { at_us, .. }
            | Self::ProfilePrepared { at_us, .. }
            | Self::Ready { at_us }
            | Self::Progress { at_us }
            | Self::Rejected { at_us, .. }
            | Self::CancelRejected { at_us, .. }
            | Self::Inspected { at_us, .. }
            | Self::Started { at_us, .. }
            | Self::HostAccepted { at_us, .. }
            | Self::ReadbackReached { at_us, .. }
            | Self::Inhibited { at_us, .. }
            | Self::Stopped { at_us, .. }
            | Self::Fault { at_us, .. } => *at_us,
        }
    }
}
pub(crate) fn bounded(value: impl std::fmt::Display) -> String {
    value.to_string().chars().take(192).collect()
}
