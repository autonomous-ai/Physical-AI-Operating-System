//! When a lamp standing at rest shifts a little by itself, and how.
//!
//! Not on a clock: each wait is drawn at random, from 40 s to 3 min. A lamp
//! left alone folds (`--awake-seconds`) and folded never shifts, which is
//! how it is still when nobody is near. Each shift is one of a few small moves, never
//! the one made last. Pure arithmetic: when the lamp may shift at all
//! (standing, at rest, nobody talking, no hand on it, no other gesture)
//! belongs to the body that asks.

use crate::clips::{self, Clip};
use std::time::Duration;

/// The shortest and the longest wait drawn.
pub const GAP_LEAST: Duration = Duration::from_secs(40);
pub const GAP_MOST: Duration = Duration::from_secs(180);

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

    /// The wait before the next shift.
    pub fn gap(&mut self) -> Duration {
        let share = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        let drawn = GAP_LEAST.as_secs_f64() + (GAP_MOST - GAP_LEAST).as_secs_f64() * share;
        Duration::from_secs_f64(drawn)
    }

    /// The next move: any but the last one. The two tilts only for a head
    /// that tilts when it stands (`--stand-roll` past `--reach`'s roll):
    /// elsewhere their roll is left out and both are the same small lift.
    pub fn pick(&mut self, tilts: bool) -> &'static Clip {
        let open: Vec<usize> = (0..MOVES.len())
            .filter(|index| Some(*index) != self.last)
            .filter(|index| tilts || !is_tilt(MOVES[*index]))
            .collect();
        let index = open[(self.next() % open.len() as u64) as usize];
        self.last = Some(index);
        MOVES[index]
    }
}

fn is_tilt(clip: &Clip) -> bool {
    clip.name == clips::TILT_LEFT.name || clip.name == clips::TILT_RIGHT.name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_are_irregular_and_inside_their_bounds() {
        let mut idler = Idler::new(7);
        let gaps: Vec<Duration> = (0..1_000).map(|_| idler.gap()).collect();
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
        assert_ne!(Idler::new(8).gap(), gaps[0]);
    }

    #[test]
    fn a_move_is_never_the_one_before_and_all_of_them_come() {
        let mut idler = Idler::new(11);
        let picks: Vec<&str> = (0..400).map(|_| idler.pick(true).name).collect();
        assert!(picks.windows(2).all(|pair| pair[0] != pair[1]), "{picks:?}");
        for shift in MOVES {
            let count = picks.iter().filter(|name| **name == shift.name).count();
            assert!((70..=130).contains(&count), "{}: {count}", shift.name);
        }
        assert!(MOVES.iter().all(|shift| is_move(shift)));
        assert!(!is_move(&clips::CALLED) && !is_move(&clips::NOD));
        // A head that does not tilt standing: no tilts, the other two by turns.
        let picks: Vec<&str> = (0..20).map(|_| idler.pick(false).name).collect();
        assert!(
            picks.iter().all(|name| !name.starts_with("tilt")),
            "{picks:?}"
        );
        assert!(picks.windows(2).all(|pair| pair[0] != pair[1]), "{picks:?}");
    }
}
