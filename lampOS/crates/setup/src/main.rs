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
  serve --config FILE --plan FILE [--seconds N] [--enable]
      The lamp's first boot, started by the keeper when the setup marker is
      missing: finish an interrupted reset, bring up the lamp's hotspot and
      answer V1's setup API until the home's Wi-Fi is joined and the
      person's settings are saved; then write the marker, and go on
      answering for 10 minutes so the page can follow the lamp.
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
        if ![
            "--plan",
            "--config",
            "--bus-root",
            "--clock-id",
            "--seconds",
        ]
        .contains(&key.as_str())
        {
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
        "serve" => {
            let config_path =
                PathBuf::from(options.remove("--config").ok_or("--config is required")?);
            let seconds = match options.remove("--seconds") {
                Some(seconds) => Some(
                    seconds
                        .parse::<u64>()
                        .ok()
                        .filter(|n| (1..=86_400).contains(n))
                        .ok_or("--seconds must be 1..86400")?,
                ),
                None => None,
            };
            if !options.is_empty() {
                return Err("serve takes --config, --plan, --seconds and --enable".into());
            }
            let config = serve_config(&config_path)?;
            if !enabled {
                println!(
                    "{}",
                    serde_json::json!({"enabled": false, "reason": "--enable was not set"})
                );
                return Ok(true);
            }
            serve(config, plan, seconds)
        }
        _ => Err(format!("unknown command {command}").into()),
    }
}

fn serve_config(path: &Path) -> Result<lamp_setup::serve::Config, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(16_385).read_to_end(&mut bytes)?;
    if bytes.len() > 16_384 {
        return Err("the configuration exceeds 16 KiB".into());
    }
    let config: lamp_setup::serve::Config = serde_json::from_slice(&bytes)?;
    config.check()?;
    Ok(config)
}

/// How long the page may still need the server after setup is done.
const LINGER: Duration = Duration::from_secs(600);

fn serve(
    config: lamp_setup::serve::Config,
    plan: Plan,
    seconds: Option<u64>,
) -> Result<bool, Box<dyn std::error::Error>> {
    use lamp_setup::{
        http::{Shared, router},
        serve::{Files, Status},
        wifi::{Host, ImageHost},
    };
    use std::sync::{Arc, Mutex};
    // A reset a power cut or a failed stop left unfinished comes first.
    let finished = reset::resume(&plan, &mut System)?;
    if !finished.is_empty() {
        eprintln!("lamp-setup: finished an interrupted reset");
    }
    if config.marker.exists() {
        eprintln!("lamp-setup: this lamp is already set up; nothing to serve");
        return Ok(true);
    }
    let mac = std::fs::read_to_string("/etc/hostname")
        .map(|name| name.trim().to_owned())
        .unwrap_or_default();
    let status = Arc::new(Mutex::new(Status::new(mac)));
    // The hotspot, as V1's os-server raises it at a first boot.
    ImageHost::new().hotspot()?;
    eprintln!(
        "lamp-setup: the lamp's hotspot is up; answering on {}",
        config.listen
    );
    let shared = Arc::new(Shared {
        config: config.clone(),
        status: Arc::clone(&status),
        start: Box::new({
            let status = Arc::clone(&status);
            let config = config.clone();
            move |accepted| {
                let status = Arc::clone(&status);
                let config = config.clone();
                std::thread::spawn(move || {
                    // V1 replies first and starts 2 s later, so the reply
                    // reaches the phone before the hotspot goes.
                    std::thread::sleep(Duration::from_secs(2));
                    let interface = config.interface.clone();
                    let hotspot = config.hotspot_address.clone();
                    lamp_setup::serve::run(
                        &status,
                        &accepted,
                        &config.country,
                        &mut ImageHost::new(),
                        &mut Files(&config),
                        move || lamp_setup::serve::station_ip(&interface, &hotspot),
                    );
                    let now = status.lock().unwrap().clone();
                    eprintln!(
                        "lamp-setup: run {} ended {:?}{}",
                        now.run,
                        now.phase,
                        if now.error.is_empty() {
                            String::new()
                        } else {
                            format!(": {}", now.error)
                        }
                    );
                });
            }
        }),
        scan: Box::new({
            let interface = config.interface.clone();
            move || lamp_setup::scan::scan(&interface)
        }),
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&config.listen).await?;
        let limit = seconds.map(Duration::from_secs);
        let started = tokio::time::Instant::now();
        let done = async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if limit.is_some_and(|limit| started.elapsed() >= limit) {
                    return;
                }
                let completed = status.lock().unwrap().set_up_completed;
                if completed {
                    tokio::time::sleep(LINGER).await;
                    return;
                }
            }
        };
        axum::serve(listener, router(shared))
            .with_graceful_shutdown(done)
            .await
    })?;
    Ok(true)
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
