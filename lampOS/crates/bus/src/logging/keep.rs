//! The shared log kept on disk for a next-morning look (`lamp-bus
//! keep-logs DIR`, #699): one JSON line per log, in files of a bounded size
//! and a bounded number, so it can never fill the card.
//!
//! - Files are `DIR/logs.NNNNNNNN.jsonl`. Each start opens the next number,
//!   so a restart is a new file. A file is closed before a line would take
//!   it past its size, and only the newest files are kept. Nothing else in
//!   `DIR` is ever touched, and `DIR` must already exist: who owns it and
//!   who may read it is the image's to decide.
//! - Loss is said, not hidden: each line carries the bus's gap and restart
//!   for its publisher, and lines that could not be written are counted in
//!   the next one that is (`unwritten_before`).
//! - Written data is synced at most once every [`SYNC_EVERY`] and at each new
//!   file, so a power cut loses about that much at most. Nobody waits on it
//!   but the keeper: publishers are isolated by the bus.
use super::Log;
use crate::Message;
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Written data is made durable this often at most.
pub const SYNC_EVERY: Duration = Duration::from_secs(1);
const PREFIX: &str = "logs.";
const SUFFIX: &str = ".jsonl";

/// One log as it is kept: the log itself and its bus identity.
#[derive(Serialize)]
struct Line<'a> {
    at_unix_ms: u64,
    level: &'a str,
    part: &'a str,
    code: &'a str,
    message: &'a str,
    attempt: u64,
    boot: &'a str,
    bus_seq: u64,
    bus_gap: u64,
    restarted: bool,
    #[serde(skip_serializing_if = "is_zero")]
    unwritten_before: u64,
}

fn is_zero(count: &u64) -> bool {
    *count == 0
}

/// The files the log is kept in, and what could not be written.
pub struct Kept {
    folder: PathBuf,
    file_bytes: u64,
    files: usize,
    number: u64,
    file: File,
    written: u64,
    synced_at: Option<Instant>,
    unsynced: bool,
    /// Lines that could not be written since the last that was.
    unwritten: u64,
    /// Lines that could not be written, in all.
    pub lost: u64,
    /// Lines written, in all.
    pub lines: u64,
}

impl Kept {
    /// Keep the log in `folder`, files of at most `file_bytes`, the newest
    /// `files` of them. Opens a new file numbered after any already there.
    pub fn open(folder: &Path, file_bytes: u64, files: usize) -> io::Result<Self> {
        if !fs::metadata(folder)?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a folder", folder.display()),
            ));
        }
        let number = numbers(folder)?.last().map_or(1, |last| last + 1);
        let file = create(folder, number)?;
        let mut kept = Self {
            folder: folder.to_owned(),
            file_bytes,
            files: files.max(1),
            number,
            file,
            written: 0,
            synced_at: None,
            unsynced: false,
            unwritten: 0,
            lost: 0,
            lines: 0,
        };
        kept.forget_old()?;
        Ok(kept)
    }

    /// Keep one log heard on the bus. A line that cannot be written is
    /// counted, said in the next one that is, and given back as the error.
    pub fn keep(&mut self, log: &Log, message: &Message, now: Instant) -> io::Result<()> {
        let mut text = serde_json::to_vec(&Line {
            at_unix_ms: log.created_unix_ms,
            level: log.level.as_str(),
            part: &log.part,
            code: &log.code,
            message: &log.message,
            attempt: log.attempt,
            boot: &message.header.boot,
            bus_seq: message.header.seq,
            bus_gap: message.gap,
            restarted: message.publisher_restarted,
            unwritten_before: self.unwritten,
        })
        .map_err(io::Error::other)?;
        text.push(b'\n');
        match self.write(&text, now) {
            Ok(()) => {
                self.unwritten = 0;
                self.lines += 1;
                Ok(())
            }
            Err(error) => {
                self.unwritten += 1;
                self.lost += 1;
                Err(error)
            }
        }
    }

    fn write(&mut self, text: &[u8], now: Instant) -> io::Result<()> {
        if self.written > 0 && self.written + text.len() as u64 > self.file_bytes {
            self.next_file()?;
        }
        self.file.write_all(text)?;
        self.written += text.len() as u64;
        self.unsynced = true;
        self.sync_if_due(now)
    }

    /// Make what was written durable, if it has not been for [`SYNC_EVERY`].
    /// Call from the keeper's loop when nothing comes, too.
    pub fn sync_if_due(&mut self, now: Instant) -> io::Result<()> {
        if self.unsynced
            && self
                .synced_at
                .is_none_or(|at| now.saturating_duration_since(at) >= SYNC_EVERY)
        {
            self.file.sync_data()?;
            self.synced_at = Some(now);
            self.unsynced = false;
        }
        Ok(())
    }

    /// Make everything written durable now: at a new file, and at the end.
    pub fn sync(&mut self) -> io::Result<()> {
        if self.unsynced {
            self.file.sync_data()?;
            self.unsynced = false;
        }
        Ok(())
    }

    fn next_file(&mut self) -> io::Result<()> {
        self.sync()?;
        let number = self.number + 1;
        self.file = create(&self.folder, number)?;
        self.number = number;
        self.written = 0;
        self.forget_old()
    }

    /// Only the newest files are kept, this one among them.
    fn forget_old(&mut self) -> io::Result<()> {
        let numbers = numbers(&self.folder)?;
        let surplus = numbers.len().saturating_sub(self.files);
        for number in &numbers[..surplus] {
            if *number != self.number {
                fs::remove_file(self.folder.join(name(*number)))?;
            }
        }
        Ok(())
    }

    /// The file being written now.
    pub fn path(&self) -> PathBuf {
        self.folder.join(name(self.number))
    }
}

fn name(number: u64) -> String {
    format!("{PREFIX}{number:08}{SUFFIX}")
}

/// The numbers of the keeper's own files in `folder`, lowest first.
fn numbers(folder: &Path) -> io::Result<Vec<u64>> {
    let mut numbers = Vec::new();
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(digits) = name
            .strip_prefix(PREFIX)
            .and_then(|rest| rest.strip_suffix(SUFFIX))
        else {
            continue;
        };
        if digits.len() == 8
            && digits.bytes().all(|byte| byte.is_ascii_digit())
            && entry.file_type()?.is_file()
        {
            numbers.push(digits.parse().expect("eight digits"));
        }
    }
    numbers.sort_unstable();
    Ok(numbers)
}

fn create(folder: &Path, number: u64) -> io::Result<File> {
    // A new file each time, readable by the lamp's group as the bus is.
    OpenOptions::new()
        .append(true)
        .create_new(true)
        .mode(0o640)
        .open(folder.join(name(number)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Header, Kind, logging::LogLevel};

    struct Folder(PathBuf);
    impl Folder {
        fn new(name: &str) -> Self {
            let folder =
                std::env::temp_dir().join(format!("lamp-keep-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&folder);
            fs::create_dir_all(&folder).unwrap();
            Self(folder)
        }
        fn kept(&self) -> Vec<String> {
            let mut names: Vec<_> = fs::read_dir(&self.0)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect();
            names.sort();
            names
        }
    }
    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn heard(seq: u64, gap: u64) -> (Log, Message) {
        let log = Log {
            part: "body".into(),
            level: LogLevel::Warn,
            code: "gave_way".into(),
            message: "a hand on the head".into(),
            attempt: seq,
            created_unix_ms: 1_792_115_940_000 + seq,
        };
        let message = Message {
            header: Header {
                topic: Log::TOPIC.into(),
                kind: Kind::Event,
                enc: crate::JSON.into(),
                type_name: Some(Log::TYPE.into()),
                source: "body".into(),
                boot: "0123456789abcdef".into(),
                seq,
                at_us: seq,
                at_unix_ms: 1_792_115_940_000 + seq,
                expires_ms: None,
            },
            payload: Vec::new(),
            gap,
            publisher_restarted: false,
            expired: false,
        };
        (log, message)
    }

    #[test]
    fn each_log_is_one_json_line_with_its_bus_identity() {
        let folder = Folder::new("line");
        let mut kept = Kept::open(&folder.0, 64 * 1024, 4).unwrap();
        let now = Instant::now();
        let (log, message) = heard(7, 2);
        kept.keep(&log, &message, now).unwrap();
        kept.sync().unwrap();
        let text = fs::read_to_string(kept.path()).unwrap();
        let line: serde_json::Value = serde_json::from_str(text.trim_end()).unwrap();
        assert_eq!(
            line,
            serde_json::json!({"at_unix_ms":1_792_115_940_007_u64,"level":"warn","part":"body",
                "code":"gave_way","message":"a hand on the head","attempt":7,
                "boot":"0123456789abcdef","bus_seq":7,"bus_gap":2,"restarted":false})
        );
        assert_eq!(text.lines().count(), 1);
        assert_eq!(folder.kept(), ["logs.00000001.jsonl"]);
    }

    #[test]
    fn files_stay_under_their_size_and_only_the_newest_are_kept() {
        let folder = Folder::new("rotate");
        fs::write(folder.0.join("notes.txt"), "not the keeper's").unwrap();
        fs::write(folder.0.join("logs.1.jsonl"), "not eight digits").unwrap();
        let (log, message) = heard(1, 0);
        let mut kept = Kept::open(&folder.0, 1_000, 3).unwrap();
        let now = Instant::now();
        for _ in 0..40 {
            kept.keep(&log, &message, now).unwrap();
        }
        kept.sync().unwrap();
        let names = folder.kept();
        let logs: Vec<_> = names
            .iter()
            .filter(|name| name.len() == "logs.00000001.jsonl".len())
            .collect();
        assert_eq!(logs.len(), 3, "{names:?}");
        assert!(names.contains(&"notes.txt".to_owned()));
        assert!(names.contains(&"logs.1.jsonl".to_owned()));
        for name in logs {
            let size = fs::metadata(folder.0.join(name)).unwrap().len();
            assert!(size <= 1_000, "{name}: {size}");
        }
        assert_eq!(kept.lines, 40);
        // A start after it opens the next number and keeps three still.
        let last = kept.number;
        drop(kept);
        let again = Kept::open(&folder.0, 1_000, 3).unwrap();
        assert_eq!(again.number, last + 1);
        assert_eq!(
            folder
                .kept()
                .iter()
                .filter(|name| name.starts_with("logs.0"))
                .count(),
            3
        );
    }

    #[test]
    fn a_line_that_cannot_be_written_is_counted_and_said_in_the_next() {
        let folder = Folder::new("unwritten");
        let mut kept = Kept::open(&folder.0, 64 * 1024, 4).unwrap();
        let now = Instant::now();
        let (log, message) = heard(1, 0);
        // The file is replaced by one opened read-only: writes fail.
        let path = kept.path();
        kept.file = File::open(&path).unwrap();
        assert!(kept.keep(&log, &message, now).is_err());
        assert!(kept.keep(&log, &message, now).is_err());
        assert_eq!(kept.lost, 2);
        kept.file = OpenOptions::new().append(true).open(&path).unwrap();
        kept.keep(&log, &message, now).unwrap();
        kept.sync().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"unwritten_before\":2"), "{text}");
        kept.keep(&log, &message, now).unwrap();
        kept.sync().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("unwritten_before").count(), 1);
    }

    #[test]
    fn written_lines_are_synced_at_most_once_a_second() {
        let folder = Folder::new("sync");
        let mut kept = Kept::open(&folder.0, 64 * 1024, 4).unwrap();
        let start = Instant::now();
        let (log, message) = heard(1, 0);
        kept.keep(&log, &message, start).unwrap();
        assert_eq!(kept.synced_at, Some(start));
        kept.keep(&log, &message, start + Duration::from_millis(500))
            .unwrap();
        assert!(kept.unsynced, "not again within a second");
        kept.sync_if_due(start + SYNC_EVERY).unwrap();
        assert!(!kept.unsynced);
        assert_eq!(kept.synced_at, Some(start + SYNC_EVERY));
    }

    #[test]
    fn the_folder_must_already_be_there() {
        assert!(Kept::open(Path::new("/nonexistent/lamp-logs"), 1_000, 2).is_err());
        let folder = Folder::new("file");
        let file = folder.0.join("a-file");
        fs::write(&file, "").unwrap();
        assert!(Kept::open(&file, 1_000, 2).is_err());
    }
}
