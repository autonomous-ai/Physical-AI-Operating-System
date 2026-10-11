//! Control priority in the real perception loop (in process): a Stop or a
//! privacy change queued behind more messages than one control slice must be
//! read before any further analysis is submitted or published.
mod common;

use lamp_camera::{
    CallTiming, CaptureId, FrameId, FrameObservation, NegotiatedMode, PixelFormat, SourceId,
    frames::{FrameServer, Role},
};
use lamp_interaction::{BootId, Controller, MonoTime, Permission, Snapshot};
use lamp_ipc::monotonic_us;
use lamp_perception::{
    analyze::{Analyzer, INPUT_WIDTH},
    model::{self, YUNET},
    observation::PerceptionEvent,
    worker::{self, CONTROL_BUDGET, PerceptionControl, PerceptionLink, WorkerOptions},
};
use std::{
    collections::VecDeque,
    io,
    num::NonZeroU64,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Copy, PartialEq)]
enum Pending {
    None,
    Stop,
    Deny,
}

#[derive(Default)]
struct Shared {
    queue: VecDeque<(PerceptionControl, Pending)>,
    /// (event, what priority message was still unread when it was published)
    published: Vec<(PerceptionEvent, Pending)>,
}

struct ScriptLink(Arc<Mutex<Shared>>);

impl ScriptLink {
    fn unread(shared: &Shared) -> Pending {
        shared
            .queue
            .iter()
            .map(|(_, p)| *p)
            .find(|p| *p != Pending::None)
            .unwrap_or(Pending::None)
    }
}

impl PerceptionLink for ScriptLink {
    fn receive_control(&mut self) -> io::Result<Option<PerceptionControl>> {
        Ok(self.0.lock().unwrap().queue.pop_front().map(|(c, _)| c))
    }
    fn publish(&mut self, event: PerceptionEvent) -> io::Result<()> {
        let mut shared = self.0.lock().unwrap();
        let unread = Self::unread(&shared);
        shared.published.push((event, unread));
        Ok(())
    }
}

fn boot(n: u8) -> BootId {
    BootId::new([n; 16]).unwrap()
}
fn now() -> MonoTime {
    MonoTime::from_micros(monotonic_us())
}

struct Rig {
    _dir: common::Private,
    _model: common::Private,
    shared: Arc<Mutex<Shared>>,
    owner: Controller,
    camera: FrameServer,
    worker: std::thread::JoinHandle<io::Result<()>>,
    jpeg: Vec<u8>,
    sequence: u64,
}

impl Rig {
    fn start(label: &str) -> Self {
        let dir = common::Private::new(label);
        let model = common::installed_model();
        let bytes = model::read_verified(&model.0, &YUNET).unwrap();
        let mut analyzer = Analyzer::new(bytes, &YUNET, INPUT_WIDTH);
        // A small frame keeps decode and inference to a few milliseconds.
        analyzer.prepare(160, 96).unwrap();
        let jpeg = common::jpeg(
            &common::scene(160, 96, [110, 120, 130], Some((80.0, 48.0, 25.0))),
            160,
            96,
        );
        let (controller, perception) = (boot(0x41), boot(0x42));
        let socket = dir.join("camera.frames");
        let camera = FrameServer::bind(
            &socket,
            controller,
            &[(Role::Perception, perception)],
            1 << 20,
        )
        .unwrap();
        let mut owner = Controller::new(controller, now());
        owner
            .set_camera_permission(now(), Permission::Allowed)
            .unwrap();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let mut link = ScriptLink(shared.clone());
        let mut options = WorkerOptions::new(socket, controller, perception);
        options.interval_us = 100_000;
        let worker = std::thread::spawn(move || worker::run(&mut link, analyzer, options));
        let mut rig = Self {
            _dir: dir,
            _model: model,
            shared,
            owner,
            camera,
            worker,
            jpeg,
            sequence: 0,
        };
        rig.authority();
        rig
    }

    fn snapshot(&mut self) -> Snapshot {
        self.owner.snapshot(now()).unwrap()
    }

    fn authority(&mut self) {
        let snapshot = self.snapshot();
        self.push(PerceptionControl::Authority(snapshot), Pending::None);
    }

    fn push(&self, control: PerceptionControl, pending: Pending) {
        self.shared
            .lock()
            .unwrap()
            .queue
            .push_back((control, pending));
    }

    fn observation(&mut self) -> FrameObservation {
        self.sequence += 1;
        let at = monotonic_us();
        FrameObservation {
            id: FrameId {
                capture: CaptureId {
                    worker: boot(0x43),
                    epoch: NonZeroU64::new(1).unwrap(),
                },
                sequence: NonZeroU64::new(self.sequence).unwrap(),
            },
            source: SourceId::new("sim").unwrap(),
            grant: self.owner.camera_grant(now()).unwrap(),
            mode: NegotiatedMode {
                source: SourceId::new("sim").unwrap(),
                format: PixelFormat::MJPG,
                width: 160,
                height: 96,
                interval: None,
                size_image: 1 << 20,
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

    /// Keep authority fresh and serve one frame when asked; true once served.
    fn tick(&mut self, serve: bool) -> bool {
        if self.shared.lock().unwrap().queue.is_empty() {
            self.authority();
        }
        self.camera.service(monotonic_us());
        if serve && self.camera.wants_frame() {
            let observation = self.observation();
            let jpeg = self.jpeg.clone();
            self.camera
                .offer(&observation, &jpeg, monotonic_us())
                .unwrap();
            return true;
        }
        false
    }

    fn observations(&self) -> usize {
        self.shared
            .lock()
            .unwrap()
            .published
            .iter()
            .filter(|(e, _)| matches!(e, PerceptionEvent::Observation { .. }))
            .count()
    }

    /// Run until one observation is published, so the pipeline is warm.
    fn warm(&mut self) {
        for _ in 0..3000 {
            self.tick(true);
            if self.observations() > 0 {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("no first observation");
    }

    /// Serve one frame, then immediately bury a message behind more than one
    /// control slice of duplicate (ignored) snapshots. The buried message is
    /// made after the duplicates, so it is the newest authority.
    fn bury(&mut self, make: impl FnOnce(&mut Self) -> (PerceptionControl, Pending)) {
        while !self.tick(true) {
            std::thread::sleep(Duration::from_micros(200));
        }
        let duplicate = self.snapshot();
        {
            let mut shared = self.shared.lock().unwrap();
            for _ in 0..=CONTROL_BUDGET * 40 {
                shared
                    .queue
                    .push_back((PerceptionControl::Authority(duplicate), Pending::None));
            }
        }
        let (message, pending) = make(self);
        self.push(message, pending);
    }

    fn violations(&self) -> Vec<String> {
        self.shared
            .lock()
            .unwrap()
            .published
            .iter()
            .filter(|(e, p)| {
                *p != Pending::None && matches!(e, PerceptionEvent::Observation { .. })
            })
            .map(|(e, _)| format!("{e:?}"))
            .collect()
    }
}

#[test]
fn a_queued_stop_is_read_before_any_further_observation_is_published() {
    for round in 0..5 {
        let mut rig = Rig::start(&format!("stop{round}"));
        rig.warm();
        rig.bury(|_| (PerceptionControl::Stop, Pending::Stop));
        let violations = loop {
            if rig.worker.is_finished() {
                break rig.violations();
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        let result = rig.worker.join().unwrap();
        assert!(result.is_ok(), "{result:?}");
        assert!(
            violations.is_empty(),
            "round {round}: published while Stop was unread: {violations:?}"
        );
    }
}

#[test]
fn a_queued_privacy_close_is_read_before_the_old_frame_is_published() {
    for round in 0..5 {
        let mut rig = Rig::start(&format!("deny{round}"));
        rig.warm();
        rig.bury(|rig| {
            rig.owner
                .set_camera_permission(now(), Permission::Denied)
                .unwrap();
            (PerceptionControl::Authority(rig.snapshot()), Pending::Deny)
        });
        // Let the loop drain everything (authority stays fresh meanwhile).
        for _ in 0..400 {
            rig.tick(false);
            std::thread::sleep(Duration::from_millis(1));
        }
        let violations = rig.violations();
        assert!(
            violations.is_empty(),
            "round {round}: published while a privacy close was unread: {violations:?}"
        );
        rig.push(PerceptionControl::Stop, Pending::None);
        assert!(rig.worker.join().unwrap().is_ok());
    }
}
