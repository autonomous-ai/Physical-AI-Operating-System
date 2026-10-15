//! Whether two things said are the same voice (#829).
//!
//! A speaker embedding of each utterance, and the cosine between two. The
//! model is CAM++ from 3D-Speaker (Apache-2.0), as sherpa-onnx exports it:
//! 80 Kaldi filter banks a frame in, one vector out. The filter banks are
//! computed here as Kaldi computes them (25 ms frames every 10 ms, the
//! Povey window, pre-emphasis 0.97, the DC offset removed, a 512-point FFT,
//! 80 triangular bands from 20 Hz to 8 kHz on the mel scale, the log of
//! their power), with the utterance's mean taken off each band, as
//! 3D-Speaker does before the model.
//!
//! Nothing on the lamp runs this yet. It is here to be measured: how often
//! the same person is taken for another, and another for the same, on what
//! the lamp's own microphone hears (`docs/same-voice.md`).
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::{path::Path, sync::Arc};
use tract_onnx::prelude::*;

/// Samples a second, the only rate taken.
pub const RATE: u32 = 16_000;
/// Filter banks a frame.
pub const BINS: usize = 80;
const FRAME: usize = 400;
const SHIFT: usize = 160;
const FFT_SIZE: usize = 512;
const PREEMPHASIS: f32 = 0.97;
const LOW_HZ: f32 = 20.0;

fn mel(hz: f32) -> f32 {
    1127.0 * (1.0 + hz / 700.0).ln()
}

/// Kaldi's filter banks for 16 kHz, as `torchaudio.compliance.kaldi.fbank`
/// computes them with its defaults and no dither.
pub struct Fbank {
    window: Vec<f32>,
    /// For each band, its first FFT bin and the weights from there.
    bands: Vec<(usize, Vec<f32>)>,
    fft: Arc<dyn Fft<f32>>,
}

impl Default for Fbank {
    fn default() -> Self {
        Self::new()
    }
}

impl Fbank {
    pub fn new() -> Self {
        let window = (0..FRAME)
            .map(|i| {
                let hann =
                    0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FRAME - 1) as f32).cos();
                hann.powf(0.85)
            })
            .collect();
        let nyquist = RATE as f32 / 2.0;
        let (low, high) = (mel(LOW_HZ), mel(nyquist));
        let step = (high - low) / (BINS + 1) as f32;
        let bins = FFT_SIZE / 2;
        let bands = (0..BINS)
            .map(|band| {
                let left = low + band as f32 * step;
                let center = left + step;
                let right = center + step;
                let weights: Vec<(usize, f32)> = (0..bins)
                    .filter_map(|bin| {
                        let m = mel(bin as f32 * RATE as f32 / FFT_SIZE as f32);
                        let weight = if m > left && m <= center {
                            (m - left) / (center - left)
                        } else if m > center && m < right {
                            (right - m) / (right - center)
                        } else {
                            0.0
                        };
                        (weight > 0.0).then_some((bin, weight))
                    })
                    .collect();
                let first = weights.first().map_or(0, |(bin, _)| *bin);
                let last = weights.last().map_or(0, |(bin, _)| *bin);
                let mut dense = vec![0.0; last + 1 - first];
                for (bin, weight) in weights {
                    dense[bin - first] = weight;
                }
                (first, dense)
            })
            .collect();
        Self {
            window,
            bands,
            fft: FftPlanner::new().plan_fft_forward(FFT_SIZE),
        }
    }

    /// The filter banks of `samples` (in [-1, 1]), one row a frame, the
    /// utterance's mean taken off each band. Empty when shorter than a
    /// frame.
    pub fn features(&self, samples: &[f32]) -> Vec<[f32; BINS]> {
        let mut rows = self.log_banks(samples);
        for band in 0..BINS {
            let mean = rows.iter().map(|row| row[band]).sum::<f32>() / rows.len().max(1) as f32;
            rows.iter_mut().for_each(|row| row[band] -= mean);
        }
        rows
    }

    /// The log filter banks, before the mean is taken off.
    fn log_banks(&self, samples: &[f32]) -> Vec<[f32; BINS]> {
        if samples.len() < FRAME {
            return Vec::new();
        }
        let frames = 1 + (samples.len() - FRAME) / SHIFT;
        let mut rows = Vec::with_capacity(frames);
        let mut buffer = vec![Complex::new(0.0f32, 0.0); FFT_SIZE];
        let mut frame = [0.0f32; FRAME];
        for index in 0..frames {
            frame.copy_from_slice(&samples[index * SHIFT..index * SHIFT + FRAME]);
            let mean = frame.iter().sum::<f32>() / FRAME as f32;
            frame.iter_mut().for_each(|sample| *sample -= mean);
            for i in (1..FRAME).rev() {
                frame[i] -= PREEMPHASIS * frame[i - 1];
            }
            frame[0] -= PREEMPHASIS * frame[0];
            for (slot, (sample, weight)) in buffer.iter_mut().zip(frame.iter().zip(&self.window)) {
                *slot = Complex::new(sample * weight, 0.0);
            }
            buffer[FRAME..].fill(Complex::new(0.0, 0.0));
            self.fft.process(&mut buffer);
            let mut row = [0.0f32; BINS];
            for (value, (first, weights)) in row.iter_mut().zip(&self.bands) {
                let energy: f32 = weights
                    .iter()
                    .zip(&buffer[*first..])
                    .map(|(weight, bin)| weight * bin.norm_sqr())
                    .sum();
                *value = energy.max(f32::EPSILON).ln();
            }
            rows.push(row);
        }
        rows
    }
}

/// The speaker model, ready to run on any number of frames.
pub struct Voice {
    plan: Arc<TypedRunnableModel>,
    fbank: Fbank,
}

impl Voice {
    pub fn load(path: &Path) -> Result<Self, String> {
        let said = |error: TractError| format!("{}: {error}", path.display());
        let model = tract_onnx::onnx()
            .with_ignore_value_info(true)
            .with_ignore_output_shapes(true)
            .model_for_path(path)
            .map_err(said)?;
        let frames = model.symbols.sym("T");
        let shape = [1.to_dim(), frames.to_dim(), (BINS as i64).to_dim()];
        let plan = model
            .with_input_fact(0, f32::fact(shape).into())
            .map_err(said)?
            .into_optimized()
            .map_err(said)?
            .into_runnable()
            .map_err(said)?;
        Ok(Self {
            plan,
            fbank: Fbank::new(),
        })
    }

    /// The embedding of `samples` (16 kHz, in [-1, 1]).
    pub fn embedding(&self, samples: &[f32]) -> Result<Vec<f32>, String> {
        let rows = self.fbank.features(samples);
        if rows.is_empty() {
            return Err("shorter than one frame".into());
        }
        let flat: Vec<f32> = rows.iter().flatten().copied().collect();
        let said = |error: TractError| error.to_string();
        let input = Tensor::from_shape(&[1, rows.len(), BINS], &flat).map_err(said)?;
        let output = self.plan.run(tvec!(input.into())).map_err(said)?;
        let first = output.first().ok_or("no output")?;
        Ok(first
            .to_plain_array_view::<f32>()
            .map_err(said)?
            .iter()
            .copied()
            .collect())
    }
}

/// The cosine between two embeddings: 1 for the same direction, whatever
/// their scale, summed in f64 so that neither very small nor very large
/// values lose it. None where it is not defined: vectors of different
/// lengths, an empty or all-zero one, or one with a value that is not
/// finite.
pub fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.iter().chain(b).any(|value| !value.is_finite()) {
        return None;
    }
    let (mut dot, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
    for (&x, &y) in a.iter().zip(b) {
        let (x, y) = (f64::from(x), f64::from(y));
        dot += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa == 0.0 || bb == 0.0 {
        return None;
    }
    Some((dot / (aa.sqrt() * bb.sqrt())).clamp(-1.0, 1.0) as f32)
}

/// A 16 kHz mono 16-bit WAV, in [-1, 1].
pub fn read_wav(path: &Path) -> Result<Vec<f32>, String> {
    let said = |error: hound::Error| format!("{}: {error}", path.display());
    let reader = hound::WavReader::open(path).map_err(said)?;
    let spec = reader.spec();
    if spec.sample_rate != RATE || spec.channels != 1 || spec.bits_per_sample != 16 {
        return Err(format!("{}: not 16 kHz mono 16-bit", path.display()));
    }
    reader
        .into_samples::<i16>()
        .map(|sample| {
            sample
                .map(|value| f32::from(value) / 32_768.0)
                .map_err(said)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_is_25_ms_every_10_ms_and_each_band_is_centred() {
        let fbank = Fbank::new();
        assert!(fbank.features(&[0.1; 399]).is_empty());
        let one_second = vec![0.0; 16_000];
        assert_eq!(fbank.features(&one_second).len(), 98);
        // A tone lights the band around it most, and the mean is taken off.
        let tone: Vec<f32> = (0..16_000)
            .map(|i| (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / 16_000.0).sin() * 0.5)
            .collect();
        let raw = fbank.log_banks(&tone);
        let loudest = (0..BINS)
            .max_by(|a, b| raw[40][*a].total_cmp(&raw[40][*b]))
            .unwrap();
        let rows = fbank.features(&tone);
        let center_hz = |band: usize| {
            let (low, high) = (mel(LOW_HZ), mel(8_000.0));
            let m = low + (band + 1) as f32 * (high - low) / (BINS + 1) as f32;
            700.0 * ((m / 1127.0).exp() - 1.0)
        };
        assert!(
            (center_hz(loudest) - 1_000.0).abs() < 60.0,
            "{}",
            center_hz(loudest)
        );
        for band in 0..BINS {
            let mean: f32 = rows.iter().map(|row| row[band]).sum::<f32>() / rows.len() as f32;
            assert!(mean.abs() < 1e-3);
        }
    }

    #[test]
    fn the_cosine_is_one_for_the_same_and_zero_for_a_right_angle() {
        assert!((cosine(&[1.0, 2.0], &[2.0, 4.0]).unwrap() - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 3.0]).unwrap().abs() < 1e-6);
        // The same at any scale (the reviewer's on #865).
        assert_eq!(cosine(&[0.0001, 0.0], &[0.0001, 0.0]), Some(1.0));
        assert_eq!(cosine(&[1e-30, 2e-30], &[2e-30, 4e-30]), Some(1.0));
        assert_eq!(cosine(&[1e20, 0.0], &[1e20, 0.0]), Some(1.0));
        assert_eq!(cosine(&[3e38, -3e38], &[-3e38, 3e38]), Some(-1.0));
        // Not defined: said so, never a number.
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 0.0]), None);
        assert_eq!(cosine(&[], &[]), None);
        assert_eq!(cosine(&[1.0], &[1.0, 0.0]), None);
        assert_eq!(cosine(&[f32::NAN, 1.0], &[1.0, 1.0]), None);
        assert_eq!(cosine(&[f32::INFINITY, 1.0], &[1.0, 1.0]), None);
    }
}
