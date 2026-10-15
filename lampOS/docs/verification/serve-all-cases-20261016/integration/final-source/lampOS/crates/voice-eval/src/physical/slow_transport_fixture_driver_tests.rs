//! These scheduler tests use real control sockets with explicitly mock peers.
//! Their receipts prove ordering only: they are not exec, stdout, recording,
//! signal, datagram or owned-Child exit evidence. The command/integration gates
//! separately retain those real operations.
use super::*;
use crate::{
    physical::{fixture_control::Participant, session::RuntimeFlags},
    record::StimulusSource,
};
use std::{
    io,
    os::unix::fs::DirBuilderExt,
    sync::{Arc, Mutex, atomic::AtomicBool, mpsc},
    thread::JoinHandle,
    time::Duration,
};

const WATCHDOG: Duration = Duration::from_secs(30);
type Observations = Arc<Mutex<Vec<(Role, Step)>>>;

struct Private(PathBuf);

impl Private {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = PathBuf::from(format!(
            "/tmp/lve-transport-driver-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
}

impl Drop for Private {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn config() -> PhysicalConfig {
    PhysicalConfig {
        lamp_command: [
            "/unused/lamp-voice-eval",
            "lamp-session",
            "--runtime",
            "/unused/lamp-voice-eval",
            "--runtime-prefix-arg",
            "fake-lamp-voice",
            "--runtime-prefix-arg",
            "--fake-room",
            "--runtime-prefix-arg",
            "/private/room with spaces.sock",
            "--runtime-prefix-arg",
            "--fake-ignore-sigterm",
            "--runtime-prefix-arg",
            "yes",
        ]
        .map(str::to_owned)
        .into(),
        lamp_work_root: "/unused/work".into(),
        mode: LampMode::Serve {
            provider_config: "/unused/provider.json".into(),
        },
        noise_suppression: "on".into(),
        diagnostics: false,
        room_recorder: None,
        room_independent: false,
        allowance_seconds: 10,
        ring_channel_ceiling: None,
        runtime_flags: RuntimeFlags::default(),
        stimulus_source: StimulusSource::LoudspeakerSynthetic,
    }
}

fn wrapped_command(delay: &str) -> Vec<String> {
    let relay = config().lamp_command;
    let mut wrapper = vec![
        relay[0].clone(),
        "fake-slow-transport".into(),
        "--delay-ms".into(),
        delay.into(),
        "--".into(),
    ];
    wrapper.extend(relay);
    wrapper
}

#[derive(Clone, Copy)]
enum AfterDelay {
    Relay,
    NoRelay,
    WrongRole,
    CloseBeforeAck,
}

struct Harness {
    fixture: ClockedServeFixture,
    observations: Observations,
    closing: Arc<AtomicBool>,
    workers: Vec<JoinHandle<io::Result<()>>>,
    enqueued: AtomicU64,
    deadline: Instant,
    // Removed only after the fixture closes and every mock thread is joined.
    _directory: Private,
}

impl Harness {
    fn new(delay_ms: Option<u64>, recorder: bool) -> Self {
        let directory = Private::new();
        let mut fixture = ClockedServeFixture::new(&directory.0).unwrap();
        let mut config = config();
        if let Some(delay) = delay_ms {
            config.lamp_command = wrapped_command(&delay.to_string());
        }
        if recorder {
            config.room_recorder = Some(
                [
                    "/unused/lamp-voice-eval",
                    "fake-room-recorder",
                    "--out",
                    "{out}",
                    "--seconds",
                    "{seconds}",
                ]
                .map(str::to_owned)
                .into(),
            );
        }
        fixture.configure(&mut config).unwrap();
        Self {
            fixture,
            observations: Observations::default(),
            closing: Arc::new(AtomicBool::new(false)),
            workers: Vec::new(),
            enqueued: AtomicU64::new(0),
            deadline: Instant::now() + WATCHDOG,
            _directory: directory,
        }
    }

    fn now(&self) -> u64 {
        self.fixture.clock().now().as_micros()
    }

    fn advance(&mut self, until: u64, sent: u64) -> Result<()> {
        self.fixture.prepare_wait(
            MonoTime::from_micros(until),
            sent,
            &self.enqueued,
            true,
            self.deadline,
        )
    }

    fn spawn_serve(&mut self, after_delay: AfterDelay) {
        let delay = self.fixture.transport_delay_us;
        let path = self.fixture.control_path.clone();
        let observations = Arc::clone(&self.observations);
        let closing = Arc::clone(&self.closing);
        let (start_runtime, runtime_start) = mpsc::channel();
        if matches!(after_delay, AfterDelay::Relay) {
            let runtime_path = path.clone();
            let runtime_observations = Arc::clone(&observations);
            let runtime_closing = Arc::clone(&closing);
            self.workers.push(thread::spawn(move || {
                match runtime_start.recv_timeout(WATCHDOG) {
                    Ok(()) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected)
                        if runtime_closing.load(Ordering::Acquire) =>
                    {
                        return Ok(());
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "mock relay closed before the runtime startup gate",
                        ));
                    }
                    Err(error) => return Err(io::Error::other(error)),
                }
                let Some(mut participant) =
                    connect(&runtime_path, Role::ServeRuntime, &runtime_closing)?
                else {
                    return Ok(());
                };
                let base = participant.current_step().at_us;
                loop {
                    let step = observe(&participant, Role::ServeRuntime, &runtime_observations);
                    let progress = Progress::ServeRuntime {
                        poll_seq: step.round,
                        stdout_frames: 0,
                        room_consumed: 0,
                        room_injected: 0,
                        cycle: 1,
                        cycle_base_us: base,
                        runtime_us: step.at_us - base,
                        observed_signal: 0,
                        logical_end: false,
                    };
                    if !next_or_closed(&mut participant, progress, &runtime_closing)? {
                        return Ok(());
                    }
                }
            }));
        }
        self.workers.push(thread::spawn(move || {
            if let Some(delay) = delay {
                let Some(mut participant) = connect(&path, Role::SlowTransport, &closing)? else {
                    return Ok(());
                };
                loop {
                    let step = observe(&participant, Role::SlowTransport, &observations);
                    assert_eq!(step.stdout_target, 0);
                    assert_eq!(step.runner_input_target, 0);
                    if matches!(after_delay, AfterDelay::CloseBeforeAck) {
                        return Ok(());
                    }
                    let progress = Progress::SlowTransport {
                        poll_seq: step.round,
                        wait_started_us: EPOCH_US,
                        exec_due_us: EPOCH_US + delay,
                        logical_end: step.at_us >= EPOCH_US + delay,
                    };
                    if progress.logical_end() {
                        participant.final_ack(progress)?;
                        break;
                    }
                    if !next_or_closed(&mut participant, progress, &closing)? {
                        return Ok(());
                    }
                }
                // A genuinely new connection is used after the mock delay Ack.
                // This deliberately does not pretend to be a real exec.
                drop(participant);
            }
            match after_delay {
                AfterDelay::NoRelay | AfterDelay::CloseBeforeAck => return Ok(()),
                AfterDelay::WrongRole => {
                    let result =
                        Participant::connect_with_watchdog(&path, Role::ServeRuntime, WATCHDOG);
                    assert!(matches!(result, Err(ref error) if closed_error(error)));
                    return Ok(());
                }
                AfterDelay::Relay => {}
            }
            let Some(mut participant) = connect(&path, Role::ServeRelay, &closing)? else {
                return Ok(());
            };
            start_runtime.send(()).map_err(io::Error::other)?;
            loop {
                let step = observe(&participant, Role::ServeRelay, &observations);
                let progress = Progress::ServeRelay {
                    poll_seq: step.round,
                    // Explicit mock receipts: no product pipe I/O is simulated
                    // by, or inferred from, these scheduler observations.
                    stdout_frames: step.stdout_target,
                    runner_inputs: step.runner_input_target,
                    output_frames: 0,
                    stop_signal_sent_us: None,
                    logical_end: false,
                };
                if !next_or_closed(&mut participant, progress, &closing)? {
                    return Ok(());
                }
            }
        }));
    }

    fn spawn_recorder(&mut self, due: u64) {
        let path = self.fixture.control_path.clone();
        let observations = Arc::clone(&self.observations);
        let closing = Arc::clone(&self.closing);
        self.workers.push(thread::spawn(move || {
            let Some(mut participant) = connect(&path, Role::Recorder, &closing)? else {
                return Ok(());
            };
            loop {
                let step = observe(&participant, Role::Recorder, &observations);
                let progress = Progress::Recorder {
                    poll_seq: step.round,
                    ready_written: true,
                    wait_started_us: EPOCH_US,
                    exit_due_us: due,
                    logical_end: step.at_us >= due,
                };
                if progress.logical_end() {
                    return participant.final_ack(progress);
                }
                if !next_or_closed(&mut participant, progress, &closing)? {
                    return Ok(());
                }
            }
        }));
        self.fixture.start_recorder(self.deadline).unwrap();
    }

    fn advance_to_start(&mut self) {
        // A finite cap detects a scheduler that returns without advancing.
        for _ in 0..205 {
            if self.fixture.started {
                return;
            }
            let before = self.now();
            // The unchanged backend writes its first Ping at EPOCH and next
            // at +250 ms. Only the announced counter is supplied here.
            let sent = if before >= EPOCH_US + 250_000 { 2 } else { 1 };
            self.advance(before + POLL_US, sent).unwrap();
            assert!(self.now() > before && self.now() - before <= POLL_US);
        }
        panic!("mock serve did not start within the complete configured delay");
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        // Release every next-Step read and the relay-to-runtime startup gate
        // on success, failure or assertion unwind before joining any worker.
        self.closing.store(true, Ordering::Release);
        self.fixture.close();
        let mut failures = Vec::new();
        for worker in self.workers.drain(..) {
            match worker.join() {
                Ok(Ok(())) => {}
                // Negative driver cases close peers before returning their
                // checked error; that closure can reach a mock before Drop.
                Ok(Err(error)) if self.fixture.failed && closed_error(&error) => {}
                Ok(Err(error)) => failures.push(error.to_string()),
                Err(_) => failures.push("mock worker panicked".into()),
            }
        }
        if !thread::panicking() {
            assert!(failures.is_empty(), "mock worker failures: {failures:?}");
        }
    }
}

fn closed_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::UnexpectedEof
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
    )
}

fn connect(path: &Path, role: Role, closing: &AtomicBool) -> io::Result<Option<Participant>> {
    match Participant::connect_with_watchdog(path, role, WATCHDOG) {
        Ok(peer) => Ok(Some(peer)),
        Err(error) if closing.load(Ordering::Acquire) && closed_error(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

fn next_or_closed(
    participant: &mut Participant,
    progress: Progress,
    closing: &AtomicBool,
) -> io::Result<bool> {
    match participant.complete(progress) {
        Ok(_) => Ok(true),
        Err(error) if closing.load(Ordering::Acquire) && closed_error(&error) => Ok(false),
        Err(error) => Err(error),
    }
}

fn observe(participant: &Participant, role: Role, observations: &Observations) -> Step {
    let step = participant.current_step();
    assert_eq!(participant.clock().now().as_micros(), step.at_us);
    assert_eq!(participant.clock().now().as_micros(), step.at_us);
    assert!(!step.wait_for_sigterm);
    observations.lock().unwrap().push((role, step));
    step
}

fn steps(observations: &Observations, role: Role) -> Vec<Step> {
    observations
        .lock()
        .unwrap()
        .iter()
        .filter_map(|(observed, step)| (*observed == role).then_some(*step))
        .collect()
}

fn times(observations: &Observations, role: Role) -> Vec<u64> {
    steps(observations, role)
        .iter()
        .map(|step| step.at_us)
        .collect()
}

#[test]
fn direct_configuration_and_bootstrap_keep_their_original_path() {
    let original = config().lamp_command;
    let path = Path::new("/private/transport control.sock");
    assert_eq!(
        controlled_transport(&original, path).unwrap(),
        (controlled_command(&original, path).unwrap(), None)
    );
    let mut harness = Harness::new(None, false);
    harness.spawn_serve(AfterDelay::Relay);
    harness.advance(BOOTSTRAP_US, 1).unwrap();
    assert_eq!(harness.now(), BOOTSTRAP_US);
    assert!(harness.fixture.started);
    assert!(harness.fixture.transport.is_none());
    assert!(harness.fixture.transport_until.is_none());
    assert_eq!(harness.fixture.next_poll_us, BOOTSTRAP_US + POLL_US);
    assert_eq!(
        times(&harness.observations, Role::ServeRelay),
        [BOOTSTRAP_US; 2]
    );
    assert_eq!(
        times(&harness.observations, Role::ServeRuntime),
        [BOOTSTRAP_US]
    );
    assert!(steps(&harness.observations, Role::SlowTransport).is_empty());
}

#[test]
fn explicit_wrapper_preserves_the_full_delay_and_every_forwarded_argument() {
    let original = wrapped_command("000400");
    let path = Path::new("/private/transport control.sock");
    let (changed, delay) = controlled_transport(&original, path).unwrap();
    assert_eq!(delay, Some(400_000));
    assert_eq!(
        &changed[..7],
        [
            original[0].as_str(),
            "fake-clock-slow-transport",
            "--fixture-control",
            path.to_str().unwrap(),
            "--delay-ms",
            "000400",
            "--",
        ]
    );
    assert_eq!(
        &changed[7..],
        controlled_command(&original[5..], path).unwrap()
    );
    assert!(
        changed
            .iter()
            .any(|arg| arg == "/private/room with spaces.sock")
    );
    assert!(changed.iter().any(|arg| arg == "--fake-ignore-sigterm"));
}

#[test]
fn malformed_or_unbounded_wrappers_fail_before_mutating_configuration() {
    let original = wrapped_command("400");
    let mut invalid = vec![Vec::new(), original[..5].to_vec()];
    for (index, value) in [
        (0, "/bin/sh"),
        (2, "--wait-ms"),
        (3, "-1"),
        (3, "18446744073709551615"),
        (3, "18446744073709"),
        (4, "not-a-delimiter"),
        (5, "/other/lamp-voice-eval"),
        (6, "fake-slow-transport"),
        (8, "/unused/lamp-live"),
    ] {
        let mut command = original.clone();
        command[index] = value.into();
        invalid.push(command);
    }
    let directory = Private::new();
    let mut fixture = ClockedServeFixture::new(&directory.0).unwrap();
    for command in invalid {
        let mut config = config();
        config.lamp_command = command.clone();
        assert!(fixture.configure(&mut config).is_err(), "{command:?}");
        assert_eq!(config.lamp_command, command);
        assert!(!fixture.configured);
        assert!(fixture.transport_delay_us.is_none());
        assert_eq!(fixture.clock().now().as_micros(), EPOCH_US);
    }
}

#[test]
fn four_hundred_ms_wait_uses_two_ms_rounds_and_retains_both_setup_inputs() {
    let mut harness = Harness::new(Some(400), false);
    harness.spawn_serve(AfterDelay::Relay);
    harness.advance_to_start();
    let due = EPOCH_US + 400_000;
    assert_eq!(harness.now(), due);
    assert_eq!(
        times(&harness.observations, Role::SlowTransport),
        (0..=200)
            .map(|poll| EPOCH_US + poll * POLL_US)
            .collect::<Vec<_>>()
    );
    let transport = harness.fixture.transport.as_ref().unwrap();
    assert_eq!(transport.round, 201);
    assert_eq!(transport.wait_started_us, Some(EPOCH_US));
    assert_eq!(transport.exec_due_us, Some(due));
    assert!(transport.logical_end);
    assert_eq!(times(&harness.observations, Role::ServeRelay), [due; 2]);
    assert_eq!(times(&harness.observations, Role::ServeRuntime), [due]);
    assert!(
        steps(&harness.observations, Role::ServeRelay)
            .iter()
            .all(|step| step.runner_input_target == 2 && step.stdout_target == 0)
    );
    assert_eq!(harness.fixture.runner_inputs, 2);
    assert_eq!(harness.fixture.next_poll_us, due + POLL_US);
    assert!(harness.fixture.transport_until.is_none());
    harness.advance(due + POLL_US, 2).unwrap();
    assert_eq!(
        times(&harness.observations, Role::ServeRuntime),
        [due, due + POLL_US]
    );
    assert_eq!(steps(&harness.observations, Role::SlowTransport).len(), 201);
}

#[test]
fn zero_and_sub_poll_delays_register_a_fresh_relay_before_bootstrap_time() {
    for delay_ms in [0, 1] {
        let mut harness = Harness::new(Some(delay_ms), false);
        harness.spawn_serve(AfterDelay::Relay);
        let due = EPOCH_US + delay_ms * 1_000;
        let first_until = if delay_ms == 0 { EPOCH_US + 1 } else { due };
        harness.advance(first_until, 1).unwrap();
        assert_eq!(harness.now(), first_until);
        assert!(harness.fixture.transport.as_ref().unwrap().logical_end);
        assert!(harness.fixture.relay.is_some());
        assert_eq!(harness.fixture.relay_round, 0);
        assert!(!harness.fixture.started);
        assert!(steps(&harness.observations, Role::ServeRelay).is_empty());
        harness.advance(BOOTSTRAP_US, 1).unwrap();
        assert!(harness.fixture.started);
        assert_eq!(harness.now(), BOOTSTRAP_US);
        assert_eq!(
            times(&harness.observations, Role::ServeRelay),
            [BOOTSTRAP_US; 2]
        );
        assert_eq!(
            times(&harness.observations, Role::ServeRuntime),
            [BOOTSTRAP_US]
        );
        assert_eq!(
            times(&harness.observations, Role::SlowTransport),
            if delay_ms == 0 {
                vec![EPOCH_US]
            } else {
                vec![EPOCH_US, due]
            }
        );
    }
}

#[test]
fn delay_completion_followed_by_the_wrong_role_fails_at_the_held_due_time() {
    for delay_ms in [0, 1] {
        let mut harness = Harness::new(Some(delay_ms), false);
        harness.spawn_serve(AfterDelay::WrongRole);
        let error = harness.advance(BOOTSTRAP_US, 1).unwrap_err();
        assert!(error.to_string().contains("ServeRelay"), "{error}");
        assert_eq!(harness.now(), EPOCH_US + delay_ms * 1_000);
        assert!(!harness.fixture.started);
        assert!(harness.fixture.failed);
        assert!(harness.fixture.relay.is_none());
        assert!(harness.fixture.runtime.is_none());
        assert!(!harness.fixture.control_path.exists());
        assert!(steps(&harness.observations, Role::ServeRelay).is_empty());
    }
}

#[test]
fn terminal_delay_without_a_fresh_relay_hello_cannot_start_or_advance() {
    let mut harness = Harness::new(Some(0), false);
    harness.spawn_serve(AfterDelay::NoRelay);
    // Install a real terminal delay receipt without entering the separate
    // accept barrier yet. The short watchdog below therefore tests only the
    // absent new Hello, with no thread-startup race against that watchdog.
    let peer = harness
        .fixture
        .listener
        .as_ref()
        .unwrap()
        .accept_until(Role::SlowTransport, harness.deadline)
        .unwrap();
    harness.fixture.transport = Some(SlowTransportControl {
        peer,
        round: 0,
        wait_started_us: None,
        exec_due_us: None,
        logical_end: false,
    });
    harness.fixture.transport_step(harness.deadline).unwrap();
    assert!(harness.fixture.transport.as_ref().unwrap().logical_end);
    let error = harness
        .fixture
        .prepare_wait(
            MonoTime::from_micros(BOOTSTRAP_US),
            1,
            &harness.enqueued,
            true,
            Instant::now() + Duration::from_millis(100),
        )
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(harness.now(), EPOCH_US);
    assert!(!harness.fixture.started);
    assert!(harness.fixture.failed);
    assert!(harness.fixture.listener.is_none());
    assert!(steps(&harness.observations, Role::ServeRelay).is_empty());
}

#[test]
fn recorder_due_before_or_on_delay_completion_holds_time_until_exit_notification() {
    let due = EPOCH_US + 400_000;
    for recorder_due in [due - 1, due] {
        let mut harness = Harness::new(Some(400), true);
        harness.spawn_recorder(recorder_due);
        harness.spawn_serve(AfterDelay::Relay);
        for _ in 0..201 {
            if harness.fixture.recorder_exit_pending() {
                break;
            }
            let now = harness.now();
            harness.advance(now + POLL_US, 2).unwrap();
        }
        assert_eq!(harness.now(), recorder_due);
        assert!(harness.fixture.recorder_exit_pending());
        assert!(!harness.fixture.started);
        assert!(steps(&harness.observations, Role::ServeRelay).is_empty());
        assert!(steps(&harness.observations, Role::ServeRuntime).is_empty());
        assert_eq!(
            harness.fixture.transport.as_ref().unwrap().logical_end,
            recorder_due == due
        );
        assert_eq!(harness.fixture.relay.is_some(), recorder_due == due);
        let snapshot = harness.observations.lock().unwrap().clone();
        let recorder_index = snapshot
            .iter()
            .position(|(role, step)| *role == Role::Recorder && step.at_us == recorder_due)
            .unwrap();
        let transport_index = snapshot
            .iter()
            .position(|(role, step)| *role == Role::SlowTransport && step.at_us == recorder_due)
            .unwrap();
        assert!(recorder_index < transport_index);
        assert!(harness.fixture.require_recorder_exit_observed().is_err());
        assert_eq!(harness.now(), recorder_due);
        // This explicitly mocks the separate backend notification after its
        // real Child::try_wait; the terminal control Ack did not provide it.
        harness.fixture.recorder_exit_observed().unwrap();
        harness.advance(due + POLL_US, 2).unwrap();
        assert!(harness.fixture.started);
        assert_eq!(harness.now(), due);
        assert!(!harness.fixture.recorder_exit_pending());
        assert_eq!(times(&harness.observations, Role::ServeRelay), [due; 2]);
        assert_eq!(times(&harness.observations, Role::ServeRuntime), [due]);
        assert_eq!(
            times(&harness.observations, Role::Recorder).last(),
            Some(&recorder_due)
        );
    }
}

#[test]
fn advancing_without_the_separate_recorder_exit_notification_fails_closed() {
    let mut harness = Harness::new(Some(400), true);
    let due = EPOCH_US + POLL_US;
    harness.spawn_recorder(due);
    harness.spawn_serve(AfterDelay::Relay);
    harness.advance(due, 1).unwrap();
    assert!(harness.fixture.recorder_exit_pending());
    let before = steps(&harness.observations, Role::SlowTransport);
    let error = harness.advance(due + POLL_US, 1).unwrap_err();
    assert!(error.to_string().contains("actual child exit"));
    assert_eq!(harness.now(), due);
    assert_eq!(steps(&harness.observations, Role::SlowTransport), before);
    assert!(harness.fixture.failed);
    assert!(!harness.fixture.started);
    assert!(harness.fixture.listener.is_none());
}

#[test]
fn later_caller_deadlines_cannot_extend_the_cumulative_startup_budget() {
    let mut harness = Harness::new(Some(400), false);
    harness.spawn_serve(AfterDelay::Relay);
    let original = harness.deadline;
    harness.advance(BOOTSTRAP_US, 1).unwrap();
    assert_eq!(harness.fixture.transport_until, Some(original));
    harness.deadline = original + WATCHDOG;
    harness.advance(BOOTSTRAP_US + POLL_US, 1).unwrap();
    assert_eq!(harness.fixture.transport_until, Some(original));
    let shorter = original - Duration::from_secs(5);
    harness.deadline = shorter;
    harness.advance(BOOTSTRAP_US + 2 * POLL_US, 1).unwrap();
    assert_eq!(harness.fixture.transport_until, Some(shorter));
    harness.deadline = original + WATCHDOG;
    harness.advance(BOOTSTRAP_US + 3 * POLL_US, 1).unwrap();
    assert_eq!(harness.fixture.transport_until, Some(shorter));
    assert!(!harness.fixture.started);
}

#[test]
fn an_expired_retained_startup_budget_cannot_be_renewed_by_the_next_call() {
    let mut harness = Harness::new(Some(400), false);
    harness.spawn_serve(AfterDelay::Relay);
    harness.advance(BOOTSTRAP_US, 1).unwrap();
    let before = steps(&harness.observations, Role::SlowTransport);
    // Expire the stored absolute Instant directly, without a timing guess.
    let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    harness.fixture.transport_until = Some(expired);
    harness.deadline = Instant::now() + WATCHDOG;
    let error = harness.advance(BOOTSTRAP_US + POLL_US, 1).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("serve progress exceeded its host watchdog")
    );
    assert_eq!(harness.fixture.transport_until, Some(expired));
    assert_eq!(harness.now(), BOOTSTRAP_US);
    assert_eq!(steps(&harness.observations, Role::SlowTransport), before);
    assert!(harness.fixture.failed);
    assert!(!harness.fixture.started);
    assert!(harness.fixture.listener.is_none());
}

#[test]
fn an_already_expired_first_call_never_registers_or_advances_the_wrapper() {
    let mut harness = Harness::new(Some(400), false);
    harness.deadline = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    let error = harness.advance(BOOTSTRAP_US, 1).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("serve progress exceeded its host watchdog")
    );
    assert_eq!(harness.now(), EPOCH_US);
    assert!(!harness.fixture.started);
    assert!(harness.fixture.failed);
    assert!(harness.fixture.transport.is_none());
    assert!(harness.fixture.listener.is_none());
    assert!(!harness.fixture.control_path.exists());
}

#[test]
fn a_wrapper_disconnect_before_its_first_ack_cannot_start_the_relay() {
    let mut harness = Harness::new(Some(400), false);
    harness.spawn_serve(AfterDelay::CloseBeforeAck);
    let error = harness.advance(BOOTSTRAP_US, 1).unwrap_err();
    assert_eq!(
        error.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::UnexpectedEof
    );
    assert_eq!(harness.now(), EPOCH_US);
    assert!(harness.fixture.failed);
    assert!(!harness.fixture.started);
    assert_eq!(
        times(&harness.observations, Role::SlowTransport),
        [EPOCH_US]
    );
    assert!(steps(&harness.observations, Role::ServeRelay).is_empty());
}

#[test]
fn explicit_close_releases_a_parked_wrapper_and_prevents_further_startup() {
    let mut harness = Harness::new(Some(400), false);
    harness.spawn_serve(AfterDelay::Relay);
    harness.advance(BOOTSTRAP_US, 1).unwrap();
    let before = steps(&harness.observations, Role::SlowTransport);
    harness.closing.store(true, Ordering::Release);
    harness.fixture.close();
    assert!(harness.advance(BOOTSTRAP_US + POLL_US, 1).is_err());
    assert_eq!(harness.now(), BOOTSTRAP_US);
    assert_eq!(steps(&harness.observations, Role::SlowTransport), before);
    assert!(harness.fixture.failed);
    assert!(!harness.fixture.started);
    assert!(!harness.fixture.control_path.exists());
}
