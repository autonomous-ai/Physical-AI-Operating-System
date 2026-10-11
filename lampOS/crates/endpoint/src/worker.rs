//! Single-flight Smart Turn inference outside the capture path.
//!
//! `submit` and `poll` never wait for model work: a request is copied into one
//! bounded slot and the result comes back tagged with the caller's token. Loading
//! and failure are visible states so a caller can fall back immediately.
use crate::{FeatureExtractor, ModelAsset, Result, SmartTurn, WINDOW_SAMPLES};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread,
    time::Instant,
};

/// Identifies one check of one pause in one utterance. A result is only
/// meaningful to the caller whose current token is identical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub utterance: u64,
    pub generation: u64,
    pub check: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Verdict {
    pub probability: f32,
    pub features_us: u64,
    pub inference_us: u64,
}

#[derive(Debug)]
pub struct Outcome {
    pub token: Token,
    pub result: std::result::Result<Verdict, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    Loading,
    Ready,
    Failed(String),
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Submit {
    Accepted,
    /// One request is already queued or running; retry later.
    Busy,
    /// Not loaded, failed or closed; use the silence fallback.
    Unavailable,
}

/// An end-of-turn classifier the detector can poll without blocking capture.
pub trait Classifier: Send {
    fn submit(&mut self, token: Token, pcm: &[i16]) -> Submit;
    fn poll(&mut self) -> Option<Outcome>;
    fn availability(&self) -> Availability;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Startup {
    pub load_us: u64,
    /// First inference after loading, run on silence before becoming Ready.
    pub cold_inference_us: u64,
}

struct State {
    availability: Availability,
    request: Option<(Token, Vec<i16>)>,
    busy: bool,
    result: Option<Outcome>,
    startup: Option<Startup>,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        // No code panics while holding this lock; keep serving after a poison.
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

pub struct SmartTurnWorker {
    shared: Arc<Shared>,
}

impl SmartTurnWorker {
    /// Starts loading in the background and returns at once in `Loading`.
    pub fn spawn(path: PathBuf, asset: ModelAsset) -> Result<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                availability: Availability::Loading,
                request: None,
                busy: false,
                result: None,
                startup: None,
            }),
            wake: Condvar::new(),
        });
        let worker = shared.clone();
        thread::Builder::new()
            .name("lamp-smart-turn".into())
            .spawn(move || run(&worker, path, asset))?;
        Ok(Self { shared })
    }

    pub fn startup(&self) -> Option<Startup> {
        self.shared.lock().startup
    }
}

impl Classifier for SmartTurnWorker {
    fn submit(&mut self, token: Token, pcm: &[i16]) -> Submit {
        let mut state = self.shared.lock();
        if state.availability != Availability::Ready {
            return Submit::Unavailable;
        }
        if state.busy {
            return Submit::Busy;
        }
        let pcm = &pcm[pcm.len().saturating_sub(WINDOW_SAMPLES)..];
        state.busy = true;
        state.result = None;
        state.request = Some((token, pcm.to_vec()));
        self.shared.wake.notify_all();
        Submit::Accepted
    }

    fn poll(&mut self) -> Option<Outcome> {
        self.shared.lock().result.take()
    }

    fn availability(&self) -> Availability {
        self.shared.lock().availability.clone()
    }
}

impl Drop for SmartTurnWorker {
    /// Never waits for a running inference; the thread exits after it.
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        state.availability = Availability::Closed;
        state.request = None;
        state.result = None;
        self.shared.wake.notify_all();
    }
}

fn elapsed_us(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_micros()).unwrap_or(u64::MAX)
}

fn classify(
    model: &SmartTurn,
    extractor: &mut FeatureExtractor,
    pcm: &[i16],
) -> std::result::Result<Verdict, String> {
    let started = Instant::now();
    let features = extractor.compute(pcm);
    let features_us = elapsed_us(started);
    let started = Instant::now();
    let probability = model
        .probability(features)
        .map_err(|error| error.to_string())?;
    Ok(Verdict {
        probability,
        features_us,
        inference_us: elapsed_us(started),
    })
}

fn guarded<T>(
    work: impl FnOnce() -> std::result::Result<T, String>,
) -> std::result::Result<T, String> {
    catch_unwind(AssertUnwindSafe(work)).unwrap_or_else(|_| Err("Smart Turn panicked".into()))
}

fn fail(shared: &Shared, reason: String) {
    let mut state = shared.lock();
    if state.availability != Availability::Closed {
        state.availability = Availability::Failed(reason);
    }
    state.busy = false;
    state.request = None;
}

fn run(shared: &Shared, path: PathBuf, asset: ModelAsset) {
    let started = Instant::now();
    let model = match guarded(|| SmartTurn::load(&path, &asset).map_err(|error| error.to_string()))
    {
        Ok(model) => model,
        Err(reason) => return fail(shared, reason),
    };
    let load_us = elapsed_us(started);
    let mut extractor = FeatureExtractor::default();
    let cold = match guarded(|| classify(&model, &mut extractor, &[])) {
        Ok(verdict) => verdict.features_us + verdict.inference_us,
        Err(reason) => return fail(shared, reason),
    };
    {
        let mut state = shared.lock();
        if state.availability == Availability::Closed {
            return;
        }
        state.availability = Availability::Ready;
        state.startup = Some(Startup {
            load_us,
            cold_inference_us: cold,
        });
    }
    loop {
        let (token, pcm) = {
            let mut state = shared.lock();
            loop {
                if state.availability == Availability::Closed {
                    return;
                }
                if let Some(request) = state.request.take() {
                    break request;
                }
                state = shared
                    .wake
                    .wait(state)
                    .unwrap_or_else(|poison| poison.into_inner());
            }
        };
        let result = guarded(|| classify(&model, &mut extractor, &pcm));
        let mut state = shared.lock();
        if state.availability == Availability::Closed {
            return;
        }
        state.busy = false;
        if let Err(reason) = &result {
            state.availability = Availability::Failed(reason.clone());
        }
        state.result = Some(Outcome { token, result });
        if matches!(state.availability, Availability::Failed(_)) {
            return;
        }
    }
}
