// Include the same parent helper so these boundary tests can inspect its closed
// state without adding test-only entry points to the scenario-facing API.
include!("support/health_control.rs");

use std::{
    fs::{DirBuilder, OpenOptions},
    net::Shutdown,
    os::unix::{fs::DirBuilderExt, process::ExitStatusExt},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(0);
const NONCE: &str = "ParentBoundary1";

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = PathBuf::from("/tmp").join(format!(
            "lmpc-{}-{}",
            std::process::id(),
            NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        DirBuilder::new().mode(0o700).create(&path).unwrap();
        File::create(path.join("log")).unwrap();
        Self(path)
    }

    fn log(&self) -> PathBuf {
        self.0.join("log")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// Closing the retained fd releases a mock blocked in either direction before
// every join, including an assertion unwind. These peers are scheduling evidence
// only: they never claim to be a Memory report, actual application or OS exit.
struct Peer {
    close: UnixStream,
    worker: Option<thread::JoinHandle<io::Result<()>>>,
}

impl Peer {
    fn finish(mut self) {
        let result = self.worker.take().unwrap().join().unwrap();
        result.unwrap();
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.close.shutdown(Shutdown::Both);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn setup(
    scratch: &Scratch,
    body: impl FnOnce(Channel, PathBuf) -> io::Result<()> + Send + 'static,
) -> (Controller, Peer) {
    let (parent, child) = UnixStream::pair().unwrap();
    let close = child.try_clone().unwrap();
    let path = scratch.log();
    let worker = thread::spawn(move || {
        let mut channel = Channel::new(child)?;
        channel.write_until(
            &Connected {
                version: protocol::VERSION,
                nonce: NONCE.to_owned(),
            },
            host_deadline()?,
        )?;
        body(channel, path)
    });
    let peer = Peer {
        close,
        worker: Some(worker),
    };
    let controller = Controller::connected(
        Channel::new(parent).unwrap(),
        NONCE.to_owned(),
        LogBarrier::open(&scratch.log()).unwrap(),
        host_deadline().unwrap(),
    )
    .unwrap();
    (controller, peer)
}

fn receipt(step: &Step, previous: Option<&Checked>, outcome: Outcome) -> Checked {
    let due = previous.is_none_or(|last| step.at_us == last.next_due_us);
    let attempted = u64::from(outcome != Outcome::NotAttempted);
    let (cycles, attempts, started, next_due, last_outcome) = match previous {
        None => (1, attempted, 0, PERIOD_US, outcome),
        Some(last) if due => (
            last.cycles + 1,
            last.attempts + attempted,
            step.at_us,
            step.at_us + PERIOD_US,
            outcome,
        ),
        Some(last) => (
            last.cycles,
            last.attempts,
            last.cycle_started_us,
            last.next_due_us,
            last.last_outcome,
        ),
    };
    Checked {
        nonce: step.nonce.clone(),
        round: step.round,
        at_us: step.at_us,
        cycles,
        attempts,
        last_outcome,
        cycle_started_us: started,
        next_due_us: next_due,
        marker: protocol::marker(&step.nonce, step.round).unwrap(),
    }
}

fn append(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().append(true).open(path)?;
    file.write_all(bytes)?;
    file.flush()
}

fn send_checked(channel: &mut Channel, log: &Path, checked: &Checked) -> io::Result<()> {
    append(log, checked.marker.as_bytes())?;
    channel.write_until(checked, host_deadline()?)
}

fn assert_closed(controller: &mut Controller) {
    let at = controller.cadence_now();
    assert!(controller.failure.is_some());
    assert_eq!(
        controller.window().err().unwrap().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(
        controller.wait_cadence(Duration::ZERO).unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(controller.cadence_now(), at);
}

#[test]
fn cadence_reads_and_same_time_rounds_do_not_invent_publications() {
    let scratch = Scratch::new();
    let (mut controller, peer) = setup(&scratch, |mut channel, log| {
        let first: Step = channel.read_until(host_deadline()?)?;
        assert_eq!((first.round, first.at_us), (1, 0));
        let checked = receipt(&first, None, Outcome::NotAttempted);
        send_checked(&mut channel, &log, &checked)?;
        let second: Step = channel.read_until(host_deadline()?)?;
        assert_eq!((second.round, second.at_us), (2, 0));
        send_checked(
            &mut channel,
            &log,
            &receipt(&second, Some(&checked), Outcome::ReturnedOk),
        )
    });
    let at = controller.cadence_now();
    for _ in 0..100 {
        assert_eq!(controller.cadence_now(), at);
    }
    assert!(
        controller.checked.is_none(),
        "Connected is not a worker wait"
    );
    let window = controller.window().unwrap();
    controller.settle(&window).unwrap();
    controller.settle(&window).unwrap();
    let checked = controller.checked.as_ref().unwrap();
    assert_eq!(checked.round, 2);
    assert_eq!(checked.cycles, 1);
    assert_eq!(checked.attempts, 0);
    assert_eq!(checked.last_outcome, Outcome::NotAttempted);
    assert_eq!(controller.cadence_now(), at);
    peer.finish();
}

#[test]
fn every_due_cycle_is_split_before_a_longer_target() {
    let scratch = Scratch::new();
    let (mut controller, peer) = setup(&scratch, |mut channel, log| {
        let mut previous = None;
        for (index, at_us) in [0, 1_000_000, 2_000_000, 3_000_000, 3_200_000]
            .into_iter()
            .enumerate()
        {
            let step: Step = channel.read_until(host_deadline()?)?;
            assert_eq!(step.round, index as u64 + 1);
            assert_eq!(step.at_us, at_us);
            let checked = receipt(&step, previous.as_ref(), Outcome::ReturnedErr);
            send_checked(&mut channel, &log, &checked)?;
            previous = Some(checked);
        }
        Ok(())
    });
    let begin = controller.cadence_now();
    let window = controller.window().unwrap();
    controller
        .advance_to(begin + Duration::from_millis(3_200), &window)
        .unwrap();
    let checked = controller.checked.as_ref().unwrap();
    assert_eq!((checked.cycles, checked.attempts), (4, 4));
    assert_eq!(checked.next_due_us, 4_000_000);
    assert_eq!(
        controller.cadence_now() - begin,
        Duration::from_millis(3_200)
    );
    assert_eq!(window.started_us, 0);
    assert_eq!(window.through_us, WINDOW_US);
    peer.finish();
}

#[test]
fn next_due_and_zero_wait_preserve_the_checked_boundary() {
    let scratch = Scratch::new();
    let (mut controller, peer) = setup(&scratch, |mut channel, log| {
        let mut previous = None;
        for expected in [0, 0, PERIOD_US] {
            let step: Step = channel.read_until(host_deadline()?)?;
            assert_eq!(step.at_us, expected);
            let checked = receipt(&step, previous.as_ref(), Outcome::ReturnedOk);
            send_checked(&mut channel, &log, &checked)?;
            previous = Some(checked);
        }
        Ok(())
    });
    let start = controller.cadence_now();
    let window = controller.window().unwrap();
    let due = controller.next_due(&window).unwrap();
    assert_eq!(due - start, Duration::from_secs(1));
    controller.wait_cadence(Duration::ZERO).unwrap();
    assert_eq!(controller.cadence_now(), start);
    controller.advance_to(due, &window).unwrap();
    assert_eq!(controller.checked.as_ref().unwrap().cycles, 2);
    peer.finish();
}

#[test]
fn initial_checked_identity_counters_and_deadline_are_validated() {
    let mutations: [fn(&mut Checked); 12] = [
        |value| value.nonce.push('X'),
        |value| value.round += 1,
        |value| value.at_us += 1,
        |value| {
            value.marker.pop();
        },
        |value| value.cycles = 0,
        |value| value.cycles = 2,
        |value| value.attempts = 0,
        |value| value.attempts = 2,
        |value| value.last_outcome = Outcome::NotAttempted,
        |value| value.cycle_started_us = 1,
        |value| value.next_due_us = 0,
        |value| value.next_due_us += 1,
    ];
    for mutate in mutations {
        let scratch = Scratch::new();
        let (mut controller, peer) = setup(&scratch, move |mut channel, log| {
            let step: Step = channel.read_until(host_deadline()?)?;
            let mut checked = receipt(&step, None, Outcome::ReturnedOk);
            mutate(&mut checked);
            send_checked(&mut channel, &log, &checked)
        });
        let at = controller.cadence_now();
        let window = controller.window().unwrap();
        assert!(controller.settle(&window).is_err());
        assert_eq!(controller.cadence_now(), at);
        assert!(controller.checked.is_none());
        assert_closed(&mut controller);
        peer.finish();
    }
}

#[test]
fn pre_due_progress_cannot_regress_or_claim_a_fresh_cycle() {
    let mutations: [fn(&mut Checked); 7] = [
        |value| value.cycles = 0,
        |value| value.cycles += 1,
        |value| value.attempts = 0,
        |value| value.attempts += 1,
        |value| value.last_outcome = Outcome::ReturnedErr,
        |value| value.cycle_started_us += 1,
        |value| value.next_due_us += 1,
    ];
    for mutate in mutations {
        let first = Step {
            nonce: NONCE.to_owned(),
            round: 1,
            at_us: 0,
        };
        let previous = receipt(&first, None, Outcome::ReturnedOk);
        for at_us in [0, 500_000] {
            let step = Step {
                round: 2,
                at_us,
                ..first.clone()
            };
            let mut checked = receipt(&step, Some(&previous), Outcome::ReturnedOk);
            mutate(&mut checked);
            assert!(validate_checked(&step, Some(&previous), &checked).is_err());
        }
    }
}

#[test]
fn exact_due_requires_one_real_cycle_and_the_matching_attempt_delta() {
    let first = Step {
        nonce: NONCE.to_owned(),
        round: 1,
        at_us: 0,
    };
    let previous = receipt(&first, None, Outcome::ReturnedErr);
    let step = Step {
        round: 2,
        at_us: PERIOD_US,
        ..first
    };
    for outcome in [
        Outcome::NotAttempted,
        Outcome::ReturnedOk,
        Outcome::ReturnedErr,
    ] {
        let checked = receipt(&step, Some(&previous), outcome);
        validate_checked(&step, Some(&previous), &checked).unwrap();
        assert_eq!(checked.cycles, 2);
        assert_eq!(
            checked.attempts,
            1 + u64::from(outcome != Outcome::NotAttempted)
        );
        let mutations: [fn(&mut Checked); 6] = [
            |value| value.cycles -= 1,
            |value| value.cycles += 1,
            |value| value.attempts += 1,
            |value| value.attempts = 0,
            |value| value.cycle_started_us -= 1,
            |value| value.next_due_us -= 1,
        ];
        for mutate in mutations {
            let mut invalid = checked.clone();
            mutate(&mut invalid);
            assert!(validate_checked(&step, Some(&previous), &invalid).is_err());
        }
    }
}

#[test]
fn logical_window_is_fixed_and_cannot_be_extended_by_a_later_call() {
    let scratch = Scratch::new();
    let (mut controller, peer) = setup(&scratch, |mut channel, log| {
        let mut previous = None;
        for at_us in (0..=8).map(|second| second * PERIOD_US) {
            let step: Step = channel.read_until(host_deadline()?)?;
            assert_eq!(step.at_us, at_us);
            let checked = receipt(&step, previous.as_ref(), Outcome::ReturnedOk);
            send_checked(&mut channel, &log, &checked)?;
            previous = Some(checked);
        }
        assert!(channel.read_until::<Step>(host_deadline()?).is_err());
        Ok(())
    });
    let start = controller.cadence_now();
    let window = controller.window().unwrap();
    let deadline = window.deadline;
    controller
        .advance_to(start + Duration::from_secs(8), &window)
        .unwrap();
    assert_eq!(window.started_us, 0);
    assert_eq!(window.through_us, 8_000_000);
    assert_eq!(window.deadline, deadline);
    assert!(
        controller
            .advance_to(start + Duration::from_micros(8_000_001), &window)
            .is_err()
    );
    assert_eq!(controller.cadence_now() - start, Duration::from_secs(8));
    assert_closed(&mut controller);
    peer.finish();
}

#[test]
fn expired_host_window_cannot_be_reset_by_a_new_step() {
    let scratch = Scratch::new();
    let (mut controller, peer) = setup(&scratch, |mut channel, _| {
        assert!(channel.read_until::<Step>(host_deadline()?).is_err());
        Ok(())
    });
    let mut window = controller.window().unwrap();
    window.deadline = Instant::now();
    assert_eq!(
        controller.settle(&window).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(controller.round, 0);
    assert_closed(&mut controller);
    peer.finish();
}

#[test]
fn a_window_from_another_controller_is_rejected_before_step() {
    let scratch_a = Scratch::new();
    let scratch_b = Scratch::new();
    let waiting = |mut channel: Channel, _: PathBuf| {
        assert!(channel.read_until::<Step>(host_deadline()?).is_err());
        Ok(())
    };
    let (controller_a, peer_a) = setup(&scratch_a, waiting);
    let (mut controller_b, peer_b) = setup(&scratch_b, waiting);
    let window = controller_a.window().unwrap();
    assert!(controller_b.settle(&window).is_err());
    assert_closed(&mut controller_b);
    drop(controller_a);
    peer_a.finish();
    peer_b.finish();
}

#[test]
fn targets_cannot_regress_round_microseconds_or_overflow() {
    let scratch = Scratch::new();
    for duration in [
        Duration::from_nanos(1),
        Duration::from_secs(9),
        Duration::MAX,
    ] {
        let (mut controller, peer) = setup(&scratch, |mut channel, _| {
            assert!(channel.read_until::<Step>(host_deadline()?).is_err());
            Ok(())
        });
        assert!(controller.wait_cadence(duration).is_err());
        assert_closed(&mut controller);
        peer.finish();
    }
    let (mut controller, peer) = setup(&scratch, |mut channel, log| {
        let mut previous = None;
        for at_us in [0, 100] {
            let step: Step = channel.read_until(host_deadline()?)?;
            assert_eq!(step.at_us, at_us);
            let checked = receipt(&step, previous.as_ref(), Outcome::ReturnedOk);
            send_checked(&mut channel, &log, &checked)?;
            previous = Some(checked);
        }
        assert!(channel.read_until::<Step>(host_deadline()?).is_err());
        Ok(())
    });
    let start = controller.cadence_now();
    let window = controller.window().unwrap();
    controller
        .advance_to(start + Duration::from_micros(100), &window)
        .unwrap();
    assert!(controller.advance_to(start, &window).is_err());
    assert_closed(&mut controller);
    peer.finish();
}

#[test]
fn counters_and_round_overflow_fail_without_wrapping() {
    let mut previous = receipt(
        &Step {
            nonce: NONCE.to_owned(),
            round: 1,
            at_us: 0,
        },
        None,
        Outcome::ReturnedOk,
    );
    previous.cycles = u64::MAX;
    let step = Step {
        nonce: NONCE.to_owned(),
        round: 2,
        at_us: PERIOD_US,
    };
    let mut value = previous.clone();
    value.round = step.round;
    value.at_us = step.at_us;
    value.marker = protocol::marker(NONCE, step.round).unwrap();
    value.cycle_started_us = step.at_us;
    value.next_due_us = step.at_us + PERIOD_US;
    value.cycles = 0;
    assert!(validate_checked(&step, Some(&previous), &value).is_err());
    previous.cycles = 1;
    previous.attempts = u64::MAX;
    value.cycles = 2;
    value.attempts = 0;
    assert!(validate_checked(&step, Some(&previous), &value).is_err());

    let scratch = Scratch::new();
    let (mut controller, peer) = setup(&scratch, |mut channel, _| {
        assert!(channel.read_until::<Step>(host_deadline()?).is_err());
        Ok(())
    });
    controller.round = u64::MAX;
    let window = controller.window().unwrap();
    assert!(controller.settle(&window).is_err());
    assert_closed(&mut controller);
    peer.finish();
}

#[test]
fn a_real_marker_requires_its_exact_complete_line() {
    let scratch = Scratch::new();
    let expected = protocol::marker(NONCE, 1).unwrap();
    let mut barrier = LogBarrier::open(&scratch.log()).unwrap();
    for bytes in [
        format!("prefix {expected}"),
        protocol::marker("OtherNonce", 1).unwrap(),
        protocol::marker(NONCE, 2).unwrap(),
        format!("{}\n", "x".repeat(16_384)),
    ] {
        append(&scratch.log(), bytes.as_bytes()).unwrap();
        assert!(
            !barrier
                .scan(expected.as_bytes(), host_deadline().unwrap())
                .unwrap()
        );
    }
    append(&scratch.log(), expected.trim_end_matches('\n').as_bytes()).unwrap();
    assert!(
        !barrier
            .scan(expected.as_bytes(), host_deadline().unwrap())
            .unwrap()
    );
    append(&scratch.log(), b"\n").unwrap();
    assert!(
        barrier
            .scan(expected.as_bytes(), host_deadline().unwrap())
            .unwrap()
    );
    assert!(
        !barrier
            .scan(expected.as_bytes(), host_deadline().unwrap())
            .unwrap()
    );
    assert!(barrier.line.capacity() <= protocol::MAX_MARKER_BYTES);
}

#[test]
fn marker_scanning_retains_a_partial_following_line() {
    let scratch = Scratch::new();
    let first = protocol::marker(NONCE, 1).unwrap();
    let second = protocol::marker(NONCE, 2).unwrap();
    let mut barrier = LogBarrier::open(&scratch.log()).unwrap();
    append(&scratch.log(), format!("{first}unfinished ").as_bytes()).unwrap();
    assert!(
        barrier
            .scan(first.as_bytes(), host_deadline().unwrap())
            .unwrap()
    );
    append(&scratch.log(), second.as_bytes()).unwrap();
    assert!(
        !barrier
            .scan(second.as_bytes(), host_deadline().unwrap())
            .unwrap()
    );
    append(&scratch.log(), second.as_bytes()).unwrap();
    assert!(
        barrier
            .scan(second.as_bytes(), host_deadline().unwrap())
            .unwrap()
    );
}

#[test]
fn missing_marker_after_child_advance_latches_closed_without_cadence_commit() {
    for partial in [false, true] {
        let scratch = Scratch::new();
        let (mut controller, peer) = setup(&scratch, move |mut channel, log| {
            let step: Step = channel.read_until(host_deadline()?)?;
            let checked = receipt(&step, None, Outcome::ReturnedOk);
            send_checked(&mut channel, &log, &checked)?;
            let next: Step = channel.read_until(host_deadline()?)?;
            assert_eq!(next.at_us, PERIOD_US);
            let advanced = receipt(&next, Some(&checked), Outcome::ReturnedOk);
            if partial {
                append(&log, advanced.marker.trim_end_matches('\n').as_bytes())?;
            }
            channel.write_until(&advanced, host_deadline()?)
        });
        let initial_window = controller.window().unwrap();
        controller.settle(&initial_window).unwrap();
        let at = controller.cadence_now();
        let mut short = controller.window().unwrap();
        short.deadline = Instant::now() + Duration::from_millis(20);
        assert_eq!(
            controller
                .advance_to(at + Duration::from_secs(1), &short)
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(controller.at_us, 0);
        assert_eq!(controller.round, 1);
        assert_eq!(controller.cadence_now(), at);
        assert_closed(&mut controller);
        peer.finish();
    }
}

#[test]
fn replacing_or_truncating_the_real_log_is_rejected() {
    for replace in [false, true] {
        let scratch = Scratch::new();
        let marker = protocol::marker(NONCE, 1).unwrap();
        let mut barrier = LogBarrier::open(&scratch.log()).unwrap();
        append(&scratch.log(), b"actual log line\n").unwrap();
        assert!(
            !barrier
                .scan(marker.as_bytes(), host_deadline().unwrap())
                .unwrap()
        );
        if replace {
            let moved = scratch.0.join("old-log");
            fs::rename(scratch.log(), moved).unwrap();
            File::create(scratch.log()).unwrap();
        } else {
            OpenOptions::new()
                .write(true)
                .open(scratch.log())
                .unwrap()
                .set_len(0)
                .unwrap();
        }
        assert!(
            barrier
                .scan(marker.as_bytes(), host_deadline().unwrap())
                .is_err()
        );
    }
}

#[test]
fn wrong_connected_identity_never_creates_a_controller() {
    for (version, nonce) in [
        (protocol::VERSION + 1, NONCE),
        (protocol::VERSION, "WrongNonce"),
    ] {
        let scratch = Scratch::new();
        let (parent, mut child) = UnixStream::pair().unwrap();
        protocol::write_frame(
            &mut child,
            &Connected {
                version,
                nonce: nonce.to_owned(),
            },
        )
        .unwrap();
        drop(child);
        assert!(
            Controller::connected(
                Channel::new(parent).unwrap(),
                NONCE.to_owned(),
                LogBarrier::open(&scratch.log()).unwrap(),
                host_deadline().unwrap(),
            )
            .is_err()
        );
    }
}

#[test]
fn connected_and_checked_queued_before_peer_close_are_read_completely() {
    let scratch = Scratch::new();
    let (parent, mut child) = UnixStream::pair().unwrap();
    protocol::write_frame(
        &mut child,
        &Connected {
            version: protocol::VERSION,
            nonce: NONCE.to_owned(),
        },
    )
    .unwrap();
    drop(child);
    let mut controller = Controller::connected(
        Channel::new(parent).unwrap(),
        NONCE.to_owned(),
        LogBarrier::open(&scratch.log()).unwrap(),
        host_deadline().unwrap(),
    )
    .unwrap();
    assert!(controller.checked.is_none());
    let window = controller.window().unwrap();
    assert!(controller.settle(&window).is_err());
    assert_closed(&mut controller);

    let (mut controller, peer) = setup(&scratch, |mut channel, log| {
        let step: Step = channel.read_until(host_deadline()?)?;
        send_checked(
            &mut channel,
            &log,
            &receipt(&step, None, Outcome::ReturnedOk),
        )
    });
    let window = controller.window().unwrap();
    controller.settle(&window).unwrap();
    peer.finish();
    assert_eq!(controller.checked.as_ref().unwrap().attempts, 1);
}

#[test]
fn malformed_truncated_and_oversized_receipts_close_control() {
    for bytes in [
        b"not json\n".to_vec(),
        b"{\"nonce\":".to_vec(),
        vec![b'x'; protocol::MAX_FRAME_BYTES + 1],
    ] {
        let scratch = Scratch::new();
        let (mut controller, peer) = setup(&scratch, move |mut channel, _| {
            let _: Step = channel.read_until(host_deadline()?)?;
            let mut raw = channel.try_clone_stream()?;
            write_pipe_until(&mut raw, &bytes, host_deadline()?)
        });
        let window = controller.window().unwrap();
        assert!(controller.settle(&window).is_err());
        assert!(controller.checked.is_none());
        assert_closed(&mut controller);
        peer.finish();
    }
}

#[test]
fn missing_reply_is_bounded_and_does_not_manufacture_report_progress() {
    let scratch = Scratch::new();
    let (mut controller, peer) = setup(&scratch, |mut channel, _| {
        let _: Step = channel.read_until(host_deadline()?)?;
        assert!(channel.read_until::<Step>(host_deadline()?).is_err());
        Ok(())
    });
    let mut window = controller.window().unwrap();
    window.deadline = Instant::now() + Duration::from_millis(20);
    assert_eq!(
        controller.settle(&window).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert!(controller.checked.is_none());
    assert_eq!(controller.round, 0);
    assert_closed(&mut controller);
    peer.finish();
}

#[test]
fn bootstrap_pipe_write_cannot_reset_an_expired_or_blocked_deadline() {
    let (mut writer, _reader) = UnixStream::pair().unwrap();
    writer.set_nonblocking(true).unwrap();
    let block = [0_u8; 4096];
    let fill_deadline = host_deadline().unwrap();
    loop {
        protocol::remaining(fill_deadline).unwrap();
        match writer.write(&block) {
            Ok(0) => panic!("socket stopped accepting bytes before WouldBlock"),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) => panic!("fill socket: {error}"),
        }
    }
    let deadline = Instant::now() + Duration::from_millis(20);
    assert_eq!(
        write_pipe_until(&mut writer, b"bootstrap\n", deadline)
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(
        write_pipe_until(&mut writer, b"bootstrap\n", Instant::now())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
}

#[test]
fn startup_validates_private_paths_and_expired_budget_before_spawn() {
    let scratch = Scratch::new();
    let mut command = Command::new("/a/nonexistent/memory-fixture-command");
    assert_eq!(
        start_until(&mut command, &scratch.0, &scratch.log(), Instant::now())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );
    fs::set_permissions(&scratch.0, Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        start(&mut command, &scratch.0, &scratch.log())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
    fs::set_permissions(&scratch.0, Permissions::from_mode(0o700)).unwrap();
    assert!(
        fs::read_dir(&scratch.0)
            .unwrap()
            .all(|entry| entry.unwrap().file_name() == "log")
    );
}

#[test]
fn startup_guard_kills_and_reaps_the_actual_owned_child() {
    let child = Command::new("/bin/sh")
        .args(["-c", "read value"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut guard = StartupChild(Some(child));
    let status = guard.stop().unwrap();
    assert_eq!(status.signal(), Some(9));
    assert!(guard.0.is_none());
    assert!(guard.stop().is_err());
}

#[test]
fn bootstrap_closes_after_one_frame_and_preserves_real_argv_and_child_ownership() {
    // This mode belongs only to this test executable's explicitly spawned copy.
    // The Memory fixture and ordinary binary have no environment activation.
    const CHILD_MODE: &str = "MEMORY_PARENT_BOUNDARY_TEST_CHILD";
    if std::env::var_os(CHILD_MODE).is_some() {
        let mut stdin = io::stdin().lock();
        let bootstrap: Bootstrap = protocol::read_frame(&mut stdin).unwrap();
        bootstrap.validate().unwrap();
        let mut extra = Vec::new();
        stdin.read_to_end(&mut extra).unwrap();
        assert!(extra.is_empty());
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        fs::write(
            bootstrap.control.parent().unwrap().join("argv"),
            serde_json::to_vec(&args).unwrap(),
        )
        .unwrap();
        let mut channel = Channel::new(UnixStream::connect(&bootstrap.control).unwrap()).unwrap();
        channel
            .write_until(
                &Connected {
                    version: protocol::VERSION,
                    nonce: bootstrap.nonce,
                },
                host_deadline().unwrap(),
            )
            .unwrap();
        let step: Step = channel.read_until(host_deadline().unwrap()).unwrap();
        let checked = receipt(&step, None, Outcome::NotAttempted);
        io::stderr().write_all(checked.marker.as_bytes()).unwrap();
        io::stderr().flush().unwrap();
        channel
            .write_until(&checked, host_deadline().unwrap())
            .unwrap();
        return;
    }
    let scratch = Scratch::new();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "bootstrap_closes_after_one_frame_and_preserves_real_argv_and_child_ownership",
            "--nocapture",
        ])
        .env(CHILD_MODE, "1")
        .stdout(Stdio::null())
        .stderr(File::create(scratch.log()).unwrap());
    let arguments: Vec<_> = command.get_args().map(|arg| arg.to_owned()).collect();
    let (child, mut controller) = start(&mut command, &scratch.0, &scratch.log()).unwrap();
    let mut child = StartupChild(Some(child));
    assert!(child.get_mut().unwrap().try_wait().unwrap().is_none());
    assert!(controller.checked.is_none());
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        arguments
            .iter()
            .map(|arg| arg.as_os_str())
            .collect::<Vec<_>>()
    );
    let received: Vec<std::ffi::OsString> =
        serde_json::from_slice(&fs::read(scratch.0.join("argv")).unwrap()).unwrap();
    assert_eq!(received, arguments);
    let window = controller.window().unwrap();
    controller.settle(&window).unwrap();
    assert_eq!(controller.checked.as_ref().unwrap().attempts, 0);
    let deadline = host_deadline().unwrap();
    let status = loop {
        protocol::remaining(deadline).unwrap();
        if let Some(status) = child.get_mut().unwrap().try_wait().unwrap() {
            break status;
        }
        thread::yield_now();
    };
    assert!(status.success());
    assert!(
        fs::read_to_string(scratch.log())
            .unwrap()
            .contains("memory-fixture barrier ")
    );
}
