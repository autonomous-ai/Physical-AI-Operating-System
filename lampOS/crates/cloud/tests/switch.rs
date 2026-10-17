//! `lamp-cloud` is off unless `LAMP_CLOUD=on` (the reviewer on #1418): the
//! binary itself, with a complete account aimed at a local listener, says
//! it is off and connects nowhere without the switch, and connects with it.
//! And its unit touches V1's `os-server` only through the drop-in that turns
//! it on.
use std::io::{ErrorKind, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_lamp-cloud");
const UNIT: &str = include_str!("../../../deploy/product/lamp-cloud.service");

/// A `CREDENTIALS_DIRECTORY` as the unit's `LoadCredential=` makes it, with
/// the broker at `port` on this host. No backend base, so no ping is made.
fn credentials(name: &str, port: u16) -> PathBuf {
    let dir = PathBuf::from(format!("/tmp/lcw-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let account = serde_json::json!({
        "device_id": "dev-42", "mqtt_endpoint": "127.0.0.1", "mqtt_port": port,
        "mqtt_username": "lamp-dev-42", "mqtt_password": "made-up-pw",
        "fa_channel": "fa/dev-42", "fd_channel": "fd/dev-42", "llm_base_url": ""
    });
    std::fs::write(dir.join("account"), account.to_string()).unwrap();
    std::fs::write(dir.join("key"), "made-up-key\n").unwrap();
    std::fs::write(dir.join("build"), "{}").unwrap();
    dir
}

fn spawn(credentials: &Path, switch: Option<&str>) -> Child {
    let mut command = Command::new(BIN);
    command
        .env("CREDENTIALS_DIRECTORY", credentials)
        .env_remove("STATE_DIRECTORY")
        .env_remove("LAMP_CLOUD")
        .env_remove("LAMP_BUS_ROOT")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(value) = switch {
        command.env("LAMP_CLOUD", value);
    }
    command.spawn().unwrap()
}

/// The child's exit within `wait`, or `None` (it is then killed), and what
/// it said.
fn finish(mut child: Child, wait: Duration) -> (Option<ExitStatus>, String) {
    let until = Instant::now() + wait;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut said = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut said);
    }
    (status, said)
}

/// Whether the listener is connected to within `wait`.
fn connected(listener: &TcpListener, wait: Duration) -> bool {
    let until = Instant::now() + wait;
    loop {
        match listener.accept() {
            Ok(_) => return true,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => panic!("{error}"),
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn without_the_switch_it_says_so_and_connects_nowhere() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let dir = credentials("off", listener.local_addr().unwrap().port());
    for switch in [None, Some("1"), Some("off"), Some("")] {
        let (status, said) = finish(spawn(&dir, switch), Duration::from_secs(10));
        let status =
            status.unwrap_or_else(|| panic!("{switch:?}: still running after 10 s: {said}"));
        assert!(status.success(), "{switch:?}: {status:?}");
        assert_eq!(
            said, "lamp-cloud: off: LAMP_CLOUD is not on\n",
            "{switch:?}"
        );
    }
    assert!(!connected(&listener, Duration::from_millis(300)));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn with_the_switch_on_it_connects_to_the_accounts_broker() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let dir = credentials("on", listener.local_addr().unwrap().port());
    let child = spawn(&dir, Some("on"));
    let reached = connected(&listener, Duration::from_secs(10));
    let (status, said) = finish(child, Duration::ZERO);
    assert!(reached, "no connection; it said: {said}");
    assert!(status.is_none(), "it ended ({status:?}); it said: {said}");
    assert!(!said.contains("not on"), "{said}");
    // The broker's password and the key are never said.
    assert!(
        !said.contains("made-up-pw") && !said.contains("made-up-key"),
        "{said}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_unit_stops_v1s_os_server_only_from_the_drop_in_that_turns_it_on() {
    // Every line that is not a comment: no Conflicts= of the unit's own.
    let lines: Vec<&str> = UNIT
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .collect();
    assert!(
        !lines.iter().any(|line| line.starts_with("Conflicts=")),
        "an off lamp-cloud would stop os-server"
    );
    // The header gives the drop-in: the switch and the conflict together.
    assert!(UNIT.contains("#   Conflicts=os-server.service\n"));
    assert!(UNIT.contains("#   Environment=LAMP_CLOUD=on\n"));
}

#[test]
fn lamp_4aces_cloud_on_writes_the_units_whole_drop_in() {
    // lamp-4ace's own installer (#1420) writes the same drop-in the unit's
    // header gives, the conflict with it, so the two can't drift.
    const CLOUD_ON: &str = include_str!("../../../deploy/lamp-4ace/cloud-on.sh");
    let line = CLOUD_ON
        .lines()
        .find(|line| line.contains("> '$DROPIN'"))
        .expect("cloud-on.sh writes the drop-in");
    assert!(
        line.contains(
            r"[Unit]\nConflicts=os-server.service\n[Service]\nEnvironment=LAMP_CLOUD=on\n"
        ),
        "{line}"
    );
}
