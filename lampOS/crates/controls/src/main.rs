use lamp_bus::Bus;
use lamp_controls::{
    engine::Config,
    replay,
    runtime::{self, MissingPins, Service},
};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

const USAGE: &str = "lamp-controls serve [--config FILE] [--enable] [--bus-root PATH] [--clock-id ID] [--record FILE] [--seconds N]\n\
lamp-controls replay --input FILE --config FILE --clock-id ID [--enable] [--output FILE] [--until-us N]\n\
lamp-controls clock [--clock-id ID]\n\n\
Shadow is the default. A config file cannot enable the process without --enable.\n\
The shipped serve adapter reports Missing: First supplies the verified board reader.\n\
Replay never opens a bus or hardware. Times are CLOCK_MONOTONIC microseconds.\n\
Record/output files must not already exist. Acquisition records are capped at 32 MiB.\n\
No controls message grants microphone/camera permission or reports a verified stop.";

#[derive(Default)]
struct Args {
    command: String,
    enabled: bool,
    config: Option<PathBuf>,
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    record: Option<PathBuf>,
    bus_root: Option<PathBuf>,
    clock: Option<String>,
    seconds: Option<u64>,
    until: Option<u64>,
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Args>, String> {
    let mut args = args.into_iter();
    let command = args.next().ok_or("a command is required")?;
    if command == "--help" || command == "-h" {
        return Ok(None);
    }
    if !["serve", "replay", "clock"].contains(&command.as_str()) {
        return Err(format!("unknown command {command}"));
    }
    let mut out = Args {
        command,
        ..Args::default()
    };
    let mut seen = BTreeSet::new();
    while let Some(option) = args.next() {
        if !seen.insert(option.clone()) {
            return Err(format!("{option} was given twice"));
        }
        let allowed = match out.command.as_str() {
            "serve" => [
                "--config",
                "--enable",
                "--bus-root",
                "--clock-id",
                "--record",
                "--seconds",
            ]
            .contains(&option.as_str()),
            "replay" => [
                "--input",
                "--config",
                "--clock-id",
                "--enable",
                "--output",
                "--until-us",
            ]
            .contains(&option.as_str()),
            _ => option == "--clock-id",
        };
        if !allowed {
            return Err(format!("{} does not accept {option}", out.command));
        }
        if option == "--enable" {
            out.enabled = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("{option} needs a value"))?;
        match option.as_str() {
            "--config" => out.config = Some(value.into()),
            "--input" => out.input = Some(value.into()),
            "--output" => out.output = Some(value.into()),
            "--record" => out.record = Some(value.into()),
            "--clock-id" => out.clock = Some(value),
            "--bus-root" => {
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err("bus root must be absolute".into());
                }
                out.bus_root = Some(path);
            }
            "--seconds" => {
                out.seconds = Some(
                    value
                        .parse::<u64>()
                        .ok()
                        .filter(|n| (1..=86400).contains(n))
                        .ok_or("seconds must be 1..86400")?,
                )
            }
            "--until-us" => {
                out.until = Some(
                    value
                        .parse()
                        .map_err(|_| "until-us must be an unsigned integer")?,
                )
            }
            _ => unreachable!(),
        }
    }
    if out.command == "replay"
        && (out.input.is_none() || out.config.is_none() || out.clock.is_none())
    {
        return Err("replay needs --input, --config and the input's --clock-id".into());
    }
    Ok(Some(out))
}

fn config(args: &Args) -> Result<Config, Box<dyn std::error::Error>> {
    let mut config = if let Some(path) = &args.config {
        let mut bytes = Vec::new();
        File::open(path)?.take(16_385).read_to_end(&mut bytes)?;
        if bytes.len() > 16_384 {
            return Err("controls configuration exceeds 16 KiB".into());
        }
        serde_json::from_slice(&bytes)?
    } else {
        Config {
            buttons: vec!["primary".into()],
            ..Config::default()
        }
    };
    config.enabled = args.enabled;
    config.check()?;
    Ok(config)
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let clock = runtime::clock_id(args.clock.as_deref())?;
    if args.command == "clock" {
        println!(
            "{}",
            serde_json::json!({"clock_id":clock,"at_us":lamp_ipc::monotonic_us()})
        );
        return Ok(());
    }
    let config = config(&args)?;
    if args.command == "replay" {
        let records = replay::read(File::open(args.input.as_ref().unwrap())?)?;
        let writer: Box<dyn Write> = if let Some(path) = args.output {
            Box::new(
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(path)?,
            )
        } else {
            Box::new(io::stdout().lock())
        };
        let result = replay::run(config, &clock, &records, args.until, writer)?;
        eprintln!("{}", serde_json::to_string(&result)?);
        return Ok(());
    }
    let bus = if let Some(root) = args.bus_root {
        Bus::new(root)?
    } else {
        Bus::from_environment()?
    };
    let mut service = Service::new(&bus, config, MissingPins, &clock)?;
    if let Some(path) = args.record {
        service.record_to(&path)?;
    }
    let stop = Arc::new(AtomicBool::new(false));
    let signal_ids = [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM]
        .map(|signal| signal_hook::flag::register(signal, stop.clone()))
        .into_iter()
        .collect::<io::Result<Vec<_>>>()?;
    let started = lamp_ipc::monotonic_us();
    while !stop.load(Ordering::Relaxed)
        && args
            .seconds
            .is_none_or(|seconds| lamp_ipc::monotonic_us() - started < seconds * 1_000_000)
    {
        let began = lamp_ipc::monotonic_us();
        service.step(lamp_ipc::monotonic_us);
        let elapsed = lamp_ipc::monotonic_us().saturating_sub(began);
        if elapsed < runtime::POLL_US {
            thread::sleep(Duration::from_micros(runtime::POLL_US - elapsed));
        }
    }
    for id in signal_ids {
        signal_hook::low_level::unregister(id);
    }
    let recording = service.finish_recording();
    println!(
        "{}",
        serde_json::json!({"counts":service.counts,"recording":recording,"physical_stop_verified":false})
    );
    Ok(())
}

fn main() -> ExitCode {
    match parse(std::env::args().skip(1)) {
        Ok(None) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Some(args)) => match run(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("lamp-controls: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("lamp-controls: {error}\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
