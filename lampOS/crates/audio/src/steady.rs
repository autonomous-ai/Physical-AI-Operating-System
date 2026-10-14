//! A reference that stays put: the lamp's echo held in one place as the
//! echo canceller sees it.
//!
//! The canceller learns where in the speaker's sound the echo in the
//! microphone lies, and its linear filter removes the echo only while it
//! stays there. On lamp-4ace it did not: against the speaker samples handed
//! to the canceller, the lamp's own voice reached the second microphone
//! 91 ms late at one moment and 129 ms late 23 s later, in steps of about
//! 5.4 ms, and then stepped back and forth by one such step every few
//! seconds (docs/duplex-listening.md, 2026-10-14). The first microphone, on
//! the speaker's own card and clock, held its echo to within 0.1 ms. The
//! canceller's own estimate followed, and while it re-learned, its
//! suppressor did the work, and took a person with the echo.
//!
//! [`SteadyReference`] stands between the reference and the canceller. It
//! follows where the echo is from the signals alone, with an
//! [`EchoObserver`] of its own given the reference as it comes and the
//! microphone before cancellation, so it needs nothing from the speaker or
//! the microphone's driver and works whatever moves the echo. Once it has
//! found the echo it delays the reference so that the canceller finds the
//! echo [`HOLD_FRAMES`] after it, and keeps it there as the echo moves. A
//! move of one analysis sample (a quarter of a millisecond, a clock's slow
//! drift) is followed at once; a larger one only once it has held for
//! [`SETTLE_BLOCKS`] blocks, so that a passing match at a voice's pitch
//! period moves nothing. Where it holds the echo depends only on where the
//! echo is found now, so a first match that was wrong costs time and
//! nothing after it is found right.
//!
//! The delay changes smoothly, by at most [`SLEW_FRAMES`] a block, through
//! the windowed-sinc interpolator of [`crate::drift`]: for a block or two
//! after a step the reference is stretched or squeezed by up to a fifth,
//! and no sample is dropped, repeated or made up. Only the canceller and the
//! echo evidence see the delayed reference; the speaker plays what it is
//! given.
//!
//! An echo that comes sooner than [`HOLD_FRAMES`] after the reference is
//! left where it is, with [`MIN_FRAMES`] of delay; one later than
//! [`MAX_FRAMES`] beyond it is held at that.
//!
//! Nothing here reads a clock, allocates after construction or does I/O.
use crate::{
    CaptureBlock, RENDER_SAMPLES, RenderBlock,
    drift::{CENTRE, PHASES, TAPS, kernel},
    evidence::{ANALYSIS_RATE, EchoObserver},
};

/// Render frames per analysis sample of the echo evidence (24 kHz / 4 kHz).
const FRAMES_PER_ANALYSIS: i64 = (crate::RENDER_RATE / ANALYSIS_RATE) as i64;
/// Frames of reference kept: 680 ms, more than [`MAX_FRAMES`] and the
/// interpolator's reach. A power of two.
const RING: usize = 16_384;
/// The least delay ever applied, half a millisecond, so that the
/// interpolator always has the samples it reads after the point it reads.
pub const MIN_FRAMES: f64 = 12.0;
/// Where the canceller is to find the echo: 20 ms after the reference it is
/// given. Far enough that the earliest of the echo's sound, a step of the
/// echo towards the reference before it is followed, and the canceller's
/// own look-back all stay after the reference.
pub const HOLD_FRAMES: f64 = 480.0;
/// The most the reference is ever delayed: 400 ms, the longest delay the
/// echo evidence searches.
pub const MAX_FRAMES: f64 = 9_600.0;
/// The most the delay changes in one 10 ms block: 2 ms, so a step of 5.4 ms
/// is followed within three blocks.
pub const SLEW_FRAMES: f64 = 48.0;
/// Blocks a move of more than one analysis sample must hold before it is
/// followed: 50 ms.
pub const SETTLE_BLOCKS: u16 = 5;

/// What the steady reference is doing, for reports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steadiness {
    /// Delay applied to the last render block, in milliseconds.
    pub delay_ms: f64,
    /// Delay it is moving to, in milliseconds.
    pub target_ms: f64,
    /// Whether the echo has been found since the last reset. Until then the
    /// reference passes with [`MIN_FRAMES`] of delay only.
    pub following: bool,
    /// The echo's delay after the reference as it comes, as followed, in
    /// milliseconds.
    pub echo_ms: Option<f64>,
}

pub struct SteadyReference {
    ring: Box<[i16; RING]>,
    table: Box<[[f32; TAPS]; PHASES + 1]>,
    /// Frames ever pushed.
    pushed: u64,
    /// Delay at the end of the last block, in frames.
    delay: f64,
    target: f64,
    follower: EchoObserver,
    /// The echo's delay after the reference as it comes, in analysis
    /// samples, as followed.
    held: Option<i64>,
    /// A different delay and the blocks it has held.
    candidate: Option<(i64, u16)>,
    out: RenderBlock,
}

impl Default for SteadyReference {
    fn default() -> Self {
        Self::new()
    }
}

impl SteadyReference {
    pub fn new() -> Self {
        Self {
            ring: Box::new([0; RING]),
            table: kernel(),
            pushed: 0,
            delay: MIN_FRAMES,
            target: MIN_FRAMES,
            follower: EchoObserver::new(),
            held: None,
            candidate: None,
            out: [0; RENDER_SAMPLES],
        }
    }

    /// Forget everything, wherever the canceller is reset.
    pub fn reset(&mut self) {
        self.ring.fill(0);
        self.pushed = 0;
        self.delay = MIN_FRAMES;
        self.target = MIN_FRAMES;
        self.follower.reset();
        self.held = None;
        self.candidate = None;
    }

    pub fn steadiness(&self) -> Steadiness {
        let ms = |frames: f64| frames * 1_000.0 / f64::from(crate::RENDER_RATE);
        Steadiness {
            delay_ms: ms(self.delay),
            target_ms: ms(self.target),
            following: self.held.is_some(),
            echo_ms: self
                .held
                .map(|held| ms((held * FRAMES_PER_ANALYSIS) as f64)),
        }
    }

    /// One render block as the speaker accepted it; returns the block the
    /// canceller is to be given in its place.
    pub fn render(&mut self, samples: &RenderBlock) -> &RenderBlock {
        self.follower.render(samples);
        let first = self.pushed;
        for &sample in samples {
            self.ring[self.pushed as usize & (RING - 1)] = sample;
            self.pushed += 1;
        }
        let start = self.delay;
        let end = start + (self.target - start).clamp(-SLEW_FRAMES, SLEW_FRAMES);
        for (index, out) in self.out.iter_mut().enumerate() {
            let delay = start + (end - start) * (index + 1) as f64 / RENDER_SAMPLES as f64;
            let position = (first + index as u64) as f64 - delay;
            *out = read(&self.ring, &self.table, self.pushed, position);
        }
        self.delay = end;
        &self.out
    }

    /// One microphone block before cancellation. Moves the target delay for
    /// the render blocks that follow.
    pub fn capture(&mut self, raw: &CaptureBlock) {
        self.follower.capture(raw, raw);
        if let Some((delay, 0)) = self.follower.echo_delay() {
            self.follow(delay);
        }
    }

    /// The echo's delay after the reference as it comes, in analysis
    /// samples, as the follower confirmed it this block.
    fn follow(&mut self, delay: i64) {
        if let Some(held) = self.held
            && (delay - held).abs() > 1
        {
            let blocks = match self.candidate {
                Some((seen, blocks)) if (seen - delay).abs() <= 1 => blocks + 1,
                _ => 1,
            };
            self.candidate = Some((delay, blocks));
            if blocks < SETTLE_BLOCKS {
                return;
            }
        }
        self.candidate = None;
        self.held = Some(delay);
        self.target =
            ((delay * FRAMES_PER_ANALYSIS) as f64 - HOLD_FRAMES).clamp(MIN_FRAMES, MAX_FRAMES);
    }
}

/// The reference at a fractional frame `position`, which lies at least
/// [`MIN_FRAMES`] before the newest frame and within the ring. Frames before
/// the first one ever pushed are silence.
fn read(ring: &[i16; RING], table: &[[f32; TAPS]; PHASES + 1], pushed: u64, position: f64) -> i16 {
    let whole = position.floor();
    let fraction = position - whole;
    let at = |frame: i64| {
        if frame < 0 || frame as u64 >= pushed || pushed - frame as u64 > RING as u64 {
            0
        } else {
            ring[frame as usize & (RING - 1)]
        }
    };
    if fraction == 0.0 {
        return at(whole as i64);
    }
    let scaled = fraction * PHASES as f64;
    let phase = scaled as usize;
    let blend = (scaled - phase as f64) as f32;
    let (low, high) = (&table[phase], &table[phase + 1]);
    let mut sum = 0.0_f32;
    for tap in 0..TAPS {
        let sample = at(whole as i64 + tap as i64 - CENTRE as i64);
        sum += f32::from(sample) * (low[tap] + (high[tap] - low[tap]) * blend);
    }
    sum.round().clamp(-32_768.0, 32_767.0) as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CAPTURE_SAMPLES;

    fn tone(frame: u64) -> i16 {
        let t = frame as f64 / 24_000.0;
        (8_000.0 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i16
    }

    fn block(first: u64) -> RenderBlock {
        std::array::from_fn(|index| tone(first + index as u64))
    }

    #[test]
    fn before_the_echo_is_found_the_reference_passes_half_a_millisecond_late() {
        let mut steady = SteadyReference::new();
        for index in 0..20u64 {
            let out = *steady.render(&block(index * 240));
            // Twelve whole frames late, exactly.
            for (sample, frame) in out.iter().zip(index * 240..) {
                assert_eq!(*sample, if frame < 12 { 0 } else { tone(frame - 12) });
            }
        }
        let steadiness = steady.steadiness();
        assert!(!steadiness.following);
        assert_eq!(steadiness.delay_ms, 0.5);
    }

    #[test]
    fn a_new_delay_is_reached_at_the_slew_rate_without_a_jump() {
        let mut steady = SteadyReference::new();
        steady.target = 12.0 + 128.0;
        let mut previous = None;
        let mut delays = Vec::new();
        for index in 0..6u64 {
            let out = *steady.render(&block(index * 240));
            delays.push(steady.delay);
            for (sample, frame) in out.iter().zip(index * 240..) {
                if let Some(previous) = previous {
                    // A 440 Hz tone moves at most 8,000 * 2 pi * 440 / 24,000
                    // = 921 a frame; stretched by a fifth, 1,106.
                    let step: i32 = i32::from(*sample) - i32::from(previous);
                    assert!(step.abs() <= 1_110, "frame {frame}: step {step}");
                }
                previous = Some(*sample);
            }
        }
        assert_eq!(delays, [60.0, 108.0, 140.0, 140.0, 140.0, 140.0]);
    }

    #[test]
    fn a_whole_delay_reads_the_reference_exactly_and_a_fractional_one_closely() {
        let mut steady = SteadyReference::new();
        steady.delay = 300.0;
        steady.target = 300.0;
        for index in 0..4u64 {
            let out = *steady.render(&block(index * 240));
            if index > 1 {
                for (sample, frame) in out.iter().zip(index * 240..) {
                    assert_eq!(*sample, tone(frame - 300));
                }
            }
        }
        steady.delay = 300.25;
        steady.target = 300.25;
        let out = *steady.render(&block(4 * 240));
        for (sample, frame) in out.iter().zip(4 * 240..) {
            let t = (frame as f64 - 300.25) / 24_000.0;
            let exact = 8_000.0 * (2.0 * std::f64::consts::PI * 440.0 * t).sin();
            assert!((f64::from(*sample) - exact).abs() < 8.0, "{sample} {exact}");
        }
    }

    #[test]
    fn small_moves_are_followed_at_once_large_ones_once_they_hold() {
        let mut steady = SteadyReference::new();
        // The echo first found 41 ms after the reference: held 20 ms after.
        steady.follow(164);
        assert_eq!(steady.target, 984.0 - HOLD_FRAMES);
        let mut follow = |delay: i64| {
            steady.follow(delay);
            steady.target
        };
        // Drift of one analysis sample: six frames, at once.
        assert_eq!(follow(165), 510.0);
        // A pitch period away for four blocks: nothing.
        for _ in 0..4 {
            assert_eq!(follow(185), 510.0);
        }
        // A step of 5.5 ms that holds: followed on the fifth block.
        for blocks in 1..=5 {
            let target = follow(187);
            assert_eq!(target, if blocks < 5 { 510.0 } else { 642.0 });
        }
    }

    #[test]
    fn an_echo_sooner_than_the_hold_is_left_where_it_is() {
        let mut steady = SteadyReference::new();
        steady.follow(40);
        assert_eq!(steady.target, MIN_FRAMES);
        assert_eq!(steady.steadiness().echo_ms, Some(10.0));
        let mut late = SteadyReference::new();
        late.follow(2_000);
        assert_eq!(late.target, MAX_FRAMES);
    }

    #[test]
    fn reset_forgets_the_place_held() {
        let mut steady = SteadyReference::new();
        steady.held = Some(500);
        steady.delay = 900.0;
        steady.render(&block(0));
        steady.capture(&[0; CAPTURE_SAMPLES]);
        steady.reset();
        assert_eq!(steady.steadiness().delay_ms, 0.5);
        assert!(!steady.steadiness().following);
        assert_eq!(steady.pushed, 0);
    }
}
