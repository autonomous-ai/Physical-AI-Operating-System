//! Labelled speech for comparing the exact and fast models
//! (`lamp-turn-end agree`), and the comparison itself.
//!
//! A clip directory holds 16 kHz mono 16-bit WAV files and a `manifest.json`:
//! `{"clips": [{"file", "id", "label", "dataset", "language", "samples",
//! "pcm_sha256"}]}`, where `label` is true when the clip ends at a finished
//! turn. This is the layout docs/endpoint-detection.md used for the
//! `pipecat-ai/smart-turn-data-v3.2-test` shards; no audio is committed.
use crate::{Result, SAMPLE_RATE};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// The voice's bands (`ConfirmPolicy` in `crates/voice/src/activity.rs`): at
/// or above this a verdict counts as finished.
pub const FINISHED: f32 = 0.9;
/// Below this a verdict counts as clearly unfinished.
pub const UNFINISHED: f32 = 0.01;

#[derive(Clone, Debug)]
pub struct Clip {
    pub id: String,
    pub dataset: String,
    pub language: String,
    /// The clip ends at a finished turn.
    pub finished: bool,
    pub samples: Option<usize>,
    pub pcm_sha256: Option<String>,
    pub path: PathBuf,
}

/// Every clip a directory's manifest lists, in its order.
pub fn load(directory: &Path) -> Result<Vec<Clip>> {
    let manifest: Value = serde_json::from_slice(&std::fs::read(directory.join("manifest.json"))?)?;
    let clips = manifest["clips"]
        .as_array()
        .ok_or("manifest.json has no clips")?;
    clips
        .iter()
        .map(|clip| {
            let text = |key: &str| clip[key].as_str().map(str::to_owned);
            Ok(Clip {
                id: text("id").ok_or("a clip has no id")?,
                dataset: text("dataset").unwrap_or_default(),
                language: text("language").unwrap_or_default(),
                finished: clip["label"].as_bool().ok_or("a clip has no label")?,
                samples: clip["samples"].as_u64().map(|n| n as usize),
                pcm_sha256: text("pcm_sha256"),
                path: directory.join(text("file").ok_or("a clip has no file")?),
            })
        })
        .collect()
}

/// A clip's samples, checked against its manifest entry.
pub fn read(clip: &Clip) -> Result<Vec<i16>> {
    let mut reader = hound::WavReader::open(&clip.path)?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate as usize != SAMPLE_RATE
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err(format!("{} is not 16 kHz mono 16-bit", clip.path.display()).into());
    }
    let pcm = reader
        .samples::<i16>()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if clip.samples.is_some_and(|samples| samples != pcm.len()) {
        return Err(format!(
            "{} has another length than its manifest",
            clip.path.display()
        )
        .into());
    }
    if let Some(expected) = &clip.pcm_sha256 {
        let bytes = pcm.iter().flat_map(|s| s.to_le_bytes()).collect::<Vec<_>>();
        let digest = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if &digest != expected {
            return Err(format!("{} does not match its manifest", clip.path.display()).into());
        }
    }
    Ok(pcm)
}

/// The clip as the voice sends it: followed by `pause_ms` of a −63 dBFS
/// noise floor (the detector asks 200 ms into a pause), from a fixed seed.
pub fn with_pause(pcm: &[i16], pause_ms: usize) -> Vec<i16> {
    let mut state = 0x9e37_79b9u32;
    let mut audio = pcm.to_vec();
    audio.extend((0..pause_ms * SAMPLE_RATE / 1_000).map(|_| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        // Uniform in [-23, 23]: about 13 / 32,768 RMS, -63 dBFS.
        ((state % 47) as i32 - 23) as i16
    }));
    audio
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Band {
    Unfinished,
    Between,
    Finished,
}

pub fn band(probability: f32) -> Band {
    if probability >= FINISHED {
        Band::Finished
    } else if probability < UNFINISHED {
        Band::Unfinished
    } else {
        Band::Between
    }
}

/// How far the fast model's verdicts agree with the exact model's.
#[derive(Clone, Debug, Default)]
pub struct Agreement {
    differences: Vec<f32>,
    /// Clips whose band changed, by (exact band, fast band).
    pub band_changes: BTreeMap<(Band, Band), usize>,
    /// Clips on opposite sides of 0.5.
    pub flips: usize,
    pub exact_right: usize,
    pub fast_right: usize,
}

impl Agreement {
    pub fn add(&mut self, exact: f32, fast: f32, finished: bool) {
        self.differences.push((exact - fast).abs());
        let (from, to) = (band(exact), band(fast));
        if from != to {
            *self.band_changes.entry((from, to)).or_default() += 1;
        }
        self.flips += usize::from((exact >= 0.5) != (fast >= 0.5));
        self.exact_right += usize::from((exact >= 0.5) == finished);
        self.fast_right += usize::from((fast >= 0.5) == finished);
    }

    pub fn clips(&self) -> usize {
        self.differences.len()
    }

    /// The `q`-quantile (0 to 1) of the absolute differences.
    pub fn difference(&self, q: f64) -> f32 {
        let mut sorted = self.differences.clone();
        sorted.sort_by(f32::total_cmp);
        match sorted.len() {
            0 => 0.0,
            n => sorted[((n - 1) as f64 * q).round() as usize],
        }
    }

    pub fn summary(&self) -> Value {
        let changes = self
            .band_changes
            .iter()
            .map(|((from, to), count)| json!({"exact": format!("{from:?}"), "fast": format!("{to:?}"), "clips": count}))
            .collect::<Vec<_>>();
        let rate = |right: usize| match self.clips() {
            0 => 0.0,
            n => right as f64 / n as f64,
        };
        json!({
            "clips": self.clips(),
            "difference_median": self.difference(0.5),
            "difference_p99": self.difference(0.99),
            "difference_max": self.difference(1.0),
            "band_changes": self.band_changes.values().sum::<usize>(),
            "band_change_detail": changes,
            "flips_at_half": self.flips,
            "exact_correct_at_half": rate(self.exact_right),
            "fast_correct_at_half": rate(self.fast_right),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_follow_the_voice_thresholds() {
        assert_eq!(band(0.9), Band::Finished);
        assert_eq!(band(0.899), Band::Between);
        assert_eq!(band(0.01), Band::Between);
        assert_eq!(band(0.0099), Band::Unfinished);
    }

    #[test]
    fn agreement_counts_changes_flips_and_correct_verdicts() {
        let mut agreement = Agreement::default();
        agreement.add(0.95, 0.95, true); // same
        agreement.add(0.95, 0.85, true); // finished -> between, still above half
        agreement.add(0.008, 0.012, false); // unfinished -> between
        agreement.add(0.6, 0.4, true); // flip, the fast one wrong
        assert_eq!(agreement.clips(), 4);
        assert_eq!(agreement.flips, 1);
        assert_eq!((agreement.exact_right, agreement.fast_right), (4, 3));
        assert_eq!(
            agreement.band_changes,
            BTreeMap::from([
                ((Band::Unfinished, Band::Between), 1),
                ((Band::Finished, Band::Between), 1)
            ])
        );
        assert!((agreement.difference(1.0) - 0.2).abs() < 1e-6);
        assert_eq!(agreement.difference(0.0), 0.0);
        let summary = agreement.summary();
        assert_eq!(summary["band_changes"], 2);
        assert_eq!(summary["fast_correct_at_half"], 0.75);
    }

    #[test]
    fn manifests_and_clips_round_trip_with_their_checks() {
        let dir = std::env::temp_dir().join(format!("lamp-clips-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pcm: Vec<i16> = (0..1_600)
            .map(|i| ((i * 37) % 2_000) as i16 - 1_000)
            .collect();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(dir.join("a.wav"), spec).unwrap();
        for &sample in &pcm {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        let bytes = pcm.iter().flat_map(|s| s.to_le_bytes()).collect::<Vec<_>>();
        let digest = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let manifest = json!({"clips": [
            {"file": "a.wav", "id": "a", "label": true, "dataset": "d", "language": "eng", "samples": 1_600, "pcm_sha256": digest},
            {"file": "a.wav", "id": "b", "label": false, "samples": 1_601},
        ]});
        std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        let clips = load(&dir).unwrap();
        assert_eq!(clips.len(), 2);
        assert!(clips[0].finished && !clips[1].finished);
        assert_eq!(read(&clips[0]).unwrap(), pcm);
        assert!(read(&clips[1]).unwrap_err().to_string().contains("length"));
        let padded = with_pause(&pcm, 200);
        assert_eq!(padded.len(), pcm.len() + 3_200);
        assert!(padded[pcm.len()..].iter().all(|s| s.abs() <= 23));
        assert_eq!(padded, with_pause(&pcm, 200), "the floor is repeatable");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
