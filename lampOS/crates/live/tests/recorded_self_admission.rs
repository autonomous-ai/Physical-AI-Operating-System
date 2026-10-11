//! Offline check: would the speech detector have started a turn on what the
//! canceller left of the lamp's own reply?
//!
//! Proposal from Agent 1 for this crate's owner. It reads recordings that are
//! never in Git, so it is ignored unless asked for, and it activates nothing.
//!
//! ```sh
//! lamp-audio replay-aec /path/to/bundle/runtime artifacts/NEW_DIR
//! LAMP_REPLAY_DIRS=artifacts/NEW_DIR[:artifacts/OTHER] \
//!     cargo test -p lamp-live --test recorded_self_admission -- --ignored --nocapture
//! ```
//!
//! For each replay directory it scores `segment-001-recorded_post_aec.wav`,
//! the cancelled signal the device itself produced, with the runtime's own
//! `SpeechProbability`, and reports every block at which `START_BLOCKS`
//! consecutive scores of 0.80 or more complete while accepted render was
//! non-silent in the last 500 ms. Set `LAMP_EXPECT_NO_SELF_ADMISSION=1` to
//! make any such block a failure.
//!
//! What it does not establish: that nobody spoke. A start during playback is
//! reported whether the lamp or a person caused it.
use lamp_audio::CAPTURE_SAMPLES;
use lamp_live::activity::{START_BLOCKS, SpeechProbability};
use serde_json::Value;
use std::{env, fs, path::Path};

/// The detector's start rule, as `TurnDetector` applies it.
const START_SCORE: f32 = 0.80;

struct Start {
    block: usize,
    render_active_blocks: u64,
    capture_explained: Option<u64>,
    residual_playback_floor: Option<u64>,
    capture_level_db: f64,
    residual_level_db: f64,
}

fn scores(wav: &Path) -> Vec<f32> {
    let mut reader = hound::WavReader::open(wav).expect("replayed cancelled signal");
    let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
    let mut detector = SpeechProbability::default();
    samples
        .chunks_exact(CAPTURE_SAMPLES)
        .map(|block| detector.analyze(block.try_into().unwrap()))
        .collect()
}

/// The device's own evidence for each capture block, in order.
fn evidence(operations: &Path) -> Vec<Value> {
    fs::read_to_string(operations)
        .expect("operations.jsonl")
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| row["kind"] == "capture_pair")
        .map(|row| row["echo_evidence"]["recorded"].clone())
        .collect()
}

fn starts_during_playback(directory: &Path) -> Vec<Start> {
    let scores = scores(&directory.join("segment-001-recorded_post_aec.wav"));
    let evidence = evidence(&directory.join("operations.jsonl"));
    assert_eq!(scores.len(), evidence.len(), "one score per capture block");
    let mut run = 0;
    let mut starts = Vec::new();
    for (block, score) in scores.iter().enumerate() {
        run = if *score >= START_SCORE { run + 1 } else { 0 };
        let observed = &evidence[block];
        let active = observed["render_active_blocks"].as_u64().unwrap_or(0);
        if run == START_BLOCKS && active > 0 {
            starts.push(Start {
                block,
                render_active_blocks: active,
                capture_explained: observed["echo"]["capture_explained"].as_u64(),
                residual_playback_floor: observed["echo"]["residual_playback_floor"].as_u64(),
                capture_level_db: observed["capture_level"].as_f64().unwrap_or(-960.0) / 10.0,
                residual_level_db: observed["residual_level"].as_f64().unwrap_or(-960.0) / 10.0,
            });
        }
    }
    starts
}

#[test]
#[ignore = "reads local recordings named by LAMP_REPLAY_DIRS"]
fn the_detector_does_not_start_a_turn_on_the_lamp_s_own_reply() {
    let directories = env::var("LAMP_REPLAY_DIRS").expect("LAMP_REPLAY_DIRS");
    let mut total = 0;
    for directory in directories.split(':').filter(|name| !name.is_empty()) {
        let starts = starts_during_playback(Path::new(directory));
        println!("{directory}: {} start(s) during playback", starts.len());
        for start in &starts {
            println!(
                "  block {} ({:.2} s): render active {} of 50; microphone {:.1} dB, explained {:?} %; cancelled {:.1} dB, playback floor {:?} %",
                start.block,
                start.block as f64 / 100.0,
                start.render_active_blocks,
                start.capture_level_db,
                start.capture_explained,
                start.residual_level_db,
                start.residual_playback_floor,
            );
        }
        total += starts.len();
    }
    if env::var_os("LAMP_EXPECT_NO_SELF_ADMISSION").is_some() {
        assert_eq!(total, 0, "the detector started during playback");
    }
}
