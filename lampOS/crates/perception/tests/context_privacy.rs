//! MAIN-015 P1: a context frame still being written when camera authority
//! changes is abandoned, never finished. Real perception loop in process, a
//! real camera `FrameServer`, and a real provider `FrameClient` that stops
//! reading so the frame stalls mid-write (larger than the socket buffer).
mod common;

use lamp_camera::{
    CallTiming, CaptureId, FrameId, FrameObservation, NegotiatedMode, PixelFormat, SourceId,
    frames::{CloseReason, FrameClient, FrameServer, LinkEvent, Role},
};
use lamp_interaction::{BootId, Controller, MonoTime, Permission};
use lamp_ipc::monotonic_us;
use lamp_perception::{
    analyze::{Analyzer, INPUT_WIDTH},
    model::{self, YUNET},
    observation::PerceptionEvent,
    worker::{self, CONTEXT_SOCKET, PerceptionControl, PerceptionLink, WorkerOptions},
};
use std::{
    collections::VecDeque,
    io,
    num::NonZeroU64,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Shared {
    queue: VecDeque<PerceptionControl>,
    published: Vec<PerceptionEvent>,
}
struct ScriptLink(Arc<Mutex<Shared>>);
impl PerceptionLink for ScriptLink {
    fn receive_control(&mut self) -> io::Result<Option<PerceptionControl>> {
        Ok(self.0.lock().unwrap().queue.pop_front())
    }
    fn publish(&mut self, event: PerceptionEvent) -> io::Result<()> {
        self.0.lock().unwrap().published.push(event);
        Ok(())
    }
}

fn boot(n: u8) -> BootId {
    BootId::new([n; 16]).unwrap()
}
fn now() -> MonoTime {
    MonoTime::from_micros(monotonic_us())
}
const CONTROLLER: u8 = 0x51;
const PERCEPTION: u8 = 0x52;
const PROVIDER: u8 = 0x54;

/// A detailed 1280x720 scene whose reduced context JPEG exceeds the frame
/// link's socket buffers.
fn detailed_scene() -> Vec<u8> {
    let (w, h) = (1280_u32, 720_u32);
    let mut pixels = common::scene(w, h, [110, 120, 130], Some((640.0, 360.0, 150.0)));
    let mut state = 0x9e37_79b9_u32;
    for byte in pixels.iter_mut() {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *byte = byte.saturating_add((state % 60) as u8).saturating_sub(30);
    }
    common::jpeg(&pixels, w, h)
}

struct Rig {
    _dir: common::Private,
    _model: common::Private,
    dir_path: std::path::PathBuf,
    shared: Arc<Mutex<Shared>>,
    owner: Controller,
    camera: FrameServer,
    worker: Option<std::thread::JoinHandle<io::Result<()>>>,
    jpeg: Vec<u8>,
    sequence: u64,
    refresh: bool,
}

impl Rig {
    fn start(label: &str) -> Self {
        // Prepared before the worker starts: a supervisor must not delay its
        // first authority past the worker's 250 ms heartbeat bound.
        let jpeg = detailed_scene();
        let dir = common::Private::new(label);
        let model = common::installed_model();
        let mut analyzer = Analyzer::new(
            model::read_verified(&model.0, &YUNET).unwrap(),
            &YUNET,
            INPUT_WIDTH,
        );
        analyzer.prepare(1280, 720).unwrap();
        let socket = dir.join("camera.frames");
        let camera = FrameServer::bind(
            &socket,
            boot(CONTROLLER),
            &[(Role::Perception, boot(PERCEPTION))],
            2 * 1024 * 1024,
        )
        .unwrap();
        let mut owner = Controller::new(boot(CONTROLLER), now());
        owner
            .set_camera_permission(now(), Permission::Allowed)
            .unwrap();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let mut link = ScriptLink(shared.clone());
        let mut options = WorkerOptions::new(socket, boot(CONTROLLER), boot(PERCEPTION));
        options.interval_us = 100_000;
        options.context = Some((boot(PROVIDER), dir.join(CONTEXT_SOCKET)));
        let worker = std::thread::spawn(move || worker::run(&mut link, analyzer, options));
        let dir_path = dir.0.clone();
        let mut rig = Self {
            _dir: dir,
            _model: model,
            dir_path,
            shared,
            owner,
            camera,
            worker: Some(worker),
            jpeg,
            sequence: 0,
            refresh: true,
        };
        rig.authority();
        rig
    }

    fn authority(&mut self) {
        let snapshot = self.owner.snapshot(now()).unwrap();
        self.shared
            .lock()
            .unwrap()
            .queue
            .push_back(PerceptionControl::Authority(snapshot));
    }

    fn observation(&mut self) -> FrameObservation {
        self.sequence += 1;
        let at = monotonic_us();
        FrameObservation {
            id: FrameId {
                capture: CaptureId {
                    worker: boot(0x53),
                    epoch: NonZeroU64::new(1).unwrap(),
                },
                sequence: NonZeroU64::new(self.sequence).unwrap(),
            },
            source: SourceId::new("sim").unwrap(),
            grant: self.owner.camera_grant(now()).unwrap(),
            mode: NegotiatedMode {
                source: SourceId::new("sim").unwrap(),
                format: PixelFormat::MJPG,
                width: 1280,
                height: 720,
                interval: None,
                size_image: 2 * 1024 * 1024,
                buffers: 2,
            },
            driver_sequence: self.sequence as u32,
            driver_timestamp: None,
            dequeue: CallTiming {
                started_at: MonoTime::from_micros(at - 10),
                completed_at: MonoTime::from_micros(at),
            },
        }
    }

    /// One millisecond of supervision: fresh authority and camera frames.
    fn tick(&mut self, serve: bool) {
        if self.refresh && self.shared.lock().unwrap().queue.is_empty() {
            self.authority();
        }
        self.camera.service(monotonic_us());
        if serve && self.camera.wants_frame() && self.owner.camera_grant(now()).is_ok() {
            let observation = self.observation();
            let jpeg = self.jpeg.clone();
            self.camera
                .offer(&observation, &jpeg, monotonic_us())
                .unwrap();
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    fn observed(&self) -> bool {
        self.shared
            .lock()
            .unwrap()
            .published
            .iter()
            .any(|e| matches!(e, PerceptionEvent::Observation { .. }))
    }

    /// Analyse one frame, then have the provider ask for context and stop
    /// reading, leaving the context frame stalled mid-write.
    fn stall_context(&mut self) -> FrameClient {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.observed() && Instant::now() < deadline {
            self.tick(true);
            if self.worker.as_ref().is_some_and(|w| w.is_finished()) {
                let result = self.worker.take().unwrap().join();
                panic!("perception worker ended early: {result:?}");
            }
        }
        assert!(
            self.observed(),
            "no observation: {:?} camera {:?} jpeg {} bytes",
            self.shared
                .lock()
                .unwrap()
                .published
                .iter()
                .take(6)
                .collect::<Vec<_>>(),
            self.camera.counts(),
            self.jpeg.len()
        );
        let mut client = FrameClient::new(
            &self.dir_path.join(CONTEXT_SOCKET),
            Role::Context,
            boot(PROVIDER),
            boot(CONTROLLER),
            64 * 1024,
        );
        while !matches!(client.connect(), Ok(true)) && Instant::now() < deadline {
            self.tick(true);
        }
        client.request(2_000_000).unwrap();
        // Wait until the worker reports the context frame handed to the link
        // and still mid-write (it keeps serving camera frames meanwhile).
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let served = self
                .shared
                .lock()
                .unwrap()
                .published
                .iter()
                .find_map(|e| match e {
                    PerceptionEvent::ContextServed {
                        in_flight, bytes, ..
                    } => Some((*in_flight, *bytes)),
                    _ => None,
                });
            if let Some((in_flight, bytes)) = served {
                assert!(
                    in_flight,
                    "a {bytes}-byte context frame fit the socket buffers; nothing stalled"
                );
                return client;
            }
            assert!(Instant::now() < deadline, "no context frame was served");
            self.tick(true);
        }
    }

    fn context_events(&self) -> Vec<LinkEvent> {
        self.shared
            .lock()
            .unwrap()
            .published
            .iter()
            .filter_map(|e| match e {
                PerceptionEvent::ContextLink { event } => Some(*event),
                _ => None,
            })
            .collect()
    }

    fn stop(&mut self) {
        self.shared
            .lock()
            .unwrap()
            .queue
            .push_back(PerceptionControl::Stop);
        if let Some(worker) = self.worker.take() {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !worker.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            let _ = worker.join();
        }
    }
}

/// Drain the client: it must end without ever completing the frame.
fn assert_cut_off(client: &mut FrameClient) {
    let mut out = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    let result = loop {
        match client.poll(&mut out) {
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(1)),
            other => break other,
        }
    };
    assert!(
        result.is_err(),
        "the stalled context frame was completed: {result:?}"
    );
    assert!(out.is_empty());
}

fn assert_discarded(events: &[LinkEvent], reason: CloseReason) {
    assert!(
        events.iter().any(|e| matches!(e,
            LinkEvent::Closed { role: Some(Role::Context), reason: r, partial_frame_discarded: true, .. } if *r == reason)),
        "{events:?}"
    );
}

#[test]
fn a_camera_denial_abandons_the_unsent_context_tail() {
    let mut rig = Rig::start("cpd");
    let mut client = rig.stall_context();
    rig.owner
        .set_camera_permission(now(), Permission::Denied)
        .unwrap();
    rig.authority();
    for _ in 0..100 {
        rig.tick(false);
    }
    assert_discarded(&rig.context_events(), CloseReason::PrivacyClosed);
    assert_cut_off(&mut client);
    rig.stop();
}

#[test]
fn a_newer_camera_grant_abandons_the_unsent_context_tail() {
    let mut rig = Rig::start("cpn");
    let mut client = rig.stall_context();
    rig.owner
        .set_camera_permission(now(), Permission::Denied)
        .unwrap();
    rig.owner
        .set_camera_permission(now(), Permission::Allowed)
        .unwrap();
    rig.authority();
    for _ in 0..100 {
        rig.tick(false);
    }
    assert_discarded(&rig.context_events(), CloseReason::PrivacyClosed);
    assert_cut_off(&mut client);
    // Policy: the same provider may reconnect and get a frame under the new grant.
    let mut again = FrameClient::new(
        &rig.dir_path.join(CONTEXT_SOCKET),
        Role::Context,
        boot(PROVIDER),
        boot(CONTROLLER),
        64 * 1024,
    );
    let current = rig.owner.camera_grant(now()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut out = Vec::new();
    let meta = loop {
        rig.tick(true);
        if !again.is_connected() {
            let _ = again.connect();
        } else if again.outstanding().is_none() {
            again.request(2_000_000).unwrap();
        }
        if let Ok(Some(meta)) = again.poll(&mut out) {
            break Some(meta);
        }
        if Instant::now() > deadline {
            break None;
        }
    }
    .expect("context after reconnect");
    assert_eq!(meta.grant, current);
    rig.stop();
}

#[test]
fn lapsed_authority_abandons_the_unsent_context_tail() {
    let mut rig = Rig::start("cpe");
    let mut client = rig.stall_context();
    // No further authority: the installed snapshot expires before the
    // worker's own heartbeat bound ends it.
    // Either the snapshot lapses first (discarded as a privacy change) or
    // the worker's 250 ms heartbeat ends it (closed as stopped); in both
    // cases the unsent tail is abandoned.
    rig.refresh = false;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline
        && !rig.context_events().iter().any(|e| {
            matches!(
                e,
                LinkEvent::Closed {
                    role: Some(Role::Context),
                    ..
                }
            )
        })
    {
        rig.tick(false);
    }
    let events = rig.context_events();
    let closed = events
        .iter()
        .find(|e| {
            matches!(
                e,
                LinkEvent::Closed {
                    role: Some(Role::Context),
                    ..
                }
            )
        })
        .copied();
    assert!(
        matches!(
            closed,
            Some(LinkEvent::Closed {
                reason: CloseReason::PrivacyClosed | CloseReason::Stopped,
                partial_frame_discarded: true,
                ..
            })
        ),
        "{events:?}"
    );
    assert_cut_off(&mut client);
    rig.stop();
}

#[test]
fn a_queued_stop_while_output_stalls_never_completes_the_frame() {
    let mut rig = Rig::start("cps");
    let mut client = rig.stall_context();
    rig.stop();
    assert_cut_off(&mut client);
}
