//! `lamp-endpoint-worker --model PATH [--parent PID]`
//! `lamp-endpoint-worker --check-model PATH` (installation check only)
//!
//! Serves Smart Turn v3.2 verdicts for one interaction core over stdin and
//! stdout (see `lamp_endpoint_ipc::frame`). It verifies the pinned model, runs
//! one cold inference, reports `Ready` and then answers one request at a time.
//! It writes no files and keeps no audio between requests. On Linux it dies
//! with the core that started it; an input end of stream is an ordinary stop.
use lamp_endpoint::{FeatureExtractor, SMART_TURN_V3_2, SmartTurn, classify};
use lamp_endpoint_ipc::serve;
use std::{ffi::OsString, io, path::PathBuf, process::ExitCode, time::Instant};

struct Arguments {
    model: PathBuf,
    parent: Option<u32>,
}

fn parse(mut arguments: impl Iterator<Item = OsString>) -> Result<Arguments, String> {
    let (mut model, mut parent) = (None, None);
    while let Some(argument) = arguments.next() {
        let value = arguments.next().ok_or("missing option value")?;
        match argument.to_str() {
            Some("--model") if model.is_none() => model = Some(PathBuf::from(value)),
            Some("--parent") if parent.is_none() => {
                let pid = value.to_str().and_then(|pid| pid.parse().ok());
                parent = Some(pid.ok_or("invalid parent process ID")?);
            }
            _ => return Err("usage: lamp-endpoint-worker --model PATH [--parent PID]".into()),
        }
    }
    Ok(Arguments {
        model: model.ok_or("missing --model")?,
        parent,
    })
}

/// A hung worker must not outlive the core that can no longer kill it.
#[cfg(target_os = "linux")]
fn follow_parent(parent: Option<u32>) -> Result<(), String> {
    use rustix::process::{Signal, getppid, set_parent_process_death_signal};
    set_parent_process_death_signal(Some(Signal::KILL))
        .map_err(|error| format!("parent death signal: {error}"))?;
    // The core may have exited before the signal was armed.
    match (parent, getppid()) {
        (Some(expected), Some(actual)) if actual.as_raw_pid() as u32 != expected => {
            Err("parent process already exited".into())
        }
        _ => Ok(()),
    }
}

#[cfg(not(target_os = "linux"))]
fn follow_parent(_parent: Option<u32>) -> Result<(), String> {
    Ok(())
}

fn sha256_bytes(hex: &str) -> [u8; 32] {
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap_or(0);
    }
    bytes
}

/// Verifies an installed model's pinned size and SHA256, and that tract can
/// load it, without speaking the worker protocol.
fn check_model(path: &OsString) -> ExitCode {
    let path = PathBuf::from(path);
    match SmartTurn::load(&path, &SMART_TURN_V3_2) {
        Ok(_) => {
            println!(
                "{}: verified {} ({} bytes, SHA256 {})",
                path.display(),
                SMART_TURN_V3_2.file_name,
                SMART_TURN_V3_2.bytes,
                SMART_TURN_V3_2.sha256
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            ExitCode::from(2)
        }
    }
}

fn main() -> ExitCode {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    if let [flag, path] = arguments.as_slice()
        && flag == "--check-model"
    {
        return check_model(path);
    }
    let mut output = io::stdout().lock();
    let started = Instant::now();
    let setup = parse(arguments.into_iter()).and_then(|arguments| {
        follow_parent(arguments.parent)?;
        SmartTurn::load(&arguments.model, &SMART_TURN_V3_2)
            .map_err(|error| format!("model {}: {error}", arguments.model.display()))
    });
    let model = match setup {
        Ok(model) => model,
        Err(reason) => {
            let _ = serve::startup_failed(&mut output, &reason);
            return ExitCode::from(2);
        }
    };
    let load_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    let mut extractor = FeatureExtractor::default();
    let cold = match classify(&model, &mut extractor, &[]) {
        Ok(cold) => cold.features_us + cold.inference_us,
        Err(reason) => {
            let _ = serve::startup_failed(&mut output, &format!("cold inference: {reason}"));
            return ExitCode::from(2);
        }
    };
    let sha256 = sha256_bytes(SMART_TURN_V3_2.sha256);
    if serve::ready(&mut output, load_us, cold, sha256).is_err() {
        return ExitCode::from(5);
    }
    let stop = serve::serve(&mut io::stdin().lock(), &mut output, |pcm| {
        classify(&model, &mut extractor, pcm)
    });
    if !matches!(stop, serve::Stop::Closed) {
        eprintln!("lamp-endpoint-worker: {stop:?}");
    }
    ExitCode::from(stop.exit_code() as u8)
}
