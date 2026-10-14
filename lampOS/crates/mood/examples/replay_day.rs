//! Print reproducible, explicitly synthetic JSONL; this never attaches a bus.
use lamp_mood::replay::{ReplayRecord, run_authored_day};
use std::io::{self, BufWriter, Write};

fn write_line(
    out: &mut impl Write,
    record: ReplayRecord<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer(&mut *out, &record)?;
    out.write_all(b"\n")?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "replay_day takes no arguments").into(),
        );
    }
    let replay = run_authored_day()?;
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    write_line(&mut out, ReplayRecord::Metadata(&replay.metadata))?;
    for phase in replay.phases {
        write_line(&mut out, ReplayRecord::Phase(phase))?;
    }
    for sample in &replay.samples {
        write_line(&mut out, ReplayRecord::Sample(sample))?;
    }
    write_line(&mut out, ReplayRecord::Summary(&replay.summary))?;
    out.flush()?;
    Ok(())
}
