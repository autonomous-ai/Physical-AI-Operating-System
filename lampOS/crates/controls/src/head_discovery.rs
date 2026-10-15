//! Bounded bench evidence for the four possible head inputs. A driven idle
//! level plus pulses is a candidate, not proof of a physical pad or permission
//! to install a mapping. Nothing here changes live Controls configuration.
use crate::touch_pins::HEAD_CANDIDATES;
use serde::Serialize;
use std::io;

pub const SETTLE_US: u64 = 500_000;
pub const EDGE_US: u64 = 20_000;
pub const FRESH_US: u64 = 100_000;

#[derive(Clone, Debug, Serialize)]
pub struct LineEvidence {
    pub line: u32,
    pub high_with_pull_down: bool,
    pub high_with_pull_up: bool,
    pub idle_high: Option<bool>,
    pub completed_pulses: u64,
    pub longest_pulse_us: u64,
    pub active_low_candidate: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub lines: Vec<LineEvidence>,
    pub observations: u64,
    pub gaps: u64,
    pub started_us: u64,
    pub last_us: Option<u64>,
    pub hardware_mapping_installed: bool,
}

#[derive(Default)]
struct Contact {
    armed: bool,
    released_since: Option<u64>,
    pressed_since: Option<u64>,
    release_edge: Option<u64>,
}

pub struct Discovery {
    report: Report,
    contacts: [Contact; 4],
}

impl Discovery {
    pub fn new(started_us: u64, probes: [(bool, bool); 4]) -> io::Result<Self> {
        if started_us > u64::MAX - 60_000_000 {
            return Err(io::Error::other("discovery clock out of range"));
        }
        Ok(Self {
            report: Report {
                lines: HEAD_CANDIDATES
                    .into_iter()
                    .zip(probes)
                    .map(|(line, (down, up))| LineEvidence {
                        line,
                        high_with_pull_down: down,
                        high_with_pull_up: up,
                        idle_high: (down == up).then_some(up),
                        completed_pulses: 0,
                        longest_pulse_us: 0,
                        active_low_candidate: None,
                    })
                    .collect(),
                observations: 0,
                gaps: 0,
                started_us,
                last_us: None,
                hardware_mapping_installed: false,
            },
            contacts: std::array::from_fn(|_| Contact::default()),
        })
    }

    pub fn ingest(&mut self, at_us: u64, high_mask: u8) -> io::Result<()> {
        if high_mask & !15 != 0
            || at_us < self.report.started_us
            || self.report.last_us.is_some_and(|last| at_us <= last)
        {
            self.contacts = std::array::from_fn(|_| Contact::default());
            return Err(io::Error::other(
                "invalid or unordered head discovery acquisition",
            ));
        }
        if self
            .report
            .last_us
            .is_some_and(|last| at_us - last >= FRESH_US)
        {
            self.report.gaps += 1;
            self.contacts = std::array::from_fn(|_| Contact::default());
        }
        self.report.last_us = Some(at_us);
        self.report.observations += 1;
        for (index, (line, contact)) in self
            .report
            .lines
            .iter_mut()
            .zip(&mut self.contacts)
            .enumerate()
        {
            let Some(idle) = line.idle_high else {
                continue;
            };
            let active = (high_mask & (1 << index) != 0) != idle;
            if at_us - self.report.started_us < SETTLE_US {
                *contact = Contact::default();
                continue;
            }
            if !contact.armed {
                if active {
                    contact.released_since = None;
                } else {
                    let since = *contact.released_since.get_or_insert(at_us);
                    if at_us - since >= EDGE_US {
                        contact.armed = true;
                    }
                }
                continue;
            }
            if active {
                // A bounce interrupts release qualification; its elapsed gap
                // cannot increase the duration of the preceding active pulse.
                if contact.release_edge.take().is_some() {
                    contact.pressed_since = Some(at_us);
                } else {
                    contact.pressed_since.get_or_insert(at_us);
                }
                continue;
            }
            let Some(pressed) = contact.pressed_since else {
                continue;
            };
            let released = *contact.release_edge.get_or_insert(at_us);
            if at_us - released >= EDGE_US {
                let duration = released - pressed;
                if duration >= EDGE_US {
                    line.completed_pulses += 1;
                    line.longest_pulse_us = line.longest_pulse_us.max(duration);
                    if line.completed_pulses >= 2 {
                        line.active_low_candidate = Some(idle);
                    }
                }
                contact.pressed_since = None;
                contact.release_edge = None;
            }
        }
        Ok(())
    }

    pub fn report(&self) -> &Report {
        &self.report
    }
}

#[cfg(target_os = "linux")]
pub fn run(seconds: u64) -> io::Result<Report> {
    use crate::pins::{GpioLines, Lines, Pin, PinsConfig, Pull};
    use std::{thread, time::Duration};
    if !(2..=60).contains(&seconds) {
        return Err(io::Error::other("head discovery lasts 2..60 seconds"));
    }
    let chip = "/dev/gpiochip0";
    let mut probes = [(false, false); 4];
    for (index, line) in HEAD_CANDIDATES.iter().enumerate() {
        probes[index] = GpioLines::probe(chip, *line)?;
    }
    let wiring = PinsConfig {
        privacy: None,
        chip: chip.into(),
        buttons: vec![],
        every_ms: 5,
        head_pads: HEAD_CANDIDATES
            .into_iter()
            .map(|line| Pin {
                name: format!("candidate-{line}"),
                line,
                active_low: false,
                pull: Pull::Up,
            })
            .collect(),
    };
    let mut lines = GpioLines::open(&wiring)?;
    let started = lamp_ipc::monotonic_us();
    let mut discovery = Discovery::new(started, probes)?;
    while lamp_ipc::monotonic_us() - started < seconds * 1_000_000 {
        let before = lamp_ipc::monotonic_us();
        let mut mask = 0;
        for (index, line) in HEAD_CANDIDATES.iter().enumerate() {
            if lines.high(*line)? {
                mask |= 1 << index;
            }
        }
        if lamp_ipc::monotonic_us() - before >= FRESH_US {
            return Err(io::Error::other(
                "head discovery GPIO read took at least 100 ms",
            ));
        }
        discovery.ingest(before, mask)?;
        thread::sleep(Duration::from_millis(5));
    }
    Ok(discovery.report)
}

#[cfg(not(target_os = "linux"))]
pub fn run(_: u64) -> io::Result<Report> {
    Err(io::Error::other("head discovery requires Linux GPIO"))
}
