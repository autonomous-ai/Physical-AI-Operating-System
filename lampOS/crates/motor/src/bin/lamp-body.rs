//! The lamp's small expressive body: one process that owns the servo port
//! and moves the head a few degrees for what the lamp is doing.

use lamp_motor::calibration::UnitId;
use std::{ffi::OsString, path::PathBuf};

const USAGE: &str = "lamp-body --device ABSOLUTE_PATH --unit-id EXPLICIT_ID --lock-directory PRIVATE_ABSOLUTE_DIRECTORY --mood-socket ABSOLUTE_PATH [--home PITCH,ROLL,YAW] [--reach UP,DOWN,ROLL,YAW] [--speed COUNTS_PER_SECOND] [--servo-speed N] [--servo-acceleration N]\n\nOwns the servo port and moves head pitch, head roll and base yaw a few degrees around\na home pose (the one given, in encoder counts, or else where they are found), for the mood\nit is told on the socket. Never touches torque, the\nshoulder or the elbow. Refuses to start unless the three joints have torque on, a clear\nstatus and the head is inside its start window. Stops at the first surprise, holding\nwhere it is. Ends on SIGINT or SIGTERM after easing back to where it started.\nThe mood socket takes datagrams of JSON: {\"mood\":\"resting|listening|thinking|speaking\",\"level\":0..255}.\nPrints one JSON report when it ends.\n";

#[derive(Debug)]
struct Arguments {
    device: PathBuf,
    unit_id: UnitId,
    lock_directory: PathBuf,
    mood_socket: PathBuf,
    home: Option<[u16; 3]>,
    reach: Option<[u16; 4]>,
    speed: Option<u16>,
    servo_speed: Option<u16>,
    servo_acceleration: Option<u8>,
}

/// Whole numbers separated by commas, exactly as many as asked for.
fn counts_of<const N: usize>(value: &str) -> Option<[u16; N]> {
    let parts: Vec<u16> = value
        .split(',')
        .map(|part| part.parse::<u16>().ok().filter(|_| !part.starts_with('+')))
        .collect::<Option<_>>()?;
    parts.try_into().ok()
}

/// Three encoder positions, each a whole number of counts on one turn.
fn home_of(value: &str) -> Option<[u16; 3]> {
    let mut parts = value.split(',').map(|part| {
        part.parse::<u16>()
            .ok()
            .filter(|count| *count <= 4_095 && !part.starts_with('+'))
    });
    let home = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(home)
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Arguments>, &'static str> {
    let mut args = args.into_iter();
    let (mut device, mut unit_id, mut lock_directory, mut mood_socket) = (None, None, None, None);
    let mut home = None;
    let (mut reach, mut speed, mut servo_speed, mut servo_acceleration) = (None, None, None, None);
    while let Some(key) = args.next() {
        if key == "--help" {
            if device.is_some()
                || unit_id.is_some()
                || lock_directory.is_some()
                || mood_socket.is_some()
                || home.is_some()
                || reach.is_some()
                || speed.is_some()
                || servo_speed.is_some()
                || servo_acceleration.is_some()
                || args.next().is_some()
            {
                return Err("--help must be used alone");
            }
            return Ok(None);
        }
        let value = args.next().ok_or("every option requires a value")?;
        match key.to_str() {
            Some("--device") if device.is_none() => device = Some(PathBuf::from(value)),
            Some("--lock-directory") if lock_directory.is_none() => {
                lock_directory = Some(PathBuf::from(value));
            }
            Some("--mood-socket") if mood_socket.is_none() => {
                mood_socket = Some(PathBuf::from(value));
            }
            Some("--home") if home.is_none() => {
                home = Some(
                    value
                        .to_str()
                        .and_then(home_of)
                        .ok_or("--home is PITCH,ROLL,YAW in encoder counts")?,
                );
            }
            Some("--unit-id") if unit_id.is_none() => {
                unit_id = Some(
                    UnitId::new(value.to_str().ok_or("unit ID must be UTF-8")?)
                        .map_err(|_| "invalid explicit unit ID")?,
                );
            }
            Some("--reach") if reach.is_none() => {
                reach = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<4>)
                        .ok_or("--reach is UP,DOWN,ROLL,YAW in encoder counts")?,
                );
            }
            Some("--speed") if speed.is_none() => {
                speed = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<1>)
                        .ok_or("--speed is a whole number of counts a second")?[0],
                );
            }
            Some("--servo-speed") if servo_speed.is_none() => {
                servo_speed = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<1>)
                        .ok_or("--servo-speed is a whole number")?[0],
                );
            }
            Some("--servo-acceleration") if servo_acceleration.is_none() => {
                servo_acceleration = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<1>)
                        .and_then(|value| u8::try_from(value[0]).ok())
                        .ok_or("--servo-acceleration is a whole number under 256")?,
                );
            }
            _ => return Err("unknown or duplicate option"),
        }
    }
    let result = Arguments {
        device: device.ok_or("--device is required")?,
        unit_id: unit_id.ok_or("--unit-id is required")?,
        lock_directory: lock_directory.ok_or("--lock-directory is required")?,
        mood_socket: mood_socket.ok_or("--mood-socket is required")?,
        home,
        reach,
        speed,
        servo_speed,
        servo_acceleration,
    };
    for path in [&result.device, &result.lock_directory, &result.mood_socket] {
        if !path.is_absolute()
            || path.as_os_str().len() > 100
            || path.to_str().is_none()
            || path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err("paths must be short absolute UTF-8 paths without parent traversal");
        }
    }
    Ok(Some(result))
}

/// One datagram from the voice service. Anything else is ignored.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn mood_of(datagram: &[u8]) -> Option<(lamp_motor::gesture::Mood, u8)> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Message {
        mood: lamp_motor::gesture::Mood,
        level: u8,
    }
    let message: Message = serde_json::from_slice(datagram).ok()?;
    Some((message.mood, message.level))
}

fn main() -> std::process::ExitCode {
    let arguments = match parse(std::env::args_os().skip(1)) {
        Ok(None) => {
            print!("{USAGE}");
            return std::process::ExitCode::SUCCESS;
        }
        Ok(Some(arguments)) => arguments,
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            return std::process::ExitCode::from(2);
        }
    };
    match run(arguments) {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::FAILURE,
        Err(error) => {
            eprintln!("lamp-body failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn run(arguments: Arguments) -> Result<bool, Box<dyn std::error::Error>> {
    let _ = (arguments.unit_id, arguments.mood_socket, arguments.home);
    let _ = (
        arguments.reach,
        arguments.speed,
        arguments.servo_speed,
        arguments.servo_acceleration,
    );
    Err("the body requires Linux; no device was opened".into())
}

#[cfg(target_os = "linux")]
fn run(arguments: Arguments) -> Result<bool, Box<dyn std::error::Error>> {
    use lamp_motor::{
        body::{Limits, Stop, Tuning},
        linux::{InspectionConfig, run_body},
    };
    use nix::sys::{
        signal::{SigSet, Signal},
        signalfd::{SfdFlags, SignalFd},
    };
    use std::{io::Write, os::unix::net::UnixDatagram};

    struct SignalMask(SigSet);
    impl Drop for SignalMask {
        fn drop(&mut self) {
            let _ = self.0.thread_set_mask();
        }
    }
    let config = InspectionConfig::new(
        arguments.device,
        arguments.unit_id,
        arguments.lock_directory,
    )?;
    // A socket file left by an earlier run is not a listener: replace it.
    match std::fs::remove_file(&arguments.mood_socket) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let moods = UnixDatagram::bind(&arguments.mood_socket)?;
    moods.set_nonblocking(true)?;
    let old_mask = SigSet::thread_get_mask()?;
    let mut mask = SigSet::empty();
    mask.add(Signal::SIGINT);
    mask.add(Signal::SIGTERM);
    mask.thread_block()?;
    let restore = SignalMask(old_mask);
    let signal_fd = SignalFd::with_flags(&mask, SfdFlags::SFD_CLOEXEC | SfdFlags::SFD_NONBLOCK)?;
    let mut cancelled = false;
    let news = || {
        // Everything waiting, and only the newest of it counts.
        let mut latest = None;
        let mut datagram = [0u8; 128];
        for _ in 0..32 {
            match moods.recv(&mut datagram) {
                Ok(length) => latest = mood_of(&datagram[..length]).or(latest),
                Err(_) => break,
            }
        }
        latest
    };
    // What is not chosen stays as the folded limits have it.
    let folded = Limits::FOLDED;
    let limits = Limits::folded_with(Tuning {
        reach: arguments.reach.unwrap_or([
            folded.pitch_up as u16,
            folded.pitch_down as u16,
            folded.roll as u16,
            folded.yaw as u16,
        ]),
        speed: arguments.speed.unwrap_or(folded.speed as u16),
        servo_speed: arguments.servo_speed.unwrap_or(folded.servo_speed),
        servo_acceleration: arguments
            .servo_acceleration
            .unwrap_or(folded.servo_acceleration),
    })?;
    // One line to the journal on each change of mood, and twice a second
    // while the goals are changing: what it did, afterwards.
    let mut said: Option<(lamp_motor::gesture::Mood, u64, [u16; 3])> = None;
    let mut eased = [0u16; 3];
    let watch = |glimpse: &lamp_motor::body::Glimpse| {
        let due = match said {
            None => true,
            Some((mood, _, _)) if mood != glimpse.mood => true,
            Some((_, step, goals)) => goals != glimpse.goals && glimpse.step - step >= 25,
        };
        if glimpse.eased != eased {
            eased = glimpse.eased;
            eprintln!(
                "lamp-body: step {} met resistance and stopped pushing; so far {:?}; positions {:?} loads {:?}",
                glimpse.step, glimpse.eased, glimpse.positions, glimpse.loads
            );
        }
        if due {
            said = Some((glimpse.mood, glimpse.step, glimpse.goals));
            eprintln!(
                "lamp-body: step {} {:?} goals {:?} positions {:?} loads {:?}",
                glimpse.step, glimpse.mood, glimpse.goals, glimpse.positions, glimpse.loads
            );
        }
    };
    let result = run_body(&config, limits, arguments.home, news, watch, || {
        if !cancelled {
            cancelled = signal_fd
                .read_signal()
                .map_err(|error| std::io::Error::from_raw_os_error(error as i32))?
                .is_some();
        }
        Ok(cancelled)
    });
    let _ = std::fs::remove_file(&arguments.mood_socket);
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, &result)?;
    writeln!(output)?;
    output.flush()?;
    restore.0.thread_set_mask()?;
    Ok(result.port.preparation_error.is_none()
        && result.port.cleanup.errors.is_empty()
        && result.body.is_some_and(|body| body.stop == Stop::Cancelled))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_motor::gesture::Mood;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    const FULL: [&str; 8] = [
        "--device",
        "/dev/example",
        "--unit-id",
        "lamp-test",
        "--lock-directory",
        "/home/test/private",
        "--mood-socket",
        "/run/lampos-body/mood.sock",
    ];

    #[test]
    fn every_path_is_explicit_and_nothing_else_is_accepted() {
        let value = parse(args(&FULL)).unwrap().unwrap();
        assert_eq!(value.unit_id.as_str(), "lamp-test");
        assert_eq!(
            value.mood_socket,
            PathBuf::from("/run/lampos-body/mood.sock")
        );
        assert_eq!(value.home, None);
        let with = |home: &'static str| parse(args(&[&FULL[..], &["--home", home]].concat()));
        assert_eq!(
            with("1930,2043,2015").unwrap().unwrap().home,
            Some([1_930, 2_043, 2_015])
        );
        let tuned = parse(args(
            &[
                &FULL[..],
                &[
                    "--reach",
                    "120,15,60,150",
                    "--speed",
                    "120",
                    "--servo-speed",
                    "400",
                    "--servo-acceleration",
                    "80",
                ],
            ]
            .concat(),
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            (
                tuned.reach,
                tuned.speed,
                tuned.servo_speed,
                tuned.servo_acceleration
            ),
            (Some([120, 15, 60, 150]), Some(120), Some(400), Some(80))
        );
        assert!(parse(args(&[&FULL[..], &["--reach", "120,15,60"]].concat())).is_err());
        assert!(
            parse(args(
                &[&FULL[..], &["--servo-acceleration", "256"]].concat()
            ))
            .is_err()
        );
        for refused in [
            "1930,2043",
            "1930,2043,2015,1",
            "1930,2043,4096",
            "1930,,2015",
            "+1930,2043,2015",
            "a,b,c",
        ] {
            assert!(with(refused).is_err(), "{refused}");
        }
        assert!(parse(args(&["--help"])).unwrap().is_none());
        for missing in 0..4 {
            let mut some = FULL.to_vec();
            some.drain(missing * 2..missing * 2 + 2);
            assert!(parse(args(&some)).is_err());
        }
        assert!(parse(args(&[&FULL[..], &["--torque", "1"]].concat())).is_err());
        assert!(
            parse(args(
                &[&FULL[..6], &["--mood-socket", "mood.sock"]].concat()
            ))
            .is_err()
        );
        assert!(
            parse(args(
                &[&FULL[..6], &["--mood-socket", "/run/../etc/x"]].concat()
            ))
            .is_err()
        );
    }

    #[test]
    fn a_mood_is_one_exact_json_object_and_anything_else_is_ignored() {
        assert_eq!(
            mood_of(br#"{"mood":"listening","level":120}"#),
            Some((Mood::Listening, 120))
        );
        assert_eq!(
            mood_of(br#"{"mood":"resting","level":0}"#),
            Some((Mood::Resting, 0))
        );
        for ignored in [
            &br#"{"mood":"dancing","level":1}"#[..],
            br#"{"mood":"speaking","level":256}"#,
            br#"{"mood":"speaking"}"#,
            br#"{"mood":"speaking","level":1,"goal":2100}"#,
            b"listening",
            b"",
        ] {
            assert_eq!(mood_of(ignored), None);
        }
    }
}
