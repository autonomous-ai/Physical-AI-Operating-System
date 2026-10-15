use super::{slow_transport, slow_transport_with_wait};
use crate::invalid;
use std::{
    cell::Cell,
    error::Error,
    fmt,
    io::{self, Write},
    num::ParseIntError,
    os::unix::process::CommandExt,
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

const USAGE: &str = "fake-slow-transport --delay-ms MS -- ARGV...";
// Only Command::env on an explicitly owned unit-test child sets this marker.
// Neither transport entrypoint reads this marker.
const CHILD_TEST: &str = "LAMVE_SLOW_TRANSPORT_CLOCK_OWNED_TEST";
const CHILD_BOUND: Duration = Duration::from_secs(10);

fn arguments(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

#[test]
fn malformed_arguments_fail_before_the_wait_in_both_entrypoints() {
    for values in [
        vec![],
        vec!["--delay-ms", "400"],
        vec!["--delay-ms", "400", "--"],
        vec!["--", "/"],
        vec!["--delay", "400", "--", "/"],
        vec!["--delay-ms", "400", "extra", "--", "/"],
        vec!["--delay-ms", "400", "--delay-ms", "400", "--", "/"],
    ] {
        let values = arguments(&values);
        let error = slow_transport_with_wait(&values, |_| {
            panic!("invalid arguments must not reach the wait")
        })
        .unwrap_err();
        assert_eq!(error.to_string(), USAGE, "{values:?}");
        assert_eq!(
            error.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(slow_transport(&values).unwrap_err().to_string(), USAGE);
    }
}

#[test]
fn malformed_delay_keeps_the_original_parse_error_before_waiting() {
    for delay in ["", "-1", "1.5", "not-a-number", "18446744073709551616"] {
        let values = arguments(&["--delay-ms", delay, "--", "/"]);
        let error = slow_transport_with_wait(&values, |_| {
            panic!("an invalid delay must not reach the wait")
        })
        .unwrap_err();
        let expected = delay.parse::<u64>().unwrap_err();
        assert_eq!(
            error.downcast_ref::<ParseIntError>().unwrap().kind(),
            expected.kind()
        );
        assert_eq!(error.to_string(), expected.to_string());
        assert_eq!(
            slow_transport(&values).unwrap_err().to_string(),
            expected.to_string()
        );
    }
}

#[test]
fn zero_and_full_u64_millisecond_delays_reach_one_wait_without_truncation() {
    for milliseconds in [0, 400, u64::from(u32::MAX) + 1, u64::MAX] {
        let delay = milliseconds.to_string();
        let values = arguments(&["--delay-ms", &delay, "--", "/"]);
        let calls = Cell::new(0);
        let error = slow_transport_with_wait(&values, |duration| {
            calls.set(calls.get() + 1);
            assert_eq!(duration.as_millis(), u128::from(milliseconds));
            assert_eq!(duration.subsec_nanos() % 1_000_000, 0);
            Err(invalid("stop after observing the requested wait"))
        })
        .unwrap_err();
        assert_eq!(calls.get(), 1);
        assert_eq!(error.to_string(), "stop after observing the requested wait");
    }
}

#[derive(Debug)]
struct WaitStopped(String);

impl fmt::Display for WaitStopped {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

impl Error for WaitStopped {}

#[test]
fn a_consuming_wait_error_is_returned_unchanged_before_exec() {
    let message = String::from("the fixture coordinator stopped its wait");
    // A directory cannot be executed. If the error were discarded, the real
    // exec error would replace this typed callback error without replacing
    // the unit-test process.
    let values = arguments(&["--delay-ms", "400", "--", "/"]);
    let error = slow_transport_with_wait(&values, move |duration| {
        assert_eq!(duration.as_millis(), 400);
        Err(Box::new(WaitStopped(message)))
    })
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<WaitStopped>().unwrap().0,
        "the fixture coordinator stopped its wait"
    );
}

struct OwnedChild(Option<Child>);

impl OwnedChild {
    fn spawn(test: &str) -> Self {
        // module_path! includes the crate name; libtest's exact filter does not.
        let (_, filter) = test.split_once("::").expect("a full module path");
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", filter, "--nocapture", "--test-threads=1"])
            .env(CHILD_TEST, test)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Self(Some(child))
    }

    fn finish(mut self) -> Output {
        let deadline = Instant::now() + CHILD_BOUND;
        loop {
            let child = self.0.as_mut().unwrap();
            if child.try_wait().unwrap().is_some() {
                return self.0.take().unwrap().wait_with_output().unwrap();
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let output = self.0.take().unwrap().wait_with_output().unwrap();
                panic!(
                    "owned exec child exceeded its host watchdog: stdout={} stderr={}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            // Observe this one owned child's real exit; never rerun the test
            // or add a delay to the transport being exercised.
            std::thread::yield_now();
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn is_owned_child(test: &str) -> bool {
    std::env::var(CHILD_TEST).is_ok_and(|value| value == test)
}

#[test]
fn real_exec_failure_keeps_the_original_io_error_after_the_wait() {
    const TEST: &str = concat!(
        module_path!(),
        "::real_exec_failure_keeps_the_original_io_error_after_the_wait"
    );
    if is_owned_child(TEST) {
        let expected = Command::new("/").exec();
        let values = arguments(&["--delay-ms", "400", "--", "/"]);
        let calls = Cell::new(0);
        let error = slow_transport_with_wait(&values, |duration| {
            calls.set(calls.get() + 1);
            assert_eq!(duration.as_millis(), 400);
            Ok(())
        })
        .unwrap_err();
        let error = error.downcast_ref::<io::Error>().unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(error.kind(), expected.kind());
        assert_eq!(error.raw_os_error(), expected.raw_os_error());
        assert_eq!(error.to_string(), expected.to_string());
        println!("\nslow-transport-original-exec-error-checked");
        return;
    }
    let output = OwnedChild::spawn(TEST).finish();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("\nslow-transport-original-exec-error-checked\n")
    );
}

#[test]
fn real_exec_replaces_the_owned_child_and_keeps_argv_and_queued_input() {
    const TEST: &str = concat!(
        module_path!(),
        "::real_exec_replaces_the_owned_child_and_keeps_argv_and_queued_input"
    );
    if is_owned_child(TEST) {
        let values = arguments(&[
            "--delay-ms",
            "400",
            "--",
            "/bin/sh",
            "-c",
            "IFS= read -r line; printf '\nslow-transport-exec|%s|%s|%s|%s|%s\n' \"$$\" \"$1\" \"$2\" \"$3\" \"$line\"; exit 37",
            "slow-transport-test",
            "two words",
            "--literal",
            "semi;literal",
        ]);
        let result = slow_transport_with_wait(&values, |duration| {
            assert_eq!(duration.as_millis(), 400);
            let mut stderr = io::stderr().lock();
            stderr.write_all(b"slow-transport-wait-completed\n")?;
            stderr.flush()?;
            Ok(())
        });
        panic!("successful exec must replace this owned child: {result:?}");
    }
    let mut child = OwnedChild::spawn(TEST);
    let process = child.0.as_mut().unwrap();
    let pid = process.id();
    process
        .stdin
        .take()
        .unwrap()
        .write_all(b"queued setup Ping\n")
        .unwrap();
    let output = child.finish();
    assert_eq!(
        output.status.code(),
        Some(37),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(&format!(
        "\nslow-transport-exec|{pid}|two words|--literal|semi;literal|queued setup Ping\n"
    )));
    assert!(String::from_utf8_lossy(&output.stderr).contains("slow-transport-wait-completed\n"));
}
