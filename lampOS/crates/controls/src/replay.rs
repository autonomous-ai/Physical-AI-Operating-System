use crate::{
    engine::{Config, Engine, Output},
    input::Observation,
    runtime::POLL_US,
};
use lamp_msgs::ControlFault;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub const MAX_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_LINES: usize = 100_000;
pub const MAX_LINE_BYTES: usize = 16 * 1024;
pub const MAX_DURATION_US: u64 = 600_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub now_us: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<Observation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<ControlFault>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy: Option<lamp_msgs::PrivacySwitch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_fault: Option<ControlFault>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub now_us: u64,
    pub output: Output,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Summary {
    pub records: usize,
    pub observations: u64,
    pub rejected: u64,
    pub faults: u64,
    pub gestures: u64,
    pub ticks: u64,
    pub frames: u64,
    pub start_us: u64,
    pub end_us: u64,
}

fn invalid(why: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.into())
}

/// Parse with total, per-line and record-count bounds. Bad acquisition stamps
/// remain in the input so the engine can demonstrate that it rejects them.
pub fn read(reader: impl Read) -> io::Result<Vec<Record>> {
    let mut bytes = Vec::new();
    reader.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("controls replay exceeds 32 MiB"));
    }
    let text = std::str::from_utf8(&bytes).map_err(|e| invalid(e.to_string()))?;
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if records.len() == MAX_LINES {
            return Err(invalid("controls replay exceeds 100000 records"));
        }
        if line.len() > MAX_LINE_BYTES || line.trim().is_empty() {
            return Err(invalid(format!(
                "line {} is empty or exceeds 16 KiB",
                index + 1
            )));
        }
        let record: Record =
            serde_json::from_str(line).map_err(|e| invalid(format!("line {}: {e}", index + 1)))?;
        records.push(record);
    }
    validate(&records)?;
    Ok(records)
}

fn validate(records: &[Record]) -> io::Result<()> {
    if records.is_empty() || records.len() > MAX_LINES {
        return Err(invalid("replay needs 1..100000 records"));
    }
    let mut last = records[0].now_us;
    for record in records {
        if record.now_us < last {
            return Err(invalid("replay receipt times must not move backwards"));
        }
        if record.observation.is_some() && record.fault.is_some() {
            return Err(invalid(
                "one record cannot be both an acquisition and an adapter fault",
            ));
        }
        if record.privacy.is_some()
            && (record.privacy_fault.is_some()
                || record.observation.is_some()
                || record.fault.is_some())
        {
            return Err(invalid("privacy acquisition must have its own record"));
        }
        last = record.now_us;
    }
    if last - records[0].now_us > MAX_DURATION_US {
        return Err(invalid("replay exceeds ten minutes"));
    }
    Ok(())
}

/// Inject acquisition and receipt clocks without sleeping, opening a bus, or
/// touching hardware. Full observations are required; silence is not a fresh
/// released level. Output is limited to 10 Hz, changes and actual gestures.
pub fn run(
    config: Config,
    clock: &str,
    records: &[Record],
    until_us: Option<u64>,
    writer: impl Write,
) -> io::Result<Summary> {
    run_with_privacy(config, clock, records, until_us, writer, false)
}

pub fn run_with_privacy(
    config: Config,
    clock: &str,
    records: &[Record],
    until_us: Option<u64>,
    mut writer: impl Write,
    privacy_enabled: bool,
) -> io::Result<Summary> {
    validate(records)?;
    let start = records[0].now_us;
    let last = records.last().unwrap().now_us;
    let end = until_us.unwrap_or_else(|| {
        last.saturating_add(200_000)
            .min(start.saturating_add(MAX_DURATION_US))
    });
    if end < last || end - start > MAX_DURATION_US || start > u64::MAX - MAX_DURATION_US {
        return Err(invalid(
            "replay end must include all input and span at most ten minutes",
        ));
    }
    let mut engine =
        Engine::new(config, clock, "controls-replay").map_err(|e| invalid(e.to_string()))?;
    let mut privacy = privacy_enabled.then(|| crate::privacy::Monitor::new(clock));
    let mut summary = Summary {
        records: records.len(),
        start_us: start,
        end_us: end,
        ..Summary::default()
    };
    let mut next_record = 0;
    let mut now = start;
    let mut last_frame = None;
    let mut last_fault = None;
    let mut last_privacy = None;
    loop {
        while next_record < records.len() && records[next_record].now_us <= now {
            let record = &records[next_record];
            if let Some(sample) = &record.observation {
                summary.observations += 1;
                if engine.ingest(record.now_us, sample.clone()).is_err() {
                    summary.rejected += 1;
                }
            }
            if let Some(fault) = record.fault {
                summary.faults += 1;
                engine.input_fault(record.now_us, fault);
            }
            if let Some(privacy) = &mut privacy {
                if let Some(sample) = &record.privacy
                    && privacy.observe(record.now_us, sample.clone()).is_err()
                {
                    summary.rejected += 1;
                }
                if let Some(fault) = record.privacy_fault {
                    privacy.fault(fault);
                }
            }
            next_record += 1;
        }
        let mut output = engine.tick(now);
        if let Some(privacy) = &mut privacy {
            output.state.privacy = Some(privacy.snapshot(now));
        }
        let privacy_key = output
            .state
            .privacy
            .as_ref()
            .map(|sample| (sample.muted, sample.fault, sample.reader_session_id.clone()));
        summary.ticks += 1;
        summary.gestures += output.gestures.len() as u64;
        if !output.gestures.is_empty()
            || last_frame.is_none_or(|at| {
                now - at
                    >= if privacy_enabled {
                        crate::privacy::STATE_EVERY_US
                    } else {
                        100_000
                    }
            })
            || last_fault != output.state.fault
            || last_privacy != privacy_key
            || now == end
        {
            last_fault = output.state.fault;
            last_privacy = privacy_key;
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
        }
        if now == end {
            break;
        }
        now = now.saturating_add(POLL_US).min(end);
    }
    writer.flush()?;
    Ok(summary)
}
