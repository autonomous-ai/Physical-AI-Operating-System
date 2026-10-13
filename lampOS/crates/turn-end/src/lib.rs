//! Local end-of-turn classification with the pinned Smart Turn v3.2 model.
//!
//! The model judges whether retained speech sounds finished. It does not
//! identify a speaker or an addressee, and its verdict is a probability, not a
//! guarantee. It runs only in the separate `lamp-turn-end` process; the
//! core keeps its own silence fallback for a missing, loading, busy, slow or
//! failed worker.
use std::{error::Error, time::Instant};

pub mod features;
pub mod model;
pub mod reference;

pub use features::{FRAMES, FeatureExtractor, MELS};
pub use lamp_turn_end_wire::{SAMPLE_RATE, WINDOW_SAMPLES};
pub use model::{ModelAsset, SMART_TURN_V3_2, SmartTurn};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

fn elapsed_us(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// One classification with its stage costs, as the worker reports it.
pub fn classify(
    model: &SmartTurn,
    extractor: &mut FeatureExtractor,
    pcm: &[i16],
) -> std::result::Result<lamp_turn_end_wire::serve::Classified, String> {
    let started = Instant::now();
    let features = extractor.compute(pcm);
    // The waveform and spectra are not needed once the features exist.
    extractor.clear();
    let features_us = elapsed_us(started);
    let started = Instant::now();
    let probability = model
        .probability(features)
        .map_err(|error| error.to_string())?;
    Ok(lamp_turn_end_wire::serve::Classified {
        probability,
        features_us,
        inference_us: elapsed_us(started),
    })
}
