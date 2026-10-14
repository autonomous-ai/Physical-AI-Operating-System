//! Pure recognition above the base touch adapter. A mask is a sampled chip
//! status, not a physical touch timestamp. The adapter must report read faults
//! and MPR121 over-current as unknown, never as an artificial release.
//!
//! Keep the useful V1 safeguards: filtered pads, a continuous qualified chord,
//! whole-contact release, raw-edge hold duration, and startup-held suppression.
//! Swipes are deliberately not actions here: travelling contacts are consumed.
use std::fmt;

pub mod strip;

pub const ALL_PADS: u16 = 0x0fff;
pub const MIN_PADS: u32 = 3;
pub const PAD_DEBOUNCE_US: u64 = 5_000;
pub const CONTACT_DEBOUNCE_US: u64 = 10_000;
pub const QUALIFY_US: u64 = 10_000;
pub const RELEASE_US: u64 = 30_000;
pub const AMBIGUOUS_RELEASE_US: u64 = 120_000;
pub const LONG_PRESS_US: u64 = 2_000_000;
pub const DOUBLE_TAP_US: u64 = 400_000;
pub const MAX_SAMPLE_AGE_US: u64 = 100_000;
pub const MAX_SAMPLE_GAP_US: u64 = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseGestureKind {
    Tap,
    DoubleTap,
    LongPress,
}

/// A classification, never permission to change microphone privacy or resume
/// playback. The controls owner determines its one permitted action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseGesture {
    pub kind: BaseGestureKind,
    /// When recognition emitted this event, on the injected arrival clock.
    pub at_us: u64,
    /// The first raw full-release sample of the accepted release interval.
    pub released_us: u64,
    /// Raw accepted press edge to raw release edge, excluding release grace.
    pub duration_us: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseError {
    InvalidSelectedMask,
    UnknownSample,
    InvalidMask,
    ClockBackwards,
    ObservationOutOfOrder,
    FutureSample,
    StaleSample,
    SampleGap,
}

impl fmt::Display for BaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSelectedMask => {
                "base needs at least three selected electrodes in bits 0..11"
            }
            Self::UnknownSample => {
                "base touch sample is unavailable; release must be established again"
            }
            Self::InvalidMask => "base mask has bits outside the twelve touch electrodes",
            Self::ClockBackwards => "base arrival clock ran backwards",
            Self::ObservationOutOfOrder => "base sample timestamp is duplicate or out of order",
            Self::FutureSample => "base sample timestamp is after its arrival",
            Self::StaleSample => "base sample is at least 100 ms old",
            Self::SampleGap => "base samples have a gap over 100 ms",
        })
    }
}

impl std::error::Error for BaseError {}

#[derive(Clone, Copy, Debug, Default)]
struct Pad {
    candidate: bool,
    since_us: u64,
    stable: bool,
}

#[derive(Clone, Copy, Debug)]
struct Contact {
    pressed_us: u64,
    origin: u16,
    seen: u16,
    moved: bool,
    release_us: Option<u64>,
}

/// All storage is fixed: twelve filters, one contact, and one preceding tap.
/// Feed each newly observed mask once. Duplicate timestamps are not fresh
/// observations and disarm recognition just like a missing sample.
#[derive(Debug)]
pub struct BaseRecognizer {
    selected: u16,
    last_now_us: Option<u64>,
    last_observed_us: Option<u64>,
    seeded: bool,
    armed: bool,
    pads: [Pad; 12],
    raw: u16,
    contact_since_us: u64,
    qualified_since_us: Option<u64>,
    qualified: bool,
    contact: Option<Contact>,
    previous_tap_us: Option<u64>,
}

impl BaseRecognizer {
    pub fn new(selected_mask: u16) -> Result<Self, BaseError> {
        if selected_mask & !ALL_PADS != 0 || selected_mask.count_ones() < MIN_PADS {
            return Err(BaseError::InvalidSelectedMask);
        }
        Ok(Self {
            selected: selected_mask,
            last_now_us: None,
            last_observed_us: None,
            seeded: false,
            armed: false,
            pads: [Pad::default(); 12],
            raw: 0,
            contact_since_us: 0,
            qualified_since_us: None,
            qualified: false,
            contact: None,
            previous_tap_us: None,
        })
    }

    /// Cancel all pending recognition and tap history. Clock high-water marks
    /// remain: cancellation must not let an old queued sample become fresh.
    pub fn reset(&mut self) {
        self.seeded = false;
        self.armed = false;
        self.pads = [Pad::default(); 12];
        self.raw = 0;
        self.contact_since_us = 0;
        self.clear_contact();
        self.previous_tap_us = None;
    }

    /// A released baseline has been established since startup or the last
    /// fault. This is recognizer readiness, not voice-listening readiness.
    pub fn ready(&self) -> bool {
        self.seeded && self.armed
    }

    /// Optional noncommitting feedback for a qualified stationary hold. A raw
    /// Only acquisitions advance it; delayed receipt or later ticks cannot
    /// cross a tier. A raw release freezes the duration during release grace.
    /// No action is emitted here, at two seconds, or at any later threshold.
    pub fn hold_us(&self, now_us: u64) -> Option<u64> {
        let observed = self.last_observed_us?;
        let last_now = self.last_now_us?;
        if !self.ready()
            || now_us < last_now
            || now_us.saturating_sub(observed) >= MAX_SAMPLE_AGE_US
            || !self.qualified
        {
            return None;
        }
        let contact = self.contact.as_ref().filter(|contact| !contact.moved)?;
        contact
            .release_us
            .unwrap_or(observed)
            .checked_sub(contact.pressed_us)
    }

    /// `now_us` is the receipt clock; `observed_us` is when the adapter actually
    /// sampled the chip. The same monotonic clock must supply both. A sample
    /// error clears all pending outcomes and requires another stable release.
    pub fn update(
        &mut self,
        now_us: u64,
        observed_us: u64,
        mask: Option<u16>,
    ) -> Result<Option<BaseGesture>, BaseError> {
        let result = self.update_checked(now_us, observed_us, mask);
        if result.is_err() {
            self.reset();
        }
        result
    }

    fn update_checked(
        &mut self,
        now_us: u64,
        observed_us: u64,
        mask: Option<u16>,
    ) -> Result<Option<BaseGesture>, BaseError> {
        let previous_now = self.last_now_us;
        let previous_observed = self.last_observed_us;
        self.last_now_us = Some(previous_now.map_or(now_us, |old| old.max(now_us)));
        if observed_us <= now_us {
            self.last_observed_us =
                Some(previous_observed.map_or(observed_us, |old| old.max(observed_us)));
        }
        if previous_now.is_some_and(|last| now_us < last) {
            return Err(BaseError::ClockBackwards);
        }
        if observed_us > now_us {
            return Err(BaseError::FutureSample);
        }
        if previous_observed.is_some_and(|last| observed_us <= last) {
            return Err(BaseError::ObservationOutOfOrder);
        }
        if now_us - observed_us >= MAX_SAMPLE_AGE_US {
            return Err(BaseError::StaleSample);
        }
        if self.seeded
            && previous_observed.is_some_and(|last| observed_us - last > MAX_SAMPLE_GAP_US)
        {
            return Err(BaseError::SampleGap);
        }
        let mask = mask.ok_or(BaseError::UnknownSample)?;
        if mask & !ALL_PADS != 0 {
            return Err(BaseError::InvalidMask);
        }
        let active = mask & self.selected;
        if !self.seeded {
            self.seeded = true;
            self.raw = active;
            self.contact_since_us = observed_us;
            for (index, pad) in self.pads.iter_mut().enumerate() {
                let touched = active & (1 << index) != 0;
                *pad = Pad {
                    candidate: touched,
                    stable: touched,
                    since_us: observed_us,
                };
            }
            return Ok(None);
        }

        // Resolve a completed prior release before this sample starts another
        // contact. A newly touched pad cannot indefinitely extend old outcomes.
        let mut emitted = None;
        if let Some(contact) = self.contact
            && let Some(released_us) = contact.release_us
        {
            let grace = if self.qualified && !contact.moved {
                RELEASE_US
            } else {
                AMBIGUOUS_RELEASE_US
            };
            if observed_us - released_us >= grace {
                emitted = self.finish(now_us, contact, released_us);
            } else if active != 0 {
                self.contact.as_mut().expect("known contact").release_us = None;
            }
        }

        if (active != 0) != (self.raw != 0) {
            self.contact_since_us = observed_us;
            if active != 0 && self.contact.is_none() {
                self.qualified_since_us = None;
                self.qualified = false;
            }
        }
        self.raw = active;
        let mut stable = 0u16;
        for (index, pad) in self.pads.iter_mut().enumerate() {
            let touched = active & (1 << index) != 0;
            if pad.candidate != touched {
                pad.candidate = touched;
                pad.since_us = observed_us;
            }
            if observed_us - pad.since_us >= PAD_DEBOUNCE_US {
                pad.stable = touched;
            }
            if pad.stable {
                stable |= 1 << index;
            }
        }

        if !self.armed {
            if active == 0
                && stable == 0
                && observed_us - self.contact_since_us >= CONTACT_DEBOUNCE_US
            {
                self.armed = true;
            }
            return Ok(emitted);
        }

        // Close the previous qualifying interval before looking at its falling
        // edge. An irregular poll at 11 ms must not discard a 10 ms chord.
        if self
            .qualified_since_us
            .is_some_and(|since| observed_us - since >= QUALIFY_US)
        {
            self.qualified = true;
        }
        if (stable & active).count_ones() >= MIN_PADS {
            self.qualified_since_us.get_or_insert(observed_us);
        } else {
            self.qualified_since_us = None;
        }

        if self.contact.is_none()
            && active != 0
            && stable != 0
            && observed_us - self.contact_since_us >= CONTACT_DEBOUNCE_US
        {
            self.contact = Some(Contact {
                pressed_us: self.contact_since_us,
                origin: stable,
                seen: stable,
                moved: false,
                release_us: None,
            });
        }
        if let Some(contact) = &mut self.contact {
            // Expansion and shrinking alone are stationary. Only a new
            // filtered arrival after an original pad departed suggests travel.
            let arrivals = stable & !contact.seen;
            let departed = contact.origin & !stable;
            if arrivals != 0 && departed != 0 {
                contact.moved = true;
            }
            contact.seen |= stable;
            if active == 0 {
                contact.release_us.get_or_insert(self.contact_since_us);
            }
        }
        Ok(emitted)
    }

    fn finish(&mut self, now_us: u64, contact: Contact, released_us: u64) -> Option<BaseGesture> {
        let qualified = self.qualified && !contact.moved;
        self.clear_contact();
        if !qualified {
            self.previous_tap_us = None;
            return None;
        }
        let duration_us = released_us - contact.pressed_us;
        let kind = if duration_us >= LONG_PRESS_US {
            self.previous_tap_us = None;
            BaseGestureKind::LongPress
        } else if self.previous_tap_us.is_some_and(|previous| {
            contact.pressed_us >= previous && contact.pressed_us - previous < DOUBLE_TAP_US
        }) {
            self.previous_tap_us = None;
            BaseGestureKind::DoubleTap
        } else {
            self.previous_tap_us = Some(released_us);
            BaseGestureKind::Tap
        };
        Some(BaseGesture {
            kind,
            at_us: now_us,
            released_us,
            duration_us,
        })
    }

    fn clear_contact(&mut self) {
        self.contact = None;
        self.qualified_since_us = None;
        self.qualified = false;
    }
}
