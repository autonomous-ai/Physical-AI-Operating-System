//! Holding the microphone back until the canceller has the render.
//!
//! The canceller can only take out an echo whose render it was given first.
//! The capture-clocked reference keeps five blocks of accepted render in
//! hand, so the canceller is given a render block about 50 ms after the
//! speaker accepted it. Through `mic2` the sound comes back about 130 ms
//! after acceptance and that is early enough. Through `mic1` it comes back
//! after about 30 ms: in the recorded sessions the lamp's voice was in the
//! microphone signal two capture blocks before the canceller had its render,
//! and the canceller removed 2 to 5 dB of it. See
//! `docs/verification/mic1-reference-20261012`.
//!
//! [`CaptureHold`] is what closes that gap: each captured frame waits a fixed
//! number of capture blocks before it is cancelled and handed on. A frame is
//! never altered, reordered or relabelled, and it leaves with the capture
//! times it arrived with: it is older by the hold when it is handed on, and
//! that is the cost. Frames still held when capture ends go with the hold:
//! nothing captured before a stop or a closed microphone is handed on after
//! it.
//!
//! Keeping less render in hand would close the same gap without the delay,
//! and is not what is done: with two blocks in hand in place of five, one
//! late packet runs the ring dry.
use crate::options::{Microphone, ReferenceClock};
use std::collections::VecDeque;

/// Capture blocks a `mic1` frame waits with the capture-clocked reference.
///
/// Three is the fewest at which the recorded `mic1` session is cancelled:
/// 27 dB of the lamp's own voice removed, against 6 to 8 dB with two or
/// fewer. Four leaves one block for what one session does not show.
pub const MIC1_CAPTURE_CLOCKED_BLOCKS: usize = 4;

/// The most blocks any hold may be: 80 ms, under the 100 ms for which the
/// coordinator still takes a captured frame.
pub const MAX_BLOCKS: usize = 8;
const _: () = assert!(MIC1_CAPTURE_CLOCKED_BLOCKS <= MAX_BLOCKS);

/// The hold for a capture device and the way its reference is kept.
///
/// Zero for everything but `mic1` with the capture-clocked reference. With
/// the fixed margin every accepted block goes to the canceller as it arrives,
/// before its sound has left the speaker.
pub fn blocks_for(device: &str, clock: ReferenceClock) -> usize {
    if device == Microphone::Mic1.device() && clock == ReferenceClock::Capture {
        MIC1_CAPTURE_CLOCKED_BLOCKS
    } else {
        0
    }
}

/// Captured frames waiting their turn, oldest first. Bounded at construction;
/// with a hold of zero every frame comes straight back.
pub struct CaptureHold<T> {
    blocks: usize,
    held: VecDeque<T>,
}

impl<T> CaptureHold<T> {
    /// `None` when `blocks` is more than [`MAX_BLOCKS`].
    pub fn new(blocks: usize) -> Option<Self> {
        (blocks <= MAX_BLOCKS).then(|| Self {
            blocks,
            held: VecDeque::with_capacity(blocks + 1),
        })
    }

    pub fn blocks(&self) -> usize {
        self.blocks
    }

    /// Frames waiting.
    pub fn held(&self) -> usize {
        self.held.len()
    }

    /// Take in the frame just captured. Returns the frame captured `blocks`
    /// frames before it, once that many are waiting.
    pub fn push(&mut self, frame: T) -> Option<T> {
        self.held.push_back(frame);
        if self.held.len() > self.blocks {
            self.held.pop_front()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CaptureHold, MAX_BLOCKS, MIC1_CAPTURE_CLOCKED_BLOCKS, blocks_for};
    use crate::options::{Microphone, ReferenceClock};

    #[test]
    fn only_mic1_with_the_capture_clocked_reference_is_held() {
        let (mic1, mic2) = (Microphone::Mic1.device(), Microphone::Mic2.device());
        assert_eq!(blocks_for(mic1, ReferenceClock::Capture), 4);
        assert_eq!(blocks_for(mic1, ReferenceClock::Fixed), 0);
        assert_eq!(blocks_for(mic2, ReferenceClock::Capture), 0);
        assert_eq!(blocks_for(mic2, ReferenceClock::Fixed), 0);
        assert_eq!(blocks_for("hw:1,0", ReferenceClock::Capture), 0);
        assert_eq!(Microphone::default(), Microphone::Mic2);
    }

    #[test]
    fn no_hold_hands_every_frame_straight_back() {
        let mut hold = CaptureHold::new(0).unwrap();
        for frame in 1..=100u64 {
            assert_eq!(hold.push(frame), Some(frame));
            assert_eq!(hold.held(), 0);
        }
    }

    #[test]
    fn a_frame_comes_back_unchanged_after_exactly_the_hold_and_in_order() {
        #[derive(Clone, Debug, PartialEq)]
        struct Frame {
            sequence: u64,
            read_completed_at_us: u64,
            samples: [i16; 4],
        }
        let frame = |sequence: u64| Frame {
            sequence,
            read_completed_at_us: 1_000_000 + sequence * 10_000,
            samples: [sequence as i16; 4],
        };
        let mut hold = CaptureHold::new(MIC1_CAPTURE_CLOCKED_BLOCKS).unwrap();
        assert_eq!(hold.blocks(), 4);
        for sequence in 1..=4 {
            assert_eq!(hold.push(frame(sequence)), None);
        }
        for sequence in 5..=1_000 {
            // The frame four blocks older, with the times it was read at.
            assert_eq!(hold.push(frame(sequence)), Some(frame(sequence - 4)));
            assert_eq!(hold.held(), 4);
        }
    }

    #[test]
    fn a_hold_longer_than_the_bound_is_refused() {
        assert!(CaptureHold::<u64>::new(MAX_BLOCKS).is_some());
        assert!(CaptureHold::<u64>::new(MAX_BLOCKS + 1).is_none());
    }
}
