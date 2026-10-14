use lamp_msgs::{CONTROL_MAX_BUTTONS, CONTROL_MAX_HEAD_PADS, ClockStamp, ControlFault};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, VecDeque},
    fmt,
};

pub const INPUT_FRESH_US: u64 = 100_000;
pub const MAX_SCRIPTED_OBSERVATIONS: usize = 4096;

/// Logical active polarity, already mapped by an explicitly configured device
/// adapter. Neither hardware pin numbers nor a privacy-switch line belong here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Level {
    pub name: String,
    pub active: bool,
}

/// One complete acquisition of every configured input. Capture/edge time is
/// not replaced with read-call time. An adapter may replay a bounded backlog of
/// complete snapshots, but must report overflow instead of losing transitions.
/// Sequence increases by exactly one for every acquisition in a reader session;
/// a gap disarms recognition even when the remaining levels appear unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub stamp: ClockStamp,
    pub session_id: String,
    pub buttons: Vec<Level>,
    pub head_pads: Vec<Level>,
    pub base_mask: Option<u16>,
}

impl Observation {
    pub fn check(&self) -> Result<(), InputError> {
        self.stamp
            .check()
            .map_err(|e| InputError::new(ControlFault::Invalid, e.to_string()))?;
        check_name(&self.session_id)?;
        check_levels(&self.buttons, CONTROL_MAX_BUTTONS)?;
        check_levels(&self.head_pads, CONTROL_MAX_HEAD_PADS)?;
        if self.base_mask.is_some_and(|mask| mask & !0x0fff != 0) {
            return Err(InputError::new(
                ControlFault::Invalid,
                "base mask exceeds twelve electrodes",
            ));
        }
        Ok(())
    }
}

pub(crate) fn check_name(name: &str) -> Result<(), InputError> {
    if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        return Err(InputError::new(
            ControlFault::Invalid,
            "input names need 1..128 bytes without controls",
        ));
    }
    Ok(())
}

fn check_levels(levels: &[Level], maximum: usize) -> Result<(), InputError> {
    if levels.len() > maximum {
        return Err(InputError::new(
            ControlFault::Overflow,
            "too many physical inputs",
        ));
    }
    let mut names = BTreeSet::new();
    for level in levels {
        check_name(&level.name)?;
        if !names.insert(&level.name) {
            return Err(InputError::new(
                ControlFault::Invalid,
                "duplicate physical input name",
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputError {
    pub fault: ControlFault,
    pub message: String,
}

impl InputError {
    pub fn new(fault: ControlFault, message: impl Into<String>) -> Self {
        Self {
            fault,
            message: message.into(),
        }
    }
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for InputError {}

/// `read` must return immediately. None means no new acquisition, never an
/// implicit released level or a fresh heartbeat. Implementations must bound
/// their queue and return Overflow on lost edges. `now_us` is caller supplied.
pub trait PinReader {
    fn read(&mut self, now_us: u64) -> Result<Option<Observation>, InputError>;
}

/// A reader chosen when the program starts: the board's, or none.
impl<R: PinReader + ?Sized> PinReader for Box<R> {
    fn read(&mut self, now_us: u64) -> Result<Option<Observation>, InputError> {
        (**self).read(now_us)
    }
}

/// A bounded deterministic stand-in. Each call returns at most one acquisition
/// due by `now_us`, preserving its original clock stamp and full levels.
pub struct ScriptedPins {
    samples: VecDeque<Observation>,
    last_now: Option<u64>,
}

impl ScriptedPins {
    pub fn new(samples: impl IntoIterator<Item = Observation>) -> Result<Self, InputError> {
        let mut queue = VecDeque::new();
        let mut previous = None;
        for sample in samples {
            sample.check()?;
            if queue.len() == MAX_SCRIPTED_OBSERVATIONS {
                return Err(InputError::new(
                    ControlFault::Overflow,
                    "script exceeds 4096 observations",
                ));
            }
            if previous.is_some_and(|at| sample.stamp.at_us <= at) {
                return Err(InputError::new(
                    ControlFault::OutOfOrder,
                    "script acquisitions must increase",
                ));
            }
            previous = Some(sample.stamp.at_us);
            queue.push_back(sample);
        }
        Ok(Self {
            samples: queue,
            last_now: None,
        })
    }

    pub fn remaining(&self) -> usize {
        self.samples.len()
    }
}

impl PinReader for ScriptedPins {
    fn read(&mut self, now_us: u64) -> Result<Option<Observation>, InputError> {
        if self.last_now.is_some_and(|last| now_us < last) {
            self.samples.clear();
            return Err(InputError::new(
                ControlFault::ClockReset,
                "stand-in clock moved backwards",
            ));
        }
        self.last_now = Some(now_us);
        if self
            .samples
            .front()
            .is_some_and(|s| s.stamp.at_us <= now_us)
        {
            Ok(self.samples.pop_front())
        } else {
            Ok(None)
        }
    }
}
