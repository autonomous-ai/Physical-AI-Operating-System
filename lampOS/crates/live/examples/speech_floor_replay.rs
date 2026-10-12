//! Offline replay: a recorded session's cancelled microphone signal through
//! the production speech model, the score the capture worker reports for it
//! and the production turn detector.
//!
//! Local files only. It opens no device and no connection, and it is not a
//! device result: the model runs on this host over audio a lamp recorded.
//!
//! ```text
//! speech_floor_replay RUNTIME_DIR [SCORES.tsv]
//! ```
//!
//! With a second argument, every block's scores are also written there: the
//! lamp's, this host's, and this host's as reported.
//!
//! `RUNTIME_DIR` is a diagnostics bundle. `capture-audio/post_aec.pcm16le` is
//! what the model was given on the lamp, and `capture-audio/events.jsonl` has
//! one `capture` row per block with the score the lamp recorded for it. The
//! detector stands alone: the coordinator's playback mask and admission are
//! not replayed, so a start here is a detector start, not an admitted request.
//!
//! Prints one JSON object: how far this host's model score is from the
//! lamp's, how many blocks were held to the faint score, and where the
//! detector started and ended on four scores: the lamp's recorded one and
//! this host's, each as the model gave it and as the worker reports it.
use lamp_audio::{CAPTURE_SAMPLES, CaptureBlock, speech_floor::reported_score};
use lamp_live::activity::{Activity, ObservedAudio, SpeechProbability, TurnDetector};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, BufWriter, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const BLOCK_BYTES: usize = CAPTURE_SAMPLES * 2;

#[derive(Default)]
struct Turns {
    detector: TurnDetector,
    starts: Vec<f64>,
    ends: Vec<f64>,
    faults: u32,
}

impl Turns {
    fn block(&mut self, index: usize, frame: ObservedAudio, score: f32) {
        match self.detector.push(frame, score) {
            // The first of the six blocks that started it.
            Activity::Start(_) => self.starts.push(index.saturating_sub(5) as f64 / 100.0),
            Activity::End(_) => self.ends.push(index as f64 / 100.0),
            Activity::Fault(_) => self.faults += 1,
            Activity::Quiet | Activity::Continue(_) => {}
        }
    }

    fn report(&self) -> Value {
        json!({ "starts_s": self.starts, "ends_s": self.ends, "faults": self.faults })
    }
}

fn field(row: &Value, name: &str) -> Result<u64> {
    row["meta"][name]
        .as_u64()
        .ok_or_else(|| io::Error::other(format!("capture row without {name}")).into())
}

fn main() -> Result<()> {
    let runtime = std::env::args()
        .nth(1)
        .ok_or_else(|| io::Error::other("usage: speech_floor_replay RUNTIME_DIR [SCORES.tsv]"))?;
    let mut scores = match std::env::args().nth(2) {
        Some(path) => Some(BufWriter::new(fs::File::create(path)?)),
        None => None,
    };
    if let Some(scores) = scores.as_mut() {
        writeln!(scores, "block\tlamp\thost\thost_reported")?;
    }
    let capture = Path::new(&runtime).join("capture-audio");
    let pcm = fs::read(capture.join("post_aec.pcm16le"))?;
    let events = fs::read_to_string(capture.join("events.jsonl"))?;

    let mut speech = SpeechProbability::default();
    let (mut lamp, mut lamp_reported) = (Turns::default(), Turns::default());
    let (mut host, mut host_reported) = (Turns::default(), Turns::default());
    let (mut blocks, mut held, mut held_on_host) = (0_usize, 0_u32, 0_u32);
    let (mut largest, mut apart, mut across) = (0_f32, 0_u32, 0_u32);
    let mut dsp_epoch = None;
    for line in events.lines() {
        if !line.contains("\"capture\"") {
            continue;
        }
        let row: Value = serde_json::from_str(line)?;
        if row["kind"] != "capture" {
            continue;
        }
        let offset = row["post_aec_byte_offset"]
            .as_u64()
            .ok_or_else(|| io::Error::other("capture row without a byte offset"))?
            as usize;
        let Some(bytes) = pcm.get(offset..offset + BLOCK_BYTES) else {
            break;
        };
        let mut samples: CaptureBlock = [0; CAPTURE_SAMPLES];
        for (sample, pair) in samples.iter_mut().zip(bytes.chunks_exact(2)) {
            *sample = i16::from_le_bytes([pair[0], pair[1]]);
        }
        // The worker resets the model with the canceller, and the coordinator
        // drops an inactive prefix that the old canceller state produced.
        let epoch = field(&row, "dsp_epoch")?;
        if dsp_epoch.is_some_and(|last| last != epoch) {
            speech.reset();
            for turns in [&mut lamp, &mut lamp_reported, &mut host, &mut host_reported] {
                turns.detector.discard_inactive_prefix();
            }
        }
        dsp_epoch = Some(epoch);
        let frame = ObservedAudio {
            sequence: field(&row, "frame_sequence")?,
            captured_at_us: field(&row, "read_completed_at_us")?,
            samples,
        };
        let recorded = row["meta"]["vad_score"]
            .as_f64()
            .ok_or_else(|| io::Error::other("capture row without a score"))?
            as f32;
        let model = speech.analyze(&samples);
        let difference = (model - recorded).abs();
        largest = largest.max(difference);
        apart += u32::from(difference > 0.01);
        across += u32::from((model >= 0.8) != (recorded >= 0.8));
        held += u32::from(reported_score(recorded, &samples) != recorded);
        held_on_host += u32::from(reported_score(model, &samples) != model);
        if let Some(scores) = scores.as_mut() {
            let reported = reported_score(model, &samples);
            writeln!(scores, "{blocks}\t{recorded:.4}\t{model:.4}\t{reported:.4}")?;
        }

        lamp.block(blocks, frame.clone(), recorded);
        lamp_reported.block(blocks, frame.clone(), reported_score(recorded, &samples));
        host.block(blocks, frame.clone(), model);
        host_reported.block(blocks, frame, reported_score(model, &samples));
        blocks += 1;
    }
    if let Some(mut scores) = scores {
        scores.flush()?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "runtime": runtime,
            "blocks": blocks,
            "this_host_model_against_the_lamp_score": {
                "largest_difference": largest,
                "blocks_more_than_0.01_apart": apart,
                "blocks_on_different_sides_of_the_start_score": across,
            },
            "blocks_held_to_the_faint_score": { "lamp_score": held, "host_score": held_on_host },
            "detector_on_the_lamp_score": lamp.report(),
            "detector_on_the_lamp_score_as_reported": lamp_reported.report(),
            "detector_on_the_host_score": host.report(),
            "detector_on_the_host_score_as_reported": host_reported.report(),
        }))?
    );
    Ok(())
}
