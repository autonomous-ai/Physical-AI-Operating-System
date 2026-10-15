//! Recorder progress never substitutes for the backend's actual child status.
use super::*;
use std::{
    cell::{Cell, RefCell},
    os::unix::fs::PermissionsExt,
    rc::Rc,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const AT_US: u64 = 1_000_000;
const GUARD: Duration = Duration::from_secs(5);

struct Fixture {
    root: PathBuf,
    assets: AssetIndex,
}

impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(format!(
            "/tmp/lve-recorder-backend-{}-{}",
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
            lamp_command: vec!["/bin/sh".into(), "-c".into(), "exec cat >/dev/null".into()],
            lamp_work_root: self.root.display().to_string(),
            mode: LampMode::Serve {
                provider_config: "/private/unused.json".into(),
            },
            noise_suppression: "off".into(),
            diagnostics: false,
            room_recorder: Some(vec!["/bin/cat".into()]),
            room_independent: false,
            allowance_seconds: 5,
            ring_channel_ceiling: None,
            stimulus_source: StimulusSource::LoudspeakerSynthetic,
            runtime_flags: Default::default(),
        }
    }

    fn backend(&self, driver: ProbeDriver, clock: Rc<Cell<u64>>) -> PhysicalBackend<'_> {
        PhysicalBackend::new_with_clock(
            self.config(),
            &self.assets,
            Box::new(FakeRoomPlayer {
                room: self.root.join("room.sock"),
            }),
            move || MonoTime::from_micros(clock.get()),
        )
        .with_fixture_driver(driver)
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Default)]
struct State {
    pending: Cell<bool>,
    observed: Cell<usize>,
    starts: Cell<usize>,
    waits: Cell<usize>,
    prepare_waits: Cell<usize>,
    finishes: Cell<usize>,
    dropped: Cell<usize>,
    start_deadline: RefCell<Option<Instant>>,
}

struct ProbeDriver {
    state: Rc<State>,
    supports: bool,
    fail_start: bool,
    fail_wait: bool,
    fail_grant: bool,
    finish_release: Option<ChildStdin>,
}

impl ProbeDriver {
    fn new() -> (Self, Rc<State>) {
        let state = Rc::new(State::default());
        (
            Self {
                state: Rc::clone(&state),
                supports: true,
                fail_start: false,
                fail_wait: false,
                fail_grant: false,
                finish_release: None,
            },
            state,
        )
    }
}

impl Drop for ProbeDriver {
    fn drop(&mut self) {
        self.state.dropped.set(self.state.dropped.get() + 1);
    }
}

impl FixtureDriver for ProbeDriver {
    fn supports_serve(&self) -> bool {
        true
    }
    fn supports_recorder(&self) -> bool {
        self.supports
    }
    fn start_recorder(&mut self, deadline: Instant) -> Result<()> {
        self.state.starts.set(self.state.starts.get() + 1);
        *self.state.start_deadline.borrow_mut() = Some(deadline);
        if self.fail_start {
            Err(invalid("recorder start control failed"))
        } else {
            Ok(())
        }
    }
    fn recorder_exit_pending(&self) -> bool {
        self.state.pending.get()
    }
    fn grant_sent(&mut self, _: &str) -> Result<()> {
        if self.fail_grant {
            Err(invalid("prepared grant control failed"))
        } else {
            Ok(())
        }
    }
    fn recorder_exit_observed(&mut self) -> Result<()> {
        assert!(self.state.pending.replace(false));
        self.state.observed.set(self.state.observed.get() + 1);
        Ok(())
    }
    fn finish_recorder(&mut self, _: Instant) -> Result<()> {
        self.state.finishes.set(self.state.finishes.get() + 1);
        if let Some(mut input) = self.finish_release.take() {
            input.write_all(b"0\n")?;
            input.flush()?;
            self.state.pending.set(true);
        }
        Ok(())
    }
    fn wait(&mut self, _: MonoTime, _: u64, _: &AtomicU64, _: bool) -> Result<()> {
        self.state.waits.set(self.state.waits.get() + 1);
        if self.fail_wait {
            Err(invalid("recorder round control failed"))
        } else {
            Ok(())
        }
    }
    fn prepare_wait(
        &mut self,
        until: MonoTime,
        sent: u64,
        enqueued: &AtomicU64,
        drained: bool,
        _: Instant,
    ) -> Result<()> {
        self.state
            .prepare_waits
            .set(self.state.prepare_waits.get() + 1);
        self.wait(until, sent, enqueued, drained)
    }
}

fn held_recorder() -> (Child, ChildStdin) {
    // A real pipe gates the process exit; no guessed delay or sleep is used.
    let mut child = Command::new("/bin/sh")
        .args(["-c", "IFS= read -r code; exit \"$code\""])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let input = child.stdin.take().unwrap();
    assert!(child.try_wait().unwrap().is_none());
    (child, input)
}

fn release(mut input: ChildStdin, code: i32) {
    writeln!(input, "{code}").unwrap();
    input.flush().unwrap();
}

fn live_session() -> (
    Session,
    mpsc::Sender<std::result::Result<SessionLine, String>>,
) {
    let mut child = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let (sender, lines) = mpsc::channel();
    (
        Session {
            stdin: child.stdin.take(),
            child,
            lines,
            next_ping_us: u64::MAX,
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
        },
        sender,
    )
}

#[test]
fn recorder_configuration_requires_its_separate_explicit_capability() {
    let fixture = Fixture::new();
    let (mut driver, state) = ProbeDriver::new();
    driver.supports = false;
    let result = PhysicalBackend::new(
        fixture.config(),
        &fixture.assets,
        Box::new(FakeRoomPlayer {
            room: fixture.root.join("room.sock"),
        }),
    )
    .with_fixture_driver(driver);
    assert!(result.is_err());
    assert_eq!(state.starts.get(), 0);
    assert_eq!(state.dropped.get(), 1);
}

#[test]
fn terminal_ack_alone_hits_the_host_bound_closes_peers_and_reaps_the_real_child() {
    let fixture = Fixture::new();
    let (driver, state) = ProbeDriver::new();
    let clock = Rc::new(Cell::new(AT_US));
    let mut backend = fixture.backend(driver, Rc::clone(&clock));
    let (child, input) = held_recorder();
    backend.room = Some((child, fixture.root.join("recording"), AT_US));
    state.pending.set(true);
    let first = backend
        .settle_fixture_recorder_exit(Instant::now())
        .unwrap_err()
        .to_string();
    assert!(first.contains("did not actually exit within the host bound"));
    assert_eq!(state.observed.get(), 0);
    assert_eq!(state.dropped.get(), 1);
    assert_eq!(clock.get(), AT_US);
    assert!(
        backend
            .room
            .as_mut()
            .unwrap()
            .0
            .try_wait()
            .unwrap()
            .is_some()
    );
    assert!(backend.fixture.is_some());
    assert!(backend.room_cover.lost.is_none());
    assert!(backend.pending.is_empty());
    let second = backend
        .settle_fixture_recorder_exit(Instant::now() + GUARD)
        .unwrap_err()
        .to_string();
    assert_eq!(first, second);
    assert_eq!(state.dropped.get(), 1);
    assert!(backend.pending.is_empty());
    drop(input);
}

#[test]
fn actual_zero_exit_is_retained_and_only_existing_coverage_logic_marks_it_early() {
    let fixture = Fixture::new();
    let (driver, state) = ProbeDriver::new();
    let clock = Rc::new(Cell::new(AT_US));
    let mut backend = fixture.backend(driver, Rc::clone(&clock));
    let (child, input) = held_recorder();
    backend.room = Some((child, fixture.root.join("recording"), AT_US));
    state.pending.set(true);
    release(input, 0);
    backend
        .settle_fixture_recorder_exit(Instant::now() + GUARD)
        .unwrap();
    assert_eq!(state.observed.get(), 1);
    assert_eq!(
        backend
            .room
            .as_mut()
            .unwrap()
            .0
            .try_wait()
            .unwrap()
            .unwrap()
            .code(),
        Some(0)
    );
    assert!(backend.room_cover.lost.is_none());
    assert!(!backend.evidence_lost);
    assert!(backend.pending.is_empty());
    assert_eq!(clock.get(), AT_US);
    let event = backend.poll_evidence().unwrap();
    assert!(
        matches!(event.kind, EventKind::EvidenceLost { source, .. } if source == "room_recorder")
    );
    assert_eq!(
        backend.room_cover.lost.as_ref().unwrap().1,
        RECORDER_EXITED_EARLY
    );
    assert!(backend.poll_evidence().is_none());
    let evidence = load_room_evidence(
        &fixture.root.join("recording/metadata.json"),
        false,
        Some(true),
    );
    assert_eq!(evidence["valid"], false);
    assert_eq!(evidence["status"], "unavailable_final_metadata");
}

#[test]
fn both_normal_and_preparation_pumps_wait_for_actual_recorder_exit() {
    for preparing in [false, true] {
        let fixture = Fixture::new();
        let (driver, state) = ProbeDriver::new();
        let clock = Rc::new(Cell::new(AT_US));
        let mut backend = fixture.backend(driver, Rc::clone(&clock));
        let (child, input) = held_recorder();
        backend.room = Some((child, fixture.root.join("recording"), AT_US));
        state.pending.set(true);
        release(input, 7);
        if preparing {
            backend
                .pump_preparation_fixture(AT_US, Instant::now() + GUARD)
                .unwrap();
        } else {
            backend.pump_fixture(AT_US, true).unwrap();
        }
        assert_eq!(state.observed.get(), 1);
        assert_eq!(state.waits.get(), 1);
        assert_eq!(state.prepare_waits.get(), usize::from(preparing));
        assert_eq!(
            backend
                .room
                .as_mut()
                .unwrap()
                .0
                .try_wait()
                .unwrap()
                .unwrap()
                .code(),
            Some(7)
        );
        assert_eq!(clock.get(), AT_US);
        assert!(backend.room_cover.lost.is_none());
    }
}

#[test]
fn recorder_settling_does_not_reorder_player_failure_evidence() {
    let fixture = Fixture::new();
    let (driver, state) = ProbeDriver::new();
    let mut backend = fixture.backend(driver, Rc::new(Cell::new(AT_US)));
    let (child, input) = held_recorder();
    backend.room = Some((child, fixture.root.join("recording"), AT_US));
    state.pending.set(true);
    release(input, 0);
    backend
        .settle_fixture_recorder_exit(Instant::now() + GUARD)
        .unwrap();
    let handle = thread::spawn(|| json!({"valid":false,"status":"failed","error":"player fault"}));
    wait_fixture_thread(&handle, Instant::now() + GUARD).unwrap();
    backend.playbacks.push(Playback {
        step: "first".into(),
        requested_us: AT_US,
        cancel_file: fixture.root.join("cancel"),
        handle: Some(handle),
        report: None,
        prepared: true,
        latest_start_us: None,
    });
    let event = backend.poll_evidence().unwrap();
    assert!(
        matches!(event.kind, EventKind::EvidenceLost { source, .. } if source == "player:first")
    );
    assert!(backend.room_cover.lost.is_none());
}

#[test]
fn terminal_recorder_drain_keeps_prior_declared_coverage_reason_and_real_status() {
    let fixture = Fixture::new();
    let (mut driver, state) = ProbeDriver::new();
    let (child, input) = held_recorder();
    driver.finish_release = Some(input);
    let clock = Rc::new(Cell::new(AT_US));
    let mut backend = fixture.backend(driver, Rc::clone(&clock));
    backend.room = Some((child, fixture.root.join("recording"), AT_US));
    backend.room_cover.end_us = Some(AT_US + 20_000_000);
    let original = (
        AT_US,
        PROMPT_NOT_COVERED,
        "original declared capture end refusal".to_owned(),
    );
    backend.room_cover.lost = Some(original.clone());
    backend
        .finish_fixture_recorder(Instant::now() + GUARD)
        .unwrap();
    assert_eq!(state.finishes.get(), 1);
    assert_eq!(state.observed.get(), 1);
    assert_eq!(backend.room_cover.lost, Some(original));
    assert_eq!(
        backend
            .room
            .as_mut()
            .unwrap()
            .0
            .try_wait()
            .unwrap()
            .unwrap()
            .code(),
        Some(0)
    );
    assert_eq!(clock.get(), AT_US);
    assert!(backend.pending.is_empty());
}

#[test]
fn a_finish_hook_without_actual_exit_cannot_certify_the_recorder() {
    let fixture = Fixture::new();
    let (driver, state) = ProbeDriver::new();
    let mut backend = fixture.backend(driver, Rc::new(Cell::new(AT_US)));
    let (child, input) = held_recorder();
    backend.room = Some((child, fixture.root.join("recording"), AT_US));
    let error = backend
        .finish_fixture_recorder(Instant::now() + GUARD)
        .unwrap_err()
        .to_string();
    assert!(error.contains("returned before actual exit"));
    assert_eq!(state.observed.get(), 0);
    assert_eq!(state.dropped.get(), 1);
    assert!(
        backend
            .room
            .as_mut()
            .unwrap()
            .0
            .try_wait()
            .unwrap()
            .is_some()
    );
    assert!(backend.room_cover.lost.is_none());
    drop(input);
}

#[test]
fn startup_reads_the_real_receipt_and_shares_the_original_fifteen_second_bound() {
    let fixture = Fixture::new();
    let (driver, state) = ProbeDriver::new();
    let clock = Rc::new(Cell::new(AT_US));
    let mut backend = fixture.backend(driver, Rc::clone(&clock));
    backend.config.room_recorder = Some(vec!["/bin/sh".into(), "-c".into(),
        r#"mkdir "$1" && printf '%s' '{"status":"capture_ready","planned_capture_end_host_monotonic_ns":21000000000}' > "$1/ready.json""#.into(),
        "recorder".into(), "{out}".into()]);
    state.pending.set(true);
    let catalog = StimulusCatalog::load().unwrap();
    let plan = crate::plan::LoadedPlan::parse(crate::plan::SERVE_PLAN, &catalog).unwrap();
    let context = AttemptContext {
        attempt_id: "ready".into(),
        run_id: "test".into(),
        scenario: &plan.plan.scenarios[0],
        repetition: 0,
        order: 0,
        seed: 1,
        run_directory: fixture.root.clone(),
    };
    let before = Instant::now();
    assert!(matches!(backend.begin(&context).unwrap(), Begin::Ready));
    let after = Instant::now();
    let deadline = state.start_deadline.borrow().unwrap();
    assert!(deadline >= before + Duration::from_secs(15));
    assert!(deadline <= after + Duration::from_secs(15));
    assert_eq!(state.starts.get(), 1);
    assert_eq!(state.observed.get(), 1);
    assert_eq!(backend.room_cover.ready["status"], "capture_ready");
    assert_eq!(backend.room_cover.end_us, Some(21_000_000));
    assert!(fixture.root.join("ready/room/ready.json").is_file());
    assert!(!fixture.root.join("ready/room/metadata.json").exists());
    assert_eq!(
        backend
            .room
            .as_mut()
            .unwrap()
            .0
            .try_wait()
            .unwrap()
            .unwrap()
            .code(),
        Some(0)
    );
    assert_eq!(clock.get(), AT_US);
}

#[test]
fn startup_control_failure_closes_its_driver_before_any_session_is_started() {
    let fixture = Fixture::new();
    let (mut driver, state) = ProbeDriver::new();
    driver.fail_start = true;
    let mut backend = fixture.backend(driver, Rc::new(Cell::new(AT_US)));
    let catalog = StimulusCatalog::load().unwrap();
    let plan = crate::plan::LoadedPlan::parse(crate::plan::SERVE_PLAN, &catalog).unwrap();
    let context = AttemptContext {
        attempt_id: "start-failed".into(),
        run_id: "test".into(),
        scenario: &plan.plan.scenarios[0],
        repetition: 0,
        order: 0,
        seed: 1,
        run_directory: fixture.root.clone(),
    };
    assert!(
        matches!(backend.begin(&context).unwrap(), Begin::Withheld(reason)
        if reason.contains("recorder start control failed"))
    );
    assert_eq!(state.starts.get(), 1);
    assert_eq!(state.dropped.get(), 1);
    assert!(backend.session.is_none());
    assert!(backend.room.is_none());
    assert!(backend.fixture.as_ref().unwrap().recorder_failure.is_some());
}

#[test]
fn a_failed_fixture_stays_nonblocking_and_finish_reuses_the_latched_error() {
    let fixture = Fixture::new();
    let (mut driver, state) = ProbeDriver::new();
    driver.fail_wait = true;
    let mut backend = fixture.backend(driver, Rc::new(Cell::new(AT_US)));
    let (child, input) = held_recorder();
    backend.room = Some((child, fixture.root.join("recording"), AT_US));
    let (session, sender) = live_session();
    backend.session = Some(session);
    let first = backend
        .pump_fixture(AT_US + 2_000, true)
        .unwrap_err()
        .to_string();
    assert_eq!(state.dropped.get(), 1);
    assert_eq!(state.waits.get(), 1);
    assert!(backend.fixture.is_some());
    assert!(backend.pending.is_empty());
    // The actual killed recorder is reported by the unchanged coverage path.
    assert!(backend.next_event(AT_US + 2_000).unwrap().is_some());
    assert_eq!(
        backend.next_event(AT_US + 2_000).unwrap_err().to_string(),
        first
    );
    let catalog = StimulusCatalog::load().unwrap();
    let plan = crate::plan::LoadedPlan::parse(crate::plan::SERVE_PLAN, &catalog).unwrap();
    let context = AttemptContext {
        attempt_id: "failed".into(),
        run_id: "test".into(),
        scenario: &plan.plan.scenarios[0],
        repetition: 0,
        order: 0,
        seed: 1,
        run_directory: fixture.root.clone(),
    };
    assert_eq!(backend.finish(&context).err().unwrap().to_string(), first);
    assert_eq!(state.dropped.get(), 1);
    assert_eq!(state.waits.get(), 1);
    assert!(backend.pending.is_empty());
    assert!(
        backend
            .room
            .as_mut()
            .unwrap()
            .0
            .try_wait()
            .unwrap()
            .is_some()
    );
    drop(input);
    drop(sender);
}

#[test]
fn grant_control_failure_closes_once_and_repeated_notifications_queue_one_error() {
    let fixture = Fixture::new();
    let (mut driver, state) = ProbeDriver::new();
    driver.fail_grant = true;
    let clock = Rc::new(Cell::new(AT_US));
    let mut backend = fixture.backend(driver, Rc::clone(&clock));
    let (child, input) = held_recorder();
    backend.room = Some((child, fixture.root.join("recording"), AT_US));
    let ready = json!({"attempt_id": "prepared"});
    let first = backend
        .notify_prepared_input(&ready)
        .unwrap_err()
        .to_string();
    backend.fixture_player_error(first.clone());
    let second = backend
        .notify_prepared_input(&ready)
        .unwrap_err()
        .to_string();
    backend.fixture_player_error(second.clone());
    assert_eq!(first, "prepared grant control failed");
    assert_eq!(first, second);
    assert_eq!(state.dropped.get(), 1);
    assert_eq!(backend.pending.len(), 1);
    assert!(backend.evidence_lost);
    assert!(backend.room_cover.lost.is_none());
    assert_eq!(clock.get(), AT_US);
    assert!(
        backend
            .room
            .as_mut()
            .unwrap()
            .0
            .try_wait()
            .unwrap()
            .is_some()
    );
    drop(input);
}
