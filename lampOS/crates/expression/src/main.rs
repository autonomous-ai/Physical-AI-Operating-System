use lamp_bus::Bus;
use lamp_expression::{Catalogue, Engine, TICK_US, replay, runtime};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{self, BufReader, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    process::ExitCode,
    sync::atomic::Ordering,
    thread,
    time::Duration,
};

const USAGE: &str = "lamp-expression serve [--enable] [--bus-root PATH] [--clock-id ID] [--catalogue FILE] [--record FILE] [--seconds N]\n\
lamp-expression replay --input FILE [--output FILE] --clock-id ID [--enable] [--catalogue FILE] [--until-us N]\n\
lamp-expression import --clock-id ID FILE...\n\
lamp-expression catalogue\n\
lamp-expression clock [--clock-id ID]\n\n\
Default is shadow mode: no body or light command is published.\n\
Replay writes one JSON line per injected 20 ms clock step and never touches a bus.\n\
Live timestamps use lamp-ipc CLOCK_MONOTONIC microseconds; Linux defaults to its boot ID.\n\
Output files are created new, never overwritten. Live --record is bounded to 64 MiB.\n\
No command opens hardware, launches a provider, or changes a device's service settings.";

#[derive(Debug, Default)]
struct Arguments {
    command: String,
    enabled: bool,
    bus_root: Option<PathBuf>,
    clock: Option<String>,
    catalogue: Option<PathBuf>,
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    record: Option<PathBuf>,
    seconds: Option<u64>,
    until: Option<u64>,
    imports: Vec<PathBuf>,
}

fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Option<Arguments>, String> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        return Err("a command is required".into());
    };
    if ["--help", "-h"].contains(&command.as_str()) {
        return Ok(None);
    }
    if !["serve", "replay", "import", "catalogue", "clock"].contains(&command.as_str()) {
        return Err(format!("unknown command {command}"));
    }
    let mut out = Arguments {
        command,
        ..Arguments::default()
    };
    let mut seen = BTreeSet::new();
    while let Some(option) = arguments.next() {
        if ["--help", "-h"].contains(&option.as_str()) {
            return Ok(None);
        }
        if out.command == "import" && !option.starts_with('-') {
            out.imports.push(option.into());
            continue;
        }
        if !seen.insert(option.clone()) {
            return Err(format!("{option} was given twice"));
        }
        let allowed = match out.command.as_str() {
            "serve" => [
                "--enable",
                "--bus-root",
                "--clock-id",
                "--catalogue",
                "--record",
                "--seconds",
            ]
            .contains(&option.as_str()),
            "replay" => [
                "--enable",
                "--clock-id",
                "--catalogue",
                "--input",
                "--output",
                "--until-us",
            ]
            .contains(&option.as_str()),
            "clock" | "import" => option == "--clock-id",
            _ => false,
        };
        if !allowed {
            return Err(format!("{} does not accept {option}", out.command));
        }
        if option == "--enable" {
            out.enabled = true;
            continue;
        }
        let value = arguments
            .next()
            .ok_or_else(|| format!("{option} needs a value"))?;
        match option.as_str() {
            "--clock-id" => out.clock = Some(value),
            "--bus-root" => {
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err("--bus-root must be absolute".into());
                }
                out.bus_root = Some(path);
            }
            "--catalogue" => out.catalogue = Some(value.into()),
            "--input" => out.input = Some(value.into()),
            "--output" => out.output = Some(value.into()),
            "--record" => out.record = Some(value.into()),
            "--seconds" => {
                out.seconds = Some(
                    value
                        .parse::<u64>()
                        .ok()
                        .filter(|v| (1..=86_400).contains(v))
                        .ok_or("--seconds must be 1..86400")?,
                )
            }
            "--until-us" => {
                out.until = Some(
                    value
                        .parse()
                        .map_err(|_| "--until-us must be an unsigned integer")?,
                )
            }
            _ => unreachable!(),
        }
    }
    if out.command == "replay" && out.input.is_none() {
        return Err("replay needs --input".into());
    }
    if out.command == "import" && out.imports.is_empty() {
        return Err("import needs bus recording files".into());
    }
    if ["replay", "import"].contains(&out.command.as_str()) && out.clock.is_none() {
        return Err("replay/import needs the recording's --clock-id".into());
    }
    Ok(Some(out))
}

fn catalogue(path: Option<&PathBuf>) -> Result<Catalogue, Box<dyn std::error::Error>> {
    let Some(path) = path else {
        return Ok(Catalogue::builtin());
    };
    let mut text = String::new();
    File::open(path)?.take(131_073).read_to_string(&mut text)?;
    if text.len() > 131_072 {
        return Err("catalogue is over 128 KiB".into());
    }
    Ok(Catalogue::from_json(&text)?)
}

fn writer(path: Option<&PathBuf>) -> io::Result<Box<dyn Write>> {
    match path {
        Some(path) => Ok(Box::new(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(path)?,
        )),
        None => Ok(Box::new(io::stdout())),
    }
}

fn run(args: Arguments) -> Result<(), Box<dyn std::error::Error>> {
    if args.command == "catalogue" {
        println!("{}", include_str!("../behaviours.json"));
        return Ok(());
    }
    let clock = runtime::clock_id(args.clock.as_deref())?;
    if args.command == "clock" {
        println!(
            "{}",
            serde_json::json!({"clock_id":clock,"at_us":lamp_ipc::monotonic_us()})
        );
        return Ok(());
    }
    if args.command == "import" {
        let imported = replay::import_bus(&args.imports, &clock)?;
        let mut stdout = io::stdout().lock();
        for record in imported.records {
            serde_json::to_writer(&mut stdout, &record)?;
            stdout.write_all(b"\n")?;
        }
        for warning in imported.warnings {
            eprintln!("lamp-expression: {warning:?}");
        }
        return Ok(());
    }
    let catalogue = catalogue(args.catalogue.as_ref())?;
    if args.command == "replay" {
        let mut engine = Engine::new(catalogue, args.enabled, clock)?;
        let summary = replay::run(
            BufReader::new(File::open(args.input.as_ref().expect("validated"))?),
            writer(args.output.as_ref())?,
            &mut engine,
            args.until,
        )?;
        eprintln!("lamp-expression: replay {summary:?}; simulated request timing only");
        return Ok(());
    }
    let bus = match args.bus_root {
        Some(path) => Bus::new(path)?,
        None => Bus::from_environment()?,
    };
    let recorder = args
        .record
        .as_ref()
        .map(|path| runtime::Recorder::new(path, 64 * 1024 * 1024))
        .transpose()?;
    let mut service = runtime::Service::new(&bus, catalogue, args.enabled, &clock)?;
    eprintln!(
        "lamp-expression: started mode={} clock_id={} root={}",
        if args.enabled { "enabled" } else { "shadow" },
        clock,
        bus.root().display()
    );
    let began = lamp_ipc::monotonic_us();
    loop {
        let started = lamp_ipc::monotonic_us();
        service.counts.records_lost = recorder
            .as_ref()
            .map_or(0, |r| r.lost.load(Ordering::Relaxed));
        let output = service.step(lamp_ipc::monotonic_us, runtime::unix_ms());
        if let Some(recorder) = &recorder {
            recorder.record(output.state.stamp.at_us, output);
        }
        if args
            .seconds
            .is_some_and(|seconds| started.saturating_sub(began) >= seconds * 1_000_000)
        {
            break;
        }
        // No catch-up storm after a stalled host. Engine and command leases
        // are evaluated against the current shared clock on the next step.
        thread::sleep(Duration::from_micros(
            TICK_US.saturating_sub(lamp_ipc::monotonic_us().saturating_sub(started)),
        ));
    }
    eprintln!(
        "lamp-expression: stopped steps={} refused={} dropped={} output_errors={} longest_step_us={} records_lost={}",
        service.counts.steps,
        service.counts.refused,
        service.counts.dropped,
        service.counts.output_errors,
        service.counts.longest_step_us,
        recorder
            .as_ref()
            .map_or(0, |r| r.lost.load(Ordering::Relaxed))
    );
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse(std::env::args().skip(1)) {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(why) => {
            eprintln!("lamp-expression: {why}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lamp-expression: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(text: &str) -> Result<Option<Arguments>, String> {
        parse(text.split_whitespace().map(str::to_owned))
    }

    #[test]
    fn outputs_require_the_explicit_enable_switch() {
        assert!(!args("serve").unwrap().unwrap().enabled);
        assert!(
            args("serve --enable --bus-root /tmp/bus")
                .unwrap()
                .unwrap()
                .enabled
        );
        assert!(
            !args("replay --input fixture --clock-id test")
                .unwrap()
                .unwrap()
                .enabled
        );
    }

    #[test]
    fn invalid_options_cannot_accidentally_enable_a_service() {
        for input in [
            "",
            "start",
            "serve --enable --enable",
            "serve --seconds 0",
            "serve --seconds 86401",
            "serve --bus-root relative",
            "serve --input replay",
            "replay",
            "replay --input f",
            "replay --input f --clock-id t --until-us -1",
            "import --clock-id t",
            "catalogue --enable",
            "clock --enable",
            "serve --enable true",
        ] {
            assert!(args(input).is_err(), "{input}");
        }
        assert!(args("--help").unwrap().is_none());
    }
}
