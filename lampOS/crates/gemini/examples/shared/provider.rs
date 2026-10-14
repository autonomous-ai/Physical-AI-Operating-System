//! What the service probes share: the private provider file the runtime
//! uses, read into a session configuration, and a spoken WAV fixture read as
//! 16 kHz mono. Nothing here prints a credential or a handle.
use lamp_gemini::{Credential, SessionConfig, ThinkingLevel};
use serde::Deserialize;
use std::{error::Error, fs, os::unix::fs::PermissionsExt, path::Path};

pub type Outcome<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// The fields of the voice's provider file a probe uses; the voice's own
/// file has more (`instruction_file`, `resumption`, `search`, ...), which a
/// probe ignores, so that it reads that file as it is.
#[derive(Deserialize)]
pub struct Provider {
    endpoint: String,
    model: String,
    voice: String,
    credential_file: String,
    #[serde(default)]
    authentication: Option<String>,
    #[serde(default)]
    thinking_level: Option<ThinkingLevel>,
    #[serde(default)]
    language_code: Option<String>,
}

/// Regular file, readable by its owner only, of bounded size.
pub fn private(path: &Path, max: u64) -> Outcome<String> {
    let info = fs::symlink_metadata(path)?;
    if !info.is_file() || info.permissions().mode() & 0o077 != 0 || info.len() > max {
        return Err("configuration must be a bounded regular file with mode 0600".into());
    }
    Ok(fs::read_to_string(path)?)
}

pub fn session(path: &Path) -> Outcome<SessionConfig> {
    let provider: Provider = serde_json::from_str(&private(path, 16_384)?)
        .map_err(|_| "invalid private provider configuration")?;
    let secret = private(Path::new(&provider.credential_file), 4_098)?;
    let secret = secret.trim_end_matches(['\r', '\n']);
    let credential = match provider.authentication.as_deref() {
        None | Some("api_key") => Credential::api_key(secret)?,
        Some("bearer") => Credential::bearer(secret)?,
        Some("ephemeral") => Credential::ephemeral(secret)?,
        Some(_) => return Err("unknown authentication".into()),
    };
    let mut config = SessionConfig::new(&provider.endpoint, credential)?
        .model(&provider.model)?
        .voice(&provider.voice)?
        .instruction("You are a voice assistant. Answer every question in one short sentence.")?;
    if let Some(level) = provider.thinking_level {
        config = config.thinking_level(level)?;
    }
    if let Some(language) = &provider.language_code {
        config = config.language_code(language)?;
    }
    Ok(config)
}

/// 16 kHz mono from a 16-bit PCM WAV, by linear interpolation. Adequate for a
/// spoken fixture; not a production decoder or resampler.
pub fn speech(path: &Path) -> Outcome<Vec<i16>> {
    let bytes = fs::read(path)?;
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("speech fixture must be a RIFF WAVE file".into());
    }
    let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let long =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let (mut format, mut data, mut at) = (None, None, 12);
    while at + 8 <= bytes.len() {
        let size = long(at + 4) as usize;
        let body = at + 8;
        let end = body.saturating_add(size).min(bytes.len());
        match &bytes[at..at + 4] {
            b"fmt " if size >= 16 && end - body >= 16 => {
                format = Some((word(body), word(body + 2), long(body + 4), word(body + 14)));
            }
            b"data" => data = Some(&bytes[body..end]),
            _ => {}
        }
        at = end + (size & 1);
    }
    let (Some((kind, channels, rate, bits)), Some(data)) = (format, data) else {
        return Err("speech fixture has no format or data chunk".into());
    };
    // 1 is integer PCM; 0xFFFE is the extensible form of the same.
    if !matches!(kind, 1 | 0xFFFE) || bits != 16 || channels == 0 || rate == 0 {
        return Err("speech fixture must be 16-bit integer PCM".into());
    }
    let channels = usize::from(channels);
    let mono: Vec<f32> = data
        .chunks_exact(2 * channels)
        .map(|frame| {
            frame
                .chunks_exact(2)
                .map(|pair| f32::from(i16::from_le_bytes([pair[0], pair[1]])))
                .sum::<f32>()
                / channels as f32
        })
        .collect();
    if mono.is_empty() {
        return Err("speech fixture holds no audio".into());
    }
    let step = rate as f32 / 16_000.0;
    let length = (mono.len() as f32 / step) as usize;
    Ok((0..length)
        .map(|index| {
            let position = index as f32 * step;
            let (low, fraction) = (position as usize, position.fract());
            let low = low.min(mono.len() - 1);
            let next = mono.get(low + 1).copied().unwrap_or(mono[low]);
            (mono[low] * (1.0 - fraction) + next * fraction) as i16
        })
        .collect())
}

/// Drop leading and trailing silence, keeping 200 ms around the speech, so a
/// padded fixture does not hold the activity open for many silent seconds.
pub fn trim(speech: &[i16]) -> &[i16] {
    let loud = |sample: &i16| sample.unsigned_abs() > 328; // about -40 dBFS
    let (Some(first), Some(last)) = (speech.iter().position(loud), speech.iter().rposition(loud))
    else {
        return speech;
    };
    &speech[first.saturating_sub(3_200)..(last + 3_200).min(speech.len())]
}
