use lamp_bus::Bus;
use lamp_msgs::{ClockStamp, TouchEvent, TouchGesture, TouchIntent, TouchSource};
use lamp_setup::{
    Watched,
    reset::{self, Effects, Plan, Step, accept},
};
use std::{
    cell::RefCell,
    fs, io,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const CLOCK: &str = "setup-test-boot";
const PROGRAM: &str = env!("CARGO_BIN_EXE_lamp-setup");
static NEXT: AtomicU64 = AtomicU64::new(1);

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "lamp-setup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::set_permissions(
            self.0.join("memory/locked"),
            fs::Permissions::from_mode(0o700),
        );
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A lamp's worth of personal data under one root, and a file outside it
/// that a link inside points at.
fn lamp(root: &Root) -> Plan {
    for dir in ["memory/days", "tasks", "setup", "outside"] {
        fs::create_dir_all(root.path(dir)).unwrap();
    }
    fs::write(root.path("memory/days/2026-10-15.json"), b"{}").unwrap();
    fs::write(root.path("memory/index.json"), b"{}").unwrap();
    fs::write(root.path("tasks/1.json"), b"{}").unwrap();
    fs::write(root.path("people.json"), b"[]").unwrap();
    fs::write(root.path("outside/keep.txt"), b"keep").unwrap();
    symlink(root.path("outside"), root.path("memory/link-to-outside")).unwrap();
    symlink(root.path("outside/keep.txt"), root.path("thread.json")).unwrap();
    fs::write(root.path("setup/done"), b"").unwrap();
    Plan {
        marker: root.path("setup/done"),
        journal: root.path("setup/reset.json"),
        wipe: vec![
            root.path("memory"),
            root.path("tasks"),
            root.path("people.json"),
            root.path("thread.json"),
            root.path("schedule"),
        ],
        stop: vec![
            "/bin/systemctl".into(),
            "stop".into(),
            "lampos.target".into(),
        ],
        reboot: vec!["/bin/systemctl".into(), "reboot".into()],
    }
}

#[derive(Default)]
struct Fake {
    calls: Rc<RefCell<Vec<String>>>,
    fail_stop: bool,
}
impl Effects for Fake {
    fn run(&mut self, argv: &[String]) -> io::Result<()> {
        self.calls.borrow_mut().push(argv[1].clone());
        if self.fail_stop && argv[1] == "stop" {
            return Err(io::Error::other("stop refused"));
        }
        Ok(())
    }
    fn sync(&mut self) {
        self.calls.borrow_mut().push("sync".into());
    }
}

fn wiped(root: &Root) {
    assert!(
        root.path("memory").is_dir(),
        "a wiped directory keeps itself"
    );
    assert_eq!(fs::read_dir(root.path("memory")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(root.path("tasks")).unwrap().count(), 0);
    assert!(!root.path("people.json").exists());
    assert!(fs::symlink_metadata(root.path("thread.json")).is_err());
    assert_eq!(fs::read(root.path("outside/keep.txt")).unwrap(), b"keep");
    assert!(!root.path("setup/done").exists());
    assert!(!root.path("setup/reset.json").exists());
}

#[test]
fn a_reset_stops_wipes_removes_the_marker_and_reboots_in_that_order() {
    let root = Root::new();
    let plan = lamp(&root);
    let mut fake = Fake::default();
    let steps = reset::reset(&plan, &mut fake).unwrap();
    wiped(&root);
    assert_eq!(steps.first(), Some(&Step::Journal));
    assert_eq!(steps[1], Step::Stopped);
    assert_eq!(
        &steps[steps.len() - 3..],
        [Step::MarkerRemoved, Step::JournalRemoved, Step::Rebooting]
    );
    assert_eq!(*fake.calls.borrow(), ["stop", "sync", "sync", "reboot"]);
}

#[test]
fn a_failed_stop_wipes_nothing_and_the_next_start_finishes_without_a_reboot() {
    let root = Root::new();
    let plan = lamp(&root);
    let mut fake = Fake {
        fail_stop: true,
        ..Fake::default()
    };
    assert!(reset::reset(&plan, &mut fake).is_err());
    assert!(root.path("setup/reset.json").exists());
    assert!(root.path("setup/done").exists());
    assert!(root.path("people.json").exists());

    let mut start = Fake::default();
    let steps = reset::resume(&plan, &mut start).unwrap();
    wiped(&root);
    assert!(!steps.contains(&Step::Rebooting));
    assert_eq!(*start.calls.borrow(), ["sync", "sync"]);
    // Nothing left to finish.
    assert!(
        reset::resume(&plan, &mut Fake::default())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_wipe_that_fails_halfway_keeps_the_journal_and_marker_until_it_finishes() {
    let root = Root::new();
    let plan = lamp(&root);
    let locked = root.path("memory/locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("inside.json"), b"{}").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();
    let error = reset::reset(&plan, &mut Fake::default()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied, "{error}");
    assert!(root.path("setup/reset.json").exists());
    assert!(root.path("setup/done").exists());

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    reset::resume(&plan, &mut Fake::default()).unwrap();
    wiped(&root);
}

#[test]
fn without_a_journal_the_start_touches_nothing() {
    let root = Root::new();
    let plan = lamp(&root);
    let mut fake = Fake::default();
    assert!(reset::resume(&plan, &mut fake).unwrap().is_empty());
    assert!(fake.calls.borrow().is_empty());
    assert!(root.path("people.json").exists());
    assert!(root.path("setup/done").exists());
}

#[test]
fn a_plan_never_wipes_software_system_or_shallow_paths() {
    let root = Root::new();
    let good = lamp(&root);
    good.check().unwrap();
    let with = |wipe: &str| Plan {
        wipe: vec![PathBuf::from(wipe)],
        ..good.clone()
    };
    for path in [
        "/opt/lampOS/voice/release",
        "/etc/lampOS/update/key.pub",
        "/etc/lampOS",
        "/usr/local/share/x",
        "/home/orangepi/data",
        "/var/lib",
        "/var",
        "var/lib/lampos-memory",
        "/var/lib/../lib/lampos-memory",
        "/var/lib/./lampos-memory",
        "/var/lib/lampos-memory/",
    ] {
        assert!(with(path).check().is_err(), "{path}");
    }
    assert!(with("/var/lib/lampos-memory").check().is_ok());
    let journal_inside = Plan {
        journal: good.wipe[0].join("reset.json"),
        ..good.clone()
    };
    assert!(journal_inside.check().is_err());
    let relative_command = Plan {
        stop: vec!["systemctl".into(), "stop".into()],
        ..good.clone()
    };
    assert!(relative_command.check().is_err());
    let nothing = Plan {
        wipe: vec![],
        ..good.clone()
    };
    assert!(nothing.check().is_err());
    // A refused plan touches nothing.
    assert!(reset::reset(&with("/var"), &mut Fake::default()).is_err());
    assert!(root.path("people.json").exists());
}

fn event(at_us: u64, held_us: u64) -> TouchEvent {
    TouchEvent {
        stamp: ClockStamp {
            clock_id: CLOCK.into(),
            at_us,
            sequence: 1,
        },
        session_id: "controls-session".into(),
        reader_session_id: "gpio-reader".into(),
        enabled: true,
        source: TouchSource::Reset,
        name: "reset".into(),
        contact_id: 1,
        gesture: TouchGesture::Hold,
        when_speaking: TouchIntent::FactoryReset,
        when_quiet: TouchIntent::FactoryReset,
        when_asleep: Some(TouchIntent::FactoryReset),
        began_us: at_us - held_us,
        observed_us: at_us,
        released_us: Some(at_us),
        active_mask: 0,
        active_names: vec![],
        position: None,
        toward_pad: None,
        distance: 0.0,
        speed: 0.0,
    }
}

#[test]
fn only_an_enabled_fresh_release_after_five_seconds_on_this_boot_is_a_reset() {
    let at = 20_000_000;
    let good = event(at, 5_000_000);
    assert_eq!(accept(&good, CLOCK, at + 1_000), Ok(()));
    assert!(accept(&event(at, 4_999_999), CLOCK, at).is_err());
    let mut shadow = good.clone();
    shadow.enabled = false;
    assert_eq!(accept(&shadow, CLOCK, at), Err("controls is in shadow"));
    assert_eq!(
        accept(&good, "another-boot", at),
        Err("another boot's clock")
    );
    assert_eq!(accept(&good, CLOCK, at + 500_000), Err("stale"));
    assert_eq!(accept(&good, CLOCK, at - 1), Err("from the future"));
    let mut primary = good.clone();
    primary.source = TouchSource::Primary;
    primary.when_speaking = TouchIntent::SoftwareMute;
    primary.when_quiet = TouchIntent::SoftwareMute;
    primary.when_asleep = Some(TouchIntent::SoftwareMute);
    assert!(accept(&primary, CLOCK, at).is_err());
}

fn publish_and_watch(enabled: bool, events: Vec<TouchEvent>) -> (Watched, usize) {
    // A short root: a Unix socket's address holds 103 bytes.
    let root = PathBuf::from(format!(
        "/tmp/ls-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _clean = Root(root.clone());
    let bus = Bus::new(&root).unwrap();
    let mut touch = bus
        .subscribe_typed(TouchEvent::TOPIC, "setup", TouchEvent::TYPE)
        .unwrap();
    let mut publisher = bus
        .publisher_declared(TouchEvent::TOPIC, &TouchEvent::declaration(), "controls")
        .unwrap();
    // The subscriber is found by the publisher's next discovery.
    std::thread::sleep(Duration::from_millis(250));
    for event in &events {
        publisher.publish_json(event.stamp.at_us, event).unwrap();
    }
    let acted = RefCell::new(0);
    let now = events.last().map_or(0, |event| event.stamp.at_us + 1_000);
    let watched = lamp_setup::watch(
        &mut touch,
        CLOCK,
        enabled,
        Some(Instant::now() + Duration::from_millis(600)),
        || now,
        |_| {},
        || {
            *acted.borrow_mut() += 1;
            Ok(())
        },
    )
    .unwrap();
    (watched, acted.into_inner())
}

#[test]
fn the_watcher_acts_once_on_a_real_bus_and_only_logs_with_the_switch_off() {
    let at = 30_000_000;
    // Passed over: one too old, one held under 5 s. Then a good one.
    let stale = event(at - 600_000, 5_000_000);
    let mut short = event(at - 2_000, 4_000_000);
    short.contact_id = 2;
    let mut good = event(at, 6_000_000);
    good.contact_id = 3;
    assert_eq!(
        publish_and_watch(true, vec![stale, short, good.clone()]),
        (Watched::Reset, 1)
    );
    assert_eq!(
        publish_and_watch(false, vec![good.clone()]),
        (Watched::Shadow, 0)
    );
    let mut shadow_controls = good.clone();
    shadow_controls.enabled = false;
    assert_eq!(
        publish_and_watch(true, vec![shadow_controls]),
        (Watched::Ended, 0)
    );
}

#[test]
fn the_command_checks_plans_and_reports_an_unfinished_reset_without_acting() {
    let root = Root::new();
    let plan = lamp(&root);
    let file = root.path("plan.json");
    fs::write(&file, serde_json::to_vec(&plan).unwrap()).unwrap();
    let run = |args: &[&str]| Command::new(PROGRAM).args(args).output().unwrap();
    let checked = run(&["check", "--plan", file.to_str().unwrap()]);
    assert!(checked.status.success(), "{checked:?}");
    fs::write(root.path("setup/reset.json"), b"{}").unwrap();
    let shadow = run(&["resume-reset", "--plan", file.to_str().unwrap()]);
    assert!(shadow.status.success());
    assert!(String::from_utf8_lossy(&shadow.stdout).contains("\"unfinished\":true"));
    assert!(
        root.path("people.json").exists(),
        "nothing wiped without --enable"
    );
    let mut bad = plan.clone();
    bad.wipe = vec![Path::new("/opt/lampOS/voice").into()];
    fs::write(&file, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert_eq!(
        run(&["check", "--plan", file.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
}
