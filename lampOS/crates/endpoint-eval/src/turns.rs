//! End-to-end turn outcomes: the production detector's events on each clip,
//! then the coordinator and provider seam as a model with stated assumptions.
//!
//! Model, per request (one `Start` to its `End`):
//! - the provider's first reply audio is ready `L` after the provider has the
//!   end of input;
//! - with an early end, reply output waits for the detector's confirmation;
//! - speech that starts before the reply is audible supersedes it, as the
//!   coordinator's replacement path does (a continuation). The provider has
//!   already received that request's end, so the successor depends on the
//!   provider settling the superseded request:
//!   - current `--provider-continuation kept-start` (`Seam::KeptStart`): the
//!     successor's start and input are kept until the superseded request's
//!     first output, then written whole, so the successor's own end reaches
//!     the provider no earlier than that;
//!   - the historical immediate start (`Seam::ImmediateStart`, the 11 October
//!     failures): the successor's start was written at once;
//!   - under either seam a provider that never settles loses the successor
//!     ("never settles"); kept-start ends it in its bounded barrier timeout;
//! - speech that starts after the reply became audible is a split.
//!
//! The detector, model and VAD are the production code. The provider timing
//! and settlement are assumptions, reported for each value, not measurements.
use crate::{Result, audio::BLOCK, replay::virtual_classifier};
use lamp_endpoint::{SMART_TURN_V3_2, SmartTurn};
use lamp_live::activity::{
    Activity, ConfirmPolicy, EarlyEnd, EndpointEvent, ObservedAudio, SpeechProbability,
    TurnDetector,
};
use serde_json::json;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

const SILENCE: f32 = 0.35;
/// A run of this many quiet blocks separates speech bursts when locating the
/// first word of resumed speech.
const BURST_GAP: usize = 10;

/// The present pilot's two profiles and the one early-end candidate, fixed
/// on development shards before any held-out clip was scored. Nothing here
/// is tuned on evaluation clips.
pub fn profiles() -> Vec<(&'static str, Option<ConfirmPolicy>)> {
    let parity = ConfirmPolicy::default();
    vec![
        ("baseline", None),
        ("parity", Some(parity)),
        (
            "early-0.9-500",
            Some(ConfirmPolicy {
                early: Some(EarlyEnd::CANDIDATE),
                ..parity
            }),
        ),
    ]
}

/// The worker keeps a successor's start for at most this long
/// (`KEPT_START_PATIENCE`); the model assumes first output within it.
const KEPT_START_PATIENCE_BLOCKS: usize = 250;

/// How the successor of a request superseded before any audible output
/// reaches the provider.
#[derive(Clone, Copy, PartialEq)]
enum Seam {
    KeptStart,
    ImmediateStart,
}

/// One request as the detector delimited it, in capture blocks.
#[derive(Clone, Debug)]
struct Request {
    start: usize,
    prefix_first: usize,
    end: Option<usize>,
    silence_blocks: usize,
    early: bool,
    /// Block of the early end's confirmation, or `None` while unresolved.
    confirmed: Option<usize>,
    continued: bool,
}

/// One profile's requests on a clip, and the clip's VAD scores.
type Detected = (Vec<Request>, Vec<f32>);

fn run_detector(pcm: &[i16], detector: &mut TurnDetector) -> Result<Detected> {
    let mut vad = SpeechProbability::default();
    let mut requests: Vec<Request> = Vec::new();
    let mut scores = Vec::with_capacity(pcm.len() / BLOCK);
    for (index, chunk) in pcm.chunks_exact(BLOCK).enumerate() {
        let samples: [i16; BLOCK] = chunk.try_into()?;
        let probability = vad.analyze(&samples);
        scores.push(probability);
        let audio = ObservedAudio {
            sequence: index as u64 + 1,
            captured_at_us: (index as u64 + 1) * 10_000,
            samples,
        };
        match detector.push(audio, probability) {
            Activity::Start(prefix) => requests.push(Request {
                start: index,
                prefix_first: index + 1 - prefix.len(),
                end: None,
                silence_blocks: 0,
                early: false,
                confirmed: None,
                continued: false,
            }),
            Activity::End(_) => {
                let decision = detector.last_endpoint().ok_or("End without decision")?;
                let request = requests.last_mut().ok_or("End without Start")?;
                request.end = Some(index);
                request.silence_blocks = (decision.silence_ms / 10) as usize;
                request.early = decision.basis == "model_early";
            }
            Activity::Fault(reason) => return Err(reason.into()),
            Activity::Quiet | Activity::Continue(_) => {}
        }
        match detector.take_endpoint_event() {
            Some(EndpointEvent::Confirmed { .. }) => {
                // A newer start would have ended the provisional state, so a
                // confirmation always belongs to the latest request.
                if let Some(request) = requests.last_mut().filter(|r| r.early) {
                    request.confirmed.get_or_insert(index);
                }
            }
            Some(EndpointEvent::Continued { .. }) => {
                let count = requests.len();
                if count >= 2 {
                    requests[count - 2].continued = true;
                }
            }
            None => {}
        }
    }
    Ok((requests, scores))
}

/// First block of the speech burst that led into `start`.
fn burst_start(scores: &[f32], start: usize) -> usize {
    let mut quiet = 0;
    let mut first = start;
    for index in (0..=start).rev() {
        if scores[index] >= SILENCE {
            first = index;
            quiet = 0;
        } else {
            quiet += 1;
            if quiet >= BURST_GAP {
                break;
            }
        }
    }
    first
}

#[derive(Clone, Copy, PartialEq)]
enum Settlement {
    Settles,
    NeverSettles,
}

struct Outcome {
    whole_thought: bool,
    splits: usize,
    interruptions: usize,
    reopenings: usize,
    openings_lost: usize,
    /// Requests whose end reached the provider later than it was spoken,
    /// because their kept start waited, and the total of those waits.
    kept_ends: usize,
    kept_blocks: usize,
    latency_ms: Option<u64>,
    audible_before_1200: bool,
    ended: bool,
}

fn simulate(
    requests: &[Request],
    scores: &[f32],
    latency: usize,
    seam: Seam,
    settlement: Settlement,
) -> Outcome {
    let mut splits = 0;
    let mut interruptions = 0;
    let mut openings_lost = 0;
    let mut reopenings = 0;
    let mut lost = false;
    let mut final_audible = None;
    // Kept start: the block at which this request's kept start, input and
    // end are written, the superseded request's first output.
    let mut released = 0;
    let (mut kept_ends, mut kept_blocks) = (0, 0);
    for (index, request) in requests.iter().enumerate() {
        if index > 0 {
            reopenings += 1;
            if request.prefix_first > burst_start(scores, request.start) {
                openings_lost += 1;
            }
        }
        let Some(end) = request.end else {
            break;
        };
        // Reply output starts when the provider is ready and, after an early
        // end, once the detector confirmed it.
        if released > end {
            kept_ends += 1;
            kept_blocks += released - end;
        }
        let ready = end.max(released) + latency;
        let audible = if request.early {
            request.confirmed.map(|confirmed| ready.max(confirmed))
        } else {
            Some(ready)
        };
        released = 0;
        match (requests.get(index + 1), audible) {
            (Some(next), Some(at)) if next.start >= at => splits += 1,
            (Some(next), _) => {
                // Superseded before any audible output; its end was committed.
                interruptions += 1;
                if settlement == Settlement::NeverSettles {
                    lost = true;
                }
                if seam == Seam::KeptStart {
                    released = ready.min(next.start + KEPT_START_PATIENCE_BLOCKS);
                }
            }
            (None, at) => final_audible = at.map(|at| (at, end, request.silence_blocks)),
        }
    }
    let ended = final_audible.is_some();
    // From the start of the first quiet block to the start of the audible
    // block: the baseline with an instant provider measures exactly 1,200 ms.
    let latency_ms = final_audible.map(|(at, end, silence)| {
        let first_quiet = end + 1 - silence;
        ((at + 1 - first_quiet) * 10) as u64
    });
    Outcome {
        whole_thought: ended && splits == 0 && !lost,
        splits,
        interruptions,
        reopenings,
        openings_lost,
        kept_ends,
        kept_blocks,
        audible_before_1200: latency_ms.is_some_and(|ms| ms < 1_200),
        latency_ms,
        ended,
    }
}

fn percentile(values: &mut [u64], q: f64) -> Option<u64> {
    values.sort_unstable();
    (!values.is_empty()).then(|| values[((values.len() - 1) as f64 * q).round() as usize])
}

/// One fixture: whether it ends a finished turn, and how to compose it.
pub struct Item {
    pub id: String,
    pub label: bool,
}

/// Turn outcomes for every item, profile, provider latency and settlement.
pub fn run(
    clips: &[Item],
    load: &(dyn Fn(usize) -> Result<Vec<i16>> + Sync),
    model: &Path,
    verdict_latency_blocks: u16,
    latencies_ms: &[u64],
    threads: usize,
    out: &Path,
) -> Result<()> {
    // Beyond the patience the kept start is written without first output,
    // which this model does not describe.
    if let Some(ms) = latencies_ms
        .iter()
        .find(|ms| ms.div_ceil(10) as usize >= KEPT_START_PATIENCE_BLOCKS)
    {
        return Err(
            format!("provider latency {ms} ms is not below the kept-start patience").into(),
        );
    }
    let model = Arc::new(SmartTurn::load(model, &SMART_TURN_V3_2)?);
    let profiles = profiles();
    let next = AtomicUsize::new(0);
    let detected: Mutex<Vec<(usize, Vec<Detected>)>> = Mutex::new(Vec::new());
    let failed: Mutex<Vec<String>> = Mutex::new(Vec::new());
    thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(clip) = clips.get(index) else {
                        return;
                    };
                    let result = load(index).and_then(|pcm| {
                        profiles
                            .iter()
                            .map(|(_, policy)| {
                                let mut detector = match policy {
                                    Some(policy) => TurnDetector::v1_baseline().with_confirmation(
                                        virtual_classifier(&model, verdict_latency_blocks),
                                        *policy,
                                    )?,
                                    None => TurnDetector::v1_baseline(),
                                };
                                run_detector(&pcm, &mut detector)
                            })
                            .collect::<Result<Vec<_>>>()
                    });
                    match result {
                        Ok(rows) => detected.lock().unwrap().push((index, rows)),
                        Err(error) => failed.lock().unwrap().push(format!("{}: {error}", clip.id)),
                    }
                    if index.is_multiple_of(200) {
                        eprintln!("turns {index}/{}", clips.len());
                    }
                }
            });
        }
    });
    let failed = failed.into_inner()?;
    if !failed.is_empty() {
        return Err(failed.join("\n").into());
    }
    let mut detected = detected.into_inner()?;
    detected.sort_by_key(|(index, _)| *index);
    let mut writer = BufWriter::new(File::create(out)?);
    let mut summary = Vec::new();
    for (profile_index, (name, policy)) in profiles.iter().enumerate() {
        for &latency_ms in latencies_ms {
            for (seam, settlement) in [
                (Seam::KeptStart, Settlement::Settles),
                (Seam::ImmediateStart, Settlement::Settles),
                (Seam::KeptStart, Settlement::NeverSettles),
            ] {
                let latency = latency_ms.div_ceil(10) as usize;
                let mut finished = Vec::new();
                let mut row = json!({"profile": name, "policy": policy, "provider_first_audio_ms": latency_ms,
                    "seam": if seam == Seam::KeptStart {"kept_start"} else {"historical_immediate_start"},
                    "settlement": if settlement == Settlement::Settles {"settles"} else {"never_settles"}});
                let (
                    mut whole,
                    mut splits,
                    mut clips_split,
                    mut interruptions,
                    mut clips_interrupted,
                ) = (0, 0, 0, 0, 0);
                let (mut reopenings, mut openings_lost, mut unended) = (0, 0, 0);
                let (mut kept_ends, mut kept_blocks) = (0, 0);
                let (mut unfinished, mut unfinished_early) = (0, 0);
                for (index, rows) in &detected {
                    let clip = &clips[*index];
                    let (requests, scores) = &rows[profile_index];
                    let outcome = simulate(requests, scores, latency, seam, settlement);
                    splits += outcome.splits;
                    clips_split += usize::from(outcome.splits > 0);
                    interruptions += outcome.interruptions;
                    clips_interrupted += usize::from(outcome.interruptions > 0);
                    reopenings += outcome.reopenings;
                    kept_ends += outcome.kept_ends;
                    kept_blocks += outcome.kept_blocks;
                    openings_lost += outcome.openings_lost;
                    if clip.label {
                        whole += usize::from(outcome.whole_thought);
                        unended += usize::from(!outcome.ended);
                        if outcome.whole_thought
                            && let Some(ms) = outcome.latency_ms
                        {
                            finished.push(ms);
                        }
                    } else {
                        unfinished += 1;
                        unfinished_early += usize::from(outcome.audible_before_1200);
                    }
                }
                let complete = clips.iter().filter(|clip| clip.label).count();
                let mean = (!finished.is_empty())
                    .then(|| finished.iter().sum::<u64>() / finished.len() as u64);
                row["clips"] = json!(clips.len());
                row["finished_clips"] = json!(complete);
                row["whole_thoughts"] = json!(whole);
                row["unended_finished"] = json!(unended);
                row["splits"] = json!(splits);
                row["clips_with_split"] = json!(clips_split);
                row["interruptions"] = json!(interruptions);
                row["clips_interrupted"] = json!(clips_interrupted);
                row["reopenings"] = json!(reopenings);
                row["kept_ends"] = json!(kept_ends);
                row["kept_end_wait_ms"] = json!(kept_blocks * 10);
                row["openings_lost"] = json!(openings_lost);
                row["stale_output"] = json!(0);
                row["unfinished_clips"] = json!(unfinished);
                row["unfinished_audible_before_1200"] = json!(unfinished_early);
                row["latency_ms"] = json!({"mean": mean, "min": percentile(&mut finished, 0.0),
                    "p10": percentile(&mut finished, 0.1), "p25": percentile(&mut finished, 0.25),
                    "p50": percentile(&mut finished, 0.5), "p75": percentile(&mut finished, 0.75),
                    "p90": percentile(&mut finished, 0.9), "p95": percentile(&mut finished, 0.95),
                    "max": percentile(&mut finished, 1.0)});
                serde_json::to_writer(&mut writer, &row)?;
                writer.write_all(b"\n")?;
                summary.push(row);
            }
        }
    }
    writer.flush()?;
    for row in summary {
        println!(
            "{:15} L={:>4} {:26} {:13} whole {:>4}/{} unended {:>2} | splits {:>3} ({:>3} clips) | continuations {:>3} ({:>3} clips) | openings lost {}/{} | kept ends {} ({} ms) | latency mean {} p50 {} p90 {} | unfinished audible <1.2 s {}/{}",
            row["profile"].as_str().unwrap_or_default(),
            row["provider_first_audio_ms"],
            row["seam"].as_str().unwrap_or_default(),
            row["settlement"].as_str().unwrap_or_default(),
            row["whole_thoughts"],
            row["finished_clips"],
            row["unended_finished"],
            row["splits"],
            row["clips_with_split"],
            row["interruptions"],
            row["clips_interrupted"],
            row["openings_lost"],
            row["reopenings"],
            row["kept_ends"],
            row["kept_end_wait_ms"],
            row["latency_ms"]["mean"],
            row["latency_ms"]["p50"],
            row["latency_ms"]["p90"],
            row["unfinished_audible_before_1200"],
            row["unfinished_clips"],
        );
    }
    Ok(())
}
