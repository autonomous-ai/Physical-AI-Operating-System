//! Explicit-clock prepared-player checks use its real ready/grant channels.
//! Read-order probes below are instrumentation, not advancing scheduler clocks.
use super::*;
use std::{
    os::unix::fs::DirBuilderExt,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const GRANT_WAIT_MS: u32 = 10_000;

struct Fixture {
    root: PathBuf,
    source: PathBuf,
    cancel: PathBuf,
    timing: SceneTiming,
    room: UnixDatagram,
}

impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(format!(
            "/tmp/lve-prepared-clock-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let source = root.join("source.wav");
        // This fake hashes the source; it does not decode or play audio.
        fs::write(&source, b"owned prepared-player timing fixture").unwrap();
        let room = UnixDatagram::bind(root.join("room")).unwrap();
        room.set_nonblocking(true).unwrap();
        Self {
            source,
            cancel: root.join("cancel"),
            timing: StimulusCatalog::load()
                .unwrap()
                .estimate("fast-followup")
                .unwrap(),
            room,
            root,
        }
    }

    fn player(&self) -> FakePreparedRoomPlayer {
        FakePreparedRoomPlayer {
            room: self.root.join("room"),
            faults: Vec::new(),
            prepared: 0,
        }
    }

    fn prepare(&self, player: &mut impl Player) -> Result<Option<Preparing>> {
        player.prepare(
            &self.source,
            &self.timing,
            &self.root,
            &self.cancel,
            GRANT_WAIT_MS,
        )
    }

    fn ready(&self, player: &mut impl Player) -> PreparedStart {
        let Preparing {
            ready,
            grant,
            handle,
        } = self.prepare(player).unwrap().unwrap();
        PreparedStart {
            ready: ready.recv_timeout(PREPARE_READY_BOUND).unwrap(),
            grant,
            handle,
        }
    }

    fn assert_delivery(&self) {
        let mut buffer = [0; ROOM_MAX_BYTES];
        let size = self.room.recv(&mut buffer).unwrap();
        assert_eq!(
            &buffer[..size],
            serde_json::to_vec(&self.timing).unwrap().as_slice()
        );
    }

    fn assert_no_delivery(&self) {
        // Call only after the producer thread has joined (or was never spawned).
        assert_eq!(
            self.room.recv(&mut [0; ROOM_MAX_BYTES]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn grant(ready: &Value, expires_ns: u64) -> Value {
    json!({"schema":1,"kind":"start","attempt_id":ready["attempt_id"],
        "expires_monotonic_ns":expires_ns})
}

fn finish(prepared: PreparedStart, input: GrantInput) -> Value {
    prepared.grant.send(input).unwrap();
    prepared.handle.join().unwrap()
}

/// Scripted observations distinguish the four call sites without host sleeps.
/// Only this read-order probe consumes samples; a fixture clock must be pure.
fn read_order_probe(
    samples: Vec<u64>,
) -> (
    impl Fn() -> MonoTime + Clone + Send + Sync + 'static,
    Arc<AtomicUsize>,
) {
    let samples = Arc::new(samples);
    let reads = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&reads);
    (
        move || MonoTime::from_micros(samples[reads.fetch_add(1, Ordering::SeqCst)]),
        observed,
    )
}

#[test]
fn a_shared_pure_clock_keeps_original_grants_and_real_datagram_delivery() {
    let fixture = Fixture::new();
    let at = Arc::new(AtomicU64::new(100_000));
    let reads = Arc::new(AtomicUsize::new(0));
    let clock_at = Arc::clone(&at);
    let clock_reads = Arc::clone(&reads);
    let mut player = fixture.player().with_clock(move || {
        clock_reads.fetch_add(1, Ordering::SeqCst);
        MonoTime::from_micros(clock_at.load(Ordering::SeqCst))
    });
    assert_eq!(player.describe(), fixture.player().describe());

    for number in 1..=2 {
        let before = at.load(Ordering::SeqCst);
        let prepared = fixture.ready(&mut player);
        assert_eq!(at.load(Ordering::SeqCst), before);
        assert_eq!(
            prepared.ready,
            json!({"schema":1,"kind":"prepared_ready",
                "attempt_id":format!("fake-{number}-{}", before * 1000),
                "ready_monotonic_ns":before * 1000,"clock":"CLOCK_MONOTONIC",
                "source_sha256":lamp_sound_fixtures::hash(&fs::read(&fixture.source).unwrap()),
                "max_wait_ms":GRANT_WAIT_MS})
        );
        // Ready acknowledges preparation before the test owner advances time
        // and sends the original grant; no wall-clock delay is used.
        at.store(200_000, Ordering::SeqCst);
        let expires = 200_000_000 + lamp_observer::playback::MAX_GRANT_HORIZON_NS;
        let input = GrantInput::Line(grant(&prepared.ready, expires).to_string());
        let report = finish(prepared, input);
        assert_eq!(report["valid"], true);
        assert_eq!(report["status"], "fake_room_delivery");
        assert_eq!(report["counters"]["source_started_host_ns"], 200_000_000);
        assert_eq!(
            report["prepared_start"]["grant"],
            json!({"decision":"start","expires_monotonic_ns":expires,
                "received_monotonic_ns":200_000_000})
        );
        assert_eq!(at.load(Ordering::SeqCst), 200_000);
        fixture.assert_delivery();
        fixture.assert_no_delivery();
    }
    assert_eq!(reads.load(Ordering::SeqCst), 8);
}

#[test]
fn the_fresh_first_copy_sample_decides_the_original_expiry_boundary() {
    for copy_us in [349, 350] {
        let fixture = Fixture::new();
        let (clock, reads) = read_order_probe(vec![100, 200, 300, copy_us]);
        let mut player = fixture.player().with_clock(clock);
        let prepared = fixture.ready(&mut player);
        assert_eq!(prepared.ready["attempt_id"], "fake-1-100000");
        assert_eq!(prepared.ready["ready_monotonic_ns"], 200_000);
        let input = GrantInput::Line(grant(&prepared.ready, 350_000).to_string());
        let report = finish(prepared, input);
        assert_eq!(reads.load(Ordering::SeqCst), 4);
        assert_eq!(
            report["prepared_start"]["grant"]["received_monotonic_ns"],
            300_000
        );
        if copy_us == 349 {
            assert_eq!(report["valid"], true);
            assert_eq!(report["status"], "fake_room_delivery");
            assert_eq!(report["counters"]["source_started_host_ns"], 349_000);
            fixture.assert_delivery();
        } else {
            assert_eq!(report["valid"], false);
            assert_eq!(report["status"], "start_expired");
            assert_eq!(report["first_copy_attempt_host_ns"], 350_000);
            assert_eq!(report["counters"]["source_started_host_ns"], 0);
        }
        fixture.assert_no_delivery();
    }
}

#[test]
fn original_grant_validation_still_refuses_invalid_identity_shape_and_horizon() {
    let fixture = Fixture::new();
    let mut player = fixture.player().with_clock(|| MonoTime::from_micros(100));
    for case in [
        "schema", "kind", "attempt", "missing", "expired", "equal", "horizon",
    ] {
        let prepared = fixture.ready(&mut player);
        let mut line = grant(&prepared.ready, 200_000);
        match case {
            "schema" => line["schema"] = json!(2),
            "kind" => line["kind"] = json!("stop"),
            "attempt" => line["attempt_id"] = json!("another-attempt"),
            "missing" => {
                line.as_object_mut().unwrap().remove("expires_monotonic_ns");
            }
            "expired" => line["expires_monotonic_ns"] = json!(99_999),
            "equal" => line["expires_monotonic_ns"] = json!(100_000),
            "horizon" => {
                line["expires_monotonic_ns"] =
                    json!(100_000 + lamp_observer::playback::MAX_GRANT_HORIZON_NS + 1);
            }
            _ => unreachable!(),
        }
        let report = finish(prepared, GrantInput::Line(line.to_string()));
        assert_eq!(report["valid"], false, "{case}");
        assert_eq!(report["status"], "grant_refused", "{case}");
        assert_eq!(report["counters"]["source_started_host_ns"], 0, "{case}");
        assert!(report["prepared_start"]["grant"].is_null(), "{case}");
        fixture.assert_no_delivery();
    }
}

#[test]
fn closed_and_malformed_inputs_still_withhold_delivery() {
    let fixture = Fixture::new();
    let mut player = fixture.player().with_clock(|| MonoTime::from_micros(100));
    for (input, status) in [
        (GrantInput::Closed, "stopped_before_grant"),
        (GrantInput::Malformed, "grant_refused"),
        (GrantInput::Line("not JSON".into()), "grant_refused"),
    ] {
        let prepared = fixture.ready(&mut player);
        let report = finish(prepared, input);
        assert_eq!(report["valid"], false);
        assert_eq!(report["status"], status);
        assert_eq!(report["counters"]["source_started_host_ns"], 0);
        fixture.assert_no_delivery();
    }
}

#[test]
fn cancellation_after_ready_still_withholds_the_first_copy() {
    let fixture = Fixture::new();
    let mut player = fixture.player().with_clock(|| MonoTime::from_micros(100));
    let prepared = fixture.ready(&mut player);
    fs::write(&fixture.cancel, b"cancel").unwrap();
    let input = GrantInput::Line(grant(&prepared.ready, 200_000).to_string());
    let report = finish(prepared, input);
    assert_eq!(report["valid"], false);
    assert_eq!(report["status"], "cancelled_before_first_copy");
    assert_eq!(report["counters"]["source_started_host_ns"], 0);
    fixture.assert_no_delivery();
}

#[test]
fn unrepresentable_clock_samples_fail_at_each_stage_before_delivery() {
    let overflow = u64::MAX / 1000 + 1;
    for (phase, samples) in [
        ("attempt ID", vec![overflow]),
        ("readiness", vec![10, overflow]),
        ("grant receipt", vec![10, 20, overflow]),
        ("first copy", vec![10, 20, 30, overflow]),
    ] {
        let fixture = Fixture::new();
        let expected_reads = samples.len();
        let (clock, reads) = read_order_probe(samples);
        let mut player = fixture.player().with_clock(clock);
        let preparing = fixture.prepare(&mut player);
        if phase == "attempt ID" {
            let error = preparing.err().expect("overflow must precede the worker");
            assert!(error.to_string().contains(phase));
            assert!(error.to_string().contains("overflows nanoseconds"));
        } else {
            let Preparing {
                ready,
                grant: sender,
                handle,
            } = preparing.unwrap().unwrap();
            if phase != "readiness" {
                let receipt = ready.recv_timeout(PREPARE_READY_BOUND).unwrap();
                sender
                    .send(GrantInput::Line(grant(&receipt, 40_000).to_string()))
                    .unwrap();
            }
            let report = handle.join().unwrap();
            assert_eq!(report["valid"], false);
            assert_eq!(report["status"], "failed");
            assert_eq!(report["counters"]["source_started_host_ns"], 0);
            let error = report["error"].as_str().unwrap();
            assert!(error.contains(phase), "{error}");
            assert!(error.contains("overflows nanoseconds"), "{error}");
            if phase == "readiness" {
                assert!(matches!(ready.try_recv(), Err(TryRecvError::Disconnected)));
                assert!(report["prepared_start"].is_null());
            } else {
                assert_eq!(
                    report["prepared_start"]["ready"]["ready_monotonic_ns"],
                    20_000
                );
                assert_eq!(
                    report["prepared_start"]["grant"].is_object(),
                    phase == "first copy"
                );
            }
        }
        assert_eq!(reads.load(Ordering::SeqCst), expected_reads);
        fixture.assert_no_delivery();
    }
}
