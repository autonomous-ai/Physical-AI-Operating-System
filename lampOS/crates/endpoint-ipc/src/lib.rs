//! Process boundary for end-of-turn inference.
//!
//! The interaction core asks a separate `lamp-endpoint-worker` process whether
//! retained speech sounds finished. This crate holds the shared request/result
//! types, the bounded binary frame format, the worker-side serving loop and the
//! core-side supervisor. It never links a model runtime: a missing, loading,
//! busy, slow, crashed or misbehaving worker is an explicit state the caller
//! answers with its own silence fallback.
pub mod client;
pub mod frame;
pub mod serve;

pub use client::{Limits, ProcessClassifier, Report, WorkerCommand};

pub const SAMPLE_RATE: usize = 16_000;
/// The classifier analyzes at most the last 8 s of a turn.
pub const WINDOW_SAMPLES: usize = 8 * SAMPLE_RATE;

/// Identifies one check of one pause in one utterance. A result is only
/// meaningful to the caller whose current token is identical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub utterance: u64,
    pub generation: u64,
    pub check: u8,
}

/// One classification, with the cost of each stage. Cross-process stages use
/// the host monotonic clock shared by both processes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Verdict {
    pub probability: f32,
    /// Copying the window into the request frame, on the caller's thread.
    pub snapshot_us: u64,
    /// Frame ready until the worker had read all of it (queue and pipe).
    pub delivery_us: u64,
    /// Worker feature extraction.
    pub features_us: u64,
    /// Worker model execution.
    pub inference_us: u64,
    /// Worker answer sent until the core had decoded it.
    pub return_us: u64,
    /// Submit until the result was available to `poll`. Excludes the caller's
    /// own polling cadence.
    pub round_trip_us: u64,
}

#[derive(Debug)]
pub struct Outcome {
    pub token: Token,
    /// Worker process incarnation that produced or lost this request.
    pub worker: u64,
    /// Request number within this classifier, for provenance.
    pub request: u64,
    pub result: Result<Verdict, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    Loading,
    Ready,
    /// A serving worker failed; its exit is being confirmed before any
    /// bounded replacement starts.
    Recovering(String),
    Failed(String),
    Closed,
}

impl Availability {
    /// Stable fallback reason for traces.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Ready => "refused",
            Self::Recovering(_) => "recovering",
            Self::Failed(_) => "failed",
            Self::Closed => "closed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Submit {
    Accepted,
    /// One request is already queued or running; retry later.
    Busy,
    /// Not ready, failed, recovering or closed; use the silence fallback.
    Unavailable,
}

/// An end-of-turn classifier the detector polls without blocking capture.
pub trait Classifier: Send {
    /// Copies at most the last [`WINDOW_SAMPLES`] samples. Never waits.
    fn submit(&mut self, token: Token, pcm: &[i16]) -> Submit;
    fn poll(&mut self) -> Option<Outcome>;
    fn availability(&self) -> Availability;
    /// Drops a queued request's audio and any unread result. A request the
    /// worker already holds still occupies the slot until it answers or the
    /// worker is replaced; its answer is discarded.
    fn cancel(&mut self);
}
