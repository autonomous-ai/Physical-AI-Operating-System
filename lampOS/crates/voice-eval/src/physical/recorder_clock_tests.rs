//! The explicit adapter keeps real files and the original recorder arguments.
//! Scripted samples below observe call boundaries; reading never advances time.
use super::*;
use std::{
    cell::{Cell, RefCell},
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    out: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(format!(
            "/tmp/lve-recorder-clock-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        Self {
            out: root.join("recording"),
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

    fn ready(&self) -> Value {
        serde_json::from_slice(&fs::read(self.out.join("ready.json")).unwrap()).unwrap()
    }

    fn assert_only_ready(&self, declared_end: Value) {
        assert_eq!(
            self.ready(),
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

#[test]
fn declaration_and_fresh_post_write_sample_stay_distinct_in_nanoseconds() {
    let fixture = Fixture::new();
    let mut arguments = fixture.arguments(260, 23_000);
    arguments.extend(["--exit-code", "0", "--declare-end-after-ms", "20000"].map(str::to_owned));
    let at_ns = Cell::new(123_456_789_u64);
    let ready_at_read = RefCell::new(Vec::new());
    let reads = Cell::new(0);
    let now = || {
        reads.set(reads.get() + 1);
        ready_at_read
            .borrow_mut()
            .push(fixture.out.join("ready.json").exists());
        at_ns.get()
    };
    let mut waits = 0;
    let exit_code = room_recorder_with_clock_and_wait(&arguments, now, |duration, clock| {
        waits += 1;
        assert_eq!(duration, Duration::from_millis(23_000));
        fixture.assert_only_ready(json!(20_123_456_789_u64));
        assert_eq!(reads.get(), 1);
        // The adapter owns this separate observation after the actual write.
        // A pure read does not move time or reuse the earlier declaration.
        at_ns.set(987_654_321);
        let wait_started_ns = clock();
        assert_eq!(wait_started_ns, 987_654_321);
        assert_eq!(at_ns.get(), wait_started_ns);
        assert_eq!(
            wait_started_ns + u64::try_from(duration.as_nanos()).unwrap(),
            23_987_654_321
        );
        fixture.assert_only_ready(json!(20_123_456_789_u64));
        Ok(())
    })
    .unwrap();
    assert_eq!(exit_code, 0);
    assert_eq!(waits, 1);
    assert_eq!(reads.get(), 2);
    assert_eq!(*ready_at_read.borrow(), vec![false, true]);
    fixture.assert_only_ready(json!(20_123_456_789_u64));
}

#[test]
fn no_declaration_adds_no_clock_sample_and_keeps_the_default_failure_code() {
    let fixture = Fixture::new();
    let mut waits = 0;
    let exit_code = room_recorder_with_clock_and_wait(
        &fixture.arguments(260, 6_000),
        || panic!("no declaration requested a clock sample"),
        |duration, _| {
            waits += 1;
            assert_eq!(duration, Duration::from_secs(6));
            fixture.assert_only_ready(Value::Null);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(exit_code, 1);
    assert_eq!(waits, 1);
    fixture.assert_only_ready(Value::Null);
}

#[test]
fn one_wait_keeps_the_full_minimum_duration_and_configured_return_code() {
    for (seconds, fail_after_ms, expected_ms, exit_code) in [
        (0, 6_000, 0, 0_i32),
        (6, 6_000, 6_000, 1),
        (5, 6_000, 5_000, 17),
        (260, 23_000, 23_000, -9),
        (u64::MAX / 1000, 1, 1, 0),
    ] {
        let fixture = Fixture::new();
        let mut arguments = fixture.arguments(seconds, fail_after_ms);
        arguments.extend(["--exit-code".to_owned(), exit_code.to_string()]);
        let mut waits = 0;
        let result = room_recorder_with_clock_and_wait(
            &arguments,
            || panic!("no declaration requested a clock sample"),
            |duration, _| {
                waits += 1;
                assert_eq!(duration, Duration::from_millis(expected_ms));
                fixture.assert_only_ready(Value::Null);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(result, exit_code);
        assert_eq!(waits, 1);
        fixture.assert_only_ready(Value::Null);
    }
}

#[test]
fn zero_declared_delay_keeps_its_optional_sample_without_shortening_the_wait() {
    let fixture = Fixture::new();
    let mut arguments = fixture.arguments(10, 6_000);
    arguments.extend(["--declare-end-after-ms", "0"].map(str::to_owned));
    let reads = Cell::new(0);
    let result = room_recorder_with_clock_and_wait(
        &arguments,
        || {
            reads.set(reads.get() + 1);
            999
        },
        |duration, _| {
            assert_eq!(duration, Duration::from_secs(6));
            fixture.assert_only_ready(json!(999));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(result, 1);
    assert_eq!(reads.get(), 1);
}

#[test]
fn original_argument_failures_precede_files_clock_samples_and_waits() {
    for case in [
        "missing_out",
        "missing_seconds",
        "seconds",
        "fail_after",
        "exit",
        "declared",
    ] {
        let fixture = Fixture::new();
        let mut arguments = fixture.arguments(260, 6_000);
        match case {
            "missing_out" => {
                arguments.drain(0..2);
            }
            "missing_seconds" => {
                arguments.drain(2..4);
            }
            "seconds" => arguments[3] = "invalid".into(),
            "fail_after" => arguments[5] = "invalid".into(),
            "exit" => arguments.extend(["--exit-code", "invalid"].map(str::to_owned)),
            _ => arguments.extend(["--declare-end-after-ms", "invalid"].map(str::to_owned)),
        }
        let result = room_recorder_with_clock_and_wait(
            &arguments,
            || panic!("invalid arguments reached the clock"),
            |_, _| panic!("invalid arguments reached the wait"),
        );
        assert!(result.is_err(), "{case}");
        assert!(!fixture.out.exists(), "{case}");
    }
}

#[test]
fn first_matching_argument_and_missing_optional_value_keep_original_meaning() {
    for missing in ["--exit-code", "--declare-end-after-ms"] {
        let fixture = Fixture::new();
        let mut arguments = fixture.arguments(10, 6_000);
        arguments.extend(["--fail-after-ms", "1", "--unused", "kept", missing].map(str::to_owned));
        let result = room_recorder_with_clock_and_wait(
            &arguments,
            || panic!("a missing optional value must not request a clock sample"),
            |duration, _| {
                assert_eq!(duration, Duration::from_secs(6));
                fixture.assert_only_ready(Value::Null);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(result, 1);
    }
}

#[test]
fn directory_failure_keeps_the_declaration_before_io_and_never_waits() {
    let fixture = Fixture::new();
    fs::create_dir(&fixture.out).unwrap();
    let mut arguments = fixture.arguments(260, 6_000);
    arguments.extend(["--declare-end-after-ms", "20000"].map(str::to_owned));
    let reads = Cell::new(0);
    let result = room_recorder_with_clock_and_wait(
        &arguments,
        || {
            reads.set(reads.get() + 1);
            1_234_567
        },
        |_, _| panic!("failed directory creation reached the wait"),
    );
    assert!(result.is_err());
    assert_eq!(reads.get(), 1);
    assert!(!fixture.out.join("ready.json").exists());
    assert_eq!(fs::read_dir(&fixture.out).unwrap().count(), 0);
}

#[test]
fn an_explicit_wait_failure_keeps_the_written_receipt_and_cannot_return_success() {
    let fixture = Fixture::new();
    let mut arguments = fixture.arguments(260, 23_000);
    arguments.extend(["--exit-code", "0", "--declare-end-after-ms", "20000"].map(str::to_owned));
    let result = room_recorder_with_clock_and_wait(
        &arguments,
        || 1_000_001,
        |duration, _| {
            assert_eq!(duration, Duration::from_secs(23));
            fixture.assert_only_ready(json!(20_001_000_001_u64));
            Err(invalid("explicit recorder wait failed"))
        },
    );
    assert_eq!(
        result.unwrap_err().to_string(),
        "explicit recorder wait failed"
    );
    fixture.assert_only_ready(json!(20_001_000_001_u64));
}

#[test]
fn ordinary_entrypoint_keeps_system_time_real_ready_file_and_no_recording() {
    let fixture = Fixture::new();
    let mut arguments = fixture.arguments(0, 6_000);
    arguments.extend(["--declare-end-after-ms", "0"].map(str::to_owned));
    let before = lamp_ipc::monotonic_ns();
    let result = room_recorder(&arguments).unwrap();
    let after = lamp_ipc::monotonic_ns();
    let declared = fixture.ready()["planned_capture_end_host_monotonic_ns"]
        .as_u64()
        .unwrap();
    assert!((before..=after).contains(&declared));
    assert_eq!(result, 1);
    fixture.assert_only_ready(json!(declared));
}
