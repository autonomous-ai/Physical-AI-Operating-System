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
//! `image PICTURE.jpg` speaks the question with one JPEG of at most 64 KiB
//! offered as visual context, and reports how long the image took to write
//! and what the model said. It is the one probe that prints the model's own
//! words, because nothing else shows whether the picture was used: choose a
//! picture and a question that are fit to appear in a log. With
//! `LAMP_PROBE_INSTRUCTION_FILE` set, the session is told that file's text in
//! place of the one-sentence instruction, as a skill's note would be (Desk
//! tutor's "never guess" check, #941).
//!
//! `continue-before-output SECOND.wav [GAP_MS]` and the distinct
//! `repeat-before-output [GAP_MS]` use strict Require/2 s and drain events
//! while sending. `continue-during-output SECOND.wav [GAP_MS]` is the same
//! trial with the successor started only once the first request's own audio
//! has actually been received, `GAP_MS` after it and at once by default: an
//! interruption of a response that is producing. A trailing `barrier-ms=N`,
//! 2000 to 5000, lengthens the barrier's quiet bound to watch whether a
//! terminal arrives late. That is observation only: the policy is still
//! Require, nothing is assumed, the result says the bound is not
//! production's, and the probe exits non-zero whatever it saw. A trailing
//! `barrier-wait=at-commit`, after `barrier-ms=N` when both are given, runs
//! the trial with the library's opt-in wait; every barrier's end is recorded
//! as a `barrier` row of counts. They print
//! bounded model/input transcript observations for
//! private review; input transcripts remain session-scoped. These modes use
//! exact mono16k PCM16 without trimming. See the session reliability document
//! for their public-API and semantic limits.
use lamp_gemini::{
    BarrierPolicy, Connection, Event, RequestId, Resumption, SessionConfig, SessionId, State,
    Timeouts,
};
use std::{
    error::Error,
    fs,
    path::Path,
    time::{Duration, Instant},
};

#[path = "shared/provider.rs"]
mod provider;
#[path = "service_probe/strict.rs"]
mod strict;

use provider::{session, speech, trim};

type Outcome<T> = Result<T, Box<dyn Error + Send + Sync>>;

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

async fn probe(config: SessionConfig, speech: &[i16], name: &str) -> Outcome<serde_json::Value> {
    let timeouts = Timeouts {
        // Observe for as long as the transport allows before assuming.
        barrier: Duration::from_secs(5),
        ..Timeouts::default()
    };
    let config = config
        .timeouts(timeouts)?
        .barrier_policy(BarrierPolicy::AssumeAfterQuiet);
    let started = Instant::now();
    Ok(match name {
        // Does the service confirm an interruption of a request it has not
        // begun to answer? This is the pause-and-resume case.
        "interrupt-before-output" => {
            let mut connection =
                lamp_gemini::connect(config.resumption(Resumption::Retain), SessionId::new(1)?)
                    .await?;
            let first = Instant::now();
            say(&connection, 1, speech, true).await?;
            // A second start issued at once would overtake the first End
            // locally: the first request would never be committed and the two
            // would be one question. Give the connection time to write the
            // End, and record what the service does meanwhile. A trial in
            // which the first answer has already begun is a different case
            // and is labelled as one.
            let before = observe(&mut connection, first, 0, Duration::from_millis(300)).await;
            let answered_first = before.iter().any(|event| event["kind"] == "first_audio");
            let resumed = Instant::now();
            say(&connection, 2, speech, true).await?;
            let events = observe(&mut connection, resumed, 2, Duration::from_secs(40)).await;
            // Only this shows the intended state was reached: the second
            // start found the first request committed.
            let committed = events
                .iter()
                .any(|event| event["kind"] == "input_started_after_committed_request");
            serde_json::json!({"probe":name,
                "first_request_committed_before_second_start":committed,
                "first_answer_began_before_second_start":answered_first,
                "trial": match (committed, answered_first) {
                    (true, false) => "interrupt_before_output",
                    (true, true) => "interrupt_during_output",
                    (false, _) => "not_an_interruption",
                },
                "before_second_start":before,
                "since":"second start queued","events":events})
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
    if strict::requested(&arguments) {
        let result = strict::run(&arguments).await?;
        println!("{result}");
        if result["observation_only"] == true {
            return Err("observation only: this bound is not production's, so the run passes nothing; see its JSON".into());
        }
        if !strict::passed(&result) {
            return Err("strict probe is failed or inconclusive; see its JSON outcome".into());
        }
        return Ok(());
    }
    let (provider, wav, name, picture) = match arguments.as_slice() {
        [provider, wav, name] if name != "image" => (provider, wav, name, None),
        [provider, wav, name, picture] if name == "image" => (provider, wav, name, Some(picture)),
        _ => {
            return Err(
                "usage: service_probe PRIVATE_PROVIDER.json QUESTION.wav PROBE | ... image PICTURE.jpg"
                    .into(),
            );
        }
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
        None => probe(session(Path::new(provider))?, speech, name).await?,
    };
    println!("{result}");
    Ok(())
}
