//! The level floor under the speech score, through the turn detector as it is.
use lamp_audio::{
    CAPTURE_SAMPLES, CaptureBlock,
    speech_floor::{FAINT_SCORE, reported_score},
};
use lamp_live::activity::{
    Activity, END_SILENCE_BLOCKS, ObservedAudio, PRE_ROLL_BLOCKS, TurnDetector,
};

/// Three steps of the scale: -81 dBFS, where the recorded start was.
const FAINT: CaptureBlock = [3; CAPTURE_SAMPLES];
/// -30 dBFS, where a person at the lamp is.
const SPOKEN: CaptureBlock = [1_000; CAPTURE_SAMPLES];
const SILENT: CaptureBlock = [0; CAPTURE_SAMPLES];

struct Heard {
    detector: TurnDetector,
    sequence: u64,
}

impl Heard {
    fn new() -> Self {
        Self {
            detector: TurnDetector::default(),
            sequence: 0,
        }
    }

    fn block(&mut self, samples: CaptureBlock, score: f32) -> Activity {
        self.sequence += 1;
        let frame = ObservedAudio {
            sequence: self.sequence,
            captured_at_us: self.sequence * 10_000,
            samples,
        };
        self.detector.push(frame, score)
    }
}

#[test]
fn faint_blocks_the_model_calls_speech_start_no_request() {
    // The recorded failure: the detector as it is, given the model's score.
    let mut raw = Heard::new();
    let started = (0..100).any(|_| matches!(raw.block(FAINT, 0.97), Activity::Start(_)));
    assert!(started);

    let mut heard = Heard::new();
    for _ in 0..100 {
        let score = reported_score(0.97, &FAINT);
        assert_eq!(score, FAINT_SCORE);
        assert!(matches!(heard.block(FAINT, score), Activity::Quiet));
    }
}

#[test]
fn speech_at_a_persons_level_starts_a_request_at_the_same_block() {
    let mut heard = Heard::new();
    for block in 1..=6 {
        let activity = heard.block(SPOKEN, reported_score(0.9, &SPOKEN));
        assert_eq!(matches!(activity, Activity::Start(_)), block == 6);
    }
}

#[test]
fn a_quiet_onset_stays_in_the_request_the_louder_blocks_start() {
    let mut heard = Heard::new();
    for _ in 0..20 {
        heard.block(SILENT, reported_score(0.0, &SILENT));
    }
    for _ in 0..10 {
        let activity = heard.block(FAINT, reported_score(0.9, &FAINT));
        assert!(matches!(activity, Activity::Quiet));
    }
    let mut start = None;
    for _ in 0..6 {
        if let Activity::Start(frames) = heard.block(SPOKEN, reported_score(0.9, &SPOKEN)) {
            start = Some(frames);
        }
    }
    let frames = start.expect("the sixth spoken block starts the request");
    assert_eq!(frames.len(), PRE_ROLL_BLOCKS);
    assert_eq!(frames.last().map(|frame| frame.sequence), Some(36));
    // All ten faint blocks of the onset, unchanged, ahead of the spoken ones.
    let onset: Vec<_> = frames
        .iter()
        .filter(|frame| frame.samples == FAINT)
        .map(|frame| frame.sequence)
        .collect();
    assert_eq!(onset, (21..=30).collect::<Vec<u64>>());
    let spoken = frames.iter().filter(|frame| frame.samples == SPOKEN);
    assert_eq!(spoken.count(), 6);
}

#[test]
fn a_request_ends_where_it_did() {
    // A faint tail the model still calls speech keeps the request open
    // exactly as long, and one it calls silence ends it at the same block.
    for (tail, ends) in [(0.97, false), (0.6, false), (0.35, false), (0.34, true)] {
        let mut raw = Heard::new();
        let mut heard = Heard::new();
        for _ in 0..6 {
            raw.block(SPOKEN, 0.9);
            heard.block(SPOKEN, reported_score(0.9, &SPOKEN));
        }
        let mut ended = (None, None);
        for block in 0..u32::from(END_SILENCE_BLOCKS) * 2 {
            if matches!(raw.block(FAINT, tail), Activity::End(_)) {
                ended.0.get_or_insert(block);
            }
            let score = reported_score(tail, &FAINT);
            if matches!(heard.block(FAINT, score), Activity::End(_)) {
                ended.1.get_or_insert(block);
            }
        }
        assert_eq!(ended.0, ended.1, "{tail}");
        assert_eq!(ended.0.is_some(), ends, "{tail}");
    }
}
