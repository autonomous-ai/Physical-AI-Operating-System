//! The voice's record of whom it answered: for each request, the first
//! output the busy-room rule judged ([`crate::attention`]), what the rule saw
//! and what it decided, one JSON line each, for fitting and checking the
//! Policy's judges (#700; the contract proposed in
//! `docs/verification/local-judge-fit-20261014/voice-first-output.md`).
//!
//! Off unless set: `LAMP_FIRST_OUTPUT=/absolute/private/directory` in the
//! voice's unit, passed to the provider worker as `--first-output-log DIR`.
//! Each worker run writes its own file there. Writing never holds the
//! voice: a line goes to a bounded queue and a thread of its own writes it;
//! a full queue, a file at its limit or a failed write is counted, and the
//! counts are the file's last line. What was heard is in these lines, so the
//! directory is private to the voice and nothing here is published.
//!
//! Not recorded yet, from the contract: the request's outcome (whether any
//! sound was played), its start, and the identities of the transcript, name
//! and eyes messages the rule used.
use serde::Serialize;
use serde_json::json;
use std::{
    fs::OpenOptions,
    io::{self, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread::JoinHandle,
};

/// Where the coordinator reads the directory, if any.
pub const ENVIRONMENT: &str = "LAMP_FIRST_OUTPUT";
/// The provider worker's flag for it.
pub const FLAG: &str = "--first-output-log";
/// Lines waiting to be written at most; more are counted as dropped.
const QUEUE: usize = 64;
/// One run's file at most; lines past it are counted as dropped.
const MOST_BYTES: u64 = 8 * 1024 * 1024;

/// What the rule saw, in the seven fields the Policy's fitting rows take as
/// their `baseline` (`crates/policy`, `fit::Row`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Baseline {
    pub kind: &'static str,
    pub room: &'static str,
    pub own_turn: bool,
    pub going_on: bool,
    pub named_sure: bool,
    pub text: String,
    pub facing: Option<bool>,
}

/// One decision of the rule, with what is known around it. Times are this
/// process's monotonic clock, in microseconds.
#[derive(Clone, Debug)]
pub struct Decision {
    pub session: u64,
    pub request: u64,
    /// What came first: `audio`, `output_transcript` or `tool_call`.
    pub trigger: &'static str,
    /// The rule's busy form was in force (`--attention-busy`).
    pub strict: bool,
    pub baseline: Baseline,
    pub play: bool,
    pub reason: &'static str,
    /// When the room, the conversation and the eyes began and finished
    /// being read for it, and when the rule decided.
    pub sampled_start_us: u64,
    pub sampled_end_us: u64,
    pub evaluated_us: u64,
    /// When the request's sound began and, if known, ended.
    pub input_began_us: Option<u64>,
    pub input_ended_us: Option<u64>,
}

#[derive(Default)]
struct Counts {
    admitted: AtomicU64,
    written: AtomicU64,
    dropped: AtomicU64,
    failed: AtomicU64,
}

pub struct FirstOutputLog {
    lines: Option<SyncSender<String>>,
    writer: Option<JoinHandle<()>>,
    counts: Arc<Counts>,
    path: PathBuf,
    run: String,
    machine: Option<String>,
    boot: Option<String>,
    build: Option<String>,
    seq: u64,
    began_unix_ms: u64,
}

impl FirstOutputLog {
    /// A new file for this run in `directory`, which must exist, be a
    /// directory and be open to its owner only.
    pub fn open(directory: &Path) -> io::Result<Self> {
        let metadata = std::fs::metadata(directory)?;
        if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the first-output directory must be a directory open to its owner only",
            ));
        }
        let began_unix_ms = unix_ms();
        let run = format!("{began_unix_ms}-{}", std::process::id());
        let path = directory.join(format!("first-output-{run}.jsonl"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        let counts = Arc::new(Counts::default());
        let (lines, queued) = sync_channel(QUEUE);
        let writer = {
            let counts = Arc::clone(&counts);
            std::thread::Builder::new()
                .name("first-output".into())
                .spawn(move || write_lines(file, &queued, &counts))?
        };
        Ok(Self {
            lines: Some(lines),
            writer: Some(writer),
            counts,
            path,
            run,
            machine: read_trimmed("/etc/machine-id"),
            boot: read_trimmed("/proc/sys/kernel/random/boot_id"),
            build: build_sha256(),
            seq: 0,
            began_unix_ms,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Record one decision. Never waits.
    pub fn decided(&mut self, decision: &Decision) {
        self.seq += 1;
        let line = json!({
            "schema": 1,
            "event": "voice_first_output",
            "machine": self.machine,
            "os_boot": self.boot,
            "voice_run": self.run,
            "build_sha256": self.build,
            "event_seq": self.seq,
            "session": decision.session,
            "request": decision.request,
            "trigger": decision.trigger,
            "attention": {"enabled": true, "strict": decision.strict},
            "baseline": decision.baseline,
            "permission": if decision.play { "play" } else { "hold" },
            "reason": decision.reason,
            "sampled_start_us": decision.sampled_start_us,
            "sampled_end_us": decision.sampled_end_us,
            "evaluated_us": decision.evaluated_us,
            "input_began_us": decision.input_began_us,
            "input_ended_us": decision.input_ended_us,
            "at_unix_ms": unix_ms(),
        });
        self.send(line.to_string());
    }

    fn send(&self, line: String) {
        let Some(lines) = &self.lines else { return };
        match lines.try_send(line) {
            Ok(()) => {
                self.counts.admitted.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.counts.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

impl Drop for FirstOutputLog {
    /// The run's last line: how long it ran and what became of its lines.
    fn drop(&mut self) {
        let Some(lines) = self.lines.take() else {
            return;
        };
        let end = json!({
            "schema": 1,
            "event": "capture_end",
            "voice_run": self.run,
            "began_unix_ms": self.began_unix_ms,
            "at_unix_ms": unix_ms(),
            "decisions": self.seq,
            "admitted": self.counts.admitted.load(Ordering::Relaxed),
            "dropped": self.counts.dropped.load(Ordering::Relaxed),
            "limits": {"queue": QUEUE, "bytes": MOST_BYTES},
        });
        // The end goes in whatever the queue holds; a full queue loses it,
        // and a file without one is a run that did not end cleanly.
        let _ = lines.try_send(end.to_string());
        drop(lines);
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

fn write_lines(mut file: std::fs::File, queued: &Receiver<String>, counts: &Counts) {
    let mut written_bytes = 0u64;
    for line in queued {
        let length = line.len() as u64 + 1;
        if written_bytes + length > MOST_BYTES {
            counts.dropped.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        match file
            .write_all(line.as_bytes())
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.flush())
        {
            Ok(()) => {
                written_bytes += length;
                counts.written.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => {
                counts.failed.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// The provider worker's arguments for the setting in the environment.
pub fn arguments_from_environment() -> Vec<String> {
    arguments(std::env::var(ENVIRONMENT).ok().as_deref())
}

fn arguments(value: Option<&str>) -> Vec<String> {
    match value {
        Some(directory) if Path::new(directory).is_absolute() => {
            vec![FLAG.to_owned(), directory.to_owned()]
        }
        _ => Vec::new(),
    }
}

/// Take a leading `--first-output-log DIR` off the provider worker's flags.
pub fn take(flags: &[String]) -> io::Result<(Option<PathBuf>, &[String])> {
    match flags {
        [flag, directory, rest @ ..] if flag == FLAG => {
            if !Path::new(directory).is_absolute() {
                return Err(io::Error::other("--first-output-log must be absolute"));
            }
            Ok((Some(PathBuf::from(directory)), rest))
        }
        [flag] if flag == FLAG => Err(io::Error::other("--first-output-log needs a directory")),
        _ => Ok((None, flags)),
    }
}

fn read_trimmed(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// The running program's SHA-256, which names its build exactly.
fn build_sha256() -> Option<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(std::env::current_exe().ok()?).ok()?;
    Some(format!("{:x}", Sha256::digest(&bytes)))
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(request: u64, play: bool) -> Decision {
        Decision {
            session: 7,
            request,
            trigger: "audio",
            strict: true,
            baseline: Baseline {
                kind: "voice_first_output",
                room: "busy",
                own_turn: false,
                going_on: false,
                named_sure: false,
                text: "And tomorrow?".into(),
                facing: None,
            },
            play,
            reason: "a reason",
            sampled_start_us: 10,
            sampled_end_us: 12,
            evaluated_us: 13,
            input_began_us: Some(1),
            input_ended_us: None,
        }
    }

    fn private_directory(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("first-output-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        directory
    }

    #[test]
    fn each_decision_is_one_line_with_the_seven_fields_and_the_run_ends_with_its_counts() {
        let directory = private_directory("lines");
        let path = {
            let mut log = FirstOutputLog::open(&directory).unwrap();
            log.decided(&decision(3, false));
            log.decided(&decision(4, true));
            log.path().to_owned()
        };
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let lines: Vec<serde_json::Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["event"], "voice_first_output");
        assert_eq!(lines[0]["permission"], "hold");
        assert_eq!(lines[1]["permission"], "play");
        assert_eq!(lines[1]["event_seq"], 2);
        let baseline = lines[0]["baseline"].as_object().unwrap();
        assert_eq!(baseline.len(), 7);
        assert_eq!(baseline["kind"], "voice_first_output");
        assert_eq!(baseline["facing"], serde_json::Value::Null);
        assert_eq!(baseline["text"], "And tomorrow?");
        assert_eq!(lines[0]["input_ended_us"], serde_json::Value::Null);
        assert_eq!(lines[2]["event"], "capture_end");
        assert_eq!(lines[2]["decisions"], 2);
        assert_eq!(lines[2]["dropped"], 0);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_directory_others_can_enter_is_refused_and_the_switch_is_an_absolute_directory() {
        let directory = private_directory("open");
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(FirstOutputLog::open(&directory).is_err());
        std::fs::remove_dir_all(directory).unwrap();
        assert_eq!(
            arguments(Some("/var/lib/x")),
            [FLAG.to_owned(), "/var/lib/x".to_owned()]
        );
        assert!(arguments(Some("relative")).is_empty() && arguments(None).is_empty());
        let flags = vec![FLAG.to_owned(), "/d".to_owned(), "--other".to_owned()];
        let (taken, rest) = take(&flags).unwrap();
        assert_eq!(taken, Some(PathBuf::from("/d")));
        assert_eq!(rest, &flags[2..]);
        assert!(take(&[FLAG.to_owned()]).is_err());
        assert!(take(&[FLAG.to_owned(), "d".to_owned()]).is_err());
    }

    #[test]
    fn a_full_queue_is_counted_and_never_waited_on() {
        let directory = private_directory("full");
        let mut log = FirstOutputLog::open(&directory).unwrap();
        for request in 0..10_000 {
            log.decided(&decision(request, true));
        }
        let admitted = log.counts.admitted.load(Ordering::Relaxed);
        let dropped = log.counts.dropped.load(Ordering::Relaxed);
        assert_eq!(admitted + dropped, 10_000);
        drop(log);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
