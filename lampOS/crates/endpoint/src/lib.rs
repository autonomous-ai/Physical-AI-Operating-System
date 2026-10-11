//! Local end-of-turn confirmation with the pinned Smart Turn v3.2 model.
//!
//! The model judges whether retained speech sounds finished. It does not
//! identify a speaker or an addressee, and its verdict is a probability, not a
//! guarantee. Callers keep their own silence fallback for a missing, loading,
//! busy, slow or failed model.
use std::error::Error;

pub mod features;
pub mod model;
pub mod worker;

pub use features::{FRAMES, FeatureExtractor, MELS, SAMPLE_RATE, WINDOW_SAMPLES};
pub use model::{ModelAsset, SMART_TURN_V3_2, SmartTurn};
pub use worker::{
    Availability, Classifier, Outcome, SmartTurnWorker, Startup, Submit, Token, Verdict,
};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
