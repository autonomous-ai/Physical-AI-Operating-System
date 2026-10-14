//! Offline only: no bus, provider, audio or robot connection.
use lamp_policy::policy::{
    fit::{self, Options},
    local::{MOST_WEIGHTS_BYTES, Weights},
};
use std::{
    env,
    fs::{File, OpenOptions},
    io::{self, BufReader, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::ExitCode,
};

const USAGE: &str = "usage: lamp-policy-fit INPUT.jsonl NEW_WEIGHTS.json NEW_REPORT.json\n       lamp-policy-fit --seed NEW_WEIGHTS.json";

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()
}

fn run() -> io::Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--help" {
        println!("{USAGE}");
        return Ok(());
    }
    if args.len() == 2 && args[0] == "--seed" {
        let bytes = serde_json::to_vec_pretty(&Weights::seed()).map_err(io::Error::other)?;
        write_new(Path::new(&args[1]), &bytes)?;
        println!("lamp-policy-fit: wrote authored, unfitted seed; no records or training used");
        return Ok(());
    }
    if args.len() != 3 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE));
    }
    // Reserve neither output until all parsing, fitting and report encoding
    // succeeded. create_new never follows an existing output symlink or
    // overwrites an input. A later write failure can leave a partial new file;
    // that is a failed run, never an accepted model.
    let rows = fit::read_rows(BufReader::new(File::open(&args[0])?))?;
    let result = fit::fit(&rows, &Options::default())?;
    let weights = serde_json::to_vec_pretty(&result.weights)
        .map_err(|_| io::Error::other("cannot encode fitted weights"))?;
    if weights.len() + 1 > MOST_WEIGHTS_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "encoded model exceeds 64 KiB; shorten provenance declarations",
        ));
    }
    let report = serde_json::to_vec_pretty(&result.report)
        .map_err(|_| io::Error::other("cannot encode fitting report"))?;
    write_new(Path::new(&args[1]), &weights)?;
    write_new(Path::new(&args[2]), &report)?;
    println!(
        "lamp-policy-fit: wrote new weights and report; train={} calibration={} test={}; mixture remains unfitted",
        result.report.train_rows, result.report.calibration_rows, result.report.test_rows
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lamp-policy-fit: {error}");
            ExitCode::FAILURE
        }
    }
}
