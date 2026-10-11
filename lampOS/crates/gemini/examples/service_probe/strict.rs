//! Explicit, bounded Require/2 s probes. This is not the production supervisor.
use super::{Outcome, bounded_regular_file, session};
use lamp_gemini::{
    BarrierPolicy, BarrierWait, Connection, Event, InputSender, Lineage, RequestId, Resumption,
    SessionConfig, SessionId, State, Timeouts,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{future::Future, path::Path, time::Duration};
use tokio::time::Instant;

const TRIAL_LIMIT: Duration = Duration::from_secs(40);
const AFTER_ANSWER: Duration = Duration::from_secs(8);
const MAX_EVENTS: usize = 2_048;
const MAX_TEXT_BYTES: usize = 65_536;
const MAX_SAMPLES: usize = 160_000;

pub(super) fn requested(arguments: &[String]) -> bool {
    arguments.get(2).is_some_and(|name| {
        matches!(
            name.as_str(),
            "continue-before-output" | "continue-during-output" | "repeat-before-output"
        )
    })
}

/// No conversion, normalization, trimming, padding or duplicated continuation.
fn pcm(bytes: &[u8]) -> Outcome<Vec<i16>> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("strict probe requires a complete RIFF WAVE".into());
    }
    let size = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    if size(4).checked_add(8) != Some(bytes.len()) {
        return Err("RIFF length mismatch".into());
    }
    let (mut format, mut data, mut at) = (false, None, 12);
    while at < bytes.len() {
        if bytes.len() - at < 8 {
            return Err("truncated WAV chunk header".into());
        }
        let length = size(at + 4);
        let start = at + 8;
        let end = start.checked_add(length).ok_or("WAV chunk overflow")?;
        if end > bytes.len() {
            return Err("truncated WAV chunk".into());
        }
        match &bytes[at..at + 4] {
            b"fmt " => {
                if format
                    || length != 16
                    || bytes[start..end] != [1, 0, 1, 0, 128, 62, 0, 0, 0, 125, 0, 0, 2, 0, 16, 0]
                {
                    return Err(
                        "strict probe requires mono 16 kHz PCM16 with one standard fmt chunk"
                            .into(),
                    );
                }
                format = true;
            }
            b"data" if data.is_some() => return Err("duplicate WAV data".into()),
            b"data" => data = Some(&bytes[start..end]),
            _ => {}
        }
        at = end.checked_add(length & 1).ok_or("WAV padding overflow")?;
        if at > bytes.len() {
            return Err("missing WAV padding".into());
        }
    }
    let data = data
        .filter(|_| format)
        .ok_or("missing WAV format or data")?;
    if data.len() % 2 != 0 || !(320..=MAX_SAMPLES * 2).contains(&data.len()) {
        return Err("strict input must contain 10 ms to 10 s of complete PCM16 samples".into());
    }
    Ok(data
        .chunks_exact(2)
        .map(|p| i16::from_le_bytes([p[0], p[1]]))
        .collect())
}

/// The production quiet bound of a barrier.
const BARRIER: Duration = Duration::from_secs(2);
/// The longest quiet bound the library accepts. The probe asks for no more:
/// a longer one would mean widening what production may be configured with.
const LONGEST_BARRIER: Duration = Duration::from_secs(5);
/// The longest the probe keeps its own second End back. With the longest
/// upload and the longest quiet bound this still ends inside the trial.
const LONGEST_HOLD: Duration = Duration::from_secs(20);

/// Always `Require`. `quiet` is the barrier's quiet bound: two seconds in
/// production. A probe may ask for longer to watch whether a terminal comes
/// late. That is observation only: the policy still assumes nothing, the
/// successor's End stays withheld for as long as nothing settles, and no
/// answer can be attributed on the strength of the longer wait.
///
/// `wait` is where that bound starts. Every barrier's end is reported with
/// counts of what was withheld behind it: the probe's drop evidence.
fn config(config: SessionConfig, quiet: Duration, wait: BarrierWait) -> Outcome<SessionConfig> {
    Ok(config
        .timeouts(Timeouts {
            barrier: quiet,
            ..Timeouts::default()
        })?
        .barrier_policy(BarrierPolicy::Require)
        .barrier_wait(wait)
        .barrier_reports(true)
        .resumption(Resumption::Retain))
}

/// A trailing `barrier-wait=at-commit`, removed from `arguments`.
fn barrier_wait(arguments: &mut Vec<String>) -> Outcome<BarrierWait> {
    let wait = match arguments
        .last()
        .and_then(|last| last.strip_prefix("barrier-wait="))
    {
        None => return Ok(BarrierWait::FromInterruption),
        Some("at-commit") => BarrierWait::AtCommit,
        Some("from-interruption") => BarrierWait::FromInterruption,
        Some(_) => return Err("barrier-wait must be at-commit or from-interruption".into()),
    };
    arguments.pop();
    Ok(wait)
}

/// A trailing `hold-end-ms=N`, removed from `arguments`: how long the probe
/// keeps its own second End back after the last sample. Under
/// `barrier-wait=at-commit` nothing is waiting to be committed meanwhile, so
/// the connection stays up, to the absolute bound, and whatever arrives for
/// the first request is recorded as it comes. Observation only.
fn end_hold(arguments: &mut Vec<String>) -> Outcome<Duration> {
    let Some(value) = arguments
        .last()
        .and_then(|last| last.strip_prefix("hold-end-ms="))
    else {
        return Ok(Duration::ZERO);
    };
    let hold = Duration::from_millis(value.parse::<u64>()?);
    if hold.is_zero() || hold > LONGEST_HOLD {
        return Err("hold-end-ms must be 1 to 20000".into());
    }
    arguments.pop();
    Ok(hold)
}

/// A trailing `barrier-ms=N`, removed from `arguments`.
fn observed_barrier(arguments: &mut Vec<String>) -> Outcome<Duration> {
    let Some(value) = arguments
        .last()
        .and_then(|last| last.strip_prefix("barrier-ms="))
    else {
        return Ok(BARRIER);
    };
    let quiet = Duration::from_millis(value.parse::<u64>()?);
    if !(BARRIER..=LONGEST_BARRIER).contains(&quiet) {
        return Err("barrier-ms must be 2000 to 5000, the longest the library accepts".into());
    }
    arguments.pop();
    Ok(quiet)
}

#[derive(Serialize)]
struct Sent {
    session: u64,
    request: u64,
    planned_samples: usize,
    accepted_samples: usize,
    accepted_blocks: usize,
    start_queued_us: Option<u64>,
    end_queued_us: Option<u64>,
}
impl Sent {
    fn new(request: u64, planned_samples: usize) -> Self {
        Self {
            session: 1,
            request,
            planned_samples,
            accepted_samples: 0,
            accepted_blocks: 0,
            start_queued_us: None,
            end_queued_us: None,
        }
    }
}

struct Observed {
    since: Instant,
    events: Vec<Value>,
    text_bytes: usize,
    first_audio_us: [Option<u64>; 2],
    audio_samples: [usize; 2],
    idle_us: [Option<u64>; 2],
    interrupted_us: [Option<u64>; 2],
    second_waiting: Option<bool>,
    assumed: bool,
    foreign_lineage: bool,
}
impl Observed {
    fn new() -> Self {
        Self {
            since: Instant::now(),
            events: Vec::new(),
            text_bytes: 0,
            first_audio_us: [None; 2],
            audio_samples: [0; 2],
            idle_us: [None; 2],
            interrupted_us: [None; 2],
            second_waiting: None,
            assumed: false,
            foreign_lineage: false,
        }
    }
    fn event(&mut self, event: Event) -> Result<(), &'static str> {
        let at = elapsed_us(self.since);
        let mut row = json!({"observed_us":at});
        let mut identity = |lineage: Lineage| {
            row["session"] = json!(lineage.session.get());
            row["request"] = json!(lineage.request.get());
            if lineage.session.get() == 1 && (1..=2).contains(&lineage.request.get()) {
                Some(lineage.request.get() as usize - 1)
            } else {
                self.foreign_lineage = true;
                None
            }
        };
        match event {
            Event::InputStarted {
                lineage,
                waiting_for_barrier,
            } => {
                if identity(lineage) == Some(1) {
                    self.second_waiting = Some(waiting_for_barrier);
                }
                row["kind"] = json!("input_started");
                row["waiting_for_barrier"] = json!(waiting_for_barrier);
            }
            Event::Audio {
                lineage,
                sequence,
                pcm,
            } => {
                if let Some(index) = identity(lineage) {
                    self.first_audio_us[index].get_or_insert(at);
                    self.audio_samples[index] += pcm.len();
                }
                row["kind"] = json!("audio");
                row["sequence"] = json!(sequence);
                row["samples"] = json!(pcm.len());
            }
            Event::TurnComplete { lineage, idle } => {
                if let Some(index) = identity(lineage)
                    && idle
                {
                    self.idle_us[index].get_or_insert(at);
                }
                row["kind"] = json!("turn_complete");
                row["idle"] = json!(idle);
            }
            Event::Interrupted { lineage } => {
                if let Some(index) = identity(lineage) {
                    self.interrupted_us[index].get_or_insert(at);
                }
                row["kind"] = json!("interrupted");
            }
            Event::GenerationComplete { lineage } => {
                identity(lineage);
                row["kind"] = json!("generation_complete");
            }
            Event::OutputTranscript {
                lineage,
                text,
                finished,
            } => {
                identity(lineage);
                row["kind"] = json!("output_transcript");
                row["text"] = json!(text);
                row["finished"] = json!(finished);
            }
            Event::ModelText { lineage, text } => {
                identity(lineage);
                row["kind"] = json!("model_text");
                row["text"] = json!(text);
            }
            Event::UncorrelatedInputTranscript {
                session,
                text,
                finished,
            } => {
                row["kind"] = json!("uncorrelated_input_transcript");
                row["session"] = json!(session.get());
                row["text"] = json!(text);
                row["finished"] = json!(finished);
            }
            Event::BarrierAssumed {
                session,
                superseded,
                successor,
            } => {
                self.assumed = true;
                row["kind"] = json!("barrier_assumed");
                row["session"] = json!(session.get());
                row["superseded"] = json!(superseded.map(RequestId::get));
                row["successor"] = json!(successor.map(RequestId::get));
            }
            Event::Barrier {
                session,
                outcome,
                report,
            } => {
                // Counts of what was received for the earlier response and
                // withheld. No audio and no text.
                row["kind"] = json!("barrier");
                row["session"] = json!(session.get());
                row["outcome"] = json!(format!("{outcome:?}"));
                row["superseded"] = json!(report.superseded.map(RequestId::get));
                row["successor"] = json!(report.successor.map(RequestId::get));
                row["waited_ms"] = json!(report.waited.as_millis());
                row["quiet_ms"] = json!(report.quiet.as_millis());
                row["successor_input_open"] = json!(report.successor_input_open);
                row["end_withheld_ms"] = json!(report.end_withheld.map(|held| held.as_millis()));
                row["audio_frames"] = json!(report.audio_frames);
                row["audio_samples"] = json!(report.audio_samples);
                row["text_frames"] = json!(report.text_frames);
                row["generation_complete"] = json!(report.generation_complete);
                row["interrupted"] = json!(report.interrupted);
                row["completions_not_idle"] = json!(report.completions_not_idle);
            }
            Event::Discarded { session, reason } => {
                row["kind"] = json!("discarded");
                row["session"] = json!(session.get());
                row["reason"] = json!(format!("{reason:?}"));
            }
            Event::VoiceActivity {
                session,
                kind,
                audio_offset,
            } => {
                row["kind"] = json!("voice_activity");
                row["session"] = json!(session.get());
                row["activity"] = json!(format!("{kind:?}"));
                row["audio_offset_us"] = json!(audio_offset.map(|d| d.as_micros() as u64));
            }
            Event::GoAway { session, time_left } => {
                row["kind"] = json!("go_away");
                row["session"] = json!(session.get());
                row["time_left_us"] = json!(time_left.map(|d| d.as_micros() as u64));
            }
        }
        self.text_bytes += row["text"].as_str().map_or(0, str::len);
        if self.events.len() == MAX_EVENTS || self.text_bytes > MAX_TEXT_BYTES {
            return Err("event_or_text_limit");
        }
        self.events.push(row);
        Ok(())
    }
}

fn elapsed_us(since: Instant) -> u64 {
    since.elapsed().as_micros() as u64
}

async fn send(
    input: InputSender,
    sent: &mut Sent,
    samples: &[i16],
    since: Instant,
    hold: Duration,
) -> Result<(), String> {
    let request = RequestId::new(sent.request).map_err(|e| format!("{e:?}"))?;
    input.try_start(request).map_err(|e| format!("{e:?}"))?;
    sent.start_queued_us = Some(elapsed_us(since));
    for (sequence, block) in samples.chunks(160).enumerate() {
        input
            .try_audio(request, sequence as u64, std::time::Instant::now(), block)
            .map_err(|e| format!("{e:?}"))?;
        sent.accepted_blocks += 1;
        sent.accepted_samples += block.len();
        // No catch-up burst following a delayed scheduler tick.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if !hold.is_zero() {
        // The End is kept back. The wire stays as the last sample left it.
        tokio::time::sleep(hold).await;
    }
    input.try_end(request).map_err(|e| format!("{e:?}"))?;
    sent.end_queued_us = Some(elapsed_us(since));
    Ok(())
}

/// Poll both halves of the public API for every sending and observation phase.
async fn drive<F: Future>(
    connection: &mut Connection,
    seen: &mut Observed,
    deadline: Instant,
    operation: F,
) -> Result<F::Output, &'static str> {
    tokio::pin!(operation);
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(deadline) => return Err("trial_deadline"),
            event = connection.next_event() => match event {
                Some(event) => seen.event(event)?,
                None => return Err("connection_ended"),
            },
            result = &mut operation => return Ok(result),
        }
    }
}

async fn trial(
    connection: Connection,
    first: &[i16],
    second: &[i16],
    gap: Duration,
    name: &str,
) -> Value {
    trial_holding(connection, first, second, gap, name, Duration::ZERO).await
}

/// `hold` keeps the second request's End back after its last sample.
async fn trial_holding(
    mut connection: Connection,
    first: &[i16],
    second: &[i16],
    gap: Duration,
    name: &str,
    hold: Duration,
) -> Value {
    let mut seen = Observed::new();
    let since = seen.since;
    // Never renewed by progress, a new request, or an early service response.
    let deadline = since + TRIAL_LIMIT;
    let (mut one, mut two) = (Sent::new(1, first.len()), Sent::new(2, second.len()));
    let mut aftermath_start = None;
    let mut aftermath_complete = false;
    let mut input_error = None;
    let stopped = async {
        for (sent, samples) in [(&mut one, first), (&mut two, second)] {
            if sent.request == 2 {
                if name == "continue-during-output" {
                    // The first request's own audio, actually received: a
                    // condition, one event at a time, never a time guess.
                    while seen.first_audio_us[0].is_none() {
                        match tokio::time::timeout_at(deadline, connection.next_event()).await {
                            Ok(Some(event)) => seen.event(event)?,
                            Ok(None) => return Err("connection_ended"),
                            Err(_) => return Err("trial_deadline"),
                        }
                    }
                }
                drive(
                    &mut connection,
                    &mut seen,
                    deadline,
                    tokio::time::sleep(gap),
                )
                .await?;
            }
            let input = connection.input();
            let hold = if sent.request == 2 {
                hold
            } else {
                Duration::ZERO
            };
            if let Err(error) = drive(
                &mut connection,
                &mut seen,
                deadline,
                send(input, sent, samples, since, hold),
            )
            .await?
            {
                input_error = Some(error);
                return Err("input_refused");
            }
        }
        while seen.idle_us[1].is_none() {
            // One event at a time so completion is a condition, not a time guess.
            match tokio::time::timeout_at(deadline, connection.next_event()).await {
                Ok(Some(event)) => seen.event(event)?,
                Ok(None) => return Err("connection_ended"),
                Err(_) => return Err("trial_deadline"),
            }
        }
        let start = Instant::now();
        aftermath_start = Some(start);
        drive(
            &mut connection,
            &mut seen,
            deadline,
            tokio::time::sleep(AFTER_ANSWER),
        )
        .await?;
        aftermath_complete = true;
        Ok(())
    }
    .await;
    let state = connection.state();
    let prior_audio = two
        .start_queued_us
        .map(|start| seen.first_audio_us[0].is_some_and(|at| at <= start));
    let answered = seen.idle_us[1].is_some() && seen.audio_samples[1] > 0;
    let barrier_confirmed =
        seen.idle_us[0].is_some_and(|idle| two.start_queued_us.is_some_and(|start| idle >= start));
    let case = match (seen.second_waiting, prior_audio) {
        (Some(true), Some(false)) => "barrier_wait_observed_without_prior_audio",
        (Some(true), Some(true)) => "barrier_wait_observed_after_audio",
        (Some(false), _) => "no_barrier_at_second_start",
        _ => "second_start_not_observed",
    };
    let outcome = if state == State::Disconnected(lamp_gemini::Error::BarrierTimeout) {
        "strict_barrier_timeout"
    } else if stopped.is_err() {
        if answered {
            "answer_completed_but_observation_incomplete"
        } else {
            "incomplete"
        }
    } else if !answered {
        "completed_without_audio"
    } else if case == "barrier_wait_observed_without_prior_audio"
        && barrier_confirmed
        && !seen.assumed
        && !seen.foreign_lineage
    {
        "strict_barrier_confirmed_and_answered"
    } else if name == "continue-during-output"
        && case == "barrier_wait_observed_after_audio"
        && barrier_confirmed
        && !seen.assumed
        && !seen.foreign_lineage
    {
        "strict_barrier_confirmed_after_audio_and_answered"
    } else {
        "answered_other_case"
    };
    json!({
        "probe":name, "barrier_policy":"Require", "barrier_ms":2_000,
        "trial_limit_ms":TRIAL_LIMIT.as_millis(), "gap_ms":gap.as_millis(),
        "observed_ms":since.elapsed().as_millis(), "outcome":outcome, "case":case,
        "stop_reason":stopped.err(), "input_error":input_error, "final_state":format!("{state:?}"),
        "inputs":[one,two], "first_audio_observed_us":seen.first_audio_us,
        "received_audio_samples":seen.audio_samples, "idle_observed_us":seen.idle_us,
        "first_audio_observed_before_second_queue":prior_audio,
        "request_one_idle_observed_after_second_queue":barrier_confirmed,
        "interrupted_observed_us":seen.interrupted_us,
        "aftermath":{"requested_ms":AFTER_ANSWER.as_millis(),
            "observed_ms":aftermath_start.map(|start| start.elapsed().as_millis()), "window_complete":aftermath_complete},
        "semantic_assessment":"unscored", "foreign_lineage_observed":seen.foreign_lineage,
        "end_wire_ack_available":false, "dropped_output_counts_available":false,
        "absolute_before_output_order_proven":false, "events":seen.events,
    })
}

/// Whether a result passes the strict contract: the earlier response was
/// confirmed finished and the successor was answered, before output or after
/// it, under production's bound. A run under a longer bound observes and
/// passes nothing, whatever it saw.
pub(super) fn passed(result: &Value) -> bool {
    let confirmed = matches!(
        result["outcome"].as_str(),
        Some(
            "strict_barrier_confirmed_and_answered"
                | "strict_barrier_confirmed_after_audio_and_answered"
        )
    );
    confirmed && result["observation_only"] == false
}

pub(super) async fn run(arguments: &[String]) -> Outcome<Value> {
    let mut arguments = arguments.to_vec();
    // Trailing settings, in this order when more than one is given:
    // `barrier-ms=N barrier-wait=at-commit hold-end-ms=N`.
    let hold = end_hold(&mut arguments)?;
    let wait = barrier_wait(&mut arguments)?;
    let quiet = observed_barrier(&mut arguments)?;
    if !hold.is_zero() && wait != BarrierWait::AtCommit {
        return Err(
            "hold-end-ms needs barrier-wait=at-commit: without it the quiet bound ends the run first"
                .into(),
        );
    }
    let (provider, first_path, name, second_path, gap) = match arguments.as_slice() {
        [provider, first, name] if name == "repeat-before-output" => (provider, first, name, first, None),
        [provider, first, name, gap] if name == "repeat-before-output" => (provider, first, name, first, Some(gap)),
        [provider, first, name, second] if name == "continue-before-output" => (provider, first, name, second, None),
        [provider, first, name, second, gap] if name == "continue-before-output" => (provider, first, name, second, Some(gap)),
        [provider, first, name, second] if name == "continue-during-output" => (provider, first, name, second, None),
        [provider, first, name, second, gap] if name == "continue-during-output" => (provider, first, name, second, Some(gap)),
        _ => return Err("usage: ... PRIVATE_PROVIDER FIRST.wav continue-before-output SECOND.wav [GAP_MS] [barrier-ms=N] [barrier-wait=at-commit] [hold-end-ms=N] | ... PRIVATE_PROVIDER FIRST.wav continue-during-output SECOND.wav [GAP_MS] [barrier-ms=N] [barrier-wait=at-commit] [hold-end-ms=N] | ... PRIVATE_PROVIDER QUESTION.wav repeat-before-output [GAP_MS] [barrier-ms=N] [barrier-wait=at-commit] [hold-end-ms=N]".into()),
    };
    // Before output the gap follows the first End. During output it follows
    // the first audio actually received, and is nothing unless asked for.
    let gap = gap
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(if name == "continue-during-output" {
            0
        } else {
            300
        });
    if gap > 5_000 {
        return Err("gap must be at most 5000 ms".into());
    }
    let first = pcm(&bounded_regular_file(
        Path::new(first_path),
        MAX_SAMPLES * 2 + 65_536,
    )?)?;
    let second = pcm(&bounded_regular_file(
        Path::new(second_path),
        MAX_SAMPLES * 2 + 65_536,
    )?)?;
    if name != "repeat-before-output" && first == second {
        return Err("continuation must contain distinct PCM; use repeat-before-output for a repeated question".into());
    }
    let config = config(session(Path::new(provider))?, quiet, wait)?;
    let connecting = Instant::now();
    let mut result = match lamp_gemini::connect(config, SessionId::new(1)?).await {
        Ok(connection) => {
            trial_holding(
                connection,
                &first,
                &second,
                Duration::from_millis(gap),
                name,
                hold,
            )
            .await
        }
        Err(error) => {
            json!({"probe":name,"outcome":"setup_failed","error":format!("{error:?}"),"inputs":[],"events":[]})
        }
    };
    result["barrier_wait"] = json!(match wait {
        BarrierWait::FromInterruption => "from_interruption",
        BarrierWait::AtCommit => "at_commit",
    });
    // `at_commit` is an opt-in setting. A pass under it qualifies that
    // setting and says nothing about the default.
    result["barrier_wait_is_default"] = json!(wait == BarrierWait::FromInterruption);
    result["dropped_output_counts_available"] = json!(true);
    result["barrier_ms"] = json!(quiet.as_millis());
    // A bound longer than production's watches for a late terminal. It
    // authorizes nothing: the policy is still `Require`.
    result["barrier_bound_is_production"] = json!(quiet == BARRIER);
    // So does a held End: the run is not what a caller would do.
    result["end_hold_ms"] = json!(hold.as_millis());
    result["absolute_barrier_bound_ms"] = json!(Timeouts::default().response.as_millis());
    result["observation_only"] = json!(quiet != BARRIER || !hold.is_zero());
    result["total_observed_ms"] = json!(connecting.elapsed().as_millis());
    result["source_files"] = json!([first_path, second_path]);
    result["input_transform"] = json!("none; exact mono16k PCM16");
    result["source_sha256_verified_by_probe"] = json!(false);
    result["instruction"] =
        json!("generic short-answer probe instruction; not production CHAT_INSTRUCTION");
    Ok(result)
}

#[cfg(test)]
#[path = "strict_tests.rs"]
mod tests;
