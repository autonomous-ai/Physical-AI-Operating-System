//! The lamp's memory: one process that keeps, on the lamp's own disk, each
//! exchange the lamp took part in and what it was asked to keep, and answers
//! on a local socket when the voice asks for something to be remembered,
//! recalled or forgotten. Its own process so that the lamp talks on, without
//! recalling earlier days, when this one has stopped.
//!
//! It runs until it is killed. The standard library cannot catch a signal
//! and this crate takes in nothing that can, so SIGTERM ends it where it
//! stands. Nothing remembered is lost by that: a line is on the disk before
//! its request is answered. The socket file and `ready` are left behind, and
//! the next process clears them as it starts.

use lamp_memory::{
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

const USAGE: &str = "usage: lamp-memory --dir ABSOLUTE_DIR --socket ABSOLUTE_SOCKET_PATH [--keep-days N] [--utc-offset-minutes M]";

const HELP: &str = "Keeps what the lamp remembers under --dir, a directory only its owner can enter (made\nwith mode 0700 if it is not there; refused if its group or everyone can enter it): one\ndirectory for each person, with a journal file for each local day and a file of what\nthe lamp was asked to keep. Text only. Answers requests, one JSON object on a line each,\non the Unix stream socket at --socket (mode 0600), and keeps a file named ready beside\nthe socket while it is taking them. --keep-days N (default 30, at least 1): a journal\nday is deleted once it is more than N days before today. --utc-offset-minutes M\n(default 0, from -720 to 840): how far east of UTC the lamp's clock is, which decides\nwhere one day ends and the next begins. See docs/memory.md.";

#[derive(Debug, Eq, PartialEq)]
struct Arguments {
    dir: PathBuf,
    socket: PathBuf,
    keep_days: u32,
    utc_offset_minutes: i32,
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Arguments>, &'static str> {
    let mut args = args.into_iter();
    let (mut dir, mut socket, mut keep_days, mut utc_offset_minutes) = (None, None, None, None);
    while let Some(key) = args.next() {
        if key == "--help" {
            let alone = dir.is_none()
                && socket.is_none()
                && keep_days.is_none()
                && utc_offset_minutes.is_none()
                && args.next().is_none();
            return if alone {
                Ok(None)
            } else {
                Err("--help must be used alone")
            };
        }
        let known = matches!(
            key.to_str(),
            Some("--dir" | "--socket" | "--keep-days" | "--utc-offset-minutes")
        );
        if !known {
            return Err("unknown option");
        }
        let value = args.next().ok_or("every option requires a value")?;
        match key.to_str() {
            Some("--dir") if dir.is_none() => dir = Some(PathBuf::from(value)),
            Some("--socket") if socket.is_none() => socket = Some(PathBuf::from(value)),
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
    };
    if !arguments.dir.is_absolute() || !arguments.socket.is_absolute() {
        return Err("--dir and --socket are absolute paths");
    }
    if arguments.socket.file_name().is_none() {
        return Err("--socket is the path of a socket file");
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
    server.run(&mut store, &mut now_unix_ms, &mut log)
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
