//! Selected Controls privacy wait. Only observation and a narrow muted display
//! exist here. Audio starts only after both previous workers have exited.
use crate::{
    privacy::{PRIVACY_FRESH_US, PrivacyGate, PrivacyState},
    privacy_bus,
    process::{RingStartup, SessionDirectory, Worker, new_boot},
    ring_wire::{RingBlankReason, RingFeedback},
    service_privacy::{self, Result, WaitOutcome, finish},
    termination::Termination,
    wire::{Control, WorkerEvent},
};
use lamp_ipc::monotonic_us;
use lamp_turn_owner::{BootId, Controller, MonoTime, Permission};
use std::{
    fs, io,
    path::Path,
    time::{Duration, Instant},
};

pub(crate) fn wait_selected(
    executable: &Path,
    termination: &Termination,
    on_state: impl FnMut(PrivacyState, BootId, u64) -> Result<()>,
    additional_gate: impl FnMut() -> Result<bool>,
) -> Result<WaitOutcome> {
    match privacy_bus::root_from_environment()? {
        None => service_privacy::wait_with_gate(executable, termination, on_state, additional_gate),
        Some(root) => wait(executable, &root, termination, on_state, additional_gate),
    }
}

fn wait(
    executable: &Path,
    root: &Path,
    termination: &Termination,
    on_state: impl FnMut(PrivacyState, BootId, u64) -> Result<()>,
    mut additional_gate: impl FnMut() -> Result<bool>,
) -> Result<WaitOutcome> {
    // The durable software latch was restored by the caller before this point.
    // Unlike the legacy path, the selected slide is observed even during a
    // software mute. This creates neither capture workers nor permission.
    additional_gate()?;
    termination.check()?;
    let directory = SessionDirectory::create()?;
    let boot = new_boot()?;
    let args = privacy_bus::worker_arguments(Some(root))?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let worker =
        Worker::spawn_with_tick(executable, &directory.path, "privacy", boot, &args, || {
            termination.check()
        })?;
    let path = directory.path.clone();
    // Optional SPI startup is isolated while the mandatory privacy worker,
    // gate and controller continue on every bounded tick.
    let start_ring = |tick: &mut dyn FnMut() -> io::Result<()>| {
        let root_text = root
            .to_str()
            .ok_or_else(|| io::Error::other("privacy root must be UTF-8"))?;
        Worker::spawn_optional_ring_with_tick(
            executable,
            &path,
            boot,
            &["--privacy", "--bus-root", root_text],
            tick,
        )
    };
    run(
        worker,
        directory,
        boot,
        termination,
        on_state,
        additional_gate,
        start_ring,
    )
}

/// The muted wait itself, with the optional ring's start injected.
fn run(
    mut worker: Worker,
    directory: SessionDirectory,
    boot: BootId,
    termination: &Termination,
    mut on_state: impl FnMut(PrivacyState, BootId, u64) -> Result<()>,
    mut additional_gate: impl FnMut() -> Result<bool>,
    mut start_ring: impl FnMut(&mut dyn FnMut() -> io::Result<()>) -> io::Result<RingStartup>,
) -> Result<WaitOutcome> {
    let mut waiting = Waiting::new(boot)?;
    let mut indicator: Option<Box<Worker>> = None;
    let mut attempted = false;
    let mut indicator_failure = None;
    let mut indicator_failed_at = None;
    let mut indicator_kill_requested = false;
    let result = (|| {
        loop {
            if termination.signal_name().is_some() {
                return Ok(WaitOutcome::Stopped);
            }
            let state = waiting.step(
                &mut worker,
                if indicator_failure.is_none() {
                    indicator.as_deref_mut()
                } else {
                    None
                },
                &mut on_state,
                &mut additional_gate,
            )?;
            if state.allowed {
                return Ok(WaitOutcome::Allowed);
            }
            if state.physical == PrivacyState::Muted && !attempted {
                attempted = true;
                let started = start_ring(&mut || {
                    termination.check()?;
                    waiting
                        .step(&mut worker, None, &mut on_state, &mut additional_gate)
                        .map(|_| ())
                        .map_err(|error| io::Error::other(error.to_string()))
                })?;
                match started {
                    RingStartup::Ready(child) => indicator = Some(child),
                    // No writer process exists, so no SPI was opened and no
                    // black write is owed: the wait goes on without its light.
                    // An optional light never ends a healthy voice.
                    RingStartup::Unavailable {
                        worker: None,
                        error,
                    } => {
                        eprintln!("lamp-voice: muted indication unavailable: {error}");
                    }
                    // A started writer keeps the verified black write and exit
                    // requirement before anything replaces it.
                    RingStartup::Unavailable {
                        worker: Some(mut child),
                        error,
                    } => {
                        eprintln!("lamp-voice: muted indication unavailable: {error}");
                        indicator_failure = Some(error);
                        indicator_failed_at = Some(monotonic_us());
                        let _ = child.channels.control.send(Control::Stop);
                        indicator = Some(child);
                    }
                }
            }
            if let Some(child) = &mut indicator {
                if indicator_failure.is_none() {
                    if let Err(error) = child.check_running() {
                        indicator_failure = Some(error);
                    } else {
                        for _ in 0..16 {
                            match child.channels.control.receive::<WorkerEvent>() {
                                Ok(Some(WorkerEvent::Ring {
                                    report: RingFeedback::Blanked { .. },
                                })) => {}
                                Ok(None) => break,
                                Ok(Some(event)) => {
                                    indicator_failure = Some(io::Error::other(format!(
                                        "unexpected privacy indicator event: {event:?}"
                                    )));
                                    break;
                                }
                                Err(error) => {
                                    indicator_failure = Some(error);
                                    break;
                                }
                            }
                        }
                    }
                    if let Some(error) = &indicator_failure {
                        indicator_failed_at = Some(monotonic_us());
                        eprintln!("lamp-voice: muted indication failed: {error}");
                        let _ = child.channels.control.send(Control::Stop);
                    }
                }
                if indicator_failure.is_some()
                    && !indicator_kill_requested
                    && indicator_failed_at
                        .is_some_and(|at| monotonic_us().saturating_sub(at) >= 300_000)
                    && !matches!(child.try_exit(), Ok(Some(_)))
                {
                    indicator_kill_requested = true;
                    if let Err(error) = child.request_termination() {
                        eprintln!("lamp-voice: privacy indicator termination failed: {error}");
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    })();
    let cleanup = cleanup(&mut worker, indicator.as_deref_mut());
    drop(indicator);
    drop(worker);
    let result = finish(result, cleanup);
    // Keep a failed writer's evidence distinct from permission to start a new
    // one, even when its process was subsequently reaped.
    let result = finish(result, indicator_failure.map_or(Ok(()), Err));
    finish(result, fs::remove_dir(&directory.path))
}

struct Step {
    physical: PrivacyState,
    allowed: bool,
}

struct Waiting {
    controller: Controller,
    boot: BootId,
    gate: PrivacyGate,
    sequence: u64,
    previous: Option<PrivacyState>,
    last_publish: Option<u64>,
    started: Instant,
}

impl Waiting {
    fn new(boot: BootId) -> Result<Self> {
        let mut controller = Controller::new(boot, MonoTime::from_micros(monotonic_us()));
        controller
            .set_microphone_permission(MonoTime::from_micros(monotonic_us()), Permission::Denied)?;
        controller
            .set_camera_permission(MonoTime::from_micros(monotonic_us()), Permission::Denied)?;
        Ok(Self {
            controller,
            boot,
            gate: PrivacyGate::default(),
            sequence: 0,
            previous: None,
            last_publish: None,
            started: Instant::now(),
        })
    }

    fn step(
        &mut self,
        worker: &mut Worker,
        indicator: Option<&mut Worker>,
        on_state: &mut impl FnMut(PrivacyState, BootId, u64) -> Result<()>,
        additional_gate: &mut impl FnMut() -> Result<bool>,
    ) -> Result<Step> {
        // Slow durable software-clear work happens while no audio exists. Age
        // the physical evidence again afterward; never reuse an old Allowed.
        let software_allowed = additional_gate()?;
        worker.check_running()?;
        let at = monotonic_us();
        if self.sequence != 0
            && self.gate.state(MonoTime::from_micros(at)) == PrivacyState::Unavailable
        {
            return Err(io::Error::other("Controls privacy freshness lost while waiting").into());
        }
        if self
            .last_publish
            .is_none_or(|last| at.saturating_sub(last) >= 25_000)
        {
            let snapshot = self.controller.snapshot(MonoTime::from_micros(at))?;
            worker
                .channels
                .control
                .send(Control::Authority { snapshot })?;
            if let Some(indicator) = indicator {
                let _ = indicator
                    .channels
                    .control
                    .send(Control::Authority { snapshot });
            }
            self.last_publish = Some(at);
        }
        let mut empty = false;
        for _ in 0..16 {
            match worker.channels.control.receive::<WorkerEvent>()? {
                Some(WorkerEvent::Privacy {
                    acquired_at_us,
                    muted,
                }) => {
                    self.sequence = self
                        .sequence
                        .checked_add(1)
                        .ok_or_else(|| io::Error::other("privacy observation count exhausted"))?;
                    self.gate.observe(
                        self.sequence,
                        MonoTime::from_micros(acquired_at_us),
                        MonoTime::from_micros(monotonic_us()),
                        muted,
                    );
                    if self.gate.state(MonoTime::from_micros(monotonic_us()))
                        == PrivacyState::Unavailable
                    {
                        return Err(io::Error::other(
                            "invalid Controls privacy observation while waiting",
                        )
                        .into());
                    }
                }
                Some(WorkerEvent::Fault { code }) => return Err(io::Error::other(code).into()),
                Some(_) => {
                    return Err(io::Error::other("unexpected Controls privacy wait event").into());
                }
                None => {
                    empty = true;
                    break;
                }
            }
        }
        let at = monotonic_us();
        let physical = self.gate.state(MonoTime::from_micros(at));
        if physical == PrivacyState::Unavailable
            && (self.sequence != 0
                || self.started.elapsed() >= Duration::from_micros(PRIVACY_FRESH_US))
        {
            return Err(io::Error::other("Controls privacy unavailable while waiting").into());
        }
        if self.previous != Some(physical) {
            on_state(physical, self.boot, at)?;
            self.previous = Some(physical);
        }
        Ok(Step {
            physical,
            allowed: software_allowed && empty && physical == PrivacyState::Allowed,
        })
    }
}

/// Send Stop to both workers before waiting. Both share one 300 ms userspace
/// grace, followed by explicit forced cleanup on expiry (always a failure).
fn cleanup(privacy: &mut Worker, mut indicator: Option<&mut Worker>) -> io::Result<()> {
    let stopped_at = monotonic_us();
    let deadline = Instant::now() + Duration::from_millis(300);
    let mut error = privacy.channels.control.send(Control::Stop).err();
    if let Some(child) = &mut indicator
        && let Err(failure) = child.channels.control.send(Control::Stop)
    {
        error.get_or_insert(failure);
    }
    let mut privacy_exited = false;
    let mut indicator_exited = indicator.is_none();
    let mut blanked = false;
    loop {
        if let Some(child) = &mut indicator {
            drain_shutdown(child, stopped_at, &mut blanked, &mut error);
            if !indicator_exited {
                match child.try_exit() {
                    Ok(Some(status)) => {
                        indicator_exited = true;
                        // A receipt can arrive between the earlier drain and
                        // exit observation. With the sender gone, drain once
                        // more before deciding whether a black write happened.
                        drain_shutdown(child, stopped_at, &mut blanked, &mut error);
                        if let Err(failure) = child.exit_result(status) {
                            error.get_or_insert(failure);
                        }
                    }
                    Ok(None) => {}
                    Err(failure) => {
                        error.get_or_insert(failure);
                    }
                }
            }
        }
        if !privacy_exited {
            match privacy.try_exit() {
                Ok(Some(status)) => {
                    privacy_exited = true;
                    if let Err(failure) = privacy.exit_result(status) {
                        error.get_or_insert(failure);
                    }
                }
                Ok(None) => {}
                Err(failure) => {
                    error.get_or_insert(failure);
                }
            }
        }
        if privacy_exited && indicator_exited {
            break;
        }
        if Instant::now() >= deadline {
            if !privacy_exited {
                let failure = privacy.terminate_and_reap(
                    io::ErrorKind::TimedOut,
                    "privacy wait shared stop deadline exceeded".into(),
                );
                error.get_or_insert(failure);
            }
            if !indicator_exited && let Some(child) = &mut indicator {
                let failure = child.terminate_and_reap(
                    io::ErrorKind::TimedOut,
                    "privacy wait shared stop deadline exceeded".into(),
                );
                error.get_or_insert(failure);
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    if indicator.is_some() && !blanked {
        error
            .get_or_insert_with(|| io::Error::other("privacy indicator black write not confirmed"));
    }
    error.map_or(Ok(()), Err)
}

fn drain_shutdown(
    child: &mut Worker,
    stopped_at: u64,
    blanked: &mut bool,
    error: &mut Option<io::Error>,
) {
    for _ in 0..16 {
        match child.channels.control.receive::<WorkerEvent>() {
            Ok(Some(WorkerEvent::Ring {
                report:
                    report @ RingFeedback::Blanked {
                        reason: RingBlankReason::Shutdown,
                        ..
                    },
            })) => {
                if !*blanked && report.confirms_shutdown(stopped_at, monotonic_us()) {
                    *blanked = true;
                } else {
                    error.get_or_insert_with(|| {
                        io::Error::other("invalid or duplicate privacy indicator shutdown receipt")
                    });
                }
            }
            Ok(Some(WorkerEvent::Ring { .. })) => {}
            Ok(Some(_)) => {
                error.get_or_insert_with(|| {
                    io::Error::other("unexpected privacy indicator cleanup event")
                });
            }
            Ok(None) => break,
            Err(failure) => {
                error.get_or_insert(failure);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_privacy::tests::{Script, mock};
    use std::sync::{Arc, atomic::AtomicBool};

    #[test]
    fn selected_wait_observes_while_software_muted_without_granting() {
        let directory = SessionDirectory::create().unwrap();
        let (mut worker, peer) = mock(
            &directory.path,
            Script::Unmute,
            Arc::new(AtomicBool::new(false)),
        );
        let mut waiting = Waiting::new(new_boot().unwrap()).unwrap();
        let mut states = Vec::new();
        let mut record = |state, _, _| {
            states.push(state);
            Ok(())
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let step = waiting
                .step(&mut worker, None, &mut record, &mut || Ok(false))
                .unwrap();
            assert!(!step.allowed, "physical open cleared the software latch");
            if step.physical == PrivacyState::Allowed {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "fixture did not produce stable open"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(
            waiting
                .step(&mut worker, None, &mut record, &mut || Ok(true))
                .unwrap()
                .allowed
        );
        cleanup(&mut worker, None).unwrap();
        let peer = peer.join().unwrap();
        assert!(peer.received_stop);
        assert!(peer.denied_heartbeats > 0);
        assert!(states.contains(&PrivacyState::Muted));
        assert!(states.contains(&PrivacyState::Allowed));
    }

    #[test]
    fn selected_wait_never_substitutes_software_clear_for_missing_or_bad_switch() {
        for script in [
            Script::Missing,
            Script::Stale,
            Script::Fault,
            Script::Invalid,
        ] {
            let directory = SessionDirectory::create().unwrap();
            let (mut worker, peer) =
                mock(&directory.path, script, Arc::new(AtomicBool::new(false)));
            let mut waiting = Waiting::new(new_boot().unwrap()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match waiting.step(&mut worker, None, &mut |_, _, _| Ok(()), &mut || Ok(true)) {
                    Err(_) => break,
                    Ok(step) => assert!(!step.allowed),
                }
                assert!(
                    Instant::now() < deadline,
                    "invalid input did not close wait"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
            cleanup(&mut worker, None).unwrap();
            assert!(peer.join().unwrap().received_stop);
        }
    }

    fn stopping_child(
        directory: &Path,
        role: &str,
        receipts: usize,
    ) -> (
        Worker,
        std::thread::JoinHandle<crate::transport::WorkerChannels>,
    ) {
        let (worker, mut peer, input) = Worker::cooperative_fixture(directory, role);
        let task = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if matches!(
                    peer.control.receive::<Control>().unwrap(),
                    Some(Control::Stop)
                ) {
                    for _ in 0..receipts {
                        let now = monotonic_us();
                        peer.control
                            .send(WorkerEvent::Ring {
                                report: RingFeedback::Blanked {
                                    reason: RingBlankReason::Shutdown,
                                    write_started_at_us: now,
                                    write_finished_at_us: now,
                                },
                            })
                            .unwrap();
                    }
                    drop(input);
                    return peer;
                }
                assert!(Instant::now() < deadline, "cleanup never requested Stop");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        (worker, task)
    }

    /// Reviewer, #795: a ring that never started must not end the voice.
    #[test]
    fn a_ring_that_never_started_leaves_the_wait_to_open_without_its_light() {
        // The fixtures keep their endpoints until joined, so they live apart
        // from the session directory the wait removes when it ends.
        let fixtures = SessionDirectory::create().unwrap();
        let directory = SessionDirectory::create().unwrap();
        let (worker, peer) = mock(
            &fixtures.path,
            Script::Unmute,
            Arc::new(AtomicBool::new(false)),
        );
        let mut starts = 0;
        let outcome = run(
            worker,
            directory,
            new_boot().unwrap(),
            &Termination::inactive(),
            |_, _, _| Ok(()),
            || Ok(true),
            |tick| {
                starts += 1;
                tick()?;
                Ok(RingStartup::Unavailable {
                    worker: None,
                    error: io::Error::other("synthetic bind failure before spawn"),
                })
            },
        );
        assert!(matches!(outcome, Ok(WaitOutcome::Allowed)), "{outcome:?}");
        assert_eq!(starts, 1, "one attempt per muted wait");
        assert!(peer.join().unwrap().received_stop);
    }

    #[test]
    fn a_started_ring_that_failed_still_needs_its_black_write_and_exit() {
        for receipts in [0, 1] {
            let fixtures = SessionDirectory::create().unwrap();
            let directory = SessionDirectory::create().unwrap();
            let (worker, peer) = mock(
                &fixtures.path,
                Script::Unmute,
                Arc::new(AtomicBool::new(false)),
            );
            let (ring, ring_peer) = stopping_child(&fixtures.path, "ring", receipts);
            let mut ring = Some(Box::new(ring));
            let outcome = run(
                worker,
                directory,
                new_boot().unwrap(),
                &Termination::inactive(),
                |_, _, _| Ok(()),
                || Ok(true),
                |_| {
                    Ok(RingStartup::Unavailable {
                        worker: ring.take(),
                        error: io::Error::other("synthetic readiness failure after spawn"),
                    })
                },
            );
            assert!(outcome.is_err(), "receipts={receipts}: {outcome:?}");
            assert!(peer.join().unwrap().received_stop);
            drop(ring_peer.join().unwrap());
        }
    }

    #[test]
    fn next_writer_needs_one_black_receipt_and_both_verified_exits() {
        for receipts in [0, 1, 2] {
            let directory = SessionDirectory::create().unwrap();
            let (mut privacy, privacy_peer) = stopping_child(&directory.path, "privacy", 0);
            let (mut indicator, ring_peer) = stopping_child(&directory.path, "ring", receipts);
            let result = cleanup(&mut privacy, Some(&mut indicator));
            assert_eq!(
                result.is_ok(),
                receipts == 1,
                "receipts={receipts}: {result:?}"
            );
            assert!(privacy.try_exit().unwrap().unwrap().success());
            assert!(indicator.try_exit().unwrap().unwrap().success());
            drop(privacy_peer.join().unwrap());
            drop(ring_peer.join().unwrap());
        }
    }

    #[test]
    fn receipt_after_sender_exit_is_still_read_and_duplicate_is_rejected() {
        let directory = SessionDirectory::create().unwrap();
        let (mut child, mut peer, input) = Worker::cooperative_fixture(&directory.path, "ring");
        let stopped = monotonic_us();
        let at = monotonic_us();
        let receipt = || WorkerEvent::Ring {
            report: RingFeedback::Blanked {
                reason: RingBlankReason::Shutdown,
                write_started_at_us: at,
                write_finished_at_us: at,
            },
        };
        peer.control.send(receipt()).unwrap();
        drop(input);
        let mut blanked = false;
        let mut error = None;
        drain_shutdown(&mut child, stopped, &mut blanked, &mut error);
        assert!(blanked);
        assert!(error.is_none());
        peer.control.send(receipt()).unwrap();
        drain_shutdown(&mut child, stopped, &mut blanked, &mut error);
        assert!(error.is_some());
        child.shutdown().unwrap();
    }
}
