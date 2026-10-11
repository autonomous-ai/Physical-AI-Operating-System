//! Bringing the echo reference onto the capture clock.
//!
//! The speaker and the microphones are separate USB devices with separate
//! clocks. The canceller needs one render block for every capture block, and
//! the speaker accepts a little more or a little less than one in the same
//! time: about 70 to 100 parts per million on the two lamps measured. Counted
//! block against block, that difference uses up any fixed margin within
//! minutes.
//!
//! [`Resampler`] holds accepted render frames in a bounded ring and hands
//! them out one block per capture block, read at a rate a few hundred parts
//! per million either side of one frame per frame. Every accepted sample is
//! used, in order, through a windowed-sinc interpolator; none is dropped or
//! repeated, and no silence is made up. At exactly one frame per frame the
//! output is the input, bit for bit.
//!
//! [`Controller`] chooses that rate from how full the ring is, slowly, and
//! within [`MAX_PPM`]. A clock difference is a slow, steady change and is
//! absorbed. Lost or stalled packets empty the ring at the full audio rate,
//! which the controller cannot and must not follow: the ring then runs dry or
//! over, and the caller reports a fault as it does today.
//!
//! Nothing here reads a clock, allocates after construction or does I/O.
use crate::{RENDER_RATE, RENDER_SAMPLES, RenderBlock};

/// Interpolator length. Sample `CENTRE` of the sixteen is the one at or just
/// before the point being read.
const TAPS: usize = 16;
const CENTRE: usize = 7;
/// Fractional positions the interpolator is tabulated at. Coefficients for a
/// position between two of them are interpolated linearly.
const PHASES: usize = 256;
/// Frames of ring. A power of two, and more than [`CAPACITY_FRAMES`].
const RING: usize = 4_096;

/// The furthest the read rate may be moved from one frame per frame.
pub const MAX_PPM: f64 = 500.0;
/// Frames the controller tries to keep in hand after a block has been taken:
/// five blocks, 50 ms, counting the part of the next packet the speaker has
/// already accepted. The ring itself then holds four and a half blocks on
/// average, and four packets in a row can be late: 40 ms, which is as old as
/// the caller lets a reference be in any case. The fixed margin allowed
/// between one and seven, depending on how far the clocks had drifted.
pub const TARGET_FRAMES: usize = RENDER_SAMPLES * 5;
/// Most frames the ring will hold unread: ten blocks. The five kept in hand,
/// the four that can be waiting when the capture side has been held up for
/// as long as it may be before the oldest of them is past the 40 ms at
/// which a reference is too old to use anyway, and one for the jitter of
/// the fill. With eight, as the fixed margin allowed, a capture loop woken
/// 27 ms late overflowed the ring on its way back: the paced check found
/// that, and no virtual-time test had.
pub const CAPACITY_FRAMES: usize = RENDER_SAMPLES * 10;
/// The most unread frames a block can need: the block at the fastest rate,
/// and the interpolator's reach beyond its last sample.
pub const READY_FRAMES: usize = RENDER_SAMPLES + TAPS - CENTRE + 1;

/// Why a block could not be accepted or taken. Neither is ever papered over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    /// More unread reference than [`CAPACITY_FRAMES`].
    Over,
    /// Fewer unread frames than a block needs.
    Dry,
}

pub struct Resampler {
    ring: Box<[i16; RING]>,
    table: Box<[[f32; TAPS]; PHASES + 1]>,
    /// Frames ever pushed.
    pushed: u64,
    /// Whole part of the position of the next sample to read.
    whole: u64,
    /// Its fractional part, in units of 2^-32 frame.
    fraction: u32,
    /// Advance per output sample, in the same units: 2^32 at nominal rate.
    advance: u64,
    taken_blocks: u64,
}

impl Default for Resampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Resampler {
    pub fn new() -> Self {
        Self {
            ring: Box::new([0; RING]),
            table: kernel(),
            pushed: 0,
            whole: 0,
            fraction: 0,
            advance: 1 << 32,
            taken_blocks: 0,
        }
    }

    /// Forget everything. What was before the first pushed frame is silence,
    /// as it is for a stream that has just been primed.
    pub fn reset(&mut self) {
        self.ring.fill(0);
        self.pushed = 0;
        self.whole = 0;
        self.fraction = 0;
        self.advance = 1 << 32;
        self.taken_blocks = 0;
    }

    /// Frames accepted and not yet read past.
    pub fn unread(&self) -> usize {
        (self.pushed - self.whole) as usize
    }

    /// Whole accepted frames read past so far.
    pub fn consumed(&self) -> u64 {
        self.whole
    }

    pub fn taken_blocks(&self) -> u64 {
        self.taken_blocks
    }

    /// The rate in force, in parts per million above one frame per frame.
    pub fn ppm(&self) -> f64 {
        (self.advance as f64 / 4_294_967_296.0 - 1.0) * 1e6
    }

    /// Set the read rate for the blocks that follow, clamped to [`MAX_PPM`].
    pub fn set_ppm(&mut self, ppm: f64) {
        let ppm = if ppm.is_finite() { ppm } else { 0.0 }.clamp(-MAX_PPM, MAX_PPM);
        self.advance = ((1.0 + ppm * 1e-6) * 4_294_967_296.0).round() as u64;
    }

    /// Accept frames in the order the speaker accepted them.
    pub fn push(&mut self, samples: &[i16]) -> Result<(), Bound> {
        if self.unread() + samples.len() > CAPACITY_FRAMES {
            return Err(Bound::Over);
        }
        for &sample in samples {
            self.ring[self.pushed as usize & (RING - 1)] = sample;
            self.pushed += 1;
        }
        Ok(())
    }

    /// Whether the ring holds what the next block reads. Exactly on the
    /// sample grid at the nominal rate that is the block itself: a stream
    /// that has needed no correction yet gives up its last accepted frame.
    pub fn ready(&self) -> bool {
        let last = u64::from(self.fraction) + self.advance * (RENDER_SAMPLES as u64 - 1);
        let on_grid = self.fraction == 0 && self.advance == 1 << 32;
        let reach = if on_grid { 1 } else { TAPS - CENTRE };
        self.unread() as u64 >= (last >> 32) + reach as u64
    }

    /// One render block on the capture clock. Fails without changing anything
    /// when the ring does not hold enough.
    pub fn take(&mut self, out: &mut RenderBlock) -> Result<(), Bound> {
        if !self.ready() {
            return Err(Bound::Dry);
        }
        for sample in out.iter_mut() {
            *sample = self.read();
            let sum = u64::from(self.fraction) + self.advance;
            self.whole += sum >> 32;
            self.fraction = sum as u32;
        }
        self.taken_blocks += 1;
        Ok(())
    }

    fn read(&self) -> i16 {
        if self.fraction == 0 {
            // Exactly on a sample: the sample itself, so that a reference
            // that needs no correction is passed through untouched.
            return self.at(self.whole);
        }
        let position = self.fraction as f64 / 4_294_967_296.0 * PHASES as f64;
        let phase = position as usize;
        let blend = (position - phase as f64) as f32;
        let (low, high) = (&self.table[phase], &self.table[phase + 1]);
        let mut sum = 0.0_f32;
        for tap in 0..TAPS {
            // Frames before the first one ever pushed are silence.
            let index = (self.whole + tap as u64).wrapping_sub(CENTRE as u64);
            let sample = if index > self.whole + TAPS as u64 {
                0
            } else {
                self.at(index)
            };
            sum += f32::from(sample) * (low[tap] + (high[tap] - low[tap]) * blend);
        }
        sum.round().clamp(-32_768.0, 32_767.0) as i16
    }

    fn at(&self, frame: u64) -> i16 {
        self.ring[frame as usize & (RING - 1)]
    }
}

/// Kaiser-windowed sinc, one row per fractional position from 0 to 1
/// inclusive, each normalised to unit gain at zero frequency.
fn kernel() -> Box<[[f32; TAPS]; PHASES + 1]> {
    let mut table = Box::new([[0.0_f32; TAPS]; PHASES + 1]);
    let half = TAPS as f64 / 2.0;
    for (phase, row) in table.iter_mut().enumerate() {
        let fraction = phase as f64 / PHASES as f64;
        let mut values = [0.0_f64; TAPS];
        for (tap, value) in values.iter_mut().enumerate() {
            let x = tap as f64 - CENTRE as f64 - fraction;
            let sinc = if x.abs() < 1e-12 {
                1.0
            } else {
                (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
            };
            // The window is centred on the point being read.
            let reach = (x / half).abs();
            let window = if reach >= 1.0 {
                0.0
            } else {
                bessel_i0(KAISER_BETA * (1.0 - reach * reach).sqrt()) / bessel_i0(KAISER_BETA)
            };
            *value = sinc * window;
        }
        let gain: f64 = values.iter().sum();
        for (coefficient, value) in row.iter_mut().zip(values) {
            *coefficient = (value / gain) as f32;
        }
    }
    table
}

const KAISER_BETA: f64 = 8.0;

fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, quarter) = (1.0, 1.0, x * x / 4.0);
    for k in 1..40 {
        term *= quarter / (k * k) as f64;
        sum += term;
    }
    sum
}

/// Parts per million of rate asked for per frame the ring is away from its
/// target.
const GAIN_PPM_PER_FRAME: f64 = 1.0;
/// Blocks over which a steady error is integrated away: two minutes.
const INTEGRAL_BLOCKS: f64 = 12_000.0;
/// Blocks the observed fill is averaged over: two seconds. Packets arrive a
/// block at a time, so what one take sees swings by a block.
const SMOOTHING_BLOCKS: f64 = 200.0;

/// Chooses the read rate from the fill seen after each block.
#[derive(Clone, Copy, Debug, Default)]
pub struct Controller {
    smoothed: Option<f64>,
    integral: f64,
    ppm: f64,
}

impl Controller {
    /// Call once per block taken, with [`Resampler::unread`] after it.
    ///
    /// Packets arrive a block at a time, so the count alone is a sawtooth as
    /// wide as a block that repeats as the two clocks slide past each other.
    /// A caller that knows how long ago the last packet was accepted can add
    /// the frames the speaker has accepted since and not yet sent; the
    /// controller then sees a steady figure and holds a steady rate.
    pub fn observe(&mut self, unread_frames: f64) -> f64 {
        let unread = if unread_frames.is_finite() {
            unread_frames.max(0.0)
        } else {
            TARGET_FRAMES as f64
        };
        let smoothed = self.smoothed.get_or_insert(unread);
        *smoothed += (unread - *smoothed) / SMOOTHING_BLOCKS;
        let error = *smoothed - TARGET_FRAMES as f64;
        self.integral =
            (self.integral + error * GAIN_PPM_PER_FRAME / INTEGRAL_BLOCKS).clamp(-MAX_PPM, MAX_PPM);
        self.ppm = (error * GAIN_PPM_PER_FRAME + self.integral).clamp(-MAX_PPM, MAX_PPM);
        self.ppm
    }

    /// The rate last chosen.
    pub fn ppm(&self) -> f64 {
        self.ppm
    }

    /// The part of it that has settled: the controller's estimate of how much
    /// faster the speaker accepts than the microphone delivers.
    pub fn steady_ppm(&self) -> f64 {
        self.integral
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Start the average of the fill again and keep the rate that was
    /// learned: for a ring that was emptied and refilled between the same
    /// two clocks.
    pub fn hold(&mut self) {
        self.smoothed = None;
    }
}

/// Frames a clock difference of `ppm` moves in `seconds`.
pub fn frames_of_skew(ppm: f64, seconds: f64) -> f64 {
    ppm * 1e-6 * seconds * f64::from(RENDER_RATE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(seed: &mut u64) -> i16 {
        *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        (*seed >> 48) as i16
    }

    fn sine(frequency: f64, amplitude: f64, frame: f64) -> f64 {
        amplitude * (2.0 * std::f64::consts::PI * frequency * frame / f64::from(RENDER_RATE)).sin()
    }

    #[test]
    fn at_the_nominal_rate_the_output_is_the_input_bit_for_bit() {
        let mut resampler = Resampler::new();
        let mut seed = 7;
        let input: Vec<i16> = (0..RENDER_SAMPLES * 40).map(|_| noise(&mut seed)).collect();
        let mut output = Vec::new();
        for block in input.chunks(RENDER_SAMPLES) {
            resampler.push(block).unwrap();
            let mut out = [0; RENDER_SAMPLES];
            while resampler.take(&mut out).is_ok() {
                output.extend(out);
            }
        }
        assert_eq!(output, input);
        assert_eq!(resampler.consumed(), output.len() as u64);
        assert_eq!(resampler.ppm(), 0.0);
    }

    /// Feeds a tone, reads it at `ppm`, and returns the error against the
    /// tone evaluated at the positions read, in dB relative to the tone.
    fn tone_error_db(frequency: f64, ppm: f64) -> f64 {
        let mut resampler = Resampler::new();
        resampler.set_ppm(ppm);
        let rate = 1.0 + resampler.ppm() * 1e-6;
        let (mut pushed, mut read) = (0_u64, 0_u64);
        let (mut error, mut power) = (0.0, 0.0);
        let mut out = [0; RENDER_SAMPLES];
        for _ in 0..600 {
            while resampler.unread() + RENDER_SAMPLES <= CAPACITY_FRAMES {
                let block: Vec<i16> = (0..RENDER_SAMPLES as u64)
                    .map(|i| sine(frequency, 12_000.0, (pushed + i) as f64).round() as i16)
                    .collect();
                resampler.push(&block).unwrap();
                pushed += RENDER_SAMPLES as u64;
            }
            resampler.take(&mut out).unwrap();
            for &sample in &out {
                let expected = sine(frequency, 12_000.0, read as f64 * rate);
                if read > 1_000 {
                    error += (f64::from(sample) - expected).powi(2);
                    power += expected * expected;
                }
                read += 1;
            }
        }
        10.0 * (error / power).log10()
    }

    #[test]
    fn a_tone_read_off_rate_stays_a_tone() {
        // What the canceller uses lies below 8 kHz. The floor of -50 dB or
        // so is the rounding of the input and output to 16 bits at this level.
        for ppm in [-500.0, -137.0, 71.0, 300.0, 500.0] {
            for (frequency, limit_db) in [(200.0, -70.0), (1_000.0, -70.0), (3_000.0, -65.0)] {
                let error = tone_error_db(frequency, ppm);
                assert!(
                    error < limit_db,
                    "{frequency} Hz at {ppm} ppm: {error:.1} dB"
                );
            }
            let error = tone_error_db(7_000.0, ppm);
            assert!(error < -45.0, "7 kHz at {ppm} ppm: {error:.1} dB");
        }
    }

    #[test]
    fn every_accepted_frame_is_read_once_at_the_rate_set() {
        // A slow ramp read 400 ppm fast must come out as a ramp that never
        // steps back, and the count of frames read must be the rate set.
        let mut resampler = Resampler::new();
        resampler.set_ppm(400.0);
        let ramp = |frame: u64| (frame as i64 / 64 - 11_000) as i16;
        let (mut pushed, mut previous) = (0_u64, None::<i16>);
        let mut out = [0; RENDER_SAMPLES];
        let mut taken = 0_u64;
        while taken < 6_000 {
            while resampler.unread() + RENDER_SAMPLES <= CAPACITY_FRAMES {
                let block: Vec<i16> = (pushed..pushed + RENDER_SAMPLES as u64).map(ramp).collect();
                resampler.push(&block).unwrap();
                pushed += RENDER_SAMPLES as u64;
            }
            resampler.take(&mut out).unwrap();
            taken += 1;
            for &sample in &out {
                // The first samples ring: before the stream there was silence.
                if let Some(before) = previous.filter(|_| taken > 1) {
                    let step = i32::from(sample) - i32::from(before);
                    assert!((0..=1).contains(&step), "step {step}");
                }
                previous = Some(sample);
            }
        }
        // 6,000 blocks at 400 ppm fast read 576 frames more than they hold.
        let read = resampler.consumed();
        assert!(
            read.abs_diff(6_000 * RENDER_SAMPLES as u64 + 576) <= 1,
            "{read}"
        );
        assert_eq!(pushed - read, resampler.unread() as u64);
        assert_eq!(resampler.taken_blocks(), 6_000);
    }

    #[test]
    fn a_full_ring_and_an_empty_one_are_refused_and_nothing_is_made_up() {
        let mut resampler = Resampler::new();
        let mut out = [5; RENDER_SAMPLES];
        assert_eq!(resampler.take(&mut out), Err(Bound::Dry));
        resampler.push(&[1; RENDER_SAMPLES - 1]).unwrap();
        assert_eq!(resampler.take(&mut out), Err(Bound::Dry));
        assert_eq!(out, [5; RENDER_SAMPLES], "a refusal writes nothing");
        assert_eq!(resampler.consumed(), 0);
        // On the grid at the nominal rate one block is enough for one block.
        resampler.push(&[1]).unwrap();
        resampler.take(&mut out).unwrap();
        assert_eq!((out, resampler.unread()), ([1; RENDER_SAMPLES], 0));
        // Off the nominal rate the interpolator reaches past the block's end.
        resampler.set_ppm(100.0);
        resampler.push(&[1; RENDER_SAMPLES + 7]).unwrap();
        assert_eq!(resampler.take(&mut out), Err(Bound::Dry));
        resampler.push(&[1]).unwrap();
        resampler.take(&mut out).unwrap();
        while resampler.push(&[1; RENDER_SAMPLES]).is_ok() {}
        assert_eq!(resampler.push(&[1; RENDER_SAMPLES]), Err(Bound::Over));
        assert!(resampler.unread() > CAPACITY_FRAMES - RENDER_SAMPLES);
        assert!(resampler.unread() <= CAPACITY_FRAMES);
    }

    #[test]
    fn the_rate_is_clamped_and_a_reset_forgets_the_stream() {
        let mut resampler = Resampler::new();
        resampler.set_ppm(9_000.0);
        assert!((resampler.ppm() - MAX_PPM).abs() < 0.001);
        resampler.set_ppm(f64::NAN);
        assert_eq!(resampler.ppm(), 0.0);
        resampler.set_ppm(-9_000.0);
        assert!((resampler.ppm() + MAX_PPM).abs() < 0.001);
        resampler.push(&[9; 600]).unwrap();
        let mut out = [0; RENDER_SAMPLES];
        resampler.take(&mut out).unwrap();
        resampler.reset();
        assert_eq!(
            (resampler.unread(), resampler.consumed(), resampler.ppm()),
            (0, 0, 0.0)
        );
        assert_eq!(resampler.take(&mut out), Err(Bound::Dry));
    }

    /// A speaker that accepts `skew_ppm` faster than the microphone delivers,
    /// for `blocks` capture blocks, with packets landing anywhere within the
    /// block. Returns the least and most unread frames seen at a take, and
    /// the controller at the end.
    fn run(
        skew_ppm: impl Fn(u64) -> f64,
        blocks: u64,
        samples: bool,
    ) -> (usize, usize, Controller) {
        let mut resampler = Resampler::new();
        let mut controller = Controller::default();
        // The two blocks of the speaker's prime, behind three blocks of the
        // silence that came before it: the ring starts at its target.
        resampler.push(&[0; TARGET_FRAMES]).unwrap();
        let (mut least, mut most) = (usize::MAX, 0);
        let mut owed = 0.0_f64;
        let mut seed = 99;
        let mut out = [0; RENDER_SAMPLES];
        let block = [0; RENDER_SAMPLES];
        for index in 0..blocks {
            owed += RENDER_SAMPLES as f64 * (1.0 + skew_ppm(index) * 1e-6);
            // Jitter: now and then a packet is one take late, then two arrive.
            let late = noise(&mut seed) % 16 == 0 && owed < 2.0 * RENDER_SAMPLES as f64;
            while owed >= RENDER_SAMPLES as f64 && !late {
                resampler.push(&block).expect("ring over");
                owed -= RENDER_SAMPLES as f64;
            }
            if samples {
                resampler.take(&mut out).expect("ring dry");
            } else {
                // The same bookkeeping without the arithmetic on samples.
                assert!(resampler.ready(), "ring dry");
                let sum = u64::from(resampler.fraction) + resampler.advance * RENDER_SAMPLES as u64;
                resampler.whole += sum >> 32;
                resampler.fraction = sum as u32;
            }
            least = least.min(resampler.unread());
            most = most.max(resampler.unread());
            // What a caller with packet times would add: the part of the
            // next packet the speaker has already accepted.
            resampler.set_ppm(controller.observe(resampler.unread() as f64 + owed));
        }
        (least, most, controller)
    }

    #[test]
    fn a_steady_clock_difference_is_learned_and_the_ring_stays_near_its_target() {
        for skew in [-300.0, -100.0, -66.0, 0.0, 98.0, 150.0, 300.0] {
            // Ten minutes.
            let (least, most, controller) = run(|_| skew, 60_000, false);
            // Never dry, or `run` would have stopped. With two packets late
            // in a row the ring may be nearly empty after a take.
            assert!(least > 0, "{skew}: least {least}");
            assert!(most <= RENDER_SAMPLES * 7, "{skew}: most {most}");
            assert!(
                (controller.steady_ppm() - skew).abs() < 25.0,
                "{skew}: learned {:.1}",
                controller.steady_ppm()
            );
        }
    }

    #[test]
    fn the_bookkeeping_used_for_long_runs_is_what_taking_samples_does() {
        let with = run(|_| 98.0, 3_000, true);
        let without = run(|_| 98.0, 3_000, false);
        assert_eq!((with.0, with.1), (without.0, without.1));
        assert_eq!(with.2.ppm(), without.2.ppm());
    }

    #[test]
    fn four_hours_at_the_measured_skew_never_touch_a_bound() {
        // The fixed margin this replaces ran out after about ten minutes.
        // Bookkeeping only, so that it runs in a debug build.
        for skew in [98.0, -98.0] {
            let (least, most, controller) = run(|_| skew, 4 * 360_000, false);
            // Never dry in 1.44 million blocks, with one packet in sixteen
            // late and now and then two in a row.
            assert!(least > 0, "{skew}: least {least}");
            assert!(most <= RENDER_SAMPLES * 7, "{skew}: most {most}");
            assert!((controller.steady_ppm() - skew).abs() < 5.0);
        }
    }

    #[test]
    fn a_change_of_skew_is_followed_without_running_dry_or_over() {
        // A clock that warms up: +120 ppm falling to -80 ppm over a minute.
        let skew = |block: u64| 120.0 - 200.0 * (block.min(6_000) as f64 / 6_000.0);
        let (least, most, controller) = run(skew, 90_000, false);
        assert!(least > 0 && most <= RENDER_SAMPLES * 7, "{least} to {most}");
        assert!((controller.steady_ppm() + 80.0).abs() < 15.0);
    }

    #[test]
    #[should_panic(expected = "ring over")]
    fn a_difference_no_clock_has_is_not_followed() {
        // 2,000 ppm is not a crystal. The rate stops at its limit and the
        // ring fills: the caller's fault, as before.
        run(|_| 2_000.0, 360_000, false);
    }

    #[test]
    #[should_panic(expected = "ring dry")]
    fn lost_packets_run_the_ring_dry_at_once() {
        // Nothing arrives for 50 ms.
        let mut resampler = Resampler::new();
        resampler.push(&[0; RENDER_SAMPLES * 3]).unwrap();
        let mut out = [0; RENDER_SAMPLES];
        for _ in 0..5 {
            resampler.take(&mut out).expect("ring dry");
        }
    }

    #[test]
    fn the_skew_measured_on_the_lamps_is_a_block_every_hundred_seconds() {
        assert!((frames_of_skew(98.0, 102.0) - 239.9).abs() < 0.1);
        // And what a ten-minute session costs a fixed margin of six blocks.
        assert!(frames_of_skew(98.0, 600.0) > RENDER_SAMPLES as f64 * 5.8);
    }
}
