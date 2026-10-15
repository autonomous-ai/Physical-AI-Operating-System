//! The reset (#806): the reset button held 5 s and released, as Controls
//! publishes it on `controls/touch`, stops the lamp, wipes the person's data,
//! removes the setup marker and reboots into setup.
//!
//! A journal is written durably before the lamp is stopped and removed only
//! after the wipe. A power cut in between leaves it in place, and the next
//! start finishes the wipe before anything else runs (`resume`).

use lamp_msgs::{TOUCH_EVENT_TTL_US, TouchEvent, TouchGesture, TouchIntent, TouchSource};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Command,
};

/// What is never wiped, whatever a plan says: the software, its update trust,
/// and the system itself.
const PROTECTED: [&str; 12] = [
    "/opt/lampOS",
    "/etc/lampOS/update",
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/boot",
    "/proc",
    "/sys",
    "/dev",
    "/root",
    "/home",
];

/// The written reset plan. Paths are absolute and plain (no `.` or `..`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    /// Present once setup is done; a reset removes it.
    pub marker: PathBuf,
    /// Written before the lamp is stopped; removed once the wipe is done.
    pub journal: PathBuf,
    /// The person's data. A file is removed; a directory keeps itself, its
    /// owner and mode, and loses everything in it (systemd units name some
    /// of these directories and would not start without them).
    pub wipe: Vec<PathBuf>,
    /// Run before the wipe, and must succeed: the lamp's parts stopped.
    pub stop: Vec<String>,
    /// Run after the wipe.
    pub reboot: Vec<String>,
}

fn plain(path: &Path) -> bool {
    // Read the text itself: `Path::components` quietly drops a `.` in the
    // middle and a trailing `/`, which would let `/var/lib/./x` through.
    let Some(text) = path.to_str() else {
        return false;
    };
    text.len() > 1
        && text.starts_with('/')
        && text[1..]
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn command(argv: &[String]) -> bool {
    argv.first()
        .is_some_and(|program| plain(Path::new(program)))
        && argv.len() <= 16
        && argv.iter().all(|arg| !arg.is_empty() && arg.len() <= 256)
}

impl Plan {
    pub fn check(&self) -> Result<(), String> {
        if !plain(&self.marker) || !plain(&self.journal) {
            return Err("the marker and the journal are absolute, plain paths".into());
        }
        if self.wipe.is_empty() || self.wipe.len() > 64 {
            return Err("a plan wipes 1 to 64 paths".into());
        }
        for path in &self.wipe {
            if !plain(path) || path.components().count() < 4 {
                return Err(format!(
                    "{} is not an absolute plain path two levels below a top directory",
                    path.display()
                ));
            }
            if PROTECTED
                .iter()
                .any(|kept| path.starts_with(kept) || Path::new(kept).starts_with(path))
            {
                return Err(format!(
                    "{} is software or system, never wiped",
                    path.display()
                ));
            }
            if self.journal.starts_with(path) {
                return Err("the journal must not be inside a wiped path".into());
            }
        }
        if !command(&self.stop) || !command(&self.reboot) {
            return Err("stop and reboot are commands with an absolute program".into());
        }
        Ok(())
    }
}

/// Whether a `controls/touch` event asks for the reset now. Only an enabled
/// Controls (not shadow), on this boot's clock, fresh, with a valid release
/// after 5 s or more of the reset button.
pub fn accept(event: &TouchEvent, clock_id: &str, now_us: u64) -> Result<(), &'static str> {
    event.check().map_err(|_| "invalid touch event")?;
    if event.source != TouchSource::Reset
        || event.gesture != TouchGesture::Hold
        || event.when_quiet != TouchIntent::FactoryReset
    {
        return Err("not the reset button held");
    }
    if !event.enabled {
        return Err("controls is in shadow");
    }
    if event.stamp.clock_id != clock_id {
        return Err("another boot's clock");
    }
    let age = now_us
        .checked_sub(event.stamp.at_us)
        .ok_or("from the future")?;
    if age >= TOUCH_EVENT_TTL_US {
        return Err("stale");
    }
    Ok(())
}

/// What the reset does outside the filesystem. Injected for tests.
pub trait Effects {
    fn run(&mut self, argv: &[String]) -> io::Result<()>;
    fn sync(&mut self);
}

pub struct System;

impl Effects for System {
    fn run(&mut self, argv: &[String]) -> io::Result<()> {
        let status = Command::new(&argv[0]).args(&argv[1..]).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!("{} ended with {status}", argv[0])))
        }
    }
    fn sync(&mut self) {
        rustix::fs::sync();
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "step", content = "path")]
pub enum Step {
    Journal,
    Stopped,
    Wiped(PathBuf),
    MarkerRemoved,
    JournalRemoved,
    Rebooting,
}

fn sync_parent(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(parent) => File::open(parent)?.sync_all(),
        None => Ok(()),
    }
}

fn write_journal(path: &Path) -> io::Result<()> {
    let temporary = path.with_extension("tmp");
    let _ = fs::remove_file(&temporary);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(b"{\"reset\":\"wiping\",\"version\":1}\n")?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    sync_parent(path)
}

fn remove_if_present(path: &Path) -> io::Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => {
            sync_parent(path)?;
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Remove a file, or everything inside a directory. Links are removed, never
/// followed, at the top and inside (std's `remove_dir_all` does not follow).
pub fn wipe(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !metadata.is_dir() {
        fs::remove_file(path)?;
        return sync_parent(path);
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    File::open(path)?.sync_all()
}

/// Begin a reset: journal, stop the lamp, then finish it and reboot. If the
/// stop fails the journal stays and nothing is wiped; the next start finishes.
pub fn reset(plan: &Plan, effects: &mut impl Effects) -> io::Result<Vec<Step>> {
    plan.check().map_err(io::Error::other)?;
    let mut steps = Vec::new();
    write_journal(&plan.journal)?;
    steps.push(Step::Journal);
    effects.run(&plan.stop)?;
    steps.push(Step::Stopped);
    finish(plan, effects, true, &mut steps)?;
    Ok(steps)
}

/// At start, before anything else: finish a reset a power cut interrupted.
/// Nothing of the lamp is running yet, so nothing is stopped and there is no
/// second reboot. Returns the steps taken, empty when there was no reset.
pub fn resume(plan: &Plan, effects: &mut impl Effects) -> io::Result<Vec<Step>> {
    plan.check().map_err(io::Error::other)?;
    let mut steps = Vec::new();
    match fs::symlink_metadata(&plan.journal) {
        Ok(_) => finish(plan, effects, false, &mut steps)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(steps)
}

fn finish(
    plan: &Plan,
    effects: &mut impl Effects,
    reboot: bool,
    steps: &mut Vec<Step>,
) -> io::Result<()> {
    for path in &plan.wipe {
        wipe(path)?;
        steps.push(Step::Wiped(path.clone()));
    }
    if remove_if_present(&plan.marker)? {
        steps.push(Step::MarkerRemoved);
    }
    effects.sync();
    remove_if_present(&plan.journal)?;
    steps.push(Step::JournalRemoved);
    effects.sync();
    if reboot {
        steps.push(Step::Rebooting);
        effects.run(&plan.reboot)?;
    }
    Ok(())
}
