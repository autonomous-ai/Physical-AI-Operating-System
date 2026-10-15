//! Explicit coordination for local prepared-player fixture threads.
//!
//! The owner holds the supplied clock still through each round. Workers run
//! the shared prepared fake and park only after doing all work available at
//! that instant. Ready and Grant remain real bounded channels, and delivery
//! remains an actual room datagram. None of these acknowledgements substitutes
//! for the backend observing its owned JoinHandle's completion.
use super::mac::{FakePlayerFault, FakePreparedRoomPlayer, Player, PreparedWait, Preparing};
use crate::{Result, invalid, stimulus::SceneTiming};
use lamp_observer::playback::GrantInput;
use lamp_turn_owner::MonoTime;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, MutexGuard, Weak,
        mpsc::{Receiver, RecvTimeoutError, TryRecvError},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

const CONTROL_BOUND: Duration = Duration::from_secs(30);
const ROOM_BOUND: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Registered,
    Running,
    Parked(u64),
    ReportReady,
}

struct Progress {
    phase: Phase,
    grant_consumed_at: Option<u64>,
    grant_acknowledged: bool,
}

struct State {
    at_us: u64,
    round: u64,
    pumping: bool,
    closed: bool,
    error: Option<String>,
    successful_sends: u64,
    players: BTreeMap<String, Progress>,
}

struct Shared {
    now: Arc<dyn Fn() -> MonoTime + Send + Sync>,
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> Result<MutexGuard<'_, State>> {
        self.state
            .lock()
            .map_err(|_| invalid("prepared fixture state was poisoned"))
    }

    fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.closed = true;
        self.changed.notify_all();
    }
}

// The last controller owner closes the workers even while player factories
// or worker threads still exist. Workers deliberately do not retain this lease.
struct Owner(Weak<Shared>);

impl Drop for Owner {
    fn drop(&mut self) {
        if let Some(shared) = self.0.upgrade() {
            shared.close();
        }
    }
}

/// Parent-owned scheduler for explicitly controlled prepared fake players.
/// Clock reads are pure; only the caller advances that clock between settled
/// rounds. A clone shares the owner lease, not a second independent scheduler.
#[derive(Clone)]
pub struct PreparedFixtureController {
    shared: Arc<Shared>,
    _owner: Arc<Owner>,
}

impl PreparedFixtureController {
    pub fn new(now: impl Fn() -> MonoTime + Send + Sync + 'static) -> Self {
        let at_us = now().as_micros();
        let shared = Arc::new(Shared {
            now: Arc::new(now),
            state: Mutex::new(State {
                at_us,
                round: 0,
                pumping: false,
                closed: false,
                error: None,
                successful_sends: 0,
                players: BTreeMap::new(),
            }),
            changed: Condvar::new(),
        });
        Self {
            _owner: Arc::new(Owner(Arc::downgrade(&shared))),
            shared,
        }
    }

    pub fn player(
        &self,
        room: PathBuf,
        faults: Vec<(usize, FakePlayerFault)>,
    ) -> impl Player + use<> {
        ControlledPlayer {
            player: FakePreparedRoomPlayer {
                room,
                faults,
                prepared: 0,
            },
            shared: Arc::downgrade(&self.shared),
        }
    }

    /// Finish work at the current instant before its owner advances time.
    pub fn settle(&self) -> Result<()> {
        self.settle_before(Instant::now() + CONTROL_BOUND)
    }

    /// Recheck channels/resources after a real participant completes work.
    pub fn pump(&self) -> Result<()> {
        self.pump_before(Instant::now() + CONTROL_BOUND)
    }

    /// The preparation owner can impose its shorter original host watchdog.
    pub fn settle_before(&self, deadline: Instant) -> Result<()> {
        self.pump_before(deadline)
    }

    pub fn pump_before(&self, deadline: Instant) -> Result<()> {
        let deadline = deadline.min(Instant::now() + CONTROL_BOUND);
        let at_us = (self.shared.now)().as_micros();
        let mut state = self.shared.lock()?;
        check_open(&state)?;
        if state.pumping {
            return Err(invalid("prepared fixture already has a round in progress"));
        }
        if at_us < state.at_us {
            return Err(invalid("prepared fixture clock moved backwards"));
        }
        if at_us != state.at_us
            && state
                .players
                .values()
                .any(|player| player.phase == Phase::Running)
        {
            return Err(invalid(
                "prepared fixture clock advanced before work settled",
            ));
        }
        let round = state
            .round
            .checked_add(1)
            .ok_or_else(|| invalid("prepared fixture round overflow"))?;
        state.round = round;
        state.at_us = at_us;
        state.pumping = true;
        self.shared.changed.notify_all();
        loop {
            if let Err(error) = check_open(&state) {
                state.pumping = false;
                return Err(error);
            }
            if state.players.values().all(|player| match player.phase {
                Phase::ReportReady => true,
                Phase::Parked(done) => done >= round,
                _ => false,
            }) {
                state.pumping = false;
                return Ok(());
            }
            let Some(remaining) = deadline
                .checked_duration_since(Instant::now())
                .filter(|duration| !duration.is_zero())
            else {
                state.pumping = false;
                state.closed = true;
                state.error =
                    Some("prepared fixture work did not settle within the host bound".into());
                self.shared.changed.notify_all();
                return Err(invalid(state.error.as_deref().unwrap()));
            };
            let (next, _) = self
                .shared
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| invalid("prepared fixture state was poisoned while settling"))?;
            state = next;
        }
    }

    /// Call only after successfully sending a real Start/Closed/malformed
    /// input. The acknowledgement proves this worker consumed that input at
    /// the current instant; it does not assert that the grant was accepted.
    pub fn grant_sent(&self, attempt_id: &str) -> Result<()> {
        self.grant_sent_before(attempt_id, Instant::now() + CONTROL_BOUND)
    }

    pub fn grant_sent_before(&self, attempt_id: &str, deadline: Instant) -> Result<()> {
        self.pump_before(deadline)?;
        let mut state = self.shared.lock()?;
        let at_us = state.at_us;
        let player = state
            .players
            .get_mut(attempt_id)
            .ok_or_else(|| invalid("prepared fixture grant names an unknown attempt"))?;
        if player.grant_acknowledged || player.grant_consumed_at != Some(at_us) {
            return Err(invalid(
                "prepared fixture did not consume the original grant at this instant",
            ));
        }
        player.grant_acknowledged = true;
        Ok(())
    }

    pub fn successful_sends(&self) -> u64 {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .successful_sends
    }

    /// A report boundary alone never proves that the owned OS thread exited.
    pub fn report_ready(&self, attempt_id: &str) -> bool {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .players
            .get(attempt_id)
            .is_some_and(|player| player.phase == Phase::ReportReady)
    }

    /// Registration precedes spawning, so an unstarted worker cannot be
    /// omitted from this report barrier. Actual thread joins remain separate.
    pub fn all_reports_ready(&self) -> bool {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .players
            .values()
            .all(|player| player.phase == Phase::ReportReady)
    }

    pub fn close(&self) {
        self.shared.close();
    }
}

fn check_open(state: &State) -> Result<()> {
    if let Some(error) = &state.error {
        return Err(invalid(error));
    }
    if state.closed {
        return Err(invalid("prepared fixture control is closed"));
    }
    Ok(())
}

struct ControlledPlayer {
    player: FakePreparedRoomPlayer,
    shared: Weak<Shared>,
}

impl Player for ControlledPlayer {
    fn start(
        &mut self,
        wav: &Path,
        timing: &SceneTiming,
        directory: &Path,
        cancel: &Path,
    ) -> Result<JoinHandle<Value>> {
        self.player.start(wav, timing, directory, cancel)
    }

    fn describe(&self) -> Value {
        self.player.describe()
    }

    fn prepare(
        &mut self,
        wav: &Path,
        timing: &SceneTiming,
        _: &Path,
        cancel: &Path,
        max_wait_ms: u32,
    ) -> Result<Option<Preparing>> {
        let shared = self
            .shared
            .upgrade()
            .ok_or_else(|| invalid("prepared fixture owner no longer exists"))?;
        {
            let state = shared.lock()?;
            check_open(&state)?;
        }
        let clock = Arc::clone(&shared.now);
        self.player.prepare_with_wait(
            wav,
            timing,
            cancel,
            max_wait_ms,
            move |phase| {
                clock().as_micros().checked_mul(1000).ok_or_else(|| {
                    invalid(&format!("fake prepared {phase} time overflows nanoseconds"))
                })
            },
            move |attempt| {
                let mut state = shared.lock()?;
                check_open(&state)?;
                if state.players.contains_key(attempt) {
                    return Err(invalid("prepared fixture attempt ID was reused"));
                }
                let registered_round = state.round;
                state.players.insert(
                    attempt.to_owned(),
                    Progress {
                        phase: Phase::Registered,
                        grant_consumed_at: None,
                        grant_acknowledged: false,
                    },
                );
                drop(state);
                Ok(Box::new(WorkerWait {
                    shared,
                    attempt: attempt.to_owned(),
                    registered_round,
                }))
            },
        )
    }
}

struct WorkerWait {
    shared: Arc<Shared>,
    attempt: String,
    registered_round: u64,
}

impl WorkerWait {
    fn snapshot(&self) -> Result<(u64, u64)> {
        let state = self.shared.lock()?;
        check_open(&state)?;
        Ok((state.round, state.at_us))
    }

    /// Publish a completed-work boundary for exactly the observed round.
    /// A newer round cannot be acknowledged using work from an older one.
    fn park(&mut self, observed_round: u64, deadline: Instant) -> Result<()> {
        let mut state = self.shared.lock()?;
        check_open(&state)?;
        if state.round != observed_round {
            return Ok(());
        }
        state.players.get_mut(&self.attempt).unwrap().phase = Phase::Parked(observed_round);
        self.shared.changed.notify_all();
        while state.round == observed_round {
            check_open(&state)?;
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .filter(|duration| !duration.is_zero())
                .ok_or_else(|| invalid("prepared fixture wait exceeded its host guard"))?;
            let (next, _) = self
                .shared
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| invalid("prepared fixture state was poisoned while parked"))?;
            state = next;
        }
        check_open(&state)?;
        state.players.get_mut(&self.attempt).unwrap().phase = Phase::Running;
        Ok(())
    }

    fn delay(&mut self, delay_ms: u32) -> Result<()> {
        let (_, now) = self.snapshot()?;
        let until = now
            .checked_add(u64::from(delay_ms) * 1000)
            .ok_or_else(|| invalid("prepared fixture delay overflow"))?;
        let deadline = Instant::now() + CONTROL_BOUND;
        loop {
            let (round, now) = self.snapshot()?;
            if now >= until {
                return Ok(());
            }
            self.park(round, deadline)?;
        }
    }
}

impl PreparedWait for WorkerWait {
    fn begin(&mut self) -> Result<()> {
        // A spawned thread cannot begin at an arbitrary host scheduling time.
        // Its first work runs only in an explicitly issued parent round.
        self.park(self.registered_round, Instant::now() + CONTROL_BOUND)?;
        let mut state = self.shared.lock()?;
        check_open(&state)?;
        state.players.get_mut(&self.attempt).unwrap().phase = Phase::Running;
        Ok(())
    }

    fn slow_prepare(&mut self, room: &Path, _: &Path, delay_ms: u32) -> Result<()> {
        let deadline = Instant::now() + ROOM_BOUND;
        loop {
            let (round, _) = self.snapshot()?;
            if room.exists() {
                break;
            }
            self.park(round, deadline)?;
        }
        self.delay(delay_ms)
    }

    fn receive_grant(
        &mut self,
        inputs: &Receiver<GrantInput>,
        _: &Path,
        max_wait_ms: u32,
    ) -> Result<std::result::Result<GrantInput, RecvTimeoutError>> {
        let (_, at_us) = self.snapshot()?;
        let until = at_us
            .checked_add(u64::from(max_wait_ms) * 1000)
            .ok_or_else(|| invalid("prepared fixture grant timeout overflow"))?;
        let deadline = Instant::now() + CONTROL_BOUND;
        loop {
            let (round, at_us) = self.snapshot()?;
            // A real queued input wins over the timeout at the same instant,
            // as recv_timeout does when an input is already available.
            match inputs.try_recv() {
                Ok(input) => {
                    self.shared
                        .lock()?
                        .players
                        .get_mut(&self.attempt)
                        .unwrap()
                        .grant_consumed_at = Some(at_us);
                    return Ok(Ok(input));
                }
                Err(TryRecvError::Disconnected) => return Ok(Err(RecvTimeoutError::Disconnected)),
                Err(TryRecvError::Empty) if at_us >= until => {
                    return Ok(Err(RecvTimeoutError::Timeout));
                }
                Err(TryRecvError::Empty) => self.park(round, deadline)?,
            }
        }
    }

    fn delay_copy(&mut self, _: &Path, delay_ms: u32) -> Result<()> {
        // Preserve the shared body's expiry-then-cancel ordering. Cleanup
        // closes the controller to wake a parked delay; a cancel file does
        // not manufacture an earlier first-copy sample.
        self.delay(delay_ms)
    }

    fn delivered(&mut self) -> Result<()> {
        let mut state = self.shared.lock()?;
        // The actual send already succeeded, so retain it even if cleanup
        // closed control immediately afterwards.
        state.successful_sends = state
            .successful_sends
            .checked_add(1)
            .ok_or_else(|| invalid("prepared fixture successful-send counter overflow"))?;
        Ok(())
    }
}

impl Drop for WorkerWait {
    fn drop(&mut self) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if std::thread::panicking() {
            state.error = Some(format!("prepared fixture worker {} panicked", self.attempt));
        }
        state.players.get_mut(&self.attempt).unwrap().phase = Phase::ReportReady;
        self.shared.changed.notify_all();
    }
}
