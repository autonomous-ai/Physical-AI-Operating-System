//! The real `lamp-perception` worker process, driven like a supervisor would:
//! the runtime's private datagram channels and envelopes, authority snapshots
//! from the real `Controller`, and a real `FrameServer` standing in for the
//! camera worker. Frames are drawn scenes (no photographs).
mod common;

use lamp_camera::{
    CallTiming, CaptureId, FrameId, FrameObservation, NegotiatedMode, PixelFormat, SourceId,
    frames::{FrameServer, Role},
};
use lamp_interaction::{BootId, Controller, MonoTime, Permission};
use lamp_ipc::monotonic_us;
use lamp_live::{
    transport::Channel,
    wire::{Control, WorkerEvent},
};
use lamp_perception::{
    model,
    observation::{DiscardReason, PerceptionEvent, Presence},
};
use std::{
    num::NonZeroU64,
    process::{Child, Command, Stdio},
    time::Duration,
};

const BIN: &str = env!("CARGO_BIN_EXE_lamp-perception");

fn boot(n: u8) -> BootId {
    BootId::new([n; 16]).unwrap()
}
fn hex(b: BootId) -> String {
    b.bytes().iter().map(|x| format!("{x:02x}")).collect()
}
fn now() -> MonoTime {
    MonoTime::from_micros(monotonic_us())
}

struct Supervisor {
    dir: common::Private,
    parent: BootId,
    worker: BootId,
    owner: Controller,
    control: Channel,
    data: Channel,
    child: Child,
    last_authority: u64,
    events: Vec<PerceptionEvent>,
    frames: u64,
}

impl Supervisor {
    fn spawn(label: &str, model_dir: &std::path::Path) -> Self {
        let dir = common::Private::new(label);
        let (parent, worker) = (boot(0x31), boot(0x32));
        let control = Channel::bind(&dir.join("perception.c.p"), parent, worker, 100_000).unwrap();
        let data = Channel::bind(&dir.join("perception.d.p"), parent, worker, 100_000).unwrap();
        let child = Command::new(BIN)
            .args(["worker", "perception"])
            .arg(&dir.0)
            .args([hex(parent), hex(worker)])
            .arg("--model-dir")
            .arg(model_dir)
            .args(["--interval-ms", "100"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let mut owner = Controller::new(parent, now());
        owner
            .set_camera_permission(now(), Permission::Allowed)
            .unwrap();
        Self {
            dir,
            parent,
            worker,
            owner,
            control,
            data,
            child,
            last_authority: 0,
            events: Vec::new(),
            frames: 0,
        }
    }

    /// Connect once the worker's sockets exist; false if it exited first.
    fn connect(&mut self) -> bool {
        for _ in 0..1500 {
            if self.child.try_wait().unwrap().is_some() {
                return false;
            }
            if self.dir.join("perception.c.w").exists() && self.dir.join("perception.d.w").exists()
            {
                self.control
                    .connect(&self.dir.join("perception.c.w"))
                    .unwrap();
                self.data.connect(&self.dir.join("perception.d.w")).unwrap();
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("worker sockets never appeared");
    }

    fn heartbeat(&mut self) {
        if monotonic_us() - self.last_authority >= 20_000 {
            let snapshot = self.owner.snapshot(now()).unwrap();
            // A send error means the worker already exited; tests check that.
            let _ = self.control.send(Control::Authority { snapshot });
            self.last_authority = monotonic_us();
        }
    }

    /// Like the runtime supervisor: no authority is sent before `Ready`
    /// (queued snapshots would expire while the model loads).
    fn startup(&mut self) -> Option<WorkerEvent> {
        if !self.connect() {
            return None;
        }
        for _ in 0..20_000 {
            match self.control.receive::<WorkerEvent>() {
                Ok(Some(event)) => {
                    if matches!(event, WorkerEvent::Ready) {
                        self.heartbeat();
                    }
                    return Some(event);
                }
                Ok(None) => {}
                Err(_) => return None,
            }
            if self.child.try_wait().unwrap().is_some() {
                return None;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("no startup event");
    }

    fn observation(&mut self, dequeued_at_us: u64) -> FrameObservation {
        self.frames += 1;
        FrameObservation {
            id: FrameId {
                capture: CaptureId {
                    worker: boot(0x33),
                    epoch: NonZeroU64::new(1).unwrap(),
                },
                sequence: NonZeroU64::new(self.frames).unwrap(),
            },
            source: SourceId::new("desk-camera").unwrap(),
            grant: self.owner.camera_grant(now()).unwrap(),
            mode: NegotiatedMode {
                source: SourceId::new("desk-camera").unwrap(),
                format: PixelFormat::MJPG,
                width: 1280,
                height: 720,
                interval: None,
                size_image: 2 * 1024 * 1024,
                buffers: 2,
            },
            driver_sequence: self.frames as u32,
            driver_timestamp: None,
            dequeue: CallTiming {
                started_at: MonoTime::from_micros(dequeued_at_us - 50),
                completed_at: MonoTime::from_micros(dequeued_at_us),
            },
        }
    }

    /// Supervise for `micros`, serving `jpeg` whenever the worker asks.
    fn run(&mut self, camera: Option<&mut FrameServer>, jpeg: &[u8], micros: u64) {
        let mut camera = camera;
        let until = monotonic_us() + micros;
        while monotonic_us() < until {
            self.heartbeat();
            if let Some(server) = camera.as_deref_mut() {
                server.service(monotonic_us());
                if server.wants_frame() {
                    let observation = self.observation(monotonic_us());
                    server.offer(&observation, jpeg, monotonic_us()).unwrap();
                }
            }
            while let Some(event) = self.data.receive::<PerceptionEvent>().unwrap() {
                self.events.push(event);
            }
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn camera(&self) -> FrameServer {
        FrameServer::bind(
            &self.dir.join(lamp_live::camera_worker::FRAME_SOCKET),
            self.parent,
            &[(Role::Perception, self.worker)],
            2 * 1024 * 1024,
        )
        .unwrap()
    }

    fn observations(&self) -> Vec<&lamp_perception::observation::VisualObservation> {
        self.events
            .iter()
            .filter_map(|e| match e {
                PerceptionEvent::Observation { observation } => Some(observation),
                _ => None,
            })
            .collect()
    }

    fn exit_within(&mut self, micros: u64) -> Option<std::process::ExitStatus> {
        let until = monotonic_us() + micros;
        while monotonic_us() < until {
            if let Some(status) = self.child.try_wait().unwrap() {
                return Some(status);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        None
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn vendored() -> std::path::PathBuf {
    model::vendored_directory().canonicalize().unwrap()
}

#[test]
fn fresh_frames_become_observations_with_their_original_lineage() {
    let jpeg = common::face_jpeg(640.0, 360.0, 150.0);
    let mut s = Supervisor::spawn("obs", &vendored());
    assert!(matches!(s.startup(), Some(WorkerEvent::Ready)));
    let mut camera = s.camera();
    s.run(Some(&mut camera), &jpeg, 1_500_000);
    assert!(s.events.iter().any(|e| matches!(
        e,
        PerceptionEvent::CameraLink {
            connected: true,
            ..
        }
    )));
    let grant = s.owner.camera_grant(now()).unwrap();
    let observations = s.observations();
    assert!(observations.len() >= 3, "{:?}", s.events);
    let mut last_sequence = 0;
    for o in &observations {
        assert!(
            matches!(o.presence, Presence::Faces { count: 1, .. }),
            "{:?}",
            o.presence
        );
        assert_eq!(o.frame.grant, grant);
        assert_eq!(o.frame.camera_worker, boot(0x33));
        assert_eq!(o.provenance.source_boot(), boot(0x32));
        assert!(o.provenance.sequence() > last_sequence);
        last_sequence = o.provenance.sequence();
        assert_eq!(
            o.provenance.acquired_at().as_micros(),
            o.frame.dequeue_completed_at_us
        );
        assert!(o.frame.dequeue_completed_at_us <= o.timing.received_at_us);
        assert!(o.timing.received_at_us <= o.timing.analysed_at_us);
        assert!(o.timing.analysed_at_us <= o.timing.published_at_us);
        // Within the worker's one-second result bound.
        assert!(o.timing.published_at_us - o.frame.dequeue_completed_at_us <= 1_000_000);
    }
    // Requests are paced by --interval-ms 100, not as fast as frames exist.
    assert!(observations.len() <= 16, "{}", observations.len());
    s.control.send(Control::Stop).unwrap();
    assert!(s.exit_within(2_000_000).unwrap().success());
}

#[test]
fn a_frame_from_before_a_privacy_change_is_discarded_not_analysed() {
    let jpeg = common::face_jpeg(640.0, 360.0, 150.0);
    let mut s = Supervisor::spawn("priv", &vendored());
    assert!(matches!(s.startup(), Some(WorkerEvent::Ready)));
    let mut camera = s.camera();
    // Wait for a request without answering it.
    let deadline = monotonic_us() + 2_000_000;
    while !camera.wants_frame() && monotonic_us() < deadline {
        s.heartbeat();
        camera.service(monotonic_us());
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(camera.wants_frame());
    // The frame was captured under the open grant; privacy then closes and
    // reopens (a new lineage) before it arrives.
    let old = s.observation(monotonic_us());
    s.owner
        .set_camera_permission(now(), Permission::Denied)
        .unwrap();
    s.owner
        .set_camera_permission(now(), Permission::Allowed)
        .unwrap();
    s.last_authority = 0;
    s.heartbeat();
    std::thread::sleep(Duration::from_millis(20));
    camera.offer(&old, &jpeg, monotonic_us()).unwrap();
    // Debug-build inference under parallel tests can take hundreds of ms.
    s.run(Some(&mut camera), &jpeg, 2_000_000);
    let first = s
        .events
        .iter()
        .find(|e| {
            matches!(
                e,
                PerceptionEvent::Discarded { .. } | PerceptionEvent::Observation { .. }
            )
        })
        .unwrap();
    assert!(
        matches!(
            first,
            PerceptionEvent::Discarded {
                reason: DiscardReason::PrivacyChanged,
                ..
            }
        ),
        "{first:?}"
    );
    // Later frames carry the new grant and are analysed normally.
    let current = s.owner.camera_grant(now()).unwrap();
    let observations = s.observations();
    assert!(observations.iter().all(|o| o.frame.grant == current));
    assert!(!observations.is_empty());
}

#[test]
fn closed_camera_privacy_means_no_requests_and_no_observations() {
    let jpeg = common::face_jpeg(640.0, 360.0, 150.0);
    let mut s = Supervisor::spawn("closed", &vendored());
    s.owner
        .set_camera_permission(now(), Permission::Denied)
        .unwrap();
    assert!(matches!(s.startup(), Some(WorkerEvent::Ready)));
    let mut camera = s.camera();
    s.run(Some(&mut camera), &jpeg, 600_000);
    assert_eq!(camera.counts().requests, 0);
    assert!(s.observations().is_empty());
}

#[test]
fn losing_the_camera_link_is_reported_and_the_worker_keeps_running() {
    let jpeg = common::face_jpeg(640.0, 360.0, 150.0);
    let mut s = Supervisor::spawn("lost", &vendored());
    assert!(matches!(s.startup(), Some(WorkerEvent::Ready)));
    let mut camera = s.camera();
    s.run(Some(&mut camera), &jpeg, 500_000);
    camera.close_all(lamp_camera::frames::CloseReason::Fault, monotonic_us());
    drop(camera);
    // A frame received before the close may still finish analysis.
    s.run(None, &jpeg, 1_500_000);
    assert!(s.events.iter().any(|e| matches!(
        e,
        PerceptionEvent::CameraLink {
            connected: false,
            ..
        }
    )));
    let settled = s.observations().len();
    s.run(None, &jpeg, 1_200_000);
    assert_eq!(
        s.observations().len(),
        settled,
        "no observation without frames"
    );
    assert!(
        s.child.try_wait().unwrap().is_none(),
        "camera loss must not end perception"
    );
    assert!(
        s.events
            .iter()
            .any(|e| matches!(e, PerceptionEvent::Progress { .. }))
    );
}

#[test]
fn controller_heartbeat_loss_ends_the_worker() {
    let mut s = Supervisor::spawn("beat", &vendored());
    assert!(matches!(s.startup(), Some(WorkerEvent::Ready)));
    let started = monotonic_us();
    let status = s
        .exit_within(2_000_000)
        .expect("worker kept running without authority");
    assert!(!status.success());
    assert!(monotonic_us() - started < 1_000_000);
}

#[test]
fn a_missing_model_reports_a_fault_and_never_readiness() {
    let empty = common::Private::new("nomodel");
    std::fs::set_permissions(
        &empty.0,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let mut s = Supervisor::spawn("fault", &empty.0);
    // The worker may exit before a supervisor connects; it must never be Ready.
    match s.startup() {
        None => {}
        Some(WorkerEvent::Fault { code }) => {
            assert!(code.starts_with("perception_model_unavailable"), "{code}")
        }
        Some(other) => panic!("{other:?}"),
    }
    assert!(!s.exit_within(2_000_000).unwrap().success());
}
