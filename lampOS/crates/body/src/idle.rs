//! When a lamp standing at rest shifts a little by itself, and how.
//!
//! Not on a clock: each wait is drawn at random, from 40 s to 3 min, and
//! drawn longer the longer nobody has been near, up to three times as long
//! after ten minutes alone. Each shift is one of a few small moves, never
//! the one made last. Pure arithmetic: when the lamp may shift at all
//! (standing, at rest, nobody talking, no hand on it, no other gesture)
//! belongs to the body that asks.

use crate::clips::{self, Clip};
use std::time::Duration;

/// The shortest and the longest wait drawn with somebody near.
pub const GAP_LEAST: Duration = Duration::from_secs(40);
pub const GAP_MOST: Duration = Duration::from_secs(180);
/// Alone this long, a wait is drawn `ALONE_STRETCH` times as long; less
/// alone, in proportion.
pub const ALONE_FOR: Duration = Duration::from_secs(600);
pub const ALONE_STRETCH: f64 = 3.0;

/// The small moves a lamp at rest shifts by: the old runtime's idle sway,
/// and three moves of the head alone inside the reach of a glance at rest
/// (`body::REST_GLANCE_REACH`): a tilt to either side and a look up.
pub const MOVES: [&Clip; 4] = [
    &clips::IDLE,
    &clips::TILT_LEFT,
    &clips::TILT_RIGHT,
    &clips::LOOK_UP,
];

/// Whether this clip is one of the shifts at rest.
pub fn is_move(clip: &Clip) -> bool {
    MOVES.iter().any(|shift| shift.name == clip.name)
}

/// Draws the waits and the moves, from a seed.
#[derive(Clone, Copy, Debug)]
pub struct Idler {
    state: u64,
    last: Option<usize>,
}

impl Idler {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed,
            last: None,
        }
    }

    /// splitmix64: any seed, zero too, gives a full-period stream.
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// The wait before the next shift, nobody having been near for `alone`.
    pub fn gap(&mut self, alone: Duration) -> Duration {
        let share = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        let drawn = GAP_LEAST.as_secs_f64() + (GAP_MOST - GAP_LEAST).as_secs_f64() * share;
        let lonely = (alone.as_secs_f64() / ALONE_FOR.as_secs_f64()).min(1.0);
        Duration::from_secs_f64(drawn * (1.0 + (ALONE_STRETCH - 1.0) * lonely))
    }

    /// The next move: any but the last one.
    pub fn pick(&mut self) -> &'static Clip {
        let others = MOVES.len() - usize::from(self.last.is_some());
        let mut index = (self.next() % others as u64) as usize;
        if self.last.is_some_and(|last| index >= last) {
            index += 1;
        }
        self.last = Some(index);
        MOVES[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_are_irregular_and_inside_their_bounds() {
        let mut idler = Idler::new(7);
        let gaps: Vec<Duration> = (0..1_000).map(|_| idler.gap(Duration::ZERO)).collect();
        assert!(gaps.iter().all(|gap| (GAP_LEAST..=GAP_MOST).contains(gap)));
        let mut sorted = gaps.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), gaps.len(), "two waits alike");
        // Spread over the whole range, not bunched: each fifth of it is hit.
        let span = (GAP_MOST - GAP_LEAST).as_secs_f64();
        for fifth in 0..5 {
            let hits = gaps
                .iter()
                .filter(|gap| {
                    ((gap.as_secs_f64() - GAP_LEAST.as_secs_f64()) / span * 5.0) as usize == fifth
                })
                .count();
            assert!((120..=280).contains(&hits), "fifth {fifth}: {hits}");
        }
        // Another seed, other waits.
        assert_ne!(Idler::new(8).gap(Duration::ZERO), gaps[0]);
    }

    #[test]
    fn waits_grow_the_longer_nobody_is_near() {
        let mean = |alone: Duration| {
            let mut idler = Idler::new(3);
            (0..500)
                .map(|_| idler.gap(alone).as_secs_f64())
                .sum::<f64>()
                / 500.0
        };
        let (near, five, ten, hour) = (
            mean(Duration::ZERO),
            mean(Duration::from_secs(300)),
            mean(ALONE_FOR),
            mean(Duration::from_secs(3_600)),
        );
        assert!((100.0..120.0).contains(&near), "{near}");
        assert!((five / near - 2.0).abs() < 0.01, "{five}");
        assert!((ten / near - 3.0).abs() < 0.01, "{ten}");
        assert_eq!(hour, ten);
        let mut idler = Idler::new(3);
        assert!((0..500).all(|_| idler.gap(Duration::from_secs(3_600)) <= GAP_MOST * 3));
    }

    #[test]
    fn a_move_is_never_the_one_before_and_all_of_them_come() {
        let mut idler = Idler::new(11);
        let picks: Vec<&str> = (0..400).map(|_| idler.pick().name).collect();
        assert!(picks.windows(2).all(|pair| pair[0] != pair[1]), "{picks:?}");
        for shift in MOVES {
            let count = picks.iter().filter(|name| **name == shift.name).count();
            assert!((70..=130).contains(&count), "{}: {count}", shift.name);
        }
        assert!(MOVES.iter().all(|shift| is_move(shift)));
        assert!(!is_move(&clips::CALLED) && !is_move(&clips::NOD));
    }
}
