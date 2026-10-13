//! Offline verification of lamp-voice's certified audio diagnostics (MAIN-021).
//!
//! The runtime trace names each diagnostic stream's start identity and finish
//! receipt. lamp-voice's own certifier checks the completion marker against
//! both; this module then re-hashes every file and checks every length against
//! that marker. A stream that fails any check is unverified, with its reason;
//! nothing is adapted from other metadata. Render times are ALSA acceptance on
//! the Lamp host clock, never acoustic times.
use crate::{Result, invalid};
use lamp_voice::diagnostic_control::{
    self, DiagnosticFinished, DiagnosticStarted, DiagnosticStream,
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub const RENDER_RATE: u32 = lamp_audio::RENDER_RATE;
pub const CAPTURE_RATE: u32 = lamp_audio::CAPTURE_RATE;
const MAX_MARKER_BYTES: u64 = 64 * 1024;
const MAX_STREAM_BYTES: u64 = lamp_voice::diagnostics::MAX_BYTES;
const MAX_RECORD_LINE: usize = 8 * 1024;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct StreamFile {
    pub name: &'static str,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct VerifiedStream {
    pub stream: DiagnosticStream,
    pub directory: PathBuf,
    /// SHA-256 of `complete.json`, as certified by the runtime's receipt.
    pub completion_sha256: String,
    /// Mono PCM16LE frames in the stream's main file.
    pub frames: u64,
    pub rate: u32,
    pub files: Vec<StreamFile>,
}

impl VerifiedStream {
    fn file(&self, name: &str) -> Result<&StreamFile> {
        self.files
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| invalid("stream file was not verified"))
    }
}

/// One accepted render record: where its frames sit in
/// `render_accepted.pcm16le` and when ALSA accepted them on the Lamp host.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct RenderSpan {
    pub offset_frames: u64,
    pub frames: u64,
    pub playback_epoch: u64,
    pub accepted_at_us: u64,
    pub queued_frames: i64,
    /// `prime`, `idle` or `speech_gap` for inserted zeros; `None` for reply PCM.
    pub silence: Option<String>,
    pub turn: Option<u64>,
}

#[derive(Debug)]
pub struct TargetAudio {
    pub render: std::result::Result<(VerifiedStream, Vec<RenderSpan>), String>,
    pub capture: std::result::Result<VerifiedStream, String>,
}

impl TargetAudio {
    /// Bounded evidence for a ledger: identities and hashes, never PCM.
    pub fn evidence(&self) -> Value {
        let stream = |result: std::result::Result<&VerifiedStream, &String>| match result {
            Ok(stream) => json!({"verified": true, "stream": stream}),
            Err(error) => json!({"verified": false, "error": error}),
        };
        json!({
            "render": stream(self.render.as_ref().map(|(s, _)| s)),
            "render_spans": self.render.as_ref().map(|(_, spans)| spans.len()).ok(),
            "capture": stream(self.capture.as_ref()),
            "scope": "lamp-voice diagnostics; host ALSA times, not acoustic times",
        })
    }
}

/// Verify the render (`render-audio`) and capture (`capture-audio`) streams of
/// one runtime directory against its trace.
pub fn verify(runtime: &Path, trace: &[Value]) -> TargetAudio {
    let render = verify_stream(runtime, trace, "speaker", DiagnosticStream::Render)
        .and_then(|(stream, records)| {
            let spans = render_spans(&records, stream.frames)?;
            Ok((stream, spans))
        })
        .map_err(|e| e.to_string());
    let capture = verify_stream(runtime, trace, "capture", DiagnosticStream::Capture)
        .map(|(stream, _)| stream)
        .map_err(|e| e.to_string());
    TargetAudio { render, capture }
}

fn one<'a>(trace: &'a [Value], kind: &str, worker: &str) -> Result<&'a Value> {
    let mut found = trace
        .iter()
        .filter(|r| r["kind"] == kind && r["worker"] == worker);
    let first = found
        .next()
        .ok_or_else(|| invalid(&format!("trace has no {kind} for the {worker} worker")))?;
    if found.next().is_some() {
        return Err(invalid(&format!(
            "trace has more than one {kind} for the {worker} worker"
        )));
    }
    Ok(first)
}

/// Returns the verified stream and its diagnostic record bytes.
fn verify_stream(
    runtime: &Path,
    trace: &[Value],
    worker: &str,
    stream: DiagnosticStream,
) -> Result<(VerifiedStream, Vec<u8>)> {
    let started: DiagnosticStarted = serde_json::from_value(
        one(trace, "audio_diagnostics_started", worker)?["details"].clone(),
    )?;
    if started.stream != stream {
        return Err(invalid(
            "diagnostic stream identity does not match its worker",
        ));
    }
    let certified = one(trace, "audio_diagnostics_certified", worker)?;
    if certified["valid"] != true {
        return Err(invalid(&format!(
            "the runtime did not certify the {worker} diagnostics: {}",
            certified["error"].as_str().unwrap_or("no detail")
        )));
    }
    let receipt: DiagnosticFinished = serde_json::from_value(certified["receipt"].clone())?;
    let (leaf, frames_key, rate) = match stream {
        DiagnosticStream::Render => ("render-audio", "render_frames", RENDER_RATE),
        DiagnosticStream::Capture => ("capture-audio", "capture_frames", CAPTURE_RATE),
    };
    let directory = runtime.join(leaf);
    diagnostic_control::certify(&directory, started, Some(&receipt))?;
    let completion = receipt
        .completion_sha256
        .clone()
        .ok_or_else(|| invalid("certified receipt has no completion hash"))?;
    let marker_bytes = read_bounded(&directory.join("complete.json"), MAX_MARKER_BYTES)?;
    if hex(&Sha256::digest(&marker_bytes)) != completion {
        return Err(invalid("completion marker changed after certification"));
    }
    let marker: Value = serde_json::from_slice(&marker_bytes)?;
    // Bounded before any arithmetic, even for a correctly hashed marker.
    let frames = marker[frames_key]
        .as_u64()
        .filter(|frames| *frames <= MAX_STREAM_BYTES / 2)
        .ok_or_else(|| {
            invalid("completion marker frame count is missing or exceeds the stream bound")
        })?;
    let (capture_bytes, render_bytes) = match stream {
        DiagnosticStream::Render => (0, frames * 2),
        DiagnosticStream::Capture => (frames * 2, 0),
    };
    let mut files = Vec::new();
    for (name, key, length) in [
        ("pre_aec.pcm16le", "pre_aec_sha256", Some(capture_bytes)),
        ("post_aec.pcm16le", "post_aec_sha256", Some(capture_bytes)),
        (
            "render_accepted.pcm16le",
            "render_accepted_sha256",
            Some(render_bytes),
        ),
        ("events.jsonl", "events_sha256", None),
    ] {
        let (bytes, sha256) = hash_file(&directory.join(name), MAX_STREAM_BYTES)?;
        if marker[key].as_str() != Some(sha256.as_str()) {
            return Err(invalid(&format!(
                "{leaf}/{name} does not match its certified hash"
            )));
        }
        if length.is_some_and(|length| length != bytes) {
            return Err(invalid(&format!(
                "{leaf}/{name} length disagrees with the certified frame count"
            )));
        }
        files.push(StreamFile {
            name,
            bytes,
            sha256,
        });
    }
    // The record file is read once more for parsing and re-hashed as read.
    let records = read_bounded(&directory.join("events.jsonl"), MAX_STREAM_BYTES)?;
    if hex(&Sha256::digest(&records)) != files[3].sha256 {
        return Err(invalid("diagnostic records changed during verification"));
    }
    Ok((
        VerifiedStream {
            stream,
            directory,
            completion_sha256: completion,
            frames,
            rate,
            files,
        },
        records,
    ))
}

/// Accepted render records in file order. Offsets must be contiguous and the
/// total must equal the certified frame count.
fn render_spans(records: &[u8], frames: u64) -> Result<Vec<RenderSpan>> {
    let mut spans = Vec::new();
    let mut next = 0_u64;
    for line in records.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        if line.len() > MAX_RECORD_LINE {
            return Err(invalid("diagnostic record exceeds its line bound"));
        }
        let record: Value = serde_json::from_slice(line)?;
        if record["kind"] != "render_accepted" {
            continue;
        }
        let meta = &record["meta"];
        let count = record["frames"]
            .as_u64()
            .ok_or_else(|| invalid("render record has no frame count"))?;
        let offset = record["render_accepted_byte_offset"]
            .as_u64()
            .ok_or_else(|| invalid("render record has no byte offset"))?;
        if offset != next * 2 {
            return Err(invalid("render records are not contiguous in the PCM file"));
        }
        let field = |name: &str| {
            meta[name]
                .as_u64()
                .ok_or_else(|| invalid(&format!("render record has no {name}")))
        };
        let end = next
            .checked_add(count)
            .filter(|end| *end <= frames)
            .ok_or_else(|| invalid("render records exceed the certified frame count"))?;
        spans.push(RenderSpan {
            offset_frames: next,
            frames: count,
            playback_epoch: field("playback_epoch")?,
            accepted_at_us: field("accepted_at_us")?,
            queued_frames: meta["queued_frames"]
                .as_i64()
                .ok_or_else(|| invalid("render record has no queued_frames"))?,
            silence: record["accepted_silence_kind"].as_str().map(str::to_owned),
            turn: meta["owner"]["turn"].as_u64(),
        });
        next = end;
    }
    if next != frames {
        return Err(invalid(
            "render records disagree with the certified frame count",
        ));
    }
    Ok(spans)
}

/// Read one verified PCM16LE file of a stream, re-checking its hash as read.
pub fn read_pcm(stream: &VerifiedStream, name: &str) -> Result<Vec<i16>> {
    let expected = stream.file(name)?;
    let bytes = read_bounded(&stream.directory.join(name), MAX_STREAM_BYTES)?;
    if bytes.len() as u64 != expected.bytes || hex(&Sha256::digest(&bytes)) != expected.sha256 {
        return Err(invalid("PCM file changed after verification"));
    }
    Ok(bytes
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect())
}

fn open_regular(path: &Path) -> Result<File> {
    let descriptor = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
    )?;
    let file = File::from(descriptor);
    if !file.metadata()?.is_file() {
        return Err(invalid("diagnostic evidence must be a regular file"));
    }
    Ok(file)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = open_regular(path)?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("diagnostic evidence exceeds its size bound"));
    }
    Ok(bytes)
}

fn hash_file(path: &Path, limit: u64) -> Result<(u64, String)> {
    let mut file = open_regular(path)?.take(limit + 1);
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        digest.update(&buffer[..count]);
    }
    if total > limit {
        return Err(invalid("diagnostic evidence exceeds its size bound"));
    }
    Ok((total, hex(&digest.finalize())))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
