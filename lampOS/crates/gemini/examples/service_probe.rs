//! Narrow probes of the live service for behavior this transport depends on
//! and that cannot be established offline. Each probe makes a few short
//! requests with one cached spoken question and prints one JSON line of event
//! kinds and timings. It prints no transcript, audio, handle or credential,
//! and it opens no microphone or speaker.
//!
//! ```sh
//! cargo run --locked -p lamp-gemini --example service_probe -- \
//!     /absolute/private/provider.json /path/to/question.wav interrupt-before-output
//! ```
//!
//! The provider file has the fields the runtime uses: `endpoint`, `model`,
//! `voice`, `credential_file`, and optional `authentication`, `thinking_level`
//! and `language_code`. Probes: `interrupt-before-output`, `handles`,
//! `handles-requested`, `resume`, `idle-close`. `fixture` is offline: it only
//! reports what would be spoken from the WAV file.
//!
//! `interrupt-before-output [GAP_MS]` speaks the question, waits `GAP_MS`
//! (300 if absent, at most 5,000) after its end and speaks it again: a person
//! who pauses and resumes before any answer. It reports separately whether
//! the service sent an interruption notice for the cut-off request, whether
//! it sent the idle completion that alone settles the barrier, and whether
//! the probe had to assume. If the second request completes, it listens for
//! eight more seconds and counts what arrives by whether it carried output.
//! Every observed event is printed as well.
//!
//! `image PICTURE.jpg` speaks the question with one JPEG of at most 64 KiB
//! offered as visual context, and reports how long the image took to write
//! and what the model said. It is the one probe that prints the model's own
//! words, because nothing else shows whether the picture was used: choose a
//! picture and a question that are fit to appear in a log.
use lamp_gemini::{
    BarrierPolicy, Connection, Credential, Event, RequestId, Resumption, SessionConfig, SessionId,
    State, ThinkingLevel, Timeouts,
};
use serde::Deserialize;
use std::{
    error::Error,
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    time::{Duration, Instant},
};

type Outcome<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Provider {
    endpoint: String,
    model: String,
    voice: String,
    credential_file: String,
    #[serde(default)]
    authentication: Option<String>,
    #[serde(default)]
    thinking_level: Option<ThinkingLevel>,
    #[serde(default)]
    language_code: Option<String>,
}

/// Regular file, readable by its owner only, of bounded size.
fn private(path: &Path, max: u64) -> Outcome<String> {
    let info = fs::symlink_metadata(path)?;
    if !info.is_file() || info.permissions().mode() & 0o077 != 0 || info.len() > max {
        return Err("configuration must be a bounded regular file with mode 0600".into());
    }
    Ok(fs::read_to_string(path)?)
}

fn session(path: &Path) -> Outcome<SessionConfig> {
    let provider: Provider = serde_json::from_str(&private(path, 16_384)?)
        .map_err(|_| "invalid private provider configuration")?;
    let secret = private(Path::new(&provider.credential_file), 4_098)?;
    let secret = secret.trim_end_matches(['\r', '\n']);
    let credential = match provider.authentication.as_deref() {
        None | Some("api_key") => Credential::api_key(secret)?,
        Some("bearer") => Credential::bearer(secret)?,
        Some("ephemeral") => Credential::ephemeral(secret)?,
        Some(_) => return Err("unknown authentication".into()),
    };
    let mut config = SessionConfig::new(&provider.endpoint, credential)?
        .model(&provider.model)?
        .voice(&provider.voice)?
        .instruction("You are a voice assistant. Answer every question in one short sentence.")?;
    if let Some(level) = provider.thinking_level {
        config = config.thinking_level(level)?;
    }
    if let Some(language) = &provider.language_code {
        config = config.language_code(language)?;
    }
    Ok(config)
}

/// 16 kHz mono from a 16-bit PCM WAV, by linear interpolation. Adequate for a
/// spoken fixture; not a production decoder or resampler.
fn speech(path: &Path) -> Outcome<Vec<i16>> {
    let bytes = fs::read(path)?;
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("speech fixture must be a RIFF WAVE file".into());
    }
    let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let long =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let (mut format, mut data, mut at) = (None, None, 12);
    while at + 8 <= bytes.len() {
        let size = long(at + 4) as usize;
        let body = at + 8;
        let end = body.saturating_add(size).min(bytes.len());
        match &bytes[at..at + 4] {
            b"fmt " if size >= 16 && end - body >= 16 => {
                format = Some((word(body), word(body + 2), long(body + 4), word(body + 14)));
            }
            b"data" => data = Some(&bytes[body..end]),
            _ => {}
        }
        at = end + (size & 1);
    }
    let (Some((kind, channels, rate, bits)), Some(data)) = (format, data) else {
        return Err("speech fixture has no format or data chunk".into());
    };
    // 1 is integer PCM; 0xFFFE is the extensible form of the same.
    if !matches!(kind, 1 | 0xFFFE) || bits != 16 || channels == 0 || rate == 0 {
        return Err("speech fixture must be 16-bit integer PCM".into());
    }
    let channels = usize::from(channels);
    let mono: Vec<f32> = data
        .chunks_exact(2 * channels)
        .map(|frame| {
            frame
                .chunks_exact(2)
                .map(|pair| f32::from(i16::from_le_bytes([pair[0], pair[1]])))
                .sum::<f32>()
                / channels as f32
        })
        .collect();
    if mono.is_empty() {
        return Err("speech fixture holds no audio".into());
    }
    let step = rate as f32 / 16_000.0;
    let length = (mono.len() as f32 / step) as usize;
    Ok((0..length)
        .map(|index| {
            let position = index as f32 * step;
            let (low, fraction) = (position as usize, position.fract());
            let low = low.min(mono.len() - 1);
            let next = mono.get(low + 1).copied().unwrap_or(mono[low]);
            (mono[low] * (1.0 - fraction) + next * fraction) as i16
        })
        .collect())
}

/// Drop leading and trailing silence, keeping 200 ms around the speech, so a
/// padded fixture does not hold the activity open for many silent seconds.
fn trim(speech: &[i16]) -> &[i16] {
    let loud = |sample: &i16| sample.unsigned_abs() > 328; // about -40 dBFS
    let (Some(first), Some(last)) = (speech.iter().position(loud), speech.iter().rposition(loud))
    else {
        return speech;
    };
    &speech[first.saturating_sub(3_200)..(last + 3_200).min(speech.len())]
}

/// Speak the fixture in 10 ms blocks at real time, then optionally end input.
async fn say(connection: &Connection, id: u64, speech: &[i16], end: bool) -> Outcome<()> {
    let request = RequestId::new(id)?;
    let input = connection.input();
    input.try_start(request)?;
    for (sequence, block) in speech.chunks(160).enumerate() {
        input.try_audio(request, sequence as u64, Instant::now(), block)?;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if end {
        input.try_end(request)?;
    }
    Ok(())
}

/// How long the interruption probe keeps listening after the second request
/// has completed.
const AFTER_ANSWER: Duration = Duration::from_secs(8);

/// Whether `request` reached its own idle completion in what was observed.
fn completed(events: &[serde_json::Value], request: u64) -> bool {
    events
        .iter()
        .any(|event| event["kind"] == "turn_complete_idle" && event["request"] == request)
}

/// What an interruption trial established about request 1, cut off by the
/// start of request 2, from the observed events and the connection's drop
/// ledger read at that start, at the end of the observation and after the
/// listening window.
///
/// The barrier of the production policy settles on one thing only: an idle
/// completion of the cut-off request. An interruption notice or a completion
/// that is not idle does not settle it, so each is reported for what it is.
/// Something arriving after the answer is reported as a second answer only
/// if it carried output; a marker with nothing in it is counted apart.
fn score(events: &[serde_json::Value], ledgers: [lamp_gemini::DropLedger; 3]) -> serde_json::Value {
    let [at_resume, at_completion, at_end] = ledgers;
    // From here on the probe was no longer waiting: it had assumed.
    let waited = events
        .iter()
        .position(|event| event["kind"] == "no_confirmation_within_barrier")
        .unwrap_or(events.len());
    let first = |kind: &str| {
        events[..waited]
            .iter()
            .any(|event| event["kind"] == kind && event["request"] == 1)
    };
    let completed = completed(events, 2);
    serde_json::json!({
        "interruption_seen": first("interrupted"),
        "idle_completion_settled": first("turn_complete_idle"),
        "completion_without_idle_seen": first("turn_complete_in_progress"),
        "probe_assumed_after_silence": waited < events.len(),
        "cut_off_request_output_messages":
            at_completion.retired_output - at_resume.retired_output,
        // Output that no request could own while the trial ran: under the
        // probe's policy, what arrived between its assumption and its commit.
        "unowned_output_messages_before_completion":
            at_completion.unowned_output - at_resume.unowned_output,
        "second_request_completed": completed,
        // Absent unless the second request completed: without that there is
        // no "after the answer" to speak of.
        "after_answer": completed.then(|| serde_json::json!({
            "listened_ms": AFTER_ANSWER.as_millis() as u64,
            "output_messages": (at_end.unowned_output - at_completion.unowned_output)
                + (at_end.retired_output - at_completion.retired_output),
            "marker_only_messages": at_end.unowned_markers - at_completion.unowned_markers,
        })),
    })
}

/// The probe's own policy: observe for as long as the transport allows, then
/// assume and say so, so that what follows can be seen at all. Nothing but
/// the probe selects it.
fn observing(config: SessionConfig) -> Outcome<SessionConfig> {
    let timeouts = Timeouts {
        barrier: Duration::from_secs(5),
        ..Timeouts::default()
    };
    Ok(config
        .timeouts(timeouts)?
        .barrier_policy(BarrierPolicy::AssumeAfterQuiet))
}

/// Speak the question, wait `gap` after its end and speak it again on the
/// same connection. Does the service confirm an interruption of a request
/// it has not begun to answer? This is the pause-and-resume case.
async fn interruption_trial(
    connection: &mut Connection,
    speech: &[i16],
    gap: Duration,
) -> Outcome<serde_json::Value> {
    let first = Instant::now();
    say(connection, 1, speech, true).await?;
    // A second start issued at once would overtake the first End
    // locally: the first request would never be committed and the two
    // would be one question. Give the connection time to write the
    // End, and record what the service does meanwhile. A trial in
    // which the first answer has already begun is a different case
    // and is labelled as one.
    let before = observe(connection, first, 0, gap).await;
    let answered_first = before.iter().any(|event| event["kind"] == "first_audio");
    let resumed = Instant::now();
    let at_resume = connection.input().drop_ledger();
    say(connection, 2, speech, true).await?;
    let events = observe(connection, resumed, 2, Duration::from_secs(40)).await;
    // Only this shows the intended state was reached: the second
    // start found the first request committed.
    let committed = events
        .iter()
        .any(|event| event["kind"] == "input_started_after_committed_request");
    let at_completion = connection.input().drop_ledger();
    // A second answer can only be looked for once the first is over.
    let completed = completed(&events, 2);
    let after = if completed {
        observe(connection, resumed, 0, AFTER_ANSWER).await
    } else {
        Vec::new()
    };
    let at_end = connection.input().drop_ledger();
    let mut result = score(&events, [at_resume, at_completion, at_end]);
    result["gap_ms"] = (gap.as_millis() as u64).into();
    result["first_request_committed_before_second_start"] = committed.into();
    result["first_answer_began_before_second_start"] = answered_first.into();
    result["trial"] = match (committed, answered_first) {
        (true, false) => "interrupt_before_output",
        (true, true) => "interrupt_during_output",
        (false, _) => "not_an_interruption",
    }
    .into();
    // Everything observed, whatever the scoring made of it.
    result["before_second_start"] = before.into();
    result["since"] = "second start queued".into();
    result["events"] = events.into();
    result["after_answer_events"] = after.into();
    Ok(result)
}

/// Event kinds for one request with milliseconds since `since`, until the
/// request completes, the connection ends or `wait` passes.
async fn observe(
    connection: &mut Connection,
    since: Instant,
    until_complete: u64,
    wait: Duration,
) -> Vec<serde_json::Value> {
    let mut seen = Vec::new();
    let mut note = |kind: &str, request: Option<u64>| {
        seen.push(serde_json::json!({"kind":kind,"request":request,"ms":since.elapsed().as_millis() as u64}));
    };
    let deadline = tokio::time::Instant::now() + wait;
    let mut first_audio = std::collections::BTreeSet::new();
    loop {
        let Ok(event) = tokio::time::timeout_at(deadline, connection.next_event()).await else {
            note("observation_window_ended", None);
            break;
        };
        match event {
            None => {
                note(&format!("connection_ended:{:?}", connection.state()), None);
                break;
            }
            Some(Event::Audio { lineage, .. }) => {
                if first_audio.insert(lineage.request.get()) {
                    note("first_audio", Some(lineage.request.get()));
                }
            }
            Some(Event::Interrupted { lineage }) => {
                note("interrupted", Some(lineage.request.get()))
            }
            // Written to the wire. `waiting_for_barrier` means an earlier
            // request was already committed when this one started, so this
            // start is a real interruption request.
            Some(Event::InputStarted {
                lineage,
                waiting_for_barrier,
            }) => note(
                if waiting_for_barrier {
                    "input_started_after_committed_request"
                } else {
                    "input_started"
                },
                Some(lineage.request.get()),
            ),
            Some(Event::GenerationComplete { lineage }) => {
                note("generation_complete", Some(lineage.request.get()));
            }
            Some(Event::TurnComplete { lineage, idle }) => {
                let request = lineage.request.get();
                note(
                    if idle {
                        "turn_complete_idle"
                    } else {
                        "turn_complete_in_progress"
                    },
                    Some(request),
                );
                if idle && request == until_complete {
                    break;
                }
            }
            Some(Event::BarrierAssumed {
                superseded,
                successor,
                ..
            }) => {
                note(
                    "no_confirmation_within_barrier",
                    superseded.or(successor).map(RequestId::get),
                );
            }
            Some(Event::Discarded { reason, .. }) => note(&format!("dropped:{reason:?}"), None),
            Some(Event::GoAway { .. }) => note("go_away", None),
            Some(_) => {}
        }
    }
    seen
}

async fn probe(
    config: SessionConfig,
    speech: &[i16],
    name: &str,
    gap: Duration,
) -> Outcome<serde_json::Value> {
    let config = observing(config)?;
    let started = Instant::now();
    Ok(match name {
        // Does the service confirm an interruption of a request it has not
        // begun to answer? This is the pause-and-resume case.
        "interrupt-before-output" => {
            let mut connection =
                lamp_gemini::connect(config.resumption(Resumption::Retain), SessionId::new(1)?)
                    .await?;
            let mut result = interruption_trial(&mut connection, speech, gap).await?;
            result["probe"] = name.into();
            result
        }
        // Are resumption handles issued, unasked or when asked?
        "handles" | "handles-requested" => {
            let mode = if name == "handles" {
                Resumption::Retain
            } else {
                Resumption::Request
            };
            let mut connection =
                lamp_gemini::connect(config.resumption(mode), SessionId::new(1)?).await?;
            let after_setup = connection.resumption().is_some();
            say(&connection, 1, speech, true).await?;
            let events = observe(&mut connection, started, 1, Duration::from_secs(40)).await;
            tokio::time::sleep(Duration::from_secs(2)).await;
            let point = connection.resumption();
            serde_json::json!({"probe":name,"handle_after_setup":after_setup,
                "handle_after_one_answer":point.is_some(),
                "handle_current":point.as_ref().is_some_and(|point| point.is_current()),
                "events":events})
        }
        // Is a handle accepted on a new connection, and does that session answer?
        "resume" => {
            let mut first = lamp_gemini::connect(
                config.clone().resumption(Resumption::Request),
                SessionId::new(1)?,
            )
            .await?;
            say(&first, 1, speech, true).await?;
            observe(&mut first, started, 1, Duration::from_secs(40)).await;
            tokio::time::sleep(Duration::from_secs(2)).await;
            let Some(point) = first.resumption() else {
                return Ok(serde_json::json!({"probe":name,"handle_issued":false}));
            };
            drop(first);
            let resumed = Instant::now();
            let second = lamp_gemini::connect(
                config
                    .resumption(Resumption::Request)
                    .resume(Some(point.handle().clone())),
                SessionId::new(2)?,
            )
            .await;
            let mut second = match second {
                Ok(connection) => connection,
                Err(error) => {
                    return Ok(serde_json::json!({"probe":name,"handle_issued":true,
                        "resumed_setup":format!("{error:?}")}));
                }
            };
            // Connect returns when setup has completed, and this is that moment.
            let resumed_setup_ms = resumed.elapsed().as_millis() as u64;
            let asked = Instant::now();
            say(&second, 1, speech, true).await?;
            let events = observe(&mut second, asked, 1, Duration::from_secs(40)).await;
            serde_json::json!({"probe":name,"handle_issued":true,"resumed_setup":"accepted",
                "resumed_setup_ms":resumed_setup_ms,
                "since":"resumed question start queued","events":events})
        }
        // How long does an unused session live, and how does it end?
        "idle-close" => {
            let mut connection =
                lamp_gemini::connect(config.resumption(Resumption::Retain), SessionId::new(1)?)
                    .await?;
            let events = observe(&mut connection, started, 0, Duration::from_secs(300)).await;
            serde_json::json!({"probe":name,"final_state":format!("{:?}", connection.state()),
                "ready":connection.state() == State::Ready,"events":events})
        }
        _ => return Err("unknown probe".into()),
    })
}

/// A regular file, not reached through a link, of at most `max` bytes. It is
/// opened once: what is checked is the descriptor that is read, and no more
/// than `max + 1` bytes are read from it whatever it becomes meanwhile.
///
/// A name that is swapped for a pipe after the first check would block its
/// open for ever. The open therefore runs on a helper thread with a one
/// second limit; if it has not returned, the probe refuses and exits with
/// that thread still parked. This crate has no dependency that names the
/// platform's nonblocking-open flag, and a probe is not worth adding one.
fn bounded_regular_file(path: &Path, max: usize) -> Outcome<Vec<u8>> {
    use std::{io::Read, os::unix::fs::MetadataExt};
    let named = fs::symlink_metadata(path)?;
    if !named.is_file() {
        return Err("picture must be a regular file, not a link or a device".into());
    }
    let (opened, result) = std::sync::mpsc::channel();
    let target = path.to_owned();
    std::thread::spawn(move || {
        let _ = opened.send(fs::File::open(target));
    });
    let file = result
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| "picture could not be opened within a second")??;
    let info = file.metadata()?;
    if !info.is_file() || (info.dev(), info.ino()) != (named.dev(), named.ino()) {
        return Err("picture changed while it was being opened".into());
    }
    let mut bytes = Vec::new();
    file.take(max as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err("picture must be at most 64 KiB".into());
    }
    Ok(bytes)
}

/// Does the service use a picture sent inside a manual activity, and what
/// does sending one cost the audio behind it on this link?
async fn image_probe(
    config: SessionConfig,
    speech: &[i16],
    picture: &[u8],
) -> Outcome<serde_json::Value> {
    let mut connection =
        lamp_gemini::connect(config.resumption(Resumption::Retain), SessionId::new(1)?).await?;
    let request = RequestId::new(1)?;
    let input = connection.input();
    let started = Instant::now();
    input.try_start(request)?;
    let mut offer = None;
    for (sequence, block) in speech.chunks(160).enumerate() {
        input.try_audio(request, sequence as u64, Instant::now(), block)?;
        // Once the question is under way, as a frame would arrive in use.
        if sequence == 20 {
            offer = Some(format!(
                "{:?}",
                input.try_image(request, 1, 1, Instant::now(), picture)
            ));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    input.try_end(request)?;
    let ended = Instant::now();
    let (mut events, mut answer) = (Vec::new(), String::new());
    let mut note = |kind: &str| {
        events.push(serde_json::json!({"kind":kind,"ms":ended.elapsed().as_millis() as u64}));
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(40);
    let mut speaking = false;
    loop {
        match tokio::time::timeout_at(deadline, connection.next_event()).await {
            Err(_) => {
                note("observation_window_ended");
                break;
            }
            Ok(None) => {
                note(&format!("connection_ended:{:?}", connection.state()));
                break;
            }
            Ok(Some(Event::Audio { .. })) if !speaking => {
                speaking = true;
                note("first_audio");
            }
            Ok(Some(Event::OutputTranscript { text, .. })) => answer.push_str(&text),
            Ok(Some(Event::GenerationComplete { .. })) => note("generation_complete"),
            Ok(Some(Event::TurnComplete { idle: true, .. })) => {
                note("turn_complete_idle");
                break;
            }
            Ok(Some(_)) => {}
        }
    }
    let ledger = input.image_ledger();
    Ok(serde_json::json!({
        "probe": "image",
        // The End was accepted locally here. It reaches the wire after the
        // image, if the image was still being written.
        "since": "end queued locally",
        "picture_bytes": picture.len(),
        "offer": offer,
        "sent": ledger.sent,
        "dropped": ledger.dropped,
        "image_written_in_ms": ledger.last_sent.map(|receipt| receipt.written_in.as_secs_f64() * 1e3),
        "image_waited_ms": ledger.last_sent.map(|receipt| receipt.waited.as_secs_f64() * 1e3),
        "question_ms": ended.duration_since(started).as_millis() as u64,
        "events": events,
        "answer": answer,
    }))
}

fn main() -> Outcome<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Outcome<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: service_probe PRIVATE_PROVIDER.json QUESTION.wav PROBE | ... image PICTURE.jpg | ... interrupt-before-output GAP_MS";
    let (provider, wav, name, picture, gap) = match arguments.as_slice() {
        [provider, wav, name] if name != "image" => (provider, wav, name, None, 300),
        [provider, wav, name, picture] if name == "image" => {
            (provider, wav, name, Some(picture), 300)
        }
        [provider, wav, name, gap] if name == "interrupt-before-output" => {
            let gap = gap.parse::<u64>().ok().filter(|gap| *gap <= 5_000);
            (provider, wav, name, None, gap.ok_or(usage)?)
        }
        _ => return Err(usage.into()),
    };
    let decoded = speech(Path::new(wav))?;
    let speech = trim(&decoded);
    if speech.len() < 8_000 || speech.len() > 16_000 * 30 {
        return Err("speech fixture must hold between 0.5 and 30 seconds of speech".into());
    }
    if name == "fixture" {
        // Offline: what would be spoken. No provider file is read.
        println!(
            "{}",
            serde_json::json!({"probe":name,"file_seconds":decoded.len() as f64 / 16_000.0,
                "spoken_seconds":speech.len() as f64 / 16_000.0,"blocks":speech.len().div_ceil(160)})
        );
        return Ok(());
    }
    let result = match picture {
        Some(picture) => {
            // Read before any connection is made.
            let picture = bounded_regular_file(Path::new(picture), lamp_gemini::MAX_IMAGE_BYTES)?;
            image_probe(session(Path::new(provider))?, speech, &picture).await?
        }
        None => {
            probe(
                session(Path::new(provider))?,
                speech,
                name,
                Duration::from_millis(gap),
            )
            .await?
        }
    };
    println!("{result}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_gemini::DropLedger;
    use serde_json::{Value, json};

    fn seen(kind: &str, request: Option<u64>) -> Value {
        json!({"kind":kind,"request":request,"ms":0})
    }
    fn none() -> [DropLedger; 3] {
        [DropLedger::default(); 3]
    }
    /// The second request, answered and completed.
    fn answered() -> Vec<Value> {
        vec![
            seen("first_audio", Some(2)),
            seen("generation_complete", Some(2)),
            seen("turn_complete_idle", Some(2)),
        ]
    }

    #[test]
    fn an_interruption_notice_alone_does_not_settle_the_barrier() {
        let mut events = vec![
            seen("input_started_after_committed_request", Some(2)),
            seen("interrupted", Some(1)),
            seen("no_confirmation_within_barrier", Some(1)),
        ];
        events.extend(answered());
        let score = score(&events, none());
        assert_eq!(score["interruption_seen"], true);
        assert_eq!(score["idle_completion_settled"], false);
        assert_eq!(score["probe_assumed_after_silence"], true);
    }

    #[test]
    fn a_completion_that_is_not_idle_does_not_settle_the_barrier() {
        let mut events = vec![
            seen("turn_complete_in_progress", Some(1)),
            seen("no_confirmation_within_barrier", Some(1)),
        ];
        events.extend(answered());
        let score = score(&events, none());
        assert_eq!(score["completion_without_idle_seen"], true);
        assert_eq!(score["idle_completion_settled"], false);
    }

    #[test]
    fn an_idle_completion_of_the_cut_off_request_settles_it() {
        let mut events = vec![
            seen("interrupted", Some(1)),
            seen("turn_complete_idle", Some(1)),
        ];
        events.extend(answered());
        let score = score(&events, none());
        assert_eq!(score["idle_completion_settled"], true);
        assert_eq!(score["probe_assumed_after_silence"], false);
    }

    #[test]
    fn what_arrives_after_the_probe_assumed_settles_nothing() {
        let mut events = vec![
            seen("no_confirmation_within_barrier", Some(1)),
            seen("interrupted", Some(1)),
            seen("turn_complete_idle", Some(1)),
        ];
        events.extend(answered());
        let score = score(&events, none());
        assert_eq!(score["interruption_seen"], false);
        assert_eq!(score["idle_completion_settled"], false);
    }

    #[test]
    fn the_second_request_s_own_events_say_nothing_about_the_first() {
        let events = answered();
        let score = score(&events, none());
        assert_eq!(score["interruption_seen"], false);
        assert_eq!(score["idle_completion_settled"], false);
        assert_eq!(score["second_request_completed"], true);
    }

    #[test]
    fn nothing_after_the_answer_is_scored_unless_the_second_request_completed() {
        // The window ran out, or the connection ended, with drops on record.
        let events = vec![
            seen("first_audio", Some(2)),
            seen("observation_window_ended", None),
        ];
        let dropped = DropLedger {
            retired_output: 0,
            unowned_output: 7,
            unowned_markers: 2,
        };
        let score = score(&events, [DropLedger::default(), dropped, dropped]);
        assert_eq!(score["second_request_completed"], false);
        assert!(score["after_answer"].is_null());
    }

    #[test]
    fn markers_after_the_answer_are_not_a_second_answer() {
        let at_end = DropLedger {
            unowned_markers: 2,
            ..DropLedger::default()
        };
        let score = score(
            &answered(),
            [DropLedger::default(), DropLedger::default(), at_end],
        );
        assert_eq!(score["after_answer"]["output_messages"], 0);
        assert_eq!(score["after_answer"]["marker_only_messages"], 2);
    }

    // The trial itself against a scripted service, on a paused clock.

    use lamp_gemini::{
        Credential, GOOGLE_ENDPOINT,
        testing::{FakeService, audio, pair},
    };

    fn marker(field: &str) -> Value {
        json!({"serverContent":{field:true}})
    }
    fn idle() -> Value {
        json!({"serverContent":{"turnComplete":true,"interactionStatus":"IDLE"}})
    }
    /// Client messages up to and including the next one that carries `field`.
    async fn until(service: &mut FakeService, field: &str) {
        loop {
            let message = service
                .try_receive(Duration::from_secs(20))
                .await
                .expect("client message");
            if message["realtimeInput"].get(field).is_some() {
                return;
            }
        }
    }
    /// Run the trial with half a second of speech and the device's gap. The
    /// service receives request 1 whole and the start of request 2, then does
    /// what `script` says.
    async fn trial<F, R>(script: F) -> Value
    where
        F: FnOnce(FakeService) -> R + Send + 'static,
        R: Future<Output = ()> + Send + 'static,
    {
        let config = SessionConfig::new(GOOGLE_ENDPOINT, Credential::api_key("test-only").unwrap())
            .unwrap()
            .resumption(Resumption::Retain);
        let (connect, mut service) = pair(
            observing(config).unwrap(),
            SessionId::new(1).unwrap(),
            1 << 20,
        )
        .await;
        let service = tokio::spawn(async move {
            service.accept().await;
            until(&mut service, "activityEnd").await;
            until(&mut service, "activityStart").await;
            script(service).await;
        });
        let mut connection = connect.await.unwrap();
        let result = interruption_trial(&mut connection, &[100; 8_000], Duration::from_millis(241))
            .await
            .unwrap();
        service.await.unwrap();
        result
    }
    /// One answer to whatever was committed last, complete and idle.
    async fn answer(service: &mut FakeService, messages: usize) {
        for _ in 0..messages {
            service.send(audio(9, 480)).await;
        }
        service.send(marker("generationComplete")).await;
        service.send(idle()).await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_silent_service_is_reported_as_assumed_and_one_answer() {
        let result = trial(|mut service| async move {
            // Nothing until the probe gives up waiting and commits.
            until(&mut service, "activityEnd").await;
            answer(&mut service, 3).await;
            service.rest(AFTER_ANSWER + Duration::from_secs(1)).await;
        })
        .await;
        assert_eq!(result["trial"], "interrupt_before_output");
        assert_eq!(result["interruption_seen"], false);
        assert_eq!(result["idle_completion_settled"], false);
        assert_eq!(result["probe_assumed_after_silence"], true);
        assert_eq!(result["second_request_completed"], true);
        assert_eq!(
            result["after_answer"],
            json!({"listened_ms":8000,"output_messages":0,"marker_only_messages":0})
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_interruption_notice_with_no_idle_completion_is_not_reported_as_settled() {
        let result = trial(|mut service| async move {
            service.send(marker("interrupted")).await;
            service
                .send(json!({"serverContent":{"turnComplete":true,"interactionStatus":"IN_PROGRESS"}}))
                .await;
            // Neither is the idle completion, so the End stays withheld
            // until the probe assumes.
            until(&mut service, "activityEnd").await;
            answer(&mut service, 3).await;
            // A late completion with nothing in it is not a second answer.
            service.send(idle()).await;
            service.rest(AFTER_ANSWER + Duration::from_secs(1)).await;
        })
        .await;
        assert_eq!(result["interruption_seen"], true);
        assert_eq!(result["completion_without_idle_seen"], true);
        assert_eq!(result["idle_completion_settled"], false);
        assert_eq!(result["probe_assumed_after_silence"], true);
        assert_eq!(result["second_request_completed"], true);
        assert_eq!(result["after_answer"]["output_messages"], 0);
        assert_eq!(result["after_answer"]["marker_only_messages"], 1);
    }

    #[tokio::test(start_paused = true)]
    async fn an_idle_completion_is_reported_as_settled_and_nothing_is_assumed() {
        let result = trial(|mut service| async move {
            service.send(marker("interrupted")).await;
            service.send(idle()).await;
            until(&mut service, "activityEnd").await;
            answer(&mut service, 3).await;
            service.rest(AFTER_ANSWER + Duration::from_secs(1)).await;
        })
        .await;
        assert_eq!(result["interruption_seen"], true);
        assert_eq!(result["idle_completion_settled"], true);
        assert_eq!(result["probe_assumed_after_silence"], false);
        assert_eq!(result["second_request_completed"], true);
    }

    #[tokio::test(start_paused = true)]
    async fn a_service_that_answers_twice_is_reported_by_its_second_answer_s_output() {
        let result = trial(|mut service| async move {
            // It ignored the interruption: the cut-off request is answered
            // after the probe has assumed and committed, and then the second.
            until(&mut service, "activityEnd").await;
            answer(&mut service, 3).await;
            answer(&mut service, 5).await;
            service.rest(AFTER_ANSWER + Duration::from_secs(1)).await;
        })
        .await;
        assert_eq!(result["probe_assumed_after_silence"], true);
        assert_eq!(result["second_request_completed"], true);
        // Five messages of output, then a generation marker and a completion.
        assert_eq!(result["after_answer"]["output_messages"], 5);
        assert_eq!(result["after_answer"]["marker_only_messages"], 2);
    }

    #[tokio::test(start_paused = true)]
    async fn an_answer_that_never_completes_leaves_the_aftermath_unscored() {
        let result = trial(|mut service| async move {
            until(&mut service, "activityEnd").await;
            service.send(audio(9, 480)).await;
            // No completion: the observation window is all there is.
            service.rest(Duration::from_secs(60)).await;
        })
        .await;
        assert_eq!(result["second_request_completed"], false);
        assert!(result["after_answer"].is_null());
        assert_eq!(result["after_answer_events"], json!([]));
    }

    #[test]
    fn output_after_the_answer_is_counted_as_output() {
        let at_completion = DropLedger {
            retired_output: 3,
            unowned_output: 0,
            unowned_markers: 1,
        };
        let at_end = DropLedger {
            retired_output: 3,
            unowned_output: 12,
            unowned_markers: 2,
        };
        let score = score(&answered(), [DropLedger::default(), at_completion, at_end]);
        // Three messages of the cut-off request's own output arrived while
        // the second request was being spoken: before the answer, not after.
        assert_eq!(score["cut_off_request_output_messages"], 3);
        assert_eq!(score["unowned_output_messages_before_completion"], 0);
        assert_eq!(score["after_answer"]["output_messages"], 12);
        assert_eq!(score["after_answer"]["marker_only_messages"], 1);
    }
}
