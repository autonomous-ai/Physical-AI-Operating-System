//! Optional camera lifecycle. Visual failure closes only the camera grant.
use super::*;

/// Keep mandatory progress/termination failure provenance separate from the
/// optional child's startup failure. The child spawn still owns its cleanup.
pub(super) fn optional_startup<T>(
    start: impl FnOnce(&mut dyn FnMut() -> io::Result<()>) -> io::Result<T>,
    mut mandatory_tick: impl FnMut() -> io::Result<()>,
) -> io::Result<Option<T>> {
    let mut mandatory_error = None;
    let result = start(&mut || match mandatory_tick() {
        Ok(()) => Ok(()),
        Err(error) => {
            mandatory_error = Some(error);
            Err(io::Error::other(
                "mandatory progress failed during camera startup",
            ))
        }
    });
    if let Some(error) = mandatory_error {
        // Preserve the original fault/termination, including its error kind.
        // Dropping a successfully returned child would still clean it up if a
        // future spawn implementation ever ignored its callback's failure.
        drop(result);
        Err(error)
    } else {
        Ok(result.ok())
    }
}

pub(super) struct PreparedCamera {
    pub path: PathBuf,
    pub digest: String,
    pub source: VisualSource,
}
impl PreparedCamera {
    pub fn load(path: &Path, directory: &Path) -> io::Result<Self> {
        let (config, digest) = CameraConfig::load(path, None)?;
        let source = VisualSource::configured(directory, new_boot()?, &config)?;
        Ok(Self {
            path: path.to_path_buf(),
            digest,
            source,
        })
    }
}

pub(super) struct OptionalCamera {
    pub worker: Worker,
    source: VisualSource,
    pub failed: bool,
    pub pending_failure: Option<&'static str>,
    start_sent: bool,
    started: bool,
    last_event_us: u64,
    opening_until_us: Option<u64>,
}
impl OptionalCamera {
    pub fn new(worker: Worker, source: VisualSource) -> Self {
        Self {
            worker,
            source,
            failed: false,
            pending_failure: None,
            start_sent: false,
            started: false,
            last_event_us: monotonic_us(),
            opening_until_us: None,
        }
    }
    /// A camera that fails after its handshake must not turn the next mandatory
    /// worker's startup into a voice failure. Permission is still closed here.
    pub fn startup_tick(&mut self, startup: &mut StartupEvents, snapshot: Option<Snapshot>) {
        if self.pending_failure.is_some() || self.failed {
            return;
        }
        let result = (|| -> io::Result<()> {
            startup.poll(&mut self.worker.channels, "camera")?;
            self.worker.check_running()?;
            if let Some(snapshot) = snapshot {
                self.worker
                    .channels
                    .control
                    .send(Control::Authority { snapshot })?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.pending_failure = Some("camera failed during mandatory worker startup");
            let _ = self.worker.channels.control.send(Control::Stop);
        }
    }
    fn poll(&mut self) -> io::Result<Vec<CameraEvent>> {
        // At most start/open/stop metadata is retained here, not every frame.
        let mut important = Vec::new();
        if self.worker.try_exit()?.is_some() && !self.failed {
            return Err(io::Error::other("camera worker exited"));
        }
        for _ in 0..16 {
            let Some(event) = self.worker.channels.data.receive::<CameraEvent>()? else {
                break;
            };
            if self.failed {
                continue;
            }
            let at = monotonic_us();
            let stamped = match &event {
                CameraEvent::Opening { at_us, deadline_us } => {
                    if !self.start_sent
                        || self.started
                        || self.opening_until_us.is_some()
                        || deadline_us.checked_sub(*at_us) != Some(lamp_camera::START_BUDGET_US)
                    {
                        return Err(io::Error::other("invalid camera opening receipt"));
                    }
                    self.opening_until_us = Some(*deadline_us);
                    *at_us
                }
                CameraEvent::Started {
                    at_us,
                    worker,
                    capture_epoch,
                    mode,
                    timing,
                    ..
                } => {
                    if !self.start_sent
                        || self.started
                        || self.opening_until_us.is_none()
                        || *worker != self.source.camera
                        || *capture_epoch == 0
                        || mode.source != self.source.source
                        || mode.format != *b"MJPG"
                        || mode.width != self.source.width
                        || mode.height != self.source.height
                        || mode.size_image > self.source.max_frame_bytes
                        || timing.started_at_us > timing.completed_at_us
                        || timing.completed_at_us > *at_us
                    {
                        return Err(io::Error::other("camera mode or start identity changed"));
                    }
                    self.started = true;
                    self.opening_until_us = None;
                    *at_us
                }
                CameraEvent::Frame { frame } => {
                    if !self.started || frame.worker != self.source.camera {
                        return Err(io::Error::other("unstarted camera frame"));
                    }
                    frame.published_at_us
                }
                CameraEvent::Progress { at_us, .. } => *at_us,
                CameraEvent::Stopped { .. } => return Err(io::Error::other("camera stopped")),
                CameraEvent::FrameLink { .. } => continue,
            };
            if stamped > at
                || stamped < self.last_event_us.saturating_sub(100_000)
                || at.saturating_sub(stamped) >= 100_000
            {
                return Err(io::Error::other("camera progress is stale or future"));
            }
            self.last_event_us = self.last_event_us.max(stamped);
            if matches!(
                event,
                CameraEvent::Opening { .. } | CameraEvent::Started { .. }
            ) {
                important.push(event);
            }
        }
        let at = monotonic_us();
        let grace = self
            .opening_until_us
            .unwrap_or(self.last_event_us.saturating_add(250_000));
        if self.start_sent && !self.failed && at >= grace {
            return Err(io::Error::other("camera progress deadline expired"));
        }
        Ok(important)
    }
}

pub(super) fn service_camera(
    rig: &mut Rig,
    owner: &mut Controller,
    allowed: bool,
    trace: &mut Vec<Value>,
) -> Result<()> {
    let Some(camera) = rig.camera.as_mut() else {
        return Ok(());
    };
    let result = camera
        .pending_failure
        .take()
        .map_or_else(|| camera.poll(), |reason| Err(io::Error::other(reason)));
    match result {
        Ok(events) => {
            for event in events {
                record(
                    trace,
                    json!({"kind":"camera_lifecycle", "at_us":monotonic_us(), "details":event,
                "boundary":"camera lifecycle only; no listening or visual-answer readiness"}),
                )?;
            }
        }
        Err(_) if !camera.failed => {
            camera.failed = true;
            // Do not wait for worker stop on the audio path. Its own authority
            // watchdog and final shared supervisor deadline bound cleanup.
            let _ = camera.worker.channels.control.send(Control::Stop);
            owner.set_camera_permission(now(), Permission::Denied)?;
            let state = owner.snapshot(now())?;
            rig.publish(state)?;
            record(
                trace,
                json!({"kind":"camera_unavailable", "at_us":monotonic_us(),
                "stage":"capture_or_transport", "audio_continues":true, "camera_revoked":true}),
            )?;
            return Ok(());
        }
        Err(_) => return Ok(()),
    }
    let camera = rig.camera.as_mut().expect("present above");
    if allowed && !camera.failed && !camera.start_sent {
        owner.set_camera_permission(now(), Permission::Allowed)?;
        let state = owner.snapshot(now())?;
        rig.publish(state)?;
        let camera = rig.camera.as_mut().expect("present above");
        if camera
            .worker
            .channels
            .control
            .send(Control::StartCapture)
            .is_err()
        {
            camera.pending_failure = Some("camera start transport failed");
        }
        camera.start_sent = true;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mandatory_fault_consumed_during_camera_spawn_wait_remains_fatal() {
        use std::os::unix::fs::PermissionsExt;
        let directory = SessionDirectory::create().unwrap();
        let executable = directory.path.join("unready-camera");
        std::fs::write(&executable, "#!/bin/sh\nexec sleep 60\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let (mut provider, mut peer) = Worker::sleeping_fixture(&directory.path, "provider");
        peer.control
            .send(WorkerEvent::Fault {
                code: "one-shot mandatory provider failure".into(),
            })
            .unwrap();
        let boot = new_boot().unwrap();
        let mut owner = Controller::new(boot, now());
        let mut startup = StartupEvents::default();
        let mut last = 0;
        let termination = Termination::install().unwrap();
        let result = optional_startup(
            |tick| {
                Worker::spawn_with_boot_and_tick(
                    &executable,
                    &directory.path,
                    "camera",
                    boot,
                    new_boot().unwrap(),
                    &[],
                    tick,
                )
            },
            || {
                pulse(
                    &mut owner,
                    &mut [&mut provider],
                    &mut last,
                    &mut startup,
                    &termination,
                )
            },
        );
        std::fs::remove_file(executable).unwrap();
        let error = result
            .err()
            .expect("mandatory callback failure must not become optional camera absence");
        assert!(
            error
                .to_string()
                .contains("one-shot mandatory provider failure")
        );
        // The notification was consumed; it cannot be recovered by checking the
        // provider's continued liveness at a later startup phase.
        assert!(
            provider
                .channels
                .control
                .receive::<WorkerEvent>()
                .unwrap()
                .is_none()
        );
        provider.check_running().unwrap();
        assert!(startup.events.is_empty());
    }

    #[test]
    fn camera_owned_spawn_failure_is_optional_but_interrupted_callback_is_not() {
        let directory = SessionDirectory::create().unwrap();
        let result = optional_startup(
            |tick| {
                Worker::spawn_with_boot_and_tick(
                    &directory.path.join("missing-camera"),
                    &directory.path,
                    "camera",
                    new_boot().unwrap(),
                    new_boot().unwrap(),
                    &[],
                    tick,
                )
            },
            || Ok(()),
        )
        .unwrap();
        assert!(result.is_none());
        // A missing executable may fail synchronously, or as an exited child
        // under an emulator. Successful wait callbacks do not change its origin.
        let result: io::Result<Option<()>> = optional_startup(
            |tick| tick(),
            || {
                Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "directed termination",
                ))
            },
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    }
    #[test]
    fn camera_failure_after_ready_cannot_abort_later_mandatory_worker_startup() {
        let directory = SessionDirectory::create().unwrap();
        let (worker, mut peer) = Worker::sleeping_fixture(&directory.path, "camera");
        let source = VisualSource {
            socket: directory.path.join("camera.frames"),
            camera: worker.boot,
            source: "desk".into(),
            width: 320,
            height: 240,
            max_frame_bytes: 128 * 1024,
        };
        let mut camera = OptionalCamera::new(worker, source);
        let mut startup = StartupEvents::default();
        let mut owner = Controller::new(new_boot().unwrap(), now());
        peer.control
            .send(WorkerEvent::Fault {
                code: "camera failed before privacy worker ready".into(),
            })
            .unwrap();
        camera.startup_tick(&mut startup, Some(owner.snapshot(now()).unwrap()));
        assert!(camera.pending_failure.is_some());
        assert!(matches!(
            peer.control.receive::<Control>().unwrap(),
            Some(Control::Stop)
        ));
        // The stopped optional worker cannot poison later mandatory pulses or
        // receive permission on a subsequent heartbeat.
        camera.startup_tick(&mut startup, Some(owner.snapshot(now()).unwrap()));
        assert!(peer.control.receive::<Control>().unwrap().is_none());
        assert_eq!(
            owner.snapshot(now()).unwrap().camera_permission(),
            Permission::Unknown
        );
        assert!(startup.events.is_empty());
    }
    #[test]
    fn camera_failure_revokes_only_sight_preserving_current_speech_and_input() {
        let directory = SessionDirectory::create().unwrap();
        let (speaker, _speaker_peer) = Worker::sleeping_fixture(&directory.path, "speaker");
        let (capture, _capture_peer) = Worker::sleeping_fixture(&directory.path, "capture");
        let (provider, mut provider_peer) = Worker::sleeping_fixture(&directory.path, "provider");
        let (privacy, _privacy_peer) = Worker::sleeping_fixture(&directory.path, "privacy");
        let (worker, mut camera_peer) = Worker::sleeping_fixture(&directory.path, "camera");
        let source = VisualSource {
            socket: directory.path.join("camera.frames"),
            camera: worker.boot,
            source: "desk".into(),
            width: 320,
            height: 240,
            max_frame_bytes: 128 * 1024,
        };
        let mut camera = OptionalCamera::new(worker, source);
        camera.pending_failure = Some("simulated absent camera before first frame");
        let mut rig = Rig {
            speaker,
            capture,
            provider,
            privacy,
            ring: None,
            camera: Some(camera),
            ring_channel_ceiling: None,
            diagnostic_directory: None,
            noise_suppression: NoiseSuppression::On,
            turn_taking: TurnTaking::VadOnly,
            playout_start: PlayoutStart::default(),
            endpoint: None,
        };
        let boot = new_boot().unwrap();
        let mut owner = Controller::new(boot, now());
        owner
            .set_microphone_permission(now(), Permission::Allowed)
            .unwrap();
        owner
            .set_camera_permission(now(), Permission::Allowed)
            .unwrap();
        let until = MonoTime::from_micros(monotonic_us() + 100_000);
        owner
            .set_capture(now(), CaptureState::RetainingUntil(until))
            .unwrap();
        owner
            .set_admission(now(), AdmissionState::OpenUntil(until))
            .unwrap();
        let turn = owner.admit(now(), AdmittedInput::NewTurn).unwrap();
        let plan = owner
            .plan_output(now(), turn, OutputKind::Speech, None)
            .unwrap();
        owner.input_ended(now(), turn).unwrap();
        let permit = owner.issue_output(now(), plan, 100_000).unwrap();
        let before = owner.snapshot(now()).unwrap();
        let mut trace = Vec::new();
        service_camera(&mut rig, &mut owner, true, &mut trace).unwrap();
        let after = owner.snapshot(now()).unwrap();
        assert_eq!(after.owner(), Some(turn));
        assert!(after.listening_ready(now()));
        assert_eq!(
            after.microphone_generation(),
            before.microphone_generation()
        );
        assert_eq!(after.camera_permission(), Permission::Denied);
        let mut guard = lamp_interaction::BoundaryGuard::new(boot, now());
        guard.install(now(), after).unwrap();
        guard.check(now(), permit, OutputKind::Speech).unwrap();
        assert!(matches!(
            camera_peer.control.receive().unwrap(),
            Some(Control::Stop)
        ));
        assert!(
            matches!(provider_peer.control.receive().unwrap(),Some(Control::Authority{snapshot})
            if snapshot.camera_permission()==Permission::Denied && snapshot.owner()==Some(turn))
        );
        assert_eq!(trace.len(), 1);
        service_camera(&mut rig, &mut owner, true, &mut trace).unwrap();
        assert_eq!(
            trace.len(),
            1,
            "no repeated camera failure or implicit restart"
        );
    }
}
