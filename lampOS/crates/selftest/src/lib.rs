//! Whether each part of the lamp works. See `docs/selftest.md`.
//!
//! - [`Check`]: one part's check, asked through the part's own interface
//!   (the bus, its owner process, the system's files). A check never writes
//!   to a device another process owns.
//! - [`run`]: every check in turn, each timed, the whole inside a budget.
//! - [`listen`]: the checks that only listen to a part on the bus.
//! - [`kernel`]: the storage, from what the kernel said since boot.
//! - [`ring`]: the ring's owner, listened to.
//! - [`mic`]: a second of the voice microphone, and its level.
//! - [`hand`]: touch by hand, at the factory.
//! - [`speaker`]: the voice's tone, heard by both microphones.
//! - [`report`]: where a run is kept, and what a person reads.

// Nothing in this library prints; the program prints what it is given.
#![deny(clippy::print_stderr, clippy::print_stdout, clippy::dbg_macro)]

use lamp_msgs::{Occasion, PartResult, SelfTest, Verdict};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub mod hand;
pub mod kernel;
pub mod listen;
pub mod mic;
pub mod report;
pub mod ring;
pub mod speaker;

/// The whole self-test has two minutes (#812).
pub const BUDGET: Duration = Duration::from_secs(120);

/// One part's check.
pub trait Check {
    /// The part's name in the report.
    fn part(&self) -> &str;

    /// Checks the part, giving up by `deadline`. The result's `took` is
    /// filled in by [`run`].
    fn run(&mut self, deadline: Instant) -> PartResult;
}

/// Runs every check in order. A check that starts after the budget is spent
/// is not run and says so; one that overruns its deadline keeps its result.
pub fn run(occasion: Occasion, checks: &mut [Box<dyn Check + '_>], budget: Duration) -> SelfTest {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let begin = Instant::now();
    let deadline = begin + budget;
    let mut parts = Vec::with_capacity(checks.len());
    for check in checks.iter_mut() {
        let at = Instant::now();
        let mut result = if at >= deadline {
            PartResult::new(
                check.part(),
                Verdict::NotChecked,
                "the self-test ran out of time before this part",
            )
        } else {
            check.run(deadline)
        };
        if result.part != check.part() {
            result.part = check.part().to_owned();
        }
        result.took = at.elapsed().as_secs_f64();
        parts.push(result);
    }
    SelfTest {
        occasion,
        started,
        took: begin.elapsed().as_secs_f64(),
        verdict: SelfTest::verdict_of(&parts),
        parts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A part that answers what it is told to, after a while.
    struct Fake {
        part: &'static str,
        verdict: Verdict,
        wait: Duration,
        ran: bool,
    }

    impl Fake {
        fn boxed(part: &'static str, verdict: Verdict, wait: Duration) -> Box<dyn Check> {
            Box::new(Self {
                part,
                verdict,
                wait,
                ran: false,
            })
        }
    }

    impl Check for Fake {
        fn part(&self) -> &str {
            self.part
        }

        fn run(&mut self, _deadline: Instant) -> PartResult {
            self.ran = true;
            std::thread::sleep(self.wait);
            let reason = if self.verdict == Verdict::Pass {
                ""
            } else {
                "told to"
            };
            PartResult::new("someone else", self.verdict, reason)
        }
    }

    #[test]
    fn every_part_is_checked_in_order_and_timed() {
        let mut checks = vec![
            Fake::boxed("camera", Verdict::Pass, Duration::from_millis(20)),
            Fake::boxed("ring", Verdict::Fail, Duration::ZERO),
            Fake::boxed("air", Verdict::NotFitted, Duration::ZERO),
        ];
        let test = run(Occasion::OnDemand, &mut checks, BUDGET);
        let names: Vec<_> = test.parts.iter().map(|p| p.part.as_str()).collect();
        assert_eq!(names, ["camera", "ring", "air"]);
        assert!(test.parts[0].took >= 0.02, "{}", test.parts[0].took);
        assert_eq!(test.verdict, Verdict::Fail);
        assert!(test.took >= test.parts[0].took);
        assert_eq!(test.check(), Ok(()));
    }

    #[test]
    fn a_part_reached_after_the_budget_is_not_checked_and_says_why() {
        let mut checks = vec![
            Fake::boxed("speaker", Verdict::Pass, Duration::from_millis(30)),
            Fake::boxed("ring", Verdict::Pass, Duration::ZERO),
        ];
        let test = run(Occasion::Factory, &mut checks, Duration::from_millis(10));
        assert_eq!(test.parts[0].verdict, Verdict::Pass);
        assert_eq!(test.parts[1].verdict, Verdict::NotChecked);
        assert!(test.parts[1].reason.contains("out of time"));
        assert_eq!(test.verdict, Verdict::NotChecked);
        assert_eq!(test.check(), Ok(()));
    }
}
