//! Core-side supervisor for one end-of-turn worker process at a time.
//!
//! Every caller-facing method takes one short lock and returns: nothing here
//! waits for model loading, inference, pipe transfer or process exit. Three
//! helper threads per worker do the blocking work:
//!
//! - the writer copies the one queued request into the worker's stdin;
//! - the reader validates answers from its stdout;
//! - the supervisor owns the `Child`, enforces the startup and hang deadlines,
//!   kills a failed worker and reaps it before any replacement starts.
//!
//! A worker that fails before `Ready` (missing or wrong model, wrong build) is
//! not restarted. One that fails while serving is replaced at most
//! [`Limits::restarts`] times, each only after its predecessor was reaped.
use crate::{
    Availability, Classifier, Outcome, Submit, Token, Verdict,
    frame::{self, PROTOCOL, Request, Response},
};
use lamp_ipc::monotonic_us;
use std::{
    ffi::OsString,
    io::{self, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct WorkerCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// The worker starts with an empty environment plus these entries.
    pub env: Vec<(OsString, OsString)>,
}

impl WorkerCommand {
    /// The `lamp-endpoint-worker` executable serving a pinned model file. On
    /// Linux the worker is killed if this process exits first.
    pub fn smart_turn(program: PathBuf, model: PathBuf) -> Self {
        Self {
            program,
            args: vec![
                "--model".into(),
                model.into(),
                "--parent".into(),
                std::process::id().to_string().into(),
            ],
            env: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Spawn until `Ready`, including model verification and a cold inference.
    pub startup: Duration,
    /// A request outstanding this long marks the worker hung; it is killed.
    pub hang: Duration,
    /// Replacements after a serving worker fails. Startup failures are final.
    pub restarts: u8,
    /// Delay before a replacement starts.
    pub backoff: Duration,
    /// After its input closes or it reports an error, time for the worker to
    /// exit on its own before it is killed.
    pub grace: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            startup: Duration::from_secs(10),
            hang: Duration::from_secs(2),
            restarts: 2,
            backoff: Duration::from_secs(1),
            grace: Duration::from_millis(300),
        }
    }
}

/// Lifetime diagnostics for traces. Nothing here grants authority.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Current or last worker incarnation, starting at 1.
    pub worker: u64,
    pub pid: Option<u32>,
    pub spawned: u64,
    pub ready: u64,
    /// The latest worker's own load and cold inference, and spawn to `Ready`
    /// as the core observed it.
    pub load_us: Option<u64>,
    pub cold_us: Option<u64>,
    pub ready_after_us: Option<u64>,
    pub model_sha256: Option<String>,
    pub requests: u64,
    pub verdicts: u64,
    pub errors: u64,
    /// Answers to cancelled requests, never delivered.
    pub discarded: u64,
    /// Requests lost with a failed worker.
    pub lost: u64,
    pub kills: u64,
    pub last_exit: Option<String>,
    pub last_failure: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Starting,
    Ready,
    Recovering(String),
    Failed(String),
    Closed,
}

struct InFlight {
    request: u64,
    token: Token,
    worker: u64,
    submitted_at: Instant,
    submitted_at_us: u64,
    snapshot_us: u64,
    /// Taken by the writer: the worker may hold this audio.
    sent: bool,
    cancelled: bool,
}

struct Fault {
    reason: String,
    /// The worker announced its own exit (error or closed output).
    exiting: bool,
    /// No replacement may follow.
    fatal: bool,
}

struct State {
    phase: Phase,
    worker: u64,
    next_request: u64,
    queued: Option<Vec<u8>>,
    in_flight: Option<InFlight>,
    result: Option<Outcome>,
    fault: Option<Fault>,
    closing: bool,
    finished: bool,
    /// The latest worker whose reader has seen the end of its output.
    drained: u64,
    report: Report,
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

    fn wait_for<'a>(&self, state: MutexGuard<'a, State>, limit: Duration) -> MutexGuard<'a, State> {
        self.wake
            .wait_timeout(state, limit)
            .unwrap_or_else(|poison| poison.into_inner())
            .0
    }
}

impl Shared {
    fn wait_finished(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        let mut state = self.lock();
        while !state.finished {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            state = self.wait_for(state, deadline - now);
        }
        true
    }

    fn availability(&self) -> Availability {
        let state = self.lock();
        match (&state.phase, &state.fault) {
            (Phase::Ready, Some(fault)) => Availability::Recovering(fault.reason.clone()),
            (Phase::Starting, _) => Availability::Loading,
            (Phase::Ready, None) => Availability::Ready,
            (Phase::Recovering(reason), _) => Availability::Recovering(reason.clone()),
            (Phase::Failed(reason), _) => Availability::Failed(reason.clone()),
            (Phase::Closed, _) => Availability::Closed,
        }
    }
}

impl State {
    /// Records the first fault of the current worker and wakes its threads.
    fn fault(&mut self, worker: u64, reason: String, exiting: bool, fatal: bool) {
        if self.worker == worker && self.fault.is_none() {
            self.report.last_failure = Some(reason.clone());
            self.fault = Some(Fault {
                reason,
                exiting,
                fatal: fatal || self.phase == Phase::Starting,
            });
        }
    }
}

pub struct ProcessClassifier {
    shared: Arc<Shared>,
}

/// Read-only view of a classifier for traces. It cannot submit, cancel or
/// close, and it does not keep the worker alive.
#[derive(Clone)]
pub struct Monitor {
    shared: Arc<Shared>,
}

impl Monitor {
    pub fn report(&self) -> Report {
        self.shared.lock().report.clone()
    }

    pub fn availability(&self) -> Availability {
        self.shared.availability()
    }

    /// For runtime teardown only: waits at most `limit` for the classifier's
    /// last worker to be reaped after it was closed. Returns whether it was.
    pub fn wait_finished(&self, limit: Duration) -> bool {
        self.shared.wait_finished(limit)
    }
}

impl ProcessClassifier {
    /// Returns at once in `Loading`; the worker starts on a supervisor thread.
    pub fn spawn(command: WorkerCommand, limits: Limits) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                phase: Phase::Starting,
                worker: 0,
                next_request: 0,
                queued: None,
                in_flight: None,
                result: None,
                fault: None,
                closing: false,
                finished: false,
                drained: 0,
                report: Report::default(),
            }),
            wake: Condvar::new(),
        });
        let supervisor = shared.clone();
        if let Err(error) = thread::Builder::new()
            .name("endpoint-supervisor".into())
            .spawn(move || supervise(&supervisor, &command, limits))
        {
            let mut state = shared.lock();
            state.phase = Phase::Failed(format!("supervisor thread: {error}"));
            state.finished = true;
        }
        Self { shared }
    }

    pub fn report(&self) -> Report {
        self.shared.lock().report.clone()
    }

    pub fn monitor(&self) -> Monitor {
        Monitor {
            shared: self.shared.clone(),
        }
    }

    /// Starts an orderly stop and returns at once: the worker's input closes,
    /// it gets the grace period to exit, then it is killed and reaped.
    pub fn close(&mut self) {
        let mut state = self.shared.lock();
        if !state.closing {
            state.closing = true;
            state.phase = Phase::Closed;
            state.queued = None;
            state.result = None;
            self.shared.wake.notify_all();
        }
    }

    /// For final teardown and tests only: waits at most `limit` for the last
    /// worker to be reaped. Returns whether it was.
    pub fn wait_finished(&self, limit: Duration) -> bool {
        self.shared.wait_finished(limit)
    }
}

impl Drop for ProcessClassifier {
    /// Never waits; the supervisor thread finishes the stop on its own.
    fn drop(&mut self) {
        self.close();
    }
}

fn elapsed_us(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_micros()).unwrap_or(u64::MAX)
}

impl Classifier for ProcessClassifier {
    fn submit(&mut self, token: Token, pcm: &[i16]) -> Submit {
        let started = Instant::now();
        let request = {
            let mut state = self.shared.lock();
            if state.phase != Phase::Ready || state.fault.is_some() || state.closing {
                return Submit::Unavailable;
            }
            if state.in_flight.is_some() {
                return Submit::Busy;
            }
            state.next_request += 1;
            state.next_request
        };
        // Only this method creates requests, and it holds `&mut self`.
        let submitted_at_us = monotonic_us();
        let encoded = frame::encode_request(
            &Request {
                request,
                token,
                submitted_at_us,
            },
            pcm,
        );
        let mut state = self.shared.lock();
        if state.phase != Phase::Ready || state.fault.is_some() || state.closing {
            return Submit::Unavailable;
        }
        state.in_flight = Some(InFlight {
            request,
            token,
            worker: state.worker,
            submitted_at: started,
            submitted_at_us,
            snapshot_us: elapsed_us(started),
            sent: false,
            cancelled: false,
        });
        state.queued = Some(encoded);
        state.report.requests += 1;
        self.shared.wake.notify_all();
        Submit::Accepted
    }

    fn poll(&mut self) -> Option<Outcome> {
        self.shared.lock().result.take()
    }

    fn availability(&self) -> Availability {
        self.shared.availability()
    }

    fn cancel(&mut self) {
        let mut state = self.shared.lock();
        state.queued = None;
        state.result = None;
        match state.in_flight.as_mut() {
            Some(in_flight) if in_flight.sent => in_flight.cancelled = true,
            Some(_) => state.in_flight = None,
            None => {}
        }
    }
}

fn write_requests(shared: &Shared, worker: u64, mut stdin: ChildStdin) {
    loop {
        let frame = {
            let mut state = shared.lock();
            loop {
                if state.closing || state.worker != worker || state.fault.is_some() {
                    // Dropping stdin closes the worker's input.
                    return;
                }
                if let Some(frame) = state.queued.take() {
                    if let Some(in_flight) = state.in_flight.as_mut() {
                        in_flight.sent = true;
                    }
                    break frame;
                }
                state = shared.wait_for(state, Duration::from_secs(1));
            }
        };
        let written = stdin.write_all(&frame).and_then(|()| stdin.flush());
        drop(frame);
        if let Err(error) = written {
            let mut state = shared.lock();
            state.fault(
                worker,
                format!("request write failed: {error}"),
                false,
                false,
            );
            shared.wake.notify_all();
            return;
        }
    }
}

fn read_responses(shared: &Shared, worker: u64, stdout: ChildStdout, spawned_at: Instant) {
    read_until_stop(shared, worker, stdout, spawned_at);
    let mut state = shared.lock();
    state.drained = state.drained.max(worker);
    shared.wake.notify_all();
}

fn read_until_stop(shared: &Shared, worker: u64, stdout: ChildStdout, spawned_at: Instant) {
    let mut input = BufReader::new(stdout);
    loop {
        let response = frame::read_response(&mut input);
        let received_at_us = monotonic_us();
        let mut state = shared.lock();
        if state.worker != worker {
            return;
        }
        let fault = match response {
            Ok(None) => Some(("worker closed its output".to_owned(), true)),
            Err(error) => Some((format!("malformed answer: {error}"), false)),
            Ok(Some(Response::Ready {
                protocol,
                load_us,
                cold_us,
                model_sha256,
            })) => {
                if state.phase != Phase::Starting || state.fault.is_some() {
                    Some(("unexpected ready".to_owned(), false))
                } else if protocol != PROTOCOL {
                    Some((
                        format!("worker protocol {protocol}, expected {PROTOCOL}"),
                        false,
                    ))
                } else {
                    state.phase = Phase::Ready;
                    let report = &mut state.report;
                    report.ready += 1;
                    report.load_us = Some(load_us);
                    report.cold_us = Some(cold_us);
                    report.ready_after_us = Some(elapsed_us(spawned_at));
                    report.model_sha256 =
                        Some(model_sha256.iter().map(|b| format!("{b:02x}")).collect());
                    None
                }
            }
            Ok(Some(Response::Verdict {
                request,
                probability,
                received_at_us: worker_received,
                features_us,
                inference_us,
                sent_at_us,
            })) => match state.in_flight.take() {
                Some(in_flight) if in_flight.request == request && in_flight.sent => {
                    state.report.verdicts += 1;
                    if in_flight.cancelled {
                        state.report.discarded += 1;
                    } else {
                        let ready_at_us = in_flight.submitted_at_us + in_flight.snapshot_us;
                        state.result = Some(Outcome {
                            token: in_flight.token,
                            worker: in_flight.worker,
                            request,
                            result: Ok(Verdict {
                                probability,
                                snapshot_us: in_flight.snapshot_us,
                                delivery_us: worker_received.saturating_sub(ready_at_us),
                                features_us,
                                inference_us,
                                return_us: received_at_us.saturating_sub(sent_at_us),
                                round_trip_us: elapsed_us(in_flight.submitted_at),
                            }),
                        });
                    }
                    None
                }
                other => {
                    state.in_flight = other;
                    Some((format!("answer to unknown request {request}"), false))
                }
            },
            Ok(Some(Response::Error { request, reason })) => {
                state.report.errors += 1;
                if let Some(in_flight) = state.in_flight.take_if(|f| f.request == request && f.sent)
                    && !in_flight.cancelled
                {
                    state.result = Some(Outcome {
                        token: in_flight.token,
                        worker: in_flight.worker,
                        request,
                        result: Err(reason.clone()),
                    });
                }
                Some((reason, true))
            }
        };
        if let Some((reason, exiting)) = fault {
            if !state.closing {
                state.fault(worker, reason, exiting, false);
            }
            shared.wake.notify_all();
            return;
        }
        shared.wake.notify_all();
    }
}

/// Watches one worker until it exits, killing it when required. Always reaps.
fn monitor(
    shared: &Shared,
    worker: u64,
    child: &mut Child,
    spawned_at: Instant,
    limits: Limits,
) -> String {
    let mut exit_by: Option<Instant> = None;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.to_string(),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                return match child.wait() {
                    Ok(status) => format!("{status} after a status error: {error}"),
                    Err(wait) => format!("unreaped after status error {error}: {wait}"),
                };
            }
        }
        let mut state = shared.lock();
        let now = Instant::now();
        if !state.closing && state.fault.is_none() {
            if state.phase == Phase::Starting && now - spawned_at > limits.startup {
                let reason = format!("not ready within {} ms", limits.startup.as_millis());
                state.fault(worker, reason, false, true);
            } else if state
                .in_flight
                .as_ref()
                .is_some_and(|f| f.sent && now - f.submitted_at > limits.hang)
            {
                let reason = format!("no answer within {} ms", limits.hang.as_millis());
                state.fault(worker, reason, false, false);
            }
            if state.fault.is_some() {
                shared.wake.notify_all();
            }
        }
        let kill = if state.closing || state.fault.as_ref().is_some_and(|f| f.exiting) {
            now >= *exit_by.get_or_insert(now + limits.grace)
        } else {
            state.fault.is_some()
        };
        if kill {
            state.report.kills += 1;
            drop(state);
            let killed = child.kill();
            // The supervisor alone reaps, so this PID cannot have been reused.
            return match (child.wait(), killed) {
                (Ok(status), _) => format!("killed: {status}"),
                (Err(wait), Ok(())) => format!("killed but unreaped: {wait}"),
                (Err(wait), Err(kill)) => format!("kill failed: {kill}; unreaped: {wait}"),
            };
        }
        drop(shared.wait_for(state, Duration::from_millis(10)));
    }
}

fn start_io(
    shared: &Arc<Shared>,
    worker: u64,
    stdin: ChildStdin,
    stdout: ChildStdout,
    spawned_at: Instant,
) -> io::Result<()> {
    let writer = shared.clone();
    thread::Builder::new()
        .name("endpoint-writer".into())
        .spawn(move || write_requests(&writer, worker, stdin))?;
    let reader = shared.clone();
    thread::Builder::new()
        .name("endpoint-reader".into())
        .spawn(move || read_responses(&reader, worker, stdout, spawned_at))?;
    Ok(())
}

fn start(command: &WorkerCommand) -> io::Result<Child> {
    Command::new(&command.program)
        .args(&command.args)
        .env_clear()
        .envs(command.env.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
}

fn supervise(shared: &Arc<Shared>, command: &WorkerCommand, limits: Limits) {
    let mut restarts = 0u8;
    loop {
        if shared.lock().closing {
            break;
        }
        let spawned_at = Instant::now();
        let mut child = match start(command) {
            Ok(child) => child,
            Err(error) => {
                let mut state = shared.lock();
                let reason = format!("cannot start {}: {error}", command.program.display());
                state.report.last_failure = Some(reason.clone());
                if !state.closing {
                    state.phase = Phase::Failed(reason);
                }
                break;
            }
        };
        let worker = {
            let mut state = shared.lock();
            state.worker += 1;
            state.fault = None;
            if !state.closing {
                state.phase = Phase::Starting;
            }
            state.report.worker = state.worker;
            state.report.spawned += 1;
            state.report.pid = Some(child.id());
            state.worker
        };
        let io = match (child.stdin.take(), child.stdout.take()) {
            (Some(stdin), Some(stdout)) => start_io(shared, worker, stdin, stdout, spawned_at),
            _ => Err(io::Error::other("worker pipes missing")),
        };
        if let Err(error) = io {
            let mut state = shared.lock();
            state.fault(worker, format!("worker threads: {error}"), false, true);
        }
        let exit = monitor(shared, worker, &mut child, spawned_at, limits);
        let mut state = shared.lock();
        // Let the reader record a final error the worker wrote before exiting.
        let drain_by = Instant::now() + limits.grace;
        while state.drained < worker && Instant::now() < drain_by {
            state = shared.wait_for(state, drain_by - Instant::now());
        }
        state.report.last_exit = Some(exit.clone());
        state.queued = None;
        if let Some(in_flight) = state.in_flight.take() {
            state.report.lost += 1;
            if !in_flight.cancelled && !state.closing {
                state.result = Some(Outcome {
                    token: in_flight.token,
                    worker,
                    request: in_flight.request,
                    result: Err(format!("worker exited: {exit}")),
                });
            }
        }
        if state.closing {
            break;
        }
        let fault = state.fault.take();
        let reason = fault
            .as_ref()
            .map_or_else(|| format!("worker exited: {exit}"), |f| f.reason.clone());
        state.report.last_failure = Some(reason.clone());
        // A worker that never became ready would fail the same way again.
        if state.phase == Phase::Starting || fault.is_some_and(|f| f.fatal) {
            state.phase = Phase::Failed(reason);
            break;
        }
        if restarts >= limits.restarts {
            state.phase = Phase::Failed(format!("{reason}; no replacement left"));
            break;
        }
        restarts += 1;
        state.phase = Phase::Recovering(reason);
        shared.wake.notify_all();
        let resume_at = Instant::now() + limits.backoff;
        while !state.closing {
            let now = Instant::now();
            if now >= resume_at {
                break;
            }
            state = shared.wait_for(state, resume_at - now);
        }
    }
    let mut state = shared.lock();
    state.finished = true;
    shared.wake.notify_all();
}
