//! Model scores at every pause of every clip, at several trailing-pause
//! lengths, for comparing endpoint policies before replaying the chosen one.
//!
//! Segmentation mirrors `lamp_live::activity`: a 30-block pre-roll, onset
//! after six blocks at or above 0.80, and a pause as a run of consecutive
//! blocks below 0.35, which any higher block ends. The model sees the same
//! retained window the detector would submit when the pause reaches each
//! offset. `replay` runs the real detector; its verdicts must match these.
use crate::{
    Result,
    audio::{BLOCK, finish, gain, silence},
    corpus::{self, Clip},
};
use lamp_endpoint::{FeatureExtractor, SMART_TURN_V3_2, SmartTurn, WINDOW_SAMPLES};
use lamp_live::activity::{PRE_ROLL_BLOCKS, START_BLOCKS, SpeechProbability};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

const ONSET: f32 = 0.80;
const SILENCE: f32 = 0.35;
pub const LEAD_MS: usize = 600;
pub const TAIL_MS: usize = 2_500;
/// Trailing pause lengths scored, in 10 ms blocks.
pub const OFFSETS: [u16; 9] = [5, 10, 15, 20, 25, 30, 40, 60, 80];

/// Lead-in, the clip at `gain_db`, a silent tail and the noise floor.
pub fn compose(clip: &Clip, gain_db: f64) -> Result<Vec<i16>> {
    let mut samples = corpus::read(clip)?;
    gain(&mut samples, gain_db);
    let mut pcm = silence(LEAD_MS);
    pcm.extend(samples);
    pcm.extend(silence(TAIL_MS));
    Ok(finish(pcm, &clip.id))
}

struct Pause {
    start: usize,
    length: u16,
    utterance_blocks: usize,
    scores: BTreeMap<u16, f32>,
}

fn score(
    model: &SmartTurn,
    extractor: &mut FeatureExtractor,
    clip: &Clip,
    pcm: &[i16],
) -> Result<Value> {
    let mut vad = SpeechProbability::default();
    let mut prefix: VecDeque<usize> = VecDeque::with_capacity(PRE_ROLL_BLOCKS);
    let mut start_count = 0u16;
    let mut onset = None;
    let mut pause = 0u16;
    let mut pauses: Vec<Pause> = Vec::new();
    let mut last_speech = None;
    let blocks: Vec<&[i16]> = pcm.chunks_exact(BLOCK).collect();
    for (index, block) in blocks.iter().enumerate() {
        let probability = vad.analyze((*block).try_into()?);
        if probability >= SILENCE {
            last_speech = Some(index);
        }
        let Some(start) = onset else {
            if prefix.len() == PRE_ROLL_BLOCKS {
                prefix.pop_front();
            }
            prefix.push_back(index);
            start_count = if probability >= ONSET {
                start_count + 1
            } else {
                0
            };
            if start_count >= START_BLOCKS {
                onset = prefix.front().copied();
            }
            continue;
        };
        if probability >= SILENCE {
            pause = 0;
            continue;
        }
        pause += 1;
        if pause == 1 {
            pauses.push(Pause {
                start: index,
                length: 0,
                utterance_blocks: index - start,
                scores: BTreeMap::new(),
            });
        }
        let current = pauses.last_mut().expect("pause started");
        current.length = pause;
        if OFFSETS.contains(&pause) {
            // The detector's retained window: pre-roll onward, last 8 s,
            // ending with this block.
            let end = (index + 1) * BLOCK;
            let begin = (start * BLOCK).max(end.saturating_sub(WINDOW_SAMPLES));
            let features = extractor.compute(&pcm[begin..end]);
            current.scores.insert(pause, model.probability(features)?);
        }
    }
    let total = blocks.len();
    let lead = LEAD_MS / 10;
    let clip_end = lead + clip.samples.div_ceil(BLOCK);
    Ok(json!({
        "shard": clip.shard, "file": clip.file, "id": clip.id, "partition": clip.partition,
        "label": clip.label, "language": clip.language, "dataset": clip.dataset,
        "midfiller": clip.midfiller, "endfiller": clip.endfiller,
        "blocks": total, "clip_blocks": [lead, clip_end],
        "onset": onset, "last_speech": last_speech,
        "pauses": pauses.iter().map(|p| json!({
            "start": p.start, "length": p.length,
            // The tail pause runs to the end of the composed audio.
            "final": p.start + usize::from(p.length) == total,
            "utterance_blocks": p.utterance_blocks,
            "scores": p.scores.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>(),
        })).collect::<Vec<_>>(),
    }))
}

pub fn run(clips: &[Clip], model: &Path, out: &Path, gain_db: f64, threads: usize) -> Result<()> {
    let model = Arc::new(SmartTurn::load(model, &SMART_TURN_V3_2)?);
    let next = AtomicUsize::new(0);
    let rows: Mutex<Vec<(usize, Value)>> = Mutex::new(Vec::with_capacity(clips.len()));
    let failed: Mutex<Vec<String>> = Mutex::new(Vec::new());
    thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                let mut extractor = FeatureExtractor::default();
                loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(clip) = clips.get(index) else {
                        return;
                    };
                    let row = compose(clip, gain_db)
                        .and_then(|pcm| score(&model, &mut extractor, clip, &pcm));
                    match row {
                        Ok(row) => rows.lock().unwrap().push((index, row)),
                        Err(error) => failed.lock().unwrap().push(format!("{}: {error}", clip.id)),
                    }
                    if index.is_multiple_of(100) {
                        eprintln!("probe {index}/{}", clips.len());
                    }
                }
            });
        }
    });
    let failed = failed.into_inner()?;
    if !failed.is_empty() {
        return Err(failed.join("\n").into());
    }
    let mut rows = rows.into_inner()?;
    rows.sort_by_key(|(index, _)| *index);
    let mut writer = BufWriter::new(File::create(out)?);
    for (_, mut row) in rows {
        row["gain_db"] = json!(gain_db);
        serde_json::to_writer(&mut writer, &row)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    Ok(())
}

/// Per-block VAD scores of each composed clip (no model), so offline policy
/// simulation can tell a real resumption (a new onset) from a noise blip.
pub fn vad(clips: &[Clip], out: &Path, gain_db: f64) -> Result<()> {
    let mut writer = BufWriter::new(File::create(out)?);
    for clip in clips {
        let pcm = compose(clip, gain_db)?;
        let mut vad = SpeechProbability::default();
        let scores: Vec<u16> = pcm
            .chunks_exact(BLOCK)
            .map(|block| Ok((vad.analyze(block.try_into()?) * 1_000.0).round() as u16))
            .collect::<Result<_>>()?;
        serde_json::to_writer(
            &mut writer,
            &json!({"id": clip.id, "file": clip.file, "shard": clip.shard, "vad_milli": scores}),
        )?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    Ok(())
}
