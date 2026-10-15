//! Clock entry-point regressions; inert local pipes/datagrams only.
use super::*;
use std::{
    cell::Cell,
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    assets: AssetIndex,
}

impl Fixture {
    fn new() -> Self {
        // Keep Unix socket paths short and private, as in the integration fixtures.
        let root = PathBuf::from(format!(
            "/tmp/lve-clock-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(root.join("objects")).unwrap();
        let assets = AssetIndex::load(&root, &[], &StimulusCatalog::load().unwrap()).unwrap();
        Self { root, assets }
    }

    fn config(&self) -> PhysicalConfig {
        PhysicalConfig {
            lamp_command: Vec::new(),
            lamp_work_root: self.root.display().to_string(),
            mode: LampMode::Fixture {
                reply: "/nonexistent/reply.wav".into(),
                sha256: "0".repeat(64),
            },
            noise_suppression: "off".into(),
            diagnostics: false,
            room_recorder: None,
            room_independent: false,
            allowance_seconds: 5,
            ring_channel_ceiling: None,
            runtime_flags: Default::default(),
            stimulus_source: StimulusSource::LoudspeakerSynthetic,
        }
    }

    fn player(&self) -> FakeRoomPlayer {
        FakeRoomPlayer {
            room: self.root.join("room.sock"),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn session(lines: Receiver<std::result::Result<SessionLine, String>>) -> Session {
    // This owned pipe consumer exits when PhysicalBackend closes its stdin.
    // It emits no protocol, launches no children and touches no hardware.
    let mut child = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    Session {
        stdin: child.stdin.take(),
        child,
        lines,
        next_ping_us: 0,
        pings: Vec::new(),
        best: None,
        trace: Vec::new(),
        end: None,
        end_synthesized: false,
        ready_seen: false,
        protocol_errors: Vec::new(),
        start: None,
        cues: Vec::new(),
        cue_order: CueOrder::default(),
        cue_failed: false,
        boot_changes: Vec::new(),
        stop_sent: false,
        runtime_ended: false,
        stdout_readiness_us: Vec::new(),
        clock_samples: Vec::new(),
        started_runner_us: None,
        held: std::collections::VecDeque::new(),
        released: std::collections::VecDeque::new(),
    }
}

fn start_line() -> SessionLine {
    SessionLine::SessionStart {
        schema: crate::physical::protocol::SCHEMA,
        lamp_us: 10_000,
        mode: "fixture".into(),
        cue_socket: true,
        runtime_argv: Vec::new(),
    }
}

fn cue(sequence: u64, kind: &str, at: u64) -> SessionLine {
    SessionLine::Cue {
        lamp_received_us: at + 2,
        cue: json!({"schema":1,"sequence":sequence,"kind":kind,
            "boot":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
            "turn":null,"generation":null,"capture_epoch":1,
            "reference_epoch_context":1,"event_us":at,
            "sent_us":at + 1,"expires_us":at + crate::events::CUE_FRESH_US}),
    }
}

#[test]
fn backend_reads_only_the_supplied_clock_and_the_default_stays_on_host_time() {
    let fixture = Fixture::new();
    let at = Cell::new(7);
    let reads = Cell::new(0);
    let mut backend = PhysicalBackend::new_with_clock(
        fixture.config(),
        &fixture.assets,
        Box::new(fixture.player()),
        || {
            reads.set(reads.get() + 1);
            MonoTime::from_micros(at.get())
        },
    );
    assert_eq!(backend.now_us(), 7);
    assert_eq!(backend.now_us(), 7);
    assert_eq!(at.get(), 7);
    assert_eq!(reads.get(), 2);
    at.set(19);
    assert_eq!(backend.now_us(), 19);
    assert_eq!(reads.get(), 3);

    let mut default = PhysicalBackend::new(
        fixture.config(),
        &fixture.assets,
        Box::new(fixture.player()),
    );
    let before = monotonic_us();
    let observed = default.now_us();
    let after = monotonic_us();
    assert!((before..=after).contains(&observed));
}

#[test]
fn ping_receipts_and_each_cue_decision_take_their_own_clock_sample() {
    let fixture = Fixture::new();
    let at = Cell::new(10_000);
    let reads = Cell::new(0);
    let mut backend = PhysicalBackend::new_with_clock(
        fixture.config(),
        &fixture.assets,
        Box::new(fixture.player()),
        || {
            reads.set(reads.get() + 1);
            MonoTime::from_micros(at.get())
        },
    );
    let (_sender, lines) = mpsc::channel();
    backend.session = Some(session(lines));
    assert!(backend.handle(start_line()).is_none());
    assert_eq!(reads.get(), 1);
    assert_eq!(
        backend.session.as_ref().unwrap().started_runner_us,
        Some(10_000)
    );

    at.set(20_000);
    assert!(backend.handle(cue(1, "listening_ready", 19_000)).is_none());
    assert_eq!(reads.get(), 2);
    assert_eq!(backend.session.as_ref().unwrap().held.len(), 1);
    at.set(30_000);
    backend.send_ping(1).unwrap();
    assert_eq!(reads.get(), 3);
    assert_eq!(backend.session.as_ref().unwrap().pings, [(1, 30_000)]);
    assert_eq!(backend.session.as_ref().unwrap().next_ping_us, 280_000);

    at.set(32_000);
    assert!(
        backend
            .handle(SessionLine::Pong {
                seq: 1,
                lamp_us: 31_000
            })
            .is_none()
    );
    // Pong receipt, then a fresh sample when the held cue is decided.
    assert_eq!(reads.get(), 5);
    let session = backend.session.as_ref().unwrap();
    assert_eq!(session.clock_samples[0]["sent_runner_us"], 30_000);
    assert_eq!(session.clock_samples[0]["received_runner_us"], 32_000);
    assert_eq!(session.clock_samples[0]["usable"], true);
    assert_eq!(session.cues[0]["accepted"], true);
    assert_eq!(session.cues[0]["runner_received_us"], 20_000);
    assert_eq!(session.cues[0]["freshness"]["decided_runner_us"], 32_000);
    assert_eq!(session.released[0].received_us, Some(20_000));

    at.set(40_000);
    let event = backend.handle(cue(2, "input_candidate", 39_000)).unwrap();
    assert_eq!(reads.get(), 7);
    assert_eq!(event.received_us, Some(40_000));
    let session = backend.session.as_ref().unwrap();
    assert_eq!(session.cues[1]["accepted"], true);
    assert_eq!(session.cues[1]["freshness"]["decided_runner_us"], 40_000);
    assert_eq!(at.get(), 40_000);
}

#[test]
fn post_end_drain_and_transport_reap_finish_with_a_frozen_clock() {
    let fixture = Fixture::new();
    let reads = Cell::new(0);
    let mut backend = PhysicalBackend::new_with_clock(
        fixture.config(),
        &fixture.assets,
        Box::new(fixture.player()),
        || {
            reads.set(reads.get() + 1);
            MonoTime::from_micros(7)
        },
    );
    // Keep the sender alive and silent, so the unchanged 300 ms host drain
    // must expire. The fixture clock stays frozen after the runtime ends.
    let (_sender, lines) = mpsc::channel();
    let mut session = session(lines);
    session.start = Some(serde_json::to_value(start_line()).unwrap());
    session.end = Some(SessionLine::SessionEnd {
        lamp_us: 7,
        exit_code: Some(0),
        killed: false,
        events_sha256: None,
        events_lines: 0,
        error: None,
        stray_processes_stopped: false,
        serve: None,
    });
    backend.session = Some(session);
    let catalog = StimulusCatalog::load().unwrap();
    let plan = crate::plan::LoadedPlan::parse(crate::plan::DEFAULT_PLAN, &catalog).unwrap();
    let context = AttemptContext {
        attempt_id: "clock-drain".into(),
        run_id: "clock".into(),
        scenario: &plan.plan.scenarios[0],
        repetition: 0,
        order: 0,
        seed: 1,
        run_directory: fixture.root.clone(),
    };
    let finished = backend.finish(&context).unwrap();
    assert_eq!(finished.aborted, None);
    assert!(backend.session.is_none());
    // The pre-end bound is sampled once; no post-end host wait reads it.
    assert_eq!(reads.get(), 1);
}

#[test]
fn immediate_player_keeps_datagrams_and_reports_in_the_selected_clock_domain() {
    let fixture = Fixture::new();
    let room = UnixDatagram::bind(fixture.root.join("room.sock")).unwrap();
    room.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let timing = StimulusCatalog::load()
        .unwrap()
        .estimate("fast-followup")
        .unwrap();
    let expected = serde_json::to_vec(&timing).unwrap();
    let mut buffer = [0; ROOM_MAX_BYTES];
    let unused = Path::new("/nonexistent/unused");
    let at = Cell::new(123_456);
    let reads = Cell::new(0);
    let mut player = fixture.player().with_clock(|| {
        reads.set(reads.get() + 1);
        MonoTime::from_micros(at.get())
    });
    assert_eq!(player.describe(), fixture.player().describe());
    for time in [123_456, 123_456, 123_457] {
        at.set(time);
        let report = player
            .start(unused, &timing, unused, unused)
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(
            report,
            json!({"valid":true,"status":"fake_room_delivery",
            "start_requested_host_monotonic_ns":time * 1000})
        );
        let size = room.recv(&mut buffer).unwrap();
        assert_eq!(&buffer[..size], &expected);
        assert_eq!(at.get(), time);
    }
    assert_eq!(reads.get(), 3);

    let mut default = fixture.player();
    let before = monotonic_us();
    let report = default
        .start(unused, &timing, unused, unused)
        .unwrap()
        .join()
        .unwrap();
    let after = monotonic_us();
    let requested_ns = report["start_requested_host_monotonic_ns"]
        .as_u64()
        .unwrap();
    assert!((before..=after).contains(&(requested_ns / 1000)));
    assert_eq!(requested_ns % 1000, 0);
    assert_eq!(report["valid"], true);
    assert_eq!(report["status"], "fake_room_delivery");
    let size = room.recv(&mut buffer).unwrap();
    assert_eq!(&buffer[..size], &expected);
}

#[test]
fn an_unrepresentable_player_timestamp_is_rejected_before_delivery() {
    let fixture = Fixture::new();
    let room = UnixDatagram::bind(fixture.root.join("room.sock")).unwrap();
    room.set_nonblocking(true).unwrap();
    let timing = StimulusCatalog::load()
        .unwrap()
        .estimate("fast-followup")
        .unwrap();
    let unused = Path::new("/nonexistent/unused");
    let mut player = fixture
        .player()
        .with_clock(|| MonoTime::from_micros(u64::MAX / 1000 + 1));
    let error = player.start(unused, &timing, unused, unused).unwrap_err();
    assert!(error.to_string().contains("overflows nanoseconds"));
    assert_eq!(
        room.recv(&mut [0; 1]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
