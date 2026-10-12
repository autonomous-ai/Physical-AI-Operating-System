//! What the lamp's head does for what the lamp is doing: a wanted pose for a
//! mood, a voice level and a time. Pure arithmetic. Limits, speed and whether
//! anything moves at all belong to the engine that asks.
//!
//! A pose is a share of the reach the engine allows around the head's home:
//! 1 is all of it, in pitch, roll and yaw. Positive pitch dips the head;
//! negative lifts it.
use serde::{Deserialize, Serialize};

/// What the lamp is doing, as far as its body is concerned. The same four as
/// its face.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mood {
    /// Nobody has its attention: at rest where it was found.
    #[default]
    Resting,
    /// Someone is speaking to it: head up a little and tilted, as a dog does.
    Listening,
    /// They have finished and it has not answered yet: it looks away and up.
    Thinking,
    /// It is speaking: back on the person, with small nods on its own voice.
    Speaking,
}

/// A wanted offset from home, each axis as a share of its reach, -1 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub pitch: f32,
    pub roll: f32,
    pub yaw: f32,
}

/// The voice level follows the syllables at this pace, and its own average
/// at the slower one; a nod is the difference between the two.
const VOICE_FAST_US: f32 = 90_000.0;
const VOICE_SLOW_US: f32 = 900_000.0;
const LONGEST_STEP_US: u64 = 100_000;

#[derive(Clone, Debug, Default)]
pub struct Gestures {
    mood: Mood,
    /// Counts the times someone began to speak to it, so that a tilt is not
    /// always to the same side.
    exchanges: u32,
    voice: f32,
    voice_average: f32,
    last_us: Option<u64>,
}

impl Gestures {
    pub fn new() -> Self {
        Self::default()
    }

    /// The pose wanted at `now_us`. `level` is how loud the voice in play is,
    /// 0 to 255: the lamp's own while it speaks.
    pub fn target(&mut self, mood: Mood, level: u8, now_us: u64) -> Pose {
        let step_us = self
            .last_us
            .map_or(0, |last| now_us.saturating_sub(last).min(LONGEST_STEP_US))
            as f32;
        self.last_us = Some(now_us);
        if mood != self.mood {
            if mood == Mood::Listening {
                self.exchanges = self.exchanges.wrapping_add(1);
            }
            self.mood = mood;
        }
        let heard = f32::from(level) / 255.0;
        self.voice += (heard - self.voice) * follow(step_us, VOICE_FAST_US);
        self.voice_average += (heard - self.voice_average) * follow(step_us, VOICE_SLOW_US);
        // Listening and the thinking that follows it share one side.
        let side = if self.exchanges % 2 == 1 { 1.0 } else { -1.0 };
        match mood {
            Mood::Resting => Pose::default(),
            Mood::Listening => Pose {
                pitch: -0.4,
                roll: 0.85 * side,
                yaw: 0.0,
            },
            Mood::Thinking => Pose {
                pitch: -0.7,
                roll: -0.4 * side,
                yaw: 0.9 * side,
            },
            Mood::Speaking => {
                // Louder than its recent average: the head dips a little.
                let nod = (self.voice - self.voice_average).clamp(-0.5, 0.5);
                Pose {
                    pitch: -0.2 + 0.5 * nod,
                    roll: 0.0,
                    yaw: 0.0,
                }
            }
        }
    }
}

fn follow(step_us: f32, pace_us: f32) -> f32 {
    1.0 - (-step_us / pace_us).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No mood asks for more than the whole reach. The engine clamps again.
    const REACH: Pose = Pose {
        pitch: 1.0,
        roll: 1.0,
        yaw: 1.0,
    };

    #[test]
    fn every_pose_stays_within_a_few_degrees_of_where_the_head_was_found() {
        let mut gestures = Gestures::new();
        let mut now = 1_000u64;
        for round in 0..40u64 {
            for mood in [
                Mood::Resting,
                Mood::Listening,
                Mood::Thinking,
                Mood::Speaking,
            ] {
                for frame in 0..50u64 {
                    now += 20_000;
                    let level = ((frame * 37 + round * 11) % 256) as u8;
                    let pose = gestures.target(mood, level, now);
                    assert!(pose.pitch.abs() <= REACH.pitch, "{mood:?} {pose:?}");
                    assert!(pose.roll.abs() <= REACH.roll, "{mood:?} {pose:?}");
                    assert!(pose.yaw.abs() <= REACH.yaw, "{mood:?} {pose:?}");
                }
            }
        }
    }

    #[test]
    fn rest_is_exactly_where_it_was_found() {
        let mut gestures = Gestures::new();
        gestures.target(Mood::Speaking, 255, 1_000);
        assert_eq!(gestures.target(Mood::Resting, 255, 21_000), Pose::default());
    }

    #[test]
    fn listening_lifts_and_tilts_and_the_side_alternates_between_exchanges() {
        let mut gestures = Gestures::new();
        let first = gestures.target(Mood::Listening, 0, 1_000);
        assert!(first.pitch < 0.0 && first.roll != 0.0 && first.yaw == 0.0);
        let away = gestures.target(Mood::Thinking, 0, 21_000);
        // It looks away to the side its head was tilted from, and up.
        assert!(away.yaw * first.roll > 0.0 && away.pitch < first.pitch);
        gestures.target(Mood::Speaking, 0, 41_000);
        gestures.target(Mood::Resting, 0, 61_000);
        let second = gestures.target(Mood::Listening, 0, 81_000);
        assert_eq!(second.roll, -first.roll);
    }

    #[test]
    fn speaking_nods_on_the_voice_and_settles_when_it_is_steady() {
        let mut gestures = Gestures::new();
        let mut now = 1_000u64;
        let mut quiet = Pose::default();
        for _ in 0..100 {
            now += 20_000;
            quiet = gestures.target(Mood::Speaking, 0, now);
        }
        // A syllable: the head dips within a tenth of a second.
        let mut loud = quiet;
        for _ in 0..5 {
            now += 20_000;
            loud = gestures.target(Mood::Speaking, 255, now);
        }
        assert!(loud.pitch > quiet.pitch + 0.1, "{quiet:?} {loud:?}");
        // Held loud, it comes back: a nod, not a new posture.
        let mut held = loud;
        for _ in 0..200 {
            now += 20_000;
            held = gestures.target(Mood::Speaking, 255, now);
        }
        assert!(
            (held.pitch - quiet.pitch).abs() < 0.03,
            "{quiet:?} {held:?}"
        );
        assert_eq!((held.roll, held.yaw), (0.0, 0.0));
    }
}
