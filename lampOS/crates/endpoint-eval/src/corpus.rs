//! Labelled clips extracted from the pinned upstream test set
//! (`pipecat-ai/smart-turn-data-v3.2-test`, CC-BY-4.0). The audio stays on the
//! development host; only manifests, hashes and results are reported.
use crate::{Result, audio::read_wav};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Deserialize)]
pub struct Clip {
    pub file: String,
    pub partition: String,
    pub id: String,
    /// The clip ends at a finished turn.
    pub label: bool,
    pub language: String,
    pub synthetic: bool,
    pub dataset: String,
    pub midfiller: Option<bool>,
    pub endfiller: Option<bool>,
    pub samples: usize,
    pub pcm_sha256: String,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    pub shard: String,
}

#[derive(Deserialize)]
struct Manifest {
    source: String,
    clips: Vec<Clip>,
}

/// Every clip of each extracted shard directory, in manifest order.
pub fn load(directories: &[PathBuf], synthetic: bool) -> Result<Vec<Clip>> {
    let mut clips = Vec::new();
    for directory in directories {
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
        for mut clip in manifest.clips {
            if clip.synthetic != synthetic {
                continue;
            }
            clip.path = directory.join(&clip.file);
            clip.shard = manifest.source.clone();
            clips.push(clip);
        }
    }
    Ok(clips)
}

/// Reads a clip and verifies its recorded PCM hash.
pub fn read(clip: &Clip) -> Result<Vec<i16>> {
    let pcm = read_wav(&clip.path)?;
    let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    let digest = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if digest != clip.pcm_sha256 || pcm.len() != clip.samples {
        return Err(format!("{} does not match its manifest", clip.path.display()).into());
    }
    Ok(pcm)
}
