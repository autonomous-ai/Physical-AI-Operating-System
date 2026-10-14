//! Hand gestures ask the existing owners to stop, close software input, or
//! acknowledge petting. No controls message is a microphone/camera permit.
use crate::{ClockStamp, Invalid};
use lamp_bus::{Declaration, Kind};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const CONTROL_EVENT_TTL_US: u64 = 500_000;
pub const CONTROL_MAX_BUTTONS: usize = 8;
pub const CONTROL_MAX_HEAD_PADS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlSource {
    Button,
    Headpad,
    Base,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gesture {
    Tap,
    DoubleTap,
    MultiTap,
    LongPress,
    Pet,
}

/// One-way requests only. Stop never opens input, SoftwareMute never toggles
/// or clears a mute, and Pet never interrupts a voice turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlIntent {
    Stop,
    SoftwareMute,
    Pet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldTier {
    None,
    Mute,
    ExtendedMute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlFault {
    Missing,
    Stale,
    Future,
    OutOfOrder,
    WrongClock,
    ReaderChanged,
    Overflow,
    Invalid,
    ClockReset,
}

/// Source edge duration is separate from publication time. Sequence is a
/// stable event identity within this controls process session; latest-state
/// delivery carries the very same record, never a newly numbered command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlGesture {
    pub stamp: ClockStamp,
    pub session_id: String,
    pub source: ControlSource,
    pub name: String,
    pub gesture: Gesture,
    pub intent: ControlIntent,
    pub duration_us: u64,
    pub released_us: u64,
    /// False in shadow. Consumers must check this before accepting any intent.
    pub enabled: bool,
}

impl ControlGesture {
    pub const TYPE: &str = "lamp_msgs/ControlGesture";
    pub const VERSION: u32 = 1;
    pub const TOPIC: &str = "controls/gesture";

    pub fn declaration() -> Declaration {
        Declaration::new(Self::TYPE, Self::VERSION, Kind::Event)
            .lasting(Duration::from_micros(CONTROL_EVENT_TTL_US))
    }

    pub fn check(&self) -> Result<(), Invalid> {
        self.stamp.check()?;
        control_name(&self.session_id)?;
        control_name(&self.name)?;
        if self.stamp.sequence == 0
            || self.released_us > self.stamp.at_us
            || self.duration_us > self.released_us
        {
            return Err(Invalid(
                "a gesture needs a nonzero identity and ordered edge/publication times".into(),
            ));
        }
        let valid = match (self.source, self.gesture, self.intent) {
            (ControlSource::Headpad, Gesture::Pet, ControlIntent::Pet) => true,
            (
                ControlSource::Button | ControlSource::Base,
                Gesture::Tap | Gesture::DoubleTap | Gesture::MultiTap,
                ControlIntent::Stop,
            ) => self.duration_us < 2_000_000,
            (
                ControlSource::Button | ControlSource::Base,
                Gesture::LongPress,
                ControlIntent::SoftwareMute,
            ) => self.duration_us >= 2_000_000,
            _ => false,
        };
        if !valid {
            return Err(Invalid(
                "gesture/source/intent do not form a permitted one-way action".into(),
            ));
        }
        Ok(())
    }

    /// The receiver must also fence process sessions and deduplicate sequence.
    /// A stored SoftwareMute remains a user request after the event expires;
    /// its owner must persist an accepted mute independently of this helper.
    pub fn fresh_at(&self, clock_id: &str, now_us: u64) -> bool {
        self.check().is_ok()
            && self.enabled
            && self.stamp.clock_id == clock_id
            && self.stamp.at_us <= now_us
            && now_us - self.stamp.at_us < CONTROL_EVENT_TTL_US
            && (self.intent != ControlIntent::Stop
                || now_us - self.released_us < CONTROL_EVENT_TTL_US)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButtonControlState {
    pub name: String,
    pub pressed: bool,
    /// A released baseline was observed after initialization or the last fault.
    pub armed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held_us: Option<u64>,
    /// Feedback only; neither tier commits an action before release.
    pub tier: HoldTier,
}

impl ButtonControlState {
    pub fn check(&self) -> Result<(), Invalid> {
        control_name(&self.name)?;
        let expected = match self.held_us {
            Some(held) if self.pressed && self.armed => hold_tier(held),
            None => HoldTier::None,
            _ => {
                return Err(Invalid(
                    "held duration needs an armed, pressed button".into(),
                ));
            }
        };
        if expected != self.tier {
            return Err(Invalid(
                "button hold tier disagrees with observed duration".into(),
            ));
        }
        Ok(())
    }
}

pub fn hold_tier(held_us: u64) -> HoldTier {
    if held_us >= 5_000_000 {
        HoldTier::ExtendedMute
    } else if held_us >= 2_000_000 {
        HoldTier::Mute
    } else {
        HoldTier::None
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlsState {
    pub stamp: ClockStamp,
    pub session_id: String,
    pub enabled: bool,
    /// Fresh complete source observations, not microphone readiness.
    pub ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<ControlFault>,
    pub buttons: Vec<ButtonControlState>,
    pub head_active: bool,
    pub base_ready: bool,
    /// Qualified stationary contact duration at the last actual acquisition.
    /// Release grace freezes it; missing input withdraws it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_held_us: Option<u64>,
    pub base_tier: HoldTier,
    /// Latched independently: later pet activity cannot hide a missed Stop or
    /// SoftwareMute. The original event time/sequence is retained on renewal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_stop: Option<ControlGesture>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_mute: Option<ControlGesture>,
    pub event_sequence: u64,
    pub rejected_inputs: u64,
}

impl ControlsState {
    pub const TYPE: &str = "lamp_msgs/ControlsState";
    pub const VERSION: u32 = 1;
    pub const TOPIC: &str = "controls/state";

    pub fn declaration() -> Declaration {
        Declaration::new(Self::TYPE, Self::VERSION, Kind::Latest)
            .every(Duration::from_millis(100))
            .lasting(Duration::from_millis(300))
    }

    pub fn check(&self) -> Result<(), Invalid> {
        self.stamp.check()?;
        control_name(&self.session_id)?;
        if self.ready == self.fault.is_some() || self.buttons.len() > CONTROL_MAX_BUTTONS {
            return Err(Invalid(
                "ready controls have no input fault and at most eight buttons".into(),
            ));
        }
        let mut names = std::collections::BTreeSet::new();
        for button in &self.buttons {
            button.check()?;
            if !names.insert(&button.name) {
                return Err(Invalid("duplicate button status name".into()));
            }
            if !self.ready && (button.armed || button.held_us.is_some()) {
                return Err(Invalid(
                    "faulted controls cannot retain armed hold feedback".into(),
                ));
            }
        }
        if !self.ready && (self.head_active || self.base_ready) {
            return Err(Invalid(
                "faulted controls cannot claim active head or ready base".into(),
            ));
        }
        let expected_base_tier = match self.base_held_us {
            Some(held) if self.base_ready && held <= self.stamp.at_us => hold_tier(held),
            None => HoldTier::None,
            _ => {
                return Err(Invalid(
                    "base hold needs a ready recognizer and an observed duration".into(),
                ));
            }
        };
        if self.base_tier != expected_base_tier {
            return Err(Invalid(
                "base hold tier disagrees with observed duration".into(),
            ));
        }
        for (record, intent) in [
            (&self.latest_stop, ControlIntent::Stop),
            (&self.latest_mute, ControlIntent::SoftwareMute),
        ] {
            if let Some(record) = record {
                record.check()?;
                if record.intent != intent
                    || record.session_id != self.session_id
                    || record.stamp.clock_id != self.stamp.clock_id
                    || record.stamp.at_us > self.stamp.at_us
                    || record.stamp.sequence > self.event_sequence
                    || record.enabled != self.enabled
                {
                    return Err(Invalid(
                        "latched controls command has a mismatched owner, intent or time".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

fn control_name(name: &str) -> Result<(), Invalid> {
    if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        return Err(Invalid(
            "controls identity must have 1..128 bytes without controls".into(),
        ));
    }
    Ok(())
}

pub fn controls_topics() -> Vec<(&'static str, Declaration)> {
    vec![
        (ControlGesture::TOPIC, ControlGesture::declaration()),
        (ControlsState::TOPIC, ControlsState::declaration()),
    ]
}
