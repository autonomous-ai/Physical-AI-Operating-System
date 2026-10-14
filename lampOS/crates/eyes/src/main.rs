//! The lamp's eyes: one process that owns the camera, finds a face in what
//! it sees and tells the body where it is. It opens the camera only while
//! the voice service keeps saying it may, and sends nothing but the face's
//! place in the picture. With `--sight-file` it also keeps its newest frame,
//! that one only, in a private file for the voice service.

use std::{ffi::OsString, path::PathBuf};

const USAGE: &str = "lamp-eyes --device ABSOLUTE_PATH --lock-directory PRIVATE_ABSOLUTE_DIRECTORY --usb VENDOR:PRODUCT@TOPOLOGY --model ABSOLUTE_PATH --permit-socket ABSOLUTE_PATH --body-socket ABSOLUTE_PATH [--sight-file ABSOLUTE_PATH] [--presence-file ABSOLUTE_PATH]\n\nOwns the camera at --device (checked against the USB identity given, hex vendor and\nproduct and the port topology, for example 01da:5875@3-1.1.4) and looks for a face in\nits frames four times a second while there is one and less than twice a second when\nthere is none, with the pinned YuNet model at --model. Each look\nsends one datagram to --body-socket: {\"face\":{\"x\":..,\"y\":..,\"size\":..,\"turned\":..}} or\n{\"face\":null}. The camera is open only while {\"camera\":true} keeps arriving on\n--permit-socket, at least every 1.5 s; {\"camera\":false} closes it at once. Without\n--sight-file no frame is written anywhere and none leaves this process. With it the\nnewest frame, and only that one, is kept in that file (mode 0600) while the camera is\nopen and removed when it closes: it is what the voice service shows the model. With\n--presence-file one line, the near face's height as a share of the picture's, is kept\nfresh in that file while there is a face: no picture, only that number.\n";

#[derive(Clone, Debug, Eq, PartialEq)]
struct Usb {
    vendor: u16,
    product: u16,
    topology: String,
}

#[derive(Debug, Eq, PartialEq)]
struct Arguments {
    device: PathBuf,
    lock_directory: PathBuf,
    usb: Usb,
    model: PathBuf,
    permit_socket: PathBuf,
    body_socket: PathBuf,
    sight_file: Option<PathBuf>,
    presence_file: Option<PathBuf>,
}

/// `VENDOR:PRODUCT@TOPOLOGY`, the two numbers as four hex digits each.
fn usb_of(value: &str) -> Option<Usb> {
    let (numbers, topology) = value.split_once('@')?;
    let (vendor, product) = numbers.split_once(':')?;
    let hex = |text: &str| {
        (text.len() == 4 && text.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| u16::from_str_radix(text, 16).ok())
            .flatten()
            .filter(|number| *number != 0)
    };
    let plain = !topology.is_empty()
        && topology.len() <= 64
        && topology
            .bytes()
            .all(|byte| byte.is_ascii_digit() || b"-.".contains(&byte));
    plain.then_some(())?;
    Some(Usb {
        vendor: hex(vendor)?,
        product: hex(product)?,
        topology: topology.into(),
    })
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Arguments>, &'static str> {
    let mut args = args.into_iter();
    let (mut device, mut lock_directory, mut usb) = (None, None, None);
    let (mut model, mut permit_socket, mut body_socket) = (None, None, None);
    let (mut sight_file, mut presence_file) = (None, None);
    while let Some(key) = args.next() {
        if key == "--help" {
            let alone = device.is_none()
                && lock_directory.is_none()
                && usb.is_none()
                && model.is_none()
                && permit_socket.is_none()
                && body_socket.is_none()
                && sight_file.is_none()
                && presence_file.is_none()
                && args.next().is_none();
            return if alone {
                Ok(None)
            } else {
                Err("--help must be used alone")
            };
        }
        let value = args.next().ok_or("every option requires a value")?;
        match key.to_str() {
            Some("--device") if device.is_none() => device = Some(PathBuf::from(value)),
            Some("--lock-directory") if lock_directory.is_none() => {
                lock_directory = Some(PathBuf::from(value));
            }
            Some("--model") if model.is_none() => model = Some(PathBuf::from(value)),
            Some("--permit-socket") if permit_socket.is_none() => {
                permit_socket = Some(PathBuf::from(value));
            }
            Some("--body-socket") if body_socket.is_none() => {
                body_socket = Some(PathBuf::from(value));
            }
            Some("--sight-file") if sight_file.is_none() => {
                sight_file = Some(PathBuf::from(value));
            }
            Some("--presence-file") if presence_file.is_none() => {
                presence_file = Some(PathBuf::from(value));
            }
            Some("--usb") if usb.is_none() => {
                usb =
                    Some(value.to_str().and_then(usb_of).ok_or(
                        "--usb is VENDOR:PRODUCT@TOPOLOGY, for example 01da:5875@3-1.1.4",
                    )?);
            }
            _ => return Err("unknown or duplicate option"),
        }
    }
    let result = Arguments {
        device: device.ok_or("--device is required")?,
        lock_directory: lock_directory.ok_or("--lock-directory is required")?,
        usb: usb.ok_or("--usb is required")?,
        model: model.ok_or("--model is required")?,
        permit_socket: permit_socket.ok_or("--permit-socket is required")?,
        body_socket: body_socket.ok_or("--body-socket is required")?,
        sight_file,
        presence_file,
    };
    for path in [
        &result.device,
        &result.lock_directory,
        &result.model,
        &result.permit_socket,
        &result.body_socket,
    ]
    .into_iter()
    .chain(&result.sight_file)
    .chain(&result.presence_file)
    {
        if !path.is_absolute()
            || path.as_os_str().len() > 200
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

/// What the voice service says about the camera: `Some(true)` it may be
/// open, `Some(false)` it must close now. Anything else says nothing.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn permit_of(datagram: &[u8]) -> Option<bool> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Message {
        camera: bool,
    }
    serde_json::from_slice::<Message>(datagram)
        .ok()
        .map(|message| message.camera)
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
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lamp-eyes failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn run(arguments: Arguments) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _ = arguments;
    Err("the eyes require Linux; no camera was opened".into())
}

#[cfg(target_os = "linux")]
fn run(arguments: Arguments) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use lamp_camera::{
        CaptureConfig, CaptureId, FrameInterval, MAX_FRAME_BYTES, OperationBudget, PortIo,
        READ_BUDGET_US, START_BUDGET_US, STOP_BUDGET_US, SourceId,
        linux::{LinuxConfig, LinuxPort, UsbIdentity},
    };
    use lamp_eyes::{
        faces::Detector,
        kept::{Kept, Presence},
        picture::Picture,
        sight::{Seen, choose},
    };
    use lamp_turn_owner::{BootId, MonoTime};
    use std::{
        num::NonZeroU64,
        os::unix::net::UnixDatagram,
        time::{Duration, Instant},
    };

    /// A permission older than this is none.
    const PERMIT_STALE: Duration = Duration::from_millis(1_500);
    /// The newest frame is looked at this often at most while there is a
    /// face to follow, and for a few seconds after the last one.
    const LOOK_EVERY: Duration = Duration::from_millis(250);
    /// And this often with nobody there: often enough to notice someone
    /// sitting down within a second, at a quarter of the work. Looking six
    /// times a second all day kept lamp-4ace's processor 8 to 10 degrees
    /// warmer (63 to 64 degrees C with the eyes stopped, 70 to 75 with them),
    /// above the 70 at which its kernel begins to hold it back.
    const GLANCE_EVERY: Duration = Duration::from_millis(600);
    const FOLLOW_FOR: Duration = Duration::from_secs(4);
    /// A camera that did not open, or failed, is tried again after this.
    const RETRY: Duration = Duration::from_secs(5);
    const REPORT_EVERY: Duration = Duration::from_secs(10);

    let detector = Detector::load(&arguments.model)?;
    // A socket file left by an earlier run is not a listener: replace it.
    match std::fs::remove_file(&arguments.permit_socket) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let permits = UnixDatagram::bind(&arguments.permit_socket)?;
    permits.set_nonblocking(true)?;
    let out = UnixDatagram::unbound()?;
    out.set_nonblocking(true)?;
    let tell = |seen: Seen| {
        if let Ok(text) = serde_json::to_vec(&seen) {
            // A body that is absent or slow changes nothing here.
            let _ = out.send_to(&text, &arguments.body_socket);
        }
    };

    let source = SourceId::new("lamp-eyes").map_err(|error| format!("{error:?}"))?;
    let mut boot = [0u8; 16];
    boot[..4].copy_from_slice(&std::process::id().to_le_bytes());
    boot[4..12].copy_from_slice(&lamp_ipc::monotonic_us().to_le_bytes());
    boot[15] = 1;
    let worker = BootId::new(boot).map_err(|error| format!("{error:?}"))?;
    let budget = |micros: u64| {
        let now = MonoTime::from_micros(lamp_ipc::monotonic_us());
        OperationBudget {
            started_at: now,
            deadline: now.checked_add(micros).unwrap_or(now),
        }
    };

    let mut kept = arguments.sight_file.clone().map(Kept::new);
    let mut presence = arguments.presence_file.clone().map(Presence::new);
    // The faces in view, for whoever listens on the lamp's bus.
    let mut bus_out = lamp_eyes::bus_out::BusOut::from_environment();
    let mut epoch = 0u64;
    let mut port: Option<LinuxPort> = None;
    let mut permitted_at: Option<Instant> = None;
    let mut retry_at = Instant::now();
    let mut frame = vec![0u8; MAX_FRAME_BYTES];
    // Bytes of a frame that has not been looked at yet.
    let mut latest: Option<usize> = None;
    let mut looked_at = Instant::now();
    let mut face_at: Option<Instant> = None;
    let (mut frames, mut looks, mut with_face, mut unreadable) = (0u32, 0u32, 0u32, 0u32);
    let mut spent = Duration::ZERO;
    let mut reported_at = Instant::now();
    // How the camera is, for the lamp's health.
    let mut camera = lamp_eyes::bus_out::Camera::Closed;
    eprintln!("lamp-eyes: ready; the camera stays closed until it is permitted");
    loop {
        // On every turn, whatever the camera is doing: that the eyes are
        // there, and how they are.
        if let Some(bus) = bus_out.as_mut() {
            let now = lamp_ipc::monotonic_us();
            bus.tick(now);
            bus.report(camera, now);
        }
        let mut datagram = [0u8; 64];
        for _ in 0..32 {
            match permits.recv(&mut datagram) {
                Ok(length) => match permit_of(&datagram[..length]) {
                    Some(true) => permitted_at = Some(Instant::now()),
                    Some(false) => permitted_at = None,
                    None => {}
                },
                Err(_) => break,
            }
        }
        if permitted_at.is_none_or(|at| at.elapsed() >= PERMIT_STALE) {
            if let Some(mut open) = port.take() {
                let _ = open.stop(budget(STOP_BUDGET_US));
                eprintln!("lamp-eyes: camera closed: not permitted");
                tell(Seen { face: None });
            }
            camera = lamp_eyes::bus_out::Camera::Closed;
            if let Some(kept) = kept.as_mut() {
                kept.forget();
            }
            if let Some(presence) = presence.as_mut() {
                presence.gone();
            }
            latest = None;
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        if port.is_none() {
            if Instant::now() < retry_at {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            epoch += 1;
            let config = LinuxConfig::new(
                arguments.device.clone(),
                arguments.lock_directory.clone(),
                source,
                UsbIdentity {
                    vendor: arguments.usb.vendor,
                    product: arguments.usb.product,
                    topology: arguments.usb.topology.clone(),
                    serial: None,
                    interface_number: 0,
                    capture_index: 0,
                },
            )
            .map_err(|error| format!("camera configuration: {error:?}"))?;
            let mut opening = LinuxPort::new(config);
            let started = opening.start(
                CaptureConfig {
                    source,
                    width: 320,
                    height: 240,
                    interval: Some(
                        FrameInterval::new(1, 10).map_err(|error| format!("{error:?}"))?,
                    ),
                    max_frame_bytes: MAX_FRAME_BYTES,
                    buffers: 2,
                },
                CaptureId {
                    worker,
                    epoch: NonZeroU64::new(epoch).ok_or("no epoch")?,
                },
                budget(START_BUDGET_US),
            );
            match started {
                Ok(mode) => {
                    eprintln!("lamp-eyes: camera open, {} by {}", mode.width, mode.height);
                    port = Some(opening);
                    camera = lamp_eyes::bus_out::Camera::Open;
                    reported_at = Instant::now();
                }
                Err(error) => {
                    eprintln!(
                        "lamp-eyes: camera did not open: {error:?} {:?}",
                        opening.diagnostics().fault
                    );
                    camera = lamp_eyes::bus_out::Camera::NotOpening;
                    retry_at = Instant::now() + RETRY;
                    continue;
                }
            }
        }
        // Everything the camera has: only the newest frame is of interest.
        let mut failed = None;
        if let Some(open) = port.as_mut() {
            loop {
                match open.try_frame(&mut frame, budget(READ_BUDGET_US)) {
                    Ok(Some(got)) => {
                        frames += 1;
                        latest = Some(got.bytes_used.min(frame.len()));
                    }
                    Ok(None) => break,
                    Err(error) => {
                        failed = Some(error);
                        break;
                    }
                }
            }
        }
        if let Some(error) = failed {
            eprintln!("lamp-eyes: camera failed: {error:?}; closing it");
            camera = lamp_eyes::bus_out::Camera::Failed;
            if let Some(mut open) = port.take() {
                let _ = open.stop(budget(STOP_BUDGET_US));
            }
            tell(Seen { face: None });
            if let Some(kept) = kept.as_mut() {
                kept.forget();
            }
            if let Some(presence) = presence.as_mut() {
                presence.gone();
            }
            latest = None;
            retry_at = Instant::now() + RETRY;
            continue;
        }
        let every = if face_at.is_some_and(|at| at.elapsed() < FOLLOW_FOR) {
            LOOK_EVERY
        } else {
            GLANCE_EVERY
        };
        if let Some(used) = latest
            && looked_at.elapsed() >= every
        {
            latest = None;
            looked_at = Instant::now();
            // What the lamp sees now, for the voice service; a frame that is
            // not whole leaves the one before it.
            if let Some(kept) = kept.as_mut() {
                let _ = kept.keep(&frame[..used]);
            }
            let found = Picture::from_jpeg(&frame[..used]).and_then(|picture| {
                detector
                    .find(&picture)
                    .map(|faces| {
                        if let Some(bus) = bus_out.as_mut() {
                            bus.saw(&faces, picture.filled, lamp_ipc::monotonic_us());
                        }
                        choose(&faces, picture.filled)
                    })
                    .map_err(|error| error.to_string())
            });
            spent += looked_at.elapsed();
            match found {
                Ok(face) => {
                    looks += 1;
                    with_face += u32::from(face.is_some());
                    if let Some(face) = face.as_ref() {
                        face_at = Some(Instant::now());
                        if let Some(presence) = presence.as_mut() {
                            let _ = presence.there(face.size);
                        }
                    }
                    tell(Seen { face });
                }
                Err(_) => unreadable += 1,
            }
        }
        if reported_at.elapsed() >= REPORT_EVERY {
            eprintln!(
                "lamp-eyes: {frames} frames, looked at {looks}, a face in {with_face}, {unreadable} unreadable, {:.0} ms a look",
                spent.as_secs_f32() * 1e3 / (looks + unreadable).max(1) as f32
            );
            (frames, looks, with_face, unreadable) = (0, 0, 0, 0);
            spent = Duration::ZERO;
            reported_at = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    const FULL: [&str; 12] = [
        "--device",
        "/dev/video0",
        "--lock-directory",
        "/home/test/private-camera-locks",
        "--usb",
        "01da:5875@3-1.1.4",
        "--model",
        "/opt/lampOS/eyes/face_detection_yunet_2023mar.onnx",
        "--permit-socket",
        "/run/lampos-eyes/permit.sock",
        "--body-socket",
        "/run/lampos-body/gaze.sock",
    ];

    #[test]
    fn every_path_and_the_camera_are_named_and_nothing_else_is_accepted() {
        let value = parse(args(&FULL)).unwrap().unwrap();
        assert_eq!(
            value.usb,
            Usb {
                vendor: 0x01da,
                product: 0x5875,
                topology: "3-1.1.4".into()
            }
        );
        assert_eq!(
            value.body_socket,
            PathBuf::from("/run/lampos-body/gaze.sock")
        );
        assert!(parse(args(&["--help"])).unwrap().is_none());
        // The newest frame is kept for the voice service only when asked.
        assert_eq!(value.sight_file, None);
        let seeing = [&FULL[..], &["--sight-file", "/run/lampos-eyes/sight.jpg"]].concat();
        assert_eq!(
            parse(args(&seeing)).unwrap().unwrap().sight_file,
            Some(PathBuf::from("/run/lampos-eyes/sight.jpg"))
        );
        assert!(parse(args(&[&FULL[..], &["--sight-file", "sight.jpg"]].concat())).is_err());
        assert!(
            parse(args(
                &[&seeing[..], &["--sight-file", "/run/x.jpg"]].concat()
            ))
            .is_err()
        );
        // And the one line that says someone is there.
        assert_eq!(value.presence_file, None);
        let present = [&FULL[..], &["--presence-file", "/run/lampos-eyes/present"]].concat();
        assert_eq!(
            parse(args(&present)).unwrap().unwrap().presence_file,
            Some(PathBuf::from("/run/lampos-eyes/present"))
        );
        assert!(parse(args(&[&FULL[..], &["--presence-file", "present"]].concat())).is_err());
        for missing in 0..6 {
            let mut some = FULL.to_vec();
            some.drain(missing * 2..missing * 2 + 2);
            assert!(parse(args(&some)).is_err());
        }
        assert!(parse(args(&[&FULL[..], &["--save-frames", "/tmp"]].concat())).is_err());
        assert!(
            parse(args(
                &[&FULL[..10], &["--body-socket", "gaze.sock"]].concat()
            ))
            .is_err()
        );
        assert!(
            parse(args(
                &[&FULL[..10], &["--body-socket", "/run/../etc/x"]].concat()
            ))
            .is_err()
        );
        for refused in [
            "01da:5875",
            "1da:5875@3-1",
            "0000:5875@3-1",
            "01da:5875@",
            "01da:5875@3-1;x",
            "zzzz:5875@3-1",
        ] {
            assert_eq!(usb_of(refused), None, "{refused}");
        }
    }

    #[test]
    fn a_permission_is_one_exact_object_and_anything_else_says_nothing() {
        assert_eq!(permit_of(br#"{"camera":true}"#), Some(true));
        assert_eq!(permit_of(br#"{"camera":false}"#), Some(false));
        for ignored in [
            &br#"{"camera":1}"#[..],
            br#"{"camera":true,"save":true}"#,
            br#"{"mood":"listening","level":3}"#,
            b"true",
            b"",
        ] {
            assert_eq!(permit_of(ignored), None);
        }
    }
}
