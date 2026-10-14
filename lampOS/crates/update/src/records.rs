//! What lamp-update keeps, in its own folder (`/var/lib/lampos-update`):
//!
//! - `state.json`: the build the lamp runs and the one before, the builds
//!   refused after a failed switch (so the same build is not tried again),
//!   and the folders it made (the only ones it ever removes).
//! - `journal.json`: a switch under way. Written before any unit is
//!   changed and removed once the switch is over, so a switch cut short by
//!   a stop or a power cut is put back at the next start.
//! - `previous/`: the units as they were before the last switch.
//! - `log.jsonl`: one line for each thing done or refused.
//! - `lock`: one lamp-update at a time.
//!
//! Every file is written whole: beside, synced, renamed, and the folder
//! synced. A state that cannot be read is kept aside and begun again, as
//! V1's updater did.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// The builds remembered as refused.
const MOST_REFUSED: usize = 32;
/// The log is begun again past this size, the old one kept once.
const MOST_LOG_BYTES: u64 = 1024 * 1024;

/// A build, by its sequence and commit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Build {
    pub sequence: u64,
    pub commit: String,
}

/// A build that was switched to and put back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refusal {
    pub sequence: u64,
    pub commit: String,
    pub why: String,
    pub at_unix_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub installed: Option<Build>,
    pub previous: Option<Build>,
    pub refused: Vec<Refusal>,
    /// Build folders this program made, as the lamp names them.
    pub made: Vec<String>,
}

impl State {
    pub fn refusal(&self, sequence: u64) -> Option<&Refusal> {
        self.refused
            .iter()
            .find(|refusal| refusal.sequence == sequence)
    }

    pub fn refuse(&mut self, build: &Build, why: &str) {
        self.refused.push(Refusal {
            sequence: build.sequence,
            commit: build.commit.clone(),
            why: why.to_owned(),
            at_unix_ms: unix_ms(),
        });
        let extra = self.refused.len().saturating_sub(MOST_REFUSED);
        self.refused.drain(..extra);
    }
}

/// One unit of a switch under way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Switching {
    pub unit: String,
    /// It ran before the switch: it is started again on its way back.
    pub was_active: bool,
}

/// A switch under way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    pub build: Build,
    pub units: Vec<Switching>,
}

pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// Writes `bytes` to `path` whole: beside, synced, renamed, folder synced.
pub fn write_whole(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("no file name"))?
        .to_string_lossy();
    let beside = path.with_file_name(format!(".{name}.lamp-update"));
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode)
            .open(&beside)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&beside, path)?;
    sync_folder(path.parent().unwrap_or(Path::new("/")))
}

pub fn sync_folder(folder: &Path) -> io::Result<()> {
    File::open(folder)?.sync_all()
}

/// Syncs the folder a path is in: its new name is then on the disk.
pub fn sync_parent(path: &Path) -> io::Result<()> {
    sync_folder(path.parent().unwrap_or(Path::new("/")))
}

/// The folder of records, made private if it is not there.
pub struct Records {
    folder: PathBuf,
}

/// Held while one lamp-update works; let go when dropped.
pub struct Lock {
    _file: File,
}

impl Records {
    pub fn new(folder: PathBuf) -> Self {
        Self { folder }
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    fn make(&self) -> io::Result<()> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(self.folder.join("previous"))
    }

    /// One lamp-update at a time: a second is told, and does nothing.
    pub fn lock(&self) -> io::Result<Lock> {
        self.make()?;
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(self.folder.join("lock"))?;
        rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive).map_err(
            |_| io::Error::new(io::ErrorKind::WouldBlock, "another lamp-update is at work"),
        )?;
        Ok(Lock { _file: file })
    }

    /// The state. One that cannot be read is moved aside, said, and begun
    /// again: an update must not stop for good because of its own file.
    pub fn state(&self, say: &mut dyn FnMut(&str)) -> io::Result<State> {
        let path = self.folder.join("state.json");
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(state) => Ok(state),
                Err(error) => {
                    let aside = path.with_file_name(format!("state.json.corrupt-{}", unix_ms()));
                    fs::rename(&path, &aside)?;
                    say(&format!(
                        "the state could not be read ({error}): kept as {} and begun again",
                        aside.display()
                    ));
                    Ok(State::default())
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(State::default()),
            Err(error) => Err(error),
        }
    }

    /// The state, read only: for a look that changes nothing.
    pub fn peek_state(&self) -> State {
        fs::read(self.folder.join("state.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save_state(&self, state: &State) -> io::Result<()> {
        self.make()?;
        let bytes = serde_json::to_vec_pretty(state).map_err(io::Error::other)?;
        write_whole(&self.folder.join("state.json"), &bytes, 0o600)
    }

    pub fn journal(&self) -> io::Result<Option<Journal>> {
        match fs::read(self.folder.join("journal.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn save_journal(&self, journal: &Journal) -> io::Result<()> {
        self.make()?;
        let bytes = serde_json::to_vec_pretty(journal).map_err(io::Error::other)?;
        write_whole(&self.folder.join("journal.json"), &bytes, 0o600)
    }

    pub fn clear_journal(&self) -> io::Result<()> {
        match fs::remove_file(self.folder.join("journal.json")) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => sync_folder(&self.folder),
        }
    }

    /// The units as they are before a switch, replacing the ones kept.
    pub fn keep_previous(&self, units: &[(String, String)]) -> io::Result<()> {
        self.make()?;
        let folder = self.folder.join("previous");
        for entry in fs::read_dir(&folder)? {
            fs::remove_file(entry?.path())?;
        }
        for (unit, text) in units {
            write_whole(&folder.join(unit), text.as_bytes(), 0o600)?;
        }
        Ok(())
    }

    pub fn previous(&self, unit: &str) -> io::Result<String> {
        fs::read_to_string(self.folder.join("previous").join(unit))
    }

    /// Every unit kept from before the last switch.
    pub fn previous_units(&self) -> Vec<String> {
        fs::read_dir(self.folder.join("previous"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| fs::read_to_string(entry.path()).ok())
            .collect()
    }

    /// One line in the log: what happened, and a sentence. Best effort: a
    /// full disk must not stop an update going back.
    pub fn log(&self, event: &str, line: &str) {
        let _ = self.make();
        let path = self.folder.join("log.jsonl");
        if fs::metadata(&path).is_ok_and(|found| found.len() > MOST_LOG_BYTES) {
            let _ = fs::rename(&path, self.folder.join("log.jsonl.1"));
        }
        let entry = serde_json::json!({"at_unix_ms": unix_ms(), "event": event, "line": line});
        if let Ok(mut file) = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(&path)
        {
            let _ = writeln!(file, "{entry}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_journal_and_previous_units_are_kept() {
        let folder = tempfile::tempdir().unwrap();
        let records = Records::new(folder.path().join("records"));
        let mut said = Vec::new();
        assert_eq!(
            records
                .state(&mut |line| said.push(line.to_owned()))
                .unwrap(),
            State::default()
        );

        let build = Build {
            sequence: 3,
            commit: "c".repeat(40),
        };
        let mut state = State {
            installed: Some(build.clone()),
            ..State::default()
        };
        for _ in 0..40 {
            state.refuse(&build, "why");
        }
        assert_eq!(state.refused.len(), MOST_REFUSED);
        records.save_state(&state).unwrap();
        assert_eq!(records.state(&mut |_| {}).unwrap(), state);
        assert_eq!(records.peek_state(), state);

        let journal = Journal {
            build,
            units: vec![Switching {
                unit: "lamp-voice.service".into(),
                was_active: true,
            }],
        };
        records.save_journal(&journal).unwrap();
        assert_eq!(records.journal().unwrap(), Some(journal));
        records.clear_journal().unwrap();
        assert_eq!(records.journal().unwrap(), None);

        records
            .keep_previous(&[("lamp-voice.service".into(), "one".into())])
            .unwrap();
        records
            .keep_previous(&[("lamp-body.service".into(), "two".into())])
            .unwrap();
        assert_eq!(records.previous_units(), vec!["two".to_owned()]);
        assert!(records.previous("lamp-voice.service").is_err());

        records.log("checked", "a line");
        let log = fs::read_to_string(records.folder().join("log.jsonl")).unwrap();
        assert!(log.contains("\"event\":\"checked\""));
    }

    #[test]
    fn a_state_that_cannot_be_read_is_kept_aside() {
        let folder = tempfile::tempdir().unwrap();
        let records = Records::new(folder.path().to_owned());
        fs::write(folder.path().join("state.json"), b"{not json").unwrap();
        let mut said = Vec::new();
        assert_eq!(
            records
                .state(&mut |line| said.push(line.to_owned()))
                .unwrap(),
            State::default()
        );
        assert_eq!(said.len(), 1);
        assert!(fs::read_dir(folder.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("state.json.corrupt-")
        }));
    }

    #[test]
    fn one_at_a_time() {
        let folder = tempfile::tempdir().unwrap();
        let records = Records::new(folder.path().to_owned());
        let held = records.lock().unwrap();
        let other = Records::new(folder.path().to_owned());
        assert!(other.lock().is_err());
        drop(held);
        assert!(other.lock().is_ok());
    }
}
