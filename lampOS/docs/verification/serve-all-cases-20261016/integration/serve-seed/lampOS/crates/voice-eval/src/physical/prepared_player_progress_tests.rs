//! Real channel/datagram boundaries for explicitly coordinated prepared fakes.
use super::*;
use crate::physical::fixture_players::PreparedFixtureController;
use std::{os::unix::fs::DirBuilderExt, sync::atomic::AtomicUsize};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const START_US: u64 = 1_000_000;
const GRANT_WAIT_MS: u32 = 10_000;

struct Fixture {
    root: PathBuf,
    source: PathBuf,
    cancel: PathBuf,
    timing: SceneTiming,
    room: Option<UnixDatagram>,
}

impl Fixture {
    fn new(room_ready: bool) -> Self {
        let root = PathBuf::from(format!(
            "/tmp/lve-prepared-progress-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let source = root.join("source.wav");
        fs::write(&source, b"owned prepared fixture; no audio is decoded").unwrap();
        let mut fixture = Self {
            source,
            cancel: root.join("cancel"),
            timing: StimulusCatalog::load()
                .unwrap()
                .estimate("fast-followup")
                .unwrap(),
            room: None,
            root,
        };
        if room_ready {
            fixture.bind_room();
        }
        fixture
    }

    fn bind_room(&mut self) {
        let room = UnixDatagram::bind(self.root.join("room")).unwrap();
        room.set_nonblocking(true).unwrap();
        self.room = Some(room);
    }

    fn controller(&self) -> (PreparedFixtureController, Arc<AtomicU64>) {
        let at = Arc::new(AtomicU64::new(START_US));
        let now = Arc::clone(&at);
        (
            PreparedFixtureController::new(move || {
                MonoTime::from_micros(now.load(Ordering::SeqCst))
            }),
            at,
        )
    }

    fn prepare(&self, player: &mut impl Player, max_wait_ms: u32) -> Preparing {
        player
            .prepare(
                &self.source,
                &self.timing,
                &self.root,
                &self.cancel,
                max_wait_ms,
            )
            .unwrap()
            .unwrap()
    }

    fn ready(
        &self,
        controller: &PreparedFixtureController,
        player: &mut impl Player,
    ) -> PreparedStart {
        let preparing = self.prepare(player, GRANT_WAIT_MS);
        assert!(matches!(
            preparing.ready.try_recv(),
            Err(TryRecvError::Empty)
        ));
        controller.pump().unwrap();
        into_ready(preparing)
    }

    fn assert_delivery(&self) {
        let mut bytes = [0; ROOM_MAX_BYTES];
        let length = self.room.as_ref().unwrap().recv(&mut bytes).unwrap();
        assert_eq!(
            &bytes[..length],
            serde_json::to_vec(&self.timing).unwrap().as_slice()
        );
    }

    fn assert_no_delivery(&self) {
        assert_eq!(
            self.room
                .as_ref()
                .unwrap()
                .recv(&mut [0; ROOM_MAX_BYTES])
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn into_ready(preparing: Preparing) -> PreparedStart {
    PreparedStart {
        ready: preparing.ready.try_recv().unwrap(),
        grant: preparing.grant,
        handle: preparing.handle,
    }
}

fn original_grant(ready: &Value, at_us: u64) -> GrantInput {
    GrantInput::Line(
        json!({"schema":1,"kind":"start","attempt_id":ready["attempt_id"],
        "expires_monotonic_ns":at_us * 1000 + 100_000_000})
        .to_string(),
    )
}

fn join_actual(handle: JoinHandle<Value>) -> Value {
    wait_fixture_thread(&handle, Instant::now() + PREPARE_READY_BOUND).unwrap();
    handle.join().unwrap()
}

#[test]
fn real_ready_grant_consumption_and_room_delivery_finish_at_one_parent_instant() {
    let fixture = Fixture::new(true);
    let (controller, at) = fixture.controller();
    let mut player = controller.player(fixture.root.join("room"), Vec::new());
    let prepared = fixture.ready(&controller, &mut player);
    let attempt = prepared.ready["attempt_id"].as_str().unwrap();
    assert_eq!(prepared.ready["ready_monotonic_ns"], START_US * 1000);
    assert_eq!(
        prepared.ready["source_sha256"],
        lamp_sound_fixtures::hash(&fs::read(&fixture.source).unwrap())
    );
    assert_eq!(prepared.ready["max_wait_ms"], GRANT_WAIT_MS);
    assert_eq!(controller.successful_sends(), 0);
    assert!(!controller.report_ready(attempt));
    // A notification cannot substitute for consuming a real input.
    assert!(controller.grant_sent(attempt).is_err());
    prepared
        .grant
        .send(original_grant(&prepared.ready, START_US))
        .unwrap();
    assert_eq!(controller.successful_sends(), 0);
    fixture.assert_no_delivery();
    controller.grant_sent(attempt).unwrap();
    assert!(controller.report_ready(attempt));
    assert!(controller.grant_sent(attempt).is_err());
    let report = join_actual(prepared.handle);
    assert_eq!(report["valid"], true);
    assert_eq!(report["status"], "fake_room_delivery");
    assert_eq!(
        report["prepared_start"]["grant"]["received_monotonic_ns"],
        START_US * 1000
    );
    assert_eq!(
        report["counters"]["source_started_host_ns"],
        START_US * 1000
    );
    assert_eq!(controller.successful_sends(), 1);
    assert_eq!(at.load(Ordering::SeqCst), START_US);
    fixture.assert_delivery();
    fixture.assert_no_delivery();
}

#[test]
fn slow_prepare_starts_its_full_delay_only_when_the_room_really_exists() {
    let mut fixture = Fixture::new(false);
    let (controller, at) = fixture.controller();
    let mut player = controller.player(
        fixture.root.join("room"),
        vec![(1, FakePlayerFault::SlowPrepare { delay_ms: 2300 })],
    );
    let preparing = fixture.prepare(&mut player, GRANT_WAIT_MS);
    controller.pump().unwrap();
    assert!(matches!(
        preparing.ready.try_recv(),
        Err(TryRecvError::Empty)
    ));
    at.store(START_US + 3_000_000, Ordering::SeqCst);
    controller.pump().unwrap();
    assert!(matches!(
        preparing.ready.try_recv(),
        Err(TryRecvError::Empty)
    ));
    fixture.bind_room();
    controller.pump().unwrap();
    at.store(START_US + 5_299_999, Ordering::SeqCst);
    controller.pump().unwrap();
    assert!(matches!(
        preparing.ready.try_recv(),
        Err(TryRecvError::Empty)
    ));
    at.store(START_US + 5_300_000, Ordering::SeqCst);
    controller.pump().unwrap();
    let prepared = into_ready(preparing);
    assert_eq!(
        prepared.ready["ready_monotonic_ns"],
        (START_US + 5_300_000) * 1000
    );
    prepared.grant.send(GrantInput::Closed).unwrap();
    controller
        .grant_sent(prepared.ready["attempt_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        join_actual(prepared.handle)["status"],
        "stopped_before_grant"
    );
    assert_eq!(controller.successful_sends(), 0);
    fixture.assert_no_delivery();
}

#[test]
fn late_copy_keeps_the_original_delay_expiry_and_cancel_check_order() {
    let fixture = Fixture::new(true);
    let (controller, at) = fixture.controller();
    let mut player = controller.player(
        fixture.root.join("room"),
        vec![(1, FakePlayerFault::LateFirstCopy { delay_ms: 150 })],
    );
    let prepared = fixture.ready(&controller, &mut player);
    let attempt = prepared.ready["attempt_id"].as_str().unwrap();
    prepared
        .grant
        .send(original_grant(&prepared.ready, START_US))
        .unwrap();
    controller.grant_sent(attempt).unwrap();
    assert!(!controller.report_ready(attempt));
    fs::write(&fixture.cancel, b"cancel\n").unwrap();
    at.store(START_US + 149_999, Ordering::SeqCst);
    controller.pump().unwrap();
    assert!(!controller.report_ready(attempt));
    fixture.assert_no_delivery();
    at.store(START_US + 150_000, Ordering::SeqCst);
    controller.pump().unwrap();
    assert!(controller.report_ready(attempt));
    let report = join_actual(prepared.handle);
    assert_eq!(report["status"], "start_expired");
    assert_eq!(
        report["first_copy_attempt_host_ns"],
        (START_US + 150_000) * 1000
    );
    assert_eq!(report["counters"]["source_started_host_ns"], 0);
    assert_eq!(controller.successful_sends(), 0);
    fixture.assert_no_delivery();
}

#[test]
fn grant_wait_uses_the_original_logical_duration_and_no_synthetic_input() {
    let fixture = Fixture::new(true);
    let (controller, at) = fixture.controller();
    let mut player = controller.player(fixture.root.join("room"), Vec::new());
    let preparing = fixture.prepare(&mut player, 10);
    controller.pump().unwrap();
    let prepared = into_ready(preparing);
    let attempt = prepared.ready["attempt_id"].as_str().unwrap();
    at.store(START_US + 9_999, Ordering::SeqCst);
    controller.pump().unwrap();
    assert!(!controller.report_ready(attempt));
    at.store(START_US + 10_000, Ordering::SeqCst);
    controller.pump().unwrap();
    assert!(controller.report_ready(attempt));
    assert!(controller.grant_sent(attempt).is_err());
    let report = join_actual(prepared.handle);
    assert_eq!(report["status"], "grant_timeout");
    assert_eq!(report["counters"]["source_started_host_ns"], 0);
    assert_eq!(controller.successful_sends(), 0);
    fixture.assert_no_delivery();
}

#[test]
fn closed_malformed_and_cancelled_inputs_preserve_the_original_decisions() {
    for case in ["closed", "malformed", "cancelled"] {
        let fixture = Fixture::new(true);
        let (controller, _) = fixture.controller();
        let mut player = controller.player(fixture.root.join("room"), Vec::new());
        let prepared = fixture.ready(&controller, &mut player);
        let (input, expected) = match case {
            "closed" => (GrantInput::Closed, "stopped_before_grant"),
            "malformed" => (GrantInput::Malformed, "grant_refused"),
            _ => {
                fs::write(&fixture.cancel, b"cancel\n").unwrap();
                (
                    original_grant(&prepared.ready, START_US),
                    "cancelled_before_first_copy",
                )
            }
        };
        prepared.grant.send(input).unwrap();
        controller
            .grant_sent(prepared.ready["attempt_id"].as_str().unwrap())
            .unwrap();
        let report = join_actual(prepared.handle);
        assert_eq!(report["status"], expected);
        assert_eq!(report["counters"]["source_started_host_ns"], 0);
        assert_eq!(controller.successful_sends(), 0);
        fixture.assert_no_delivery();
    }
}

#[test]
fn actual_failed_room_send_does_not_increment_the_delivery_counter() {
    let fixture = Fixture::new(true);
    let (controller, _) = fixture.controller();
    let mut player = controller.player(fixture.root.join("room"), Vec::new());
    let prepared = fixture.ready(&controller, &mut player);
    fs::remove_file(fixture.root.join("room")).unwrap();
    prepared
        .grant
        .send(original_grant(&prepared.ready, START_US))
        .unwrap();
    controller
        .grant_sent(prepared.ready["attempt_id"].as_str().unwrap())
        .unwrap();
    let report = join_actual(prepared.handle);
    assert_eq!(report["valid"], false);
    assert_eq!(report["status"], "failed");
    // The existing report samples a first-copy attempt even when send fails.
    assert_eq!(
        report["counters"]["source_started_host_ns"],
        START_US * 1000
    );
    assert_eq!(controller.successful_sends(), 0);
    fixture.assert_no_delivery();
}

#[test]
fn prepare_failure_closes_the_real_ready_channel_and_keeps_its_report() {
    let fixture = Fixture::new(true);
    let (controller, _) = fixture.controller();
    let mut player = controller.player(
        fixture.root.join("room"),
        vec![(1, FakePlayerFault::PrepareFails)],
    );
    let preparing = fixture.prepare(&mut player, GRANT_WAIT_MS);
    controller.pump().unwrap();
    let report = join_actual(preparing.handle);
    assert!(matches!(
        preparing.ready.try_recv(),
        Err(TryRecvError::Disconnected)
    ));
    assert_eq!(report["status"], "failed");
    assert_eq!(report["error"], "fake prepare failure");
    assert_eq!(controller.successful_sends(), 0);
    fixture.assert_no_delivery();
}

#[test]
fn closing_control_wakes_resource_grant_and_delayed_copy_workers() {
    for case in ["room", "grant", "copy"] {
        let fixture = Fixture::new(case != "room");
        let (controller, _) = fixture.controller();
        let faults = match case {
            "room" => vec![(1, FakePlayerFault::SlowPrepare { delay_ms: 2300 })],
            "copy" => vec![(1, FakePlayerFault::LateFirstCopy { delay_ms: 150 })],
            _ => Vec::new(),
        };
        let mut player = controller.player(fixture.root.join("room"), faults);
        let preparing = fixture.prepare(&mut player, GRANT_WAIT_MS);
        controller.pump().unwrap();
        if case == "copy" {
            let ready = preparing.ready.try_recv().unwrap();
            preparing
                .grant
                .send(original_grant(&ready, START_US))
                .unwrap();
            controller
                .grant_sent(ready["attempt_id"].as_str().unwrap())
                .unwrap();
        }
        controller.close();
        let report = join_actual(preparing.handle);
        assert_eq!(report["status"], "failed");
        assert!(
            report["error"]
                .as_str()
                .unwrap()
                .contains("control is closed")
        );
        assert_eq!(controller.successful_sends(), 0);
        assert!(controller.pump().is_err());
    }
}

#[test]
fn last_controller_clone_drop_wakes_threads_even_if_the_player_factory_survives() {
    let fixture = Fixture::new(true);
    let (controller, _) = fixture.controller();
    let second = controller.clone();
    let mut player = controller.player(fixture.root.join("room"), Vec::new());
    let prepared = fixture.ready(&controller, &mut player);
    drop(controller);
    second.settle().unwrap();
    assert!(!second.report_ready(prepared.ready["attempt_id"].as_str().unwrap()));
    drop(second);
    let report = join_actual(prepared.handle);
    assert_eq!(report["status"], "failed");
    assert!(
        report["error"]
            .as_str()
            .unwrap()
            .contains("control is closed")
    );
    assert!(
        player
            .prepare(
                &fixture.source,
                &fixture.timing,
                &fixture.root,
                &fixture.cancel,
                GRANT_WAIT_MS
            )
            .is_err()
    );
}

#[test]
fn a_report_boundary_cannot_replace_observing_an_owned_thread_exit() {
    let (reported, ready) = mpsc::sync_channel(1);
    let (release, held) = mpsc::sync_channel(1);
    let handle = thread::spawn(move || {
        reported.send(()).unwrap();
        held.recv().unwrap();
        json!({"status":"finished"})
    });
    ready.recv_timeout(PREPARE_READY_BOUND).unwrap();
    assert!(!handle.is_finished());
    assert!(wait_fixture_thread(&handle, Instant::now()).is_err());
    release.send(()).unwrap();
    assert_eq!(join_actual(handle)["status"], "finished");
}

#[test]
fn a_fixture_ready_pump_receives_the_original_host_deadline() {
    let (published, ready) = mpsc::sync_channel(1);
    let (grant, inputs) = mpsc::sync_channel(9);
    let (done, finished) = mpsc::sync_channel(1);
    let handle = thread::spawn(move || {
        assert!(matches!(inputs.recv(), Ok(GrantInput::Closed)));
        done.send(()).unwrap();
        json!({"status":"stopped_before_grant"})
    });
    let started = Instant::now();
    let result = await_ready_fixture(
        Preparing {
            ready,
            grant,
            handle,
        },
        |deadline| {
            assert!(deadline >= started + PREPARE_READY_BOUND);
            assert!(deadline <= Instant::now() + PREPARE_READY_BOUND);
            Err(invalid("deliberate preparation pump failure"))
        },
    );
    drop(published);
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("deliberate preparation pump failure")
    );
    finished.recv_timeout(PREPARE_READY_BOUND).unwrap();
}
