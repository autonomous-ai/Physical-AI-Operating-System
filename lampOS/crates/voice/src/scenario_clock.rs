//! Clock for explicitly stepped coordinator fixtures, never a runtime option.
//!
//! A scope belongs to one test thread. All participating IPC peers run in its
//! callback before a logical step advances. Reading the clock does no work;
//! spawned threads and unscoped tests retain the actual monotonic clock.
//! Real child startup, exit observation and the shared cleanup watchdog stay on
//! the host clock. Scoped protocol deadlines, including camera teardown, use
//! logical time. Scenario durations are not measurements of host/lamp latency.
use std::{
    cell::{Cell, RefCell},
    io,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const STEP: Duration = Duration::from_millis(2);

#[derive(Clone, Copy, Debug)]
pub(crate) enum Phase {
    Run,
    /// The coordinator's one shared, real cleanup deadline.
    Shutdown {
        deadline: Instant,
    },
}

type Pump = Box<dyn FnMut(Phase) -> io::Result<()>>;

struct State {
    id: u64,
    origin: Arc<Origin>,
    epoch_us: u64,
    epoch: Instant,
    elapsed: Duration,
    host_deadline: Instant,
    pump: Option<Pump>,
    pumping: Option<Phase>,
}

// The allocation identifies one root even when two threads have identical local
// scope IDs and clock values. Moments keep that identity alive after invalidation.
#[derive(Debug)]
struct Origin {
    epoch_us: u64,
    epoch: Instant,
    active: AtomicBool,
}

impl Origin {
    fn new(epoch_us: u64, epoch: Instant) -> Self {
        Self {
            epoch_us,
            epoch,
            active: AtomicBool::new(true),
        }
    }

    fn check_active(&self) -> io::Result<()> {
        if self.active.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(io::Error::other("scenario moment root is no longer active"))
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        // Root removal, including callback unwind and TLS teardown, invalidates
        // later adoption. It never changes an observer's last admitted reading.
        self.origin.active.store(false, Ordering::Release);
    }
}

/// An immutable snapshot from one root, safe to send to an observing thread.
/// Microsecond timestamps truncate the same exact elapsed Duration used by
/// Instant. Neither a getter nor cloning a moment samples or advances a clock.
#[derive(Clone, Debug)]
pub(crate) struct Moment {
    origin: Arc<Origin>,
    elapsed: Duration,
    monotonic_us: u64,
    instant: Instant,
}

impl Moment {
    fn checked(origin: Arc<Origin>, elapsed: Duration) -> io::Result<Self> {
        origin.check_active()?;
        let offset = u64::try_from(elapsed.as_micros())
            .map_err(|_| io::Error::other("scenario moment microseconds overflow"))?;
        let monotonic_us = origin
            .epoch_us
            .checked_add(offset)
            .ok_or_else(|| io::Error::other("scenario moment monotonic clock overflow"))?;
        let instant = origin
            .epoch
            .checked_add(elapsed)
            .ok_or_else(|| io::Error::other("scenario moment Instant overflow"))?;
        Ok(Self {
            origin,
            elapsed,
            monotonic_us,
            instant,
        })
    }

    fn validate(&self) -> io::Result<()> {
        let checked = Self::checked(self.origin.clone(), self.elapsed)?;
        if self.monotonic_us != checked.monotonic_us || self.instant != checked.instant {
            return Err(io::Error::other("scenario moment does not match its epoch"));
        }
        Ok(())
    }

    /// Project an inclusive interval at an absolute offset from its root epoch.
    /// Both endpoints must be valid, ordered snapshots of the same active root.
    /// This creates only a value: it neither admits time nor runs or advances peers.
    pub(crate) fn between(
        &self,
        later: &Moment,
        elapsed_from_origin: Duration,
    ) -> io::Result<Self> {
        self.validate()?;
        later.validate()?;
        if !Arc::ptr_eq(&self.origin, &later.origin) {
            return Err(io::Error::other(
                "scenario moment interval belongs to different roots",
            ));
        }
        if self.elapsed > later.elapsed {
            return Err(io::Error::other("scenario moment interval is reversed"));
        }
        if elapsed_from_origin < self.elapsed || elapsed_from_origin > later.elapsed {
            return Err(io::Error::other(
                "scenario moment projection is outside its interval",
            ));
        }
        Self::checked(self.origin.clone(), elapsed_from_origin)
    }

    pub(crate) fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub(crate) fn monotonic_us(&self) -> u64 {
        self.monotonic_us
    }

    pub(crate) fn instant(&self) -> Instant {
        self.instant
    }
}

struct ObserverState {
    id: u64,
    moment: Moment,
}

thread_local! {
    static ACTIVE: RefCell<Option<State>> = const { RefCell::new(None) };
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
    static OBSERVER: RefCell<Option<ObserverState>> = const { RefCell::new(None) };
    static NEXT_OBSERVER_ID: Cell<u64> = const { Cell::new(1) };
}

fn has_observer() -> bool {
    OBSERVER.with(|observer| observer.borrow().is_some())
}

fn observer_monotonic_us() -> u64 {
    OBSERVER.with(|observer| {
        observer
            .borrow()
            .as_ref()
            .map_or_else(lamp_ipc::monotonic_us, |state| state.moment.monotonic_us())
    })
}

fn observer_instant() -> Instant {
    OBSERVER.with(|observer| {
        observer
            .borrow()
            .as_ref()
            .map_or_else(Instant::now, |state| state.moment.instant())
    })
}

/// A thread-bound reader of explicitly admitted moments, never a root/pump.
/// Root invalidation rejects further adoption but keeps the last reading frozen;
/// the controller must cancel the observer. Drop alone restores the host clock.
pub(crate) struct ObserverScope {
    id: u64,
    _thread: PhantomData<Rc<()>>,
}

impl ObserverScope {
    pub(crate) fn enter(moment: Moment) -> io::Result<Self> {
        if ACTIVE.with(|active| active.borrow().is_some()) || has_observer() {
            return Err(io::Error::other("nested scenario observer scope"));
        }
        moment.validate()?;
        let id = NEXT_OBSERVER_ID.with(|next| {
            let id = next.get();
            let successor = id
                .checked_add(1)
                .ok_or_else(|| io::Error::other("scenario observer identity exhausted"))?;
            next.set(successor);
            Ok::<_, io::Error>(id)
        })?;
        OBSERVER.with(|observer| {
            *observer.borrow_mut() = Some(ObserverState { id, moment });
        });
        Ok(Self {
            id,
            _thread: PhantomData,
        })
    }

    pub(crate) fn observe(&mut self, moment: Moment) -> io::Result<()> {
        OBSERVER.with(|observer| {
            let mut observer = observer.borrow_mut();
            let state = observer
                .as_mut()
                .filter(|state| state.id == self.id)
                .ok_or_else(|| io::Error::other("scenario observer is no longer active"))?;
            moment.validate()?;
            if !Arc::ptr_eq(&state.moment.origin, &moment.origin) {
                return Err(io::Error::other("scenario moment belongs to another root"));
            }
            // Compare the full Duration: sub-microsecond regressions can have
            // the same integer timestamp but a different Instant.
            if moment.elapsed < state.moment.elapsed {
                return Err(io::Error::other("scenario observer time regressed"));
            }
            state.moment = moment;
            Ok(())
        })
    }
}

impl Drop for ObserverScope {
    fn drop(&mut self) {
        let removed = OBSERVER.try_with(|observer| {
            let mut observer = observer.try_borrow_mut().ok()?;
            if observer.as_ref().is_some_and(|state| state.id == self.id) {
                observer.take()
            } else {
                None
            }
        });
        drop(removed);
    }
}

/// Dropping a scope restores the host clock and releases its fixture state.
/// The Rc marker prevents moving a scope to another thread or sharing it.
pub(crate) struct Scope {
    id: u64,
    _thread: PhantomData<Rc<()>>,
}

impl Scope {
    pub(crate) fn enter(watchdog: Duration) -> io::Result<Self> {
        let epoch = Instant::now();
        let host_deadline = epoch
            .checked_add(watchdog)
            .filter(|_| !watchdog.is_zero())
            .ok_or_else(|| io::Error::other("invalid scenario watchdog"))?;
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            if active.is_some() || has_observer() {
                return Err(io::Error::other("nested scenario clock scope"));
            }
            let id = NEXT_ID.with(|next| {
                let id = next.get();
                let successor = id
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("scenario clock identity exhausted"))?;
                next.set(successor);
                Ok::<_, io::Error>(id)
            })?;
            let epoch_us = lamp_ipc::monotonic_us();
            *active = Some(State {
                id,
                origin: Arc::new(Origin::new(epoch_us, epoch)),
                epoch_us,
                epoch,
                elapsed: Duration::ZERO,
                host_deadline,
                pump: None,
                pumping: None,
            });
            Ok(Self {
                id,
                _thread: PhantomData,
            })
        })
    }

    /// Snapshot this active root without running its peers or reading host time.
    pub(crate) fn moment(&self) -> io::Result<Moment> {
        ACTIVE.with(|active| {
            let active = active.borrow();
            let state = active
                .as_ref()
                .filter(|state| state.id == self.id)
                .ok_or_else(|| io::Error::other("scenario clock scope is no longer active"))?;
            if state.epoch_us != state.origin.epoch_us || state.epoch != state.origin.epoch {
                return Err(io::Error::other("scenario root epoch changed"));
            }
            Moment::checked(state.origin.clone(), state.elapsed)
        })
    }

    pub(crate) fn set_pump(
        &self,
        pump: impl FnMut(Phase) -> io::Result<()> + 'static,
    ) -> io::Result<()> {
        // Never run or drop caller-owned state while the TLS RefCell is borrowed.
        let mut pump: Option<Pump> = Some(Box::new(pump));
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            let state = active
                .as_mut()
                .filter(|state| state.id == self.id)
                .ok_or_else(|| io::Error::other("scenario clock scope is no longer active"))?;
            if state.pumping.is_some() || state.pump.is_some() {
                return Err(io::Error::other(
                    "scenario peers already installed or stepping",
                ));
            }
            state.pump = pump.take();
            Ok(())
        })
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        // Drop must also be safe during unwinding/TLS teardown. No API exposes a
        // RefCell borrow; callbacks and their destructors always run outside it.
        let removed = ACTIVE.try_with(|active| {
            let mut active = active.try_borrow_mut().ok()?;
            if active.as_ref().is_some_and(|state| state.id == self.id) {
                active.take()
            } else {
                None
            }
        });
        drop(removed);
    }
}

pub(crate) fn is_active() -> bool {
    ACTIVE.with(|active| active.borrow().is_some()) || has_observer()
}

pub(crate) fn monotonic_us() -> u64 {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .map_or_else(observer_monotonic_us, |state| {
                state.epoch_us
                    + u64::try_from(state.elapsed.as_micros()).expect("bounded scenario time")
            })
    })
}

pub(crate) fn instant() -> Instant {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .map_or_else(observer_instant, |state| state.epoch + state.elapsed)
    })
}

fn advance(state: &mut State, by: Duration) -> io::Result<()> {
    state.elapsed = state
        .elapsed
        .checked_add(by)
        .filter(|elapsed| elapsed.as_micros() <= u128::from(u64::MAX - state.epoch_us))
        .filter(|elapsed| state.epoch.checked_add(*elapsed).is_some())
        .ok_or_else(|| io::Error::other("scenario clock overflow"))?;
    Ok(())
}

/// Explicit stale-input fault injection inside a peer callback. Ordinary
/// scenarios only advance by STEP after all peers complete their work.
pub(crate) fn elapse_for_fault(by: Duration) -> io::Result<()> {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let state = active
            .as_mut()
            .filter(|state| matches!(state.pumping, Some(Phase::Run)))
            .ok_or_else(|| io::Error::other("clock fault requires an active run step"))?;
        advance(state, by)
    })
}

/// Pump one complete bounded peer step, then advance exactly two milliseconds.
/// Cleanup pumps retain the current logical instant and the real shared budget.
/// Returns false for an ordinary unscoped test. A callback panic invalidates its
/// scope before resuming unwind, so catching that panic cannot keep a half-step.
pub(crate) fn step(phase: Phase) -> io::Result<bool> {
    if has_observer() {
        return Err(io::Error::other(
            "scenario observers cannot pump or advance",
        ));
    }
    let taken = ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let Some(state) = active.as_mut() else {
            return Ok(None);
        };
        if state.pumping.is_some() {
            return Err(io::Error::other("recursive scenario peer step"));
        }
        if matches!(phase, Phase::Run) && Instant::now() >= state.host_deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "scenario host watchdog",
            ));
        }
        let pump = state
            .pump
            .take()
            .ok_or_else(|| io::Error::other("scenario peers not installed"))?;
        state.pumping = Some(phase);
        Ok(Some((state.id, pump)))
    })?;
    let Some((id, mut pump)) = taken else {
        return Ok(false);
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| pump(phase)));
    if let Err(panic) = outcome {
        let removed = ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            if active.as_ref().is_some_and(|state| state.id == id) {
                active.take()
            } else {
                None
            }
        });
        drop(removed);
        drop(pump);
        resume_unwind(panic);
    }
    let result = outcome.expect("panic resumed above");
    let mut pump = Some(pump);
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let state = active
            .as_mut()
            .filter(|state| state.id == id)
            .ok_or_else(|| io::Error::other("scenario clock scope changed during peer step"))?;
        state.pumping = None;
        state.pump = pump.take();
        result?;
        if matches!(phase, Phase::Run) {
            if Instant::now() >= state.host_deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "scenario host watchdog",
                ));
            }
            advance(state, STEP)?;
        }
        Ok(true)
    })
}

/// Observe this owned fixture child's successful exit without consuming it.
/// The supervisor must still perform its real wait/reap and exit checks.
pub(crate) fn wait_child_exited(pid: u32, deadline: Instant) -> io::Result<()> {
    use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
    let child = i32::try_from(pid)
        .ok()
        .and_then(Pid::from_raw)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid fixture child PID"))?;
    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("fixture child {pid} exit observation timed out"),
            ));
        }
        match waitid(
            WaitId::Pid(child),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
        ) {
            Ok(Some(status)) if status.exit_status() == Some(0) => return Ok(()),
            Ok(Some(status)) => {
                return Err(io::Error::other(format!(
                    "fixture child {pid} exited abnormally: {status:?}"
                )));
            }
            Ok(None) => std::thread::sleep(
                Duration::from_millis(1).min(deadline.saturating_duration_since(Instant::now())),
            ),
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => {
                return Err(io::Error::other(format!(
                    "fixture child {pid} exit observation failed: {error}"
                )));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn scope() -> Scope {
        let scope = Scope::enter(Duration::from_secs(10)).unwrap();
        scope.set_pump(|_| Ok(())).unwrap();
        scope
    }

    #[test]
    fn host_pause_does_not_age_a_scoped_packet_but_logical_time_does() {
        use crate::{
            process::{SessionDirectory, new_boot},
            transport::WorkerChannels,
        };
        let _scope = scope();
        let directory = SessionDirectory::create().unwrap();
        let parent = new_boot().unwrap();
        let worker = new_boot().unwrap();
        let mut tx = WorkerChannels::bind(&directory.path, "probe", parent, worker, false).unwrap();
        let mut rx = WorkerChannels::bind(&directory.path, "probe", parent, worker, true).unwrap();
        tx.connect(&directory.path, "probe", false).unwrap();
        rx.connect(&directory.path, "probe", true).unwrap();
        let before = monotonic_us();
        tx.control.send(7_u64).unwrap();
        std::thread::sleep(Duration::from_millis(70));
        assert_eq!(monotonic_us(), before);
        assert_eq!(rx.control.receive::<u64>().unwrap(), Some(7));
        // Since #791 one late message that is next in order is taken, as a
        // held-up sender's would be; a second late one in a row is not.
        tx.control.send(8_u64).unwrap();
        for _ in 0..50 {
            step(Phase::Run).unwrap();
        }
        assert_eq!(monotonic_us() - before, 100_000);
        assert_eq!(rx.control.receive::<u64>().unwrap(), Some(8));
        tx.control.send(9_u64).unwrap();
        for _ in 0..50 {
            step(Phase::Run).unwrap();
        }
        assert!(
            rx.control.receive::<u64>().is_err(),
            "production 100 ms envelope boundary must still reject"
        );
    }

    #[test]
    fn concurrent_scopes_and_new_threads_do_not_share_a_clock() {
        let barrier = Arc::new(Barrier::new(2));
        let threads: Vec<_> = [3, 8]
            .into_iter()
            .map(|steps| {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let _scope = scope();
                    let before = monotonic_us();
                    barrier.wait();
                    for _ in 0..steps {
                        step(Phase::Run).unwrap();
                    }
                    barrier.wait();
                    assert_eq!(monotonic_us() - before, steps * 2_000);
                    std::thread::spawn(|| {
                        assert!(!is_active());
                        let before = lamp_ipc::monotonic_us();
                        let value = monotonic_us();
                        assert!(value >= before && value <= lamp_ipc::monotonic_us());
                    })
                    .join()
                    .unwrap();
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert!(!is_active());
    }

    #[test]
    fn callback_keeps_clock_active_and_rejects_nesting_and_recursion() {
        let scope = Scope::enter(Duration::from_secs(10)).unwrap();
        let before = monotonic_us();
        scope
            .set_pump(move |_| {
                assert!(is_active());
                assert_eq!(monotonic_us(), before);
                assert!(Scope::enter(Duration::from_secs(1)).is_err());
                assert!(step(Phase::Run).is_err());
                Ok(())
            })
            .unwrap();
        step(Phase::Run).unwrap();
        assert_eq!(monotonic_us(), before + 2_000);
        assert!(scope.set_pump(|_| Ok(())).is_err());
    }

    #[test]
    fn callback_panic_releases_state_and_old_scope_cannot_clear_its_successor() {
        let old = Scope::enter(Duration::from_secs(10)).unwrap();
        let owned = Rc::new(());
        let captured = owned.clone();
        old.set_pump(move |_| {
            let _keep = &captured;
            panic!("injected peer panic")
        })
        .unwrap();
        assert!(catch_unwind(|| step(Phase::Run)).is_err());
        assert!(!is_active());
        assert_eq!(Rc::strong_count(&owned), 1);
        let next = scope();
        drop(old);
        assert!(is_active());
        step(Phase::Run).unwrap();
        drop(next);
        assert!(!is_active());
    }

    #[test]
    fn callback_error_keeps_its_peers_for_shutdown_without_advancing_time() {
        let scope = Scope::enter(Duration::from_secs(10)).unwrap();
        let deadline = Instant::now() + Duration::from_millis(300);
        let calls = Rc::new(Cell::new((0, 0)));
        let observed = calls.clone();
        scope
            .set_pump(move |phase| {
                let (run, shutdown) = observed.get();
                match phase {
                    Phase::Run => {
                        observed.set((run + 1, shutdown));
                        Err(io::Error::other("injected peer error"))
                    }
                    Phase::Shutdown { deadline: given } => {
                        assert_eq!(given, deadline);
                        observed.set((run, shutdown + 1));
                        Ok(())
                    }
                }
            })
            .unwrap();
        let before = (monotonic_us(), instant());
        assert_eq!(
            step(Phase::Run).unwrap_err().to_string(),
            "injected peer error"
        );
        assert_eq!((monotonic_us(), instant()), before);
        assert!(step(Phase::Shutdown { deadline }).unwrap());
        assert!(step(Phase::Shutdown { deadline }).unwrap());
        assert_eq!((monotonic_us(), instant()), before);
        assert_eq!(calls.get(), (1, 2));
        drop(scope);
        assert!(!is_active());
        assert_eq!(Rc::strong_count(&calls), 1);
    }

    #[test]
    fn host_watchdog_before_or_after_a_peer_step_still_allows_shutdown() {
        // Expire the private host deadline directly, so this regression does
        // not itself depend on sleeping or winning a host scheduling race.
        fn expire_watchdog() {
            ACTIVE.with(|active| {
                active.borrow_mut().as_mut().unwrap().host_deadline = Instant::now();
            });
        }
        for expire_during_pump in [false, true] {
            let scope = Scope::enter(Duration::from_secs(10)).unwrap();
            let deadline = Instant::now() + Duration::from_millis(300);
            let calls = Rc::new(Cell::new((0, 0)));
            let observed = calls.clone();
            scope
                .set_pump(move |phase| {
                    let (run, shutdown) = observed.get();
                    match phase {
                        Phase::Run => {
                            assert!(expire_during_pump);
                            observed.set((run + 1, shutdown));
                            expire_watchdog();
                        }
                        Phase::Shutdown { deadline: given } => {
                            assert_eq!(given, deadline);
                            observed.set((run, shutdown + 1));
                        }
                    }
                    Ok(())
                })
                .unwrap();
            let before = (monotonic_us(), instant());
            if !expire_during_pump {
                expire_watchdog();
            }
            let error = step(Phase::Run).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            assert_eq!(error.to_string(), "scenario host watchdog");
            assert_eq!((monotonic_us(), instant()), before);
            assert!(step(Phase::Shutdown { deadline }).unwrap());
            assert_eq!((monotonic_us(), instant()), before);
            assert_eq!(calls.get(), (i32::from(expire_during_pump), 1));
            drop(scope);
            assert!(!is_active());
            assert_eq!(Rc::strong_count(&calls), 1);
        }
    }

    #[test]
    fn shutdown_steps_keep_logical_time_and_share_the_given_host_deadline() {
        let scope = Scope::enter(Duration::from_secs(10)).unwrap();
        let deadline = Instant::now() + Duration::from_millis(300);
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        scope
            .set_pump(move |phase| {
                assert!(matches!(phase, Phase::Shutdown { deadline: d } if d == deadline));
                assert!(elapse_for_fault(Duration::from_millis(50)).is_err());
                observed.set(observed.get() + 1);
                Ok(())
            })
            .unwrap();
        let before = (monotonic_us(), instant());
        step(Phase::Shutdown { deadline }).unwrap();
        step(Phase::Shutdown { deadline }).unwrap();
        assert_eq!((monotonic_us(), instant()), before);
        assert_eq!(calls.get(), 2);
    }
}

#[cfg(test)]
#[path = "scenario_moment_tests.rs"]
mod moment_tests;

#[cfg(test)]
#[path = "scenario_moment_between_tests.rs"]
mod moment_between_tests;
