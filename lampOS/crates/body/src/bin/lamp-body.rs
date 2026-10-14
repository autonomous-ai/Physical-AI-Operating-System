//! The lamp's small expressive body: one process that owns the servo port
//! and moves the head a few degrees for what the lamp is doing.

use lamp_body::calibration::UnitId;
use std::{ffi::OsString, path::PathBuf};

const USAGE: &str = "lamp-body --device ABSOLUTE_PATH --unit-id EXPLICIT_ID --lock-directory PRIVATE_ABSOLUTE_DIRECTORY --mood-socket ABSOLUTE_PATH [--home PITCH,ROLL,YAW] [--reach UP,DOWN,ROLL,YAW] [--speed COUNTS_PER_SECOND] [--servo-speed N] [--servo-acceleration N] [--stand PITCH,ROLL,YAW,SHOULDER,ELBOW --folded-arm SHOULDER,ELBOW [--stand-share PERCENT] [--stand-seconds N] [--awake-seconds N] [--arm-load SHOULDER,ELBOW] [--idle on|off]] [--gaze-socket ABSOLUTE_PATH --gaze YAW,PITCH [--gaze-reach YAW,PITCH]] [--stand-roll COUNTS] [--force-limit N] [--torque on|off] [--wake on|off] [--give-way return|stay] [--overload release] [--rest-glance on|off] [--pitch-window LOW,HIGH] [--look-at-name on|off]\n\nOwns the servo port and moves head pitch, head roll and base yaw a few degrees around\na home pose (the one given, in encoder counts, or else where they are found), for the mood\nit is told on the socket. Without --stand it never touches the shoulder or the elbow, and\nwithout --stand or --torque on it never touches torque. With --stand (which needs --home and --folded-arm) it also raises the shoulder and\nthe elbow to that pose when a mood other than rest arrives, folds them down again\n--awake-seconds after the last one, and writes torque once to a shoulder or elbow found\nwith an old overload latched. With --gaze-socket and --gaze it takes where a face is from\nlamp-eyes on that socket ({\"face\":{...}} or {\"face\":null}) and, while the lamp listens\nor speaks, turns the head to it in small corrections: --gaze is the counts of base yaw and of\nhead pitch, signed, that bring a face from the right and from the bottom edge of the picture\nto its middle. Refuses to start unless the three joints have torque on, a clear\nstatus and the head is inside its start window. With --torque on it first takes hold of a\njoint found without torque (the three always, the shoulder and the elbow with --stand):\nits goal is written as where it is, then torque on, and if it then moves more than 12 counts\nit does not start. With --wake on and --home, a joint found outside where it may start\n(the head out of its start window, or a joint farther from home than it may be found) but within\n400 counts of home is not refused: before anything else it is brought home slowly, a small stir,\na pause, then an eased rise at 20 counts a second at most, never away from home; an arm found off\nits way is not lowered, and the lamp runs folded. With --give-way, a joint that pushes at its goal without moving for\n1.5 s is told to stop (its goal becomes where it is; torque is never written off); with\n--give-way stay, a head or base joint that gave way and is moved more than 15 counts by\na hand for over a second stays where it was put. With --overload release, any joint that\nreads a load of 800 of 1000 (80% of --force-limit for a joint held to one) for 1.5 s has\ntorque switched off on all five, so the lamp slumps; 120 s later each is held where it is\n(its goal, then torque on) and the head is brought home as --wake does, while the arm\nwaits where it slumped until the lamp is next asked to stand; a second time within ten\nminutes ends the run with torque off. With --rest-glance on, a lamp at rest takes what\nExpression asks of the head (50 counts of pitch, 25 of roll at most, never the base or the arm)\nand comes back to rest when it runs out. --pitch-window is this lamp's own window for the\nhead's pitch while it is beside the base (by default lamp-4ace's, 1740,2030): never under\nthe head's floor (1670), around --home's pitch, and inside the servo's own limits or it does\nnot start. With --look-at-name on, the lamp's name heard on the bus\n(name/heard) while it does not speak is taken as being spoken to for two seconds: it rises,\nlooks up and turns to a face at once; with nobody in view it glances a little to one side and\nthe other once it stands, and lets it go. At most once in ten seconds. Stops at the first surprise, holding\nwhere it is. Ends on SIGINT or SIGTERM after easing back to where it started.\nThe mood socket takes datagrams of JSON: {\"mood\":\"resting|listening|thinking|speaking\",\"level\":0..255}.\nA lamp standing at its full height also makes a gesture asked for there: {\"gesture\":\"nod\"}\n(nod, headshake, greeting, laugh, shy, stretching).\nPrints one JSON report when it ends.\n";

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
    force_limit: Option<u16>,
    torque_on: bool,
    wake: bool,
    give_way: Option<GiveWay>,
    overload_release: bool,
    rest_glance: bool,
    pitch_window: Option<(u16, u16)>,
    look_at_name: bool,
}

/// What `--give-way` said; the body's own type is built for Linux only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GiveWay {
    Return,
    Stay,
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
    idle: bool,
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
    let mut idle = None;
    let (mut gaze_socket, mut gaze_turn, mut gaze_reach) = (None, None, None);
    let mut stand_roll = None;
    let mut force_limit = None;
    let mut torque = None;
    let mut wake = None;
    let mut give_way = None;
    let mut overload = None;
    let mut rest_glance = None;
    let mut pitch_window = None;
    let mut look_at_name = None;
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
                || idle.is_some()
                || gaze_socket.is_some()
                || gaze_turn.is_some()
                || gaze_reach.is_some()
                || stand_roll.is_some()
                || force_limit.is_some()
                || torque.is_some()
                || wake.is_some()
                || give_way.is_some()
                || overload.is_some()
                || rest_glance.is_some()
                || pitch_window.is_some()
                || look_at_name.is_some()
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
            Some("--idle") if idle.is_none() => {
                idle = Some(match value.to_str() {
                    Some("on") => true,
                    Some("off") => false,
                    _ => return Err("--idle is on or off"),
                });
            }
            Some("--force-limit") if force_limit.is_none() => {
                force_limit = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<1>)
                        .map(|value| value[0])
                        .filter(|force| (300..=1_000).contains(force))
                        .ok_or("--force-limit is a whole number from 300 to 1000")?,
                );
            }
            Some("--torque") if torque.is_none() => {
                torque = Some(match value.to_str() {
                    Some("on") => true,
                    Some("off") => false,
                    _ => return Err("--torque is on or off"),
                });
            }
            Some("--wake") if wake.is_none() => {
                wake = Some(match value.to_str() {
                    Some("on") => true,
                    Some("off") => false,
                    _ => return Err("--wake is on or off"),
                });
            }
            Some("--rest-glance") if rest_glance.is_none() => {
                rest_glance = Some(match value.to_str() {
                    Some("on") => true,
                    Some("off") => false,
                    _ => return Err("--rest-glance is on or off"),
                });
            }
            Some("--pitch-window") if pitch_window.is_none() => {
                // The head's floor, `Limits::FOLDED.pitch_floor` (built for
                // Linux only).
                let floor = 1_670;
                pitch_window = Some(
                    value
                        .to_str()
                        .and_then(counts_of::<2>)
                        .filter(|[low, high]| floor <= *low && low < high && *high <= 4_095)
                        .map(|[low, high]| (low, high))
                        .ok_or(
                            "--pitch-window is LOW,HIGH in counts, LOW under HIGH, 1670 or over",
                        )?,
                );
            }
            Some("--give-way") if give_way.is_none() => {
                give_way = Some(match value.to_str() {
                    Some("return") => GiveWay::Return,
                    Some("stay") => GiveWay::Stay,
                    _ => return Err("--give-way is return or stay"),
                });
            }
            Some("--overload") if overload.is_none() => {
                overload = Some(match value.to_str() {
                    Some("release") => true,
                    _ => return Err("--overload is release"),
                });
            }
            Some("--look-at-name") if look_at_name.is_none() => {
                look_at_name = Some(match value.to_str() {
                    Some("on") => true,
                    Some("off") => false,
                    _ => return Err("--look-at-name is on or off"),
                });
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
    // A load bound at or over the force limit could never be read, and the
    // guard it is would be off.
    if let (Some(force), Some(load)) = (force_limit, arm_load)
        && load.iter().any(|bound| *bound >= force)
    {
        return Err("--arm-load must be under --force-limit");
    }
    // Standing is one choice made whole: a pose, the folded arm and a home.
    let standing = match (stand, folded_arm) {
        (Some(pose), Some(folded)) if home.is_some() => Some(StandingChoice {
            folded,
            pose,
            share: stand_share.unwrap_or(100),
            seconds: stand_seconds.unwrap_or(5),
            awake: awake_seconds.unwrap_or(90),
            // A joint held to a force limit never reads a load above
            // it: the bound that sends the arm back down sits under the
            // limit, or the limit would switch that guard off.
            load: arm_load
                .unwrap_or_else(|| force_limit.map_or([950, 950], |force| [force - 40; 2])),
            idle: idle.unwrap_or(true),
        }),
        (None, None)
            if arm_load.is_none()
                && stand_share.is_none()
                && stand_seconds.is_none()
                && awake_seconds.is_none()
                && idle.is_none() =>
        {
            None
        }
        _ => return Err("--stand needs --home and --folded-arm, and its options need --stand"),
    };
    // A window the home's pitch is outside would push the head off home.
    if let (Some((low, high)), Some(home)) = (pitch_window, home)
        && !(low..=high).contains(&home[0])
    {
        return Err("--pitch-window must contain --home's pitch");
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
        standing,
        gaze,
        stand_roll,
        force_limit,
        torque_on: torque.unwrap_or(false),
        wake: wake.unwrap_or(true),
        give_way,
        overload_release: overload.unwrap_or(false),
        rest_glance: rest_glance.unwrap_or(true),
        pitch_window,
        look_at_name: look_at_name.unwrap_or(true),
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
fn mood_of(datagram: &[u8]) -> Option<(lamp_body::gesture::Mood, u8)> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Message {
        mood: lamp_body::gesture::Mood,
        level: u8,
    }
    let message: Message = serde_json::from_slice(datagram).ok()?;
    Some((message.mood, message.level))
}

/// A gesture asked for by the voice service: `{"gesture":"nod"}` and nothing
/// else in it, naming a clip this body has.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn gesture_of(datagram: &[u8]) -> Option<&'static lamp_body::clips::Clip> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Message<'a> {
        gesture: &'a str,
    }
    let message: Message = serde_json::from_slice(datagram).ok()?;
    lamp_body::clips::named(message.gesture)
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

/// What each step tells systemd: ready at the first, which comes only once
/// the start checks have passed, and alive at every one (sent at most twice
/// in a watchdog interval, and never without `$NOTIFY_SOCKET`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn step_said(notify: &lamp_keeper::Notify, ready: &std::cell::Cell<bool>) {
    if !ready.replace(true) {
        notify.ready();
    }
    notify.alive();
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
        arguments.force_limit,
        arguments.torque_on,
        arguments.wake,
        arguments.give_way,
        arguments.overload_release,
        arguments.rest_glance,
        arguments.pitch_window,
        arguments.look_at_name,
    );
    Err("the body requires Linux; no device was opened".into())
}

#[cfg(target_os = "linux")]
fn run(arguments: Arguments) -> Result<bool, Box<dyn std::error::Error>> {
    use lamp_body::{
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
        force: arguments.force_limit,
        torque_on: arguments.torque_on,
        wake: arguments.wake,
        give_way: arguments.give_way.map(|choice| match choice {
            GiveWay::Return => lamp_body::body::GiveWay::Return,
            GiveWay::Stay => lamp_body::body::GiveWay::Stay,
        }),
        overload_release: arguments.overload_release,
        rest_glance: arguments.rest_glance,
        pitch_window: arguments.pitch_window.unwrap_or(folded.pitch_window),
        look_at_name: arguments.look_at_name,
        ..limits
    };
    // One line to the journal on each change of mood, and twice a second
    // while the goals are changing: what it did, afterwards.
    let mut said: Option<(lamp_body::gesture::Mood, u64, [u16; 3], [u16; 2])> = None;
    let asked_to_stand = arguments.standing.is_some();
    let mut told_whether_it_stands = false;
    let mut face_said = 0u64;
    let mut gesture_said: Option<&'static str> = None;
    let mut eased = [0u16; 3];
    let mut handled = ([0u16; 3], [0u16; 5]);
    let mut limp = false;
    // Where the joints are and how the body is, for the rest of the lamp,
    // if the unit says so. Offered and never waited for.
    let mut bus = lamp_body::bus_out::BusOut::from_environment(|why| eprintln!("lamp-body: {why}"));
    let notify = lamp_keeper::Notify::from_environment();
    let ready = std::cell::Cell::new(false);
    let watch = |glimpse: &lamp_body::body::Glimpse| {
        step_said(&notify, &ready);
        if let Some(bus) = bus.as_mut() {
            bus.offer(glimpse);
        }
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
        if glimpse.limp != limp {
            limp = glimpse.limp;
            eprintln!(
                "lamp-body: step {} {}; positions {:?} loads {:?} arm positions {:?} loads {:?} warmth {:?} {:?}",
                glimpse.step,
                if limp {
                    format!(
                        "let go under overload (time {}): torque off on all five for 120 s, or for good if it is the second within ten minutes",
                        glimpse.released
                    )
                } else {
                    "took hold again after letting go, and brings the head home slowly".to_owned()
                },
                glimpse.positions,
                glimpse.loads,
                glimpse.arm_positions,
                glimpse.arm_loads,
                glimpse.temperatures,
                glimpse.arm_temperatures
            );
        }
        if glimpse.eased != eased {
            eased = glimpse.eased;
            eprintln!(
                "lamp-body: step {} met resistance and stopped pushing; so far {:?}; positions {:?} loads {:?}",
                glimpse.step, glimpse.eased, glimpse.positions, glimpse.loads
            );
        }
        if (glimpse.kept_where_put, glimpse.stalls) != handled {
            handled = (glimpse.kept_where_put, glimpse.stalls);
            eprintln!(
                "lamp-body: step {} left where a hand put it {:?}; stopped pushing without moving {:?}; positions {:?} loads {:?} arm positions {:?} loads {:?}",
                glimpse.step,
                glimpse.kept_where_put,
                glimpse.stalls,
                glimpse.positions,
                glimpse.loads,
                glimpse.arm_positions,
                glimpse.arm_loads
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
                Some(lamp_body::gaze::Looking::new(choice.turn, choice.reach)?),
                Some(sights),
            )
        }
        None => (None, None),
    };
    let eyes = || {
        // Everything waiting, and only the newest of it counts.
        let sights = sights.as_ref()?;
        let mut latest = None;
        let mut datagram = [0u8; 160];
        for _ in 0..32 {
            match sights.recv(&mut datagram) {
                Ok(length) => {
                    latest = serde_json::from_slice::<lamp_body::gaze::Seen>(&datagram[..length])
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
            lamp_body::body::Standing::new(
                choice.folded,
                choice.pose,
                choice.share,
                choice.seconds,
                choice.awake,
                choice.load,
            )
            .map(|standing| standing.idling(choice.idle))
        })
        .transpose()?;
    // What Expression asks of the head, if the unit says so. Looked at
    // each step and never waited for (docs/expression.md).
    let mut expression = lamp_body::expression_in::ExpressionIn::from_environment(|why| {
        eprintln!("lamp-body: {why}")
    });
    // The lamp's name, heard, if it is to turn to a call.
    let mut name = if arguments.look_at_name {
        match lamp_bus::Bus::from_environment().and_then(|bus| lamp_body::name_in::NameIn::on(&bus))
        {
            Ok(name) => {
                eprintln!("lamp-body: it turns to a call, from name/heard");
                Some(name)
            }
            Err(error) => {
                eprintln!("lamp-body: no call is taken from name/heard: {error}");
                None
            }
        }
    } else {
        None
    };
    let result = run_body(
        &config,
        limits,
        arguments.home,
        standing,
        looking,
        news,
        || lamp_body::body::Asked {
            seen: eyes(),
            gesture: gesture_asked.take(),
            expression: expression.as_mut().and_then(|asked| {
                asked.offsets(lamp_ipc::monotonic_us(), |why| {
                    eprintln!("lamp-body: {why}")
                })
            }),
            called: name.as_mut().is_some_and(|name| name.called()),
        },
        watch,
        || {
            if !cancelled {
                cancelled = signal_fd
                    .read_signal()
                    .map_err(|error| std::io::Error::from_raw_os_error(error as i32))?
                    .is_some();
                if cancelled {
                    // It eases back from here.
                    notify.stopping();
                }
            }
            Ok(cancelled)
        },
    );
    // Again for a body that stopped by itself; a second time is harmless.
    notify.stopping();
    if let Some(woke) = result.body.as_ref().and_then(|body| body.woke.as_ref()) {
        eprintln!(
            "lamp-body: woke {:?} from {:?} to {:?} in {:.1} s",
            woke.joints, woke.from, woke.to, woke.seconds
        );
    }
    for released in result.body.iter().flat_map(|body| &body.released) {
        eprintln!(
            "lamp-body: let go under overload: {} read {} at {:.1} s",
            released.joint, released.load, released.seconds
        );
    }
    let _ = std::fs::remove_file(&arguments.mood_socket);
    if let Some(choice) = &arguments.gaze {
        let _ = std::fs::remove_file(&choice.socket);
    }
    // The body's last word on the bus: that it has stopped, and why.
    if let Some(bus) = bus {
        if bus.let_go > 0 {
            eprintln!(
                "lamp-body: {} steps were not published: the bus thread had no room",
                bus.let_go
            );
        }
        match &result.body {
            Some(report) => bus.stopped(&report.stop),
            None => bus.stopped(&Stop::Refused {
                why: "the servo port could not be made ready".into(),
            }),
        }
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
    use lamp_body::gesture::Mood;

    #[test]
    fn each_step_says_alive_and_the_first_says_ready() {
        use std::os::unix::net::UnixDatagram;
        let directory =
            std::env::temp_dir().join(format!("lamp-body-notify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("notify");
        let ear = UnixDatagram::bind(&path).unwrap();
        ear.set_nonblocking(true).unwrap();
        let notify = lamp_keeper::Notify::new(
            Some(path.to_str().unwrap().to_owned()),
            Some(std::time::Duration::from_secs(10)),
        );
        let ready = std::cell::Cell::new(false);
        for _ in 0..3 {
            step_said(&notify, &ready);
        }
        let mut heard = Vec::new();
        let mut buffer = [0u8; 64];
        while let Ok(length) = ear.recv(&mut buffer) {
            heard.push(String::from_utf8_lossy(&buffer[..length]).into_owned());
        }
        // Three steps within one watchdog interval: alive is sent once.
        assert_eq!(heard, ["READY=1", "WATCHDOG=1"]);
        std::fs::remove_dir_all(directory).unwrap();
    }

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
                // On unless turned off (docs/joining.md).
                idle: true,
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

    /// The force limit is a number from 300 to 1,000, off unless given;
    /// and with it the arm's load bound comes down under it.
    #[test]
    fn a_force_limit_is_a_number_in_its_range_and_brings_the_load_bound_under_it() {
        let with = |more: &[&'static str]| parse(args(&[&FULL[..], more].concat()));
        assert_eq!(with(&[]).unwrap().unwrap().force_limit, None);
        assert_eq!(
            with(&["--force-limit", "600"])
                .unwrap()
                .unwrap()
                .force_limit,
            Some(600)
        );
        for refused in [
            &["--force-limit", "299"][..],
            &["--force-limit", "1001"],
            &["--force-limit", "+600"],
            &["--force-limit", "six"],
            &["--force-limit", "600", "--force-limit", "700"],
        ] {
            assert!(with(refused).is_err(), "{refused:?}");
        }
        let stand = [
            "--home",
            "1950,2043,2015",
            "--stand",
            "1772,2045,2015,2657,2602",
            "--folded-arm",
            "2048,2047",
        ];
        let standing = |more: &[&'static str]| {
            with(&[&stand[..], more].concat()).map(|value| value.unwrap().standing.unwrap().load)
        };
        assert_eq!(standing(&[]), Ok([950, 950]));
        assert_eq!(standing(&["--force-limit", "600"]), Ok([560, 560]));
        // A bound that was asked for is kept, if it is under the limit.
        assert_eq!(
            standing(&["--force-limit", "600", "--arm-load", "500,520"]),
            Ok([500, 520])
        );
        assert!(standing(&["--force-limit", "600", "--arm-load", "500,600"]).is_err());
        assert!(standing(&["--arm-load", "900,950", "--force-limit", "600"]).is_err());
    }

    /// Taking hold of a joint found without torque is off unless asked for.
    #[test]
    fn torque_is_on_or_off_and_off_unless_given() {
        let with = |more: &[&'static str]| parse(args(&[&FULL[..], more].concat()));
        assert!(!with(&[]).unwrap().unwrap().torque_on);
        assert!(with(&["--torque", "on"]).unwrap().unwrap().torque_on);
        assert!(!with(&["--torque", "off"]).unwrap().unwrap().torque_on);
        for refused in [
            &["--torque", "1"][..],
            &["--torque", "ON"],
            &["--torque", ""],
            &["--torque"],
            &["--torque", "on", "--torque", "on"],
            &["--torque", "off", "--torque", "on"],
        ] {
            assert!(with(refused).is_err(), "{refused:?}");
        }
    }

    /// Waking from wherever the lamp is found is off unless asked for.
    #[test]
    fn wake_is_on_or_off_and_on_unless_turned_off() {
        let with = |more: &[&'static str]| parse(args(&[&FULL[..], more].concat()));
        // On unless turned off: confirmed on lamp-4ace (docs/joining.md).
        assert!(with(&[]).unwrap().unwrap().wake);
        assert!(with(&["--wake", "on"]).unwrap().unwrap().wake);
        assert!(!with(&["--wake", "off"]).unwrap().unwrap().wake);
        for refused in [
            &["--wake", "1"][..],
            &["--wake", "ON"],
            &["--wake", ""],
            &["--wake"],
            &["--wake", "on", "--wake", "on"],
            &["--wake", "on", "--help"],
        ] {
            assert!(with(refused).is_err(), "{refused:?}");
        }
    }

    /// A glance from rest is off unless asked for.
    #[test]
    fn rest_glance_is_on_or_off_and_on_unless_turned_off() {
        let with = |more: &[&'static str]| parse(args(&[&FULL[..], more].concat()));
        assert!(with(&[]).unwrap().unwrap().rest_glance);
        assert!(with(&["--rest-glance", "on"]).unwrap().unwrap().rest_glance);
        assert!(
            !with(&["--rest-glance", "off"])
                .unwrap()
                .unwrap()
                .rest_glance
        );
        for refused in [
            &["--rest-glance", "1"][..],
            &["--rest-glance"],
            &["--rest-glance", "on", "--rest-glance", "on"],
            &["--rest-glance", "on", "--help"],
        ] {
            assert!(with(refused).is_err(), "{refused:?}");
        }
    }

    /// A lamp's own window for the head beside the base, lamp-4ace's
    /// unless given.
    #[test]
    fn pitch_window_is_low_and_high_and_lamp_4ace_s_unless_given() {
        let with = |more: &[&'static str]| parse(args(&[&FULL[..], more].concat()));
        assert_eq!(with(&[]).unwrap().unwrap().pitch_window, None);
        assert_eq!(
            with(&["--pitch-window", "1670,1995"])
                .unwrap()
                .unwrap()
                .pitch_window,
            Some((1_670, 1_995))
        );
        let home = ["--home", "1771,2035,2035"];
        assert!(with(&[&home[..], &["--pitch-window", "1700,1995"]].concat()).is_ok());
        for refused in [
            &["--pitch-window", "1669,1995"][..],
            &["--pitch-window", "1995,1700"],
            &["--pitch-window", "1700,1700"],
            &["--pitch-window", "1700,4096"],
            &["--pitch-window", "1700"],
            &["--pitch-window", "+1700,1995"],
            &["--pitch-window"],
            &["--pitch-window", "1700,1995", "--pitch-window", "1700,1995"],
            &["--pitch-window", "1700,1995", "--help"],
            &["--home", "1771,2035,2035", "--pitch-window", "1800,1995"],
            &["--home", "1771,2035,2035", "--pitch-window", "1700,1770"],
        ] {
            assert!(with(refused).is_err(), "{refused:?}");
        }
    }

    /// Giving way is return or stay, and as before unless given.
    #[test]
    fn give_way_is_return_or_stay_and_as_before_unless_given() {
        let with = |more: &[&'static str]| parse(args(&[&FULL[..], more].concat()));
        assert_eq!(with(&[]).unwrap().unwrap().give_way, None);
        assert_eq!(
            with(&["--give-way", "return"]).unwrap().unwrap().give_way,
            Some(GiveWay::Return)
        );
        assert_eq!(
            with(&["--give-way", "stay"]).unwrap().unwrap().give_way,
            Some(GiveWay::Stay)
        );
        for refused in [
            &["--give-way", "STAY"][..],
            &["--give-way", "on"],
            &["--give-way", ""],
            &["--give-way"],
            &["--give-way", "stay", "--give-way", "return"],
        ] {
            assert!(with(refused).is_err(), "{refused:?}");
        }
    }

    /// Letting go under overload is `release` or not given, and off unless given.
    #[test]
    fn overload_is_release_and_off_unless_given() {
        let with = |more: &[&'static str]| parse(args(&[&FULL[..], more].concat()));
        assert!(!with(&[]).unwrap().unwrap().overload_release);
        assert!(
            with(&["--overload", "release"])
                .unwrap()
                .unwrap()
                .overload_release
        );
        for refused in [
            &["--overload", "on"][..],
            &["--overload", "off"],
            &["--overload", "RELEASE"],
            &["--overload", ""],
            &["--overload"],
            &["--overload", "release", "--overload", "release"],
            &["--overload", "release", "--help"],
        ] {
            assert!(with(refused).is_err(), "{refused:?}");
        }
    }

    /// Turning to a call is on or off, and off unless given.
    #[test]
    fn look_at_name_is_on_or_off_and_on_unless_turned_off() {
        let with = |more: &[&'static str]| parse(args(&[&FULL[..], more].concat()));
        assert!(with(&[]).unwrap().unwrap().look_at_name);
        assert!(
            with(&["--look-at-name", "on"])
                .unwrap()
                .unwrap()
                .look_at_name
        );
        assert!(
            !with(&["--look-at-name", "off"])
                .unwrap()
                .unwrap()
                .look_at_name
        );
        for refused in [
            &["--look-at-name", "ON"][..],
            &["--look-at-name", ""],
            &["--look-at-name"],
            &["--look-at-name", "on", "--look-at-name", "on"],
            &["--look-at-name", "on", "--help"],
        ] {
            assert!(with(refused).is_err(), "{refused:?}");
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
