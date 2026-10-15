//! Offline: a `lamp-audio replay-aec` output heard as the voice hears it
//! (#828).
//!
//! Local files only. It opens no device and no connection, and it is not a
//! device result: the replayed canceller ran on the host.
//!
//! ```text
//! hearing_replay REPLAY_DIR
//! ```
//!
//! For each replayed capture block (`capture_pair` in `operations.jsonl`),
//! the AEC-only output (`segment-001-aec_only.wav`) is scored by the voice's
//! own detector, one stream as the coordinator runs it, and the block is
//! judged by `Hearing::EVALUATED` against the replayed canceller's own echo
//! evidence, as the duplex rules judge it. It writes
//! `REPLAY_DIR/segment-001-hearing.tsv`, one line per block with the time
//! the lamp read it, so that a played word's own time finds its blocks.
//! Nothing is decided here: yields and stops are counted from the table. A
//! replay of more than one segment is refused.
use lamp_audio::{
    CAPTURE_SAMPLES, CaptureBlock,
    duplex::{Heard, Hearing},
    evidence::EchoEvidence,
};
use lamp_voice::activity::SpeechProbability;
use serde_json::Value;
use std::{
    fs,
    io::{self, BufRead, BufWriter, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let (Some(directory), None) = (args.next(), args.next()) else {
        return Err(io::Error::other("usage: hearing_replay REPLAY_DIR").into());
    };
    let directory = Path::new(&directory);
    let report: Value = serde_json::from_slice(&fs::read(directory.join("report.json"))?)?;
    let segments = report["segments"].as_array().map_or(0, Vec::len);
    if segments != 1 {
        return Err(io::Error::other(format!("{segments} segments; only one is read")).into());
    }
    let samples: Vec<i16> = hound::WavReader::open(directory.join("segment-001-aec_only.wav"))?
        .into_samples::<i16>()
        .collect::<std::result::Result<_, _>>()?;
    let mut table = BufWriter::new(fs::File::create(directory.join("segment-001-hearing.tsv"))?);
    writeln!(
        table,
        "block\tread_at_us\tvad\trender_quiet\tmatched\tcapture_explained\tresidual_floor\talignment_age\tcapture_tenths\tresidual_tenths\theard\tplain"
    )?;
    let hearing = Hearing::EVALUATED;
    let mut detector = SpeechProbability::default();
    let mut blocks = 0usize;
    let operations = io::BufReader::new(fs::File::open(directory.join("operations.jsonl"))?);
    for line in operations.lines() {
        let record: Value = serde_json::from_str(&line?)?;
        if record["kind"] != "capture_pair" {
            continue;
        }
        let offset = record["segment_capture_sample_offset"]
            .as_u64()
            .ok_or_else(|| io::Error::other("capture_pair without a sample offset"))?
            as usize;
        if offset != blocks * CAPTURE_SAMPLES {
            return Err(io::Error::other(format!("block {blocks} at sample {offset}")).into());
        }
        let block: CaptureBlock = samples
            .get(offset..offset + CAPTURE_SAMPLES)
            .ok_or_else(|| io::Error::other("the output is shorter than its records"))?
            .try_into()?;
        let probability = detector.analyze(&block);
        let read_at = record["meta"]["first_read_started_at_us"]
            .as_u64()
            .ok_or_else(|| io::Error::other("capture_pair without its read time"))?;
        let evidence: Option<EchoEvidence> =
            serde_json::from_value(record["echo_evidence"]["aec_only"].clone())?;
        let (heard, plain) = evidence.map_or((Heard::Unknown, false), |evidence| {
            let heard = hearing.heard(probability, &evidence);
            (heard, hearing.plain(heard, &evidence))
        });
        let matched = evidence.and_then(|evidence| evidence.echo);
        writeln!(
            table,
            "{blocks}\t{read_at}\t{probability:.4}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{heard:?}\t{}",
            evidence.map_or(-1, |evidence| i32::from(evidence.render_quiet_blocks)),
            u8::from(matched.is_some()),
            matched.map_or(0, |m| m.capture_explained),
            matched.map_or(0, |m| m.residual_playback_floor),
            matched.map_or(0, |m| m.alignment_age_blocks),
            evidence.map_or(-960, |evidence| evidence.capture_level.tenths_of_db()),
            evidence.map_or(-960, |evidence| evidence.residual_level.tenths_of_db()),
            u8::from(plain),
        )?;
        blocks += 1;
    }
    table.flush()?;
    eprintln!("hearing_replay: {blocks} blocks");
    Ok(())
}
