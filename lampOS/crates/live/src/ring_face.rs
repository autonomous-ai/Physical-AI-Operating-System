//! The lamp's face: what its 32-pixel ring shows, as a function of what the
//! lamp is doing, how loud the voice in play is, and time. Pure arithmetic:
//! no I/O, no clock of its own and no authority. The ring worker decides
//! whether a frame may be written at all; this only says what it would show.
//!
//! Every pattern here is symmetric around the ring or moves around it, so
//! none depends on which pixel is at the top of the head.
use lamp_ring::{PIXEL_COUNT, Rgb};
use std::f32::consts::TAU;

/// What the lamp is doing, as far as its face is concerned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mood {
    /// Nobody has the lamp's attention: a slow breath.
    Resting,
    /// Someone is speaking to it: open, and moving with their voice.
    Listening,
    /// They have finished and nothing has been said back yet.
    Thinking,
    /// The lamp is speaking: the glow follows its own voice.
    Speaking,
}

/// The quietest and the loudest block that move the face, in dBFS.
const LEVEL_FLOOR_DB: f32 = -48.0;
const LEVEL_FULL_DB: f32 = -14.0;

/// How loud a block of speech is on the face's scale: 0 at or under
/// -48 dBFS, 255 at or over -14 dBFS, by its root mean square.
pub fn level_of(samples: &[i16]) -> u8 {
    if samples.is_empty() {
        return 0;
    }
    let power = samples
        .iter()
        .map(|sample| {
            let value = f32::from(*sample) / 32_768.0;
            value * value
        })
        .sum::<f32>()
        / samples.len() as f32;
    if power <= 0.0 {
        return 0;
    }
    let db = 10.0 * power.log10();
    let scaled = (db - LEVEL_FLOOR_DB) / (LEVEL_FULL_DB - LEVEL_FLOOR_DB);
    (scaled.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A step longer than this between two frames is treated as this long: the
/// face eases on from where it was instead of jumping after a pause.
const LONGEST_STEP_US: u64 = 100_000;
const FIRST_STEP_US: u64 = 20_000;
/// How fast the shown colours follow the wanted ones.
const EASE_US: f32 = 70_000.0;
/// The voice level rises at once and falls slowly, as a mouth would.
const LEVEL_RISE_US: f32 = 20_000.0;
const LEVEL_FALL_US: f32 = 170_000.0;

/// One face. It keeps what is on the ring now, so that a change of mood, or
/// a resting frame after a spoken one, eases from one look into the other.
#[derive(Clone, Debug)]
pub struct Face {
    /// What is shown, per pixel and channel, as a share of the ceiling.
    shown: [[f32; 3]; PIXEL_COUNT],
    last_us: Option<u64>,
    level: f32,
}

impl Default for Face {
    fn default() -> Self {
        Self::new()
    }
}

impl Face {
    pub const fn new() -> Self {
        Self {
            shown: [[0.0; 3]; PIXEL_COUNT],
            last_us: None,
            level: 0.0,
        }
    }

    /// The frame for `now_us`. `level` is the voice in play on the scale of
    /// [`level_of`]: the person's while listening, the lamp's own while
    /// speaking. No channel of the result exceeds `ceiling`.
    pub fn render(
        &mut self,
        mood: Mood,
        level: u8,
        now_us: u64,
        ceiling: u8,
    ) -> [Rgb; PIXEL_COUNT] {
        // The first frame counts as one ordinary step from dark, so that the
        // very first thing shown is already visible.
        let step_us = self.last_us.map_or(FIRST_STEP_US, |last| {
            now_us.saturating_sub(last).min(LONGEST_STEP_US)
        }) as f32;
        self.last_us = Some(now_us);
        let wanted_level = f32::from(level) / 255.0;
        let pace = if wanted_level > self.level {
            LEVEL_RISE_US
        } else {
            LEVEL_FALL_US
        };
        self.level += (wanted_level - self.level) * follow(step_us, pace);
        let seconds = (now_us % 3_600_000_000) as f32 / 1_000_000.0;
        let ease = follow(step_us, EASE_US);
        let mut frame = [Rgb::BLACK; PIXEL_COUNT];
        for (pixel, (shown, out)) in self.shown.iter_mut().zip(&mut frame).enumerate() {
            let angle = TAU * pixel as f32 / PIXEL_COUNT as f32;
            let wanted = look(mood, self.level, seconds, angle);
            for (channel, target) in shown.iter_mut().zip(wanted) {
                *channel += (target - *channel) * ease;
            }
            // A share of the ceiling rarely lands on a whole step. Each pixel
            // rounds at its own fixed point, so a level between two steps is
            // shown as some pixels on the higher one: a dim breath stays
            // smooth instead of stepping, and nothing flickers in time.
            let dither = f32::from(pixel_order(pixel)) / PIXEL_COUNT as f32;
            let step = |share: f32| {
                let value = (share.clamp(0.0, 1.0) * f32::from(ceiling) + dither).floor();
                value.min(f32::from(ceiling)) as u8
            };
            *out = Rgb::new(step(shown[0]), step(shown[1]), step(shown[2]));
        }
        frame
    }
}

/// How much of the way to go in one step for a given pace.
fn follow(step_us: f32, pace_us: f32) -> f32 {
    1.0 - (-step_us / pace_us).exp()
}

/// The pixel's place in a bit-reversed order of the ring, so that pixels
/// that round up first are spread evenly around it.
fn pixel_order(pixel: usize) -> u8 {
    ((pixel as u8) << 3).reverse_bits()
}

/// A wave between 0 and 1 with the given period, at an angle on the ring.
fn wave(seconds: f32, period: f32, lobes: f32, angle: f32) -> f32 {
    0.5 - 0.5 * (TAU * seconds / period - lobes * angle).cos()
}

/// The wanted colour of one pixel, each channel as a share of the ceiling.
fn look(mood: Mood, level: f32, seconds: f32, angle: f32) -> [f32; 3] {
    let (tint, glow) = match mood {
        Mood::Resting => {
            // A warm, low breath, with a soft brighter patch that wanders
            // around the ring so that two breaths never look the same. Low
            // on purpose: on this ring a few steps of 255 are already a
            // clear glow, and V1 rested at one.
            let breath = wave(seconds, 5.2, 0.0, 0.0);
            let patch = wave(seconds, 23.0, 1.0, angle).powi(4);
            ([1.0, 0.62, 0.22], 0.05 + 0.05 * breath + 0.03 * patch)
        }
        Mood::Listening => {
            // Cool and open at once, brighter with the person's voice, with
            // a slow three-lobed shimmer so it is alive when they pause.
            let shimmer = wave(seconds, 1.7, 3.0, angle);
            ([0.50, 0.85, 1.0], 0.42 + 0.40 * level + 0.07 * shimmer)
        }
        Mood::Thinking => {
            // An amber point circling the ring with a fading tail behind it.
            let head = TAU * (seconds / 1.15).fract();
            let behind = (head - angle).rem_euclid(TAU);
            ([1.0, 0.55, 0.10], 0.14 + 0.80 * (-behind / 0.75).exp())
        }
        Mood::Speaking => {
            // Warm, and as bright as the lamp's own voice is loud, with a
            // slow two-lobed turn so that it is never a flat disc.
            let turn = wave(seconds, 3.1, 2.0, angle);
            (
                [1.0, 0.72, 0.30],
                (0.26 + 0.74 * level.powf(0.8)) * (0.90 + 0.10 * turn),
            )
        }
    };
    tint.map(|channel| channel * glow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brightness(frame: &[Rgb; PIXEL_COUNT]) -> u32 {
        frame
            .iter()
            .map(|pixel| u32::from(pixel.red) + u32::from(pixel.green) + u32::from(pixel.blue))
            .sum()
    }

    /// Run a mood for a while at 50 frames a second and return the frames.
    fn play(
        face: &mut Face,
        mood: Mood,
        level: u8,
        from_us: u64,
        frames: u64,
    ) -> Vec<[Rgb; PIXEL_COUNT]> {
        (0..frames)
            .map(|frame| face.render(mood, level, from_us + frame * 20_000, 48))
            .collect()
    }

    #[test]
    fn a_block_of_speech_is_scored_by_its_loudness() {
        assert_eq!(level_of(&[]), 0);
        assert_eq!(level_of(&[0; 240]), 0);
        // About -60 dBFS: under the floor.
        assert_eq!(level_of(&[33; 240]), 0);
        // Full scale: over the top.
        assert_eq!(level_of(&[i16::MAX; 240]), 255);
        // About -30 dBFS sits near the middle and louder is higher.
        let quiet = level_of(&[1_036; 240]);
        let loud = level_of(&[3_277; 240]);
        assert!((100..160).contains(&quiet), "{quiet}");
        assert!(loud > quiet);
    }

    #[test]
    fn no_channel_ever_exceeds_the_ceiling() {
        for ceiling in [0, 1, 12, 48, 120] {
            let mut face = Face::new();
            for mood in [
                Mood::Resting,
                Mood::Listening,
                Mood::Thinking,
                Mood::Speaking,
            ] {
                for frame in 0..200u64 {
                    let pixels = face.render(mood, 255, 1 + frame * 20_000, ceiling);
                    assert!(pixels.iter().all(|pixel| pixel.red <= ceiling
                        && pixel.green <= ceiling
                        && pixel.blue <= ceiling));
                }
            }
        }
    }

    #[test]
    fn a_first_frame_is_already_visible_and_eases_in() {
        let mut face = Face::new();
        let frames = play(&mut face, Mood::Listening, 0, 1_000, 30);
        assert!(brightness(&frames[0]) > 32);
        assert!(brightness(&frames[0]) < brightness(&frames[29]) / 2);
        assert!(
            frames
                .windows(2)
                .take(10)
                .all(|pair| brightness(&pair[1]) >= brightness(&pair[0]))
        );
        assert!(brightness(&frames[29]) > 32 * 30);
    }

    #[test]
    fn the_resting_breath_is_dim_and_moves_in_small_steps() {
        let mut face = Face::new();
        play(&mut face, Mood::Resting, 0, 1_000, 100);
        let frames = play(&mut face, Mood::Resting, 0, 2_001_000, 600);
        let levels: Vec<u32> = frames.iter().map(brightness).collect();
        let (low, high) = (levels.iter().min().unwrap(), levels.iter().max().unwrap());
        // It breathes: a clear difference between its lowest and highest.
        assert!(*high as f32 > *low as f32 * 1.4, "{low} {high}");
        // And never bright: well under a third of the ceiling on every pixel.
        assert!(*high < 32 * 3 * 16, "{high}");
        // From one frame to the next the whole ring changes by a few steps
        // at most, of the 4,608 it has: no visible jump.
        assert!(
            levels
                .windows(2)
                .all(|pair| pair[0].abs_diff(pair[1]) <= 12)
        );
    }

    #[test]
    fn listening_and_speaking_follow_the_voice() {
        for mood in [Mood::Listening, Mood::Speaking] {
            let mut face = Face::new();
            let quiet = play(&mut face, mood, 0, 1_000, 60);
            let loud = play(&mut face, mood, 255, 1_201_000, 60);
            let after = play(&mut face, mood, 0, 2_401_000, 60);
            let (quiet, loud, after) = (
                brightness(quiet.last().unwrap()),
                brightness(loud.last().unwrap()),
                brightness(after.last().unwrap()),
            );
            assert!(loud as f32 > quiet as f32 * 1.5, "{mood:?} {quiet} {loud}");
            assert!(
                (after as f32) < loud as f32 * 0.75,
                "{mood:?} {after} {loud}"
            );
        }
    }

    #[test]
    fn the_voice_shows_within_two_frames_and_fades_over_several() {
        let mut face = Face::new();
        play(&mut face, Mood::Speaking, 0, 1_000, 60);
        let before = brightness(&face.render(Mood::Speaking, 0, 1_201_000, 48));
        face.render(Mood::Speaking, 255, 1_221_000, 48);
        let risen = brightness(&face.render(Mood::Speaking, 255, 1_241_000, 48));
        assert!(risen as f32 > before as f32 * 1.25, "{before} {risen}");
        let peak = brightness(
            play(&mut face, Mood::Speaking, 255, 1_261_000, 30)
                .last()
                .unwrap(),
        );
        let one_later = brightness(&face.render(Mood::Speaking, 0, 1_881_000, 48));
        assert!(one_later as f32 > peak as f32 * 0.8, "{peak} {one_later}");
    }

    #[test]
    fn thinking_is_a_point_that_travels_around_the_ring() {
        let mut face = Face::new();
        let frames = play(&mut face, Mood::Thinking, 0, 1_000, 120);
        let brightest = |frame: &[Rgb; PIXEL_COUNT]| {
            (0..PIXEL_COUNT)
                .max_by_key(|pixel| frame[*pixel].red)
                .unwrap()
        };
        let places: std::collections::BTreeSet<usize> =
            frames[20..].iter().map(brightest).collect();
        // Over two seconds the bright point has been on most of the ring.
        assert!(places.len() > 24, "{}", places.len());
        // At any moment most of the ring is much dimmer than the point.
        let frame = frames[60];
        let peak = frame[brightest(&frame)].red;
        let dim = frame.iter().filter(|pixel| pixel.red < peak / 2).count();
        assert!(dim >= 20, "{dim}");
    }

    #[test]
    fn a_change_of_mood_eases_instead_of_jumping() {
        let mut face = Face::new();
        let speaking = play(&mut face, Mood::Speaking, 255, 1_000, 60);
        let resting = play(&mut face, Mood::Resting, 0, 1_201_000, 60);
        let from = brightness(speaking.last().unwrap());
        let first = brightness(&resting[0]);
        let last = brightness(resting.last().unwrap());
        // The first resting frame is still close to the spoken one.
        assert!(first as f32 > from as f32 * 0.6, "{from} {first}");
        assert!(last < first / 2, "{first} {last}");
    }

    #[test]
    fn a_long_gap_between_frames_does_not_jump() {
        let mut face = Face::new();
        play(&mut face, Mood::Speaking, 255, 1_000, 60);
        let before = brightness(&face.render(Mood::Speaking, 255, 1_201_000, 48));
        // Ten seconds with no frame, then a resting one.
        let after = brightness(&face.render(Mood::Resting, 0, 11_201_000, 48));
        assert!(after as f32 > before as f32 * 0.2, "{before} {after}");
    }

    #[test]
    fn a_level_between_two_steps_lights_some_pixels_one_step_higher() {
        let orders: std::collections::BTreeSet<u8> = (0..PIXEL_COUNT).map(pixel_order).collect();
        assert_eq!(orders.len(), PIXEL_COUNT);
        assert_eq!(*orders.iter().max().unwrap() as usize, PIXEL_COUNT - 1);
        // Neighbours on the ring round at points far apart.
        assert!(
            (0..PIXEL_COUNT - 1)
                .all(|pixel| pixel_order(pixel).abs_diff(pixel_order(pixel + 1)) >= 8)
        );
    }
}
