use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        // Keep real Unix socket paths short on macOS as well as Linux.
        let path = fs::canonicalize("/tmp").unwrap().join(format!(
            "rp-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn rejected_roots_and_failed_declarations_do_not_sample_the_cadence_clock() {
    let scratch = Scratch::new();
    let blocked = scratch.0.join("blocked");
    fs::write(&blocked, b"keep this file").unwrap();
    let reads = Arc::new(AtomicUsize::new(0));
    let fixed = Instant::now();
    for root in [Path::new("relative-reporter-clock"), blocked.as_path()] {
        let counted = Arc::clone(&reads);
        assert!(
            Reporter::new_with_cadence_clock(root, move || {
                counted.fetch_add(1, Ordering::SeqCst);
                fixed
            })
            .is_err()
        );
    }
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    assert_eq!(fs::read(blocked).unwrap(), b"keep this file");
}

#[test]
fn declaration_samples_the_clock_before_the_separate_real_header_start() {
    let scratch = Scratch::new();
    let bus = Bus::new(&scratch.0).unwrap();
    let mut subscriber = bus
        .subscribe_typed(Diagnostic::TOPIC, "header", Diagnostic::TYPE)
        .unwrap();
    let root = scratch.0.clone();
    let reads = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&reads);
    let before = Instant::now();
    let future = before + Duration::from_secs(3600);
    let mut reporter = Reporter::new_with_cadence_clock(&scratch.0, move || {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            assert!(root.join("diagnostics/declaration").is_file());
            assert!(!root.join("diagnostics/pubs").exists());
        }
        future
    })
    .unwrap();
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert!(reporter.started >= before && reporter.started <= Instant::now());
    let report = Diagnostic::ok("records");
    reporter.publish(&report).unwrap();
    let after = Instant::now();
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    let message = subscriber.recv(Duration::ZERO).unwrap().unwrap();
    assert!(u128::from(message.header.at_us) <= after.duration_since(before).as_micros());
    assert_eq!(message.header.seq, 1);
    assert_eq!(message.header.type_name.as_deref(), Some(Diagnostic::TYPE));
    let received: Diagnostic = serde_json::from_value(message.json().unwrap()).unwrap();
    assert_eq!(received, report);
}

#[test]
fn the_records_constructor_installs_the_original_scan_and_presence_cadence() {
    let scratch = Scratch::new();
    let bus = Bus::new(&scratch.0).unwrap();
    let mut first = bus
        .subscribe_typed(Diagnostic::TOPIC, "first", Diagnostic::TYPE)
        .unwrap();
    let origin = Instant::now();
    let micros = Arc::new(AtomicU64::new(0));
    let current = Arc::clone(&micros);
    let mut reporter = Reporter::new_with_cadence_clock(&scratch.0, move || {
        origin + Duration::from_micros(current.load(Ordering::SeqCst))
    })
    .unwrap();
    let report = Diagnostic::ok("records");
    reporter.publish(&report).unwrap();
    assert_eq!(first.recv(Duration::ZERO).unwrap().unwrap().header.seq, 1);
    let mut later = bus
        .subscribe_typed(Diagnostic::TOPIC, "later", Diagnostic::TYPE)
        .unwrap();
    for (at, sequence) in [(0, 2), (199_999, 3)] {
        micros.store(at, Ordering::SeqCst);
        reporter.publish(&report).unwrap();
        assert_eq!(
            first.recv(Duration::ZERO).unwrap().unwrap().header.seq,
            sequence
        );
        assert!(later.recv(Duration::ZERO).unwrap().is_none());
    }
    micros.store(200_000, Ordering::SeqCst);
    reporter.publish(&report).unwrap();
    assert_eq!(first.recv(Duration::ZERO).unwrap().unwrap().header.seq, 4);
    assert_eq!(later.recv(Duration::ZERO).unwrap().unwrap().header.seq, 4);

    let mut entries = fs::read_dir(scratch.0.join("diagnostics/pubs")).unwrap();
    let entry = entries.next().unwrap().unwrap().path();
    assert!(entries.next().is_none());
    fs::remove_file(&entry).unwrap();
    micros.store(999_999, Ordering::SeqCst);
    reporter.publish(&report).unwrap();
    assert!(!entry.exists());
    assert!(first.recv(Duration::ZERO).unwrap().is_some());
    assert!(later.recv(Duration::ZERO).unwrap().is_some());
    micros.store(1_000_000, Ordering::SeqCst);
    reporter.publish(&report).unwrap();
    assert!(entry.is_file());
    assert!(first.recv(Duration::ZERO).unwrap().is_some());
    assert!(later.recv(Duration::ZERO).unwrap().is_some());
}
