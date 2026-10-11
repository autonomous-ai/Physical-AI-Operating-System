//! Locate a known reference signal (a certified render segment or an exact
//! stimulus) inside a room recording, for boundary proposals (`MAIN-021`).
//!
//! The two recordings have independent, unsynchronized sample clocks and
//! pass through a real room, so no host time is trusted. A coarse search
//! correlates gain-independent energy envelopes over the whole recording and
//! keeps the best separated candidates; each is refined by waveform
//! correlation within a small window. Absolute correlation is used, so an
//! inverted speaker still matches (the sign is reported).
//!
//! The result is a proposal with alternatives, never an annotation. The
//! acceptance thresholds below are unqualified heuristics; every score and
//! margin is reported so a person can judge it.
use crate::{Result, invalid};
use serde::Serialize;

/// Envelope frame: 10 ms at 16 kHz.
pub const FRAME: usize = 160;
/// Coarse candidates kept, separated by at least `SEPARATION_FRAMES`.
const CANDIDATES: usize = 5;
const SEPARATION_FRAMES: usize = 20;
/// Waveform refinement window around a coarse candidate, in samples (±20 ms).
const REFINE: i64 = 320;
/// At most this much of the reference (from its start) is used to refine.
const REFINE_SAMPLES: usize = 2 * 16_000;
/// Heuristic acceptance: minimum refined |correlation| and lead over the
/// runner-up. Unqualified on real rooms.
pub const MIN_SCORE: f64 = 0.2;
pub const MIN_MARGIN: f64 = 0.05;
/// References shorter than this are too short to locate by envelope.
pub const MIN_REFERENCE_FRAMES: usize = 20;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Candidate {
    /// Room sample where reference sample 0 lands.
    pub offset: i64,
    /// Refined |normalized correlation| (0..1).
    pub score: f64,
    /// Polarity of the refined correlation.
    pub inverted: bool,
    /// Coarse envelope correlation at this candidate.
    pub envelope_score: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Located,
    Ambiguous,
    NotFound,
}

#[derive(Clone, Debug, Serialize)]
pub struct Search {
    pub status: Status,
    /// Best first, at most `CANDIDATES`.
    pub candidates: Vec<Candidate>,
    /// Best score minus the runner-up's (1.0 when there is none).
    pub margin: f64,
    /// Half-width of the refined peak at 90% of its height, plus one sample.
    pub uncertainty_samples: f64,
    /// Fraction of room samples at the 16-bit rails over the located window.
    pub clipped_fraction: Option<f64>,
}

impl Search {
    pub fn best(&self) -> Option<&Candidate> {
        self.candidates.first()
    }
}

/// Supported sample rates for resampling, inclusive.
pub const MIN_RATE: u32 = 8_000;
pub const MAX_RATE: u32 = 192_000;
/// Longest resampled output, in seconds at the output rate.
pub const MAX_SECONDS: u64 = 600;

/// Band-limited rational resampling with a Hann-windowed sinc (32 taps each
/// side). Used to bring the 24 kHz render to the room's 16 kHz. Rates and
/// the output length are validated before any division or allocation.
pub fn resample(input: &[f32], from: u32, to: u32) -> Result<Vec<f32>> {
    if !(MIN_RATE..=MAX_RATE).contains(&from) || !(MIN_RATE..=MAX_RATE).contains(&to) {
        return Err(invalid("sample rate must be 8 to 192 kHz for resampling"));
    }
    let count = u64::try_from(input.len() as u128 * u128::from(to) / u128::from(from))
        .ok()
        .filter(|count| *count <= u64::from(to) * MAX_SECONDS)
        .ok_or_else(|| invalid("resampled signal would exceed 600 s"))?;
    if from == to || input.is_empty() {
        return Ok(input.to_vec());
    }
    const TAPS: i64 = 32;
    let ratio = f64::from(from) / f64::from(to);
    // Low-pass at the lower Nyquist frequency.
    let cutoff = (f64::from(to) / f64::from(from)).min(1.0);
    let count = count as usize;
    let mut output = Vec::with_capacity(count);
    for n in 0..count {
        let x = n as f64 * ratio;
        let centre = x.floor() as i64;
        let mut sum = 0.0_f64;
        let mut weight = 0.0_f64;
        for k in centre - TAPS + 1..=centre + TAPS {
            if k < 0 || k as usize >= input.len() {
                continue;
            }
            let distance = x - k as f64;
            let window = 0.5 + 0.5 * (std::f64::consts::PI * distance / TAPS as f64).cos();
            let argument = std::f64::consts::PI * cutoff * distance;
            let sinc = if argument.abs() < 1e-9 {
                1.0
            } else {
                argument.sin() / argument
            };
            let tap = cutoff * sinc * window;
            sum += f64::from(input[k as usize]) * tap;
            weight += tap;
        }
        output.push(if weight.abs() > 1e-9 {
            (sum / weight) as f32
        } else {
            0.0
        });
    }
    Ok(output)
}

/// Root-mean-square energy per `FRAME` samples.
pub fn envelope(samples: &[f32]) -> Vec<f64> {
    samples
        .chunks(FRAME)
        .map(|frame| {
            let energy: f64 = frame.iter().map(|&v| f64::from(v) * f64::from(v)).sum();
            (energy / frame.len() as f64).sqrt()
        })
        .collect()
}

/// First and last sample whose absolute value exceeds `threshold`.
pub fn active_span(samples: &[f32], threshold: f32) -> Option<(usize, usize)> {
    let first = samples.iter().position(|v| v.abs() > threshold)?;
    let last = samples.iter().rposition(|v| v.abs() > threshold)?;
    Some((first, last))
}

fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let mean_a = a.iter().sum::<f64>() / n;
    let mean_b = b.iter().sum::<f64>() / n;
    let (mut cov, mut var_a, mut var_b) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        let (dx, dy) = (x - mean_a, y - mean_b);
        cov += dx * dy;
        var_a += dx * dx;
        var_b += dy * dy;
    }
    if var_a <= f64::EPSILON || var_b <= f64::EPSILON {
        0.0
    } else {
        cov / (var_a * var_b).sqrt()
    }
}

/// Normalized waveform correlation of `reference` against `room` at `offset`.
fn correlate(room: &[f32], reference: &[f32], offset: i64) -> Option<f64> {
    if offset < 0 || offset as usize + reference.len() > room.len() {
        return None;
    }
    let window = &room[offset as usize..offset as usize + reference.len()];
    let (mut dot, mut energy_r, mut energy_w) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (r, w) in reference.iter().zip(window) {
        let (r, w) = (f64::from(*r), f64::from(*w));
        dot += r * w;
        energy_r += r * r;
        energy_w += w * w;
    }
    if energy_r <= f64::EPSILON || energy_w <= f64::EPSILON {
        return Some(0.0);
    }
    Some(dot / (energy_r * energy_w).sqrt())
}

/// Locate `reference` in `room` (same sample rate). `None` when the
/// reference has no content or does not fit inside the room recording.
pub fn locate(room: &[f32], reference: &[f32], clipped: Option<&[i16]>) -> Option<Search> {
    let reference_envelope = envelope(reference);
    let room_envelope = envelope(room);
    if reference_envelope.len() < MIN_REFERENCE_FRAMES
        || reference_envelope.len() > room_envelope.len()
        || reference_envelope.iter().all(|v| *v <= f64::EPSILON)
    {
        return None;
    }
    // Coarse: envelope correlation at every frame lag where it fits.
    let lags = room_envelope.len() - reference_envelope.len() + 1;
    let coarse: Vec<f64> = (0..lags)
        .map(|lag| {
            pearson(
                &reference_envelope,
                &room_envelope[lag..lag + reference_envelope.len()],
            )
        })
        .collect();
    let mut order: Vec<usize> = (0..lags).collect();
    order.sort_by(|a, b| coarse[*b].total_cmp(&coarse[*a]));
    let mut picked: Vec<usize> = Vec::new();
    for lag in order {
        if picked.len() == CANDIDATES {
            break;
        }
        if picked.iter().all(|p| p.abs_diff(lag) >= SEPARATION_FRAMES) {
            picked.push(lag);
        }
    }
    // Fine: waveform correlation around each coarse candidate.
    let head = &reference[..reference.len().min(REFINE_SAMPLES)];
    let mut candidates = Vec::new();
    let mut peaks = Vec::new();
    for lag in picked {
        let centre = (lag * FRAME) as i64;
        let scores: Vec<(i64, f64)> = (centre - REFINE..=centre + REFINE)
            .filter_map(|offset| correlate(room, head, offset).map(|s| (offset, s)))
            .collect();
        let Some(&(offset, score)) = scores.iter().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        else {
            continue;
        };
        candidates.push(Candidate {
            offset,
            score: score.abs(),
            inverted: score < 0.0,
            envelope_score: coarse[lag],
        });
        peaks.push(scores);
    }
    if candidates.is_empty() {
        return None;
    }
    let mut ranked: Vec<usize> = (0..candidates.len()).collect();
    ranked.sort_by(|a, b| candidates[*b].score.total_cmp(&candidates[*a].score));
    let best = ranked[0];
    let peak = &peaks[best];
    let height = candidates[best].score;
    let width = peak
        .iter()
        .filter(|(_, s)| s.abs() >= 0.9 * height)
        .map(|(o, _)| (o - candidates[best].offset).abs())
        .max()
        .unwrap_or(0);
    let candidates: Vec<Candidate> = ranked.into_iter().map(|i| candidates[i].clone()).collect();
    let margin = candidates
        .get(1)
        .map_or(1.0, |second| candidates[0].score - second.score);
    let status = if candidates[0].score < MIN_SCORE {
        Status::NotFound
    } else if margin < MIN_MARGIN {
        Status::Ambiguous
    } else {
        Status::Located
    };
    let clipped_fraction = clipped.and_then(|raw| {
        let start = usize::try_from(candidates[0].offset).ok()?;
        let window = raw.get(start..(start + reference.len()).min(raw.len()))?;
        (!window.is_empty()).then(|| {
            window
                .iter()
                .filter(|v| **v == i16::MAX || **v == i16::MIN)
                .count() as f64
                / window.len() as f64
        })
    });
    Some(Search {
        status,
        candidates,
        margin,
        uncertainty_samples: width as f64 + 1.0,
        clipped_fraction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic speech-like signal: noise shaped by a syllable envelope.
    fn speechlike(seed: u64, samples: usize, rate: f64) -> Vec<f32> {
        let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        let mut low = 0.0_f32;
        (0..samples)
            .map(|n| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let noise = (state >> 40) as f32 / (1u64 << 24) as f32 - 0.5;
                low = 0.7 * low + 0.3 * noise;
                let t = n as f64 / rate;
                let syllable = (std::f64::consts::PI * 4.0 * t + seed as f64).sin().abs();
                (low as f64 * syllable * 8000.0) as f32
            })
            .collect()
    }

    fn room_with(parts: &[(&[f32], usize, f32)], length: usize, noise: f32) -> Vec<f32> {
        let mut room = speechlike(99, length, 16_000.0)
            .into_iter()
            .map(|v| v * noise)
            .collect::<Vec<_>>();
        for (signal, at, gain) in parts {
            for (i, v) in signal.iter().enumerate() {
                if let Some(slot) = room.get_mut(at + i) {
                    *slot += v * gain;
                }
            }
        }
        room
    }

    #[test]
    fn a_reference_is_located_at_its_sample_despite_a_similar_distractor() {
        let reply = speechlike(1, 24_000, 16_000.0);
        let distractor = speechlike(2, 24_000, 16_000.0);
        let room = room_with(
            &[(&distractor, 20_000, 1.0), (&reply, 70_123, 0.3)],
            120_000,
            0.02,
        );
        let search = locate(&room, &reply, None).unwrap();
        assert_eq!(search.status, Status::Located, "{search:?}");
        assert_eq!(search.best().unwrap().offset, 70_123);
        assert!(search.margin > 0.3, "{search:?}");
    }

    #[test]
    fn a_24k_render_is_located_in_a_16k_room_and_drift_widens_nothing_much() {
        let reply_24k = speechlike(3, 36_000, 24_000.0);
        let reply_16k = resample(&reply_24k, 24_000, 16_000).unwrap();
        assert_eq!(reply_16k.len(), 24_000);
        // The room hears the same audio on a clock 100 ppm fast.
        let stretched = resample(&reply_24k, 24_000, 16_002).unwrap();
        let room = room_with(&[(&stretched, 40_000, 0.5)], 90_000, 0.02);
        let search = locate(&room, &reply_16k, None).unwrap();
        assert_eq!(search.status, Status::Located, "{search:?}");
        assert!(
            (search.best().unwrap().offset - 40_000).abs() <= 2,
            "{search:?}"
        );
    }

    #[test]
    fn two_identical_copies_are_ambiguous_and_both_are_kept() {
        let reply = speechlike(4, 16_000, 16_000.0);
        let room = room_with(
            &[(&reply, 10_000, 0.5), (&reply, 60_000, 0.5)],
            90_000,
            0.02,
        );
        let search = locate(&room, &reply, None).unwrap();
        assert_eq!(search.status, Status::Ambiguous, "{search:?}");
        let offsets: Vec<i64> = search.candidates.iter().take(2).map(|c| c.offset).collect();
        assert!(
            offsets.contains(&10_000) && offsets.contains(&60_000),
            "{offsets:?}"
        );
    }

    #[test]
    fn an_absent_reference_is_not_found() {
        let reply = speechlike(5, 16_000, 16_000.0);
        let other = speechlike(6, 16_000, 16_000.0);
        let room = room_with(&[(&other, 30_000, 0.5)], 90_000, 0.02);
        let search = locate(&room, &reply, None).unwrap();
        assert_eq!(search.status, Status::NotFound, "{search:?}");
    }

    #[test]
    fn inverted_and_clipped_audio_is_still_located_and_reported() {
        let reply = speechlike(7, 16_000, 16_000.0);
        let inverted: Vec<f32> = reply.iter().map(|v| -v * 12.0).collect();
        let room = room_with(&[(&inverted, 50_000, 1.0)], 90_000, 0.02);
        let raw: Vec<i16> = room
            .iter()
            .map(|v| v.clamp(-32768.0, 32767.0) as i16)
            .collect();
        let clipped: Vec<f32> = raw.iter().map(|v| f32::from(*v)).collect();
        let search = locate(&clipped, &reply, Some(&raw)).unwrap();
        assert_eq!(search.best().unwrap().offset, 50_000, "{search:?}");
        assert!(search.best().unwrap().inverted);
        assert!(search.clipped_fraction.unwrap() > 0.0, "{search:?}");
    }

    #[test]
    fn silent_short_or_oversized_references_are_refused() {
        let room = room_with(&[], 50_000, 0.02);
        assert!(locate(&room, &[0.0; 16_000], None).is_none());
        assert!(locate(&room, &speechlike(8, 1_000, 16_000.0), None).is_none());
        assert!(locate(&room, &speechlike(9, 60_000, 16_000.0), None).is_none());
    }

    #[test]
    fn active_span_finds_the_first_and_last_audible_samples() {
        let mut pcm = vec![0.0_f32; 100];
        pcm[10] = 50.0;
        pcm[80] = -60.0;
        assert_eq!(active_span(&pcm, 20.0), Some((10, 80)));
        assert_eq!(active_span(&[0.0; 10], 20.0), None);
    }
}
