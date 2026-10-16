//! Synthetic child/socket supervision only; no hardware or provider connection.
use super::*;
use crate::service_privacy::PhysicalMute;

#[test]
fn cleanup_failure_cannot_leave_a_restartable_physical_mute() {
    let directory = SessionDirectory::create().unwrap();
    let (speaker, _speaker_peer) = Worker::sleeping_fixture(&directory.path, "speaker");
    let (capture, _capture_peer) = Worker::sleeping_fixture(&directory.path, "capture");
    let (provider, _provider_peer) = Worker::sleeping_fixture(&directory.path, "provider");
    let (privacy, _privacy_peer) = Worker::sleeping_fixture(&directory.path, "privacy");
    let mut rig = Rig {
        second: None,
        microphone: crate::options::Microphone::Mic2,
        end_silence_blocks: None,
        early_end_blocks: None,
        faint_level: None,
        speaker,
        capture,
        provider,
        privacy,
        privacy_controls: false,
        ring: None.into(),
        camera: None,
        motor: None,
        ring_channel_ceiling: None,
        diagnostic_directory: None,
        noise_suppression: NoiseSuppression::On,
        turn_taking: TurnTaking::VadOnly,
        playout_start: PlayoutStart::default(),
        endpoint: None,
        call_wait_blocks: None,
        cut_keeps_name: false,
        asker_first: crate::asker_first::Mode::Off,
        paused: Box::new(|| false),
    };
    let (mut controller, current, _) = active_reply();
    let mut reply = Some(current);
    let mut trace = SessionState::default();
    let error = finish_active_run(
        &mut rig,
        &mut controller,
        &mut reply,
        &mut trace,
        &mut None,
        Err(Box::new(PhysicalMute {
            observed_at_us: monotonic_us(),
        })),
    )
    .unwrap_err();
    assert!(
        trace
            .iter()
            .any(|row| row["kind"] == "worker_forced_shutdown")
    );
    assert!(
        error.downcast_ref::<PhysicalMute>().is_none(),
        "failed cleanup retained restart token: {error}"
    );
    assert!(error.to_string().contains("physical mute"));
    assert!(error.to_string().contains("forced termination"));
    let waits = std::cell::Cell::new(0);
    let mut failure = Some(error);
    let restarted = crate::coordinator::service::supervise(
        &mut trace,
        &mut None,
        &Termination::inactive(),
        |_, _| {
            waits.set(waits.get() + 1);
            Ok(crate::service_privacy::WaitOutcome::Allowed)
        },
        |_, _| Err(failure.take().expect("must never retry failed cleanup")),
    );
    assert!(restarted.is_err());
    assert_eq!(waits.get(), 1);
    assert!(reply.is_none());
    assert!(controller.snapshot(now()).unwrap().owner().is_none());
    for worker in [
        &mut rig.speaker,
        &mut rig.capture,
        &mut rig.provider,
        &mut rig.privacy,
    ] {
        assert!(worker.try_exit().unwrap().is_some());
    }
}

use crate::service_privacy::{
    self, WaitOutcome,
    tests::{Script, mock},
};
use std::{
    cell::Cell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

fn active_mute(trace: &mut SessionState, invalid: bool) -> Result<()> {
    let directory = SessionDirectory::create().unwrap();
    let (speaker, speaker_peer, speaker_input) =
        Worker::cooperative_fixture(&directory.path, "speaker");
    let (capture, capture_peer, capture_input) =
        Worker::cooperative_fixture(&directory.path, "capture");
    let (provider, provider_peer, provider_input) =
        Worker::cooperative_fixture(&directory.path, "provider");
    let (privacy, privacy_peer, privacy_input) =
        Worker::cooperative_fixture(&directory.path, "privacy");
    let peers = [
        (speaker_peer, speaker_input, false),
        (capture_peer, capture_input, false),
        (provider_peer, provider_input, false),
        (privacy_peer, privacy_input, true),
    ]
    .map(|(mut channels, input, privacy)| {
        thread::spawn(move || {
            let mut sent = false;
            let mut denied = false;
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                match channels.control.receive::<Control>().unwrap() {
                    Some(Control::Authority { snapshot }) => {
                        denied |= snapshot.microphone_permission() == Permission::Denied;
                        if privacy && !sent {
                            channels
                                .control
                                .send(if invalid {
                                    WorkerEvent::Privacy {
                                        acquired_at_us: monotonic_us()
                                            - crate::privacy::PRIVACY_FRESH_US,
                                        muted: true,
                                    }
                                } else {
                                    WorkerEvent::Privacy {
                                        acquired_at_us: monotonic_us(),
                                        muted: true,
                                    }
                                })
                                .unwrap();
                            sent = true;
                        }
                    }
                    Some(Control::Stop) => break,
                    Some(_) => {}
                    None => {}
                }
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(1));
            }
            drop(input);
            (denied, channels)
        })
    });
    let mut rig = Rig {
        second: None,
        microphone: crate::options::Microphone::Mic2,
        end_silence_blocks: None,
        early_end_blocks: None,
        faint_level: None,
        speaker,
        capture,
        provider,
        privacy,
        privacy_controls: false,
        ring: None.into(),
        camera: None,
        motor: None,
        ring_channel_ceiling: None,
        diagnostic_directory: None,
        noise_suppression: NoiseSuppression::On,
        turn_taking: TurnTaking::VadOnly,
        playout_start: PlayoutStart::default(),
        endpoint: None,
        call_wait_blocks: None,
        cut_keeps_name: false,
        asker_first: crate::asker_first::Mode::Off,
        paused: Box::new(|| false),
    };
    let (mut controller, current, permit) = active_reply();
    let mut reply = Some(current);
    let result = conversation(
        &mut rig,
        &mut controller,
        Lifetime::Serve,
        trace,
        StartupEvents::default(),
        &mut reply,
        &mut None,
        &Termination::inactive(),
        &FailureNotices::none(),
    );
    assert_eq!(
        result
            .as_ref()
            .unwrap_err()
            .downcast_ref::<PhysicalMute>()
            .is_some(),
        !invalid
    );
    let result = finish_active_run(
        &mut rig,
        &mut controller,
        &mut reply,
        trace,
        &mut None,
        result,
    );
    assert!(reply.is_none());
    let snapshot = controller.snapshot(now()).unwrap();
    let mut guard = BoundaryGuard::new(snapshot.boot(), now());
    guard.install(now(), snapshot).unwrap();
    assert!(guard.check(now(), permit, OutputKind::Speech).is_err());
    for peer in peers {
        assert!(peer.join().unwrap().0);
    }
    for worker in [
        &mut rig.capture,
        &mut rig.speaker,
        &mut rig.provider,
        &mut rig.privacy,
    ] {
        assert!(worker.try_exit().unwrap().is_some());
    }
    drop(rig);
    fs::remove_dir(&directory.path).unwrap();
    result
}

#[test]
fn active_physical_mute_cancels_owned_reply_then_reaps_without_completion() {
    let mut trace = SessionState::default();
    let error = active_mute(&mut trace, false).unwrap_err();
    assert!(error.downcast_ref::<PhysicalMute>().is_some());
    assert!(
        trace
            .iter()
            .any(|row| row["kind"] == "turn_finished" && row["outcome"] == "privacy_muted")
    );
    assert!(
        !trace
            .iter()
            .any(|row| row["outcome"] == "audio_written_unscored")
    );
}

#[test]
fn stale_switch_during_active_run_is_never_a_restartable_mute() {
    let mut trace = SessionState::default();
    let error = active_mute(&mut trace, true).unwrap_err();
    assert!(error.downcast_ref::<PhysicalMute>().is_none());
    assert!(!trace.iter().any(|row| row["kind"] == "physical_mute"));
    assert!(trace.iter().any(|row| row["outcome"] == "runtime_failed"));
}

#[test]
fn repeated_verified_mute_cycles_keep_one_logger_and_never_open_audio_while_waiting() {
    let directory = SessionDirectory::create().unwrap();
    let output = directory.path.join("output");
    fs::create_dir(&output).unwrap();
    let mut trace = SessionState::service(&output);
    let active_count = Cell::new(0);
    let wait_count = Cell::new(0);
    let active_open = Cell::new(false);
    let mut boots = Vec::new();
    crate::coordinator::service::supervise(
        &mut trace,
        &mut None,
        &Termination::inactive(),
        |_, _| {
            assert!(!active_open.get());
            if wait_count.get() == 3 {
                return Ok(WaitOutcome::Stopped);
            }
            let scratch = SessionDirectory::create().unwrap();
            let seen = Arc::new(AtomicBool::new(false));
            let (worker, peer) = mock(&scratch.path, Script::Unmute, seen.clone());
            let boot = new_boot().unwrap();
            assert!(!boots.contains(&boot));
            boots.push(boot);
            let result = service_privacy::wait_worker(
                worker,
                boot,
                &Termination::inactive(),
                |state, _, _| {
                    assert!(!active_open.get());
                    assert_eq!(active_count.get(), wait_count.get());
                    if state == PrivacyState::Allowed {
                        assert!(seen.load(Ordering::SeqCst));
                    }
                    Ok(())
                },
            );
            assert!(peer.join().unwrap().received_stop);
            fs::remove_dir(&scratch.path).unwrap();
            wait_count.set(wait_count.get() + 1);
            result
        },
        |trace, _| {
            assert_eq!(active_count.get() + 1, wait_count.get());
            active_open.set(true);
            let result = active_mute(trace, false);
            active_open.set(false);
            active_count.set(active_count.get() + 1);
            result
        },
    )
    .unwrap();
    assert_eq!(active_count.get(), 3);
    assert_eq!(wait_count.get(), 3);
    assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
    assert!(trace.finish_logging().unwrap().acknowledged);
    assert_eq!(fs::read_dir(output.join("metadata")).unwrap().count(), 5);
    fs::remove_dir_all(&output).unwrap();
}

#[test]
fn unavailable_startup_or_failed_cleanup_never_calls_next_runtime() {
    for cleanup in [false, true] {
        let mut trace = SessionState::default();
        let opened = Cell::new(0);
        let result = crate::coordinator::service::supervise(
            &mut trace,
            &mut None,
            &Termination::inactive(),
            |_, _| {
                if cleanup {
                    service_privacy::finish(
                        Ok(WaitOutcome::Allowed),
                        Err(io::Error::other("forced termination")),
                    )
                } else {
                    Err(io::Error::other("privacy unavailable").into())
                }
            },
            |_, _| {
                opened.set(opened.get() + 1);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(opened.get(), 0);
    }
}
