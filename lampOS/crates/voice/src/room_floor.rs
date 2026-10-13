//! The level under which sound is not speech, following the room.
//!
//! A fixed floor is right for one room at one hour. On lamp-4ace on
//! 2026-10-13 the talk of a working office reached the lamp at a median of
//! -27 to -35 dBFS and people speaking to it at its desk at -15 to -25: at
//! -40 the lamp heard, lit up for and sometimes answered the whole office,
//! and at -28, set by hand, it stopped. But -28 in an empty room in the
//! evening turns away a quiet voice for nothing.
//!
//! So the floor moves between a lowest and a highest level. It follows how
//! loud the speech is that was not for the lamp: what the detector scores as
//! speech outside any turn (too quiet to begin one, or under the floor
//! already), and what was heard in turns that ended by themselves with the
//! lamp saying nothing. Speech in turns the lamp answered is never counted,
//! nor is anything heard while the lamp itself is speaking, nor a turn that
//! was replaced by more talk (that may be one person pausing). The floor
//! sits three decibels above the middle of that, so that most of such talk
//! falls under it, and goes back to the lowest level when there has been
//! too little of it lately to say.
//!
//! Levels are on the face's scale (`ring_face::level_of`): 0 is -48 dBFS or
//! under, 255 is -14 dBFS or over, 7.5 to a decibel.

/// Where the coordinator reads the highest level the floor may rise to, in
/// dBFS (-47 to -20). The lowest is `--faint-floor-db`. Unset, or not above
/// it: the floor stays where that option puts it.
pub const ENVIRONMENT: &str = "LAMP_FAINT_FLOOR_MAX_DB";

/// The floor for a service given `--faint-floor-db` as `lowest` on the
/// face's scale, with the environment's highest level if there is one.
pub fn from_environment(lowest: u8, now_us: u64) -> RoomFloor {
    let highest = std::env::var(ENVIRONMENT)
        .ok()
        .and_then(|value| value.parse::<i16>().ok())
        .filter(|db| (-47..=-20).contains(db))
        .map_or(lowest, |db| ((db + 48) * 255 / 34) as u8);
    RoomFloor::new(lowest, highest, now_us)
}

/// Where the coordinator reads how long a block under the floor still
/// counts as someone at the lamp after a block over it, in milliseconds
/// (0 to 1,000). Unset or 0: each block is judged alone, as it always was.
pub const HOLD_ENVIRONMENT: &str = "LAMP_FAINT_HOLD_MS";

/// Whether a block is loud enough to be someone at the lamp, judged over a
/// moment and not one block at a time. Speech at a desk has quiet stretches
/// inside it (a stop, a nasal, a trailing syllable): judged block by block,
/// each of those is thrown out as too faint and one short sentence becomes
/// two scraps. A voice across the room has no block over the floor, so it
/// is never let through by this.
#[derive(Debug, Default)]
pub struct Near {
    hold_us: u64,
    last_over_us: Option<u64>,
}

impl Near {
    pub fn new(hold_ms: u64) -> Self {
        Self {
            hold_us: hold_ms.min(1_000) * 1_000,
            last_over_us: None,
        }
    }

    /// For what the environment says.
    pub fn from_environment() -> Self {
        Self::new(
            std::env::var(HOLD_ENVIRONMENT)
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
        )
    }

    /// Whether a block of this level, read at this time, is at or over the
    /// floor or follows closely enough on one that was.
    pub fn hears(&mut self, level: u8, floor: u8, at_us: u64) -> bool {
        if level >= floor {
            self.last_over_us = Some(at_us);
            return true;
        }
        self.hold_us > 0
            && self
                .last_over_us
                .is_some_and(|last| at_us.saturating_sub(last) <= self.hold_us)
    }
}

/// How many blocks of other people's speech, as it fades, there must be to
/// move the floor: about three seconds' worth.
const ENOUGH_BLOCKS: f32 = 300.0;
/// Three decibels above the middle of it.
const ABOVE: u8 = 23;
/// What has been heard fades by half in about two and a half minutes.
const FADE_A_SECOND: f32 = 0.9954;
const BINS: usize = 64;

pub struct RoomFloor {
    lowest: u8,
    highest: u8,
    /// Faded counts of blocks by level, four steps of the scale to a bin.
    heard: [f32; BINS],
    faded_us: u64,
    floor: u8,
    /// Where the room's talk alone would put it; none with too little.
    unbounded: Option<u8>,
}

impl RoomFloor {
    /// A floor that stays between `lowest` and `highest`. Equal: fixed.
    pub fn new(lowest: u8, highest: u8, now_us: u64) -> Self {
        Self {
            lowest,
            highest: highest.max(lowest),
            heard: [0.0; BINS],
            faded_us: now_us,
            floor: lowest,
            unbounded: None,
        }
    }

    /// One 10 ms block of speech, this loud, that was not for the lamp.
    pub fn not_for_the_lamp(&mut self, level: u8) {
        self.heard[usize::from(level) * BINS / 256] += 1.0;
    }

    /// Call often. Fades what was heard once a second and says where the
    /// floor is now.
    pub fn at(&mut self, now_us: u64) -> u8 {
        let seconds = now_us.saturating_sub(self.faded_us) / 1_000_000;
        if seconds > 0 {
            self.faded_us += seconds * 1_000_000;
            let fade = FADE_A_SECOND.powi(seconds.min(3_600) as i32);
            let mut total = 0.0;
            for count in &mut self.heard {
                *count *= fade;
                total += *count;
            }
            self.unbounded = if total < ENOUGH_BLOCKS {
                None
            } else {
                // The level half of it is under.
                let mut under = 0.0;
                let mut middle = 0usize;
                for (bin, count) in self.heard.iter().enumerate() {
                    under += count;
                    if under >= total / 2.0 {
                        middle = bin;
                        break;
                    }
                }
                let level = (middle * 256 / BINS + 256 / BINS / 2) as u8;
                Some(level.saturating_add(ABOVE))
            };
            self.floor = self
                .unbounded
                .map_or(self.lowest, |level| level.clamp(self.lowest, self.highest));
        }
        self.floor
    }

    /// Where the room's talk alone would put the floor, before its lowest
    /// and highest levels are applied; nothing when too little was heard.
    /// For the record: it shows what a wider range would do in a room
    /// before anyone gives it one.
    pub fn unbounded(&self) -> Option<u8> {
        self.unbounded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quiet_stretch_inside_speech_is_heard_for_as_long_as_the_hold_and_no_longer() {
        let floor = 150;
        // Judged alone, as it always was.
        let mut alone = Near::new(0);
        assert!(alone.hears(200, floor, 0));
        assert!(!alone.hears(149, floor, 10_000));
        // Over a moment.
        let mut near = Near::new(400);
        // Nothing has been over the floor yet: a faint voice never is.
        assert!(!near.hears(149, floor, 0));
        assert!(!near.hears(100, floor, 5_000_000));
        assert!(near.hears(150, floor, 6_000_000));
        assert!(near.hears(60, floor, 6_010_000));
        assert!(near.hears(60, floor, 6_400_000));
        assert!(!near.hears(60, floor, 6_400_001));
        // Each block over the floor begins the hold again.
        assert!(near.hears(151, floor, 7_000_000));
        assert!(near.hears(0, floor, 7_399_000));
        // An hour is not a hold.
        assert_eq!(Near::new(3_600_000).hold_us, 1_000_000);
    }

    const S: u64 = 1_000_000;
    /// The face's scale for a level in dBFS.
    fn level(db: f32) -> u8 {
        ((db + 48.0) * 255.0 / 34.0).round() as u8
    }
    fn db(level: u8) -> f32 {
        -48.0 + f32::from(level) * 34.0 / 255.0
    }

    #[test]
    fn it_rises_over_a_room_that_is_talking_and_comes_down_when_the_room_goes_quiet() {
        let mut floor = RoomFloor::new(level(-40.0), level(-26.0), 0);
        let mut now = 0;
        // An empty room: the lowest level.
        assert_eq!(floor.at(60 * S), level(-40.0));
        assert_eq!(floor.unbounded(), None);
        // A second of someone else's talk is not enough to say.
        for _ in 0..100 {
            floor.not_for_the_lamp(level(-31.0));
        }
        now += 61 * S;
        assert_eq!(floor.at(now), level(-40.0));
        // Colleagues talking for a minute, around -31 dBFS: the floor comes
        // to about three decibels over that.
        for second in 0..60 {
            for block in 0..40 {
                floor.not_for_the_lamp(level(-34.0 + (block % 7) as f32));
            }
            now += S;
            floor.at(now);
            let _ = second;
        }
        let risen = db(floor.at(now));
        assert!((-29.5..=-26.5).contains(&risen), "{risen}");
        // It holds while they go on, and never passes its highest.
        for _ in 0..120 {
            for _ in 0..40 {
                floor.not_for_the_lamp(level(-22.0));
            }
            now += S;
            floor.at(now);
        }
        assert_eq!(floor.at(now), level(-26.0));
        // They leave. Twenty minutes later it is back where it began.
        now += 1_200 * S;
        assert_eq!(floor.at(now), level(-40.0));
    }

    #[test]
    fn with_one_level_for_both_ends_it_never_moves() {
        let mut floor = RoomFloor::new(level(-28.0), level(-28.0), 0);
        for second in 1..=300u64 {
            for _ in 0..80 {
                floor.not_for_the_lamp(level(-20.0));
            }
            assert_eq!(floor.at(second * S), level(-28.0));
        }
        // What the room alone would ask for is still known.
        assert!(
            floor
                .unbounded()
                .is_some_and(|wanted| wanted > level(-28.0))
        );
        // A highest under the lowest is the lowest.
        let mut floor = RoomFloor::new(level(-30.0), level(-45.0), 0);
        for _ in 0..1_000 {
            floor.not_for_the_lamp(255);
        }
        assert_eq!(floor.at(5 * S), level(-30.0));
    }
}
