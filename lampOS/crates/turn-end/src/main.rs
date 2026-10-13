//! `lamp-turn-end --model PATH [--parent PID]`
//! `lamp-turn-end --check-model PATH` (installation self-check)
//!
//! Serves Smart Turn v3.2 verdicts for one interaction core over stdin and
//! stdout (see `lamp_turn_end_wire::frame`). It verifies the pinned model, runs
//! one cold inference, reports `Ready` and then answers one request at a time.
//! It writes no files. After each request it zeroes its request and
//! feature-extraction buffers; freed allocations and the inference runtime's
//! internal memory are not scrubbed. On Linux it dies with the core that
//! started it; an input end of stream is an ordinary stop. Before it reads the
//! model on Linux, it disables core files, caps its address space and lowers
//! its CPU priority.
use lamp_turn_end::{FeatureExtractor, SMART_TURN_V3_2, SmartTurn, classify, reference};
use lamp_turn_end_wire::serve;
use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};

/// Niceness the worker adds on Linux. Under CPU contention, the fair
/// scheduler gives each capture, playback and provider thread about nine
/// times the worker's share; a verdict that arrives late is answered by the
/// core's silence fallback.
#[cfg(target_os = "linux")]
const NICE: i32 = 10;
/// Linux address-space cap: 2.2 times the 113.5 MiB `VmPeak` the self-check
/// measured on ARM64 Linux (docs/endpoint-detection.md). An allocation beyond
/// it ends the worker, which the core treats like any other worker failure.
#[cfg(target_os = "linux")]
const ADDRESS_SPACE_LIMIT: u64 = 256 << 20;
/// Classifications of the reference signal in `--check-model`. The first is
/// cold; the rest report the warm cost.
const CHECK_RUNS: usize = 10;

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
            _ => return Err("usage: lamp-turn-end --model PATH [--parent PID]".into()),
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

/// The lowest of an inherited soft limit, an inherited hard limit (`None` is
/// unlimited) and the worker's own cap.
#[cfg(any(target_os = "linux", test))]
fn lowest_limit(current: Option<u64>, maximum: Option<u64>, cap: u64) -> u64 {
    [current, maximum].into_iter().flatten().fold(cap, u64::min)
}

/// Bounds the worker before it reads the model. Never raises a soft or hard
/// limit the service manager already set lower.
#[cfg(target_os = "linux")]
fn bound_resources() -> Result<(), String> {
    use rustix::process::{Resource, Rlimit, getrlimit, nice, setrlimit};
    let cap = |resource, bytes: u64| {
        let inherited = getrlimit(resource);
        let bytes = lowest_limit(inherited.current, inherited.maximum, bytes);
        setrlimit(
            resource,
            Rlimit {
                current: Some(bytes),
                maximum: Some(bytes),
            },
        )
    };
    // A crash must not write retained speech to a core file.
    cap(Resource::Core, 0).map_err(|error| format!("core file limit: {error}"))?;
    cap(Resource::As, ADDRESS_SPACE_LIMIT)
        .map_err(|error| format!("address space limit: {error}"))?;
    nice(NICE).map_err(|error| format!("priority: {error}"))?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn bound_resources() -> Result<(), String> {
    Ok(())
}

/// The effective address-space limit in KiB and niceness, as JSON values.
#[cfg(target_os = "linux")]
fn bounds() -> [String; 2] {
    use rustix::process::{Resource, getpriority_process, getrlimit};
    let limit = getrlimit(Resource::As).current;
    [
        limit.map_or("null".into(), |bytes| (bytes >> 10).to_string()),
        getpriority_process(None).map_or("null".into(), |nice| nice.to_string()),
    ]
}

#[cfg(not(target_os = "linux"))]
fn bounds() -> [String; 2] {
    ["null".into(), "null".into()]
}

/// `VmPeak`, `VmHWM` and `VmRSS` in KiB, from Linux `/proc/self/status`.
fn memory_kib() -> [Option<u64>; 3] {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    ["VmPeak:", "VmHWM:", "VmRSS:"].map(|key| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|value| value.trim().trim_end_matches("kB").trim().parse().ok())
    })
}

fn cpu_us() -> u64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    (now.tv_sec as u64) * 1_000_000 + (now.tv_nsec as u64) / 1_000
}

fn ms(us: u64) -> f64 {
    us as f64 / 1_000.0
}

/// Loads the model under the worker's own bounds, classifies the reference
/// signal `CHECK_RUNS` times against ONNX Runtime's probability, and returns
/// one JSON line of costs.
fn self_check(path: &Path) -> Result<String, String> {
    bound_resources()?;
    let (started, cpu_started) = (Instant::now(), cpu_us());
    let model = SmartTurn::load(path, &SMART_TURN_V3_2).map_err(|error| error.to_string())?;
    let load_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    let load_cpu_us = cpu_us() - cpu_started;
    let (mut extractor, signal) = (FeatureExtractor::default(), reference::voiced());
    let (mut costs, mut probability) = (Vec::with_capacity(CHECK_RUNS), 0.0);
    let cpu_started = cpu_us();
    for _ in 0..CHECK_RUNS {
        let classified = classify(&model, &mut extractor, &signal)?;
        probability = classified.probability;
        if (probability - reference::VOICED_PROBABILITY).abs() > reference::TOLERANCE {
            return Err(format!(
                "reference probability {probability} differs from ONNX Runtime's {}",
                reference::VOICED_PROBABILITY
            ));
        }
        costs.push(classified.features_us + classified.inference_us);
    }
    let cpu_per_check_us = (cpu_us() - cpu_started) / CHECK_RUNS as u64;
    let cold_us = costs[0];
    let mut warm = costs[1..].to_vec();
    warm.sort_unstable();
    let [peak, hwm, rss] = memory_kib().map(|value| value.map_or("null".into(), |v| v.to_string()));
    let [address_space, nice] = bounds();
    Ok(format!(
        concat!(
            "{{\"model\":\"{}\",\"bytes\":{},\"sha256\":\"{}\",\"probability\":{},",
            "\"reference\":{},\"load_ms\":{:.1},\"load_cpu_ms\":{:.1},\"cold_ms\":{:.1},",
            "\"warm_median_ms\":{:.1},\"warm_max_ms\":{:.1},\"cpu_per_check_ms\":{:.1},",
            "\"vm_peak_kib\":{},\"vm_hwm_kib\":{},\"vm_rss_kib\":{},",
            "\"address_space_limit_kib\":{},\"nice\":{}}}"
        ),
        SMART_TURN_V3_2.file_name,
        SMART_TURN_V3_2.bytes,
        SMART_TURN_V3_2.sha256,
        probability,
        reference::VOICED_PROBABILITY,
        ms(load_us),
        ms(load_cpu_us),
        ms(cold_us),
        ms(warm[warm.len() / 2]),
        ms(warm[warm.len() - 1]),
        ms(cpu_per_check_us),
        peak,
        hwm,
        rss,
        address_space,
        nice,
    ))
}

fn main() -> ExitCode {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    if let [flag, path] = arguments.as_slice()
        && flag == "--check-model"
    {
        let path = PathBuf::from(path);
        return match self_check(&path) {
            Ok(line) => {
                println!("{line}");
                ExitCode::SUCCESS
            }
            Err(reason) => {
                eprintln!("{}: {reason}", path.display());
                ExitCode::from(2)
            }
        };
    }
    let mut output = io::stdout().lock();
    let started = Instant::now();
    let setup = parse(arguments.into_iter()).and_then(|arguments| {
        follow_parent(arguments.parent)?;
        bound_resources()?;
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
    let sha256 = lamp_turn_end_wire::sha256_bytes(SMART_TURN_V3_2.sha256);
    if serve::ready(&mut output, load_us, cold, sha256).is_err() {
        return ExitCode::from(5);
    }
    let stop = serve::serve(&mut io::stdin().lock(), &mut output, |pcm| {
        classify(&model, &mut extractor, pcm)
    });
    if !matches!(stop, serve::Stop::Closed) {
        eprintln!("lamp-turn-end: {stop:?}");
    }
    ExitCode::from(stop.exit_code() as u8)
}

#[cfg(test)]
mod tests {
    use super::lowest_limit;

    #[test]
    fn inherited_lower_limits_are_never_raised() {
        const CAP: u64 = 256 << 20;
        // The review finding: a 96 MiB soft limit under a higher hard limit.
        assert_eq!(lowest_limit(Some(96 << 20), Some(512 << 20), CAP), 96 << 20);
        assert_eq!(lowest_limit(Some(96 << 20), None, CAP), 96 << 20);
        assert_eq!(lowest_limit(None, Some(128 << 20), CAP), 128 << 20);
        assert_eq!(
            lowest_limit(Some(300 << 20), Some(200 << 20), CAP),
            200 << 20
        );
        assert_eq!(lowest_limit(None, None, CAP), CAP);
        assert_eq!(lowest_limit(Some(1 << 40), None, CAP), CAP);
        assert_eq!(lowest_limit(None, None, 0), 0);
    }
}
