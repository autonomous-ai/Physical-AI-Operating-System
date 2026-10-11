//! When a reply's first block may leave its queue for the speaker.
//!
//! The speaker clock never stops: with no speech in hand it accepts zeros. A
//! reply that starts the moment its first audio arrives, and then has to wait
//! for the service's next burst, gets those zeros in the middle of a word. The
//! gate holds the start until the audio in hand carries playout to a fixed
//! horizon after the first audio. It holds nothing after that: a stall later
//! in the reply is not something a start can cover.
//!
//! This module decides and measures. It owns no queue, no clock and no
//! permission. The caller keeps the PCM and asks before handing over a block,
//! so held audio has never reached the device and a cancellation during the
//! hold has nothing to flush.
use crate::RENDER_RATE;

/// Previously evaluated experimental horizon; not the live default or a proven winner.
/// See `docs/playout-start.md`.
pub const START_HORIZON_US: u64 = 200_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenedBy {
    /// The time since the first audio plus the audio in hand reached the horizon.
    Covered,
    /// The current generation or terminal output was declared complete.
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opened {
    pub at_us: u64,
    /// Time from the first audio to the opening.
    pub held_us: u64,
    pub buffered_frames: usize,
    pub by: OpenedBy,
}

#[derive(Clone, Copy, Debug)]
pub struct StartGate {
    horizon_us: u64,
    first_audio_us: Option<u64>,
    opened: Option<Opened>,
}

impl StartGate {
    /// A horizon of zero opens with the first audio.
    pub const fn new(horizon_us: u64) -> Self {
        Self {
            horizon_us,
            first_audio_us: None,
            opened: None,
        }
    }

    /// The first call after a reset starts the horizon. Later calls change nothing.
    pub fn audio_arrived(&mut self, at_us: u64) {
        self.first_audio_us.get_or_insert(at_us);
    }

    /// Whether a block may be handed over now. Once open it stays open until
    /// `reset`, however little is in hand later.
    ///
    /// A gate that was never told of an arrival starts its horizon at the
    /// first question asked with audio in hand, so a missed `audio_arrived`
    /// costs one bounded hold and never a silent reply.
    pub fn open(&mut self, now_us: u64, buffered_frames: usize, complete: bool) -> bool {
        if self.opened.is_some() {
            return true;
        }
        if buffered_frames == 0 {
            return false;
        }
        let first = *self.first_audio_us.get_or_insert(now_us);
        let held_us = now_us.saturating_sub(first);
        let by = if complete {
            OpenedBy::Complete
        } else if held_us.saturating_add(frames_us(buffered_frames)) >= self.horizon_us {
            OpenedBy::Covered
        } else {
            return false;
        };
        self.opened = Some(Opened {
            at_us: now_us,
            held_us,
            buffered_frames,
            by,
        });
        true
    }

    pub fn horizon_us(&self) -> u64 {
        self.horizon_us
    }

    pub fn opened(&self) -> Option<Opened> {
        self.opened
    }

    /// For the next reply, and for a reply cancelled while held.
    pub fn reset(&mut self) {
        self.first_audio_us = None;
        self.opened = None;
    }
}

/// What start a reply's own arrivals called for, whatever the gate did.
///
/// Arrivals are counted when the audio became available to the queue. Local
/// hand-over to the speaker is not included.
#[derive(Clone, Copy, Debug, Default)]
pub struct StartNeed {
    first_us: Option<u64>,
    frames: u64,
    deficit_us: u64,
    horizon_us: u64,
}

impl StartNeed {
    pub fn arrival(&mut self, at_us: u64, frames: usize) {
        let first = *self.first_us.get_or_insert(at_us);
        let after_us = at_us.saturating_sub(first);
        let deficit_us = after_us.saturating_sub(rate_us(self.frames));
        if deficit_us > self.deficit_us {
            self.deficit_us = deficit_us;
            self.horizon_us = after_us;
        }
        self.frames = self.frames.saturating_add(frames as u64);
    }

    /// Arrival-only estimate of the initial wait needed for these arrivals.
    /// It excludes the final-block holdback, local handoff and driver timing.
    pub fn deficit_us(&self) -> u64 {
        self.deficit_us
    }

    /// Time from the first audio to the arrival that set the deficit. A gate
    /// with at least this horizon covers the arrival-only deficit, not a
    /// guarantee of playable coverage. Zero when no arrival was late.
    pub fn horizon_us(&self) -> u64 {
        self.horizon_us
    }
}

fn frames_us(frames: usize) -> u64 {
    rate_us(frames as u64)
}

fn rate_us(frames: u64) -> u64 {
    frames.saturating_mul(1_000_000) / u64::from(RENDER_RATE)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HORIZON: u64 = 200_000;
    // 130 ms at 24 kHz.
    const BURST: usize = 3_120;

    #[test]
    fn nothing_starts_before_the_audio_in_hand_reaches_the_horizon() {
        let mut gate = StartGate::new(HORIZON);
        assert!(!gate.open(0, 0, false), "nothing to play is never a start");
        gate.audio_arrived(1_000);
        assert!(!gate.open(1_000, BURST, false));
        // 130 ms in hand covers the horizon 70 ms after the first audio.
        assert!(!gate.open(70_999, BURST, false));
        assert!(gate.open(71_000, BURST, false));
        assert_eq!(
            gate.opened(),
            Some(Opened {
                at_us: 71_000,
                held_us: 70_000,
                buffered_frames: BURST,
                by: OpenedBy::Covered,
            })
        );
    }

    #[test]
    fn a_first_burst_as_long_as_the_horizon_starts_at_once() {
        let mut gate = StartGate::new(HORIZON);
        gate.audio_arrived(5_000);
        assert!(gate.open(5_000, 4_800, false));
        assert_eq!(gate.opened().unwrap().held_us, 0);
    }

    #[test]
    fn a_complete_reply_starts_at_once_however_short() {
        let mut gate = StartGate::new(HORIZON);
        gate.audio_arrived(0);
        assert!(!gate.open(10_000, 480, false));
        assert!(gate.open(10_000, 480, true));
        assert_eq!(gate.opened().unwrap().by, OpenedBy::Complete);
    }

    #[test]
    fn the_hold_never_outlasts_the_horizon() {
        let mut gate = StartGate::new(HORIZON);
        gate.audio_arrived(0);
        assert!(!gate.open(199_958, 1, false));
        assert!(gate.open(200_000, 1, false));
        assert_eq!(gate.opened().unwrap().held_us, HORIZON);
    }

    #[test]
    fn an_open_gate_does_not_close_when_the_queue_runs_low() {
        let mut gate = StartGate::new(HORIZON);
        gate.audio_arrived(0);
        assert!(gate.open(70_000, BURST, false));
        let opened = gate.opened();
        // A stall later in the reply: one frame in hand, nothing complete.
        assert!(gate.open(400_000, 1, false));
        assert!(gate.open(400_000, 0, false));
        assert_eq!(gate.opened(), opened, "the first opening is the record");
    }

    #[test]
    fn a_reset_during_the_hold_leaves_nothing_started_and_arms_the_next_reply() {
        let mut gate = StartGate::new(HORIZON);
        gate.audio_arrived(0);
        assert!(!gate.open(30_000, BURST, false));
        gate.reset();
        assert_eq!(gate.opened(), None);
        // The next reply is measured from its own first audio, not the old one.
        gate.audio_arrived(1_000_000);
        assert!(!gate.open(1_030_000, BURST, false));
        assert!(gate.open(1_070_000, BURST, false));
    }

    #[test]
    fn a_gate_never_told_of_an_arrival_holds_once_and_then_opens() {
        let mut gate = StartGate::new(HORIZON);
        assert!(!gate.open(50_000, 480, false));
        assert!(!gate.open(229_999, 480, false));
        assert!(gate.open(230_000, 480, false));
        assert_eq!(gate.opened().unwrap().held_us, 180_000);
    }

    #[test]
    fn a_clock_reading_before_the_first_audio_is_no_time_held() {
        let mut gate = StartGate::new(HORIZON);
        gate.audio_arrived(10_000);
        assert!(!gate.open(9_000, BURST, false));
        assert!(gate.open(9_000, 4_800, false));
        assert_eq!(gate.opened().unwrap().held_us, 0);
    }

    #[test]
    fn a_zero_horizon_is_no_gate() {
        let mut gate = StartGate::new(0);
        assert!(!gate.open(0, 0, false));
        assert!(gate.open(0, 1, false));
    }

    #[test]
    fn the_need_is_set_by_the_latest_arrival_relative_to_the_audio_before_it() {
        // The first reply of the first Rust pilot, as provider events.
        let mut need = StartNeed::default();
        for (at_us, frames) in [
            (0, 1),
            (14_558, 3_120),
            (171_828, 5_760),
            (275_060, 3_840),
            (435_788, 5_760),
        ] {
            need.arrival(at_us, frames);
        }
        // 3,121 frames are 130,041 us; the third event came 171,828 us in.
        assert_eq!(need.deficit_us(), 41_787);
        assert_eq!(need.horizon_us(), 171_828);
    }

    #[test]
    fn arrivals_that_stay_ahead_of_playout_need_no_hold() {
        let mut need = StartNeed::default();
        for (at_us, frames) in [(0, 4_320), (171_990, 8_640), (335_346, 5_760)] {
            need.arrival(at_us, frames);
        }
        assert_eq!((need.deficit_us(), need.horizon_us()), (0, 0));
    }
}
