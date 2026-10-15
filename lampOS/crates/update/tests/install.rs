//! `lamp-update install` onto a blank lamp made of files, with a channel
//! and a machine of the test's own: no network, no systemd, no lamp
//! (docs/install.md, task #811).
use lamp_update::{
    fetch::{Channel, Fetch},
    install::{self, Host, Install, PRODUCT},
    manifest::{self, File, Manifest, sha256_hex},
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
        Ok(())
    }

    fn tmpfiles(&mut self, path: &str) -> Result<(), String> {
        self.calls.push(format!("tmpfiles {path}"));
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
