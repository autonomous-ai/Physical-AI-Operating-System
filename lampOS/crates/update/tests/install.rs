//! `lamp-update install` onto a blank lamp made of files, with a channel
//! and a machine of the test's own: no network, no systemd, no lamp
//! (docs/install.md, task #811).
use lamp_update::{
    fetch::{Channel, Fetch},
    install::{self, Host, Install, PRODUCT, SETTINGS},
    manifest::{self, File, Manifest, sha256_hex},
    records::{Records, State},
};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const CHANNEL: &str = "https://channel.test/lamp-voice/";

#[derive(Default)]
struct FakeChannel {
    files: HashMap<String, Vec<u8>>,
}

impl Channel for FakeChannel {
    fn fetch(&mut self, url: &str, most: u64, out: &mut dyn Write) -> Result<u64, Fetch> {
        let bytes = self.files.get(url).ok_or(Fetch::Missing)?;
        if bytes.len() as u64 > most {
            return Err(Fetch::Failed("larger than expected".into()));
        }
        out.write_all(bytes).unwrap();
        Ok(bytes.len() as u64)
    }
}

/// The machine: users go into the root's /etc/passwd, and every call is
/// remembered.
struct FakeHost {
    root: PathBuf,
    calls: Vec<String>,
    /// How many of the next `enable`s fail, as a `systemctl` that is not
    /// there yet or refuses.
    failing_enables: u32,
    /// `harness service install` refuses, as for a user it cannot serve.
    harness_refuses: bool,
}

impl Host for FakeHost {
    fn add_user(&mut self, name: &str, groups: &[String]) -> Result<(), String> {
        let passwd = self.root.join("etc/passwd");
        let mut text = fs::read_to_string(&passwd).unwrap_or_default();
        text.push_str(&format!(
            "{name}:x:999:999::/nonexistent:/usr/sbin/nologin\n"
        ));
        fs::write(passwd, text).unwrap();
        self.calls
            .push(format!("useradd {name} {}", groups.join(",")));
        Ok(())
    }

    fn enable(&mut self, units: &[String]) -> Result<(), String> {
        self.calls.push(format!("enable {}", units.join(" ")));
        if self.failing_enables > 0 {
            self.failing_enables -= 1;
            return Err("systemctl enable: Failed to connect to bus".into());
        }
        Ok(())
    }

    fn tmpfiles(&mut self, path: &str) -> Result<(), String> {
        self.calls.push(format!("tmpfiles {path}"));
        Ok(())
    }

    fn harness_at_boot(&mut self, user: &str, home: &str) -> Result<(), String> {
        self.calls.push(format!("harness {user} {home}"));
        if self.harness_refuses {
            return Err("harness service install: no user manager".into());
        }
        // What logind and the Harness CLI leave behind.
        let linger = self.root.join("var/lib/systemd/linger");
        fs::create_dir_all(&linger).unwrap();
        fs::write(linger.join(user), "").unwrap();
        let unit = self
            .root
            .join(home.trim_start_matches('/'))
            .join(".config/systemd/user");
        fs::create_dir_all(&unit).unwrap();
        fs::write(unit.join("harnessd.service"), "[Service]\n").unwrap();
        Ok(())
    }

    fn utc_offset_minutes(&mut self, zone: &str) -> Result<i32, String> {
        self.calls.push(format!("offset {zone}"));
        Ok(if zone == "Asia/Ho_Chi_Minh" { 420 } else { 0 })
    }
}

struct Lamp {
    _dir: tempfile::TempDir,
    root: PathBuf,
    channel: FakeChannel,
    host: FakeHost,
    config: Install,
    said: Vec<String>,
}

/// Every binary the product starts, and the voice's worker: a line of text each.
fn binaries() -> Vec<String> {
    let mut names: Vec<String> = install::parts()
        .into_iter()
        .map(|part| part.binary)
        .collect();
    names.push("lamp-turn-end".into());
    names
}

impl Lamp {
    /// A blank lamp: the base image's /etc, its zone and the update key.
    /// The channel carries `build` with every binary but those in `leave_out`.
    fn new(sequence: u64, leave_out: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("etc")).unwrap();
        fs::write(root.join("etc/passwd"), "root:x:0:0::/root:/bin/bash\n").unwrap();
        fs::write(
            root.join("etc/group"),
            "root:x:0:\naudio:x:29:\nvideo:x:44:\ndialout:x:20:\n",
        )
        .unwrap();
        fs::write(root.join("etc/timezone"), "Asia/Ho_Chi_Minh\n").unwrap();
        let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap();
        let mut channel = FakeChannel::default();
        let mut files = Vec::new();
        for name in binaries() {
            if leave_out.contains(&name.as_str()) {
                continue;
            }
            let bytes = format!("{name} of build {sequence}\n").into_bytes();
            files.push(File {
                name: name.clone(),
                sha256: sha256_hex(&bytes),
                size: bytes.len() as u64,
            });
            channel.files.insert(format!("{CHANNEL}{name}"), bytes);
        }
        let manifest = Manifest {
            schema: 1,
            channel: "lamp-voice".into(),
            sequence,
            commit: "a".repeat(40),
            files_url: CHANNEL.into(),
            files,
            units: Vec::new(),
        };
        let bytes = serde_json::to_vec(&manifest).unwrap();
        channel.files.insert(
            format!("{CHANNEL}{}", manifest::ENVELOPE),
            manifest::seal(&bytes, &key).unwrap(),
        );
        let config = Install {
            root: root.clone(),
            channel: "lamp-voice".into(),
            channel_url: CHANNEL.into(),
            key: key.public_key().as_ref().try_into().unwrap(),
            records: "/var/lib/lampos-update/".into(),
        };
        let host = FakeHost {
            root: root.clone(),
            calls: Vec::new(),
            failing_enables: 0,
            harness_refuses: false,
        };
        Self {
            _dir: dir,
            root,
            channel,
            host,
            config,
            said: Vec::new(),
        }
    }

    fn install(&mut self) -> Result<install::Installed, String> {
        let said = &mut self.said;
        install::install(
            &self.config,
            &mut self.channel,
            &mut self.host,
            &mut |line: &str| said.push(line.to_owned()),
        )
    }

    fn on(&self, path: &str) -> PathBuf {
        self.root.join(path.trim_start_matches('/'))
    }
}

/// Every file and link under a folder, with its bytes or its target.
fn tree(root: &Path) -> BTreeMap<PathBuf, String> {
    let mut found = BTreeMap::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            let key = path.strip_prefix(root).unwrap().to_path_buf();
            if meta.file_type().is_symlink() {
                found.insert(
                    key,
                    format!("-> {}", fs::read_link(&path).unwrap().display()),
                );
            } else if meta.is_dir() {
                folders.push(path);
            } else {
                found.insert(key, sha256_hex(&fs::read(&path).unwrap()));
            }
        }
    }
    found
}

#[test]
fn a_blank_lamp_gets_every_part_its_units_and_nothing_started() {
    let mut lamp = Lamp::new(188, &[]);
    let done = lamp.install().unwrap();
    assert_eq!(done.sequence, 188);

    for part in install::parts() {
        let current = lamp.on(&format!("{}current", part.folder));
        let target = fs::read_link(&current).unwrap();
        assert_eq!(
            target.to_string_lossy().len(),
            64,
            "{current:?} -> {target:?}"
        );
        let binary = fs::read_to_string(current.join(&part.binary)).unwrap();
        assert_eq!(binary, format!("{} of build 188\n", part.binary));
    }
    // The voice's worker beside the voice, where the voice starts it.
    assert!(lamp.on("/opt/lampOS/voice/current/lamp-turn-end").is_file());

    for (name, text) in PRODUCT {
        let written = fs::read_to_string(lamp.on(&format!("/etc/systemd/system/{name}"))).unwrap();
        assert_eq!(&written, text, "{name}");
    }
    assert!(lamp.on("/etc/tmpfiles.d/lampos.conf").is_file());
    assert!(lamp.on("/etc/lampOS/controls/controls-pins.json").is_file());
    assert!(lamp.on("/etc/lampOS/expression/expression.json").is_file());
    assert_eq!(
        fs::read_to_string(lamp.on("/etc/lampOS/zone.env"))
            .unwrap()
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>(),
        ["LAMP_ZONE=Asia/Ho_Chi_Minh", "LAMP_UTC_OFFSET_MINUTES=420"]
    );

    // Users, the tmpfiles, enabled; never started, and setup's marker never written.
    let calls = lamp.host.calls.join("\n");
    assert!(
        calls.contains("useradd lampos audio,video,dialout"),
        "{calls}"
    );
    assert!(calls.contains("useradd lampos-memory lampos"), "{calls}");
    assert!(
        calls.contains("tmpfiles /etc/tmpfiles.d/lampos.conf"),
        "{calls}"
    );
    let enabled = lamp
        .host
        .calls
        .iter()
        .find(|call| call.starts_with("enable "))
        .unwrap();
    for name in [
        "lampos.target",
        "lamp-voice.service",
        "lamp-update.service",
        "lamp-records.timer",
    ] {
        assert!(
            enabled.split(' ').any(|word| word == name),
            "{name} in {enabled}"
        );
    }
    assert!(
        !enabled.contains("lamp-records.service"),
        "the timer starts records"
    );
    assert!(!lamp.on("/var/lib/lampos-setup/done").exists());
    // What is one lamp's own is not written, and the person is told.
    assert!(!lamp.on("/etc/lampOS/body/lamp.env").exists());
    assert!(
        lamp.said.iter().any(|line| line.contains("body/lamp.env")),
        "{:?}",
        lamp.said
    );
    assert!(
        lamp.said.iter().any(|line| line.contains("no group i2c")),
        "{:?}",
        lamp.said
    );

    // lamp-update knows which build the lamp runs.
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(lamp.on("/var/lib/lampos-update/state.json")).unwrap())
            .unwrap();
    assert_eq!(state["installed"]["sequence"], 188, "{state}");
}

#[test]
fn a_closed_etc_lampos_is_said_and_left_as_it_is() {
    use std::os::unix::fs::PermissionsExt;
    let mut lamp = Lamp::new(188, &[]);
    fs::create_dir_all(lamp.on("/etc/lampOS")).unwrap();
    fs::set_permissions(lamp.on("/etc/lampOS"), fs::Permissions::from_mode(0o700)).unwrap();
    lamp.install().unwrap();
    let mode = fs::metadata(lamp.on("/etc/lampOS"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);
    assert!(
        lamp.said
            .iter()
            .any(|line| line.contains("/etc/lampOS is closed")),
        "{:?}",
        lamp.said
    );
}

#[test]
fn run_twice_it_changes_nothing() {
    let mut lamp = Lamp::new(188, &[]);
    lamp.install().unwrap();
    let before = tree(&lamp.root);
    let calls = lamp.host.calls.len();
    let again = lamp.install().unwrap();
    assert_eq!(again.changed, Vec::<String>::new());
    assert_eq!(tree(&lamp.root), before);
    assert_eq!(lamp.host.calls.len(), calls, "nothing asked of the machine");
}

#[test]
fn a_file_that_does_not_match_installs_nothing_of_it() {
    let mut lamp = Lamp::new(188, &[]);
    lamp.channel.files.insert(
        format!("{CHANNEL}lamp-body"),
        b"not what was signed\n".to_vec(),
    );
    let why = lamp.install().unwrap_err();
    assert!(
        why.contains("lamp-body does not match its SHA-256"),
        "{why}"
    );
    let body = lamp.on("/opt/lampOS/body");
    let left: Vec<_> = fs::read_dir(&body).unwrap().collect();
    assert!(left.is_empty(), "nothing of the body is left: {left:?}");
    assert!(!lamp.on("/etc/systemd/system/lamp-body.service").exists());
    // No part is linked to a build of a run that failed.
    assert!(fs::symlink_metadata(lamp.on("/opt/lampOS/voice/current")).is_err());
}

#[test]
fn a_build_without_a_part_is_refused_before_anything_is_written() {
    let mut lamp = Lamp::new(188, &["lamp-eyes"]);
    let why = lamp.install().unwrap_err();
    assert!(why.contains("build 188 carries no lamp-eyes"), "{why}");
    assert!(!lamp.on("/opt/lampOS").exists());
    assert!(!lamp.on("/etc/systemd").exists());
}

#[test]
fn a_newer_build_moves_each_link_and_keeps_the_old_folders() {
    let mut lamp = Lamp::new(188, &[]);
    lamp.install().unwrap();
    let old = fs::read_link(lamp.on("/opt/lampOS/voice/current")).unwrap();
    let mut newer = Lamp::new(189, &[]);
    newer.root = lamp.root.clone();
    newer.config.root = lamp.root.clone();
    newer.host.root = lamp.root.clone();
    // The image's key is the newer channel's: the lamp is given it again.
    newer.install().unwrap();
    let new = fs::read_link(newer.on("/opt/lampOS/voice/current")).unwrap();
    assert_ne!(old, new);
    assert!(
        lamp.on("/opt/lampOS/voice").join(&old).is_dir(),
        "the old build stays"
    );
}

/// Every component of the brain with a process of its own has a product
/// unit (docs/brain-build.md, "Components, processes and owners").
#[test]
fn every_part_of_the_brain_has_a_product_unit() {
    let page = include_str!("../../../docs/brain-build.md");
    let table = page
        .split("## Components, processes and owners")
        .nth(1)
        .unwrap()
        .split("\n\n")
        .find(|block| block.starts_with("| Component"))
        .unwrap();
    let mut named = Vec::new();
    for row in table.lines().skip(2) {
        let process = row.split('|').nth(2).unwrap_or_default();
        // The first `lamp-<name>` in the process column; the bus is a
        // library, not a process.
        let Some(start) = process.find("`lamp-") else {
            continue;
        };
        let name: String = process[start + 1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        if name != "lamp-bus" {
            named.push(name);
        }
    }
    assert!(named.len() >= 12, "the table names {named:?}");
    let units: Vec<&str> = PRODUCT.iter().map(|(name, _)| *name).collect();
    for name in &named {
        let unit = format!("{name}.service");
        assert!(units.contains(&unit.as_str()), "{name} has no product unit");
    }
}

/// Setup's marker (agreed with the third on #811, 2026-10-15 17:07; the
/// `marker` of setup's plan).
const SETUP_DONE: &str = "/var/lib/lampos-setup/done";

/// The lines of one section of a unit, without comments.
fn section<'a>(text: &'a str, name: &str) -> Vec<&'a str> {
    let header = format!("[{name}]");
    let mut inside = false;
    let mut lines = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == header;
        } else if inside && !line.is_empty() && !line.starts_with('#') {
            lines.push(line);
        }
    }
    lines
}

/// Nothing of the lamp starts before setup has finished, on any boot
/// (#806). systemd checks a unit's condition only when its own job runs,
/// after the jobs it pulls in are queued, so the target's condition holds
/// back none of its parts (the reviewer on #815): each part in the target
/// carries the condition itself, in `[Unit]` where systemd reads it. The
/// units outside the target listen to nothing and run before setup, but
/// setup itself, which runs only while the marker is missing, and the path
/// unit that starts the lamp once it is there (#811).
#[test]
fn every_part_of_the_lamp_waits_for_setups_marker_itself() {
    let condition = format!("ConditionPathExists={SETUP_DONE}");
    let mut gated = Vec::new();
    for (name, text) in PRODUCT {
        let unit = section(text, "Unit");
        let part_of = unit.contains(&"PartOf=lampos.target");
        let wanted = section(text, "Install").contains(&"WantedBy=lampos.target");
        assert_eq!(part_of, wanted, "{name}: in the target one way only");
        if part_of || *name == "lampos.target" {
            assert!(
                unit.contains(&condition.as_str()),
                "{name} starts before setup"
            );
            gated.push(*name);
        } else if *name == "lamp-setup.service" {
            // Setup is the one that runs while the marker is missing.
            assert!(unit.contains(&format!("ConditionPathExists=!{SETUP_DONE}").as_str()));
        } else if *name == "lampos-setup-done.path" {
            // And this starts the lamp once it is there.
            let path = section(text, "Path");
            assert!(path.contains(&format!("PathExists={SETUP_DONE}").as_str()));
            assert!(path.contains(&"Unit=lampos.target"));
        } else {
            assert!(
                !text.contains("lampos-setup"),
                "{name} is outside the target and names setup"
            );
        }
    }
    assert_eq!(gated.len(), 16, "{gated:?}");
    for outside in ["lamp-update.service", "lamp-records.timer"] {
        assert!(!gated.contains(&outside), "{outside}");
    }
    let page = include_str!("../../../docs/install.md");
    assert!(page.contains(&format!("`{condition}`")));
}

/// #811: no battery clock. Every product part that acts on the time of day
/// trusts the wall clock only once the kernel says a time client set it:
/// the schedule, Expression's night and the voice's telling of the time.
#[test]
fn time_of_day_parts_wait_for_the_kernel_to_say_the_clock_is_set() {
    let unit = |name: &str| {
        install::PRODUCT
            .iter()
            .find(|(unit, _)| *unit == name)
            .unwrap_or_else(|| panic!("no {name}"))
            .1
    };
    let exec = |name: &str| {
        section(unit(name), "Service")
            .into_iter()
            .find(|line| line.starts_with("ExecStart="))
            .unwrap()
            .to_owned()
    };
    let schedule = exec("lamp-schedule.service");
    assert!(schedule.contains(" --kernel-clock") && !schedule.contains("--trust-clock"));
    assert!(exec("lamp-expression.service").contains(" --kernel-clock"));
    let voice = section(unit("lamp-voice.service"), "Service");
    assert!(voice.contains(&"Environment=LAMP_KERNEL_CLOCK=on"));
}

const PINS: &str = "/etc/lampOS/controls/controls-pins.json";

/// The reviewer's case on #815: install, move the primary button to
/// another line on this lamp, install the same build again. The lamp's
/// line stays, it is said, and nothing else is written or asked.
#[test]
fn a_setting_changed_on_the_lamp_is_kept_when_install_runs_again() {
    let mut lamp = Lamp::new(188, &[]);
    lamp.install().unwrap();
    let pins = fs::read_to_string(lamp.on(PINS)).unwrap();
    let ours = pins.replace("\"line\": 99", "\"line\": 100");
    assert_ne!(ours, pins);
    let parsed: serde_json::Value = serde_json::from_str(&ours).unwrap();
    assert_eq!(parsed["buttons"][0]["line"], 100);
    fs::write(lamp.on(PINS), &ours).unwrap();
    let before = tree(&lamp.root);
    let calls = lamp.host.calls.len();
    lamp.said.clear();

    let again = lamp.install().unwrap();
    assert_eq!(fs::read_to_string(lamp.on(PINS)).unwrap(), ours);
    assert_eq!(again.changed, Vec::<String>::new());
    assert_eq!(tree(&lamp.root), before);
    assert_eq!(lamp.host.calls.len(), calls, "nothing asked of the machine");
    assert!(
        lamp.said
            .iter()
            .any(|line| line.starts_with(&format!("kept {PINS}"))),
        "{:?}",
        lamp.said
    );
}

/// A newer build's default reaches a setting nobody changed; a setting
/// changed on the lamp, or there before any install, keeps the lamp's
/// bytes; a setting that is missing is written.
#[test]
fn a_newer_default_replaces_only_a_setting_nobody_changed() {
    let lamp = Lamp::new(188, &[]);
    let (untouched, changed_here, there_before, missing) = (
        "/etc/lampOS/a/untouched.json",
        "/etc/lampOS/a/changed.json",
        "/etc/lampOS/a/before.json",
        "/etc/lampOS/a/missing.json",
    );
    fs::create_dir_all(lamp.on("/etc/lampOS/a")).unwrap();
    fs::write(lamp.on(there_before), "the lamp's own\n").unwrap();
    let mut written = State::default();
    let mut said = Vec::new();
    let mut run = |defaults: &[(&str, &str)], written: &mut State| {
        let mut changed = Vec::new();
        install::settings(
            &lamp.config,
            defaults,
            written,
            &mut changed,
            &mut |line| said.push(line.to_owned()),
            &mut |_| Ok(()),
        )
        .unwrap();
        changed
    };

    let first = run(
        &[
            (untouched, "one\n"),
            (changed_here, "one\n"),
            (there_before, "one\n"),
        ],
        &mut written,
    );
    assert_eq!(
        first,
        [
            format!("wrote {untouched}"),
            format!("wrote {changed_here}")
        ]
    );
    assert!(!written.settings.contains_key(there_before));
    fs::write(lamp.on(changed_here), "mine\n").unwrap();

    let second = run(
        &[
            (untouched, "two\n"),
            (changed_here, "two\n"),
            (there_before, "two\n"),
            (missing, "two\n"),
        ],
        &mut written,
    );
    assert_eq!(
        second,
        [format!("wrote {untouched}"), format!("wrote {missing}")]
    );
    let read = |path: &str| fs::read_to_string(lamp.on(path)).unwrap();
    assert_eq!(read(untouched), "two\n");
    assert_eq!(read(changed_here), "mine\n");
    assert_eq!(read(there_before), "the lamp's own\n");
    assert_eq!(read(missing), "two\n");
    assert_eq!(written.settings[untouched], sha256_hex(b"two\n"));
    assert_eq!(
        written.settings[changed_here],
        sha256_hex(b"one\n"),
        "left as it was"
    );

    // Set back to the default by hand, it is the default's again, and the
    // next build's default replaces it.
    fs::write(lamp.on(changed_here), "two\n").unwrap();
    assert!(run(&[(changed_here, "two\n")], &mut written).is_empty());
    assert_eq!(written.settings[changed_here], sha256_hex(b"two\n"));
    assert_eq!(
        run(&[(changed_here, "three\n")], &mut written),
        [format!("wrote {changed_here}")]
    );
    assert_eq!(read(changed_here), "three\n");

    // Said each time one was kept: the one there before, twice, and the
    // changed one once; never the others.
    let kept: Vec<&String> = said
        .iter()
        .filter(|line| line.starts_with("kept "))
        .collect();
    assert_eq!(kept.len(), 3, "{said:?}");
    assert_eq!(
        kept.iter()
            .filter(|line| line.contains(there_before))
            .count(),
        2
    );
    assert_eq!(
        kept.iter()
            .filter(|line| line.contains(changed_here))
            .count(),
        1
    );
}

impl Lamp {
    fn state(&self) -> State {
        Records::new(self.on("/var/lib/lampos-update")).peek_state()
    }
}

/// The reviewer on #815 (19:17): an install whose `systemctl enable` failed
/// left every file as it should be, so the next run changed nothing and
/// never enabled the lamp; and the settings it wrote were not recorded, so a
/// later build's default took them for the lamp's own. Now what it wrote is
/// recorded before systemd is asked, the debt with it, and the next run pays
/// it once.
#[test]
fn an_install_whose_enable_failed_enables_at_the_next_run_and_keeps_its_settings_record() {
    let mut lamp = Lamp::new(188, &[]);
    lamp.host.failing_enables = 1;
    let why = lamp.install().unwrap_err();
    assert!(why.contains("Failed to connect to bus"), "{why}");
    let state = lamp.state();
    assert!(state.systemd_owed);
    assert_eq!(state.installed.as_ref().map(|b| b.sequence), Some(188));
    for (path, text) in SETTINGS {
        assert_eq!(state.settings[*path], sha256_hex(text.as_bytes()), "{path}");
    }

    let calls = lamp.host.calls.len();
    let again = lamp.install().unwrap();
    assert_eq!(again.changed.len(), 1, "{:?}", again.changed);
    assert!(
        again.changed[0].starts_with("enabled "),
        "{:?}",
        again.changed
    );
    let asked: Vec<&str> = lamp.host.calls[calls..]
        .iter()
        .map(|call| call.split(' ').next().unwrap())
        .collect();
    assert_eq!(asked, ["tmpfiles", "enable"]);
    assert!(!lamp.state().systemd_owed);

    // Paid once: a third run asks nothing.
    let calls = lamp.host.calls.len();
    assert!(lamp.install().unwrap().changed.is_empty());
    assert_eq!(lamp.host.calls.len(), calls);

    // The default the failed run wrote is still known as install's: a later
    // build's default replaces it.
    let (path, _) = SETTINGS[2];
    let mut written = lamp.state();
    let mut changed = Vec::new();
    install::settings(
        &lamp.config,
        &[(path, "{}\n")],
        &mut written,
        &mut changed,
        &mut |_| {},
        &mut |_| Ok(()),
    )
    .unwrap();
    assert_eq!(changed, [format!("wrote {path}")]);
}

/// A setting that cannot be read stops the install before any setting is
/// written; out of the way, the next run writes and records them all.
#[test]
fn a_setting_in_the_way_stops_the_install_before_any_is_written() {
    let mut lamp = Lamp::new(188, &[]);
    let (last, _) = SETTINGS[SETTINGS.len() - 1];
    // A folder where the file goes: it cannot be read or written over.
    fs::create_dir_all(lamp.on(last).join("in-the-way")).unwrap();
    let why = lamp.install().unwrap_err();
    assert!(why.starts_with(last), "{why}");
    for (path, _) in &SETTINGS[..SETTINGS.len() - 1] {
        assert!(!lamp.on(path).exists(), "{path}");
    }

    fs::remove_dir_all(lamp.on(last)).unwrap();
    lamp.install().unwrap();
    let state = lamp.state();
    assert!(state.settings_writing.is_empty(), "{state:?}");
    for (path, text) in SETTINGS {
        assert_eq!(state.settings[*path], sha256_hex(text.as_bytes()), "{path}");
    }
}

/// A later setting's write that fails leaves the earlier ones written and
/// still known as install's, from what was recorded before the first write:
/// a newer default replaces them.
#[test]
fn settings_written_before_one_that_fails_are_still_installs() {
    let lamp = Lamp::new(188, &[]);
    let (first, second) = ("/etc/lampOS/a/first.json", "/etc/lampOS/a/second.json");
    let mut state = State::default();
    let mut saved: Vec<State> = Vec::new();
    let failed = install::settings(
        &lamp.config,
        &[(first, "one\n"), (second, "one\n")],
        &mut state,
        &mut Vec::new(),
        &mut |_| {},
        &mut |state| {
            saved.push(state.clone());
            // Once the intent is saved, something takes the second's place.
            fs::create_dir_all(lamp.on(second).join("in-the-way")).unwrap();
            Ok(())
        },
    );
    assert!(failed.unwrap_err().starts_with(second));
    assert_eq!(fs::read_to_string(lamp.on(first)).unwrap(), "one\n");
    // What is on disk is the intent: the final record was never made.
    assert_eq!(saved.len(), 1);
    let mut on_disk = saved.remove(0);
    assert!(!on_disk.settings.contains_key(first));

    fs::remove_dir_all(lamp.on(second)).unwrap();
    let mut changed = Vec::new();
    install::settings(
        &lamp.config,
        &[(first, "two\n"), (second, "two\n")],
        &mut on_disk,
        &mut changed,
        &mut |line| panic!("{line}"),
        &mut |_| Ok(()),
    )
    .unwrap();
    assert_eq!(
        changed,
        [format!("wrote {first}"), format!("wrote {second}")]
    );
    assert!(on_disk.settings_writing.is_empty());
}

/// The reviewer on #815 (19:32): an install stopped after its units,
/// settings and zone were written but before it saved anything else (there,
/// at its "the body waits" line) left no debt, and the next run asked
/// systemd nothing. The debt is now saved before the first unit is written.
#[test]
fn an_install_stopped_after_writing_its_units_enables_at_the_next_run() {
    let mut lamp = Lamp::new(188, &[]);
    let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        install::install(
            &lamp.config,
            &mut lamp.channel,
            &mut lamp.host,
            &mut |line: &str| {
                if line.starts_with("the body waits") {
                    panic!("stopped here");
                }
            },
        )
    }));
    assert!(stopped.is_err());
    assert!(lamp.on("/etc/systemd/system/lamp-ears.service").exists());
    assert!(lamp.state().systemd_owed);
    assert!(
        lamp.host
            .calls
            .iter()
            .all(|call| !call.starts_with("enable"))
    );

    let again = lamp.install().unwrap();
    assert!(
        again
            .changed
            .iter()
            .any(|line| line.starts_with("enabled ")),
        "{:?}",
        again.changed
    );
    assert!(!lamp.state().systemd_owed);
}

/// A run stopped between writing a setting and recording it: the next
/// run, with a newer default, still knows the file as install's and
/// replaces it, from what was recorded before the write.
#[test]
fn a_setting_written_but_not_yet_recorded_is_still_installs() {
    let lamp = Lamp::new(188, &[]);
    let path = "/etc/lampOS/a/setting.json";
    let mut state = State::default();
    let mut saved: Vec<State> = Vec::new();
    // The first save is the intent; the run is stopped at the second, after
    // the write: what is on disk is the first.
    let stopped = install::settings(
        &lamp.config,
        &[(path, "ninety-nine\n")],
        &mut state,
        &mut Vec::new(),
        &mut |_| {},
        &mut |state| {
            saved.push(state.clone());
            if saved.len() == 2 {
                Err("stopped".into())
            } else {
                Ok(())
            }
        },
    );
    assert!(stopped.is_err());
    assert_eq!(fs::read_to_string(lamp.on(path)).unwrap(), "ninety-nine\n");
    let mut on_disk = saved[0].clone();
    assert!(!on_disk.settings.contains_key(path));

    let mut changed = Vec::new();
    install::settings(
        &lamp.config,
        &[(path, "one hundred\n")],
        &mut on_disk,
        &mut changed,
        &mut |line| panic!("{line}"),
        &mut |_| Ok(()),
    )
    .unwrap();
    assert_eq!(changed, [format!("wrote {path}")]);
    assert_eq!(on_disk.settings[path], sha256_hex(b"one hundred\n"));
    assert!(on_disk.settings_writing.is_empty());
}

/// The reviewer on #815 (21:42): the first run writes one setting and fails
/// on the next; the second, with newer defaults, fails on its first write;
/// the third must still know the first file as install's. Each run's own
/// record is what it saved before it failed.
#[test]
fn two_failed_runs_in_a_row_keep_a_written_setting_as_installs() {
    let lamp = Lamp::new(188, &[]);
    let (first, second) = ("/etc/lampOS/a/first.json", "/etc/lampOS/a/second.json");
    // A folder where a file's write goes beside it: that write fails, and
    // the file itself is still there to read.
    let block = |path: &str| {
        let name = Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let beside = lamp.on(path).with_file_name(format!(".{name}.lamp-update"));
        fs::create_dir_all(beside.join("in-the-way")).unwrap();
        beside
    };
    let run = |defaults: &[(&str, &str)], on_disk: &State, blocked: &str| {
        let mut state = on_disk.clone();
        let mut saved: Vec<State> = Vec::new();
        let result = install::settings(
            &lamp.config,
            defaults,
            &mut state,
            &mut Vec::new(),
            &mut |_| {},
            &mut |state| {
                saved.push(state.clone());
                if saved.len() == 1 && !blocked.is_empty() {
                    block(blocked);
                }
                Ok(())
            },
        );
        (
            result,
            saved.last().cloned().unwrap_or_else(|| on_disk.clone()),
        )
    };

    let (result, on_disk) = run(
        &[(first, "99\n"), (second, "99\n")],
        &State::default(),
        second,
    );
    assert!(result.unwrap_err().starts_with(second));
    assert_eq!(fs::read_to_string(lamp.on(first)).unwrap(), "99\n");
    fs::remove_dir_all(lamp.on("/etc/lampOS/a/.second.json.lamp-update")).unwrap();

    let (result, on_disk) = run(&[(first, "100\n"), (second, "100\n")], &on_disk, first);
    assert!(result.unwrap_err().starts_with(first));
    assert_eq!(fs::read_to_string(lamp.on(first)).unwrap(), "99\n");
    fs::remove_dir_all(lamp.on("/etc/lampOS/a/.first.json.lamp-update")).unwrap();

    let mut said = Vec::new();
    let mut state = on_disk;
    let mut changed = Vec::new();
    install::settings(
        &lamp.config,
        &[(first, "100\n"), (second, "100\n")],
        &mut state,
        &mut changed,
        &mut |line| said.push(line.to_owned()),
        &mut |_| Ok(()),
    )
    .unwrap();
    assert_eq!(said, Vec::<String>::new(), "nothing kept as the lamp's own");
    assert_eq!(
        changed,
        [format!("wrote {first}"), format!("wrote {second}")]
    );
    assert_eq!(fs::read_to_string(lamp.on(first)).unwrap(), "100\n");
    assert!(state.settings_writing.is_empty());
}

/// #811, the first builder's 01:56 recipe: with Harness on the lamp, install
/// sets it to start at boot as `lampos` once; without it, it says so and
/// goes on; a refusal stops the install and is said.
#[test]
fn harness_is_set_to_start_at_boot_once_when_it_is_there() {
    // Not there: said, nothing asked, the install still done.
    let mut lamp = Lamp::new(188, &[]);
    lamp.install().unwrap();
    assert!(
        !lamp
            .host
            .calls
            .iter()
            .any(|call| call.starts_with("harness"))
    );
    assert!(
        lamp.said
            .iter()
            .any(|line| line.contains("Harness is not at"))
    );

    // There: asked once, as lampos in the tasks part's home.
    let mut lamp = Lamp::new(188, &[]);
    let harness = lamp.on(install::HARNESS);
    fs::create_dir_all(harness.parent().unwrap()).unwrap();
    fs::write(&harness, "#!/bin/sh\n").unwrap();
    let done = lamp.install().unwrap();
    assert!(
        done.changed
            .iter()
            .any(|line| line == "Harness starts at boot as lampos")
    );
    let asked = |lamp: &Lamp| {
        lamp.host
            .calls
            .iter()
            .filter(|call| *call == "harness lampos /var/lib/lampos-tasks")
            .count()
    };
    assert_eq!(asked(&lamp), 1);
    // Run again: nothing to do.
    let again = lamp.install().unwrap();
    assert!(!again.changed.iter().any(|line| line.contains("Harness")));
    assert_eq!(asked(&lamp), 1);

    // The reviewer's case on #934: the CLI wrote its unit (and linger was
    // set) and then failed. The next install asks again, and only a success
    // is recorded.
    let mut lamp = Lamp::new(188, &[]);
    let harness = lamp.on(install::HARNESS);
    fs::create_dir_all(harness.parent().unwrap()).unwrap();
    fs::write(&harness, "#!/bin/sh\n").unwrap();
    for (dir, file) in [
        ("var/lib/systemd/linger", "lampos"),
        (
            "var/lib/lampos-tasks/.config/systemd/user",
            "harnessd.service",
        ),
    ] {
        fs::create_dir_all(lamp.root.join(dir)).unwrap();
        fs::write(lamp.root.join(dir).join(file), "").unwrap();
    }
    lamp.host.harness_refuses = true;
    assert!(lamp.install().is_err());
    lamp.host.harness_refuses = false;
    let retried = lamp.install().unwrap();
    assert!(
        retried
            .changed
            .iter()
            .any(|line| line == "Harness starts at boot as lampos")
    );
    assert_eq!(asked(&lamp), 2, "asked again after the failure");
    lamp.install().unwrap();
    assert_eq!(asked(&lamp), 2, "and not after the success");

    // Refused: the install says why and stops there.
    let mut lamp = Lamp::new(188, &[]);
    let harness = lamp.on(install::HARNESS);
    fs::create_dir_all(harness.parent().unwrap()).unwrap();
    fs::write(&harness, "#!/bin/sh\n").unwrap();
    lamp.host.harness_refuses = true;
    let refused = lamp.install().unwrap_err();
    assert!(refused.contains("no user manager"), "{refused}");
}
