//! A stepped deadline still observes, kills and reaps an actual local process.
use super::*;
use std::{
    cell::{Cell, RefCell},
    os::unix::process::ExitStatusExt,
    panic::{AssertUnwindSafe, catch_unwind},
    process::ExitStatus,
    rc::Rc,
};

struct State {
    now_us: Cell<u64>,
    waits: RefCell<Vec<Duration>>,
    reaped: Cell<Option<ExitStatus>>,
    host_deadline: Instant,
}

#[derive(Clone)]
pub(super) struct Clock(Rc<State>);

/// `Child` alone does not terminate a process on unwind. This fixture owns
/// cleanup even if its immutable host watchdog or a boundary check fails.
struct Cleanup<'a> {
    child: &'a mut Child,
    armed: bool,
}
impl<'a> Cleanup<'a> {
    fn new(child: &'a mut Child) -> Self {
        Self { child, armed: true }
    }
}
impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

impl Clock {
    fn new(now_us: u64, host_deadline: Instant) -> Self {
        Self(Rc::new(State {
            now_us: Cell::new(now_us),
            waits: RefCell::new(Vec::new()),
            reaped: Cell::new(None),
            host_deadline,
        }))
    }

    fn now(&self) -> u64 {
        self.0.now_us.get()
    }

    pub(super) fn wait(&self, child: &mut Child, bound_us: u64) -> Option<ExitStatus> {
        let cleanup = Cleanup::new(child);
        let deadline_us = self.now().checked_add(bound_us).unwrap();
        let result = wait_child_with(
            cleanup.child,
            deadline_us,
            || self.now(),
            |duration| {
                assert!(
                    Instant::now() < self.0.host_deadline,
                    "transport fixture exceeded its host watchdog"
                );
                // This replaces only the original wait. try_wait still precedes
                // every deadline sample, and kill/wait still run at expiry.
                assert_eq!(duration, Duration::from_millis(20));
                self.0.waits.borrow_mut().push(duration);
                self.0.now_us.set(
                    self.now()
                        .checked_add(u64::try_from(duration.as_micros()).unwrap())
                        .unwrap(),
                );
            },
        );
        self.0.reaped.set(cleanup.child.try_wait().unwrap());
        result
    }
}

pub(super) fn expired_transport_report() -> Finished {
    let origin_us = 1_000_000;
    let clock = Clock::new(origin_us, Instant::now() + Duration::from_secs(5));
    let result = audit_grant_tests::exit_report_prepared(
        0,
        &["/bin/sleep", "30"],
        false,
        "sleep",
        |backend, _| {
            // Spawn has succeeded, and the actual child has not exited. No
            // fixture receipt or logical clock value substitutes for try_wait.
            assert!(
                backend
                    .session
                    .as_mut()
                    .unwrap()
                    .child
                    .try_wait()
                    .unwrap()
                    .is_none()
            );
            backend.transport_exit_clock = Some(clock.clone());
        },
    );
    assert_eq!(clock.now(), origin_us + TRANSPORT_EXIT_BOUND_US);
    assert_eq!(
        clock.0.waits.borrow().as_slice(),
        vec![Duration::from_millis(20); 250]
    );
    assert_eq!(clock.0.reaped.get().unwrap().signal(), Some(9));
    result
}

fn sleeper() -> Child {
    let mut child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    {
        let mut cleanup = Cleanup::new(&mut child);
        assert!(cleanup.child.try_wait().unwrap().is_none());
        // Transfer the live child only after the startup observation succeeds.
        cleanup.armed = false;
    }
    child
}

#[test]
fn clock_reads_neither_advance_time_nor_wait() {
    let clock = Clock::new(7, Instant::now() + Duration::from_secs(5));
    assert_eq!((clock.now(), clock.now(), clock.now()), (7, 7, 7));
    assert!(clock.0.waits.borrow().is_empty());
    assert!(clock.0.reaped.get().is_none());
}

#[test]
fn a_live_child_is_killed_and_reaped_at_the_full_logical_boundary() {
    let mut child = sleeper();
    let clock = Clock::new(17, Instant::now() + Duration::from_secs(5));
    assert!(clock.wait(&mut child, 40_000).is_none());
    assert_eq!(clock.now(), 40_017);
    assert_eq!(
        clock.0.waits.borrow().as_slice(),
        [Duration::from_millis(20); 2]
    );
    let actual = child.try_wait().unwrap().unwrap();
    assert_eq!(actual.signal(), Some(9));
    assert_eq!(clock.0.reaped.get(), Some(actual));
}

#[test]
fn zero_budget_kills_and_reaps_without_advancing() {
    let mut child = sleeper();
    let clock = Clock::new(21, Instant::now() + Duration::from_secs(5));
    assert!(clock.wait(&mut child, 0).is_none());
    assert_eq!(clock.now(), 21);
    assert!(clock.0.waits.borrow().is_empty());
    assert_eq!(child.try_wait().unwrap().unwrap().signal(), Some(9));
}

#[test]
fn an_actual_successful_exit_precedes_even_an_expired_deadline() {
    let mut child = Command::new("/usr/bin/true").spawn().unwrap();
    let cleanup = Cleanup::new(&mut child);
    let actual = cleanup.child.wait().unwrap();
    let result = wait_child_with(
        cleanup.child,
        0,
        || panic!("an already-exited child needs no deadline sample"),
        |_| panic!("an already-exited child needs no wait"),
    );
    assert_eq!(result, Some(actual));
    assert!(actual.success());
}

#[test]
fn an_actual_failed_exit_keeps_its_exit_status() {
    let mut child = Command::new("/bin/sh")
        .args(["-c", "exit 7"])
        .spawn()
        .unwrap();
    let cleanup = Cleanup::new(&mut child);
    let actual = cleanup.child.wait().unwrap();
    let clock = Clock::new(29, Instant::now() + Duration::from_secs(5));
    assert_eq!(clock.wait(cleanup.child, 40_000), Some(actual));
    assert_eq!(actual.code(), Some(7));
    assert_eq!(clock.now(), 29);
    assert!(clock.0.waits.borrow().is_empty());
}

#[test]
fn an_expired_host_watchdog_reaps_before_unwind_without_advancing() {
    let mut child = sleeper();
    let clock = Clock::new(31, Instant::now());
    let failure = catch_unwind(AssertUnwindSafe(|| clock.wait(&mut child, 40_000)));
    assert!(failure.is_err());
    assert_eq!(clock.now(), 31);
    assert!(clock.0.waits.borrow().is_empty());
    assert!(clock.0.reaped.get().is_none());
    assert_eq!(child.try_wait().unwrap().unwrap().signal(), Some(9));
}

#[test]
fn a_panic_before_clock_installation_still_reaps_the_child() {
    let mut child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    let failure = catch_unwind(AssertUnwindSafe(|| {
        let cleanup = Cleanup::new(&mut child);
        assert!(cleanup.child.try_wait().unwrap().is_none());
        panic!("fixture failed before clock installation");
    }));
    assert!(failure.is_err());
    assert_eq!(child.try_wait().unwrap().unwrap().signal(), Some(9));
}
