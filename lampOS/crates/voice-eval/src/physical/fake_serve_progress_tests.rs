//! Progress helpers are tested without running the signal-owning serve loop.
use super::*;
use crate::{
    plan::{LoadedPlan, SERVE_PLAN},
    stimulus::StimulusCatalog,
};
use std::{cell::RefCell, sync::mpsc};

fn cycle() -> Cycle {
    let catalog = StimulusCatalog::load().unwrap();
    let plan = LoadedPlan::parse(SERVE_PLAN, &catalog).unwrap();
    let serve = FakeServe {
        scenario: plan.scenario("serve-conversation").unwrap().clone(),
        profile: plan.profile("v2-serve-v1-reopen").unwrap().clone(),
        fixed_reply: plan.plan.fixed_reply,
        seed: 7,
        output: PathBuf::from("/unused/serve-progress"),
        cue_path: PathBuf::from("/unused/serve-progress-cue"),
        room: None,
        cue_fault: None,
        faults: ServeFaults::default(),
    };
    Cycle::new(1, &serve, &|| MonoTime::from_micros(90_000))
}

fn sigterm_target() -> ServePollTargets {
    ServePollTargets {
        wait_for_sigterm: true,
    }
}

#[test]
fn signal_barrier_never_fabricates_an_observation_or_accepts_another_signal() {
    // These atomics are stand-ins for the handler state; no handler is registered.
    let signal = AtomicUsize::new(0);
    await_signal_observation(&signal, ServePollTargets::default(), Duration::ZERO).unwrap();
    assert_eq!(signal.load(Ordering::SeqCst), 0);

    let error = await_signal_observation(&signal, sigterm_target(), Duration::ZERO).unwrap_err();
    assert_eq!(
        error.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(signal.load(Ordering::SeqCst), 0);

    signal.store(signal_hook::consts::SIGINT as usize, Ordering::SeqCst);
    let error = await_signal_observation(&signal, sigterm_target(), Duration::ZERO).unwrap_err();
    assert!(error.to_string().contains("expected SIGTERM observation"));
    assert_eq!(
        signal.load(Ordering::SeqCst),
        signal_hook::consts::SIGINT as usize
    );

    signal.store(signal_hook::consts::SIGTERM as usize, Ordering::SeqCst);
    await_signal_observation(&signal, sigterm_target(), Duration::ZERO).unwrap();
}

#[test]
fn signal_barrier_waits_for_the_shared_atomic_under_a_host_deadline() {
    let signal = Arc::new(AtomicUsize::new(0));
    let worker_signal = Arc::clone(&signal);
    let (started, waiting) = mpsc::sync_channel(1);
    let (finished, result) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        started.send(()).unwrap();
        let outcome =
            await_signal_observation(&worker_signal, sigterm_target(), Duration::from_secs(5));
        finished.send(outcome).unwrap();
    });
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)));
    // The test owner publishes the stand-in observation; the barrier only reads it.
    signal.store(signal_hook::consts::SIGTERM as usize, Ordering::SeqCst);
    result
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    worker.join().unwrap();
    assert_eq!(
        signal.load(Ordering::SeqCst),
        signal_hook::consts::SIGTERM as usize
    );
}

#[derive(Default)]
struct Output {
    bytes: Vec<u8>,
    flushed: Vec<u8>,
    flushes: usize,
    fail_after: Option<usize>,
    fail_flush: bool,
}

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self
            .fail_after
            .map_or(usize::MAX, |limit| limit.saturating_sub(self.bytes.len()));
        if remaining == 0 {
            return Err(io::Error::other("test stdout write failed"));
        }
        // Short writes exercise writeln's complete-line behavior.
        let count = bytes.len().min(3).min(remaining);
        self.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        if self.fail_flush {
            return Err(io::Error::other("test stdout flush failed"));
        }
        self.flushed.clone_from(&self.bytes);
        Ok(())
    }
}

#[test]
fn stdout_counts_only_complete_lines_after_successful_flush() {
    let mut output = Output::default();
    let mut count = 0;
    let values = [
        json!({"status":"listening_ready","seconds":null}),
        json!({"kind":"service_metadata_finished","note":"escaped\nnewline"}),
    ];
    let mut expected = Vec::new();
    for (index, value) in values.iter().enumerate() {
        emit_stdout_line(&mut output, value, &mut count).unwrap();
        expected.extend_from_slice(format!("{value}\n").as_bytes());
        assert_eq!(output.flushed, expected);
        assert_eq!(output.flushes, index + 1);
        assert_eq!(count, (index + 1) as u64);
        assert_eq!(
            output.flushed.iter().filter(|&&b| b == b'\n').count(),
            index + 1
        );
    }

    let mut write_failure = Output {
        fail_after: Some(4),
        ..Output::default()
    };
    let mut count = 0;
    assert!(emit_stdout_line(&mut write_failure, &values[0], &mut count).is_err());
    assert_eq!(count, 0);
    assert_eq!(write_failure.bytes.len(), 4);
    assert_eq!(write_failure.flushes, 0);

    let mut flush_failure = Output {
        fail_flush: true,
        ..Output::default()
    };
    assert!(emit_stdout_line(&mut flush_failure, &values[0], &mut count).is_err());
    assert_eq!(count, 0);
    assert!(flush_failure.bytes.ends_with(b"\n"));
    assert!(flush_failure.flushed.is_empty());
}

#[test]
fn stdout_counter_overflow_fails_before_emitting_an_unannounced_line() {
    let mut output = Output::default();
    let mut count = u64::MAX;
    let error = emit_stdout_line(&mut output, &json!({"line":1}), &mut count).unwrap_err();
    assert!(error.to_string().contains("stdout-line counter overflow"));
    assert_eq!(count, u64::MAX);
    assert!(output.bytes.is_empty());
    assert_eq!(output.flushes, 0);
}

#[test]
fn room_progress_distinguishes_decoding_injection_and_muted_consumption() {
    let (sender, receiver) = UnixDatagram::pair().unwrap();
    receiver.set_nonblocking(true).unwrap();
    let timing = StimulusCatalog::load()
        .unwrap()
        .estimate("fast-followup")
        .unwrap();
    let bytes = serde_json::to_vec(&timing).unwrap();
    let mut cycle = cycle();
    let mut progress = ServePollState::default();
    let mut buffer = [0; ROOM_MAX_BYTES];

    sender.send(&bytes).unwrap();
    sender.send(&bytes).unwrap();
    receive_room(&receiver, &mut buffer, &mut cycle, false, &mut progress).unwrap();
    assert_eq!((progress.room_consumed, progress.room_injected), (2, 2));
    assert_eq!(cycle.runtime.now_us(), 0);

    sender.send(&bytes).unwrap();
    receive_room(&receiver, &mut buffer, &mut cycle, true, &mut progress).unwrap();
    assert_eq!((progress.room_consumed, progress.room_injected), (3, 2));
    receive_room(&receiver, &mut buffer, &mut cycle, false, &mut progress).unwrap();
    assert_eq!((progress.room_consumed, progress.room_injected), (3, 2));

    sender.send(b"{").unwrap();
    assert!(receive_room(&receiver, &mut buffer, &mut cycle, true, &mut progress).is_err());
    assert_eq!((progress.room_consumed, progress.room_injected), (3, 2));
}

#[test]
fn room_counter_overflow_never_wraps_or_claims_injection() {
    let timing = StimulusCatalog::load()
        .unwrap()
        .estimate("fast-followup")
        .unwrap();
    let bytes = serde_json::to_vec(&timing).unwrap();
    for consumed_overflow in [true, false] {
        let (sender, receiver) = UnixDatagram::pair().unwrap();
        receiver.set_nonblocking(true).unwrap();
        sender.send(&bytes).unwrap();
        let mut cycle = cycle();
        let mut progress = ServePollState {
            room_consumed: if consumed_overflow { u64::MAX } else { 0 },
            room_injected: if consumed_overflow { 0 } else { u64::MAX },
            ..ServePollState::default()
        };
        let error = receive_room(
            &receiver,
            &mut [0; ROOM_MAX_BYTES],
            &mut cycle,
            false,
            &mut progress,
        )
        .unwrap_err();
        assert!(error.to_string().contains("counter overflow"));
        assert_eq!(
            (progress.room_consumed, progress.room_injected),
            if consumed_overflow {
                (u64::MAX, 0)
            } else {
                (1, u64::MAX)
            }
        );
        assert_eq!(cycle.runtime.now_us(), 0);
    }
}

#[test]
fn completed_polls_keep_the_original_sample_and_cumulative_real_progress() {
    let mut cycle = cycle();
    let mut state = ServePollState {
        at_us: 100_000,
        stdout_lines: 1,
        room_consumed: 3,
        room_injected: 2,
        observed_signal: signal_hook::consts::SIGTERM as usize,
        ..ServePollState::default()
    };
    let polls = RefCell::new(Vec::new());
    let mut before = || Ok(ServePollTargets::default());
    let mut completed = |poll| {
        polls.borrow_mut().push(poll);
        Ok(())
    };
    let mut control = Some(ServePollControl {
        before: &mut before,
        completed: &mut completed,
    });
    complete_poll(&mut control, &mut state, &cycle, false).unwrap();
    cycle.runtime.tick();
    complete_poll(&mut control, &mut state, &cycle, false).unwrap();
    let mut output = Output::default();
    emit_stdout_line(
        &mut output,
        &json!({"kind":"service_metadata_finished"}),
        &mut state.stdout_lines,
    )
    .unwrap();
    complete_poll(&mut control, &mut state, &cycle, true).unwrap();
    let polls = polls.borrow();
    assert_eq!(polls.len(), 3);
    for (index, poll) in polls.iter().enumerate() {
        assert_eq!(poll.poll_seq, (index + 1) as u64);
        assert_eq!(poll.at, MonoTime::from_micros(100_000));
        assert_eq!(poll.cycle, 1);
        assert_eq!(poll.cycle_base_us, 90_000);
        assert_eq!(poll.room_consumed, 3);
        assert_eq!(poll.room_injected, 2);
        assert_eq!(poll.observed_signal, signal_hook::consts::SIGTERM as usize);
        assert_eq!(poll.terminal, index == 2);
    }
    assert_eq!(polls[0].runtime_us, 0);
    assert_eq!(polls[1].runtime_us, crate::fake::TICK_US);
    assert_eq!(polls[2].runtime_us, crate::fake::TICK_US);
    assert_eq!(polls[0].stdout_lines, 1);
    assert_eq!(polls[1].stdout_lines, 1);
    assert_eq!(polls[2].stdout_lines, 2);
}

#[test]
fn completion_errors_propagate_once_and_poll_sequence_overflow_is_refused() {
    let cycle = cycle();
    let calls = RefCell::new(0);
    let mut before = || Ok(ServePollTargets::default());
    let mut completed = |_| {
        *calls.borrow_mut() += 1;
        Err(invalid("test completion failure"))
    };
    let mut control = Some(ServePollControl {
        before: &mut before,
        completed: &mut completed,
    });
    let mut state = ServePollState::default();
    let error = complete_poll(&mut control, &mut state, &cycle, false).unwrap_err();
    assert!(error.to_string().contains("test completion failure"));
    assert_eq!(*calls.borrow(), 1);
    assert_eq!(state.poll_seq, 1);

    state.poll_seq = u64::MAX;
    let error = complete_poll(&mut control, &mut state, &cycle, true).unwrap_err();
    assert!(error.to_string().contains("poll sequence overflow"));
    assert_eq!(*calls.borrow(), 1);
    assert_eq!(state.poll_seq, u64::MAX);
}
