//! `lamp-selftest`: checks each part of the lamp once, keeps the report
//! under /var/lib/lampOS/selftest/, publishes it on `lamp/selftest`, and
//! prints one line a part (docs/selftest.md).
use lamp_bus::Bus;
use lamp_msgs::{Occasion, SelfTest, Verdict};
use lamp_selftest::{BUDGET, listen, report};
use std::{path::PathBuf, process::ExitCode, time::Instant};

const USAGE: &str = "usage: lamp-selftest [--occasion factory|first-boot|on-demand] [--dir ABSOLUTE] [--bus-root ABSOLUTE]";
const HELP: &str = "Checks each part of the lamp within two minutes and says pass, fail,
not fitted or not checked for each, with one reason. The report is kept in
--dir (default /var/lib/lampOS/selftest) and published on lamp/selftest.

Exit status: 0 every part passed or is not fitted, 1 a part failed,
3 a part could not be checked, 2 the arguments were wrong.";

struct Arguments {
    occasion: Occasion,
    dir: PathBuf,
    bus_root: Option<PathBuf>,
}

fn parse(given: &[String]) -> Result<Option<Arguments>, String> {
    let mut arguments = Arguments {
        occasion: Occasion::OnDemand,
        dir: report::DIR.into(),
        bus_root: None,
    };
    let mut given = given.iter();
    while let Some(flag) = given.next() {
        let mut value = || given.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "-h" | "--help" => return Ok(None),
            "--occasion" => {
                arguments.occasion = match value()?.as_str() {
                    "factory" => Occasion::Factory,
                    "first-boot" => Occasion::FirstBoot,
                    "on-demand" => Occasion::OnDemand,
                    other => return Err(format!("no occasion {other:?}")),
                }
            }
            "--dir" => arguments.dir = absolute(flag, value()?)?,
            "--bus-root" => arguments.bus_root = Some(absolute(flag, value()?)?),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Some(arguments))
}

fn absolute(flag: &str, value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(format!("{flag} must be an absolute path"))
    }
}

fn main() -> ExitCode {
    // Its log lines never block it or end it (`lamp_ipc::log_relay`).
    let _log = lamp_ipc::log_relay::guard();
    let given: Vec<String> = std::env::args().skip(1).collect();
    let arguments = match parse(&given) {
        Ok(Some(arguments)) => arguments,
        Ok(None) => {
            println!("{USAGE}\n\n{HELP}");
            return ExitCode::SUCCESS;
        }
        Err(why) => {
            eprintln!("lamp-selftest: {why}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let started = Instant::now();
    let bus = match &arguments.bus_root {
        Some(root) => Bus::new(root),
        None => Bus::from_environment(),
    };
    let bus = match bus {
        Ok(bus) => bus,
        Err(error) => {
            eprintln!("lamp-selftest: the bus cannot be opened: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut checks = listen::checks(&bus);
    let test = lamp_selftest::run(arguments.occasion, &mut checks, BUDGET);
    print!("{}", report::text(&test));
    match report::keep(&arguments.dir, &test) {
        Ok(path) => eprintln!("lamp-selftest: kept {}", path.display()),
        Err(error) => eprintln!(
            "lamp-selftest: the report cannot be kept in {}: {error}",
            arguments.dir.display()
        ),
    }
    if let Err(error) = publish(&bus, &test, started) {
        eprintln!("lamp-selftest: {} not published: {error}", SelfTest::TOPIC);
    }
    match test.verdict {
        Verdict::Pass | Verdict::NotFitted => ExitCode::SUCCESS,
        Verdict::Fail => ExitCode::FAILURE,
        Verdict::NotChecked => ExitCode::from(3),
    }
}

fn publish(bus: &Bus, test: &SelfTest, started: Instant) -> std::io::Result<()> {
    let mut out = bus.publisher_declared(SelfTest::TOPIC, &SelfTest::declaration(), "selftest")?;
    out.publish_json(started.elapsed().as_micros() as u64, test)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(given: &[&str]) -> Result<Option<Arguments>, String> {
        parse(&given.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn arguments_have_defaults_and_refuse_what_they_do_not_know() {
        let plain = args(&[]).unwrap().unwrap();
        assert_eq!(plain.occasion, Occasion::OnDemand);
        assert_eq!(plain.dir, PathBuf::from("/var/lib/lampOS/selftest"));
        let factory = args(&["--occasion", "factory", "--dir", "/tmp/x"])
            .unwrap()
            .unwrap();
        assert_eq!(factory.occasion, Occasion::Factory);
        assert!(args(&["--occasion", "later"]).is_err());
        assert!(args(&["--dir", "relative"]).is_err());
        assert!(args(&["--occasion"]).is_err());
        assert!(args(&["--help"]).unwrap().is_none());
    }
}
