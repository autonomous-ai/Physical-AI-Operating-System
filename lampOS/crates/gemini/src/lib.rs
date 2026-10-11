//! Bounded Gemini Live transport for a separately supervised provider process.
//!
//! This crate does not admit speech, mint actuator permits, or decide addressees.
//! The caller maps each immutable [`Lineage`] to the original interaction owner.
//! Input transcription and provider VAD are session-scoped observations, never
//! authorization. Local playback cancellation must not wait for this transport.
#![forbid(unsafe_code)]

mod config;
mod recovery;
mod session;
#[doc(hidden)]
pub mod testing;
pub mod wire;

pub use config::{
    BarrierPolicy, BarrierWait, Credential, GOOGLE_ENDPOINT, MAX_RETAINED_SAMPLES,
    MAX_SUCCESSOR_PATIENCE, Resumption, ResumptionHandle, SessionConfig, SuccessorStart,
    ThinkingLevel, Timeouts,
};
pub use recovery::{
    Attempt, Connect, Context, Dialer, Failure, FailureReason, LinkUpdate, LinkUpdateKind, Notice,
    OutputNotice, RecoveryPolicy, Stage, StartRefusal, Supervisor,
};
use serde::{Deserialize, Serialize};
pub use session::{Connection, InputSender, ResumptionPoint, connect};
use std::{fmt, num::NonZeroU64, time::Duration};

pub const INPUT_RATE: u32 = 16_000;
pub const OUTPUT_RATE: u32 = 24_000;
pub const MAX_INPUT_SAMPLES: usize = 320;
pub const MAX_INPUT_AGE: Duration = Duration::from_secs(1);
pub const INPUT_QUEUE_CAPACITY: usize = 32;
pub const EVENT_QUEUE_CAPACITY: usize = 16;
/// Decoded output held for a consumer slower than the network, as real-time
/// playback always is: 300 s of 24 kHz PCM. Past this the socket is not read.
pub const OUTPUT_BUFFER_BYTES: usize = 300 * 2 * OUTPUT_RATE as usize;
pub const MAX_HANDLE_BYTES: usize = 8_192;
pub const MAX_OUTPUT_SAMPLES: usize = 48_000;
pub const MAX_WIRE_BYTES: usize = 262_144;
/// Largest JPEG accepted as visual context. Audio and images share one
/// connection in order, so an image delays the audio behind it by its own
/// transmission time; this bound keeps that delay small on an ordinary link.
pub const MAX_IMAGE_BYTES: usize = 64 * 1024;
/// Images one request may carry. A newer image replacing one not yet sent
/// does not count twice.
pub const MAX_IMAGES_PER_REQUEST: u8 = 2;
/// Oldest image accepted, from capture to offer and again to sending.
pub const MAX_IMAGE_AGE: Duration = Duration::from_secs(2);
pub const MAX_TEXT_BYTES: usize = 8_192;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(NonZeroU64);
impl SessionId {
    /// Must be fresh after every reconnect, supplied by the trusted supervisor.
    pub fn new(value: u64) -> Result<Self> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(Error::InvalidConfiguration)
    }
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestId(NonZeroU64);
impl RequestId {
    /// Strictly increasing within a session; old identifiers are never reused.
    pub fn new(value: u64) -> Result<Self> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(Error::InvalidConfiguration)
    }
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Lineage {
    pub session: SessionId,
    pub request: RequestId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Ready,
    Disconnected(Error),
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VoiceActivity {
    Start,
    End,
    Unspecified,
}

#[derive(Debug)]
pub enum Event {
    InputStarted {
        lineage: Lineage,
        waiting_for_barrier: bool,
    },
    Audio {
        lineage: Lineage,
        sequence: u64,
        pcm: Vec<i16>,
    },
    OutputTranscript {
        lineage: Lineage,
        text: String,
        finished: bool,
    },
    ModelText {
        lineage: Lineage,
        text: String,
    },
    GenerationComplete {
        lineage: Lineage,
    },
    Interrupted {
        lineage: Lineage,
    },
    /// `idle=false` is not an ownership-retirement barrier.
    TurnComplete {
        lineage: Lineage,
        idle: bool,
    },
    /// Google gives input transcripts no reliable per-turn ordering.
    UncorrelatedInputTranscript {
        session: SessionId,
        text: String,
        finished: bool,
    },
    VoiceActivity {
        session: SessionId,
        kind: VoiceActivity,
        audio_offset: Option<Duration>,
    },
    /// Advance notice that the service will close this connection. Not a
    /// failure: an unfinished request continues until its idle barrier.
    GoAway {
        session: SessionId,
        time_left: Option<Duration>,
    },
    /// A server message that no current request can own was dropped. It never
    /// cancels, completes or supplies output for a request.
    Discarded {
        session: SessionId,
        reason: Discard,
    },
    /// [`BarrierPolicy::AssumeAfterQuiet`] treated an earlier response as over
    /// because the service, asked to interrupt it, stayed silent for
    /// `Timeouts::barrier` without confirming. The service never said so:
    /// ownership of `successor`'s answer rests on that assumption and is not
    /// qualified evidence.
    BarrierAssumed {
        session: SessionId,
        superseded: Option<RequestId>,
        successor: Option<RequestId>,
    },
    /// A barrier ended: the earlier response settled, or the wait for it ran
    /// out. Counts only; no audio and no text.
    Barrier {
        session: SessionId,
        outcome: BarrierOutcome,
        report: BarrierReport,
    },
}

/// How a barrier ended. An assumption is reported as
/// [`Event::BarrierAssumed`], as before.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarrierOutcome {
    /// The earlier response's idle completion arrived.
    Settled,
    /// It did not within the bound, and the connection is about to fail
    /// with [`Error::BarrierTimeout`].
    TimedOut,
}

/// What stood behind a barrier while it stood: enough to tell a terminal
/// that never came from one that came and was dropped. Everything counted
/// here was received for the earlier response and withheld from the caller.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BarrierReport {
    /// The response asked to stop. Unknown when the barrier was raised by
    /// stray output with no response on record.
    pub superseded: Option<RequestId>,
    /// The request waiting behind it, if any.
    pub successor: Option<RequestId>,
    /// From the interruption to this report.
    pub waited: Duration,
    /// From the earlier response's last event, or the interruption, to this
    /// report.
    pub quiet: Duration,
    /// Whether the successor was still being spoken when the barrier ended.
    pub successor_input_open: bool,
    /// How long the successor's `activityEnd` had been withheld.
    pub end_withheld: Option<Duration>,
    pub audio_frames: u32,
    pub audio_samples: u64,
    pub text_frames: u32,
    pub generation_complete: u32,
    pub interrupted: u32,
    /// Completions that did not settle it: `turnComplete` without idle.
    pub completions_not_idle: u32,
    /// How long the next request's `activityStart` was kept back, when
    /// [`SuccessorStart::AfterFirstOutput`] kept it back at all.
    pub start_held: Option<Duration>,
    /// What ended that.
    pub start_released_by: Option<StartRelease>,
    /// Samples of the next request that were being kept when it ended.
    pub retained_samples: u64,
    /// Messages carrying the documented `waitingForInput` while the barrier
    /// stood. An observation: nothing is decided on it.
    pub waiting_for_input: u32,
    /// Session resumption updates while the barrier stood, by whether they
    /// said the session could be resumed at that point. The documentation
    /// says it cannot be while the model is generating. An observation.
    pub resumable_updates: u32,
    pub unresumable_updates: u32,
}

/// What ended the keeping back of a next request's `activityStart`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartRelease {
    /// The earlier response produced its first output and was interrupted.
    FirstOutput,
    /// The earlier response completed idle; nothing was interrupted.
    Completion,
    /// Neither came within the patience. Written anyway: from there the
    /// default behavior, including its strict wait and its failure.
    Patience,
    /// More audio than may be kept. Written anyway, as for patience.
    Overflow,
}

/// Why an image was not accepted. None of these affects the connection or
/// the request's audio.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageRefusal {
    NotConnected,
    /// Not the request now speaking: unknown, superseded, cancelled, or its
    /// input has already ended.
    NotCurrent,
    TooLarge,
    /// Not a complete JPEG by its markers. Contents are not decoded.
    NotJpeg,
    TooOld,
    /// The request already carries [`MAX_IMAGES_PER_REQUEST`].
    LimitReached,
    /// Offered under a camera grant that has since been revoked.
    Revoked,
}

/// What a revocation found. See `InputSender::revoke_images`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageRevocation {
    /// No image of a revoked grant was waiting or being written.
    Nothing,
    /// One was waiting and has been dropped unsent.
    DroppedUnsent,
    /// One was being written. The write is abandoned and with it the
    /// connection, because part of the image may already be on the wire.
    AbandonedInFlight,
}

/// An accepted image waits in one slot until the connection can take it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageOffer {
    Queued,
    /// It took the place of an image of the same request not yet sent.
    ReplacedUnsent,
}

/// The last image that reached the socket.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageReceipt {
    pub request: RequestId,
    /// The caller's own token for the frame, returned unchanged.
    pub frame: u64,
    pub bytes: u32,
    /// From the offer until the write completed.
    pub waited: Duration,
    /// The write alone: how long audio behind it was held up locally.
    pub written_in: Duration,
}

/// What became of the images offered on one connection. Counters only grow.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImageLedger {
    pub accepted: u64,
    pub sent: u64,
    /// Replaced by a newer image before being sent.
    pub replaced: u64,
    /// Accepted, then dropped unsent: its request ended, was cancelled or
    /// superseded, or the image grew too old while waiting.
    pub dropped: u64,
    /// Accepted, then withdrawn because its camera grant was revoked, before
    /// or during its write.
    pub revoked: u64,
    pub last_sent: Option<ImageReceipt>,
}

/// Why a server message was dropped instead of being attached to a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Discard {
    /// `interrupted` for a request this client never asked to interrupt. With
    /// provider activity detection disabled only an explicit activityStart can
    /// interrupt, so the event belongs to an earlier request.
    LateInterruption,
    /// Completion that arrived before the current request could have one.
    LateTerminal,
    /// Audio or text that arrived while the current request was still input.
    LateOutput,
    /// Output or lifecycle with no request in progress.
    UnownedOutput,
}

/// What a caller may do about a connection that ended with this error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Recovery {
    /// A fresh connection may succeed now.
    Reconnect,
    /// The service refused for a reason that will not clear on its own soon
    /// (quota, rate or usage limit). Retrying immediately only adds load.
    Unavailable,
    /// Configuration, authentication, protocol or local fault.
    Fatal,
}

/// Fixed diagnostic categories. Never retain raw HTTP/WebSocket/server errors,
/// close reasons, URLs, headers, response bodies, or authentication values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidConfiguration,
    ConnectTimeout,
    SetupTimeout,
    ReadTimeout,
    WriteTimeout,
    BarrierTimeout,
    /// No output arrived for an ended input within `Timeouts::response`.
    ResponseTimeout,
    /// Output began, then stopped before generation completed.
    StalledResponse,
    /// Generation completed but the idle barrier never followed.
    CompletionTimeout,
    Authentication,
    RateLimited,
    QuotaExceeded,
    ServerUnavailable,
    ServerRejected,
    ServerGoAway,
    Transport,
    PeerClosed {
        code: Option<u16>,
    },
    MalformedMessage,
    UnsupportedMessage,
    MessageTooLarge,
    InvalidAudio,
    Backpressure,
    StaleInput,
    InputSequence,
    StaleRequest,
    OverlappingInput,
    UnexpectedResponse,
    /// Input that was being sent was withdrawn locally and its connection
    /// abandoned: an image whose camera grant was revoked mid-write.
    InputCancelled,
    /// Input was offered while no provider connection was established.
    NotConnected,
    Closed,
}
impl Error {
    pub fn recovery(self) -> Recovery {
        match self {
            Self::ConnectTimeout
            | Self::SetupTimeout
            | Self::ReadTimeout
            | Self::WriteTimeout
            | Self::BarrierTimeout
            | Self::ResponseTimeout
            | Self::StalledResponse
            | Self::CompletionTimeout
            | Self::ServerUnavailable
            | Self::ServerGoAway
            | Self::Transport
            // A consumer that took nothing for the delivery bound, or input
            // that overran its queue. The connection is abandoned with what
            // it still held, the request in flight is reported lost once,
            // and a new connection serves the next request. The connection
            // itself was healthy; reconnecting is how its stuck work is shed.
            | Self::Backpressure
            // Input already partly on the wire was withdrawn locally, which
            // can only be done by abandoning the connection that carries it.
            | Self::InputCancelled => Recovery::Reconnect,
            // Application close codes observed on the deployment relay for a
            // missing key, an unconfigured route and an exhausted usage limit.
            Self::PeerClosed {
                code: Some(4001 | 4002 | 4029),
            } => Recovery::Unavailable,
            Self::PeerClosed { .. } => Recovery::Reconnect,
            Self::RateLimited | Self::QuotaExceeded => Recovery::Unavailable,
            Self::InvalidConfiguration
            | Self::Authentication
            | Self::ServerRejected
            | Self::MalformedMessage
            | Self::UnsupportedMessage
            | Self::MessageTooLarge
            | Self::InvalidAudio
            | Self::StaleInput
            | Self::InputSequence
            | Self::StaleRequest
            | Self::OverlappingInput
            | Self::UnexpectedResponse
            | Self::NotConnected
            | Self::Closed => Recovery::Fatal,
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // All variants contain only fixed labels and an optional numeric code.
        write!(f, "Gemini Live {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
