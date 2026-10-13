//! The decisions log: one line of JSON for every decision, in a file for
//! each day.
//!
//! This is what the policy is tuned from. A line holds what was known,
//! what the decision model answered and what was chosen, so that the
//! thresholds and the weights can be fitted again and, later, a model of
//! the lamp's own trained.
//!
//! A line holds what was heard, in words. That is deliberate, since a
//! judgement cannot be fitted again without what it was a judgement of,
//! and it is why the directory is the owner's alone: made with mode 0700,
//! refused if anyone else can enter it, its files 0600.
use serde::Serialize;
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::{DirBuilderExt, FileExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

/// Make the directory, private, or refuse one that others can enter.
pub fn private_dir(dir: &Path) -> io::Result<()> {
    match fs::metadata(dir) {
        Ok(found) if !found.is_dir() => Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            format!("{} is not a directory", dir.display()),
        )),
        Ok(found) if found.permissions().mode() & 0o077 != 0 => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} can be entered by its group or by everyone (mode {:03o}); the decisions log holds what was heard and is kept only where its owner alone can read: chmod 700 it",
                dir.display(),
                found.permissions().mode() & 0o777
            ),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            DirBuilder::new().recursive(true).mode(0o700).create(dir)
        }
        Err(error) => Err(error),
    }
}

/// The day a moment falls on by UTC, as `2026-10-14`.
pub fn day(unix_ms: u64) -> String {
    // Days since 1970 to a date of the civil calendar, by the usual
    // arithmetic on eras of 400 years that begin on 1 March.
    let days = (unix_ms / 86_400_000) as i64 + 719_468;
    let era = days / 146_097;
    let of_era = days % 146_097;
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * of_year + 2) / 153;
    let day = of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The log, written a line at a time.
pub struct Record {
    dir: PathBuf,
    /// The day's file, while it is open.
    open: Option<(String, File)>,
}

impl Record {
    /// A log under `dir`. Nothing is opened until there is a line.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            open: None,
        }
    }

    /// The file a line written at `unix_ms` goes to.
    pub fn file(&self, unix_ms: u64) -> PathBuf {
        self.dir.join(format!("decisions-{}.jsonl", day(unix_ms)))
    }

    /// Append `line` to the file of the day `unix_ms` falls on, with one
    /// write: it is there whole or not at all, unless the disk fills in
    /// the middle of it.
    ///
    /// A failure is the caller's to count. The file is then let go, so
    /// that the next line opens it afresh and finds what the failure left.
    pub fn write<T: Serialize>(&mut self, unix_ms: u64, line: &T) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(line)?;
        bytes.push(b'\n');
        let written = self
            .day_file(unix_ms)
            .and_then(|file| file.write_all(&bytes));
        if written.is_err() {
            self.open = None;
        }
        written
    }

    fn day_file(&mut self, unix_ms: u64) -> io::Result<&mut File> {
        let day = day(unix_ms);
        if self.open.as_ref().is_none_or(|(open, _)| *open != day) {
            let path = self.dir.join(format!("decisions-{day}.jsonl"));
            let open = || {
                OpenOptions::new()
                    .read(true)
                    .append(true)
                    .create(true)
                    .mode(0o600)
                    .open(&path)
            };
            let mut file = match open() {
                // The directory was removed under the running program.
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    private_dir(&self.dir)?;
                    open()?
                }
                opened => opened?,
            };
            // A write that failed, here or in a process before this one,
            // may have left part of a line. It is ended, so that the next
            // line stands by itself: a reader passes over a line that is
            // not JSON.
            let length = file.metadata()?.len();
            let mut last = [b'\n'];
            if length > 0 {
                file.read_exact_at(&mut last, length - 1)?;
            }
            if last != [b'\n'] {
                file.write_all(b"\n")?;
            }
            self.open = Some((day, file));
        }
        match self.open.as_mut() {
            Some((_, file)) => Ok(file),
            None => Err(io::Error::other("the day's file is not open")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lamp-policy-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    fn lines(path: &Path) -> Vec<String> {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn a_moment_is_filed_under_its_day_by_utc() {
        assert_eq!(day(0), "1970-01-01");
        assert_eq!(day(86_399_999), "1970-01-01");
        assert_eq!(day(86_400_000), "1970-01-02");
        // 2026-10-13 23:59:59.999 and the millisecond after.
        assert_eq!(day(1_791_935_999_999), "2026-10-13");
        assert_eq!(day(1_791_936_000_000), "2026-10-14");
        // A leap day, the day after it, and a century that has none.
        assert_eq!(day(1_709_164_800_000), "2024-02-29");
        assert_eq!(day(1_709_251_200_000), "2024-03-01");
        assert_eq!(day(951_782_400_000), "2000-02-29");
        assert_eq!(day(4_107_456_000_000), "2100-02-28");
        assert_eq!(day(4_107_542_400_000), "2100-03-01");
        assert_eq!(day(1_798_761_599_999), "2026-12-31");
        assert_eq!(day(1_798_761_600_000), "2027-01-01");
    }

    #[test]
    fn the_directory_is_made_private_and_one_others_can_enter_is_refused() {
        let dir = dir("private");
        private_dir(&dir.join("made/below")).unwrap();
        assert_eq!(mode(&dir.join("made/below")), 0o700);
        // One that is there and private is taken as it is.
        private_dir(&dir.join("made/below")).unwrap();
        for open in [0o750, 0o705, 0o777] {
            fs::set_permissions(dir.join("made"), fs::Permissions::from_mode(open)).unwrap();
            let error = private_dir(&dir.join("made")).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            assert!(error.to_string().contains("chmod 700"), "{error}");
        }
        fs::write(dir.join("file"), b"").unwrap();
        let error = private_dir(&dir.join("file")).unwrap_err();
        assert!(error.to_string().contains("is not a directory"), "{error}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn each_line_goes_to_its_days_file_which_only_its_owner_can_read() {
        let dir = dir("days");
        private_dir(&dir).unwrap();
        let mut record = Record::new(&dir);
        let evening = 1_791_935_999_000;
        record.write(evening, &json!({"n": 1})).unwrap();
        record
            .write(evening + 500, &json!({"n": 2, "heard": "Lamp ơi"}))
            .unwrap();
        record.write(evening + 1_000, &json!({"n": 3})).unwrap();
        let first = dir.join("decisions-2026-10-13.jsonl");
        let second = dir.join("decisions-2026-10-14.jsonl");
        assert_eq!(
            (record.file(evening), record.file(evening + 1_000)),
            (first.clone(), second.clone())
        );
        assert_eq!(
            lines(&first),
            ["{\"n\":1}", "{\"heard\":\"Lamp ơi\",\"n\":2}"]
        );
        assert_eq!(lines(&second), ["{\"n\":3}"]);
        assert_eq!((mode(&first), mode(&second)), (0o600, 0o600));
        // A program started later the same day adds to the day's file.
        let mut later = Record::new(&dir);
        later.write(evening + 2_000, &json!({"n": 4})).unwrap();
        assert_eq!(lines(&second), ["{\"n\":3}", "{\"n\":4}"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_write_that_fails_is_told_and_the_next_line_is_whole() {
        let dir = dir("fails");
        private_dir(&dir).unwrap();
        let mut record = Record::new(&dir);
        let now = 1_791_936_000_000;
        record.write(now, &json!({"n": 1})).unwrap();
        // The day's file cannot be opened: something else has its name.
        let tomorrow = now + 86_400_000;
        fs::create_dir(record.file(tomorrow)).unwrap();
        assert!(record.write(tomorrow, &json!({"n": 2})).is_err());
        assert!(record.write(tomorrow, &json!({"n": 3})).is_err());
        fs::remove_dir(record.file(tomorrow)).unwrap();
        // Whatever the failure left, the line after it stands by itself.
        fs::write(record.file(tomorrow), b"{\"n\":2,\"hea").unwrap();
        record.write(tomorrow, &json!({"n": 4})).unwrap();
        record.write(tomorrow, &json!({"n": 5})).unwrap();
        let read: Vec<Value> = lines(&record.file(tomorrow))
            .iter()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        assert_eq!(read, [json!({"n": 4}), json!({"n": 5})]);
        assert_eq!(lines(&record.file(now)), ["{\"n\":1}"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_directory_removed_under_the_program_is_made_again_private() {
        let dir = dir("removed");
        private_dir(&dir).unwrap();
        let mut record = Record::new(&dir);
        let now = 1_791_936_000_000;
        record.write(now, &json!({"n": 1})).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        // The open file is still written to, though nobody can find it.
        record.write(now, &json!({"n": 2})).unwrap();
        // The next day's file needs the directory.
        record.write(now + 86_400_000, &json!({"n": 3})).unwrap();
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(lines(&record.file(now + 86_400_000)), ["{\"n\":3}"]);
        fs::remove_dir_all(dir).unwrap();
    }
}
