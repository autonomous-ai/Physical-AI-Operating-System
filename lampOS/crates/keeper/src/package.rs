//! The keeper as the one service the old runtime's updater installs: a
//! package with `bin/service` and `VERSION` (the parent repository's
//! `docs/bootstrap-ota.md`, "service replacement"). The updater runs
//! `bin/service serve` under a unit of its own and asks `bin/service health`
//! whether it is ready. The keeper does not run the parts itself: systemd
//! does, from the units the package carries, which the keeper links and
//! ties to its own unit so that stopping it stops the lamp.
use crate::units::{LAMP, Systemd};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// What the updater sets in the environment of `serve` and `health`.
pub const NAME_VARIABLE: &str = "AUTONOMOUS_SERVICE_NAME";
pub const DATA_VARIABLE: &str = "AUTONOMOUS_SERVICE_DATA";
/// Where `serve` leaves what `health` reads, in the service's data folder.
pub const STATE_FILE: &str = "keeper.json";

/// The answer `health` prints, exactly the fields the updater checks:
/// ready, the version it installed, and the process its unit runs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub ready: bool,
    pub version: String,
    pub pid: u32,
    /// The parts of the lamp that are not running yet, for a person.
    #[serde(default)]
    pub waiting_for: Vec<String>,
    /// The old runtime's units this run found running and stopped.
    #[serde(default)]
    pub legacy_stopped: Vec<String>,
}

/// A release: the folder above `bin/`, holding `VERSION` and `units/`.
pub struct Release {
    pub root: PathBuf,
}

impl Release {
    /// The release this program was started from.
    pub fn of_this_program() -> Option<Self> {
        let program = std::env::current_exe().ok()?;
        let root = program.parent()?.parent()?.to_path_buf();
        Some(Self { root })
    }

    pub fn version(&self) -> String {
        fs::read_to_string(self.root.join("VERSION"))
            .map(|text| text.trim().to_owned())
            .unwrap_or_default()
    }

    /// Every unit file the release carries, in name order.
    pub fn unit_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = fs::read_dir(self.root.join("units"))
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                    .filter(|path| {
                        matches!(
                            path.extension().and_then(|ext| ext.to_str()),
                            Some("service" | "target" | "slice")
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        files
    }
}

/// The old runtime's units whose running makes a part refuse to start:
/// exactly the list `lamp-voice` checks (`refuse_legacy_owners`).
pub const LEGACY: [&str; 4] = [
    "hal.service",
    "os-server.service",
    "led-boot.service",
    "led-shutdown.service",
];

/// Stop each of the old runtime's units that is running, and mask it so
/// that it does not come back: `led-shutdown.service` starts again whenever
/// `hal` stops. Called only when the keeper's unit is the updater's
/// replacement for the old runtime, which has already stopped `hal` and
/// `os-server`. Nothing is deleted; unmasking is the updater's rollback's.
/// Returns the units it stopped; a unit it could not mask is still stopped,
/// and said.
pub fn clear_legacy(systemd: &mut impl Systemd) -> Vec<String> {
    let mut stopped = Vec::new();
    for unit in LEGACY {
        if !systemd.active(unit) {
            continue;
        }
        // Masked first: stopping one may start another (hal's stop starts
        // led-shutdown), and a masked unit is not started.
        if let Err(why) = systemd.mask(unit) {
            eprintln!("lamp-keeper: masking {unit}: {why}");
        }
        match systemd.command("stop", unit) {
            Ok(()) => {
                eprintln!("lamp-keeper: stopped the old runtime's {unit}");
                stopped.push(unit.to_owned());
            }
            Err(why) => eprintln!("lamp-keeper: stopping {unit}: {why}"),
        }
    }
    stopped
}

/// The group the parts share: the first `Group=` other than root in the
/// release's units (lamp-52e6's and lamp-4ace's: `orangepi`).
pub fn parts_group(release: &Release) -> Option<String> {
    release.unit_files().iter().find_map(|file| {
        fs::read_to_string(file).ok()?.lines().find_map(|line| {
            let group = line.trim().strip_prefix("Group=")?.trim();
            (!group.is_empty() && group != "root").then(|| group.to_owned())
        })
    })
}

/// A group's id from the text of `/etc/group`.
pub fn gid_of(groups: &str, name: &str) -> Option<u32> {
    groups.lines().find_map(|line| {
        let mut fields = line.split(':');
        if fields.next()? != name {
            return None;
        }
        fields.nth(1)?.parse().ok()
    })
}

/// The bus root as on lamp-4ace (bus.md, "On the lamp"): root's, the
/// parts' group's, mode 2770, so that the voice (root) and the parts run as
/// the lamp's user can both use it, and everything made below takes the
/// group. Made if missing and put right if there; what is already below it
/// is left as it is. `chown` is `std::os::unix::fs::chown` on a lamp.
pub fn make_bus_root(
    root: &Path,
    gid: u32,
    chown: impl FnOnce(&Path, u32, u32) -> std::io::Result<()>,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let at = |error: std::io::Error| format!("{}: {error}", root.display());
    fs::create_dir_all(root).map_err(at)?;
    chown(root, 0, gid).map_err(at)?;
    // After the chown, which may clear the set-group-ID bit.
    fs::set_permissions(root, fs::Permissions::from_mode(0o2770)).map_err(at)
}

/// The drop-in that ties the lamp to the keeper's own unit: when the
/// updater stops the keeper, the lamp stops with it, in order; started, the
/// lamp comes after it.
pub fn binding(service: &str) -> String {
    format!("[Unit]\nBindsTo={service}.service\nAfter={service}.service\n")
}

/// The part that sets a lamp up for a home (#806): what runs instead of the
/// lamp until the setup marker exists.
pub const SETUP: &str = "lamp-setup.service";

/// Where the updater's keeper keeps a link to itself, so that others (the
/// setup's reset) can run `lamp-keeper stop lamp` by one path whatever the
/// version.
pub const COMMAND_LINK: &str = "/usr/local/bin/lamp-keeper";

/// Bring the lamp up from a release: link its units, tie the lamp to the
/// keeper's unit, reload, make each part wanted by the lamp, start the
/// lamp. The lamp itself is not enabled: at power-up it is the keeper's unit
/// that starts, and the lamp with it. `drop_in_dir` is
/// `/etc/systemd/system/lampos.target.d` on a lamp.
///
/// With a setup marker (`--setup-marker`) that does not exist yet, the
/// lamp is not started: only [`SETUP`] is, and [`start_when_set_up`] starts
/// the lamp once the marker appears. True when the lamp was started.
pub fn bring_up(
    systemd: &mut impl Systemd,
    release: &Release,
    service: &str,
    drop_in_dir: &Path,
    setup_marker: Option<&Path>,
) -> Result<bool, String> {
    let files = release.unit_files();
    if files.is_empty() {
        return Err(format!(
            "no units in {}",
            release.root.join("units").display()
        ));
    }
    for file in &files {
        systemd.link(file)?;
    }
    fs::create_dir_all(drop_in_dir)
        .map_err(|error| format!("{}: {error}", drop_in_dir.display()))?;
    let drop_in = drop_in_dir.join("keeper.conf");
    fs::write(&drop_in, binding(service))
        .map_err(|error| format!("{}: {error}", drop_in.display()))?;
    systemd.reload()?;
    for unit in services(&files) {
        systemd.enable(&unit)?;
    }
    if setup_marker.is_some_and(|marker| !marker.exists()) {
        systemd.command("start", SETUP)?;
        return Ok(false);
    }
    systemd.command("start", LAMP)?;
    Ok(true)
}

/// Starts the lamp once the setup marker exists. True when it did.
pub fn start_when_set_up(systemd: &mut impl Systemd, marker: &Path) -> Result<bool, String> {
    if !marker.exists() {
        return Ok(false);
    }
    systemd.command("start", LAMP)?;
    Ok(true)
}

/// Points `link` at this release's `bin/service`, replaced whole: a new
/// link under a temporary name, renamed over the old one.
pub fn link_command(release: &Release, link: &Path) -> Result<(), String> {
    let partial = link.with_extension("partial");
    let _ = fs::remove_file(&partial);
    std::os::unix::fs::symlink(release.root.join("bin/service"), &partial)
        .and_then(|()| fs::rename(&partial, link))
        .map_err(|error| format!("{}: {error}", link.display()))
}

/// The names of the parts among these unit files.
fn services(files: &[PathBuf]) -> Vec<String> {
    files
        .iter()
        .filter_map(|file| file.file_name()?.to_str().map(str::to_owned))
        .filter(|unit| unit.ends_with(".service"))
        .collect()
}

/// The parts the release carries that are not running: the lamp is ready
/// when this is empty.
pub fn waiting_for(systemd: &mut impl Systemd, release: &Release) -> Vec<String> {
    services(&release.unit_files())
        .into_iter()
        .filter(|unit| !systemd.active(unit))
        .collect()
}

/// Written by `serve` after every look, read by `health`. Written whole and
/// renamed into place, so `health` never reads half of it.
pub fn write_answer(data: &Path, answer: &Answer) -> Result<(), String> {
    let text = serde_json::to_string(answer).map_err(|error| error.to_string())?;
    let partial = data.join(format!("{STATE_FILE}.partial"));
    fs::write(&partial, text).map_err(|error| format!("{}: {error}", partial.display()))?;
    fs::rename(&partial, data.join(STATE_FILE)).map_err(|error| error.to_string())
}

/// What `health` prints: what `serve` last wrote, or not ready.
pub fn read_answer(data: &Path) -> Answer {
    fs::read_to_string(data.join(STATE_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Fake {
        said: RefCell<Vec<String>>,
        running: Vec<&'static str>,
    }

    impl Systemd for Fake {
        fn active(&mut self, unit: &str) -> bool {
            self.running.contains(&unit)
        }
        fn command(&mut self, verb: &str, unit: &str) -> Result<(), String> {
            self.said.borrow_mut().push(format!("{verb} {unit}"));
            Ok(())
        }
        fn show(&mut self, _: &[&str]) -> Result<String, String> {
            Ok(String::new())
        }
        fn link(&mut self, file: &Path) -> Result<(), String> {
            let name = file.file_name().unwrap().to_str().unwrap();
            self.said.borrow_mut().push(format!("link {name}"));
            Ok(())
        }
        fn reload(&mut self) -> Result<(), String> {
            self.said.borrow_mut().push("daemon-reload".into());
            Ok(())
        }
        fn enable(&mut self, unit: &str) -> Result<(), String> {
            self.said.borrow_mut().push(format!("enable {unit}"));
            Ok(())
        }
        fn mask(&mut self, unit: &str) -> Result<(), String> {
            self.said.borrow_mut().push(format!("mask {unit}"));
            Ok(())
        }
    }

    fn release(units: &[&str]) -> (tempdir::Dir, Release) {
        let dir = tempdir::Dir::new();
        fs::create_dir_all(dir.0.join("units")).unwrap();
        fs::write(dir.0.join("VERSION"), "0.3.1\n").unwrap();
        for unit in units {
            fs::write(dir.0.join("units").join(unit), "[Unit]\n").unwrap();
        }
        fs::write(dir.0.join("units").join("README.md"), "").unwrap();
        let root = dir.0.clone();
        (dir, Release { root })
    }

    mod tempdir {
        pub struct Dir(pub std::path::PathBuf);
        impl Dir {
            pub fn new() -> Self {
                use std::sync::atomic::{AtomicU32, Ordering};
                static NEXT: AtomicU32 = AtomicU32::new(0);
                let path = std::env::temp_dir().join(format!(
                    "lamp-keeper-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir_all(&path).unwrap();
                Self(path)
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn the_lamp_is_brought_up_from_the_units_the_release_carries_and_tied_to_the_keeper() {
        let (dir, release) = release(&["lamp-voice.service", "lamp-body.service", "lampos.target"]);
        let drop_ins = dir.0.join("lampos.target.d");
        let mut systemd = Fake::default();
        assert!(bring_up(&mut systemd, &release, "lamp-os", &drop_ins, None).unwrap());
        assert_eq!(
            *systemd.said.borrow(),
            [
                "link lamp-body.service",
                "link lamp-voice.service",
                "link lampos.target",
                "daemon-reload",
                "enable lamp-body.service",
                "enable lamp-voice.service",
                "start lampos.target"
            ]
        );
        assert_eq!(
            fs::read_to_string(drop_ins.join("keeper.conf")).unwrap(),
            "[Unit]\nBindsTo=lamp-os.service\nAfter=lamp-os.service\n"
        );
        assert_eq!(release.version(), "0.3.1");
    }

    #[test]
    fn with_its_marker_present_the_setup_check_changes_nothing() {
        let units = ["lamp-voice.service", "lampos.target"];
        let said = |marker: Option<&Path>| {
            let (dir, release) = release(&units);
            let mut systemd = Fake::default();
            let drop_ins = dir.0.join("lampos.target.d");
            assert!(bring_up(&mut systemd, &release, "lamp-os", &drop_ins, marker).unwrap());
            systemd.said.into_inner()
        };
        let (marked, _) = release(&[]);
        let marker = marked.0.join("done");
        fs::write(&marker, "").unwrap();
        assert_eq!(said(Some(&marker)), said(None));
    }

    #[test]
    fn without_its_marker_only_setup_starts_and_the_lamp_once_the_marker_appears() {
        let (dir, release) = release(&["lamp-voice.service", "lampos.target"]);
        let marker = dir.0.join("done");
        let mut systemd = Fake::default();
        let drop_ins = dir.0.join("lampos.target.d");
        assert!(!bring_up(&mut systemd, &release, "lamp-os", &drop_ins, Some(&marker)).unwrap());
        assert_eq!(
            systemd.said.borrow().last().unwrap(),
            "start lamp-setup.service"
        );
        assert!(
            !systemd
                .said
                .borrow()
                .iter()
                .any(|said| said == "start lampos.target")
        );
        // Not yet set up: nothing more.
        assert!(!start_when_set_up(&mut systemd, &marker).unwrap());
        fs::write(&marker, "").unwrap();
        assert!(start_when_set_up(&mut systemd, &marker).unwrap());
        let starts = systemd
            .said
            .borrow()
            .iter()
            .filter(|said| *said == "start lampos.target")
            .count();
        assert_eq!(starts, 1);
    }

    #[test]
    fn the_command_link_points_at_the_running_release_and_is_replaced_whole() {
        let (dir, old) = release(&[]);
        let link = dir.0.join("lamp-keeper");
        link_command(&old, &link).unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), old.root.join("bin/service"));
        let (_newer_dir, newer) = release(&[]);
        link_command(&newer, &link).unwrap();
        assert_eq!(
            fs::read_link(&link).unwrap(),
            newer.root.join("bin/service")
        );
        assert!(!link.with_extension("partial").exists());
    }

    #[test]
    fn a_release_without_units_is_not_started() {
        let (dir, release) = release(&[]);
        let mut systemd = Fake::default();
        let why = bring_up(&mut systemd, &release, "lamp-os", &dir.0.join("d"), None).unwrap_err();
        assert!(why.starts_with("no units in"), "{why}");
        assert!(systemd.said.borrow().is_empty());
    }

    #[test]
    fn the_lamp_is_ready_when_every_part_it_carries_runs() {
        let (_dir, release) =
            release(&["lamp-voice.service", "lamp-body.service", "lampos.target"]);
        let mut systemd = Fake {
            running: vec!["lamp-voice.service"],
            ..Fake::default()
        };
        assert_eq!(waiting_for(&mut systemd, &release), ["lamp-body.service"]);
        systemd.running.push("lamp-body.service");
        assert!(waiting_for(&mut systemd, &release).is_empty());
    }

    #[test]
    fn health_reads_what_serve_wrote_and_is_not_ready_without_it() {
        let (dir, _release) = release(&[]);
        assert_eq!(read_answer(&dir.0), Answer::default());
        let answer = Answer {
            ready: true,
            version: "0.3.1".into(),
            pid: 4242,
            waiting_for: vec![],
            legacy_stopped: vec!["led-shutdown.service".into()],
        };
        write_answer(&dir.0, &answer).unwrap();
        assert_eq!(read_answer(&dir.0), answer);
        // The fields the updater checks, by those names.
        let text = fs::read_to_string(dir.0.join(STATE_FILE)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["ready"], true);
        assert_eq!(value["version"], "0.3.1");
        assert_eq!(value["pid"], 4242);
    }

    #[test]
    fn only_the_old_units_that_run_are_masked_and_stopped() {
        let mut systemd = Fake {
            running: vec!["led-shutdown.service", "lamp-voice.service"],
            ..Fake::default()
        };
        assert_eq!(clear_legacy(&mut systemd), ["led-shutdown.service"]);
        assert_eq!(
            *systemd.said.borrow(),
            ["mask led-shutdown.service", "stop led-shutdown.service"]
        );
        let mut idle = Fake::default();
        assert!(clear_legacy(&mut idle).is_empty());
        assert!(idle.said.borrow().is_empty());
    }

    #[test]
    fn the_bus_root_is_root_s_with_the_parts_group_and_set_group_id() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, release) = release(&[]);
        let units = dir.0.join("units");
        fs::write(
            units.join("lamp-body.service"),
            "[Service]\nUser=orangepi\nGroup=orangepi\n",
        )
        .unwrap();
        fs::write(
            units.join("lamp-actions.service"),
            "[Service]\nUser=root\nGroup=root\n",
        )
        .unwrap();
        assert_eq!(parts_group(&release).as_deref(), Some("orangepi"));
        let groups = "root:x:0:\naudio:x:29:orangepi\norangepi:x:1000:\n";
        assert_eq!(gid_of(groups, "orangepi"), Some(1000));
        assert_eq!(gid_of(groups, "nobody"), None);

        let root = dir.0.join("run/lampos-bus");
        // Twice: made, then found and put right the same way.
        for _ in 0..2 {
            let mut owned = None;
            make_bus_root(&root, 1000, |path, uid, gid| {
                owned = Some((path.to_owned(), uid, gid));
                Ok(())
            })
            .unwrap();
            assert_eq!(owned, Some((root.clone(), 0, 1000)));
            let mode = fs::metadata(&root).unwrap().permissions().mode() & 0o7777;
            assert_eq!(mode, 0o2770);
        }
    }
}
