//! Looking half as often while one face sits still at the lamp (#705).
//!
//! The look rate matters when someone arrives or moves, not while they sit:
//! on lamp-4ace looking four times a second at someone sitting cost 44 % of
//! a core. So while the one face near enough (the known one, with
//! recognition on) has kept its place and size for [`STEADY_FOR_US`], the
//! eyes look every [`STEADY_EVERY_MS`] ms instead of every
//! 250 ms. A move past the margins, a second face, a loss, another name or a
//! full look or remember asked goes back to four a second at once.
use crate::sight::Sight;

/// How far the face's centre may be from where it was when it settled, in
/// the body's half-pictures (-1 to 1 across and down). A person sitting at
/// the desk wanders 0.01 to 0.02 of the picture from look to look on
/// lamp-4ace (docs/eyes.md, #763), and `still::NEAR` holds a place to 0.03
/// of the picture: 0.03 of the picture is 0.06 of a half-picture. The body
/// does not turn for a face under 0.15 across or 0.20 down from the middle
/// (`crates/body/src/gaze.rs`, `NEAR`), so a face that moves less than this
/// would not have moved the head.
pub const MOVED: f32 = 0.06;
/// How far its height may change, as a share of the picture's: the same
/// 0.03 as `still::NEAR`. People at the desk are 0.12 to 0.27 of the
/// height, so leaning in or back is several times this.
pub const RESIZED: f32 = 0.03;
/// How long it keeps its place before the eyes look half as often.
pub const STEADY_FOR_US: u64 = 3_000_000;
/// The look period while it is steady: twice a second instead of four.
pub const STEADY_EVERY_MS: u64 = 500;
/// Looks further apart than this (a camera closed, frames that did not
/// come) are not one steady run.
const GAP_US: u64 = 1_000_000;

#[derive(Debug, Default)]
pub struct Steady {
    /// Where the face was when it settled, whose it was, when it settled
    /// and when it was last seen there.
    since: Option<(Sight, Option<String>, u64, u64)>,
}

impl Steady {
    /// One look: how many faces near enough to follow it found, the one the
    /// body is told of, whether recognition is on and whose face is known
    /// at the lamp now.
    pub fn look(
        &mut self,
        near: usize,
        sight: Option<Sight>,
        recognising: bool,
        known: Option<&str>,
        now_us: u64,
    ) {
        let Some(sight) = sight.filter(|_| near == 1 && (!recognising || known.is_some())) else {
            self.since = None;
            return;
        };
        if let Some((first, name, _, last)) = self.since.as_mut()
            && name.as_deref() == known
            && now_us.saturating_sub(*last) <= GAP_US
            && (sight.x - first.x).abs() <= MOVED
            && (sight.y - first.y).abs() <= MOVED
            && (sight.size - first.size).abs() <= RESIZED
        {
            *last = now_us;
            return;
        }
        self.since = Some((sight, known.map(str::to_owned), now_us, now_us));
    }

    /// Whether to look half as often now. `asked` is a full look or a
    /// remember under way: it ends the steady run at once.
    pub fn halved(&mut self, asked: bool, now_us: u64) -> bool {
        if asked {
            self.since = None;
        }
        self.since
            .as_ref()
            .is_some_and(|(_, _, at, _)| now_us.saturating_sub(*at) >= STEADY_FOR_US)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOK_US: u64 = 250_000;

    fn at(x: f32, size: f32) -> Option<Sight> {
        Some(Sight {
            x,
            y: -0.2,
            size,
            turned: 0.0,
        })
    }

    /// Looks four times a second at the same face from `from` for `for_us`.
    fn sit(steady: &mut Steady, from: u64, for_us: u64) -> u64 {
        let mut now = from;
        while now < from + for_us {
            // A sitting person's wander, 0.02 of a half-picture each way.
            let wander = if (now / LOOK_US).is_multiple_of(2) {
                0.02
            } else {
                -0.02
            };
            steady.look(1, at(0.1 + wander, 0.2), false, None, now);
            now += LOOK_US;
        }
        now
    }

    #[test]
    fn a_face_steady_for_three_seconds_halves_the_rate() {
        let mut steady = Steady::default();
        let now = sit(&mut steady, 0, 2_750_000);
        assert!(!steady.halved(false, now - LOOK_US));
        let now = sit(&mut steady, now, LOOK_US);
        assert!(steady.halved(false, now));
    }

    #[test]
    fn a_move_a_second_face_or_a_loss_restores_four_a_second() {
        // A face settled for over three seconds, then one more look.
        let after = |near: usize, sight: Option<Sight>, gap_us: u64| {
            let mut steady = Steady::default();
            let now = sit(&mut steady, 0, STEADY_FOR_US + LOOK_US);
            assert!(steady.halved(false, now));
            steady.look(near, sight, false, None, now + gap_us);
            steady.halved(false, now + gap_us)
        };
        assert!(after(1, at(0.1, 0.2), 0));
        assert!(!after(1, at(0.1 + MOVED + 0.05, 0.2), 0));
        assert!(!after(1, at(0.1, 0.2 + RESIZED + 0.01), 0));
        assert!(!after(2, at(0.1, 0.2), 0));
        assert!(!after(0, None, 0));
        // Looks that stopped coming are not one run.
        assert!(!after(1, at(0.1, 0.2), 2 * GAP_US));
    }

    #[test]
    fn a_full_look_or_remember_asked_restores_four_a_second() {
        let mut steady = Steady::default();
        let now = sit(&mut steady, 0, STEADY_FOR_US + LOOK_US);
        assert!(steady.halved(false, now));
        assert!(!steady.halved(true, now));
        // And it takes three steady seconds again after it.
        let later = sit(&mut steady, now, 2 * LOOK_US);
        assert!(!steady.halved(false, later));
    }

    #[test]
    fn with_recognition_only_a_known_face_and_the_same_one_counts() {
        let mut steady = Steady::default();
        let mut now = 0;
        while now <= STEADY_FOR_US {
            steady.look(1, at(0.1, 0.2), true, None, now);
            now += LOOK_US;
        }
        assert!(!steady.halved(false, now));
        let mut now = 0;
        while now <= STEADY_FOR_US {
            steady.look(1, at(0.1, 0.2), true, Some("Minh"), now);
            now += LOOK_US;
        }
        assert!(steady.halved(false, now));
        steady.look(1, at(0.1, 0.2), true, Some("Leo"), now);
        assert!(!steady.halved(false, now));
    }
}
