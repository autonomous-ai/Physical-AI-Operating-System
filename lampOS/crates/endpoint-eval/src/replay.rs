//! Replays fixtures through the production `TurnDetector` with each endpoint
//! profile. `virtual` runs the model in process and releases each verdict a
//! fixed number of capture blocks later (reproducible, emulates a slower
//! device); `paced` feeds blocks in real time to the real worker process.
use crate::{
    Result,
    audio::BLOCK,
    corpus::Clip,
    probe::{self, LEAD_MS},
};
use lamp_endpoint::{FeatureExtractor, SMART_TURN_V3_2, SmartTurn};
use lamp_endpoint_ipc::{
    Availability, Classifier, Limits, Outcome, ProcessClassifier, Submit, Token, Verdict,
    WorkerCommand,
};
use lamp_live::activity::{
    Activity, ConfirmPolicy, EndpointDecision, ObservedAudio, SpeechProbability, TurnDetector,
};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// Named endpoint policies; `None` is the unchanged 1.2 s silence baseline.
pub fn profiles() -> Vec<(&'static str, Option<ConfirmPolicy>)> {
    let parity = ConfirmPolicy::default();
    vec![
        ("baseline", None),
        ("parity", Some(parity)),
        (
            "strict",
            Some(ConfirmPolicy {
                check_blocks: 15,
                commit_blocks: 80,
                complete_probability: 0.99,
                incomplete_probability: 0.0,
                incomplete_blocks: 120,
                ..parity
            }),
        ),
        ("responsive", Some(ConfirmPolicy::responsive())),
        (
            "previous_candidate",
            Some(ConfirmPolicy {
                commit_blocks: 40,
                complete_probability: 0.8,
                incomplete_probability: 0.8,
                incomplete_blocks: 120,
                ..parity
            }),
        ),
    ]
}

#[derive(Clone)]
pub enum Mode {
    Virtual { latency_blocks: u16 },
    Paced { worker: PathBuf },
}

/// Synchronous inference whose verdict becomes visible `latency` polls later.
struct Virtual {
    model: Arc<SmartTurn>,
    extractor: FeatureExtractor,
    latency: u16,
    requests: u64,
    pending: Option<(u16, Outcome)>,
}

impl Classifier for Virtual {
    fn submit(&mut self, token: Token, pcm: &[i16]) -> Submit {
        if self.pending.is_some() {
            return Submit::Busy;
        }
        self.requests += 1;
        let started = Instant::now();
        let features = self.extractor.compute(pcm);
        let features_us = started.elapsed().as_micros() as u64;
        let started = Instant::now();
        let result = self
            .model
            .probability(features)
            .map(|probability| Verdict {
                probability,
                features_us,
                inference_us: started.elapsed().as_micros() as u64,
                ..Verdict::default()
            })
            .map_err(|error| error.to_string());
        let outcome = Outcome {
            token,
            worker: 0,
            request: self.requests,
            result,
        };
        self.pending = Some((self.latency, outcome));
        Submit::Accepted
    }
    fn poll(&mut self) -> Option<Outcome> {
        match self.pending.as_mut() {
            Some((remaining, _)) if *remaining > 1 => {
                *remaining -= 1;
                None
            }
            Some(_) => self.pending.take().map(|(_, outcome)| outcome),
            None => None,
        }
    }
    fn availability(&self) -> Availability {
        Availability::Ready
    }
    fn cancel(&mut self) {
        self.pending = None;
    }
}

pub fn virtual_classifier(model: &Arc<SmartTurn>, latency_blocks: u16) -> Box<dyn Classifier> {
    Box::new(Virtual {
        model: model.clone(),
        extractor: FeatureExtractor::default(),
        latency: latency_blocks,
        requests: 0,
        pending: None,
    })
}

/// Lends one long-lived worker client to successive detectors, as in one
/// session; dropping a detector does not close the shared worker.
#[derive(Clone)]
struct Shared(Arc<Mutex<ProcessClassifier>>);

impl Classifier for Shared {
    fn submit(&mut self, token: Token, pcm: &[i16]) -> Submit {
        self.0.lock().unwrap().submit(token, pcm)
    }
    fn poll(&mut self) -> Option<Outcome> {
        self.0.lock().unwrap().poll()
    }
    fn availability(&self) -> Availability {
        self.0.lock().unwrap().availability()
    }
    fn cancel(&mut self) {
        self.0.lock().unwrap().cancel();
    }
}

#[derive(Debug)]
pub enum Event {
    Start {
        at_block: usize,
    },
    End {
        at_block: usize,
        decision: EndpointDecision,
    },
}

pub fn replay(pcm: &[i16], detector: &mut TurnDetector, paced: bool) -> Result<Vec<Event>> {
    let mut vad = SpeechProbability::default();
    let mut events = Vec::new();
    let started = Instant::now();
    for (index, chunk) in pcm.chunks_exact(BLOCK).enumerate() {
        if paced {
            let due = Duration::from_millis(index as u64 * 10);
            if let Some(wait) = due.checked_sub(started.elapsed()) {
                thread::sleep(wait);
            }
        }
        let samples: [i16; BLOCK] = chunk.try_into()?;
        let probability = vad.analyze(&samples);
        let audio = ObservedAudio {
            sequence: index as u64 + 1,
            captured_at_us: (index as u64 + 1) * 10_000,
            samples,
        };
        match detector.push(audio, probability) {
            Activity::Start(_) => events.push(Event::Start { at_block: index }),
            Activity::End(_) => events.push(Event::End {
                at_block: index,
                decision: detector
                    .last_endpoint()
                    .cloned()
                    .ok_or("End without decision")?,
            }),
            Activity::Fault(reason) => return Err(reason.into()),
            Activity::Quiet | Activity::Continue(_) => {}
        }
    }
    Ok(events)
}

/// A human clip's outcome. Every endpoint followed by a new utterance start
/// split speech the person continued.
fn score_clip(clip: &Clip, profile: &str, events: &[Event]) -> Value {
    let ends: Vec<(usize, &EndpointDecision)> = events
        .iter()
        .filter_map(|event| match event {
            Event::End { at_block, decision } => Some((*at_block, decision)),
            Event::Start { .. } => None,
        })
        .collect();
    let last_start = events.iter().rev().find_map(|event| match event {
        Event::Start { at_block } => Some(*at_block),
        Event::End { .. } => None,
    });
    let splits = ends
        .iter()
        .filter(|(at, _)| last_start.is_some_and(|start| *at < start))
        .map(|(at, decision)| json!({"at_block": at, "silence_ms": decision.silence_ms, "basis": decision.basis, "probability": decision.probability}))
        .collect::<Vec<_>>();
    let last = ends
        .last()
        .filter(|(at, _)| last_start.is_some_and(|start| *at > start));
    json!({
        "id": clip.id, "shard": clip.shard, "partition": clip.partition, "label": clip.label,
        "dataset": clip.dataset, "language": clip.language, "profile": profile,
        "starts": events.iter().filter(|e| matches!(e, Event::Start { .. })).count(),
        "splits": splits,
        "final": last.map(|(at, decision)| json!({"at_block": at, "decision": decision})),
    })
}

fn detector(
    policy: Option<ConfirmPolicy>,
    classifier: Box<dyn Classifier>,
) -> Result<TurnDetector> {
    let detector = TurnDetector::v1_baseline();
    Ok(match policy {
        Some(policy) => detector.with_confirmation(classifier, policy)?,
        None => detector,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_human(
    clips: &[Clip],
    model: &Path,
    mode: &Mode,
    selected: &[String],
    gain_db: f64,
    threads: usize,
    out: &Path,
) -> Result<()> {
    let profiles: Vec<_> = profiles()
        .into_iter()
        .filter(|(name, _)| selected.is_empty() || selected.iter().any(|s| s == name))
        .collect();
    let shared_model = match mode {
        Mode::Virtual { .. } => Some(Arc::new(SmartTurn::load(model, &SMART_TURN_V3_2)?)),
        Mode::Paced { .. } => None,
    };
    let next = AtomicUsize::new(0);
    let rows: Mutex<Vec<(usize, Vec<Value>)>> = Mutex::new(Vec::new());
    let failed: Mutex<Vec<String>> = Mutex::new(Vec::new());
    thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                let make = |policy: Option<ConfirmPolicy>,
                            worker: &Option<Shared>|
                 -> Result<TurnDetector> {
                    let classifier: Box<dyn Classifier> = match (mode, worker) {
                        (Mode::Virtual { latency_blocks }, _) => virtual_classifier(
                            shared_model.as_ref().expect("virtual model"),
                            *latency_blocks,
                        ),
                        (Mode::Paced { .. }, Some(worker)) => Box::new(worker.clone()),
                        (Mode::Paced { .. }, None) => return Err("no worker".into()),
                    };
                    detector(policy, classifier)
                };
                // One worker process per replay thread, reused like one session.
                let worker = match mode {
                    Mode::Paced { worker } => {
                        let client = ProcessClassifier::spawn(
                            WorkerCommand::smart_turn(worker.clone(), model.to_path_buf()),
                            Limits::default(),
                        );
                        let started = Instant::now();
                        while client.availability() == Availability::Loading
                            && started.elapsed() < Duration::from_secs(30)
                        {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Some(Shared(Arc::new(Mutex::new(client))))
                    }
                    Mode::Virtual { .. } => None,
                };
                loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(clip) = clips.get(index) else {
                        return;
                    };
                    let result = probe::compose(clip, gain_db).and_then(|pcm| {
                        profiles
                            .iter()
                            .map(|(name, policy)| {
                                let mut detector = make(*policy, &worker)?;
                                let paced = matches!(mode, Mode::Paced { .. }) && policy.is_some();
                                let events = replay(&pcm, &mut detector, paced)?;
                                Ok(score_clip(clip, name, &events))
                            })
                            .collect::<Result<Vec<_>>>()
                    });
                    match result {
                        Ok(scored) => rows.lock().unwrap().push((index, scored)),
                        Err(error) => failed.lock().unwrap().push(format!("{}: {error}", clip.id)),
                    }
                    if index.is_multiple_of(100) {
                        eprintln!("replay {index}/{}", clips.len());
                    }
                }
            });
        }
    });
    let failed = failed.into_inner()?;
    if !failed.is_empty() {
        return Err(failed.join("\n").into());
    }
    let mut rows = rows.into_inner()?;
    rows.sort_by_key(|(index, _)| *index);
    let mut writer = BufWriter::new(File::create(out)?);
    for (_, scored) in rows {
        for mut row in scored {
            row["lead_blocks"] = json!(LEAD_MS / 10);
            row["gain_db"] = json!(gain_db);
            row["mode"] = match mode {
                Mode::Virtual { latency_blocks } => {
                    json!({"virtual_latency_ms": latency_blocks * 10})
                }
                Mode::Paced { .. } => json!("paced_real_worker"),
            };
            serde_json::to_writer(&mut writer, &row)?;
            writer.write_all(b"\n")?;
        }
    }
    writer.flush()?;
    Ok(())
}
