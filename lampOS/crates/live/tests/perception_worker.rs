//! The real perception worker role (`lamp-live worker perception ...`), driven
//! like a supervisor would:
//! the runtime's private datagram channels and envelopes, authority snapshots
//! from the real `Controller`, and a real `FrameServer` standing in for the
//! camera worker. Frames are drawn scenes (no photographs).
use lamp_perception::testing as common;

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
use lamp_perception::observation::{DiscardReason, PerceptionEvent, Presence};
use std::{
    num::NonZeroU64,
    process::{Child, Command, Stdio},
    time::Duration,
};

const BIN: &str = env!("CARGO_BIN_EXE_lamp-live");

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
        Self::spawn_with(label, model_dir, &[])
    }

    fn spawn_with(label: &str, model_dir: &std::path::Path, extra: &[String]) -> Self {
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
            .args(extra)
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

#[test]
fn fresh_frames_become_observations_with_their_original_lineage() {
    let jpeg = common::face_jpeg(640.0, 360.0, 150.0);
    let model = common::installed_model();
    let mut s = Supervisor::spawn("obs", &model.0);
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
    let model = common::installed_model();
    let mut s = Supervisor::spawn("priv", &model.0);
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
    let model = common::installed_model();
    let mut s = Supervisor::spawn("closed", &model.0);
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
    let model = common::installed_model();
    let mut s = Supervisor::spawn("lost", &model.0);
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
    let model = common::installed_model();
    let mut s = Supervisor::spawn("beat", &model.0);
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

const CONTEXT: u8 = 0x34;

fn context_client(s: &Supervisor) -> lamp_camera::frames::FrameClient {
    lamp_camera::frames::FrameClient::new(
        &s.dir.join("perception.frames"),
        Role::Context,
        boot(CONTEXT),
        s.parent,
        64 * 1024,
    )
}

/// Serve camera frames (when given) and wait for one context frame.
fn await_context(
    s: &mut Supervisor,
    mut camera: Option<&mut FrameServer>,
    jpeg: &[u8],
    client: &mut lamp_camera::frames::FrameClient,
    micros: u64,
) -> Option<(lamp_camera::frames::FrameMeta, Vec<u8>)> {
    let until = monotonic_us() + micros;
    let mut out = Vec::new();
    while monotonic_us() < until {
        if !client.is_connected() {
            client.connect().unwrap();
        }
        if client.is_connected() && client.outstanding().is_none() {
            client.request(2_000_000).unwrap();
        }
        if let Some(meta) = client.poll(&mut out).unwrap() {
            return Some((meta, out));
        }
        s.run(camera.as_deref_mut(), jpeg, 3_000);
    }
    None
}

#[test]
fn context_frames_are_small_fresh_and_keep_the_original_lineage() {
    let jpeg = common::face_jpeg(640.0, 360.0, 150.0);
    let model = common::installed_model();
    let flag = format!("context={}", hex(boot(CONTEXT)));
    let mut s = Supervisor::spawn_with("ctx", &model.0, &["--frame-consumer".into(), flag]);
    assert!(matches!(s.startup(), Some(WorkerEvent::Ready)));
    let mut camera = s.camera();
    let mut client = context_client(&s);
    let (meta, bytes) = await_context(&mut s, Some(&mut camera), &jpeg, &mut client, 5_000_000)
        .expect("context frame");
    assert!(bytes.len() <= 64 * 1024, "{} bytes", bytes.len());
    let derived = meta.derived.expect("derived image");
    assert_eq!((derived.source_width, derived.source_height), (1280, 720));
    assert_eq!((meta.width, meta.height), (640, 360));
    let decoded = lamp_perception::image::decode_jpeg(&bytes).unwrap();
    assert_eq!((decoded.width, decoded.height), (640, 360));
    // Original lineage: the camera worker, a frame the camera actually sent, its grant.
    assert_eq!(meta.camera_worker, boot(0x33));
    assert!(meta.frame_sequence >= 1 && meta.frame_sequence <= s.frames);
    assert_eq!(meta.grant, s.owner.camera_grant(now()).unwrap());
    assert!(meta.copied_at_us - meta.dequeue_completed_at_us <= 2_000_000);
    assert!(
        s.events
            .iter()
            .any(|e| matches!(e, PerceptionEvent::ContextLink { .. }))
    );
}

#[test]
fn no_context_frame_from_before_a_privacy_change_is_served() {
    let jpeg = common::face_jpeg(640.0, 360.0, 150.0);
    let model = common::installed_model();
    let flag = format!("context={}", hex(boot(CONTEXT)));
    let mut s = Supervisor::spawn_with("ctxp", &model.0, &["--frame-consumer".into(), flag]);
    assert!(matches!(s.startup(), Some(WorkerEvent::Ready)));
    let mut camera = s.camera();
    let mut client = context_client(&s);
    await_context(&mut s, Some(&mut camera), &jpeg, &mut client, 5_000_000).expect("first frame");
    // Close and reopen camera privacy, then stop supplying camera frames.
    s.owner
        .set_camera_permission(now(), Permission::Denied)
        .unwrap();
    s.owner
        .set_camera_permission(now(), Permission::Allowed)
        .unwrap();
    s.last_authority = 0;
    s.heartbeat();
    assert!(
        await_context(&mut s, None, &jpeg, &mut client, 800_000).is_none(),
        "a retained image from the old privacy lineage was served"
    );
    // Fresh frames under the new grant are served again.
    let (meta, _) =
        await_context(&mut s, Some(&mut camera), &jpeg, &mut client, 5_000_000).expect("new frame");
    assert_eq!(meta.grant, s.owner.camera_grant(now()).unwrap());
}

#[test]
fn the_largest_observation_fits_one_runtime_datagram() {
    use lamp_camera::frames::{WireDomain, WirePoint, WireTimestamp};
    use lamp_interaction::{BootId, Confidence, Controller, MonoTime, Observation, Permission};
    use lamp_perception::observation::*;
    let mut controller = Controller::new(BootId::new([9; 16]).unwrap(), MonoTime::from_micros(0));
    controller
        .set_camera_permission(MonoTime::from_micros(1), Permission::Allowed)
        .unwrap();
    let face = FaceEvidence {
        score_per_mille: 1000,
        bounds: [10_000; 4],
        landmarks: [[10_000; 2]; 5],
        yaw_tenths: Some(900),
    };
    let event = PerceptionEvent::Observation {
        observation: VisualObservation {
            provenance: Observation::new(
                BootId::new([255; 16]).unwrap(),
                std::num::NonZeroU64::new(u64::MAX).unwrap(),
                MonoTime::from_micros(u64::MAX),
                Confidence::per_mille(1000).unwrap(),
            ),
            frame: FrameRef {
                camera_worker: BootId::new([255; 16]).unwrap(),
                capture_epoch: u64::MAX,
                frame_sequence: u64::MAX,
                grant: controller.camera_grant(MonoTime::from_micros(1)).unwrap(),
                width: 1280,
                height: 720,
                driver_timestamp: Some(WireTimestamp {
                    micros: u64::MAX,
                    domain: WireDomain::HostMonotonic,
                    point: WirePoint::StartOfExposure,
                }),
                dequeue_completed_at_us: u64::MAX,
            },
            timing: AnalysisTiming {
                received_at_us: u64::MAX,
                decode_us: u32::MAX,
                inference_us: u32::MAX,
                analysed_at_us: u64::MAX,
                published_at_us: u64::MAX,
            },
            exposure: lamp_perception::image::Exposure {
                mean_luma: 255,
                dark_per_mille: 1000,
                bright_per_mille: 1000,
                verdict: lamp_perception::image::ExposureVerdict::Overexposed,
            },
            presence: Presence::Faces {
                count: 255,
                best_per_mille: 1000,
            },
            faces: vec![face; MAX_FACES],
            attention: Attention::Away {
                face: 255,
                yaw_tenths: 900,
            },
            model: ModelRef {
                id: "yunet-2023mar".into(),
                sha256_prefix: "8f2383e4dd3cfbb4".into(),
                input_width: 1280,
                input_height: 1280,
            },
        },
    };
    let envelope = lamp_live::wire::Envelope {
        boot: BootId::new([255; 16]).unwrap(),
        sequence: u64::MAX,
        sent_at_us: u64::MAX,
        payload: &event,
    };
    let encoded = lamp_live::wire::encode(&envelope).unwrap();
    assert!(
        encoded.len() < lamp_ipc::MAX_DATAGRAM_BYTES / 2,
        "{} bytes",
        encoded.len()
    );
    let decoded: lamp_live::wire::Envelope<PerceptionEvent> =
        lamp_live::wire::decode(&encoded).unwrap();
    assert_eq!(decoded.payload, event);
    // Diagnostic text is bounded too.
    assert_eq!(bounded_text(&"x".repeat(10_000)).len(), 160);
}

/// The production supervisor path: `process::Worker::spawn` (bounded socket
/// and readiness handshake, 3 s) and cooperative `shutdown` (300 ms).
#[test]
fn the_runtime_supervisor_spawns_and_stops_the_perception_worker() {
    let model = common::installed_model();
    let directory = lamp_live::process::SessionDirectory::create().unwrap();
    let parent = lamp_live::process::new_boot().unwrap();
    let model_dir = model.0.display().to_string();
    let started = std::time::Instant::now();
    let mut worker = lamp_live::process::Worker::spawn(
        std::path::Path::new(BIN),
        &directory.path,
        "perception",
        parent,
        &["--model-dir", &model_dir],
    )
    .unwrap();
    println!(
        "MEASURED perception_worker_spawn_to_ready_ms={} (Worker::spawn, debug={}, host)",
        started.elapsed().as_millis(),
        cfg!(debug_assertions)
    );
    let mut owner = Controller::new(parent, now());
    owner
        .set_camera_permission(now(), Permission::Allowed)
        .unwrap();
    for _ in 0..10 {
        let snapshot = owner.snapshot(now()).unwrap();
        worker
            .channels
            .control
            .send(Control::Authority { snapshot })
            .unwrap();
        while worker
            .channels
            .data
            .receive::<PerceptionEvent>()
            .unwrap()
            .is_some()
        {}
        std::thread::sleep(Duration::from_millis(20));
    }
    worker.check_running().unwrap();
    worker.shutdown().unwrap();
}
