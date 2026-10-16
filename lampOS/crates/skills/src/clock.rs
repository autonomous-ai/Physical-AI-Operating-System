//! The program's one clock for its own waits (`docs/skills.md`, "A test's
//! clock"): a skill's `for`, a clip's length, a held pose said again, a
//! light asked again, the Policy's 2.5 s, the voice's line and the reload.
//!
//! It is the host's monotonic clock, and stays so in production. With
//! `--clock-control` a test may skip it ahead ([`skip`]), so that it sits
//! through no real second it does not have to: the clock then runs on from
//! there at the host's pace, and every wait below wakes on it within
//! [`SLICE`]. Bus stamps, the freshness of what other parts said, a light's
//! claim and the socket timeouts against other parts stay on the host's
//! clocks, which those parts read too.
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// How far a test skipped the clock, in µs.
static SKIPPED_US: AtomicU64 = AtomicU64::new(0);
/// Whether a test drives the clock: only then do waits look at it in slices.
static STEPPED: AtomicBool = AtomicBool::new(false);

/// How soon a wait sees a skip.
pub const SLICE: Duration = Duration::from_millis(2);

/// From now on a test may skip the clock: waits look at it every [`SLICE`].
pub fn stepped() {
    STEPPED.store(true, Ordering::SeqCst);
}

/// Whether a test drives the clock.
pub fn is_stepped() -> bool {
    STEPPED.load(Ordering::SeqCst)
}

/// Moves the clock `by` ahead at once.
pub fn skip(by: Duration) {
    SKIPPED_US.fetch_add(
        u64::try_from(by.as_micros()).unwrap_or(u64::MAX),
        Ordering::SeqCst,
    );
}

fn skipped() -> Duration {
    Duration::from_micros(SKIPPED_US.load(Ordering::SeqCst))
}

/// Now, on this clock.
pub fn now() -> Instant {
    Instant::now() + skipped()
}

/// Now in µs, on the host's monotonic clock and what was skipped.
pub fn now_us() -> u64 {
    lamp_ipc::monotonic_us() + SKIPPED_US.load(Ordering::SeqCst)
}

/// How long a poll for something else waits: no longer than a slice while
/// a test drives the clock, so that a skip is taken in at once.
pub fn poll(wait: Duration) -> Duration {
    if is_stepped() { wait.min(SLICE) } else { wait }
}

/// Sleeps `wait` on this clock.
pub fn sleep(wait: Duration) {
    if !is_stepped() {
        return std::thread::sleep(wait);
    }
    let until = now() + wait;
    loop {
        let left = until.saturating_duration_since(now());
        if left.is_zero() {
            return;
        }
        std::thread::sleep(left.min(SLICE));
    }
}

/// `receiver.recv_timeout(wait)`, the wait on this clock.
pub fn recv_timeout<T>(
    receiver: &mpsc::Receiver<T>,
    wait: Duration,
) -> Result<T, mpsc::RecvTimeoutError> {
    if !is_stepped() {
        return receiver.recv_timeout(wait);
    }
    let until = now() + wait;
    loop {
        let left = until.saturating_duration_since(now());
        if left.is_zero() {
            return Err(mpsc::RecvTimeoutError::Timeout);
        }
        match receiver.recv_timeout(left.min(SLICE)) {
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            done => return done,
        }
    }
}
