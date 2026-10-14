use lamp_records::{
    RunReport,
    config::{Config, absolute_path, invalid},
    json_file::{self, CONFIG_BYTES, REPORT_BYTES},
    report::{Reporter, diagnostic},
    run,
};
use std::{
    env, io,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const USAGE: &str = "lamp-records defaults | check CONFIG [--apply] [--report RESULT] | report CONFIG RESULT [--seconds N]\ncheck is a dry run unless --apply is present; apply also requires enabled=true in CONFIG.\nDry runs write only stdout. The independent report process publishes the last result every second.";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Defaults,
    Check {
        config: PathBuf,
        apply: bool,
        report: Option<PathBuf>,
    },
    Report {
        config: PathBuf,
        result: PathBuf,
        seconds: Option<u64>,
    },
}

fn arguments(args: &[String]) -> io::Result<Command> {
    match args {
        [one] if one == "defaults" => Ok(Command::Defaults),
        [one, config, rest @ ..] if one == "check" => {
            let mut apply = false;
            let mut report = None;
            let mut options = rest.iter();
            while let Some(option) = options.next() {
                match option.as_str() {
                    "--apply" if !apply => apply = true,
                    "--report" if report.is_none() => {
                        report = Some(PathBuf::from(
                            options
                                .next()
                                .ok_or_else(|| invalid("--report needs a path"))?,
                        ));
                    }
                    _ => return Err(invalid(USAGE)),
                }
            }
            if report.is_some() && !apply {
                return Err(invalid("a dry run cannot write a report file"));
            }
            absolute_path(&PathBuf::from(config))?;
            if let Some(path) = &report {
                absolute_path(path)?;
            }
            Ok(Command::Check {
                config: config.into(),
                apply,
                report,
            })
        }
        [one, config, result, rest @ ..] if one == "report" => {
            let seconds = match rest {
                [] => None,
                [flag, value] if flag == "--seconds" => {
                    let count = value
                        .parse::<u64>()
                        .map_err(|_| invalid("seconds must be an integer"))?;
                    if !(1..=86_400).contains(&count) {
                        return Err(invalid("seconds must be 1..86400"));
                    }
                    Some(count)
                }
                _ => return Err(invalid(USAGE)),
            };
            absolute_path(&PathBuf::from(config))?;
            absolute_path(&PathBuf::from(result))?;
            Ok(Command::Report {
                config: config.into(),
                result: result.into(),
                seconds,
            })
        }
        _ => Err(invalid(USAGE)),
    }
}

fn now() -> io::Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs())
}

fn print(value: &impl serde::Serialize) -> io::Result<()> {
    use io::Write;
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value).map_err(io::Error::other)?;
    writeln!(stdout)?;
    Ok(())
}

fn synthetic(config: &Config, at_unix_s: u64, error: Option<String>) -> RunReport {
    RunReport {
        schema_version: 1,
        config_fingerprint: config.fingerprint(),
        at_unix_s,
        mode: "apply".into(),
        enabled: config.enabled,
        roots: vec![],
        error,
    }
}

fn persist(config: &Config, path: &Path, result: &mut RunReport) {
    let persisted = result
        .validate()
        .and_then(|()| json_file::write(path, result));
    if let Err(error) = persisted {
        // An oversized/invalid result must not poison the next timer run.
        // Keep detailed outcomes on stdout and try a small, valid failure
        // receipt for the live reporter.
        result.error = Some(format!("cannot persist full cleanup result: {error}"));
        let failure = synthetic(
            config,
            result.at_unix_s,
            Some("cannot persist full cleanup result; see cleanup stdout".into()),
        );
        if json_file::write(path, &failure).is_err() {
            eprintln!("lamp-records: cannot persist even the failure receipt");
        }
    }
}

fn reporting(config_path: PathBuf, result: PathBuf, seconds: Option<u64>) -> io::Result<()> {
    let mut config: Config = json_file::read(&config_path, CONFIG_BYTES)?;
    config.validate()?;
    let mut config_stamp = None;
    let mut result_stamp = None;
    let mut cached: Option<RunReport> = None;
    let mut publisher: Option<Reporter> = None;
    let began = Instant::now();
    let mut last_error = None;
    loop {
        let step = Instant::now();
        let at = now()?;
        let config_result =
            json_file::read_changed::<Config>(&config_path, CONFIG_BYTES, config_stamp.as_ref());
        let config_error = match config_result {
            Ok(Some((next, stamp))) => match next.validate() {
                Ok(()) => {
                    if next.bus_root != config.bus_root {
                        publisher = None;
                    }
                    config = next;
                    config_stamp = Some(stamp);
                    None
                }
                Err(_) => Some("records configuration is invalid".to_owned()),
            },
            Ok(None) => None,
            Err(_) => Some("records configuration cannot be read".to_owned()),
        };
        let fallback;
        let status = if let Some(error) = config_error {
            fallback = synthetic(&config, at, Some(error));
            &fallback
        } else if !config.enabled {
            fallback = synthetic(&config, at, None);
            &fallback
        } else {
            match json_file::read_changed::<RunReport>(&result, REPORT_BYTES, result_stamp.as_ref())
            {
                Ok(Some((report, stamp))) => {
                    cached = Some(report);
                    result_stamp = Some(stamp);
                }
                Ok(None) => {}
                Err(_) => {
                    cached = None;
                    result_stamp = None;
                }
            }
            if let Some(report) = cached
                .as_ref()
                .filter(|report| report.config_fingerprint == config.fingerprint())
            {
                report
            } else {
                fallback = synthetic(
                    &config,
                    at,
                    Some("no readable cleanup result for the current configuration".into()),
                );
                &fallback
            }
        };
        // A file/transport failure never controls or restarts any producer.
        let published = (|| -> io::Result<()> {
            if publisher.is_none() {
                publisher = Some(Reporter::new(&config.bus_root)?);
            }
            publisher
                .as_mut()
                .unwrap()
                .publish(&diagnostic(Some(status), at))
        })();
        if published.is_err() {
            publisher = None;
            if last_error.is_none_or(|time: Instant| time.elapsed() >= Duration::from_secs(60)) {
                eprintln!(
                    "lamp-records: diagnostics unavailable; will retry, cleanup is independent"
                );
                last_error = Some(Instant::now());
            }
        }
        if seconds.is_some_and(|limit| began.elapsed() >= Duration::from_secs(limit)) {
            break;
        }
        std::thread::sleep(Duration::from_secs(1).saturating_sub(step.elapsed()));
    }
    Ok(())
}

fn command(command: Command) -> io::Result<u8> {
    match command {
        Command::Defaults => {
            print(&Config::customer_default())?;
            Ok(0)
        }
        Command::Check {
            config: config_path,
            apply,
            report,
        } => {
            let config: Config = json_file::read(&config_path, CONFIG_BYTES)?;
            config.validate()?;
            if let Some(path) = &report {
                let temporary = json_file::temporary_path(path)?;
                if path == &config_path
                    || temporary == config_path
                    || json_file::same_file(&config_path, path)?
                    || json_file::same_file(&config_path, &temporary)?
                {
                    return Err(invalid("report file must not replace the configuration"));
                }
                // Result output is explicitly separate from every retention
                // root and the live bus. Never replace records with a receipt.
                if config.roots.iter().any(|root| path.starts_with(&root.path))
                    || path.starts_with(&config.bus_root)
                {
                    return Err(invalid(
                        "report file must be outside record roots and the bus",
                    ));
                }
                // Refuse an unrelated existing file. Atomic prior receipts can
                // be replaced; corrupted/unknown files need operator review.
                match json_file::read::<RunReport>(path, REPORT_BYTES) {
                    Ok(_) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(_) => {
                        return Err(invalid(
                            "existing report file is not a recognized cleanup result",
                        ));
                    }
                }
            }
            let mut result = run::execute(&config, now()?, apply);
            if let Some(path) = report {
                persist(&config, &path, &mut result);
            }
            print(&result)?;
            Ok(run::exit_code(&result))
        }
        Command::Report {
            config,
            result,
            seconds,
        } => {
            reporting(config, result, seconds)?;
            Ok(0)
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    if args == ["--help"] || args == ["-h"] {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match arguments(&args).and_then(command) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("lamp-records: {error}");
            ExitCode::from(64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(value: &str) -> io::Result<Command> {
        arguments(
            &value
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        )
    }
    #[test]
    fn check_defaults_to_dry_and_never_accepts_a_live_override() {
        assert_eq!(
            args("check /etc/lampOS/records.json").unwrap(),
            Command::Check {
                config: "/etc/lampOS/records.json".into(),
                apply: false,
                report: None,
            }
        );
        for value in [
            "check /a --live none",
            "check /a --ignore-live",
            "check /a --report /r",
            "check /a --apply --apply",
            "check relative",
            "check /a --apply --report",
        ] {
            assert!(args(value).is_err(), "{value}");
        }
        assert!(args("check /a --apply --report /run/records/last.json").is_ok());
    }
    #[test]
    fn reporter_has_explicit_bounded_test_lifetime() {
        assert!(args("report /config /last --seconds 1").is_ok());
        for value in [
            "report /config /last --seconds 0",
            "report /config /last --seconds 86401",
            "report /config /last extra",
        ] {
            assert!(args(value).is_err());
        }
    }

    #[test]
    fn output_cannot_overwrite_config_or_an_unrelated_file() {
        use std::{fs, os::unix::fs::PermissionsExt};
        let dir = fs::canonicalize(env::temp_dir())
            .unwrap()
            .join(format!("lamp-records-cli-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let config_path = dir.join("records.json");
        let bytes = serde_json::to_vec(&Config::customer_default()).unwrap();
        fs::write(&config_path, &bytes).unwrap();
        let result = command(Command::Check {
            config: config_path.clone(),
            apply: true,
            report: Some(config_path.clone()),
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&config_path).unwrap(), bytes);
        let unrelated = dir.join("unrelated.json");
        fs::write(&unrelated, b"{}").unwrap();
        assert!(
            command(Command::Check {
                config: config_path,
                apply: true,
                report: Some(unrelated.clone())
            })
            .is_err()
        );
        assert_eq!(fs::read(unrelated).unwrap(), b"{}");
        let reserved_config = dir.join(".last.json.lamp-records-tmp");
        fs::write(&reserved_config, &bytes).unwrap();
        fs::set_permissions(&reserved_config, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            command(Command::Check {
                config: reserved_config.clone(),
                apply: true,
                report: Some(dir.join("last.json"))
            })
            .is_err()
        );
        assert_eq!(fs::read(reserved_config).unwrap(), bytes);
        assert!(!dir.join("last.json").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn old_result_does_not_match_a_changed_age_policy_or_enable_switch() {
        let config = Config::customer_default();
        let result = synthetic(&config, 10, None);
        assert_eq!(result.config_fingerprint, config.fingerprint());
        let mut changed = config.clone();
        changed.enabled = true;
        assert_ne!(result.config_fingerprint, changed.fingerprint());
        changed = config;
        changed.roots[0].max_age_seconds += 1;
        assert_ne!(result.config_fingerprint, changed.fingerprint());
    }

    #[test]
    fn oversized_result_leaves_readable_failure_and_next_cleanup_can_replace_it() {
        use lamp_records::{Disk, RootReport};
        use std::{fs, os::unix::fs::PermissionsExt};
        let dir = fs::canonicalize(env::temp_dir())
            .unwrap()
            .join(format!("lamp-records-receipt-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.join("last.json");
        let config = Config::customer_default();
        let mut result = synthetic(&config, 10, None);
        result.roots.push(RootReport {
            root: config.roots[0].path.clone(),
            unit: config.roots[0].unit.clone(),
            before_bytes: 0,
            planned_bytes: 0,
            removed_bytes: 0,
            remaining_bytes: 0,
            limit_bytes: config.roots[0].max_bytes,
            active: vec![],
            planned: vec![],
            removed: vec![],
            refused: vec!["x".repeat(1024); 9000],
            processes: lamp_records::ProcessObservations::default(),
            disk: Some(Disk {
                total_bytes: 100,
                available_bytes: 100,
            }),
            error: None,
        });
        result.validate().unwrap();
        persist(&config, &path, &mut result);
        assert!(result.error.is_some());
        assert_eq!(result.roots[0].refused.len(), 9000);
        let failure: RunReport = json_file::read(&path, REPORT_BYTES).unwrap();
        assert!(failure.error.is_some());
        assert!(failure.roots.is_empty());
        assert_eq!(failure.config_fingerprint, config.fingerprint());
        assert_eq!(
            diagnostic(Some(&failure), 10).level,
            lamp_msgs::Level::Error
        );
        let config_path = dir.join("records.json");
        json_file::write(&config_path, &config).unwrap();
        assert_eq!(
            command(Command::Check {
                config: config_path,
                apply: true,
                report: Some(path.clone()),
            })
            .unwrap(),
            0
        );
        let recovered: RunReport = json_file::read(&path, REPORT_BYTES).unwrap();
        assert!(recovered.error.is_none());
        assert!(!recovered.enabled);
        assert!(!json_file::temporary_path(&path).unwrap().exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
