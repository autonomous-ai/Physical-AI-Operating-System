//! Handler tests use real private files and bounded local control, not a child
//! status inferred from progress. The original recorder still records nothing.
use super::*;
use crate::physical::fixture_control::{ParentListener, ParentPeer, Step};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};

const WATCHDOG: Duration = Duration::from_secs(2);
const EPOCH_US: u64 = 1_000_000;
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    out: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(format!(
            "/tmp/lve-recorder-command-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        Self {
            out: root.join("recording with spaces"),
            root,
        }
    }

    fn arguments(&self, seconds: u64, fail_after_ms: u64) -> Vec<String> {
        vec![
            "--out".into(),
            self.out.display().to_string(),
            "--seconds".into(),
            seconds.to_string(),
            "--fail-after-ms".into(),
            fail_after_ms.to_string(),
        ]
    }

    fn start(
        &self,
        arguments: Vec<String>,
    ) -> (
        ParentListener,
        ParentPeer,
        Receiver<Result<i32>>,
        JoinHandle<()>,
    ) {
        let socket = self.root.join("control.sock");
        let listener = ParentListener::bind_with_watchdog(&socket, WATCHDOG).unwrap();
        let mut controlled = vec!["--fixture-control".into(), socket.display().to_string()];
        controlled.extend(arguments);
        let (result_tx, result_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let _ = result_tx.send(recorder(controlled));
        });
        let peer = listener.accept(Role::Recorder).unwrap();
        (listener, peer, result_rx, worker)
    }

    fn assert_only_ready(&self, declared_end: Value) {
        let ready: Value =
            serde_json::from_slice(&fs::read(self.out.join("ready.json")).unwrap()).unwrap();
        assert_eq!(
            ready,
            json!({"fake": true, "note": "fake recorder; records nothing",
                "planned_capture_end_host_monotonic_ns": declared_end})
        );
        let names: Vec<_> = fs::read_dir(&self.out)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("ready.json")]);
        assert!(!self.out.join("metadata.json").exists());
        assert!(!self.out.join("room.wav").exists());
        assert_eq!(
            fs::metadata(&self.out).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn step(round: u64, at_us: u64) -> Step {
    Step {
        round,
        at_us,
        stdout_target: 0,
        runner_input_target: 0,
        wait_for_sigterm: false,
    }
}

fn progress(poll_seq: u64, due: u64, ended: bool) -> Progress {
    Progress::Recorder {
        poll_seq,
        ready_written: true,
        wait_started_us: EPOCH_US,
        exit_due_us: due,
        logical_end: ended,
    }
}

fn finish(result: Receiver<Result<i32>>, worker: JoinHandle<()>) -> Result<i32> {
    let result = result
        .recv_timeout(WATCHDOG)
        .expect("recorder handler did not return");
    worker.join().unwrap();
    result
}

#[test]
fn recorder_first_step_precedes_real_ready_write_and_the_entire_original_wait() {
    let fixture = Fixture::new();
    let (_listener, mut peer, result, worker) = fixture.start(fixture.arguments(260, 6000));
    // Hello proves the handler is parked at its first Step, without a sleep or
    // guessed scheduling window. It cannot create readiness before that Step.
    assert!(!fixture.out.exists());
    let due = EPOCH_US + 6_000_000;
    assert_eq!(
        peer.step(step(1, EPOCH_US)).unwrap(),
        progress(1, due, false)
    );
    fixture.assert_only_ready(Value::Null);
    let original_ready = fs::read(fixture.out.join("ready.json")).unwrap();
    assert_eq!(
        peer.step(step(2, EPOCH_US)).unwrap(),
        progress(2, due, false)
    );
    assert_eq!(
        peer.step(step(3, due - 1)).unwrap(),
        progress(3, due, false)
    );
    assert!(matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert_eq!(peer.step(step(4, due)).unwrap(), progress(4, due, true));
    assert_eq!(finish(result, worker).unwrap(), 1);
    assert_eq!(
        fs::read(fixture.out.join("ready.json")).unwrap(),
        original_ready
    );
    fixture.assert_only_ready(Value::Null);
}

#[test]
fn declared_capture_end_does_not_shorten_the_twenty_three_second_wait() {
    let fixture = Fixture::new();
    let mut arguments = fixture.arguments(260, 23_000);
    arguments.extend(["--exit-code", "0", "--declare-end-after-ms", "20000"].map(str::to_owned));
    let (_listener, mut peer, result, worker) = fixture.start(arguments);
    let due = EPOCH_US + 23_000_000;
    let declared_us = EPOCH_US + 20_000_000;
    assert_eq!(
        peer.step(step(1, EPOCH_US)).unwrap(),
        progress(1, due, false)
    );
    fixture.assert_only_ready(json!(declared_us * 1000));
    assert_eq!(
        peer.step(step(2, declared_us)).unwrap(),
        progress(2, due, false)
    );
    assert_eq!(
        peer.step(step(3, due - 1)).unwrap(),
        progress(3, due, false)
    );
    assert!(matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert_eq!(peer.step(step(4, due)).unwrap(), progress(4, due, true));
    assert_eq!(finish(result, worker).unwrap(), 0);
    fixture.assert_only_ready(json!(declared_us * 1000));
}

#[test]
fn zero_and_limited_durations_keep_real_ready_and_configured_return_codes() {
    for (seconds, fail_after_ms, duration_us, exit_code) in [
        (0, 6000, 0, 0),
        (10, 0, 0, 17),
        (5, 6000, 5_000_000, -9),
        (260, 6000, 6_000_000, 0),
    ] {
        let fixture = Fixture::new();
        let mut arguments = fixture.arguments(seconds, fail_after_ms);
        arguments.extend(["--exit-code".to_owned(), exit_code.to_string()]);
        let (_listener, mut peer, result, worker) = fixture.start(arguments);
        let due = EPOCH_US + duration_us;
        assert_eq!(
            peer.step(step(1, EPOCH_US)).unwrap(),
            progress(1, due, duration_us == 0)
        );
        fixture.assert_only_ready(Value::Null);
        if duration_us != 0 {
            assert_eq!(
                peer.step(step(2, due - 1)).unwrap(),
                progress(2, due, false)
            );
            assert_eq!(peer.step(step(3, due)).unwrap(), progress(3, due, true));
        }
        assert_eq!(finish(result, worker).unwrap(), exit_code);
        fixture.assert_only_ready(Value::Null);
    }
}

#[test]
fn time_arithmetic_overflows_fail_before_the_shared_body_creates_files() {
    for (seconds, fail_after_ms, declared_ms, expected) in [
        (u64::MAX / 1000 + 1, 0, None, "seconds-to-milliseconds"),
        (
            u64::MAX / 1000,
            u64::MAX,
            None,
            "wait duration microseconds",
        ),
        (
            u64::MAX / 1_000_000 + 1,
            u64::MAX / 1000,
            None,
            "wait deadline microseconds",
        ),
        (
            u64::MAX / 1_000_000_000 + 1,
            u64::MAX / 1_000_000,
            None,
            "microseconds-to-nanoseconds",
        ),
        (
            0,
            0,
            Some(u64::MAX / 1_000_000 + 1),
            "declared-end nanoseconds",
        ),
        (0, 0, Some(u64::MAX / 1_000_000), "declared-end nanoseconds"),
    ] {
        let fixture = Fixture::new();
        let mut arguments = fixture.arguments(seconds, fail_after_ms);
        if let Some(ms) = declared_ms {
            arguments.extend(["--declare-end-after-ms".to_owned(), ms.to_string()]);
        }
        assert!(
            recorder_timing(&arguments, EPOCH_US)
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
        let (_listener, mut peer, result, worker) = fixture.start(arguments);
        assert!(peer.step(step(1, EPOCH_US)).is_err());
        let error = finish(result, worker).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
        assert!(!fixture.out.exists());
    }
}

#[test]
fn explicit_timing_checks_both_nanosecond_boundaries_without_wrapping() {
    let fixture = Fixture::new();
    let arguments = fixture.arguments(0, 0);
    let last_us = u64::MAX / 1000;
    assert_eq!(
        recorder_timing(&arguments, last_us).unwrap(),
        Duration::ZERO
    );
    assert!(recorder_timing(&arguments, last_us + 1).is_err());
    assert_eq!(
        recorder_nanoseconds(last_us).unwrap(),
        u64::MAX - u64::MAX % 1000
    );
    assert!(recorder_exit_due(last_us, Duration::from_micros(1)).is_err());
    let mut declared = arguments.clone();
    declared.extend([
        "--declare-end-after-ms".to_owned(),
        ((u64::MAX - EPOCH_US * 1000) / 1_000_000).to_string(),
    ]);
    assert_eq!(
        recorder_timing(&declared, EPOCH_US).unwrap(),
        Duration::ZERO
    );
    assert!(!fixture.out.exists());
}

#[test]
fn first_matching_values_and_missing_optional_values_keep_original_meaning() {
    for missing in ["--exit-code", "--declare-end-after-ms"] {
        let fixture = Fixture::new();
        let mut arguments = fixture.arguments(10, 6000);
        arguments.extend(["--fail-after-ms", "1", "--unused", "kept", missing].map(str::to_owned));
        let (_listener, mut peer, result, worker) = fixture.start(arguments);
        let due = EPOCH_US + 6_000_000;
        assert_eq!(
            peer.step(step(1, EPOCH_US)).unwrap(),
            progress(1, due, false)
        );
        fixture.assert_only_ready(Value::Null);
        assert_eq!(peer.step(step(2, due)).unwrap(), progress(2, due, true));
        assert_eq!(finish(result, worker).unwrap(), 1);
    }
}

#[test]
fn invalid_shared_arguments_or_output_creation_never_acknowledge_ready() {
    for failure in ["out", "exit", "existing_directory"] {
        let fixture = Fixture::new();
        let mut arguments = fixture.arguments(260, 6000);
        match failure {
            "out" => {
                arguments.drain(0..2);
            }
            "exit" => arguments.extend(["--exit-code", "invalid"].map(str::to_owned)),
            _ => fs::create_dir(&fixture.out).unwrap(),
        }
        let (_listener, mut peer, result, worker) = fixture.start(arguments);
        assert!(peer.step(step(1, EPOCH_US)).is_err());
        assert!(finish(result, worker).is_err());
        assert!(!fixture.out.join("ready.json").exists());
    }
}

#[test]
fn a_lost_parent_returns_a_control_error_without_completing_the_wait() {
    let fixture = Fixture::new();
    let (_listener, mut peer, result, worker) = fixture.start(fixture.arguments(260, 6000));
    let due = EPOCH_US + 6_000_000;
    assert_eq!(
        peer.step(step(1, EPOCH_US)).unwrap(),
        progress(1, due, false)
    );
    drop(peer);
    let error = finish(result, worker).unwrap_err();
    assert!(error.to_string().contains("fixture control"));
    fixture.assert_only_ready(Value::Null);
}

#[test]
fn recorder_command_requires_the_explicit_leading_control_pair() {
    for arguments in [
        vec![],
        vec!["--out", "/unused/recorder"],
        vec!["--fixture-control"],
        vec!["--fixture-control", "relative.sock"],
    ] {
        let error = recorder(arguments.into_iter().map(str::to_owned).collect()).unwrap_err();
        assert!(error.to_string().contains("fixture-control"));
    }
}
