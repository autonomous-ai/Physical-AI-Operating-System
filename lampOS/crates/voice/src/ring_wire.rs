//! Fixed conversational cues and bounded metadata. Data cannot request unowned off writes.
use crate::ring_face::Mood;
use lamp_ring::Rgb;
use lamp_turn_owner::{Error, OutputPermit, Snapshot, TurnOwner};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RingPhase {
    Listening,
    Waiting,
    Speaking,
}

impl RingPhase {
    /// A turn is required. This does not grant readiness or output authority.
    pub fn from_snapshot(snapshot: Snapshot) -> Option<Self> {
        snapshot.owner()?;
        Some(if snapshot.input_active() {
            Self::Listening
        } else if snapshot.playback().is_some() {
            Self::Speaking
        } else {
            Self::Waiting
        })
    }

    /// Modest static prototypes; physical brightness and meaning are unqualified.
    pub const fn color(self) -> Rgb {
        match self {
            Self::Listening => Rgb::new(24, 32, 40),
            Self::Waiting => Rgb::new(32, 24, 8),
            Self::Speaking => Rgb::new(32, 32, 24),
        }
    }

    /// The face that goes with this phase of a turn.
    pub const fn mood(self) -> Mood {
        match self {
            Self::Listening => Mood::Listening,
            Self::Waiting => Mood::Thinking,
            Self::Speaking => Mood::Speaking,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RingFrame {
    pub snapshot: Snapshot,
    pub permit: OutputPermit,
    pub phase: RingPhase,
    pub ceiling: u16,
    pub requested_at_us: u64,
    /// How loud the voice in play is, on the scale of
    /// [`crate::ring_face::level_of`]: the person's while listening, the
    /// lamp's own while speaking. It shapes the frame and grants nothing.
    #[serde(default)]
    pub level: u8,
    /// The lamp has stopped speaking for someone who spoke over it, and has
    /// not yet taken their turn or gone on: a speaking frame is drawn as the
    /// listening face. It shapes the frame and grants nothing.
    #[serde(default)]
    pub attending: bool,
    /// The internet cannot be reached (#830, `LAMP_OFFLINE_VOICE=on`): a
    /// listening frame is drawn as the offline breath. It shapes the frame
    /// and grants nothing.
    #[serde(default)]
    pub offline: bool,
    /// The turn in hand is the head tap's "hm?" (#711): every frame of it is
    /// drawn as the listening face, which the ring then keeps at rest for the
    /// tap's window. It shapes the frame and grants nothing.
    #[serde(default)]
    pub listening: bool,
    /// A short accent to the face with the lamp's gesture (#940): which one
    /// ([`crate::ring_face::accent_code`], 0 for none) and when it began on
    /// the monotonic clock. It shapes the frame and grants nothing.
    #[serde(default)]
    pub accent: u8,
    #[serde(default)]
    pub accent_at_us: u64,
}

impl RingFrame {
    /// The face this frame is drawn with.
    pub const fn mood(&self) -> Mood {
        match self.phase {
            _ if self.listening => Mood::Listening,
            RingPhase::Speaking if self.attending => Mood::Listening,
            RingPhase::Listening if self.offline => Mood::Offline,
            phase => phase.mood(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RingRejection {
    ExpiredRequest,
    FutureRequest,
    InvalidCeiling,
    PhaseMismatch,
    StaleSnapshot,
    StaleOwner,
    StalePresentation,
    ExpiredPermit,
    ExpiredState,
    FuturePermit,
    FutureState,
    InputUnavailable,
    PrivacyClosed,
    StaleCameraGrant,
    WrongBoot,
    WrongOutputKind,
    StateTooOld,
    NoAuthority,
    OtherAuthorityFailure,
}

impl From<Error> for RingRejection {
    fn from(error: Error) -> Self {
        match error {
            Error::StaleSnapshot => Self::StaleSnapshot,
            Error::StaleOwner => Self::StaleOwner,
            Error::StalePresentation => Self::StalePresentation,
            Error::ExpiredPermit => Self::ExpiredPermit,
            Error::ExpiredState => Self::ExpiredState,
            Error::FuturePermit => Self::FuturePermit,
            Error::FutureState => Self::FutureState,
            Error::InputUnavailable => Self::InputUnavailable,
            Error::PrivacyClosed => Self::PrivacyClosed,
            Error::StaleCameraGrant => Self::StaleCameraGrant,
            Error::WrongBoot => Self::WrongBoot,
            Error::WrongOutputKind => Self::WrongOutputKind,
            Error::StateTooOld => Self::StateTooOld,
            Error::NoAuthority => Self::NoAuthority,
            _ => Self::OtherAuthorityFailure,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RingBlankReason {
    Shutdown,
    ControlLost,
    AuthorityLost { reason: RingRejection },
}

impl<'de> Deserialize<'de> for RingBlankReason {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Serde's internally tagged unit variants ignore extra fields. Empty
        // struct variants enforce the same exact wire shape for every reason.
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum StrictReason {
            Shutdown {},
            ControlLost {},
            AuthorityLost { reason: RingRejection },
        }
        Ok(match StrictReason::deserialize(deserializer)? {
            StrictReason::Shutdown {} => Self::Shutdown,
            StrictReason::ControlLost {} => Self::ControlLost,
            StrictReason::AuthorityLost { reason } => Self::AuthorityLost { reason },
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RingFeedback {
    Presented {
        owner: TurnOwner,
        phase: RingPhase,
        requested_at_us: u64,
        write_started_at_us: u64,
        write_finished_at_us: u64,
    },
    Blanked {
        reason: RingBlankReason,
        write_started_at_us: u64,
        write_finished_at_us: u64,
    },
    Rejected {
        owner: TurnOwner,
        phase: RingPhase,
        requested_at_us: u64,
        reason: RingRejection,
    },
}

impl RingFeedback {
    /// A successful black write after this stop request. The caller still
    /// verifies the sending worker, rejects duplicates and checks process exit.
    pub fn confirms_shutdown(self, requested_at_us: u64, observed_at_us: u64) -> bool {
        matches!(self, Self::Blanked {
            reason: RingBlankReason::Shutdown,
            write_started_at_us,
            write_finished_at_us,
        } if requested_at_us > 0
            && write_started_at_us >= requested_at_us
            && write_finished_at_us >= write_started_at_us
            && write_finished_at_us <= observed_at_us)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_receipt_requires_this_stop_and_ordered_completed_write() {
        let receipt = |reason, start, finish| RingFeedback::Blanked {
            reason,
            write_started_at_us: start,
            write_finished_at_us: finish,
        };
        assert!(receipt(RingBlankReason::Shutdown, 100, 101).confirms_shutdown(100, 101));
        for (requested, observed, start, finish) in [
            (0, 101, 100, 101),
            (100, 101, 0, 101),
            (100, 101, 99, 101),
            (100, 101, 100, 99),
            (100, 101, 100, 102),
            (100, 99, 100, 101),
        ] {
            assert!(
                !receipt(RingBlankReason::Shutdown, start, finish)
                    .confirms_shutdown(requested, observed)
            );
        }
        assert!(!receipt(RingBlankReason::ControlLost, 100, 101).confirms_shutdown(100, 101));
        assert!(
            !receipt(
                RingBlankReason::AuthorityLost {
                    reason: RingRejection::PrivacyClosed
                },
                100,
                101,
            )
            .confirms_shutdown(100, 101)
        );
    }
}
