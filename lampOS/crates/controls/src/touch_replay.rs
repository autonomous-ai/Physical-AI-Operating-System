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
    Privacy {
        sample: lamp_msgs::PrivacySwitch,
    },
    PrivacyFault {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controls: Option<lamp_msgs::ControlsState>,
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
    writer: impl Write,
) -> io::Result<Summary> {
    run_with_privacy(config, clock, records, until_us, writer, false)
}

pub fn run_with_privacy(
    config: TouchConfig,
    clock: &str,
    records: &[Record],
    until_us: Option<u64>,
    mut writer: impl Write,
    privacy_enabled: bool,
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
    let mut controls = privacy_enabled
        .then(|| {
            crate::engine::Engine::new(
                crate::engine::Config {
                    enabled: config.enabled,
                    buttons: config.primary.iter().cloned().collect(),
                    ..Default::default()
                },
                clock,
                "controls-replay",
            )
            .map_err(io::Error::other)
        })
        .transpose()?;
    let mut privacy = privacy_enabled.then(|| crate::privacy::Monitor::new(clock));
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
    let mut last_privacy = None;
    loop {
        while next_record < records.len() && records[next_record].now_us <= now {
            let record = &records[next_record];
            let result = match &record.input {
                Input::Gpio { sample } => {
                    summary.gpio_samples += 1;
                    let result = engine.ingest_gpio(record.now_us, sample.clone());
                    if let Some(controls) = &mut controls {
                        if result.is_ok() {
                            if let Some(name) = controls.config().buttons.first().cloned() {
                                let _ = controls.ingest(
                                    record.now_us,
                                    crate::input::Observation {
                                        stamp: sample.stamp.clone(),
                                        session_id: sample.session_id.clone(),
                                        buttons: vec![crate::input::Level {
                                            name,
                                            active: sample.primary.unwrap_or(false),
                                        }],
                                        head_pads: vec![],
                                        base_mask: None,
                                    },
                                );
                            }
                        } else {
                            controls.input_fault(record.now_us, ControlFault::Invalid);
                        }
                    }
                    result
                }
                Input::Strip { sample } => {
                    summary.strip_samples += 1;
                    engine.ingest_strip(record.now_us, sample.clone())
                }
                Input::GpioFault { fault } => {
                    summary.faults += 1;
                    engine.gpio_fault(record.now_us, *fault);
                    if let Some(controls) = &mut controls {
                        controls.input_fault(record.now_us, *fault);
                    }
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
                    if let Some(controls) = &mut controls {
                        controls.input_fault(record.now_us, *fault);
                    }
                    if let Some(privacy) = &mut privacy {
                        privacy.fault(*fault);
                    }
                    Ok(())
                }
                Input::Privacy { sample } => match &mut privacy {
                    Some(privacy) => privacy.observe(record.now_us, sample.clone()),
                    None => Ok(()),
                },
                Input::PrivacyFault { fault } => {
                    if let Some(privacy) = &mut privacy {
                        privacy.fault(*fault);
                    }
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
        let controls = controls.as_mut().map(|controls| {
            let mut state = controls.tick(now).state;
            state.privacy = privacy.as_mut().map(|privacy| privacy.snapshot(now));
            state
        });
        let privacy_key = controls
            .as_ref()
            .and_then(|state| state.privacy.as_ref())
            .map(|sample| (sample.muted, sample.fault, sample.reader_session_id.clone()));
        summary.ticks += 1;
        summary.events += output.events.len() as u64;
        let semantic = semantic_state(&output.state);
        if !output.events.is_empty()
            || last_frame.is_none_or(|at| now - at >= FRAME_US)
            || last_state.as_ref() != Some(&semantic)
            || last_privacy != privacy_key
            || now == end
        {
            serde_json::to_writer(
                &mut writer,
                &Frame {
                    now_us: now,
                    output,
                    controls,
                },
            )?;
            writer.write_all(b"\n")?;
            summary.frames += 1;
            last_frame = Some(now);
            last_state = Some(semantic);
            last_privacy = privacy_key;
        }
        if now == end {
            break;
        }
        now = now.saturating_add(TICK_US).min(end);
    }
    writer.flush()?;
    Ok(summary)
}

/// No hand inputs means no TouchState is invented. Emit the ordinary Controls
/// replay frames, carrying only independent privacy evidence and hand Missing.
pub fn run_privacy_only(
    config: crate::engine::Config,
    clock: &str,
    records: &[Record],
    until_us: Option<u64>,
    writer: impl Write,
) -> io::Result<crate::replay::Summary> {
    validate(records)?;
    if !config.buttons.is_empty()
        || !config.head_pads.is_empty()
        || config.base_selected_mask.is_some()
    {
        return Err(invalid("privacy-only replay cannot configure hand inputs"));
    }
    let records = records
        .iter()
        .map(|record| {
            let (privacy, privacy_fault) = match &record.input {
                Input::Privacy { sample } => (Some(sample.clone()), None),
                Input::PrivacyFault { fault } | Input::AllFault { fault } => (None, Some(*fault)),
                Input::Tick {} => (None, None),
                _ => {
                    return Err(invalid(
                        "privacy-only replay cannot contain hand acquisitions",
                    ));
                }
            };
            Ok(crate::replay::Record {
                now_us: record.now_us,
                observation: None,
                fault: None,
                privacy,
                privacy_fault,
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    crate::replay::run_with_privacy(config, clock, &records, until_us, writer, true)
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
