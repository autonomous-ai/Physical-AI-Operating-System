//! The storage fixtures that start a child process, in their own test binary
//! (#887). A fork copies every open descriptor, and `flock` belongs to the open
//! file description, so until its `exec` a child holds the lock of any `Store`
//! another test thread has open. A test that then drops and reopens its store
//! sees it as owned. Here no such test runs beside them, and `ONE_AT_A_TIME`
//! keeps the two from overlapping each other.
use lamp_schedule::store::{MAX_STATE_BYTES, Store};
use rusqlite::{Connection, params};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{self, Command},
    sync::{Mutex, MutexGuard},
};

const DATABASE: &str = "schedule.sqlite3";

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn one_at_a_time() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Scratch {
    _root: tempfile::TempDir,
    dir: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("private");
        Self { _root: root, dir }
    }

    fn open(&self) -> Store {
        Store::open(&self.dir).unwrap()
    }

    fn populated(&self) {
        self.open().commit(b"previous committed snapshot").unwrap();
    }
}

#[test]
fn exclusive_ownership_blocks_another_process_and_is_released_on_drop() {
    const CHILD: &str = "LAMP_SCHEDULE_STORAGE_LOCK_CHILD";
    if let Some(path) = std::env::var_os(CHILD) {
        assert_eq!(
            Store::open(Path::new(&path)).err().unwrap().kind(),
            io::ErrorKind::WouldBlock
        );
        process::exit(73);
    }
    let _turn = one_at_a_time();
    let scratch = Scratch::new();
    let mut store = scratch.open();
    store.commit(b"owned").unwrap();
    assert_eq!(
        Store::open(&scratch.dir).err().unwrap().kind(),
        io::ErrorKind::WouldBlock
    );
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "exclusive_ownership_blocks_another_process_and_is_released_on_drop",
        ])
        .env(CHILD, &scratch.dir)
        .output()
        .unwrap();
    assert_eq!(child.status.code(), Some(73), "{child:?}");
    drop(store);
    assert_eq!(
        scratch.open().load().unwrap().as_deref(),
        Some(b"owned".as_slice())
    );
}

#[test]
fn interrupted_transaction_recovers_the_last_committed_snapshot() {
    const CHILD: &str = "LAMP_SCHEDULE_STORAGE_CRASH_CHILD";
    if let Some(path) = std::env::var_os(CHILD) {
        let db = Connection::open(Path::new(&path).join(DATABASE)).unwrap();
        db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA cache_size=2; PRAGMA cache_spill=ON; BEGIN IMMEDIATE;").unwrap();
        db.execute(
            "UPDATE state SET snapshot=?1",
            params![vec![0x55_u8; MAX_STATE_BYTES]],
        )
        .unwrap();
        // Exit without running Connection/Transaction destructors. The tiny
        // page cache makes the uncommitted update spill to the database file.
        process::exit(91);
    }
    let _turn = one_at_a_time();
    let scratch = Scratch::new();
    scratch.populated();
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "interrupted_transaction_recovers_the_last_committed_snapshot",
        ])
        .env(CHILD, &scratch.dir)
        .output()
        .unwrap();
    assert_eq!(child.status.code(), Some(91), "{child:?}");
    let journal = scratch.dir.join(format!("{DATABASE}-journal"));
    assert!(fs::metadata(&journal).unwrap().len() > 0);
    assert_eq!(
        scratch.open().load().unwrap().as_deref(),
        Some(b"previous committed snapshot".as_slice())
    );
    assert!(!journal.exists());
}
