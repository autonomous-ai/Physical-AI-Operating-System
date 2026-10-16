//! Bounded, no-sleep replay of independent touch acquisitions. A GPIO sample
//! cannot refresh strip evidence, and neither an absent row nor a fault is a
//! released level. Changed-only hardware logs require explicitly labelled
//! authored timing/continuity reconstruction before becoming these records.

use crate::touch::{GpioSample, StripSample, TouchConfig, TouchEngine, TouchOutput};
use lamp_msgs::{ControlFault, TouchState};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub use crate::replay::{MAX_BYTES, MAX_DURATION_US, MAX_LINE_BYTES, MAX_LINES};
pub const TICK_US: u64 = 2_000;
pub const FRAME_US: u64 = 20_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// Actual receipt time on the same monotonic clock as acquisition stamps.
    /// Written scenarios must explicitly describe any authored clock mapping.
    pub now_us: u64,
    pub input: Input,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    Gpio {
        sample: GpioSample,
    },
    Strip {
        sample: StripSample,
    },
    GpioFault {
        fault: ControlFault,
    },
    StripFault {
        fault: ControlFault,
    },
    AllFault {
        fault: ControlFault,
    },
    Tick {},
    /// The body began or stopped feeling the head pushed by hand
    /// (`body/hand`, #711), as the runtime saw it.
    HeadPushed {
        pushed: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub now_us: u64,
    pub output: TouchOutput,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaultCount {
    pub fault: ControlFault,
    pub count: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub records: usize,
    pub gpio_samples: u64,
    pub strip_samples: u64,
    /// Rejected acquisitions, separate from explicitly injected faults.
    pub rejected: u64,
    pub rejected_by_fault: Vec<FaultCount>,
    /// Explicit fault records. One AllFault counts once, affecting both readers.
    pub faults: u64,
    pub events: u64,
    pub ticks: u64,
    pub frames: u64,
    pub start_us: u64,
    pub end_us: u64,
}

fn invalid(why: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.into())
}

/// Keep malformed acquisition clocks and sequences as evidence for the engine
/// to reject. Only the envelope/schema/resource bounds are parser constraints.
pub fn read(reader: impl Read) -> io::Result<Vec<Record>> {
    let mut bytes = Vec::new();
    reader.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("touch replay exceeds 32 MiB"));
    }
    let text = std::str::from_utf8(&bytes).map_err(|e| invalid(e.to_string()))?;
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if records.len() == MAX_LINES {
            return Err(invalid("touch replay exceeds 100000 records"));
        }
        if line.len() > MAX_LINE_BYTES || line.trim().is_empty() {
            return Err(invalid(format!(
                "line {} is empty or exceeds 16 KiB",
                index + 1
            )));
        }
        records.push(
            serde_json::from_str(line).map_err(|e| invalid(format!("line {}: {e}", index + 1)))?,
        );
    }
    validate(&records)?;
    Ok(records)
}

fn validate(records: &[Record]) -> io::Result<()> {
    if records.is_empty() || records.len() > MAX_LINES {
        return Err(invalid("touch replay needs 1..100000 records"));
    }
    let mut last = records[0].now_us;
    for record in records {
        if record.now_us < last {
            return Err(invalid(
                "touch replay receipt times must not move backwards",
            ));
        }
        last = record.now_us;
    }
    if last - records[0].now_us > MAX_DURATION_US {
        return Err(invalid("touch replay exceeds ten minutes"));
    }
    Ok(())
}

/// The engine runs on a 2 ms authored grid. Each input keeps its exact receipt
/// time and all equal-time inputs are applied, in file order, before that tick.
/// Frames include events and semantic state changes immediately, plus a 20 ms
/// heartbeat and the final frame. Position never advances between acquisitions.
/// This function opens no devices, sockets or files and never sleeps.
pub fn run(
    config: TouchConfig,
    clock: &str,
    records: &[Record],
    until_us: Option<u64>,
    mut writer: impl Write,
) -> io::Result<Summary> {
    validate(records)?;
    let start = records[0].now_us;
    let last = records.last().expect("validated nonempty").now_us;
    let end = until_us.unwrap_or_else(|| {
        last.saturating_add(200_000)
            .min(start.saturating_add(MAX_DURATION_US))
    });
    if end < last || end - start > MAX_DURATION_US || start > u64::MAX - MAX_DURATION_US {
        return Err(invalid(
            "touch replay end must include all input and span at most ten minutes",
        ));
    }
    let mut engine =
        TouchEngine::new(config, clock, "touch-replay").map_err(|e| invalid(e.to_string()))?;
    let mut summary = Summary {
        records: records.len(),
        start_us: start,
        end_us: end,
        ..Summary::default()
    };
    let mut next_record = 0;
    let mut now = start;
    let mut last_frame = None;
    let mut last_state = None;
    loop {
        while next_record < records.len() && records[next_record].now_us <= now {
            let record = &records[next_record];
            let result = match &record.input {
                Input::Gpio { sample } => {
                    summary.gpio_samples += 1;
                    engine.ingest_gpio(record.now_us, sample.clone())
                }
                Input::Strip { sample } => {
                    summary.strip_samples += 1;
                    engine.ingest_strip(record.now_us, sample.clone())
                }
                Input::GpioFault { fault } => {
                    summary.faults += 1;
                    engine.gpio_fault(record.now_us, *fault);
                    Ok(())
                }
                Input::StripFault { fault } => {
                    summary.faults += 1;
                    engine.strip_fault(record.now_us, *fault);
                    Ok(())
                }
                Input::AllFault { fault } => {
                    summary.faults += 1;
                    engine.gpio_fault(record.now_us, *fault);
                    engine.strip_fault(record.now_us, *fault);
                    Ok(())
                }
                Input::Tick {} => Ok(()),
                Input::HeadPushed { pushed } => {
                    engine.set_head_pushed(*pushed);
                    Ok(())
                }
            };
            if let Err(error) = result {
                summary.rejected += 1;
                if let Some(count) = summary
                    .rejected_by_fault
                    .iter_mut()
                    .find(|c| c.fault == error.fault)
                {
                    count.count += 1;
                } else {
                    summary.rejected_by_fault.push(FaultCount {
                        fault: error.fault,
                        count: 1,
                    });
                }
            }
            next_record += 1;
        }
        let output = engine.tick(now);
        summary.ticks += 1;
        summary.events += output.events.len() as u64;
        let semantic = semantic_state(&output.state);
        if !output.events.is_empty()
            || last_frame.is_none_or(|at| now - at >= FRAME_US)
            || last_state.as_ref() != Some(&semantic)
            || now == end
        {
            serde_json::to_writer(
                &mut writer,
                &Frame {
                    now_us: now,
                    output,
                },
            )?;
            writer.write_all(b"\n")?;
            summary.frames += 1;
            last_frame = Some(now);
            last_state = Some(semantic);
        }
        if now == end {
            break;
        }
        now = now.saturating_add(TICK_US).min(end);
    }
    writer.flush()?;
    Ok(summary)
}

// Compare semantic changes without treating every tick's publication sequence
// or an unchanged input's renewed acquisition/hold timer as a spatial change.
// This private comparison value is never published or validated as evidence.
fn semantic_state(state: &TouchState) -> TouchState {
    let mut semantic = state.clone();
    semantic.stamp.at_us = 0;
    semantic.stamp.sequence = 0;
    for source in &mut semantic.sources {
        source.observed_us = None;
        source.held_us = None;
    }
    semantic
}
