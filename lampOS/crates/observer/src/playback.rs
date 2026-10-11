//! Finite cached-WAV output. Software delivery is not an acoustic observation.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(test)]
mod tests;

use crate::{CPAL_REVISION, Result, files};
#[cfg(any(target_os = "macos", test))]
use serde::Deserialize;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, BufRead, Read},
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, SyncSender, sync_channel},
};

#[cfg(any(target_os = "macos", test))]
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

pub const OUTPUT_NAME: &str = "iMac Speakers";
/// Longest wait for a start grant after the ready receipt (`--prepared-start`).
pub const MAX_PREPARED_WAIT_MS: u32 = 120_000;
/// A start grant may expire at most this long after it is received.
pub const MAX_GRANT_HORIZON_NS: u64 = 2_000_000_000;
/// Largest grant line, and how many lines are read before input is ignored.
pub const MAX_GRANT_LINE_BYTES: usize = 512;
pub const MAX_GRANT_LINES: usize = 8;
pub const MAX_WAV_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_SECONDS: u32 = 120;
#[cfg(any(target_os = "macos", test))]
const MAX_PREPARED_SAMPLES: usize = 64 * 1024 * 1024 / 4;
#[cfg(any(target_os = "macos", test))]
const MAX_PHASES: usize = 4096;
#[cfg(any(target_os = "macos", test))]
const TAPS: usize = 32;
#[cfg(any(target_os = "macos", test))]
const MAX_CALLBACK_FRAMES: usize = 8192;
#[cfg(any(target_os = "macos", test))]
const MAX_CALLBACKS: u64 = 250_000;
#[cfg(any(target_os = "macos", test))]
const REQUIRED_TAIL_CALLBACKS: u64 = 2;

#[derive(Clone, Debug, Serialize)]
pub struct PlayOptions {
    pub output_device: String,
    pub wav: PathBuf,
    pub output: PathBuf,
    pub cancel_file: Option<PathBuf>,
    /// Opt-in two-phase start: prepare and open the output, publish a ready
    /// receipt, then wait at most this many milliseconds for one start grant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepared_start_ms: Option<u32>,
}

impl PlayOptions {
    pub fn validate(&self) -> Result<()> {
        validate_output_name(&self.output_device)?;
        if self.wav.as_os_str().is_empty()
            || self.output.as_os_str().is_empty()
            || self
                .cancel_file
                .as_ref()
                .is_some_and(|p| p.as_os_str().is_empty())
            || self
                .prepared_start_ms
                .is_some_and(|ms| !(1..=MAX_PREPARED_WAIT_MS).contains(&ms))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "require WAV and fresh report directory paths, and a 1..120000 ms prepared wait",
            )
            .into());
        }
        Ok(())
    }
}

pub fn validate_output_name(name: &str) -> Result<()> {
    if name != OUTPUT_NAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "explicit --output 'iMac Speakers' is required; no default or alternate route",
        )
        .into());
    }
    Ok(())
}

/// Select from enumerated output devices, never from a default-device handle.
#[cfg(any(target_os = "macos", test))]
fn unique_output_index(names: &[String], requested: &str) -> Result<usize> {
    validate_output_name(requested)?;
    let mut matches = names
        .iter()
        .enumerate()
        .filter(|(_, name)| *name == requested);
    match (matches.next(), matches.next()) {
        (Some((index, _)), None) => Ok(index),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "output name must match exactly one enumerated output device",
        )
        .into()),
    }
}

pub fn inspect_output(name: &str) -> Result<Value> {
    validate_output_name(name)?;
    #[cfg(target_os = "macos")]
    {
        macos::inspect(name)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "cached WAV output supports macOS only",
        )
        .into())
    }
}

pub fn play(options: &PlayOptions) -> Result<Value> {
    if options.prepared_start_ms.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "prepared start needs play_prepared and its grant input",
        )
        .into());
    }
    play_inner(options, None)
}

/// The prepared mode's control input: one ready receipt out, start grant
/// lines in. The caller owns where they travel (stdout and stdin for the CLI).
pub struct PreparedIo<'a> {
    pub ready: &'a mut dyn FnMut(&Value) -> io::Result<()>,
    pub inputs: Receiver<GrantInput>,
}

/// Two-phase playback: validate, cache and prepare the source and start the
/// output in silence, publish one ready receipt through `ready`, then play
/// only on one original start grant from `inputs` whose CLOCK_MONOTONIC
/// expiry has not passed when the first source frame would be submitted.
pub fn play_prepared(options: &PlayOptions, io: PreparedIo<'_>) -> Result<Value> {
    if options.prepared_start_ms.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "play_prepared needs --prepared-start",
        )
        .into());
    }
    play_inner(options, Some(io))
}

fn play_inner(options: &PlayOptions, prepared: Option<PreparedIo<'_>>) -> Result<Value> {
    options.validate()?;
    let attempt =
        files::Attempt::create_diagnostic(&options.output, &serde_json::to_value(options)?)?;
    let mut report = json!({
        "schema_version": 1, "attempt_id": attempt.id, "requested": options,
        "valid": false, "status": "pending", "audio_stream_opened": false,
        "scope": "cached_wav_output_delivery_only_not_acoustic_timing_or_conversation_score",
        "cpal_git_revision": CPAL_REVISION,
        "clock_domains": {
            "host_monotonic_ns": "lamp-ipc CLOCK_MONOTONIC; same-host diagnostics only",
            "callback_stream_ns": "CPAL CoreAudio mach absolute stream clock",
            "device_stream_ns": "CPAL estimated output delivery in stream clock; not measured DAC or acoustic time",
            "acoustic_boundary": "not measured; annotate a separate continuous room recording"
        },
        "underflow_visibility": "backend-reported xruns and callback-clock gaps only; unreported hardware/driver faults can be missed",
        "cancellation": "optional sentinel checked outside callbacks; forced termination leaves pending/incomplete artifacts",
        "source_starvation": "source is fully pre-rendered; no streaming producer or refill queue",
        "started_host_monotonic_ns": lamp_ipc::monotonic_ns()
    });
    if let Some(wait) = options.prepared_start_ms {
        report["prepared_start"] = json!({
            "max_wait_ms": wait,
            "clock": GRANT_CLOCK,
            "grant_input": "one JSON line; later lines are counted and never start playback",
            "expiry_enforced_at": "first source-frame submission in the output callback"
        });
    }
    let attempt_id = attempt.id.clone();
    let result = (|| -> Result<()> {
        if cancellation_requested(options.cancel_file.as_deref())? {
            report["status"] = json!(if prepared.is_some() {
                "cancelled_before_grant"
            } else {
                "cancelled_before_prepare"
            });
            return Ok(());
        }
        let source = load_source(&options.wav)?;
        report["source"] = serde_json::to_value(&source.info)?;
        #[cfg(target_os = "macos")]
        {
            macos::play(options, source, &mut report, prepared, &attempt_id)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (source.samples, prepared, attempt_id);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "cached WAV output supports macOS only",
            )
            .into())
        }
    })();
    if let Err(error) = result {
        report["valid"] = json!(false);
        report["status"] = json!("failed");
        report["error"] = json!(error.to_string());
    }
    report["completed_host_monotonic_ns"] = json!(lamp_ipc::monotonic_ns());
    attempt.finish(&report)?;
    Ok(report)
}

/// The clock of every grant expiry and ready receipt time.
pub const GRANT_CLOCK: &str = "CLOCK_MONOTONIC (clock_gettime), nanoseconds; on macOS Python must use \
time.clock_gettime_ns(time.CLOCK_MONOTONIC), not time.monotonic_ns() (mach_absolute_time)";

/// One control-channel item for a prepared player.
#[derive(Debug, PartialEq)]
pub enum GrantInput {
    Line(String),
    /// A line longer than `MAX_GRANT_LINE_BYTES`, or not UTF-8.
    Malformed,
    /// The parent closed the channel: a stop before any grant.
    Closed,
}

/// Reads at most `MAX_GRANT_LINES` bounded lines from `input` on its own
/// thread. The queue holds at most that many items; nothing is retried.
pub fn spawn_grant_reader(input: impl Read + Send + 'static) -> Receiver<GrantInput> {
    let (sender, receiver) = sync_channel(MAX_GRANT_LINES + 1);
    std::thread::spawn(move || read_grant_lines(input, &sender));
    receiver
}

fn read_grant_lines(input: impl Read, sender: &SyncSender<GrantInput>) {
    let mut input = io::BufReader::new(input);
    for _ in 0..MAX_GRANT_LINES {
        let mut line = Vec::new();
        let read = (&mut input)
            .take(MAX_GRANT_LINE_BYTES as u64 + 1)
            .read_until(b'\n', &mut line);
        let item = match read {
            Ok(0) => GrantInput::Closed,
            // Oversized, or cut off by end of input: never parsed as a grant.
            Ok(_) if line.last() != Some(&b'\n') => GrantInput::Malformed,
            Ok(_) => match String::from_utf8(line) {
                Ok(text) => GrantInput::Line(text.trim_end_matches(['\n', '\r']).to_owned()),
                Err(_) => GrantInput::Malformed,
            },
            Err(_) => GrantInput::Closed,
        };
        let last = !matches!(item, GrantInput::Line(_));
        if sender.send(item).is_err() || last {
            return;
        }
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    schema: u8,
    kind: String,
    attempt_id: String,
    expires_monotonic_ns: u64,
}

/// What the first control line decides; later lines decide nothing.
#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GateDecision {
    Start { expires_ns: u64 },
    Refuse(&'static str),
}

/// Accepts exactly one original grant, for this attempt, not yet expired and
/// no further ahead than `MAX_GRANT_HORIZON_NS`. Every later line is counted
/// as a duplicate and never starts or restarts playback.
#[cfg(any(target_os = "macos", test))]
pub(crate) struct StartGate {
    attempt_id: String,
    decided: Option<GateDecision>,
    pub(crate) duplicates: u64,
}

#[cfg(any(target_os = "macos", test))]
impl StartGate {
    pub(crate) fn new(attempt_id: &str) -> Self {
        Self {
            attempt_id: attempt_id.to_owned(),
            decided: None,
            duplicates: 0,
        }
    }
    pub(crate) fn input(&mut self, input: GrantInput, now_ns: u64) -> Option<GateDecision> {
        if self.decided.is_some() {
            self.duplicates += 1;
            return None;
        }
        let decision = match input {
            GrantInput::Closed => GateDecision::Refuse("stopped_before_grant"),
            GrantInput::Malformed => GateDecision::Refuse("grant_malformed"),
            GrantInput::Line(line) => self.decide(&line, now_ns),
        };
        self.decided = Some(decision);
        Some(decision)
    }
    fn decide(&self, line: &str, now_ns: u64) -> GateDecision {
        let Ok(grant) = serde_json::from_str::<Grant>(line) else {
            return GateDecision::Refuse("grant_malformed");
        };
        if grant.schema != 1 || grant.kind != "start" {
            return GateDecision::Refuse("grant_malformed");
        }
        if grant.attempt_id != self.attempt_id {
            return GateDecision::Refuse("grant_for_another_attempt");
        }
        if grant.expires_monotonic_ns <= now_ns {
            return GateDecision::Refuse("grant_expired_on_receipt");
        }
        if grant.expires_monotonic_ns - now_ns > MAX_GRANT_HORIZON_NS {
            return GateDecision::Refuse("grant_horizon_exceeds_bound");
        }
        GateDecision::Start {
            expires_ns: grant.expires_monotonic_ns,
        }
    }
}

fn cancellation_requested(path: Option<&Path>) -> io::Result<bool> {
    match path.map(std::fs::symlink_metadata) {
        None => Ok(false),
        Some(Ok(_)) => Ok(true),
        Some(Err(e)) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Some(Err(e)) => Err(e),
    }
}

#[derive(Clone, Debug, Serialize)]
struct SourceInfo {
    sha256: String,
    file_bytes: u64,
    sample_rate_hz: u32,
    channels: u16,
    bits_per_sample: u16,
    frames: u64,
    duration_s: f64,
    peak_normalized: f32,
    rail_samples: u64,
}

struct Source {
    info: SourceInfo,
    samples: Vec<f32>,
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn load_source(path: &Path) -> Result<Source> {
    // O_NONBLOCK also applies if a path is replaced by a FIFO before open.
    // Reject the final symlink and verify the opened descriptor itself is regular.
    let descriptor = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
    )?;
    let mut file = File::from(descriptor);
    let metadata = file.metadata()?;
    if !metadata.is_file() || !(44..=MAX_WAV_BYTES).contains(&metadata.len()) {
        return Err(
            invalid("WAV must be a regular non-symlink file of 44 bytes through 32 MiB").into(),
        );
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    (&mut file)
        .take(MAX_WAV_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != metadata.len() || file.metadata()?.len() != metadata.len() {
        return Err(invalid("WAV changed size during bounded read").into());
    }
    decode_source(&bytes)
}

fn decode_source(bytes: &[u8]) -> Result<Source> {
    if !(44..=MAX_WAV_BYTES as usize).contains(&bytes.len())
        || &bytes[..4] != b"RIFF"
        || &bytes[8..12] != b"WAVE"
        || u32::from_le_bytes(bytes[4..8].try_into()?) as usize + 8 != bytes.len()
    {
        return Err(invalid("require complete RIFF/WAVE with exact declared length").into());
    }
    let mut at = 12usize;
    let mut format = None;
    let mut data = None;
    while at < bytes.len() {
        if at + 8 > bytes.len() {
            return Err(invalid("partial WAV chunk header").into());
        }
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into()?) as usize;
        let start = at + 8;
        let end = start
            .checked_add(size)
            .ok_or_else(|| invalid("WAV chunk overflow"))?;
        let next = end
            .checked_add(size % 2)
            .ok_or_else(|| invalid("WAV padding overflow"))?;
        if next > bytes.len() {
            return Err(invalid("truncated WAV chunk or padding").into());
        }
        match &bytes[at..at + 4] {
            b"fmt " => {
                if format.is_some() || size < 16 {
                    return Err(invalid("missing or duplicate PCM format").into());
                }
                let value = &bytes[start..end];
                let encoding = u16::from_le_bytes(value[..2].try_into()?);
                let channels = u16::from_le_bytes(value[2..4].try_into()?);
                let rate = u32::from_le_bytes(value[4..8].try_into()?);
                let byte_rate = u32::from_le_bytes(value[8..12].try_into()?);
                let align = u16::from_le_bytes(value[12..14].try_into()?);
                let bits = u16::from_le_bytes(value[14..16].try_into()?);
                if encoding != 1
                    || bits != 16
                    || !(1..=2).contains(&channels)
                    || !(8000..=192000).contains(&rate)
                    || align != channels * 2
                    || byte_rate != rate * u32::from(align)
                {
                    return Err(invalid(
                        "require PCM16, mono/stereo, 8000..192000 Hz with consistent alignment",
                    )
                    .into());
                }
                format = Some((channels, rate));
            }
            b"data" => {
                if data.is_some() {
                    return Err(invalid("duplicate WAV data chunk").into());
                }
                data = Some(&bytes[start..end]);
            }
            _ => {}
        }
        at = next;
    }
    let (channels, rate) = format.ok_or_else(|| invalid("missing WAV format"))?;
    let data = data.ok_or_else(|| invalid("missing WAV data"))?;
    if data.is_empty() || !data.len().is_multiple_of(usize::from(channels) * 2) {
        return Err(invalid("empty or partial PCM frame").into());
    }
    let frames = data.len() as u64 / (u64::from(channels) * 2);
    if frames > u64::from(rate) * u64::from(MAX_SECONDS) {
        return Err(invalid("stimulus exceeds 120 seconds").into());
    }
    let mut peak = 0.0_f32;
    let mut rails = 0;
    let samples: Vec<f32> = data
        .chunks_exact(2)
        .map(|value| {
            let sample = i16::from_le_bytes([value[0], value[1]]);
            rails += u64::from(sample == i16::MIN || sample == i16::MAX);
            let value = f32::from(sample) / 32768.0;
            peak = peak.max(value.abs());
            value
        })
        .collect();
    if peak == 0.0 || rails != 0 {
        return Err(
            invalid("all-zero or PCM-rail stimulus is not a clean playback fixture").into(),
        );
    }
    Ok(Source {
        info: SourceInfo {
            sha256: format!("{:x}", Sha256::digest(bytes)),
            file_bytes: bytes.len() as u64,
            sample_rate_hz: rate,
            channels,
            bits_per_sample: 16,
            frames,
            duration_s: frames as f64 / f64::from(rate),
            peak_normalized: peak,
            rail_samples: rails,
        },
        samples,
    })
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Serialize)]
struct PreparedInfo {
    sha256_float32_le: String,
    sample_rate_hz: u32,
    channels: u16,
    frames: u64,
    duration_s: f64,
    peak_normalized: f32,
    rate_conversion: &'static str,
    channel_mapping: &'static str,
}

#[cfg(any(target_os = "macos", test))]
struct Prepared {
    info: PreparedInfo,
    samples: Vec<f32>,
}

#[cfg(any(target_os = "macos", test))]
fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Offline 32-tap Hann-windowed sinc; no conversion runs in the callback.
/// Phase normalization preserves DC away from zero-padded file boundaries.
#[cfg(any(target_os = "macos", test))]
fn prepare(
    source: &Source,
    rate: u32,
    channels: u16,
    cancelled: impl Fn() -> io::Result<bool>,
) -> Result<Option<Prepared>> {
    if !(8000..=192000).contains(&rate)
        || !(1..=2).contains(&channels)
        || (source.info.channels == 2 && channels != 2)
    {
        return Err(invalid(
            "output needs mono/stereo at 8000..192000 Hz; stereo is never downmixed",
        )
        .into());
    }
    let frames =
        (source.info.frames * u64::from(rate)).div_ceil(u64::from(source.info.sample_rate_hz));
    let count = frames * u64::from(channels);
    if count > MAX_PREPARED_SAMPLES as u64 {
        return Err(invalid("prepared float output exceeds 64 MiB").into());
    }
    let same_rate = rate == source.info.sample_rate_hz;
    let divisor = gcd(rate, source.info.sample_rate_hz);
    let phases = rate / divisor;
    if !same_rate && phases as usize > MAX_PHASES {
        return Err(invalid("resampling requires more than 4096 bounded phases").into());
    }
    let mut kernel = Vec::new();
    if !same_rate {
        let cutoff = (f64::from(rate) / f64::from(source.info.sample_rate_hz)).min(1.0) * 0.94;
        for phase in 0..phases {
            let fraction = f64::from(phase) / f64::from(phases);
            let mut row = [0.0_f64; TAPS];
            for (tap, weight) in row.iter_mut().enumerate() {
                let distance = tap as f64 - (TAPS / 2 - 1) as f64 - fraction;
                let angle = std::f64::consts::PI * distance * cutoff;
                let sinc = if angle.abs() < 1e-12 {
                    cutoff
                } else {
                    cutoff * angle.sin() / angle
                };
                let window =
                    0.5 + 0.5 * (std::f64::consts::PI * distance / (TAPS as f64 / 2.0)).cos();
                *weight = sinc * window;
            }
            let total: f64 = row.iter().sum();
            for weight in &mut row {
                *weight /= total;
            }
            kernel.push(row);
        }
    }
    let mut samples = Vec::with_capacity(count as usize);
    let mut peak = 0.0_f32;
    let mut hash = Sha256::new();
    for frame in 0..frames {
        if frame % 8192 == 0 && cancelled()? {
            return Ok(None);
        }
        let numerator = frame * u64::from(source.info.sample_rate_hz);
        let center = (numerator / u64::from(rate)) as i64;
        let phase = ((numerator % u64::from(rate)) / u64::from(divisor)) as usize;
        for output_channel in 0..channels {
            let input_channel = if source.info.channels == 1 {
                0
            } else {
                output_channel
            };
            let value = if same_rate {
                source.samples[frame as usize * usize::from(source.info.channels)
                    + usize::from(input_channel)]
            } else {
                let mut sum = 0.0_f64;
                for (tap, weight) in kernel[phase].iter().enumerate() {
                    let index = center + tap as i64 - (TAPS / 2 - 1) as i64;
                    if index >= 0 && index < source.info.frames as i64 {
                        sum += f64::from(
                            source.samples[index as usize * usize::from(source.info.channels)
                                + usize::from(input_channel)],
                        ) * weight;
                    }
                }
                sum as f32
            };
            if !value.is_finite() || value.abs() >= 1.0 {
                return Err(invalid(
                    "prepared output clips or is nonfinite; gain is not silently changed",
                )
                .into());
            }
            peak = peak.max(value.abs());
            hash.update(value.to_le_bytes());
            samples.push(value);
        }
    }
    if cancelled()? {
        return Ok(None);
    }
    Ok(Some(Prepared {
        info: PreparedInfo {
            sha256_float32_le: format!("{:x}", hash.finalize()),
            sample_rate_hz: rate,
            channels,
            frames,
            duration_s: frames as f64 / f64::from(rate),
            peak_normalized: peak,
            rate_conversion: if same_rate {
                "none"
            } else {
                "offline-32tap-Hann-windowed-sinc-0.94-cutoff-zero-padded-boundaries"
            },
            channel_mapping: if source.info.channels == channels {
                "preserved"
            } else {
                "mono-duplicated-to-stereo-no-gain-change"
            },
        },
        samples,
    }))
}

#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
struct Shared {
    cancelled: AtomicBool,
    /// Prepared mode: no source frame is submitted before a grant.
    armed: AtomicBool,
    /// The accepted grant's CLOCK_MONOTONIC expiry; zero until then.
    grant_expires_ns: AtomicU64,
    /// Latched when the first submission came after the grant expired.
    start_expired: AtomicBool,
    /// Host time of the callback that submitted the first source frame.
    source_started_host_ns: AtomicU64,
    /// Silent frames written while armed, before any source frame.
    armed_zero_frames: AtomicU64,
    callbacks: AtomicU64,
    submitted_frames: AtomicU64,
    tail_callbacks: AtomicU64,
    intentional_zero_frames: AtomicU64,
    rejected_frames: AtomicU64,
    faults: AtomicU64,
    backend_xruns: AtomicU64,
    stream_errors: AtomicU64,
    stream_error_kinds: AtomicU64,
    timestamp_gaps: AtomicU64,
    first_host_ns: AtomicU64,
    last_host_ns: AtomicU64,
    first_device_ns: AtomicU64,
    last_callback_ns: AtomicU64,
    estimated_source_end_device_ns: AtomicU64,
    max_host_gap_ns: AtomicU64,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Serialize)]
struct Snapshot {
    cancelled: bool,
    armed: bool,
    grant_expires_ns: u64,
    start_expired: bool,
    source_started_host_ns: u64,
    armed_zero_frames: u64,
    callbacks: u64,
    submitted_frames: u64,
    tail_callbacks: u64,
    intentional_zero_frames: u64,
    rejected_frames: u64,
    faults: u64,
    backend_xruns: u64,
    stream_errors: u64,
    stream_error_kinds: u64,
    timestamp_gaps: u64,
    first_host_ns: u64,
    last_host_ns: u64,
    first_device_ns: u64,
    last_callback_ns: u64,
    estimated_source_end_device_ns: u64,
    max_host_gap_ns: u64,
}

#[cfg(any(target_os = "macos", test))]
impl Shared {
    fn snapshot(&self) -> Snapshot {
        let read = |value: &AtomicU64| value.load(Ordering::Acquire);
        Snapshot {
            cancelled: self.cancelled.load(Ordering::Acquire),
            armed: self.armed.load(Ordering::Acquire),
            grant_expires_ns: read(&self.grant_expires_ns),
            start_expired: self.start_expired.load(Ordering::Acquire),
            source_started_host_ns: read(&self.source_started_host_ns),
            armed_zero_frames: read(&self.armed_zero_frames),
            callbacks: read(&self.callbacks),
            submitted_frames: read(&self.submitted_frames),
            tail_callbacks: read(&self.tail_callbacks),
            intentional_zero_frames: read(&self.intentional_zero_frames),
            rejected_frames: read(&self.rejected_frames),
            faults: read(&self.faults),
            backend_xruns: read(&self.backend_xruns),
            stream_errors: read(&self.stream_errors),
            stream_error_kinds: read(&self.stream_error_kinds),
            timestamp_gaps: read(&self.timestamp_gaps),
            first_host_ns: read(&self.first_host_ns),
            last_host_ns: read(&self.last_host_ns),
            first_device_ns: read(&self.first_device_ns),
            last_callback_ns: read(&self.last_callback_ns),
            estimated_source_end_device_ns: read(&self.estimated_source_end_device_ns),
            max_host_gap_ns: read(&self.max_host_gap_ns),
        }
    }
    fn note_error(&self, kind: u64) {
        self.stream_error_kinds.fetch_or(kind, Ordering::Relaxed);
        self.stream_errors.fetch_add(1, Ordering::Release);
        self.faults.fetch_or(16, Ordering::Release);
    }
}

#[cfg(any(target_os = "macos", test))]
impl Snapshot {
    fn complete(&self, target: u64) -> bool {
        !self.cancelled
            && self.faults == 0
            && self.submitted_frames == target
            && self.tail_callbacks >= REQUIRED_TAIL_CALLBACKS
            && self.estimated_source_end_device_ns > 0
            && self.last_callback_ns >= self.estimated_source_end_device_ns
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy)]
struct Times {
    host_ns: u64,
    callback_ns: u64,
    device_ns: u64,
}

#[cfg(any(target_os = "macos", test))]
struct Renderer {
    prepared: Prepared,
    shared: Arc<Shared>,
    cursor: usize,
    previous: Option<(Times, usize)>,
}

#[cfg(any(target_os = "macos", test))]
impl Renderer {
    fn render(&mut self, output: &mut [f32], time: Times, xrun: bool) {
        output.fill(0.0);
        let channels = usize::from(self.prepared.info.channels);
        let frames = output.len() / channels;
        let index = self.shared.callbacks.fetch_add(1, Ordering::Relaxed);
        if index == 0 {
            self.shared
                .first_host_ns
                .store(time.host_ns, Ordering::Relaxed);
            self.shared
                .first_device_ns
                .store(time.device_ns, Ordering::Relaxed);
        }
        self.shared
            .last_host_ns
            .store(time.host_ns, Ordering::Relaxed);
        self.shared
            .last_callback_ns
            .store(time.callback_ns, Ordering::Release);
        if xrun {
            self.shared.backend_xruns.fetch_add(1, Ordering::Relaxed);
            self.shared.faults.fetch_or(1, Ordering::Relaxed);
        }
        if frames == 0
            || frames > MAX_CALLBACK_FRAMES
            || !output.len().is_multiple_of(channels)
            || index >= MAX_CALLBACKS
        {
            self.shared
                .rejected_frames
                .fetch_add(frames as u64, Ordering::Relaxed);
            self.shared.faults.fetch_or(2, Ordering::Release);
            return;
        }
        let rate = u64::from(self.prepared.info.sample_rate_hz);
        let invalid_clock =
            time.host_ns == 0 || time.callback_ns == 0 || time.device_ns < time.callback_ns;
        let gap = self.previous.is_some_and(|(prior, count)| {
            self.shared.max_host_gap_ns.fetch_max(
                time.host_ns.saturating_sub(prior.host_ns),
                Ordering::Relaxed,
            );
            let expected = count as u64 * 1_000_000_000 / rate;
            time.host_ns <= prior.host_ns
                || time.callback_ns <= prior.callback_ns
                || time.device_ns <= prior.device_ns
                || time
                    .device_ns
                    .saturating_sub(prior.device_ns)
                    .abs_diff(expected)
                    > 2_000_000_000 / rate + 2
        });
        self.previous = Some((time, frames));
        if invalid_clock || gap {
            self.shared.timestamp_gaps.fetch_add(1, Ordering::Relaxed);
            self.shared.faults.fetch_or(4, Ordering::Release);
        }
        if self.shared.cancelled.load(Ordering::Acquire)
            || self.shared.faults.load(Ordering::Acquire) != 0
        {
            self.shared
                .rejected_frames
                .fetch_add(frames as u64, Ordering::Release);
            return;
        }
        if self.cursor == 0 && self.shared.armed.load(Ordering::Acquire) {
            // The grant's expiry is enforced here, at the first source-frame
            // submission, not when the parent's request returned.
            let expires = self.shared.grant_expires_ns.load(Ordering::Acquire);
            if expires != 0
                && time.host_ns > expires
                && !self.shared.start_expired.load(Ordering::Acquire)
            {
                self.shared.start_expired.store(true, Ordering::Release);
            }
            if expires == 0 || self.shared.start_expired.load(Ordering::Acquire) {
                self.shared
                    .armed_zero_frames
                    .fetch_add(frames as u64, Ordering::Relaxed);
                return;
            }
            self.shared
                .source_started_host_ns
                .store(time.host_ns, Ordering::Release);
        }
        let remaining = self.prepared.samples.len() - self.cursor;
        let copied = remaining.min(output.len());
        output[..copied].copy_from_slice(&self.prepared.samples[self.cursor..self.cursor + copied]);
        self.cursor += copied;
        self.shared
            .submitted_frames
            .fetch_add((copied / channels) as u64, Ordering::Release);
        self.shared.intentional_zero_frames.fetch_add(
            ((output.len() - copied) / channels) as u64,
            Ordering::Relaxed,
        );
        if copied == 0 {
            self.shared.tail_callbacks.fetch_add(1, Ordering::Release);
        }
        if copied > 0 && self.cursor == self.prepared.samples.len() {
            self.shared.estimated_source_end_device_ns.store(
                time.device_ns
                    .saturating_add((copied / channels) as u64 * 1_000_000_000 / rate),
                Ordering::Release,
            );
        }
    }
}

/// Checked by the controller even when the device stops invoking callbacks.
#[cfg(any(target_os = "macos", test))]
fn stop_decision(
    state: &Snapshot,
    target: u64,
    duration_ns: u64,
    elapsed_ns: u64,
    now_host_ns: u64,
    cancelled: bool,
) -> Option<&'static str> {
    if cancelled || state.cancelled {
        return Some("cancelled");
    }
    if state.faults != 0 {
        return Some("callback_or_stream_fault");
    }
    if state.start_expired {
        return Some("start_expired_at_first_submission");
    }
    if state.complete(target) {
        return Some("frames_submitted_and_estimated_drain_checked");
    }
    if elapsed_ns >= duration_ns.saturating_add(8_000_000_000) {
        return Some("wall_deadline");
    }
    if state.callbacks == 0 && elapsed_ns >= 5_000_000_000 {
        return Some("first_callback_timeout");
    }
    if state.callbacks > 0 && now_host_ns.saturating_sub(state.last_host_ns) >= 2_000_000_000 {
        return Some("callback_stall");
    }
    None
}

/// How a prepared player's wait for its grant ended.
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, PartialEq)]
enum GrantWait {
    /// The grant is installed for the output callback.
    Granted { expires_ns: u64, received_ns: u64 },
    /// Nothing is played; the status names why.
    NotStarted {
        status: &'static str,
        reason: Option<&'static str>,
    },
}

/// The prepared controller between the ready receipt and the grant. The
/// output stream is already running in silence. `next` waits at most a few
/// milliseconds for one control item; `cancelled` checks the sentinel. The
/// grant's expiry is only installed here; the callback enforces it.
#[cfg(any(target_os = "macos", test))]
fn wait_for_grant(
    shared: &Shared,
    gate: &mut StartGate,
    max_wait_ns: u64,
    mut now_ns: impl FnMut() -> u64,
    mut next: impl FnMut() -> Option<GrantInput>,
    mut cancelled: impl FnMut() -> io::Result<bool>,
) -> io::Result<GrantWait> {
    let began = now_ns();
    loop {
        let not_started = |status, reason| Ok(GrantWait::NotStarted { status, reason });
        if cancelled()? {
            return not_started("cancelled_before_grant", None);
        }
        let state = shared.snapshot();
        if state.faults != 0 {
            return not_started("failed_before_grant", Some("callback_or_stream_fault"));
        }
        let now = now_ns();
        if now.saturating_sub(state.last_host_ns) >= 2_000_000_000 {
            return not_started("failed_before_grant", Some("callback_stall"));
        }
        if let Some(input) = next() {
            let received = now_ns();
            match gate.input(input, received) {
                Some(GateDecision::Start { expires_ns }) => {
                    shared.grant_expires_ns.store(expires_ns, Ordering::Release);
                    return Ok(GrantWait::Granted {
                        expires_ns,
                        received_ns: received,
                    });
                }
                Some(GateDecision::Refuse(reason)) => {
                    let status = if reason == "stopped_before_grant" {
                        "stopped_before_grant"
                    } else {
                        "grant_refused"
                    };
                    return not_started(status, Some(reason));
                }
                None => {}
            }
        } else if now.saturating_sub(began) >= max_wait_ns {
            return not_started("grant_timeout", None);
        }
    }
}
