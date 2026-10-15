//! Bounded audio processing for the Lamp microphone process.
//!
//! The caller supplies the speaker's actual accepted PCM as render reference.
//! Provider-generated but unplayed audio must never enter this reference path.
//! Capture and render are independent 10 ms streams. Device discontinuities
//! must be surfaced and the processor reset; they must not be hidden as silence.
use sonora::config::{EchoCanceller, EchoCanceller3Config, NoiseSuppression};
use sonora::{AudioProcessing, Config, StreamConfig};
use std::io;

#[cfg(target_os = "linux")]
mod alsa_device;
#[cfg(unix)]
pub mod capture;
#[cfg(unix)]
pub mod device;
pub mod drift;
pub mod duplex;
pub mod evidence;
#[cfg(unix)]
pub mod notice;
pub mod ownership;
#[cfg(target_os = "linux")]
pub mod pcm;
pub mod playout;
pub mod qualification;
#[cfg(unix)]
pub mod replay;
pub mod steady;
#[cfg(unix)]
pub mod testing;

pub const CAPTURE_RATE: u32 = 16_000;
pub const RENDER_RATE: u32 = 24_000;
pub const CAPTURE_SAMPLES: usize = 160;
pub const RENDER_SAMPLES: usize = 240;
pub const BLOCK_MS: u32 = 10;

pub type CaptureBlock = [i16; CAPTURE_SAMPLES];
pub type RenderBlock = [i16; RENDER_SAMPLES];
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// How the echo canceller is given the speaker's sound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reference {
    /// As the speaker accepted it. The default.
    #[default]
    AsGiven,
    /// Through a [`steady::SteadyReference`], which holds the lamp's echo in
    /// one place as the canceller sees it (docs/steady-reference.md).
    Steady,
}

/// How the echo canceller's suppressor treats a person heard over the lamp's
/// own voice (#828, docs/stop-word.md). AEC3 lets a person through only once
/// its near-end detector finds them dominant: in the default configuration,
/// residual echo under a quarter of them and they thirty times over the
/// noise, in twelve bands, before it eases its masking. A short word under
/// the lamp's voice rarely gets there, and on lamp-4ace most were taken out
/// with the echo. These are the settings to try; none is the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Suppression {
    /// AEC3 as sonora sets it up. The default.
    #[default]
    Standard,
    /// The near-end detector says a person dominates sooner: residual echo
    /// up to as loud as the person, ten times over the noise, in six bands.
    /// Once it does, the suppressor's own near-end masking applies.
    NearEnd,
    /// Less masking all the time: the normal masks moved halfway to the
    /// near-end ones. More of a person passes, and more of the lamp's echo.
    Transparent,
}

impl Suppression {
    /// The AEC3 configuration for this choice; `None` is sonora's own.
    pub fn echo_canceller3(self) -> Option<EchoCanceller3Config> {
        let mut config = EchoCanceller3Config::default();
        match self {
            Self::Standard => return None,
            Self::NearEnd => {
                let detect = &mut config.suppressor.dominant_nearend_detection;
                detect.enr_threshold = 1.0;
                detect.snr_threshold = 10.0;
                detect.trigger_threshold = 6;
            }
            Self::Transparent => {
                let (normal, nearend) = (
                    &config.suppressor.normal_tuning.clone(),
                    &config.suppressor.nearend_tuning.clone(),
                );
                let half = |a: f32, b: f32| (a + b) / 2.0;
                let tuning = &mut config.suppressor.normal_tuning;
                tuning.mask_lf.enr_transparent = half(
                    normal.mask_lf.enr_transparent,
                    nearend.mask_lf.enr_transparent,
                );
                tuning.mask_lf.enr_suppress =
                    half(normal.mask_lf.enr_suppress, nearend.mask_lf.enr_suppress);
                tuning.mask_hf.enr_transparent = half(
                    normal.mask_hf.enr_transparent,
                    nearend.mask_hf.enr_transparent,
                );
                tuning.mask_hf.enr_suppress =
                    half(normal.mask_hf.enr_suppress, nearend.mask_hf.enr_suppress);
            }
        }
        Some(config)
    }
}

impl std::str::FromStr for Suppression {
    type Err = io::Error;

    fn from_str(value: &str) -> io::Result<Self> {
        match value {
            "standard" => Ok(Self::Standard),
            "near-end" => Ok(Self::NearEnd),
            "transparent" => Ok(Self::Transparent),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "suppressor must be standard, near-end or transparent",
            )),
        }
    }
}

/// A single microphone worker owns this processor; no shared mutex or network I/O.
pub struct EchoProcessor {
    inner: AudioProcessing,
    observer: evidence::EchoObserver,
    steady: Option<Box<steady::SteadyReference>>,
    noise_suppression: bool,
    suppression: Suppression,
    render_input: [f32; RENDER_SAMPLES],
    render_output: [f32; RENDER_SAMPLES],
    capture_input: [f32; CAPTURE_SAMPLES],
    capture_output: [f32; CAPTURE_SAMPLES],
}

impl EchoProcessor {
    pub fn new(noise_suppression: bool) -> Self {
        Self::with_reference(noise_suppression, Reference::AsGiven)
    }

    pub fn with_reference(noise_suppression: bool, reference: Reference) -> Self {
        Self::with_suppression(noise_suppression, reference, Suppression::Standard)
    }

    /// The same, with the suppressor chosen (`Suppression::Standard` is
    /// [`Self::with_reference`] exactly).
    pub fn with_suppression(
        noise_suppression: bool,
        reference: Reference,
        suppression: Suppression,
    ) -> Self {
        Self {
            inner: Self::build(noise_suppression, suppression),
            observer: evidence::EchoObserver::new(),
            steady: (reference == Reference::Steady).then(Box::default),
            noise_suppression,
            suppression,
            render_input: [0.0; RENDER_SAMPLES],
            render_output: [0.0; RENDER_SAMPLES],
            capture_input: [0.0; CAPTURE_SAMPLES],
            capture_output: [0.0; CAPTURE_SAMPLES],
        }
    }

    fn build(noise_suppression: bool, suppression: Suppression) -> AudioProcessing {
        AudioProcessing::builder()
            .config(Config {
                echo_canceller: Some(EchoCanceller::default()),
                echo_canceller3: suppression.echo_canceller3(),
                noise_suppression: noise_suppression.then(NoiseSuppression::default),
                // Do not silently change mic gain or boost background speech.
                ..Config::default()
            })
            .capture_config(StreamConfig::new(CAPTURE_RATE, 1))
            .render_config(StreamConfig::new(RENDER_RATE, 1))
            .build()
    }

    /// Reinitialize after lost reference/capture samples, device reopen or privacy.
    /// Reset is a cold operation and must be recorded as a discontinuity.
    pub fn reset(&mut self) {
        self.inner = Self::build(self.noise_suppression, self.suppression);
        self.observer.reset();
        if let Some(steady) = self.steady.as_mut() {
            steady.reset();
        }
        self.render_input.fill(0.0);
        self.render_output.fill(0.0);
        self.capture_input.fill(0.0);
        self.capture_output.fill(0.0);
    }

    pub fn render(&mut self, samples: &RenderBlock) -> Result<()> {
        let steadied;
        let samples = match self.steady.as_mut() {
            Some(steady) => {
                steadied = *steady.render(samples);
                &steadied
            }
            None => samples,
        };
        pcm_to_float(samples, &mut self.render_input);
        self.inner
            .process_render_f32(&[&self.render_input], &mut [&mut self.render_output])
            .map_err(|e| io::Error::other(format!("AEC render: {e:?}")))?;
        self.observer.render(samples);
        Ok(())
    }

    /// The queue-derived delay is a hint, not a forced acoustic alignment.
    /// Sonora's default AEC3 uses its internal adaptive delay estimator; the
    /// external hint can seed the render buffer after an internal reset.
    pub fn capture(
        &mut self,
        samples: &CaptureBlock,
        device_delay_ms: u16,
    ) -> Result<CaptureBlock> {
        if device_delay_ms > 500 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "AEC device delay exceeds 500 ms",
            )
            .into());
        }
        // With the steady reference, the canceller is handed the speaker's
        // sound that much later, and the delay it is told shrinks to match.
        let device_delay_ms = match self.steady.as_mut() {
            Some(steady) => {
                steady.capture(samples);
                let applied = steady.steadiness().delay_ms.round() as u16;
                device_delay_ms.saturating_sub(applied)
            }
            None => device_delay_ms,
        };
        self.inner
            .set_stream_delay_ms(i32::from(device_delay_ms))
            .map_err(|e| io::Error::other(format!("AEC delay: {e:?}")))?;
        pcm_to_float(samples, &mut self.capture_input);
        self.inner
            .process_capture_f32(&[&self.capture_input], &mut [&mut self.capture_output])
            .map_err(|e| io::Error::other(format!("AEC capture: {e:?}")))?;
        if self.capture_output.iter().any(|sample| !sample.is_finite()) {
            return Err(io::Error::other("AEC returned non-finite audio").into());
        }
        let clean = self.capture_output.map(float_to_pcm);
        self.observer.capture(samples, &clean);
        Ok(clean)
    }

    /// What the render explains of the capture block just processed, before
    /// and after cancellation. `None` until a capture has run since the last
    /// reset. These are measurements for an admission decision made
    /// elsewhere; see [`evidence`].
    pub fn evidence(&self) -> Option<evidence::EchoEvidence> {
        self.observer.latest()
    }

    /// Cached AEC render-buffer alignment, in milliseconds. Reading it does not
    /// run DSP or enable the optional residual-echo detector. `Some` is not
    /// convergence or echo-free evidence: initial and retained alignment also
    /// have values. A new/reset processor reports `None` until capture runs.
    pub fn internal_alignment_ms(&self) -> Option<i32> {
        self.inner.statistics().delay_ms
    }

    /// What the steady reference is doing, when it was chosen.
    pub fn steadiness(&self) -> Option<steady::Steadiness> {
        self.steady.as_ref().map(|steady| steady.steadiness())
    }
}

fn pcm_to_float<const N: usize>(samples: &[i16; N], output: &mut [f32; N]) {
    for (source, target) in samples.iter().zip(output.iter_mut()) {
        *target = f32::from(*source) / 32768.0;
    }
}

pub fn float_to_pcm(sample: f32) -> i16 {
    // Rust's float-to-int conversion saturates; caller-generated audio is finite.
    (sample.clamp(-1.0, 1.0) * 32768.0).round() as i16
}
