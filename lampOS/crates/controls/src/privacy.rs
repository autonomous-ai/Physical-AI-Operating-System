//! Independent, explicitly selected privacy-slide acquisition. LOW is muted.
//! Nothing here opens audio, clears software mute, or recognizes hand gestures.
use crate::{
    input::InputError,
    pins::{Lines, Pull},
};
use lamp_msgs::{ClockStamp, ControlFault, PRIVACY_SWITCH_TTL_US, PrivacySwitch};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, ffi::OsStr};

pub const ENV: &str = "LAMP_PRIVACY_CONTROLS";
pub const POLL_US: u64 = 5_000;
pub const STATE_EVERY_US: u64 = 20_000;
pub const STEP_US: u64 = 1_000;
const MAX_RETIRED: usize = 32;

pub fn selected(value: Option<&OsStr>) -> bool {
    value == Some(OsStr::new("on"))
}

/// Apply only to an explicitly selected privacy route. The normal Bus discovery
/// period can exceed the switch's freshness budget when a subscriber joins late.
pub fn selected_bus(bus: lamp_bus::Bus) -> lamp_bus::Bus {
    bus.rescan_every(std::time::Duration::from_millis(5))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pins {
    pub chip: String,
    pub line: u32,
    pub active_low: bool,
    pub pull: Pull,
}

impl Pins {
    pub fn check(&self) -> Result<(), String> {
        if self.chip != "/dev/gpiochip1"
            || self.line != 9
            || !self.active_low
            || self.pull != Pull::Up
        {
            return Err("privacy is active-low chip 1 line 9 with pull-up".into());
        }
        Ok(())
    }
}

/// At most one actual acquisition per call, without waiting for a transition.
/// None is no acquisition; it never refreshes the last level.
pub trait Reader {
    fn read(&mut self, now_us: u64) -> Option<PrivacySwitch>;
}

pub struct LineReader<L: Lines> {
    lines: L,
    clock: String,
    session: String,
    sequence: u64,
    next_us: u64,
}

impl<L: Lines> LineReader<L> {
    pub fn new(lines: L, clock: &str, session: &str) -> Self {
        Self {
            lines,
            clock: clock.into(),
            session: session.into(),
            sequence: 0,
            next_us: 0,
        }
    }
}

impl<L: Lines> Reader for LineReader<L> {
    fn read(&mut self, now_us: u64) -> Option<PrivacySwitch> {
        if now_us < self.next_us {
            return None;
        }
        self.next_us = now_us.saturating_add(POLL_US);
        let sequence = self.sequence.checked_add(1)?;
        self.sequence = sequence;
        let (muted, fault) = match self.lines.high(9) {
            Ok(high) => (Some(!high), None),
            Err(_) => (None, Some(ControlFault::Missing)),
        };
        Some(PrivacySwitch {
            stamp: ClockStamp {
                clock_id: self.clock.clone(),
                at_us: now_us,
                sequence,
            },
            reader_session_id: self.session.clone(),
            muted,
            fault,
        })
    }
}

/// An unavailable selected chip remains unknown, never an implicit HIGH.
pub struct MissingLine;
impl Lines for MissingLine {
    fn high(&mut self, _: u32) -> std::io::Result<bool> {
        Err(std::io::Error::other(
            "selected privacy line is unavailable",
        ))
    }
}

#[cfg(target_os = "linux")]
pub fn open(pins: &Pins, clock: &str, session: &str) -> Result<Box<dyn Reader>, String> {
    pins.check()?;
    let request = gpiocdev::Request::builder()
        .on_chip(&pins.chip)
        .with_consumer("lampOS-controls-privacy")
        .with_line(pins.line)
        .as_input()
        .with_bias(gpiocdev::line::Bias::PullUp)
        .request();
    struct PrivacyLine(gpiocdev::Request);
    impl Lines for PrivacyLine {
        fn high(&mut self, line: u32) -> std::io::Result<bool> {
            self.0
                .value(line)
                .map(|v| v == gpiocdev::line::Value::Active)
                .map_err(std::io::Error::other)
        }
    }
    Ok(match request {
        Ok(request) => Box::new(LineReader::new(PrivacyLine(request), clock, session)),
        Err(_) => Box::new(LineReader::new(MissingLine, clock, session)),
    })
}

#[cfg(not(target_os = "linux"))]
pub fn open(pins: &Pins, clock: &str, session: &str) -> Result<Box<dyn Reader>, String> {
    pins.check()?;
    Ok(Box::new(LineReader::new(MissingLine, clock, session)))
}

/// The small independent acquisition fence is also used by hardware-free replay.
pub struct Monitor {
    clock: String,
    current: PrivacySwitch,
    reader: Option<String>,
    retired: BTreeSet<String>,
    last: Option<(u64, u64)>,
    now: Option<u64>,
    terminal: Option<ControlFault>,
}

impl Monitor {
    pub fn new(clock: &str) -> Self {
        Self {
            clock: clock.into(),
            current: PrivacySwitch {
                stamp: ClockStamp {
                    clock_id: clock.into(),
                    at_us: 0,
                    sequence: 0,
                },
                reader_session_id: "privacy-unobserved".into(),
                muted: None,
                fault: Some(ControlFault::Missing),
            },
            reader: None,
            retired: BTreeSet::new(),
            last: None,
            now: None,
            terminal: None,
        }
    }

    pub fn fault(&mut self, fault: ControlFault) {
        self.current.muted = None;
        self.current.fault = Some(self.terminal.unwrap_or(fault));
    }

    pub fn clock(&self) -> &str {
        &self.clock
    }

    fn advance(&mut self, now: u64) {
        if self.now.is_some_and(|last| now < last) {
            self.terminal = Some(ControlFault::ClockReset);
        }
        self.now = Some(now);
        if let Some(fault) = self.terminal {
            self.fault(fault);
        } else if self.current.muted.is_some()
            && now.saturating_sub(self.current.stamp.at_us) >= PRIVACY_SWITCH_TTL_US
        {
            self.fault(ControlFault::Stale);
        }
    }

    pub fn observe(&mut self, now: u64, sample: PrivacySwitch) -> Result<(), InputError> {
        self.advance(now);
        let result = self.accept(now, sample);
        if let Err(error) = &result {
            self.fault(error.fault);
        }
        result
    }

    fn accept(&mut self, now: u64, sample: PrivacySwitch) -> Result<(), InputError> {
        let fail = |fault, message| InputError::new(fault, message);
        if let Some(fault) = self.terminal {
            return Err(fail(fault, "privacy fence is terminal"));
        }
        sample
            .check()
            .map_err(|_| fail(ControlFault::Invalid, "invalid privacy acquisition"))?;
        if sample.stamp.sequence == 0 {
            return Err(fail(
                ControlFault::Invalid,
                "privacy acquisition sequence is zero",
            ));
        }
        if sample.stamp.clock_id != self.clock {
            return Err(fail(ControlFault::WrongClock, "privacy boot clock differs"));
        }
        let at = sample.stamp.at_us;
        if at > now {
            return Err(fail(
                ControlFault::Future,
                "privacy acquisition is in the future",
            ));
        }
        if now - at >= PRIVACY_SWITCH_TTL_US {
            return Err(fail(ControlFault::Stale, "privacy acquisition is stale"));
        }
        let changed = self
            .reader
            .as_ref()
            .is_some_and(|old| old != &sample.reader_session_id);
        if self.retired.contains(&sample.reader_session_id) {
            return Err(fail(
                ControlFault::ReaderChanged,
                "privacy reader is retired",
            ));
        }
        if let Some((last_at, sequence)) = self.last {
            if at <= last_at || (!changed && sample.stamp.sequence <= sequence) {
                return Err(fail(
                    ControlFault::OutOfOrder,
                    "privacy acquisition did not advance",
                ));
            }
            if !changed && sequence.checked_add(1) != Some(sample.stamp.sequence) {
                self.last = Some((at, sample.stamp.sequence));
                return Err(fail(
                    ControlFault::Overflow,
                    "privacy acquisition sequence gap",
                ));
            }
            if !changed && at - last_at >= PRIVACY_SWITCH_TTL_US {
                self.last = Some((at, sample.stamp.sequence));
                return Err(fail(
                    ControlFault::Stale,
                    "privacy acquisition continuity was lost",
                ));
            }
        }
        if changed {
            if self.retired.len() == MAX_RETIRED {
                self.terminal = Some(ControlFault::Overflow);
                return Err(fail(
                    ControlFault::Overflow,
                    "privacy reader fence exhausted",
                ));
            }
            self.retired
                .insert(self.reader.take().expect("changed reader"));
        }
        self.reader = Some(sample.reader_session_id.clone());
        self.last = Some((at, sample.stamp.sequence));
        self.current = sample;
        if changed {
            return Err(fail(ControlFault::ReaderChanged, "privacy reader changed"));
        }
        Ok(())
    }

    pub fn snapshot(&mut self, now: u64) -> PrivacySwitch {
        self.advance(now);
        self.current.clone()
    }
}

pub struct Source {
    reader: Box<dyn Reader>,
    pub monitor: Monitor,
}

impl Source {
    pub fn new(reader: Box<dyn Reader>, clock: &str) -> Self {
        Self {
            reader,
            monitor: Monitor::new(clock),
        }
    }

    /// Preserve the original acquisition in records, including rejected input.
    pub fn poll(&mut self, now: impl Fn() -> u64) -> Option<(u64, PrivacySwitch)> {
        let sample = self.reader.read(now())?;
        let received = now();
        let _ = self.monitor.observe(received, sample.clone());
        Some((received, sample))
    }
}
