//! Fixture audio: 16 kHz mono PCM16 files, macOS `say` renderings and the
//! deterministic noise floor added to every composed replay.
use crate::Result;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

pub const RATE: usize = 16_000;
pub const BLOCK: usize = 160;
/// Uniform noise of +/-40 LSB: about -63 dBFS RMS.
const NOISE: u32 = 40;

pub fn read_wav(path: &Path) -> Result<Vec<i16>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.sample_rate != RATE as u32 || spec.channels != 1 || spec.bits_per_sample != 16 {
        return Err(format!("{} is not 16 kHz mono PCM16", path.display()).into());
    }
    Ok(reader
        .samples::<i16>()
        .collect::<std::result::Result<_, _>>()?)
}

/// Renders once with macOS `say` into a cache keyed by voice, rate and text.
pub fn say(voice: &str, wpm: u32, text: &str, cache: &Path) -> Result<Vec<i16>> {
    let key = Sha256::digest(format!("{voice}\n{wpm}\n{text}"));
    let name = key.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let path = cache.join(format!("{name}.wav"));
    if !path.exists() {
        if !cfg!(target_os = "macos") {
            return Err(format!("missing rendered clip {}", path.display()).into());
        }
        fs::create_dir_all(cache)?;
        let partial = cache.join(format!(".{name}.wav"));
        let status = Command::new("/usr/bin/say")
            .args(["-v", voice, "-r", &wpm.to_string(), "-o"])
            .arg(&partial)
            .args(["--file-format=WAVE", "--data-format=LEI16@16000", text])
            .status()?;
        if !status.success() {
            return Err(format!("say failed for {text:?}").into());
        }
        fs::rename(&partial, &path)?;
    }
    read_wav(&path)
}

pub fn gain(samples: &mut [i16], db: f64) {
    let factor = 10f64.powf(db / 20.0);
    for sample in samples {
        *sample = (f64::from(*sample) * factor)
            .round()
            .clamp(-32_768.0, 32_767.0) as i16;
    }
}

pub fn silence(ms: usize) -> Vec<i16> {
    vec![0; ms * RATE / 1_000]
}

/// Pads to whole blocks and adds noise seeded by `seed`, so every replay of
/// the same fixture sees identical samples.
pub fn finish(mut pcm: Vec<i16>, seed: &str) -> Vec<i16> {
    pcm.resize(pcm.len().div_ceil(BLOCK) * BLOCK, 0);
    let mut state = seed.bytes().fold(2_166_136_261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16_777_619)
    }) | 1;
    for sample in &mut pcm {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let noise = (state % (2 * NOISE + 1)) as i32 - NOISE as i32;
        *sample = (i32::from(*sample) + noise).clamp(-32_768, 32_767) as i16;
    }
    pcm
}
