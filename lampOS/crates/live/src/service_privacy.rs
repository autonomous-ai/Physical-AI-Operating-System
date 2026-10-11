//! A service restart token can come only from a fresh physical mute, followed
//! by verified cleanup. Unknown privacy and failed cleanup are never resumable.
use std::{fmt, io};

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug)]
pub(crate) struct PhysicalMute {
    pub(crate) observed_at_us: u64,
}
impl fmt::Display for PhysicalMute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "physical mute observed at {} us", self.observed_at_us)
    }
}
impl std::error::Error for PhysicalMute {}

#[derive(Debug)]
struct CleanupFailed {
    original: Option<Box<dyn std::error::Error + Send + Sync>>,
    cleanup: io::Error,
}
impl fmt::Display for CleanupFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(original) = &self.original {
            write!(f, "{original}; ")?;
        }
        write!(f, "verified cleanup failed: {}", self.cleanup)
    }
}
impl std::error::Error for CleanupFailed {}

/// Cleanup failure must not retain a downcastable physical-mute restart token.
pub(crate) fn finish<T>(result: Result<T>, cleanup: io::Result<()>) -> Result<T> {
    match cleanup {
        Ok(()) => result,
        Err(cleanup) => Err(Box::new(CleanupFailed {
            original: result.err(),
            cleanup,
        })),
    }
}

use crate::{
    privacy::{PRIVACY_FRESH_US, PrivacyGate, PrivacyState},
    process::{SessionDirectory, Worker, new_boot},
    termination::Termination,
    wire::{Control, WorkerEvent},
};
use lamp_interaction::{BootId, Controller, MonoTime, Permission};
use lamp_ipc::monotonic_us;
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WaitOutcome {
    Allowed,
    Stopped,
}

/// Only this GPIO worker exists during the wait. Its authority snapshot remains
/// denied; observing stable unmute never grants microphone or camera permission.
pub(crate) fn wait(
    executable: &Path,
    termination: &Termination,
    on_state: impl FnMut(PrivacyState, BootId, u64) -> Result<()>,
) -> Result<WaitOutcome> {
    if termination.signal_name().is_some() {
        return Ok(WaitOutcome::Stopped);
    }
    let directory = SessionDirectory::create()?;
    let boot = new_boot()?;
    let worker =
        Worker::spawn_with_tick(executable, &directory.path, "privacy", boot, &[], || {
            termination.check()
        })?;
    let result = wait_worker(worker, boot, termination, on_state);
    // Worker endpoints and the GPIO owner have gone before removing this empty
    // directory. Refuse another cycle if anything unexpected remains.
    finish(result, fs::remove_dir(&directory.path))
}

pub(crate) fn wait_worker(
    mut worker: Worker,
    boot: BootId,
    termination: &Termination,
    on_state: impl FnMut(PrivacyState, BootId, u64) -> Result<()>,
) -> Result<WaitOutcome> {
    let result = wait_loop(&mut worker, boot, termination, on_state);
    // Bounded cooperative cleanup is outside every active audio loop. A forced
    // kill or abnormal exit stays a failure even when unmute was observed.
    let shutdown = worker.shutdown();
    finish(result, shutdown)
}

fn wait_loop(
    worker: &mut Worker,
    boot: BootId,
    termination: &Termination,
    mut on_state: impl FnMut(PrivacyState, BootId, u64) -> Result<()>,
) -> Result<WaitOutcome> {
    let mut controller = Controller::new(boot, MonoTime::from_micros(monotonic_us()));
    controller
        .set_microphone_permission(MonoTime::from_micros(monotonic_us()), Permission::Denied)?;
    controller.set_camera_permission(MonoTime::from_micros(monotonic_us()), Permission::Denied)?;
    let mut gate = PrivacyGate::default();
    let mut sequence = 0u64;
    let mut previous_state = None;
    let mut last_publish = None;
    let started = Instant::now();
    loop {
        if termination.signal_name().is_some() {
            return Ok(WaitOutcome::Stopped);
        }
        worker.check_running()?;
        let at = monotonic_us();
        if sequence != 0 && gate.state(MonoTime::from_micros(at)) == PrivacyState::Unavailable {
            return Err(io::Error::other("privacy freshness lost before next observation").into());
        }
        if last_publish.is_none_or(|last| at.saturating_sub(last) >= 25_000) {
            worker.channels.control.send(Control::Authority {
                snapshot: controller.snapshot(MonoTime::from_micros(at))?,
            })?;
            last_publish = Some(at);
        }
        let mut empty = false;
        for _ in 0..16 {
            match worker.channels.control.receive::<WorkerEvent>()? {
                Some(WorkerEvent::Privacy {
                    acquired_at_us,
                    muted,
                }) => {
                    sequence = sequence
                        .checked_add(1)
                        .ok_or_else(|| io::Error::other("privacy sequence overflow"))?;
                    gate.observe(
                        sequence,
                        MonoTime::from_micros(acquired_at_us),
                        MonoTime::from_micros(monotonic_us()),
                        muted,
                    );
                    if gate.state(MonoTime::from_micros(monotonic_us()))
                        == PrivacyState::Unavailable
                    {
                        return Err(io::Error::other(
                            "invalid or stale privacy observation while waiting",
                        )
                        .into());
                    }
                }
                Some(WorkerEvent::Fault { code }) => return Err(io::Error::other(code).into()),
                Some(_) => return Err(io::Error::other("unexpected privacy wait event").into()),
                None => {
                    empty = true;
                    break;
                }
            }
        }
        let at = monotonic_us();
        let state = gate.state(MonoTime::from_micros(at));
        if state == PrivacyState::Unavailable
            && (sequence != 0 || started.elapsed() >= Duration::from_micros(PRIVACY_FRESH_US))
        {
            return Err(
                io::Error::other("physical privacy observation unavailable while waiting").into(),
            );
        }
        if previous_state != Some(state) {
            on_state(state, boot, at)?;
            previous_state = Some(state);
        }
        if empty && state == PrivacyState::Allowed {
            return Ok(WaitOutcome::Allowed);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::transport::WorkerChannels;
    use lamp_interaction::Permission;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
    };

    #[derive(Clone, Copy)]
    pub(crate) enum Script {
        Unmute,
        Muted,
        Missing,
        Gap,
        Stale,
        Fault,
        Invalid,
    }
    pub(crate) struct Peer {
        // Keep mock endpoints alive through the parent's final exit check.
        pub(crate) channels: WorkerChannels,
        pub(crate) received_stop: bool,
        pub(crate) denied_heartbeats: usize,
    }
    pub(crate) fn mock(
        directory: &Path,
        script: Script,
        muted_seen: Arc<AtomicBool>,
    ) -> (Worker, thread::JoinHandle<Peer>) {
        let (worker, mut peer, input) = Worker::cooperative_fixture(directory, "privacy");
        let task = thread::spawn(move || {
            let mut ticks = 0usize;
            let mut heartbeats = 0;
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                for _ in 0..16 {
                    match peer.control.receive::<Control>().unwrap() {
                        Some(Control::Stop) => {
                            drop(input);
                            return Peer {
                                channels: peer,
                                received_stop: true,
                                denied_heartbeats: heartbeats,
                            };
                        }
                        Some(Control::Authority { snapshot }) => {
                            assert!(snapshot.owner().is_none());
                            assert_eq!(snapshot.microphone_permission(), Permission::Denied);
                            assert_eq!(snapshot.camera_permission(), Permission::Denied);
                            heartbeats += 1;
                        }
                        Some(_) => panic!("non-privacy command during mute wait"),
                        None => break,
                    }
                }
                if heartbeats != 0 {
                    let at = monotonic_us();
                    let event = match script {
                        Script::Missing => None,
                        Script::Gap if ticks == 0 => Some(WorkerEvent::Privacy {
                            acquired_at_us: at,
                            muted: true,
                        }),
                        Script::Stale if ticks == 0 => Some(WorkerEvent::Privacy {
                            acquired_at_us: at - PRIVACY_FRESH_US,
                            muted: true,
                        }),
                        Script::Fault if ticks == 0 => Some(WorkerEvent::Fault {
                            code: "synthetic GPIO fault".into(),
                        }),
                        Script::Invalid if ticks == 0 => Some(WorkerEvent::Ready),
                        Script::Muted | Script::Unmute => {
                            let muted = matches!(script, Script::Muted) || ticks < 4;
                            if muted {
                                muted_seen.store(true, Ordering::SeqCst);
                            }
                            Some(WorkerEvent::Privacy {
                                acquired_at_us: at,
                                muted,
                            })
                        }
                        _ => None,
                    };
                    if let Some(event) = event {
                        peer.control.send(event).unwrap();
                    }
                    ticks += 1;
                }
                assert!(
                    Instant::now() < deadline,
                    "privacy mock never received Stop"
                );
                thread::sleep(Duration::from_millis(5));
            }
        });
        (worker, task)
    }

    #[test]
    fn muted_gpio_only_startup_requires_stable_unmute_and_verified_stop() {
        let directory = SessionDirectory::create().unwrap();
        let seen = Arc::new(AtomicBool::new(false));
        let (worker, peer) = mock(&directory.path, Script::Unmute, seen.clone());
        let mut states = Vec::new();
        let result = wait_worker(
            worker,
            new_boot().unwrap(),
            &Termination::inactive(),
            |state, _, _| {
                states.push(state);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(result, WaitOutcome::Allowed);
        assert!(seen.load(Ordering::SeqCst));
        assert!(states.contains(&PrivacyState::Muted));
        assert!(states.contains(&PrivacyState::Stabilizing));
        assert_eq!(states.last(), Some(&PrivacyState::Allowed));
        let peer = peer.join().unwrap();
        assert!(peer.received_stop && peer.denied_heartbeats >= 2);
        drop(peer.channels);
        fs::remove_dir(&directory.path).unwrap();
    }

    #[test]
    fn unknown_stale_invalid_or_failed_gpio_never_provides_unmute_authority() {
        for script in [
            Script::Missing,
            Script::Gap,
            Script::Stale,
            Script::Fault,
            Script::Invalid,
        ] {
            let directory = SessionDirectory::create().unwrap();
            let (worker, peer) = mock(&directory.path, script, Arc::new(AtomicBool::new(false)));
            let mut allowed = false;
            let error = wait_worker(
                worker,
                new_boot().unwrap(),
                &Termination::inactive(),
                |state, _, _| {
                    allowed |= state == PrivacyState::Allowed;
                    Ok(())
                },
            )
            .unwrap_err();
            assert!(error.downcast_ref::<PhysicalMute>().is_none());
            assert!(!allowed);
            assert!(peer.join().unwrap().received_stop);
        }
    }

    #[test]
    fn unmute_cannot_hide_privacy_worker_cleanup_failure() {
        let directory = SessionDirectory::create().unwrap();
        let (mut worker, mut peer) = Worker::sleeping_fixture(&directory.path, "privacy");
        let gate = thread::spawn(move || {
            let mut reads = 0;
            loop {
                match peer.control.receive::<Control>().unwrap() {
                    Some(Control::Stop) => return peer,
                    Some(Control::Authority { .. }) => {}
                    None => {}
                    Some(_) => panic!("unexpected control"),
                }
                peer.control
                    .send(WorkerEvent::Privacy {
                        acquired_at_us: monotonic_us(),
                        muted: false,
                    })
                    .unwrap();
                reads += 1;
                assert!(reads < 100);
                thread::sleep(Duration::from_millis(5));
            }
        });
        let result = wait_loop(
            &mut worker,
            new_boot().unwrap(),
            &Termination::inactive(),
            |_, _, _| Ok(()),
        );
        assert_eq!(result.as_ref().unwrap(), &WaitOutcome::Allowed);
        let result = finish(result, worker.shutdown());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("forced termination")
        );
        assert!(worker.try_exit().unwrap().is_some());
        drop(gate.join().unwrap());
    }
}
