//! Automatic acoustic boundary proposals (`MAIN-021`). Each certified render
//! segment of a reply (and, when supplied, the exact stimulus) is located in
//! the verified room recording by `align`. The output is a separate proposals
//! file: never an annotation, never scored, never `listened`. A person must
//! listen and copy confirmed times into an annotation file.
//!
//! A proposed reply onset is the earliest rendered reply audio as heard at
//! the room microphone. The first substantive word may be later, and the
//! opening may be inaudible; a later, clearer word is never substituted.
use crate::{
    Result,
    align::{self, Search},
    import::{MAX_ROOM_WAV_BYTES, verify_room_evidence},
    invalid,
    record::{AttemptRecord, Attribution},
    target_audio::{self, RENDER_RATE, RenderSpan},
};
use lamp_acoustic::hash;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, io::Read, path::Path};

pub const SCHEMA: u32 = 1;
const ROOM_RATE: u32 = 16_000;
/// Render samples quieter than this (or 2% of the segment peak) are silence.
const ACTIVE_FLOOR: f32 = 64.0;
const ACTIVE_FRACTION: f32 = 0.02;

/// Exact stimulus audio supplied by the operator, already read.
pub struct Stimulus {
    pub sha256: String,
    pub pcm: Vec<f32>,
}

/// Longest stimulus file read into memory.
const MAX_STIMULUS_BYTES: u64 = 64 * 1024 * 1024;

/// Read a regular file within `limit`; the caller hashes these exact bytes.
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("audio input must be a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("audio input exceeds its size bound"));
    }
    Ok(bytes)
}

/// Decode WAV bytes already held (and hashed) by the caller into mono f32 at
/// 16 kHz, so the parsed audio is exactly the hashed bytes. Rate, channels,
/// sample format and duration are validated before any resampling.
fn decode_wav_16k(bytes: &[u8]) -> Result<(Vec<f32>, Vec<i16>)> {
    let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes))?;
    let spec = reader.spec();
    if !(align::MIN_RATE..=align::MAX_RATE).contains(&spec.sample_rate)
        || !(1..=8).contains(&spec.channels)
    {
        return Err(invalid(
            "WAV sample rate must be 8 to 192 kHz with 1 to 8 channels",
        ));
    }
    let frames = u64::from(reader.duration());
    if frames == 0 || frames > u64::from(spec.sample_rate) * align::MAX_SECONDS {
        return Err(invalid("WAV is empty or longer than 600 s"));
    }
    let channels = usize::from(spec.channels);
    let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|s| s.map(f32::from))
            .collect::<std::result::Result<_, _>>()?,
        (hound::SampleFormat::Int, bits @ (24 | 32)) => {
            let scale = 2_f32.powi(i32::from(bits) - 16);
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<std::result::Result<_, _>>()?
        }
        (hound::SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .map(|s| s.map(|v| v * 32767.0))
            .collect::<std::result::Result<_, _>>()?,
        _ => return Err(invalid("WAV must be PCM16/24/32 or float32")),
    };
    let mono: Vec<f32> = samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();
    let raw: Vec<i16> = if spec.sample_rate == ROOM_RATE {
        mono.iter()
            .map(|v| v.round().clamp(-32768.0, 32767.0) as i16)
            .collect()
    } else {
        Vec::new()
    };
    let mono = align::resample(&mono, spec.sample_rate, ROOM_RATE)?;
    Ok((mono, raw))
}

/// Read an operator-supplied exact stimulus WAV, hashing and decoding the
/// same bytes. Malformed input is refused with a reason.
pub fn read_stimulus(path: &Path) -> Result<Stimulus> {
    let bytes = read_bounded(path, MAX_STIMULUS_BYTES)?;
    let (pcm, _) = decode_wav_16k(&bytes)?;
    Ok(Stimulus {
        sha256: hash(&bytes),
        pcm,
    })
}

/// Reply segments: for each owning turn (or contiguous unowned reply run),
/// the render frames from its first to its last reply span, including any
/// inserted gaps between them.
fn reply_segments(spans: &[RenderSpan]) -> Vec<(Option<u64>, u64, u64)> {
    let mut segments: Vec<(Option<u64>, u64, u64)> = Vec::new();
    for span in spans.iter().filter(|s| s.silence.is_none() && s.frames > 0) {
        let end = span.offset_frames.saturating_add(span.frames);
        match segments.last_mut() {
            Some((turn, _, last))
                if *turn == span.turn && (span.turn.is_some() || *last == span.offset_frames) =>
            {
                *last = end
            }
            _ => segments.push((span.turn, span.offset_frames, end)),
        }
    }
    segments
}

fn active(pcm: &[f32]) -> Option<(usize, usize)> {
    let peak = pcm.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    align::active_span(pcm, ACTIVE_FLOOR.max(ACTIVE_FRACTION * peak))
}

fn seconds(sample: i64) -> f64 {
    sample as f64 / f64::from(ROOM_RATE)
}

fn search_json(search: &Search, onset: usize, end: usize) -> Value {
    let best = search.best();
    json!({
        "status": search.status,
        "first_audible_reply_audio_s": best.map(|b| seconds(b.offset + onset as i64)),
        "reply_audio_end_s": best.map(|b| seconds(b.offset + end as i64)),
        "uncertainty_ms": search.uncertainty_samples * 1000.0 / f64::from(ROOM_RATE),
        "score": best.map(|b| b.score),
        "margin": search.margin,
        "inverted": best.map(|b| b.inverted),
        "clipped_fraction": search.clipped_fraction,
        "alternatives": search.candidates.iter().skip(1).map(|c| json!({
            "first_audible_reply_audio_s": seconds(c.offset + onset as i64),
            "score": c.score, "inverted": c.inverted})).collect::<Vec<_>>(),
    })
}

/// The verified render stream of an imported trace, re-checked against the
/// trace file the record names.
fn render_for(record: &AttemptRecord) -> Result<(Vec<f32>, Vec<RenderSpan>)> {
    let path = record.evidence["trace_path"]
        .as_str()
        .ok_or_else(|| invalid("no target trace is retained for this attempt"))?;
    let text = fs::read_to_string(path)?;
    if record.evidence["trace_sha256"].as_str() != Some(hash(text.as_bytes()).as_str()) {
        return Err(invalid("target trace changed since import"));
    }
    let records: Vec<Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let audio = target_audio::verify(
        Path::new(path).parent().unwrap_or_else(|| Path::new(".")),
        &records,
    );
    let (stream, spans) = audio.render.map_err(|e| invalid(&e))?;
    let pcm = target_audio::read_pcm(&stream, "render_accepted.pcm16le")?
        .into_iter()
        .map(f32::from)
        .collect();
    Ok((pcm, spans))
}

fn steps_for(record: &AttemptRecord, turn: Option<u64>) -> Vec<String> {
    match (&record.attribution, turn) {
        (Attribution::Declared { turns, .. }, Some(turn)) => turns
            .iter()
            .filter(|(_, t)| *t == turn)
            .map(|(step, _)| step.clone())
            .collect(),
        _ => Vec::new(),
    }
}

fn attempt(record: &AttemptRecord, stimulus: Option<&Stimulus>) -> Value {
    let room_evidence = &record.evidence["room_audio"];
    let mut entry = json!({
        "room_recording_sha256": room_evidence["sha256"],
        "listened": false,
    });
    let room = (|| -> Result<(Vec<f32>, Vec<i16>)> {
        let verified = verify_room_evidence(room_evidence)?;
        let path = room_evidence["wav_path"]
            .as_str()
            .ok_or_else(|| invalid("room recording has no WAV path"))?;
        // Decode the bytes that match the verified hash, not a fresh open.
        let bytes = read_bounded(Path::new(path), MAX_ROOM_WAV_BYTES)?;
        if hash(&bytes) != verified.sha256 {
            return Err(invalid("room WAV changed after verification"));
        }
        decode_wav_16k(&bytes)
    })();
    let (room, raw) = match room {
        Ok(room) => {
            entry["room"] = json!({"verified": true});
            room
        }
        Err(error) => {
            entry["room"] = json!({"verified": false, "error": error.to_string()});
            entry["replies"] = json!([]);
            entry["stimulus"] =
                json!({"status": "unavailable", "reason": "no verified room recording"});
            return entry;
        }
    };
    let clipped = (!raw.is_empty()).then_some(raw.as_slice());
    entry["replies"] = match render_for(record) {
        Err(error) => {
            entry["render"] = json!({"verified": false, "error": error.to_string()});
            json!([])
        }
        Ok((render, spans)) => {
            entry["render"] = json!({"verified": true, "rate": RENDER_RATE});
            let replies: Vec<Value> = reply_segments(&spans)
                .into_iter()
                .map(|(turn, start, end)| {
                    let segment = &render[start as usize..end as usize];
                    let mut reply = json!({"turn": turn, "render_frames": [start, end],
                        "steps": steps_for(record, turn),
                        "caution": "earliest rendered reply audio at the room microphone; the first substantive word may be later and the opening may be inaudible; confirm by listening"});
                    let reference = match align::resample(segment, RENDER_RATE, ROOM_RATE) {
                        Ok(reference) => reference,
                        Err(error) => {
                            reply["status"] = json!("unavailable");
                            reply["reason"] = json!(error.to_string());
                            return reply;
                        }
                    };
                    match active(&reference).zip(align::locate(&room, &reference, clipped)) {
                        Some(((onset, last), search)) => {
                            if let (Some(fields), Some(found)) =
                                (reply.as_object_mut(), search_json(&search, onset, last).as_object())
                            {
                                fields.extend(found.clone());
                            }
                        }
                        None => {
                            reply["status"] = json!("unavailable");
                            reply["reason"] = json!("segment is silent, shorter than 200 ms or longer than the room recording");
                        }
                    }
                    reply
                })
                .collect();
            json!(replies)
        }
    };
    entry["stimulus"] = match stimulus {
        None => json!({"status": "unavailable",
            "reason": "exact source stimulus PCM was not supplied; it is never reconstructed from transcript text"}),
        Some(stimulus) => {
            match active(&stimulus.pcm).zip(align::locate(&room, &stimulus.pcm, clipped)) {
                None => json!({"status": "unavailable", "stimulus_sha256": stimulus.sha256,
                "reason": "stimulus is silent, too short or longer than the room recording"}),
                Some(((onset, last), search)) => {
                    let best = search.best();
                    json!({"status": search.status, "stimulus_sha256": stimulus.sha256,
                    "user_speech_start_s": best.map(|b| seconds(b.offset + onset as i64)),
                    "user_speech_end_s": best.map(|b| seconds(b.offset + last as i64)),
                    "uncertainty_ms": search.uncertainty_samples * 1000.0 / f64::from(ROOM_RATE),
                    "score": best.map(|b| b.score), "margin": search.margin,
                    "alternatives": search.candidates.iter().skip(1).map(|c| json!({
                        "user_speech_end_s": seconds(c.offset + last as i64), "score": c.score})).collect::<Vec<_>>(),
                    "caution": "speech end of the supplied stimulus file as heard at the room microphone; confirm by listening"})
                }
            }
        }
    };
    entry
}

/// Proposals for every attempt that has room evidence.
pub fn propose(run_id: &str, records: &[AttemptRecord], stimulus: Option<&Stimulus>) -> Value {
    let attempts: BTreeMap<String, Value> = records
        .iter()
        .filter(|r| r.stratum.is_physical() && !r.evidence["room_audio"].is_null())
        .map(|r| (r.attempt_id.clone(), attempt(r, stimulus)))
        .collect();
    json!({
        "schema": SCHEMA,
        "kind": "boundary_proposals",
        "run_id": run_id,
        "notice": "Automatic proposals, not annotations: never scored and never marked listened. Confirm by listening, then copy times into an annotation file.",
        "method": {
            "alignment": "envelope correlation over the whole room recording, then waveform correlation within 20 ms; per reply segment, so independent sample clocks only shift each segment",
            "min_score": align::MIN_SCORE, "min_margin": align::MIN_MARGIN,
            "thresholds": "unqualified heuristics",
            "room_rate": ROOM_RATE, "render_rate": RENDER_RATE,
        },
        "attempts": attempts,
    })
}
