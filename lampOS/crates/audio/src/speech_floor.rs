//! A level floor under the speech score.
//!
//! The speech model does not look at level. On a lamp it scored 0.8 and over
//! for a second of microphone signal at -82 dBFS, two decibels above the idle
//! floor, and the six blocks that start a request were found in it. Across
//! the recorded sessions nothing else that started a request with the lamp
//! silent had a block under -54 dBFS among its six; see
//! `docs/verification/speech-floor-20261012`.
//!
//! A faint block is held to [`FAINT_SCORE`] and not to zero. That is under
//! what starts a request, so six of them start none. It is not under what
//! counts as silence at the end of one, nor under what [`Hearing`] reads as
//! speech-like, so a request already open ends where it did and the lamp
//! hears over itself what it did. The samples are untouched: a quiet onset
//! stays in the pre-roll of the request that the louder blocks start.
//!
//! This says nothing about whom a sound is addressed to. A sound loud enough
//! to be a person at the lamp passes whatever it is.
//!
//! [`Hearing`]: crate::duplex::Hearing
use crate::CaptureBlock;

/// Sum of squares of a capture block's 160 samples at -70 dBFS: a mean square
/// of 32768² · 10⁻⁷, about ten steps of the 16-bit scale. A block under it,
/// measured after cancellation, is faint.
pub const FAINT_SQUARES: u64 = 17_180;
/// The most a faint block is reported to score.
pub const FAINT_SCORE: f32 = 0.5;

/// The speech score to report for one block: the model's, unless the block
/// is faint.
pub fn reported_score(model: f32, clean: &CaptureBlock) -> f32 {
    if model <= FAINT_SCORE {
        return model;
    }
    let squares: u64 = clean
        .iter()
        .map(|&sample| u64::from(sample.unsigned_abs()).pow(2))
        .sum();
    if squares < FAINT_SQUARES {
        FAINT_SCORE
    } else {
        model
    }
}

#[cfg(test)]
mod tests {
    use super::{FAINT_SCORE, FAINT_SQUARES, reported_score};
    use crate::{CAPTURE_SAMPLES, CaptureBlock, duplex::Hearing};

    const SCORES: [f32; 10] = [0.0, 0.2, 0.34, 0.35, 0.49, 0.5, 0.79, 0.8, 0.97, 1.0];
    /// Three steps of the scale: -81 dBFS, where the recorded start was.
    const FAINT: CaptureBlock = [3; CAPTURE_SAMPLES];
    /// -30 dBFS, where a person at the lamp is.
    const SPOKEN: CaptureBlock = [1_000; CAPTURE_SAMPLES];

    #[test]
    fn the_floor_is_seventy_decibels_under_full_scale() {
        let mean = FAINT_SQUARES as f64 / CAPTURE_SAMPLES as f64;
        let dbfs = 10.0 * (mean / 32_768f64.powi(2)).log10();
        assert!((dbfs + 70.0).abs() < 0.001, "{dbfs}");
        // Ten steps of the scale are under it and eleven are not.
        assert_eq!(reported_score(0.97, &[10; CAPTURE_SAMPLES]), FAINT_SCORE);
        assert_eq!(reported_score(0.97, &[-10; CAPTURE_SAMPLES]), FAINT_SCORE);
        assert_eq!(reported_score(0.97, &[11; CAPTURE_SAMPLES]), 0.97);
        assert_eq!(reported_score(0.97, &[i16::MIN; CAPTURE_SAMPLES]), 0.97);
    }

    #[test]
    fn only_a_faint_block_the_model_scores_over_the_cap_is_changed() {
        for model in SCORES {
            assert_eq!(reported_score(model, &SPOKEN), model);
            assert_eq!(reported_score(model, &FAINT), model.min(FAINT_SCORE));
        }
    }

    #[test]
    fn one_loud_sample_is_enough_for_a_block_not_to_be_faint() {
        let mut click = [0; CAPTURE_SAMPLES];
        click[80] = 132;
        assert_eq!(reported_score(0.97, &click), 0.97);
        click[80] = 131;
        assert_eq!(reported_score(0.97, &click), FAINT_SCORE);
    }

    #[test]
    fn the_duplex_hearing_reads_a_capped_score_as_it_read_the_model() {
        let speech = Hearing::EVALUATED.speech;
        for model in SCORES {
            let reported = reported_score(model, &FAINT);
            assert_eq!(reported >= speech, model >= speech, "{model}");
        }
    }
}
