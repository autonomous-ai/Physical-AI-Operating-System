//! `lamp-setup`: a lamp's first boot and its way back there (#806).
use lamp_bus::Bus;
use lamp_msgs::TouchEvent;
use lamp_setup::reset::{self, Plan, System};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant},
};

const NAME: &str = "setup";

const USAGE: &str = "\
usage: lamp-setup COMMAND [OPTIONS]
  check --plan FILE
      Check a reset plan and print it. Touches nothing.
  watch-reset --plan FILE [--bus-root PATH] [--clock-id ID] [--seconds N] [--enable]
      Watch controls/touch for the reset button held 5 s and released. With
      --enable: stop the lamp, wipe the person's data, remove the setup
      marker and reboot. Without it, a valid reset is only logged.
  resume-reset --plan FILE [--enable]
      At start, before the lamp: finish a reset a power cut interrupted.
      Without --enable it only says whether one is unfinished.

The plan names the setup marker, the reset journal, the paths to wipe and
the stop and reboot commands (docs/setup.md, \"The reset\").
";

fn options(
    given: impl Iterator<Item = String>,
) -> Result<(BTreeMap<String, String>, bool), String> {
    let mut given = given.peekable();
    let mut options = BTreeMap::new();
    let mut enabled = false;
    while let Some(key) = given.next() {
        if key == "--enable" {
            if enabled {
                return Err("--enable given twice".into());
            }
            enabled = true;
            continue;
        }
        if !["--plan", "--bus-root", "--clock-id", "--seconds"].contains(&key.as_str()) {
            return Err(format!("unknown option {key}"));
        }
        let value = given.next().ok_or(format!("{key} needs a value"))?;
        if options.insert(key.clone(), value).is_some() {
            return Err(format!("{key} given twice"));
        }
    }
    Ok((options, enabled))
}

fn plan(path: &Path) -> Result<Plan, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(16_385).read_to_end(&mut bytes)?;
    if bytes.len() > 16_384 {
        return Err("the plan exceeds 16 KiB".into());
    }
    let plan: Plan = serde_json::from_slice(&bytes)?;
    plan.check()?;
    Ok(plan)
}

fn clock_id(given: Option<&String>) -> io::Result<String> {
    match given {
        Some(id) => Ok(id.clone()),
        None => Ok(std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?
            .trim()
            .to_owned()),
    }
}

fn run() -> Result<bool, Box<dyn std::error::Error>> {
    let mut given = std::env::args().skip(1);
    let command = given.next().ok_or("a command is required")?;
    if command == "--help" || command == "-h" {
        print!("{USAGE}");
        return Ok(true);
    }
    let (mut options, enabled) = options(given)?;
    let plan_path = PathBuf::from(options.remove("--plan").ok_or("--plan is required")?);
    let plan = plan(&plan_path)?;
    match command.as_str() {
        "check" => {
            if enabled || !options.is_empty() {
                return Err("check takes only --plan".into());
            }
            println!("{}", serde_json::to_string(&plan)?);
            Ok(true)
        }
        "resume-reset" => {
            if !options.is_empty() {
                return Err("resume-reset takes --plan and --enable".into());
            }
            let unfinished = plan.journal.exists();
            if !enabled {
                println!(
                    "{}",
                    serde_json::json!({"enabled": false, "unfinished": unfinished})
                );
                return Ok(true);
            }
            let steps = reset::resume(&plan, &mut System)?;
            if !steps.is_empty() {
                eprintln!("lamp-setup: finished a reset a power cut had interrupted");
            }
            println!("{}", serde_json::json!({"enabled": true, "steps": steps}));
            Ok(true)
        }
        "watch-reset" => {
            let bus = match options.remove("--bus-root") {
                Some(root) => Bus::new(root)?,
                None => Bus::from_environment()?,
            };
            let clock = clock_id(options.get("--clock-id"))?;
            options.remove("--clock-id");
            let until = match options.remove("--seconds") {
                Some(seconds) => {
                    let seconds: u64 = seconds
                        .parse()
                        .ok()
                        .filter(|n| (1..=86_400).contains(n))
                        .ok_or("--seconds must be 1..86400")?;
                    Some(Instant::now() + Duration::from_secs(seconds))
                }
                None => None,
            };
            if !options.is_empty() {
                return Err("watch-reset does not take that option".into());
            }
            let mut touch = bus.subscribe_typed(TouchEvent::TOPIC, NAME, TouchEvent::TYPE)?;
            let watched = lamp_setup::watch(
                &mut touch,
                &clock,
                enabled,
                until,
                lamp_ipc::monotonic_us,
                |line| eprintln!("lamp-setup: {line}"),
                || {
                    let steps = reset::reset(&plan, &mut System)?;
                    eprintln!("lamp-setup: {}", serde_json::to_string(&steps)?);
                    Ok(())
                },
            )?;
            println!(
                "{}",
                serde_json::json!({"enabled": enabled, "watched": format!("{watched:?}")})
            );
            Ok(true)
        }
        _ => Err(format!("unknown command {command}").into()),
    }
}

fn main() -> ExitCode {
    // Its log lines never block it or end it (`lamp_ipc::log_relay`).
    let _log = lamp_ipc::log_relay::guard();
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("lamp-setup: {error}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
