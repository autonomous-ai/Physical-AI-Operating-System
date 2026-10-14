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

const USAGE: &str = "lamp-controls serve [--config FILE] [--pins FILE] [--enable] [--bus-root PATH] [--clock-id ID] [--record FILE] [--seconds N]\n\
lamp-controls replay --input FILE --config FILE --clock-id ID [--enable] [--output FILE] [--until-us N]\n\
lamp-controls probe --pins FILE\n\
lamp-controls clock [--clock-id ID]\n\n\
Shadow is the default. A config file cannot enable the process without --enable.\n\
Without --pins, serve reads nothing and reports Missing. With it, the board's GPIO lines named in FILE are read.\n\
probe reads each line of FILE once with a pull-down and once with a pull-up, and serves nothing.\n\
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
    pins: Option<PathBuf>,
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Args>, String> {
    let mut args = args.into_iter();
    let command = args.next().ok_or("a command is required")?;
    if command == "--help" || command == "-h" {
        return Ok(None);
    }
    if !["serve", "replay", "clock", "probe"].contains(&command.as_str()) {
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
                "--pins",
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
            "probe" => option == "--pins",
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
            "--pins" => out.pins = Some(value.into()),
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
    if out.command == "probe" && out.pins.is_none() {
        return Err("probe needs --pins".into());
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

/// The board's wiring, from its file.
#[cfg(target_os = "linux")]
fn pins(
    path: &std::path::Path,
) -> Result<lamp_controls::pins::PinsConfig, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(16_385).read_to_end(&mut bytes)?;
    if bytes.len() > 16_384 {
        return Err("the pins file exceeds 16 KiB".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

/// The reader serve uses: the board's lines if a pins file was given, and
/// otherwise the stand-in that says no input is there.
#[cfg(target_os = "linux")]
fn reader(
    args: &Args,
    config: &Config,
    clock: &str,
) -> Result<Box<dyn lamp_controls::input::PinReader>, Box<dyn std::error::Error>> {
    use lamp_controls::pins::{GpioLines, LinePins};
    let Some(path) = &args.pins else {
        return Ok(Box::new(MissingPins));
    };
    let wiring = pins(path)?;
    wiring.check(config)?;
    let lines = GpioLines::open(&wiring)?;
    let session = format!("pins-{}-{}", std::process::id(), lamp_ipc::monotonic_us());
    Ok(Box::new(LinePins::new(
        lines, wiring, config, clock, session,
    )?))
}

#[cfg(not(target_os = "linux"))]
fn reader(
    args: &Args,
    _: &Config,
    _: &str,
) -> Result<Box<dyn lamp_controls::input::PinReader>, Box<dyn std::error::Error>> {
    if args.pins.is_some() {
        return Err("--pins reads GPIO lines, which this system does not have".into());
    }
    Ok(Box::new(MissingPins))
}

#[cfg(target_os = "linux")]
fn probe(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let wiring = pins(path)?;
    for (kind, pin) in wiring
        .buttons
        .iter()
        .map(|pin| ("button", pin))
        .chain(wiring.head_pads.iter().map(|pin| ("head_pad", pin)))
    {
        let read = lamp_controls::pins::GpioLines::probe(&wiring.chip, pin.line);
        println!(
            "{}",
            match read {
                Ok((pulled_down, pulled_up)) =>
                    serde_json::json!({"kind":kind,"name":pin.name,"line":pin.line,
                    "high_with_pull_down":pulled_down,"high_with_pull_up":pulled_up,
                    "reads":if pulled_down == pulled_up {"driven: something holds this line"} else {"follows the pull: nothing drives it now"}}),
                Err(error) =>
                    serde_json::json!({"kind":kind,"name":pin.name,"line":pin.line,"error":error.to_string()}),
            }
        );
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn probe(_: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    Err("probe reads GPIO lines, which this system does not have".into())
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.command == "probe" {
        return probe(args.pins.as_ref().expect("checked when parsed"));
    }
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
    let reader = reader(&args, &config, &clock)?;
    let bus = if let Some(root) = args.bus_root {
        Bus::new(root)?
    } else {
        Bus::from_environment()?
    };
    let mut service = Service::new(&bus, config, reader, &clock)?;
    if let Some(path) = args.record {
        service.record_to(&path)?;
    }
    let stop = Arc::new(AtomicBool::new(false));
    let signal_ids = [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM]
        .map(|signal| signal_hook::flag::register(signal, stop.clone()))
        .into_iter()
        .collect::<io::Result<Vec<_>>>()?;
    let started = lamp_ipc::monotonic_us();
    let mut hands = HandLog::default();
    while !stop.load(Ordering::Relaxed)
        && args
            .seconds
            .is_none_or(|seconds| lamp_ipc::monotonic_us() - started < seconds * 1_000_000)
    {
        let began = lamp_ipc::monotonic_us();
        let output = service.step(lamp_ipc::monotonic_us);
        hands.note(began, &output);
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

/// One journal line whenever a hand changes something: a button pressed or
/// let go, the head touched or let go, a gesture, a fault or its end. So the
/// first real press or pat is kept even when nobody is watching the bus.
/// Written to standard error: standard output carries only the closing summary.
/// Changes only, and at most `HandLog::LINES` lines in any ten seconds, so a
/// line that flaps cannot fill the journal; what is held back is counted.
#[derive(Default)]
struct HandLog {
    last: Option<String>,
    window_us: u64,
    lines: u32,
    held_back: u64,
}

impl HandLog {
    const LINES: u32 = 20;
    const WINDOW_US: u64 = 10_000_000;

    fn note(&mut self, at_us: u64, output: &lamp_controls::engine::Output) {
        let state = &output.state;
        let mut now = state
            .buttons
            .iter()
            .map(|button| {
                let word = if button.pressed {
                    "pressed"
                } else {
                    "released"
                };
                format!("button {} {word}", button.name)
            })
            .collect::<Vec<_>>();
        now.push(
            if state.head_active {
                "head touched"
            } else {
                "head not touched"
            }
            .into(),
        );
        if let Some(fault) = state.fault {
            now.push(format!("fault {fault:?}"));
        }
        let gestures = output
            .gestures
            .iter()
            .map(|gesture| {
                format!(
                    "gesture {:?} on {} ({:?}), {} ms, acted on: {}",
                    gesture.gesture,
                    gesture.name,
                    gesture.intent,
                    gesture.duration_us / 1000,
                    gesture.enabled
                )
            })
            .collect();
        for line in self.lines(at_us, now.join(", "), gestures) {
            eprintln!("lamp-controls: hand: {line}");
        }
    }

    fn lines(&mut self, at_us: u64, now: String, gestures: Vec<String>) -> Vec<String> {
        let mut said = Vec::new();
        if self.last.as_deref() != Some(now.as_str()) {
            said.push(now.clone());
            self.last = Some(now);
        }
        said.extend(gestures);
        let mut out = Vec::new();
        if said.is_empty() {
            return out;
        }
        if at_us.saturating_sub(self.window_us) >= Self::WINDOW_US {
            if self.held_back > 0 {
                out.push(format!("{} more changes not written", self.held_back));
                self.held_back = 0;
            }
            self.window_us = at_us;
            self.lines = 0;
        }
        for line in said {
            if self.lines < Self::LINES {
                self.lines += 1;
                out.push(line);
            } else {
                self.held_back += 1;
            }
        }
        out
    }
}

#[cfg(test)]
mod hand_log_tests {
    use super::HandLog;

    #[test]
    fn only_changes_are_written() {
        let mut log = HandLog::default();
        let start = 20_000_000;
        assert_eq!(
            log.lines(start, "head not touched".into(), vec![]),
            ["head not touched"]
        );
        assert!(
            log.lines(start + 5_000, "head not touched".into(), vec![])
                .is_empty()
        );
        assert_eq!(
            log.lines(start + 10_000, "head touched".into(), vec![]),
            ["head touched"]
        );
        assert_eq!(
            log.lines(
                start + 15_000,
                "head touched".into(),
                vec!["gesture Pet".into()]
            ),
            ["gesture Pet"]
        );
    }

    #[test]
    fn a_flapping_line_is_held_to_twenty_lines_in_ten_seconds_and_counted() {
        let mut log = HandLog::default();
        let start = 20_000_000;
        let mut written = 0;
        for i in 0..200u64 {
            let now = if i % 2 == 0 {
                "head touched"
            } else {
                "head not touched"
            };
            written += log.lines(start + i * 5_000, now.into(), vec![]).len();
        }
        assert_eq!(written, 20);
        let next = log.lines(start + 10_000_000, "head touched".into(), vec![]);
        assert_eq!(
            next,
            [
                "180 more changes not written".to_string(),
                "head touched".into()
            ]
        );
    }
}
