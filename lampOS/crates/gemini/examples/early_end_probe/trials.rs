//! The trials of the early-end probe (second-P1, #723), on any connector:
//! the live service through `Dialer` on a lamp, a scripted service in tests.
//!
//! Each trial is one new session: a first request that is answered in full
//! (so that the service gives a resumption handle), then a piece of speech
//! ended early, as the voice's `--early-end-ms` ends it, and then what the
//! person "goes on to say", sent one of three ways:
//!
//! - `control`: the piece is answered in full first, as a request of its own;
//! - `resume`, `resume-late`, `whole-resumed`: the session is given up and
//!   resumed from the handle the service gave before the piece, right after
//!   the early end or after the piece's whole answer;
//! - `whole-labelled`: on the same session, with a written note that the
//!   request before was cut off and this is the whole of it.
//!
//! Nothing here prints a handle, a credential or the person's own words: a
//! trial's row holds timings and the service's text.
use lamp_gemini::{Connect, Connection, Event, RequestId, ResumptionPoint, SessionId};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::Instant;

pub type Outcome<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// The recorded utterances, 16 kHz mono, by what each one says.
pub struct Speech {
    /// "Please remember the word orange."
    pub seed: Vec<i16>,
    /// "Also remember the word purple." Ended early.
    pub piece: Vec<i16>,
    /// "Which words did I ask you to remember?"
    pub check: Vec<i16>,
    /// "What is the capital of", the first half of a question. Ended early.
    pub half: Vec<i16>,
    /// "What is the capital of Japan?"
    pub whole: Vec<i16>,
}

/// The files a speech directory holds, in the order of [`Speech`].
pub const FILES: [&str; 5] = [
    "seed.wav",
    "piece.wav",
    "check.wav",
    "half.wav",
    "whole.wav",
];
pub const TRIALS: [&str; 5] = [
    "control",
    "resume",
    "resume-late",
    "whole-resumed",
    "whole-labelled",
];
/// No more requests than this in a run (the first builder's bound).
pub const MAX_REQUESTS: u32 = 20;
/// Every trial makes three.
const TRIAL_REQUESTS: u32 = 3;
/// A run ends within this, whatever the service does (the bound is two
/// minutes; this leaves room to start and to print).
pub const DEADLINE: Duration = Duration::from_secs(115);
/// After an early end, how long the service is let go before the person's
/// speech goes on: long enough for an answer to begin, as it began at
/// 13:45 on lamp-4ace with `--early-end-ms 200`.
pub const EARLY_WINDOW: Duration = Duration::from_millis(1_000);
/// After the piece's whole answer, how long before resuming (`resume-late`):
/// time for the service to give a newer handle than the one resumed from.
pub const LATE_WAIT: Duration = Duration::from_secs(2);
/// The longest one answer is waited for.
pub const ANSWER_WAIT: Duration = Duration::from_secs(15);
/// The longest a handle is waited for after the first answer.
pub const HANDLE_WAIT: Duration = Duration::from_secs(3);
/// The note sent with the whole question after a piece ended early.
pub const NOTE: &str = "The person paused and then went on: the request just before this one was cut off by that pause, and this is the whole of it. Answer only this.";

/// What one request heard back.
#[derive(Default)]
struct Heard {
    /// The service's words for this request.
    text: String,
    /// From the request's end to the first sound of its answer.
    first_audio_ms: Option<u64>,
    /// The answer finished (an idle turn completion for this request).
    complete: bool,
    interrupted: bool,
}

impl Heard {
    fn row(&self) -> Value {
        json!({"text": self.text, "first_audio_ms": self.first_audio_ms,
            "complete": self.complete, "interrupted": self.interrupted})
    }
}

/// One session and the next request number on it.
struct Talk {
    connection: Connection,
    next: u64,
}

pub struct Probe<'a, C> {
    connector: &'a mut C,
    speech: &'a Speech,
    started: Instant,
    pub requests: u32,
    sessions: u64,
}

impl<'a, C: Connect> Probe<'a, C> {
    pub fn new(connector: &'a mut C, speech: &'a Speech) -> Self {
        Self {
            connector,
            speech,
            started: Instant::now(),
            requests: 0,
            sessions: 0,
        }
    }

    fn left(&self) -> Duration {
        DEADLINE.saturating_sub(self.started.elapsed())
    }

    /// Every trial in turn, each row as soon as it is known.
    pub async fn run(&mut self, mut each: impl FnMut(&Value)) -> Vec<Value> {
        let mut rows = Vec::new();
        for name in TRIALS {
            let row = if self.left().is_zero() || self.requests + TRIAL_REQUESTS > MAX_REQUESTS {
                json!({"trial": name, "skipped": "the run's two minutes or twenty requests"})
            } else {
                match tokio::time::timeout(self.left(), self.trial(name)).await {
                    Ok(Ok(row)) => row,
                    Ok(Err(error)) => json!({"trial": name, "error": error.to_string()}),
                    Err(_) => json!({"trial": name, "error": "the run's two minutes ended"}),
                }
            };
            each(&row);
            rows.push(row);
        }
        rows
    }

    async fn open(&mut self, from: Option<&ResumptionPoint>) -> Outcome<Talk> {
        self.sessions += 1;
        let connection = self
            .connector
            .connect(
                SessionId::new(self.sessions)?,
                from.map(|point| point.handle().clone()),
            )
            .await?;
        Ok(Talk {
            connection,
            next: 1,
        })
    }

    /// Speak `speech` at real time in 10 ms blocks, then end the request,
    /// with `note` written just before the end if there is one.
    async fn say(&mut self, talk: &mut Talk, speech: &[i16], note: Option<&str>) -> Outcome<u64> {
        let id = talk.next;
        talk.next += 1;
        self.requests += 1;
        let request = RequestId::new(id)?;
        let input = talk.connection.input();
        input.try_start(request)?;
        for (sequence, block) in speech.chunks(160).enumerate() {
            input.try_audio(request, sequence as u64, std::time::Instant::now(), block)?;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if let Some(note) = note {
            input.try_note(request, note)?;
        }
        input.try_end(request)?;
        Ok(id)
    }

    /// Listen for `wait`, or until request `id`'s answer is complete when
    /// `whole` is set.
    async fn listen(talk: &mut Talk, id: u64, wait: Duration, whole: bool) -> Heard {
        let since = Instant::now();
        let deadline = since + wait;
        let mut heard = Heard::default();
        while let Ok(Some(event)) =
            tokio::time::timeout_at(deadline, talk.connection.next_event()).await
        {
            match event {
                Event::Audio { lineage, .. } if lineage.request.get() == id => {
                    heard
                        .first_audio_ms
                        .get_or_insert(since.elapsed().as_millis() as u64);
                }
                Event::OutputTranscript { lineage, text, .. } if lineage.request.get() == id => {
                    heard.text.push_str(&text);
                }
                Event::Interrupted { lineage } if lineage.request.get() == id => {
                    heard.interrupted = true;
                }
                Event::TurnComplete {
                    lineage,
                    idle: true,
                } if lineage.request.get() == id => {
                    heard.complete = true;
                    if whole {
                        break;
                    }
                }
                _ => {}
            }
        }
        heard
    }

    /// Ask and wait for the whole answer.
    async fn ask(&mut self, talk: &mut Talk, speech: &[i16], note: Option<&str>) -> Outcome<Heard> {
        let id = self.say(talk, speech, note).await?;
        Ok(Self::listen(talk, id, ANSWER_WAIT, true).await)
    }

    /// The service's current resumption point, waited for while the
    /// session's events are drained.
    async fn handle(talk: &mut Talk) -> Option<ResumptionPoint> {
        let deadline = Instant::now() + HANDLE_WAIT;
        loop {
            if let Some(point) = talk
                .connection
                .resumption()
                .filter(ResumptionPoint::is_current)
            {
                return Some(point);
            }
            let slice = (Instant::now() + Duration::from_millis(100)).min(deadline);
            if tokio::time::timeout_at(slice, talk.connection.next_event())
                .await
                .is_ok_and(|event| event.is_none())
                || Instant::now() >= deadline
            {
                return None;
            }
        }
    }

    async fn trial(&mut self, name: &str) -> Outcome<Value> {
        let speech = self.speech;
        let began = Instant::now();
        let mut first = self.open(None).await?;
        let seed = self.ask(&mut first, &speech.seed, None).await?;
        let before = Self::handle(&mut first).await;
        let mut row = json!({"trial": name, "seed_answer": seed.row(),
            "handle_before_piece": before.is_some()});
        let (piece, rest) = match name {
            "control" | "resume" | "resume-late" => (&speech.piece, &speech.check),
            _ => (&speech.half, &speech.whole),
        };
        if name == "control" {
            // The piece as a request of its own, answered in full.
            row["piece_answer"] = self.ask(&mut first, piece, None).await?.row();
            row["answer"] = self.ask(&mut first, rest, None).await?.row();
        } else {
            let id = self.say(&mut first, piece, None).await?;
            let mut early = Self::listen(&mut first, id, EARLY_WINDOW, false).await;
            if name == "resume-late" {
                // The rest of the piece's answer, unless it is already whole.
                if !early.complete {
                    let more = Self::listen(&mut first, id, ANSWER_WAIT, true).await;
                    early.text.push_str(&more.text);
                    early.complete = more.complete;
                    early.first_audio_ms = early.first_audio_ms.or(more.first_audio_ms);
                }
                Self::listen(&mut first, id, LATE_WAIT, false).await;
            }
            row["piece_answer_before_going_on"] = early.row();
            if name == "whole-labelled" {
                row["answer"] = self.ask(&mut first, rest, Some(NOTE)).await?.row();
            } else {
                first.connection.shutdown();
                drop(first);
                let Some(before) = before else {
                    row["error"] = "no handle was given before the piece".into();
                    return Ok(row);
                };
                let asked = Instant::now();
                match self.open(Some(&before)).await {
                    Ok(mut second) => {
                        row["resumed"] = true.into();
                        row["resume_setup_ms"] = (asked.elapsed().as_millis() as u64).into();
                        row["answer"] = self.ask(&mut second, rest, None).await?.row();
                    }
                    Err(error) => {
                        row["resumed"] = false.into();
                        row["resume_refused"] = error.to_string().into();
                    }
                }
            }
        }
        let answer = row["answer"]["text"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        row["answer_mentions"] = json!({"orange": answer.contains("orange"),
            "purple": answer.contains("purple"), "tokyo": answer.contains("tokyo")});
        row["trial_ms"] = (began.elapsed().as_millis() as u64).into();
        Ok(row)
    }
}

/// What a run would send, without connecting: for `--dry-run`.
pub fn plan(speech: &Speech) -> Value {
    let seconds = |samples: &[i16]| (samples.len() as f64 / 16_000.0 * 100.0).round() / 100.0;
    let say = |what: &str, samples: &[i16]| json!({"say": what, "seconds": seconds(samples), "then": "end the request"});
    let early = |what: &str, samples: &[i16]| {
        json!({"say": what, "seconds": seconds(samples),
            "then": format!("end the request early and listen {} ms", EARLY_WINDOW.as_millis())})
    };
    let resume =
        json!({"connect": "a new session, resumed from the handle given before the piece"});
    let trials: Vec<Value> = TRIALS
        .iter()
        .map(|name| {
            let mut steps = vec![
                json!({"connect": "a new session, asking for resumption handles"}),
                say("seed.wav", &speech.seed),
                json!({"wait": format!("for a current handle, at most {} s", HANDLE_WAIT.as_secs())}),
            ];
            match *name {
                "control" => steps.extend([
                    say("piece.wav", &speech.piece),
                    say("check.wav", &speech.check),
                ]),
                "resume" => steps.extend([
                    early("piece.wav", &speech.piece),
                    resume.clone(),
                    say("check.wav", &speech.check),
                ]),
                "resume-late" => steps.extend([
                    early("piece.wav", &speech.piece),
                    json!({"wait": format!("for the piece's whole answer, then {} s", LATE_WAIT.as_secs())}),
                    resume.clone(),
                    say("check.wav", &speech.check),
                ]),
                "whole-resumed" => steps.extend([
                    early("half.wav", &speech.half),
                    resume.clone(),
                    say("whole.wav", &speech.whole),
                ]),
                _ => steps.extend([
                    early("half.wav", &speech.half),
                    json!({"say": "whole.wav", "seconds": seconds(&speech.whole),
                        "note": NOTE, "then": "end the request"}),
                ]),
            }
            json!({"trial": name, "steps": steps})
        })
        .collect();
    json!({"dry_run": true, "requests": TRIALS.len() as u32 * TRIAL_REQUESTS,
        "at_most_requests": MAX_REQUESTS, "ends_within_s": DEADLINE.as_secs(), "trials": trials})
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
