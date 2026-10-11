//! When a reply's first block may leave its queue for the speaker.
//!
//! The speaker clock never stops: with no speech in hand it accepts zeros. A
//! reply that starts the moment its first audio arrives, and then has to wait
//! for the service's next burst, gets those zeros in the middle of a word. The
//! gate holds the start until the audio in hand carries playout to a fixed
//! horizon after the first audio. It holds nothing after that: a stall later
//! in the reply is not something a start can cover.
//!
//! The opt-in [`Continuity`] start asks for more. The service delivers a reply
//! in bursts at about real time, a little ahead of a playout that began with
//! the first audio, and now and then a burst comes late by more than that
//! lead. A start that waits until playout runs a set margin behind the
//! delivery still has that much in hand when a late burst is due. The lead is
//! measured from the reply's own arrivals, so a reply that already shows the
//! margin is not held for it, and the wait is bounded whatever arrives.
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
    /// Continuity: the time since the first audio plus the measured lead
    /// reached the margin.
    Margin,
    /// Continuity: the hold reached its bound before the arrivals showed the
    /// margin.
    HoldBound,
}

/// What the opt-in continuity start waits for. See `docs/playout-continuity.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Continuity {
    /// How far playout should run behind the service's delivery: the audio
    /// still in hand when a burst is due, and so how late a burst can be
    /// before the speaker runs dry.
    pub margin_us: u64,
    /// The longest the start waits for the arrivals to show that margin. A
    /// start released by this bound has this much, plus whatever lead the
    /// service turns out to keep.
    pub hold_us: u64,
    /// The plain gate's horizon: time since the first audio plus audio in
    /// hand. It decides only when very little has arrived by then.
    pub cover_us: u64,
}

impl Continuity {
    /// Evaluated offline on 23 recorded replies. Not a live default, and not
    /// measured on a device.
    ///
    /// The margin is the largest late burst recorded, 306 ms, with the
    /// withheld block, one 10 ms slot and 14 ms for local hand-over. The hold
    /// is that margin less the smallest lead recorded in a reply that had
    /// audio by then, 44 ms. The cover bridges the longest recorded stall at
    /// a start, 50 ms of audio and then nothing until 349 ms: with 50 ms in
    /// hand it releases the first block at 330 ms.
    pub const EVALUATED: Self = Self {
        margin_us: 340_000,
        hold_us: 290_000,
        cover_us: 380_000,
    };

    /// The longest a start can be held after the first audio once a block
    /// could be handed over.
    pub const fn longest_hold_us(self) -> u64 {
        let shown = if self.hold_us < self.margin_us {
            self.hold_us
        } else {
            self.margin_us
        };
        if shown > self.cover_us {
            shown
        } else {
            self.cover_us
        }
    }
}

/// Arrivals closer together than one playout block are pieces of one burst.
const BURST_GAP_US: u64 = 10_000;

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
    continuity: Option<Continuity>,
    first_audio_us: Option<u64>,
    last_arrival_us: Option<u64>,
    frames: u64,
    lead_us: u64,
    ended: bool,
    opened: Option<Opened>,
}

impl StartGate {
    /// A horizon of zero opens with the first audio.
    pub const fn new(horizon_us: u64) -> Self {
        Self::configured(horizon_us, None)
    }

    /// The opt-in continuity start. Tell it of every arrival with `arrival`,
    /// and of each generation's last block with `generation_ended`.
    pub const fn continuous(policy: Continuity) -> Self {
        Self::configured(policy.cover_us, Some(policy))
    }

    const fn configured(horizon_us: u64, continuity: Option<Continuity>) -> Self {
        Self {
            horizon_us,
            continuity,
            first_audio_us: None,
            last_arrival_us: None,
            frames: 0,
            lead_us: 0,
            ended: false,
            opened: None,
        }
    }

    /// The first call after a reset starts the horizon. Later calls change
    /// nothing a plain gate uses. `arrival` says the same and also counts.
    pub fn audio_arrived(&mut self, at_us: u64) {
        self.arrival(at_us, 0);
    }

    /// Every arrival, with its frame count. The first after a reset starts
    /// the hold, and under continuity so does the first after
    /// `generation_ended`.
    ///
    /// The arrivals also measure the service's lead: the audio received
    /// before a burst, less the time since the first audio. That is what
    /// would still be in hand at that burst had playout begun with the first
    /// audio. The largest value is kept, because a late burst says nothing
    /// about the lead and an early one has not been recorded.
    pub fn arrival(&mut self, at_us: u64, frames: usize) {
        if self.ended {
            *self = Self::configured(self.horizon_us, self.continuity);
        }
        let first = *self.first_audio_us.get_or_insert(at_us);
        if self
            .last_arrival_us
            .is_some_and(|last| at_us.saturating_sub(last) >= BURST_GAP_US)
        {
            let lead_us = rate_us(self.frames).saturating_sub(at_us.saturating_sub(first));
            self.lead_us = self.lead_us.max(lead_us);
        }
        self.frames = self.frames.saturating_add(frames as u64);
        self.last_arrival_us = Some(at_us);
    }

    /// The caller handed over the last block of a generation. Under
    /// continuity the next arrival begins a new start, held like the first.
    /// Audio that arrived before this call is not affected, so a generation
    /// that follows without a break plays without one. A plain gate opens
    /// once for the whole reply and ignores this.
    pub fn generation_ended(&mut self) {
        self.ended = self.continuity.is_some();
    }

    /// Whether a block may be handed over now. Once open it stays open until
    /// `reset`, however little is in hand later, and under continuity until
    /// the first arrival of a later generation.
    ///
    /// Under continuity the start also waits until the time since the first
    /// audio plus the measured lead reaches the margin, or until the hold
    /// reaches its bound. A complete reply still starts at once.
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
        let covered = held_us.saturating_add(frames_us(buffered_frames)) >= self.horizon_us;
        let by = match self.continuity {
            _ if complete => OpenedBy::Complete,
            _ if !covered => return false,
            None => OpenedBy::Covered,
            Some(policy) if held_us.saturating_add(self.lead_us) >= policy.margin_us => {
                OpenedBy::Margin
            }
            Some(policy) if held_us >= policy.hold_us => OpenedBy::HoldBound,
            Some(_) => return false,
        };
        self.opened = Some(Opened {
            at_us: now_us,
            held_us,
            buffered_frames,
            by,
        });
        true
    }

    /// The plain horizon, or the continuity start's cover.
    pub fn horizon_us(&self) -> u64 {
        self.horizon_us
    }

    pub fn continuity(&self) -> Option<Continuity> {
        self.continuity
    }

    /// The lead measured so far in this start. Zero until a burst after the
    /// first has arrived ahead of real time. A plain gate measures it too
    /// when told of arrivals with `arrival`, and does not use it.
    pub fn lead_us(&self) -> u64 {
        self.lead_us
    }

    /// The latest opening. Under continuity it is `None` again from the
    /// first arrival of a later generation until that generation starts.
    pub fn opened(&self) -> Option<Opened> {
        self.opened
    }

    /// For the next reply, and for a reply cancelled while held.
    pub fn reset(&mut self) {
        *self = Self::configured(self.horizon_us, self.continuity);
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

    const POLICY: Continuity = Continuity::EVALUATED;
    // One enqueue of 40 ms, as the provider worker splits a burst.
    const PIECE: usize = 960;

    /// A burst as the queue sees it: 40 ms pieces a quarter of a millisecond apart.
    fn burst(gate: &mut StartGate, at_us: u64, frames: usize) {
        for (piece, start) in (0..frames).step_by(PIECE).enumerate() {
            gate.arrival(at_us + piece as u64 * 250, PIECE.min(frames - start));
        }
    }

    #[test]
    fn a_reply_that_shows_its_lead_starts_as_soon_as_the_margin_is_reached() {
        // The opening of the ocean reply: 160 ms at once, 160 ms more at
        // 168 ms and at 213 ms. The third burst finds 320 ms received 213 ms
        // after the first audio, so the service leads by 107 ms.
        let mut gate = StartGate::continuous(POLICY);
        burst(&mut gate, 0, 3_840);
        assert!(!gate.open(160_000, 3_840, false));
        burst(&mut gate, 168_000, 3_840);
        assert_eq!(gate.lead_us(), 0, "eight milliseconds behind is no lead");
        assert!(!gate.open(212_000, 7_680, false));
        burst(&mut gate, 213_000, 3_840);
        assert_eq!(gate.lead_us(), 107_000);
        // 233 ms held and 107 ms of lead are the 340 ms margin.
        assert!(!gate.open(232_999, 11_520, false));
        assert!(gate.open(233_000, 11_520, false));
        let opened = gate.opened().unwrap();
        assert_eq!((opened.by, opened.held_us), (OpenedBy::Margin, 233_000));
    }

    #[test]
    fn without_a_measured_lead_the_start_is_released_by_the_hold_bound() {
        // The opening of the coffee reply: one frame, then 200 ms of audio
        // 185 ms later and 240 ms more at 307 ms, both behind real time.
        let mut gate = StartGate::continuous(POLICY);
        gate.arrival(0, 1);
        burst(&mut gate, 185_000, 4_800);
        assert!(!gate.open(289_999, 4_801, false));
        assert_eq!(gate.lead_us(), 0);
        assert!(gate.open(290_000, 4_801, false));
        assert_eq!(gate.opened().unwrap().by, OpenedBy::HoldBound);
        // What arrives after the opening is still measured, for the record.
        burst(&mut gate, 307_000, 5_760);
        burst(&mut gate, 395_000, 4_800);
        assert_eq!(gate.lead_us(), 45_041);
    }

    #[test]
    fn a_start_with_almost_nothing_in_hand_waits_for_cover_and_no_longer() {
        // 40 ms arrived and then nothing: past the hold bound, 290 ms held
        // and 40 ms in hand still fall short of the 380 ms cover.
        let mut gate = StartGate::continuous(POLICY);
        burst(&mut gate, 0, PIECE);
        assert!(!gate.open(290_000, PIECE, false));
        assert!(!gate.open(339_999, PIECE, false));
        assert!(gate.open(340_000, PIECE, false));
        // By then the hold alone is the margin, whatever the service leads by.
        assert_eq!(gate.opened().unwrap().by, OpenedBy::Margin);
        // A single frame in hand is released when the cover is reached.
        let mut gate = StartGate::continuous(POLICY);
        gate.arrival(0, 1);
        assert!(!gate.open(379_957, 1, false));
        assert!(gate.open(379_959, 1, false));
        assert_eq!(gate.opened().unwrap().held_us, 379_959);
        assert_eq!(POLICY.longest_hold_us(), 380_000);
    }

    #[test]
    fn a_complete_reply_starts_at_once_under_continuity() {
        let mut gate = StartGate::continuous(POLICY);
        burst(&mut gate, 0, 480);
        assert!(!gate.open(10_000, 480, false));
        assert!(gate.open(10_000, 480, true));
        assert_eq!(gate.opened().unwrap().by, OpenedBy::Complete);
    }

    #[test]
    fn first_audio_that_comes_late_is_not_held_again() {
        // One frame, then the audio 355 ms later: the time already passed is
        // more than the margin and what is in hand covers, so the first block
        // leaves at once.
        let mut gate = StartGate::continuous(POLICY);
        gate.arrival(0, 1);
        assert!(!gate.open(300_000, 1, false), "one frame does not cover");
        burst(&mut gate, 355_000, 5_760);
        assert!(gate.open(355_000, 961, false));
        assert_eq!(gate.opened().unwrap().by, OpenedBy::Margin);
    }

    #[test]
    fn pieces_of_one_burst_are_not_a_lead() {
        // 600 ms delivered as fifteen pieces within four milliseconds. Each
        // piece after the first finds audio received ahead of real time, and
        // none of them is a later burst.
        let mut gate = StartGate::continuous(POLICY);
        burst(&mut gate, 0, 14_400);
        assert_eq!(gate.lead_us(), 0);
        // The next burst, nine milliseconds after the last piece, is still
        // the same one; ten milliseconds after it would be the next.
        gate.arrival(12_500, PIECE);
        assert_eq!(gate.lead_us(), 0);
        gate.arrival(22_500, PIECE);
        assert_eq!(gate.lead_us(), 640_000 - 22_500);
    }

    #[test]
    fn a_late_burst_does_not_lower_the_lead_and_time_running_back_does_not_raise_it() {
        let mut gate = StartGate::continuous(POLICY);
        burst(&mut gate, 0, 4_800);
        burst(&mut gate, 100_000, 4_800);
        assert_eq!(gate.lead_us(), 100_000);
        burst(&mut gate, 700_000, 4_800);
        assert_eq!(gate.lead_us(), 100_000, "300 ms late is not a smaller lead");
        gate.arrival(600_000, PIECE);
        assert_eq!(gate.lead_us(), 100_000);
    }

    #[test]
    fn a_later_generation_is_held_again_only_after_the_one_before_it_ended() {
        let mut gate = StartGate::continuous(POLICY);
        burst(&mut gate, 0, 4_800);
        assert!(gate.open(290_000, 4_800, false));
        // More audio while the first generation is still being played joins it.
        burst(&mut gate, 900_000, 4_800);
        assert!(gate.open(900_000, 4_800, false));
        // Its last block has been handed over. Nothing changes until audio comes.
        gate.generation_ended();
        assert!(gate.open(1_500_000, 0, false));
        assert!(gate.opened().is_some());
        // The story of the six-turn run: 40 ms, then 160 ms more 170 ms later.
        burst(&mut gate, 4_000_000, PIECE);
        assert_eq!(gate.opened(), None);
        assert_eq!(gate.lead_us(), 0, "the lead is measured again");
        assert!(!gate.open(4_000_000, PIECE, false));
        burst(&mut gate, 4_170_000, 3_840);
        assert!(!gate.open(4_289_999, 4_800, false));
        assert!(gate.open(4_290_000, 4_800, false));
        let opened = gate.opened().unwrap();
        assert_eq!((opened.held_us, opened.by), (290_000, OpenedBy::HoldBound));
        // One end is one new start.
        burst(&mut gate, 4_600_000, PIECE);
        assert!(gate.open(4_600_000, PIECE, false));
    }

    #[test]
    fn a_plain_gate_opens_once_for_the_reply_and_only_measures_the_lead() {
        let mut gate = StartGate::new(HORIZON);
        assert_eq!(gate.continuity(), None);
        burst(&mut gate, 0, 3_840);
        // 160 ms in hand covers the 200 ms horizon 40 ms after the first audio.
        assert!(!gate.open(39_999, 3_840, false));
        assert!(gate.open(40_000, 3_840, false));
        assert_eq!(gate.opened().unwrap().by, OpenedBy::Covered);
        burst(&mut gate, 60_000, 3_840);
        assert_eq!(gate.lead_us(), 100_000);
        gate.generation_ended();
        burst(&mut gate, 4_000_000, PIECE);
        assert!(gate.open(4_000_000, PIECE, false));
        assert_eq!(gate.opened().unwrap().at_us, 40_000);
    }

    #[test]
    fn a_reset_during_a_continuity_hold_forgets_the_reply_and_keeps_the_policy() {
        let mut gate = StartGate::continuous(POLICY);
        burst(&mut gate, 0, 4_800);
        burst(&mut gate, 100_000, 4_800);
        assert!(!gate.open(150_000, 9_600, false));
        gate.reset();
        assert_eq!((gate.opened(), gate.lead_us()), (None, 0));
        assert_eq!(gate.continuity(), Some(POLICY));
        assert_eq!(gate.horizon_us(), POLICY.cover_us);
        burst(&mut gate, 1_000_000, 4_800);
        assert!(!gate.open(1_289_999, 4_800, false));
        assert!(gate.open(1_290_000, 4_800, false));
    }

    #[test]
    fn no_setting_holds_a_start_longer_than_it_says() {
        for policy in [
            POLICY,
            Continuity {
                margin_us: 200_000,
                hold_us: 500_000,
                cover_us: 0,
            },
            Continuity {
                margin_us: 0,
                hold_us: 0,
                cover_us: 0,
            },
            Continuity {
                margin_us: u64::MAX,
                hold_us: 150_000,
                cover_us: 100_000,
            },
        ] {
            let bound = policy.longest_hold_us();
            let mut gate = StartGate::continuous(policy);
            gate.arrival(1_000, 1);
            if bound > 0 {
                assert!(!gate.open(1_000 + bound - 42, 1, false), "{policy:?}");
            }
            assert!(gate.open(1_000 + bound, 1, false), "{policy:?}");
        }
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
