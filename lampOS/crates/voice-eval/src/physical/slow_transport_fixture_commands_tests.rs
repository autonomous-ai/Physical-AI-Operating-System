//! Real control exchanges and owned command children exercise delay boundaries.
//! Shell exec is a shared-adapter test, not proof of product relay startup.
use super::*;
use crate::physical::fixture_control::{ParentListener, ParentPeer, Step};
use std::{
    fs,
    os::unix::{fs::DirBuilderExt, process::CommandExt},
    process::{Child, Command, Output, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

const WATCHDOG: Duration = Duration::from_secs(2);
const CHILD_BOUND: Duration = Duration::from_secs(10);
const EPOCH_US: u64 = 1_000_000;
const CHILD_TEST: &str = "LAMVE_SLOW_TRANSPORT_ADAPTER_OWNED_TEST";
const CHILD_CASE: &str = "LAMVE_SLOW_TRANSPORT_ADAPTER_OWNED_CASE";
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    control: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(format!(
            "/tmp/lve-transport-command-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        Self {
            control: root.join("control.sock"),
            root,
        }
    }

    fn listener(&self) -> ParentListener {
        ParentListener::bind_with_watchdog(&self.control, WATCHDOG).unwrap()
    }

    fn start_wait(
        &self,
        duration: Duration,
    ) -> (
        ParentListener,
        ParentPeer,
        Receiver<Result<()>>,
        JoinHandle<()>,
    ) {
        let listener = self.listener();
        let control = self.control.clone();
        let (send, result) = mpsc::channel();
        let worker = thread::spawn(move || {
            let outcome =
                Participant::connect_with_watchdog(&control, Role::SlowTransport, WATCHDOG)
                    .map_err(Into::into)
                    .and_then(|participant| wait_for_transport_exec(duration, participant));
            let _ = send.send(outcome);
        });
        let peer = listener.accept(Role::SlowTransport).unwrap();
        (listener, peer, result, worker)
    }

    fn relay_arguments(&self) -> Vec<String> {
        let executable = std::env::current_exe().unwrap().display().to_string();
        vec![
            "--delay-ms".into(),
            "400".into(),
            "--".into(),
            executable.clone(),
            "fake-clock-serve-session".into(),
            "--fixture-control".into(),
            self.control.display().to_string(),
            "--runtime".into(),
            executable,
            "--runtime-prefix-arg".into(),
            "fake-clock-serve-runtime".into(),
            "--runtime-prefix-arg".into(),
            "--fixture-control".into(),
            "--runtime-prefix-arg".into(),
            self.control.display().to_string(),
            "--work".into(),
            self.root.join("relay work").display().to_string(),
            "--seconds".into(),
            "60".into(),
            "--serve-config".into(),
            "/private/unused-provider.json".into(),
            "--allowance-seconds".into(),
            "10".into(),
            "--runtime-prefix-arg".into(),
            "--fake-plan".into(),
            "--runtime-prefix-arg".into(),
            "a plan path; kept literally.json".into(),
        ]
    }

    fn shell_arguments(&self) -> Vec<String> {
        vec![
            "--delay-ms".into(), "400".into(), "--".into(),
            "/bin/sh".into(), "-c".into(),
            "IFS= read -r first && IFS= read -r second || exit 41; printf 'actual exec\n' > \"$1\"; printf '\ncontrolled-transport-exec|%s|%s|%s|%s|%s\n' \"$$\" \"$2\" \"$3\" \"$first\" \"$second\"; exit 37".into(),
            "slow-transport-adapter-test".into(), self.root.join("exec.marker").display().to_string(),
            "two words".into(), "semi;literal".into(),
        ]
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
    Progress::SlowTransport {
        poll_seq,
        wait_started_us: EPOCH_US,
        exec_due_us: due,
        logical_end: ended,
    }
}

fn finish_wait(result: Receiver<Result<()>>, worker: JoinHandle<()>) -> Result<()> {
    let result = result
        .recv_timeout(WATCHDOG)
        .expect("bounded transport wait completion");
    worker.join().unwrap();
    result
}

#[test]
fn the_wait_keeps_all_four_hundred_milliseconds_and_distinct_same_time_rounds() {
    let fixture = Fixture::new();
    let (_listener, mut peer, result, worker) = fixture.start_wait(Duration::from_millis(400));
    let due = EPOCH_US + 400_000;
    assert_eq!(
        peer.step(step(1, EPOCH_US)).unwrap(),
        progress(1, due, false)
    );
    assert_eq!(
        peer.step(step(2, EPOCH_US)).unwrap(),
        progress(2, due, false)
    );
    assert_eq!(
        peer.step(step(3, due - 1)).unwrap(),
        progress(3, due, false)
    );
    assert_eq!(peer.step(step(4, due)).unwrap(), progress(4, due, true));
    finish_wait(result, worker).unwrap();
    assert!(peer.step(step(5, due)).is_err());
}

#[test]
fn a_zero_delay_completes_at_its_first_step_without_a_synthetic_exec_receipt() {
    let fixture = Fixture::new();
    let (listener, mut peer, result, worker) = fixture.start_wait(Duration::ZERO);
    assert_eq!(
        peer.step(step(1, EPOCH_US)).unwrap(),
        progress(1, EPOCH_US, true)
    );
    finish_wait(result, worker).unwrap();
    assert_eq!(
        listener
            .accept_until(Role::ServeRelay, Instant::now() + Duration::from_millis(20))
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
}

#[test]
fn duration_and_domain_overflows_are_rejected_before_any_wait_ack() {
    for (duration, expected) in [
        (
            Duration::from_millis(u64::MAX),
            "duration microseconds overflow",
        ),
        (
            Duration::from_micros(u64::MAX),
            "deadline microseconds overflow",
        ),
        (
            Duration::from_micros(u64::MAX / 1000),
            "shared nanosecond clock domain",
        ),
    ] {
        let fixture = Fixture::new();
        let (_listener, mut peer, result, worker) = fixture.start_wait(duration);
        assert!(peer.step(step(1, EPOCH_US)).is_err());
        assert!(
            finish_wait(result, worker)
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
        assert!(!fixture.root.join("exec.marker").exists());
    }
    let last = u64::MAX / 1000;
    assert_eq!(slow_transport_exec_due(last, Duration::ZERO).unwrap(), last);
    assert!(slow_transport_exec_due(last + 1, Duration::ZERO).is_err());
    assert!(slow_transport_exec_due(last, Duration::from_micros(1)).is_err());
    assert_eq!(
        slow_transport_exec_due(EPOCH_US, Duration::from_millis(400)).unwrap(),
        EPOCH_US + 400_000
    );
}

#[test]
fn a_lost_parent_cancels_the_wait_without_completing_its_deadline() {
    let fixture = Fixture::new();
    let (_listener, mut peer, result, worker) = fixture.start_wait(Duration::from_millis(400));
    assert_eq!(
        peer.step(step(1, EPOCH_US)).unwrap(),
        progress(1, EPOCH_US + 400_000, false)
    );
    drop(peer);
    assert!(
        finish_wait(result, worker)
            .unwrap_err()
            .to_string()
            .contains("fixture control")
    );
}

#[test]
fn only_the_same_controlled_evaluator_relay_and_socket_are_accepted_without_rewriting() {
    let fixture = Fixture::new();
    let arguments = fixture.relay_arguments();
    let original = arguments.clone();
    validate_slow_transport_arguments(&arguments, &fixture.control).unwrap();
    assert_eq!(arguments, original);
    assert!(!fixture.control.exists());
    assert!(!fixture.root.join("relay work").exists());
    for (index, value) in [
        (0, "--delay"),
        (1, "18446744073709551616"),
        (3, "/bin/sh"),
        (3, "relative-evaluator"),
        (4, "fake-clock-session"),
        (4, "fake-clock-slow-transport"),
        (5, "--other-control"),
        (6, "/private/other-control.sock"),
        (8, "/bin/sh"),
        (10, "fake-lamp-voice"),
        (14, "/private/other-runtime-control.sock"),
        (19, "--provider-config"),
    ] {
        let mut invalid = arguments.clone();
        invalid[index] = value.into();
        assert!(
            validate_slow_transport_arguments(&invalid, &fixture.control).is_err(),
            "{index} {value}"
        );
    }
    for invalid in [
        Vec::new(),
        vec!["--delay-ms".into(), "400".into(), "--".into()],
    ] {
        assert!(validate_slow_transport_arguments(&invalid, &fixture.control).is_err());
    }
}

#[test]
fn the_explicit_handler_requires_a_leading_absolute_control_pair() {
    for arguments in [
        vec![],
        vec!["--delay-ms", "400"],
        vec!["--fixture-control"],
        vec!["--fixture-control", "relative.sock"],
    ] {
        assert!(
            slow_transport(arguments.into_iter().map(str::to_owned).collect())
                .unwrap_err()
                .to_string()
                .contains("fixture-control")
        );
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ChildCase {
    control: PathBuf,
    arguments: Vec<String>,
}

fn child_case(test: &str) -> Option<ChildCase> {
    if std::env::var(CHILD_TEST).is_ok_and(|value| value == test) {
        Some(serde_json::from_str(&std::env::var(CHILD_CASE).unwrap()).unwrap())
    } else {
        None
    }
}

fn run_child_adapter(case: ChildCase) -> Result<i32> {
    // This tests the identical shared adapter after public target validation.
    // The explicit public handler rejects this test's shell/directory target.
    let participant =
        Participant::connect_with_watchdog(&case.control, Role::SlowTransport, WATCHDOG)?;
    controlled_slow_transport(&case.arguments, participant)
}

struct OwnedChild(Option<Child>);

impl OwnedChild {
    fn spawn(test: &str, fixture: &Fixture, arguments: Vec<String>) -> Self {
        let (_, filter) = test.split_once("::").expect("full module path");
        let case = ChildCase {
            control: fixture.control.clone(),
            arguments,
        };
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", filter, "--nocapture", "--test-threads=1"])
            // These markers select only this owned test child. No shared
            // environment or transport entrypoint reads them.
            .env(CHILD_TEST, test)
            .env(CHILD_CASE, serde_json::to_string(&case).unwrap())
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
                    "owned controlled command exceeded its host bound: stdout={} stderr={}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            // Observe this one child's actual exit, without sleeping or
            // retrying any test or logical transport step.
            thread::yield_now();
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

#[test]
fn real_exec_occurs_after_the_full_delay_and_preserves_the_same_pipe_pid_and_argv() {
    const TEST: &str = concat!(
        module_path!(),
        "::real_exec_occurs_after_the_full_delay_and_preserves_the_same_pipe_pid_and_argv"
    );
    if let Some(case) = child_case(TEST) {
        let result = run_child_adapter(case);
        panic!("successful exec must replace only this owned child: {result:?}");
    }
    let fixture = Fixture::new();
    let listener = fixture.listener();
    let mut child = OwnedChild::spawn(TEST, &fixture, fixture.shell_arguments());
    let process = child.0.as_mut().unwrap();
    let pid = process.id();
    process
        .stdin
        .take()
        .unwrap()
        .write_all(b"Ping 1\nPing 2\n")
        .unwrap();
    let mut peer = listener.accept(Role::SlowTransport).unwrap();
    let marker = fixture.root.join("exec.marker");
    assert!(!marker.exists());
    let due = EPOCH_US + 400_000;
    for (round, at) in [(1, EPOCH_US), (2, EPOCH_US), (3, due - 1)] {
        assert_eq!(
            peer.step(step(round, at)).unwrap(),
            progress(round, due, false)
        );
        assert!(!marker.exists());
    }
    assert_eq!(peer.step(step(4, due)).unwrap(), progress(4, due, true));
    let output = child.finish();
    assert_eq!(
        output.status.code(),
        Some(37),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(marker).unwrap(), "actual exec\n");
    assert!(String::from_utf8_lossy(&output.stdout).contains(&format!(
        "\ncontrolled-transport-exec|{pid}|two words|semi;literal|Ping 1|Ping 2\n"
    )));
    assert_eq!(
        listener
            .accept_until(Role::ServeRelay, Instant::now() + Duration::from_millis(20))
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
}

#[test]
fn terminal_delay_ack_still_returns_the_real_exec_error_when_replacement_fails() {
    const TEST: &str = concat!(
        module_path!(),
        "::terminal_delay_ack_still_returns_the_real_exec_error_when_replacement_fails"
    );
    if let Some(case) = child_case(TEST) {
        let expected = Command::new("/").exec();
        let error = run_child_adapter(case).unwrap_err();
        let error = error.downcast_ref::<io::Error>().unwrap();
        assert_eq!(error.kind(), expected.kind());
        assert_eq!(error.raw_os_error(), expected.raw_os_error());
        assert_eq!(error.to_string(), expected.to_string());
        println!("\ncontrolled-transport-original-exec-error");
        return;
    }
    let fixture = Fixture::new();
    let listener = fixture.listener();
    let child = OwnedChild::spawn(
        TEST,
        &fixture,
        ["--delay-ms", "400", "--", "/"].map(str::to_owned).to_vec(),
    );
    let mut peer = listener.accept(Role::SlowTransport).unwrap();
    let due = EPOCH_US + 400_000;
    assert_eq!(
        peer.step(step(1, EPOCH_US)).unwrap(),
        progress(1, due, false)
    );
    assert_eq!(peer.step(step(2, due)).unwrap(), progress(2, due, true));
    let output = child.finish();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("\ncontrolled-transport-original-exec-error\n")
    );
}

#[test]
fn a_lost_control_peer_prevents_the_real_command_from_reaching_exec() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_lost_control_peer_prevents_the_real_command_from_reaching_exec"
    );
    if let Some(case) = child_case(TEST) {
        let error = run_child_adapter(case).unwrap_err();
        assert!(error.to_string().contains("fixture control"), "{error}");
        println!("\ncontrolled-transport-parent-loss-returned");
        return;
    }
    let fixture = Fixture::new();
    let listener = fixture.listener();
    let child = OwnedChild::spawn(TEST, &fixture, fixture.shell_arguments());
    let mut peer = listener.accept(Role::SlowTransport).unwrap();
    assert_eq!(
        peer.step(step(1, EPOCH_US)).unwrap(),
        progress(1, EPOCH_US + 400_000, false)
    );
    drop(peer);
    let output = child.finish();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("\ncontrolled-transport-parent-loss-returned\n")
    );
    assert!(!fixture.root.join("exec.marker").exists());
}
