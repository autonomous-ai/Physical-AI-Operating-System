//! Whisper-tiny log-mel features with Smart Turn v3's input contract.
//!
//! Mirrors Pipecat `53d313b1`, `src/pipecat/audio/turn/smart_turn/`:
//! `local_smart_turn_v3.py` keeps the last 8 s of 16 kHz audio and pads short
//! input with zeros at the beginning, then `_whisper_features.py` normalizes
//! the waveform to zero mean and unit variance, takes a reflect-padded
//! 400-point periodic-Hann power spectrum every 160 samples, projects it onto
//! 80 Slaney mel filters, drops the trailing frame, applies log10 with an
//! 8-decade dynamic range and maps it to `(x + 4) / 4`.
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::sync::Arc;

pub use lamp_turn_end_wire::{SAMPLE_RATE, WINDOW_SAMPLES};
pub const MELS: usize = 80;
pub const FRAMES: usize = 800;
const N_FFT: usize = 400;
const HOP: usize = 160;
const BINS: usize = N_FFT / 2 + 1;
const MEL_FLOOR: f64 = 1e-10;
const VARIANCE_EPS: f64 = 1e-7;

struct MelFilter {
    first_bin: usize,
    weights: Vec<f64>,
}

pub struct FeatureExtractor {
    fft: Arc<dyn Fft<f64>>,
    window: Vec<f64>,
    filters: Vec<MelFilter>,
    waveform: Vec<f64>,
    frame: Vec<Complex<f64>>,
    scratch: Vec<Complex<f64>>,
    power: Vec<f64>,
}

impl Default for FeatureExtractor {
    fn default() -> Self {
        let fft = FftPlanner::new().plan_fft_forward(N_FFT);
        let scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
        Self {
            fft,
            // numpy.hanning(401)[:-1], the periodic window torch also uses.
            window: (0..N_FFT)
                .map(|n| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / N_FFT as f64).cos())
                .collect(),
            filters: mel_filters(),
            waveform: vec![0.0; WINDOW_SAMPLES + N_FFT],
            frame: vec![Complex::default(); N_FFT],
            scratch,
            power: vec![0.0; BINS * (FRAMES + 1)],
        }
    }
}

impl FeatureExtractor {
    /// Zeroes the reused buffers that hold the last request's normalized
    /// waveform, FFT frames and power spectra. The returned features, freed
    /// temporaries and the model runtime's own memory are not scrubbed.
    pub fn clear(&mut self) {
        self.waveform.fill(0.0);
        self.frame.fill(Complex::default());
        self.scratch.fill(Complex::default());
        self.power.fill(0.0);
    }

    /// Returns `[MELS][FRAMES]` row-major features for the latest 8 s of PCM.
    pub fn compute(&mut self, pcm: &[i16]) -> Vec<f32> {
        let pcm = &pcm[pcm.len().saturating_sub(WINDOW_SAMPLES)..];
        let pad = WINDOW_SAMPLES - pcm.len();
        // Pipecat converts PCM16 with /32768 and normalizes the padded float32
        // buffer, including leading zeros. f64 statistics differ only in rounding.
        let mut sum = 0.0;
        let mut squares = 0.0;
        for &sample in pcm {
            let value = f64::from(f32::from(sample) / 32_768.0);
            sum += value;
            squares += value * value;
        }
        let mean = sum / WINDOW_SAMPLES as f64;
        let variance = (squares / WINDOW_SAMPLES as f64 - mean * mean).max(0.0);
        let scale = 1.0 / (variance + VARIANCE_EPS).sqrt();
        let half = N_FFT / 2;
        let signal = &mut self.waveform[half..half + WINDOW_SAMPLES];
        signal[..pad].fill(-mean * scale);
        for (dst, &sample) in signal[pad..].iter_mut().zip(pcm) {
            *dst = (f64::from(f32::from(sample) / 32_768.0) - mean) * scale;
        }
        // numpy "reflect" padding excludes the edge sample.
        for k in 0..half {
            self.waveform[k] = self.waveform[2 * half - k];
            let last = half + WINDOW_SAMPLES - 1;
            self.waveform[last + 1 + k] = self.waveform[last - 1 - k];
        }
        for frame in 0..=FRAMES {
            let start = frame * HOP;
            for ((dst, &sample), &weight) in self
                .frame
                .iter_mut()
                .zip(&self.waveform[start..start + N_FFT])
                .zip(&self.window)
            {
                *dst = Complex::new(sample * weight, 0.0);
            }
            self.fft
                .process_with_scratch(&mut self.frame, &mut self.scratch);
            let power = &mut self.power[frame * BINS..(frame + 1) * BINS];
            for (dst, value) in power.iter_mut().zip(&self.frame) {
                *dst = value.norm_sqr();
            }
        }
        let mut features = vec![0.0f64; MELS * FRAMES];
        let mut maximum = f64::NEG_INFINITY;
        // The reference drops the trailing frame before taking the maximum.
        for (mel, filter) in self.filters.iter().enumerate() {
            for frame in 0..FRAMES {
                let power = &self.power[frame * BINS + filter.first_bin..];
                let energy = filter
                    .weights
                    .iter()
                    .zip(power)
                    .map(|(weight, value)| weight * value)
                    .sum::<f64>();
                let value = energy.max(MEL_FLOOR).log10();
                maximum = maximum.max(value);
                features[mel * FRAMES + frame] = value;
            }
        }
        features
            .into_iter()
            .map(|value| ((value.max(maximum - 8.0) + 4.0) / 4.0) as f32)
            .collect()
    }
}

fn hertz_to_mel(hertz: f64) -> f64 {
    if hertz >= 1_000.0 {
        15.0 + (hertz / 1_000.0).ln() * (27.0 / 6.4f64.ln())
    } else {
        3.0 * hertz / 200.0
    }
}

fn mel_to_hertz(mel: f64) -> f64 {
    if mel >= 15.0 {
        1_000.0 * (6.4f64.ln() / 27.0 * (mel - 15.0)).exp()
    } else {
        200.0 * mel / 3.0
    }
}

fn linspace(start: f64, stop: f64, count: usize) -> Vec<f64> {
    let step = (stop - start) / (count - 1) as f64;
    let mut values = (0..count)
        .map(|index| start + index as f64 * step)
        .collect::<Vec<_>>();
    values[count - 1] = stop;
    values
}

/// Slaney-normalized triangular filters, stored sparsely by first nonzero bin.
fn mel_filters() -> Vec<MelFilter> {
    let nyquist = (SAMPLE_RATE / 2) as f64;
    let edges = linspace(hertz_to_mel(0.0), hertz_to_mel(nyquist), MELS + 2)
        .into_iter()
        .map(mel_to_hertz)
        .collect::<Vec<_>>();
    let bins = linspace(0.0, nyquist, BINS);
    (0..MELS)
        .map(|mel| {
            let norm = 2.0 / (edges[mel + 2] - edges[mel]);
            let dense = bins
                .iter()
                .map(|&hertz| {
                    let down = -(edges[mel] - hertz) / (edges[mel + 1] - edges[mel]);
                    let up = (edges[mel + 2] - hertz) / (edges[mel + 2] - edges[mel + 1]);
                    down.min(up).max(0.0) * norm
                })
                .collect::<Vec<_>>();
            let first_bin = dense.iter().position(|&w| w > 0.0).unwrap_or(0);
            let last_bin = dense.iter().rposition(|&w| w > 0.0).unwrap_or(0);
            MelFilter {
                first_bin,
                weights: dense[first_bin..=last_bin].to_vec(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_zeroes_every_reused_buffer() {
        let mut extractor = FeatureExtractor::default();
        let loud: Vec<i16> = (0..32_000)
            .map(|i| ((i * 37) % 20_000) as i16 - 10_000)
            .collect();
        extractor.compute(&loud);
        assert!(extractor.waveform.iter().any(|&v| v != 0.0));
        assert!(extractor.power.iter().any(|&v| v != 0.0));
        extractor.clear();
        assert!(extractor.waveform.iter().all(|&v| v == 0.0));
        assert!(extractor.frame.iter().all(|v| v.re == 0.0 && v.im == 0.0));
        assert!(extractor.scratch.iter().all(|v| v.re == 0.0 && v.im == 0.0));
        assert!(extractor.power.iter().all(|&v| v == 0.0));
        // Clearing never changes the next result.
        let mut fresh = FeatureExtractor::default();
        assert_eq!(extractor.compute(&loud), fresh.compute(&loud));
    }

    #[test]
    fn silence_maps_to_the_floor_without_nan() {
        let features = FeatureExtractor::default().compute(&[]);
        assert_eq!(features.len(), MELS * FRAMES);
        // log10(1e-10) = -10, clamped to max - 8 = -18 does not bind: -1.5.
        assert!(features.iter().all(|&value| value == -1.5));
    }

    #[test]
    fn only_the_last_window_is_used() {
        let mut extractor = FeatureExtractor::default();
        let tail = (0..WINDOW_SAMPLES)
            .map(|n| ((n * 7_919) % 4_001) as i16 - 2_000)
            .collect::<Vec<_>>();
        let mut long = vec![12_345i16; 3 * SAMPLE_RATE];
        long.extend_from_slice(&tail);
        assert_eq!(extractor.compute(&long), extractor.compute(&tail));
    }

    #[test]
    fn filters_are_bounded_and_nonempty() {
        let filters = mel_filters();
        assert_eq!(filters.len(), MELS);
        for filter in &filters {
            assert!(!filter.weights.is_empty());
            assert!(filter.first_bin + filter.weights.len() <= BINS);
        }
    }
}
