//! The lamp's memory: one process that keeps, on the lamp's own disk, each
//! exchange the lamp took part in and what it was asked to keep, and answers
//! on a local socket when the voice asks for something to be remembered,
//! recalled or forgotten. Its own process so that the lamp talks on, without
//! recalling earlier days, when this one has stopped.
//!
//! It runs until it is killed. The standard library cannot catch a signal
//! and this binary registers no signal handler, so SIGTERM ends it where it
//! stands. Nothing remembered is lost by that: a line is on the disk before
//! its request is answered. The socket file and `ready` are left behind, and
//! the next process clears them as it starts.

use lamp_memory::{
    health::{CountSource, Reporter, State},
    server::Server,
    store::{Options, Store, private_dir},
};
use std::{
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

const USAGE: &str = "usage: lamp-memory --dir ABSOLUTE_DIR --socket ABSOLUTE_SOCKET_PATH [--keep-days N] [--utc-offset-minutes M] [--memory-days] [--bus-root ABSOLUTE_DIR]";

const HELP: &str = "Keeps what the lamp remembers under --dir, a directory only its owner can enter (made\nwith mode 0700 if it is not there; refused if its group or everyone can enter it).\nThe default stores a journal file for each local day and a kept file per person.\n--memory-days opts into SQLite, ranked search and linked observations, importing and\nremoving the old files before serving. That directory then requires --memory-days;\nno model is configured or called. See docs/memory-days.md before enabling it.\nAnswers one JSON object per line on --socket (mode 0600), with ready beside the\nsocket while taking requests. --keep-days N (default 30, at least 1): delete journal\ndays more than N days before today. --utc-offset-minutes M (default 0, -720 to 840):\nminutes east of UTC for day boundaries.\n--bus-root ABSOLUTE_DIR opts into a memory health report once a second. The bus\nis optional and cannot hold the request loop; without this flag no reports run.\nSee docs/memory.md.";

#[derive(Debug, Eq, PartialEq)]
struct Arguments {
    dir: PathBuf,
    socket: PathBuf,
    keep_days: u32,
    utc_offset_minutes: i32,
    memory_days: bool,
    bus_root: Option<PathBuf>,
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Arguments>, &'static str> {
    let mut args = args.into_iter();
    let (mut dir, mut socket, mut keep_days, mut utc_offset_minutes) = (None, None, None, None);
    let mut memory_days = false;
    let mut bus_root = None;
    while let Some(key) = args.next() {
        if key == "--help" {
            let alone = dir.is_none()
                && socket.is_none()
                && keep_days.is_none()
                && utc_offset_minutes.is_none()
                && !memory_days
                && bus_root.is_none()
                && args.next().is_none();
            return if alone {
                Ok(None)
            } else {
                Err("--help must be used alone")
            };
        }
        if key == "--memory-days" {
            if memory_days {
                return Err("an option was given twice");
            }
            memory_days = true;
            continue;
        }
        let known = matches!(
            key.to_str(),
            Some("--dir" | "--socket" | "--keep-days" | "--utc-offset-minutes" | "--bus-root")
        );
        if !known {
            return Err("unknown option");
        }
        let value = args.next().ok_or("every option requires a value")?;
        match key.to_str() {
            Some("--dir") if dir.is_none() => dir = Some(PathBuf::from(value)),
            Some("--socket") if socket.is_none() => socket = Some(PathBuf::from(value)),
            Some("--bus-root") if bus_root.is_none() => bus_root = Some(PathBuf::from(value)),
            Some("--keep-days") if keep_days.is_none() => {
                let days = value.to_str().and_then(|value| value.parse::<u32>().ok());
                keep_days = Some(
                    days.filter(|days| *days >= 1)
                        .ok_or("--keep-days is a whole number of days, at least 1")?,
                );
            }
            Some("--utc-offset-minutes") if utc_offset_minutes.is_none() => {
                let minutes = value.to_str().and_then(|value| value.parse::<i32>().ok());
                utc_offset_minutes = Some(
                    minutes
                        .filter(|minutes| (-720..=840).contains(minutes))
                        .ok_or("--utc-offset-minutes is a whole number from -720 to 840")?,
                );
            }
            _ => return Err("an option was given twice"),
        }
    }
    let arguments = Arguments {
        dir: dir.ok_or("--dir is required")?,
        socket: socket.ok_or("--socket is required")?,
        keep_days: keep_days.unwrap_or(30),
        utc_offset_minutes: utc_offset_minutes.unwrap_or(0),
        memory_days,
        bus_root,
    };
    if !arguments.dir.is_absolute() || !arguments.socket.is_absolute() {
        return Err("--dir and --socket are absolute paths");
    }
    if arguments.socket.file_name().is_none() {
        return Err("--socket is the path of a socket file");
    }
    if arguments
        .bus_root
        .as_ref()
        .is_some_and(|root| !root.is_absolute())
    {
        return Err("--bus-root is an absolute path");
    }
    Ok(Some(arguments))
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// One line to the journal. A journal that has gone away is no reason for
/// the memory to stop.
fn log(line: &str) {
    let _ = writeln!(io::stderr(), "lamp-memory: {line}");
}

fn run(arguments: &Arguments) -> io::Result<()> {
    // The socket first, so that a second process started by mistake stops
    // here, before it has touched the store; the store before `ready`.
    let server = Server::bind(&arguments.socket)?;
    let mut reporter = arguments.bus_root.as_ref().and_then(|root| {
        Reporter::start(
            root.clone(),
            if arguments.memory_days {
                CountSource::Days
            } else {
                CountSource::Legacy(arguments.dir.clone())
            },
        )
        .inspect_err(|_| log("health worker could not start; serving without reports"))
        .ok()
    });
    let result = serve(arguments, server, reporter.as_ref().map(Reporter::state));
    if let Some(reporter) = &mut reporter {
        reporter.finish(result.is_err());
    }
    result
}

fn serve(arguments: &Arguments, server: Server, health: Option<&State>) -> io::Result<()> {
    if arguments.memory_days {
        let mut store = lamp_memory::days::Days::open(
            Options {
                dir: arguments.dir.clone(),
                keep_days: arguments.keep_days,
                utc_offset_minutes: arguments.utc_offset_minutes,
            },
            now_unix_ms(),
        )
        .map_err(io::Error::other)?;
        if let Some(health) = health {
            let counts = store
                .enable_health_counts()
                .and_then(|()| store.health_counts());
            health.counts(counts.map_err(io::Error::other));
        }
        log("started memory_days=true; the model adapter is not configured");
        return server.run_reporting(&mut store, &mut now_unix_ms, &mut log, health);
    }
    let mut store = Store::open(
        Options {
            dir: arguments.dir.clone(),
            keep_days: arguments.keep_days,
            utc_offset_minutes: arguments.utc_offset_minutes,
        },
        now_unix_ms(),
    )?;
    let found = store.found();
    log(&format!(
        "started dir={} socket={} keep_days={} utc_offset_minutes={} people={} skipped_lines={} expired_days={} unfinished_files={}",
        arguments.dir.display(),
        arguments.socket.display(),
        arguments.keep_days,
        arguments.utc_offset_minutes,
        found.people,
        found.skipped_lines,
        found.expired_days,
        found.unfinished_files,
    ));
    server.run_reporting(&mut store, &mut now_unix_ms, &mut log, health)
}

fn main() -> ExitCode {
    let arguments = match parse(std::env::args_os().skip(1)) {
        Ok(Some(arguments)) => arguments,
        Ok(None) => {
            println!("{USAGE}\n\n{HELP}");
            return ExitCode::SUCCESS;
        }
        Err(why) => {
            eprintln!("lamp-memory: {why}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    // Before anything else is made: a directory others can enter is the
    // caller's mistake, like a wrong option.
    if let Err(error) = private_dir(&arguments.dir) {
        eprintln!("lamp-memory: {error}");
        return ExitCode::from(2);
    }
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lamp-memory: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(args: &[&str]) -> Result<Option<Arguments>, &'static str> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn the_two_paths_are_required_and_the_days_and_the_offset_have_defaults() {
        assert_eq!(
            arguments(&[
                "--dir",
                "/var/lib/lamp/memory",
                "--socket",
                "/run/lamp/memory.sock"
            ]),
            Ok(Some(Arguments {
                dir: "/var/lib/lamp/memory".into(),
                socket: "/run/lamp/memory.sock".into(),
                keep_days: 30,
                utc_offset_minutes: 0,
                memory_days: false,
                bus_root: None,
            }))
        );
        assert_eq!(
            arguments(&[
                "--utc-offset-minutes",
                "420",
                "--socket",
                "/run/m.sock",
                "--keep-days",
                "7",
                "--dir",
                "/m",
            ]),
            Ok(Some(Arguments {
                dir: "/m".into(),
                socket: "/run/m.sock".into(),
                keep_days: 7,
                utc_offset_minutes: 420,
                memory_days: false,
                bus_root: None,
            }))
        );
        let west = arguments(&[
            "--dir",
            "/m",
            "--socket",
            "/s",
            "--utc-offset-minutes",
            "-300",
        ]);
        assert_eq!(west.unwrap().unwrap().utc_offset_minutes, -300);
        assert_eq!(arguments(&["--help"]), Ok(None));
        assert!(
            arguments(&["--dir", "/m", "--socket", "/s", "--memory-days"])
                .unwrap()
                .unwrap()
                .memory_days
        );
        assert!(
            arguments(&[
                "--dir",
                "/m",
                "--socket",
                "/s",
                "--memory-days",
                "--memory-days"
            ])
            .is_err()
        );
        assert!(arguments(&["--memory-days", "--help"]).is_err());
    }

    #[test]
    fn health_reporting_requires_one_explicit_absolute_bus_root() {
        let base = ["--dir", "/m", "--socket", "/s"];
        let with = |more: &[&str]| arguments(&[&base[..], more].concat());
        assert_eq!(with(&[]).unwrap().unwrap().bus_root, None);
        assert_eq!(
            with(&["--bus-root", "/b"]).unwrap().unwrap().bus_root,
            Some("/b".into())
        );
        for bad in [
            &["--bus-root"][..],
            &["--bus-root", "relative"],
            &["--bus-root", ""],
            &["--bus-root", "/b", "--bus-root", "/b"],
            &["--bus-root", "/b", "--help"],
        ] {
            assert!(with(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_relative_path_an_unknown_option_or_a_bad_number_is_refused() {
        let paths = ["--dir", "/m", "--socket", "/s"];
        let with = |more: &[&str]| arguments(&[&paths[..], more].concat());
        assert!(with(&[]).is_ok());
        for bad in [
            &["--dir", "memory", "--socket", "/s"][..],
            &["--dir", "/m", "--socket", "m.sock"],
            &["--dir", "./m", "--socket", "/s"],
            &["--dir", "", "--socket", "/s"],
            &["--dir", "/m", "--socket", "/"],
            &["--dir", "/m"],
            &["--socket", "/s"],
            &[],
            &["--dir"],
            &["--dir", "/m", "--socket"],
            &["/m", "/s"],
            &["--help", "--dir", "/m"],
            &["--dir", "/m", "--help"],
        ] {
            assert!(arguments(bad).is_err(), "{bad:?}");
        }
        for bad in [
            &["--verbose"][..],
            &["--verbose", "yes"],
            &["-d", "/x"],
            &["--dir", "/again"],
            &["--socket", "/again"],
            &["--keep-days", "0"],
            &["--keep-days", "-3"],
            &["--keep-days", "thirty"],
            &["--keep-days", "3.5"],
            &["--keep-days", ""],
            &["--keep-days", "4294967296"],
            &["--keep-days", "3", "--keep-days", "4"],
            &["--utc-offset-minutes", "841"],
            &["--utc-offset-minutes", "-721"],
            &["--utc-offset-minutes", "+7h"],
            &["--utc-offset-minutes", "420", "--utc-offset-minutes", "420"],
            &["extra"],
        ] {
            assert!(with(bad).is_err(), "{bad:?}");
        }
        assert_eq!(with(&["--verbose"]), Err("unknown option"));
        assert!(with(&["--keep-days", "1", "--utc-offset-minutes", "840"]).is_ok());
        assert!(with(&["--keep-days", "4294967295", "--utc-offset-minutes", "-720"]).is_ok());
    }
}
