//! The lamp's small expressive body: one process that owns the servo port
//! and moves the head a few degrees for what the lamp is doing.

use lamp_motor::calibration::UnitId;
use std::{ffi::OsString, path::PathBuf};

const USAGE: &str = "lamp-body --device ABSOLUTE_PATH --unit-id EXPLICIT_ID --lock-directory PRIVATE_ABSOLUTE_DIRECTORY --mood-socket ABSOLUTE_PATH [--home PITCH,ROLL,YAW] [--reach UP,DOWN,ROLL,YAW] [--speed COUNTS_PER_SECOND] [--servo-speed N] [--servo-acceleration N] [--stand PITCH,ROLL,YAW,SHOULDER,ELBOW --folded-arm SHOULDER,ELBOW [--stand-share PERCENT] [--stand-seconds N] [--awake-seconds N] [--arm-load SHOULDER,ELBOW]] [--gaze-socket ABSOLUTE_PATH --gaze YAW,PITCH [--gaze-reach YAW,PITCH]] [--stand-roll COUNTS]\n\nOwns the servo port and moves head pitch, head roll and base yaw a few degrees around\na home pose (the one given, in encoder counts, or else where they are found), for the mood\nit is told on the socket. Without --stand it never touches torque, the shoulder or the\nelbow. With --stand (which needs --home and --folded-arm) it also raises the shoulder and\nthe elbow to that pose when a mood other than rest arrives, folds them down again\n--awake-seconds after the last one, and writes torque once to a shoulder or elbow found\nwith an old overload latched. With --gaze-socket and --gaze it takes where a face is from\nlamp-eyes on that socket ({\"face\":{...}} or {\"face\":null}) and, while the lamp listens\nor speaks, turns the head to it in small corrections: --gaze is the counts of base yaw and of\nhead pitch, signed, that bring a face from the right and from the bottom edge of the picture\nto its middle. Refuses to start unless the three joints have torque on, a clear\nstatus and the head is inside its start window. Stops at the first surprise, holding\nwhere it is. Ends on SIGINT or SIGTERM after easing back to where it started.\nThe mood socket takes datagrams of JSON: {\"mood\":\"resting|listening|thinking|speaking\",\"level\":0..255}.\nA lamp standing at its full height also makes a gesture asked for there: {\"gesture\":\"nod\"}\n(nod, headshake, greeting, laugh, shy, stretching).\nPrints one JSON report when it ends.\n";

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
    standing: Option<StandingChoice>,
    gaze: Option<GazeChoice>,
    stand_roll: Option<u16>,
}

/// What `--gaze-socket`, `--gaze` and `--gaze-reach` said.
#[derive(Clone, Debug, Eq, PartialEq)]
struct GazeChoice {
    socket: PathBuf,
    turn: [i16; 2],
    reach: [u16; 2],
}

/// What `--stand` and its options said. The body checks it against its own
/// caps when it is made.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StandingChoice {
    folded: [u16; 2],
    pose: [u16; 5],
    share: u8,
    seconds: u8,
    awake: u16,
    load: [u16; 2],
}

/// Whole numbers separated by commas, exactly as many as asked for.
fn counts_of<const N: usize>(value: &str) -> Option<[u16; N]> {
    let parts: Vec<u16> = value
        .split(',')
        .map(|part| part.parse::<u16>().ok().filter(|_| !part.starts_with('+')))
        .collect::<Option<_>>()?;
    parts.try_into().ok()
}

/// Two whole numbers separated by a comma, each with or without a minus.
fn signed_pair(value: &str) -> Option<[i16; 2]> {
    let (first, second) = value.split_once(',')?;
    let number = |part: &str| part.parse::<i16>().ok().filter(|_| !part.starts_with('+'));
    Some([number(first)?, number(second)?])
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
    let (mut stand, mut folded_arm, mut arm_load) = (None, None, None);
    let (mut stand_share, mut stand_seconds, mut awake_seconds) = (None, None, None);
    let (mut gaze_socket, mut gaze_turn, mut gaze_reach) = (None, None, None);
    let mut stand_roll = None;
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
                || stand.is_some()
                || folded_arm.is_some()
                || arm_load.is_some()
                || stand_share.is_some()
                || stand_seconds.is_some()
                || awake_seconds.is_some()
                || gaze_socket.is_some()
                || gaze_turn.is_some()
                || gaze_reach.is_some()
                || stand_roll.is_some()
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
            Some("--stand") if stand.is_none() => {
                stand = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<5>)
                        .ok_or("--stand is PITCH,ROLL,YAW,SHOULDER,ELBOW in encoder counts")?,
                );
            }
            Some("--folded-arm") if folded_arm.is_none() => {
                folded_arm = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<2>)
                        .ok_or("--folded-arm is SHOULDER,ELBOW in encoder counts")?,
                );
            }
            Some("--arm-load") if arm_load.is_none() => {
                arm_load = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<2>)
                        .ok_or("--arm-load is SHOULDER,ELBOW")?,
                );
            }
            Some("--stand-share") if stand_share.is_none() => {
                stand_share = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<1>)
                        .and_then(|value| u8::try_from(value[0]).ok())
                        .ok_or("--stand-share is a whole percentage")?,
                );
            }
            Some("--stand-seconds") if stand_seconds.is_none() => {
                stand_seconds = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<1>)
                        .and_then(|value| u8::try_from(value[0]).ok())
                        .ok_or("--stand-seconds is a whole number of seconds")?,
                );
            }
            Some("--awake-seconds") if awake_seconds.is_none() => {
                awake_seconds = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<1>)
                        .ok_or("--awake-seconds is a whole number of seconds")?[0],
                );
            }
            Some("--stand-roll") if stand_roll.is_none() => {
                stand_roll = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<1>)
                        .map(|value| value[0])
                        .filter(|counts| *counts <= 200)
                        .ok_or("--stand-roll is a whole number of counts, 200 at most")?,
                );
            }
            Some("--gaze-socket") if gaze_socket.is_none() => {
                gaze_socket = Some(PathBuf::from(value));
            }
            Some("--gaze") if gaze_turn.is_none() => {
                gaze_turn = Some(
                    value
                        .to_str()
                        .and_then(signed_pair)
                        .ok_or("--gaze is YAW,PITCH in signed encoder counts")?,
                );
            }
            Some("--gaze-reach") if gaze_reach.is_none() => {
                gaze_reach = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<2>)
                        .ok_or("--gaze-reach is YAW,PITCH in encoder counts")?,
                );
            }
            _ => return Err("unknown or duplicate option"),
        }
    }
    // Eyes are a socket and what the picture is worth, or nothing.
    let gaze = match (gaze_socket, gaze_turn) {
        (Some(socket), Some(turn)) => Some(GazeChoice {
            socket,
            turn,
            reach: gaze_reach.unwrap_or([250, 120]),
        }),
        (None, None) if gaze_reach.is_none() => None,
        _ => return Err("--gaze-socket and --gaze go together, and --gaze-reach needs them"),
    };
    // Standing is one choice made whole: a pose, the folded arm and a home.
    let standing = match (stand, folded_arm) {
        (Some(pose), Some(folded)) if home.is_some() => Some(StandingChoice {
            folded,
            pose,
            share: stand_share.unwrap_or(100),
            seconds: stand_seconds.unwrap_or(5),
            awake: awake_seconds.unwrap_or(90),
            load: arm_load.unwrap_or([950, 950]),
        }),
        (None, None)
            if arm_load.is_none()
                && stand_share.is_none()
                && stand_seconds.is_none()
                && awake_seconds.is_none() =>
        {
            None
        }
        _ => return Err("--stand needs --home and --folded-arm, and its options need --stand"),
    };
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
        standing,
        gaze,
        stand_roll,
    };
    if result.stand_roll.is_some() && result.standing.is_none() {
        return Err("--stand-roll needs --stand");
    }
    let gaze_path = result.gaze.as_ref().map(|gaze| &gaze.socket);
    for path in [&result.device, &result.lock_directory, &result.mood_socket]
        .into_iter()
        .chain(gaze_path)
    {
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

/// A gesture asked for by the voice service: `{"gesture":"nod"}` and nothing
/// else in it, naming a clip this body has.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn gesture_of(datagram: &[u8]) -> Option<&'static lamp_motor::clips::Clip> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Message<'a> {
        gesture: &'a str,
    }
    let message: Message = serde_json::from_slice(datagram).ok()?;
    lamp_motor::clips::named(message.gesture)
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
        arguments.standing,
        arguments.gaze,
        arguments.stand_roll,
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
    // A gesture asked for on the same socket, until the body takes it.
    let gesture_asked = std::cell::Cell::new(None);
    let news = || {
        // Everything waiting, and only the newest of it counts.
        let mut latest = None;
        let mut datagram = [0u8; 128];
        for _ in 0..32 {
            match moods.recv(&mut datagram) {
                Ok(length) => match gesture_of(&datagram[..length]) {
                    Some(clip) => gesture_asked.set(Some(clip)),
                    None => latest = mood_of(&datagram[..length]).or(latest),
                },
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
    // The head's tilt, for a lamp that has stood up.
    let limits = Limits {
        stand_roll: arguments.stand_roll.map_or(folded.stand_roll, f32::from),
        ..limits
    };
    // One line to the journal on each change of mood, and twice a second
    // while the goals are changing: what it did, afterwards.
    let mut said: Option<(lamp_motor::gesture::Mood, u64, [u16; 3], [u16; 2])> = None;
    let asked_to_stand = arguments.standing.is_some();
    let mut told_whether_it_stands = false;
    let mut face_said = 0u64;
    let mut gesture_said: Option<&'static str> = None;
    let mut eased = [0u16; 3];
    let watch = |glimpse: &lamp_motor::body::Glimpse| {
        let due = match said {
            None => true,
            Some((mood, _, _, _)) if mood != glimpse.mood => true,
            // While it stands, a line every ten seconds whatever happens:
            // how warm the two joints that carry it are getting.
            Some((_, step, _, _)) if glimpse.stand > 0.0 && glimpse.step - step >= 500 => true,
            Some((_, step, goals, arm)) => {
                (goals != glimpse.goals || arm != glimpse.arm_goals) && glimpse.step - step >= 25
            }
        };
        if glimpse.eased != eased {
            eased = glimpse.eased;
            eprintln!(
                "lamp-body: step {} met resistance and stopped pushing; so far {:?}; positions {:?} loads {:?}",
                glimpse.step, glimpse.eased, glimpse.positions, glimpse.loads
            );
        }
        if glimpse.gesture != gesture_said {
            gesture_said = glimpse.gesture;
            match glimpse.gesture {
                Some(name) => eprintln!("lamp-body: step {} gesture {name}", glimpse.step),
                None => eprintln!("lamp-body: step {} gesture over", glimpse.step),
            }
        }
        // What the eyes report and where that has it look, at most twice a
        // second: for setting the two numbers of --gaze on a lamp.
        if let Some(face) = glimpse.face
            && glimpse.step - face_said >= 25
        {
            face_said = glimpse.step;
            eprintln!(
                "lamp-body: step {} {:?} face at {:.2} across {:.2} down, size {:.2}; looking {:?}",
                glimpse.step, glimpse.mood, face[0], face[1], face[2], glimpse.look
            );
        }
        if due {
            said = Some((glimpse.mood, glimpse.step, glimpse.goals, glimpse.arm_goals));
            if asked_to_stand && !told_whether_it_stands {
                told_whether_it_stands = true;
                eprintln!(
                    "lamp-body: {}",
                    if glimpse.stands {
                        "it may stand"
                    } else {
                        "it will not stand this run; the report at its end says why"
                    }
                );
            }
            if glimpse.stands {
                eprintln!(
                    "lamp-body: step {} {:?} goals {:?} positions {:?} loads {:?} stand {:.2} arm goals {:?} positions {:?} loads {:?} warmth {:?}",
                    glimpse.step,
                    glimpse.mood,
                    glimpse.goals,
                    glimpse.positions,
                    glimpse.loads,
                    glimpse.stand,
                    glimpse.arm_goals,
                    glimpse.arm_positions,
                    glimpse.arm_loads,
                    glimpse.arm_temperatures
                );
            } else {
                eprintln!(
                    "lamp-body: step {} {:?} goals {:?} positions {:?} loads {:?}",
                    glimpse.step, glimpse.mood, glimpse.goals, glimpse.positions, glimpse.loads
                );
            }
        }
    };
    // The eyes' socket, for a body that looks. A file left by an earlier
    // run is not a listener: replace it.
    let (looking, sights) = match &arguments.gaze {
        Some(choice) => {
            match std::fs::remove_file(&choice.socket) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            let sights = UnixDatagram::bind(&choice.socket)?;
            sights.set_nonblocking(true)?;
            (
                Some(lamp_motor::gaze::Looking::new(choice.turn, choice.reach)?),
                Some(sights),
            )
        }
        None => (None, None),
    };
    let mut eyes = || {
        // Everything waiting, and only the newest of it counts.
        let sights = sights.as_ref()?;
        let mut latest = None;
        let mut datagram = [0u8; 160];
        for _ in 0..32 {
            match sights.recv(&mut datagram) {
                Ok(length) => {
                    latest = serde_json::from_slice::<lamp_motor::gaze::Seen>(&datagram[..length])
                        .ok()
                        .or(latest);
                }
                Err(_) => break,
            }
        }
        latest
    };
    let standing = arguments
        .standing
        .map(|choice| {
            lamp_motor::body::Standing::new(
                choice.folded,
                choice.pose,
                choice.share,
                choice.seconds,
                choice.awake,
                choice.load,
            )
        })
        .transpose()?;
    let result = run_body(
        &config,
        limits,
        arguments.home,
        standing,
        looking,
        news,
        || lamp_motor::body::Asked {
            seen: eyes(),
            gesture: gesture_asked.take(),
        },
        watch,
        || {
            if !cancelled {
                cancelled = signal_fd
                    .read_signal()
                    .map_err(|error| std::io::Error::from_raw_os_error(error as i32))?
                    .is_some();
            }
            Ok(cancelled)
        },
    );
    let _ = std::fs::remove_file(&arguments.mood_socket);
    if let Some(choice) = &arguments.gaze {
        let _ = std::fs::remove_file(&choice.socket);
    }
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
    fn standing_is_a_pose_a_folded_arm_and_a_home_or_nothing() {
        let with = |more: &[&'static str]| parse(args(&[&FULL[..], more].concat()));
        let stood = with(&[
            "--home",
            "1950,2043,2015",
            "--stand",
            "1772,2045,2015,2657,2602",
            "--folded-arm",
            "2048,2047",
            "--stand-share",
            "30",
            "--stand-seconds",
            "6",
            "--awake-seconds",
            "45",
        ])
        .unwrap()
        .unwrap()
        .standing
        .unwrap();
        assert_eq!(
            stood,
            StandingChoice {
                folded: [2_048, 2_047],
                pose: [1_772, 2_045, 2_015, 2_657, 2_602],
                share: 30,
                seconds: 6,
                awake: 45,
                load: [950, 950],
            }
        );
        assert!(with(&[]).unwrap().unwrap().standing.is_none());
        let home = ["--home", "1950,2043,2015"];
        let pose = ["--stand", "1772,2045,2015,2657,2602"];
        let arm = ["--folded-arm", "2048,2047"];
        for refused in [
            // No home, no folded arm, an option without a pose, a pose of four.
            [&pose[..], &arm].concat(),
            [&home[..], &pose].concat(),
            [&home[..], &["--stand-share", "30"]].concat(),
            [&home[..], &["--stand", "1772,2045,2015,2657"], &arm].concat(),
            [&home[..], &pose, &arm, &["--stand-share", "300"]].concat(),
        ] {
            assert!(with(&refused).is_err(), "{refused:?}");
        }
    }

    #[test]
    fn eyes_are_a_socket_and_what_the_picture_is_worth_or_nothing() {
        let with = |more: &[&'static str]| parse(args(&[&FULL[..], more].concat()));
        let socket = ["--gaze-socket", "/run/lampos-body/gaze.sock"];
        let looked = with(&[&socket[..], &["--gaze", "-342,200"]].concat())
            .unwrap()
            .unwrap()
            .gaze
            .unwrap();
        assert_eq!(
            looked,
            GazeChoice {
                socket: PathBuf::from("/run/lampos-body/gaze.sock"),
                turn: [-342, 200],
                reach: [250, 120],
            }
        );
        let reached = with(
            &[
                &socket[..],
                &["--gaze", "342,-200", "--gaze-reach", "300,100"],
            ]
            .concat(),
        )
        .unwrap()
        .unwrap()
        .gaze
        .unwrap();
        assert_eq!((reached.turn, reached.reach), ([342, -200], [300, 100]));
        assert!(with(&[]).unwrap().unwrap().gaze.is_none());
        for refused in [
            &socket[..],
            &["--gaze", "342,200"],
            &["--gaze-reach", "300,100"],
            &["--gaze-socket", "gaze.sock", "--gaze", "342,200"],
            &[
                "--gaze-socket",
                "/run/lampos-body/gaze.sock",
                "--gaze",
                "342",
            ],
            &[
                "--gaze-socket",
                "/run/lampos-body/gaze.sock",
                "--gaze",
                "+342,200",
            ],
        ] {
            assert!(with(refused).is_err(), "{refused:?}");
        }
    }

    #[test]
    fn a_gesture_is_asked_for_by_the_name_of_a_clip_and_nothing_else() {
        assert_eq!(
            gesture_of(br#"{"gesture":"nod"}"#).map(|clip| clip.name),
            Some("nod")
        );
        assert_eq!(
            gesture_of(br#"{"gesture":"headshake"}"#).map(|clip| clip.name),
            Some("headshake")
        );
        for ignored in [
            &br#"{"gesture":"dance"}"#[..],
            br#"{"gesture":"nod","scale":3}"#,
            br#"{"gesture":7}"#,
            br#"{"mood":"listening","level":3}"#,
            b"nod",
            b"",
        ] {
            assert!(gesture_of(ignored).is_none());
        }
        // And a gesture is not a mood.
        assert_eq!(mood_of(br#"{"gesture":"nod"}"#), None);
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
