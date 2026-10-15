//! Timestamp checks use the cycle helpers, never the signal-driven serve loop.
use super::*;
use crate::{
    plan::{LoadedPlan, SERVE_PLAN},
    stimulus::StimulusCatalog,
};
use std::cell::{Cell, RefCell};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    serve: FakeServe,
    metadata: Metadata,
    cues: CueSink,
    _receiver: UnixDatagram,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        // Keep Unix socket paths short, including on macOS.
        let root = PathBuf::from(format!(
            "/tmp/lve-serve-clock-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let cue_path = root.join("cue");
        let receiver = UnixDatagram::bind(&cue_path).unwrap();
        let catalog = StimulusCatalog::load().unwrap();
        let plan = LoadedPlan::parse(SERVE_PLAN, &catalog).unwrap();
        Self {
            serve: FakeServe {
                scenario: plan.scenario("serve-conversation").unwrap().clone(),
                profile: plan.profile("v2-serve-v1-reopen").unwrap().clone(),
                fixed_reply: plan.plan.fixed_reply.clone(),
                seed: 7,
                output: root.clone(),
                cue_path: cue_path.clone(),
                room: None,
                cue_fault: None,
                faults: ServeFaults::default(),
            },
            metadata: Metadata::open(&root).unwrap(),
            cues: CueSink::connect(&cue_path).unwrap(),
            _receiver: receiver,
            root,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn cycle_reads_the_supplied_instant_without_advancing_it() {
    let fixture = Fixture::new();
    let instant = Cell::new(8_000_000);
    let clock = || MonoTime::from_micros(instant.get());
    let first = Cycle::new(1, &fixture.serve, &clock);
    let same = Cycle::new(2, &fixture.serve, &clock);
    assert_eq!(first.base_us, 8_000_000);
    assert_eq!(same.base_us, first.base_us);
    assert_eq!(instant.get(), 8_000_000);

    instant.set(8_000_002);
    let later = Cycle::new(2, &fixture.serve, &clock);
    assert_eq!(later.base_us, 8_000_002);
    assert_eq!(first.boot, BOOT_BYTES);
    assert_eq!(same.boot, CYCLE_TWO_BOOT);
    assert_eq!(later.runtime.now_us(), 0);
}

#[test]
fn initial_cycle_keeps_distinct_metadata_and_runtime_clock_samples() {
    let mut fixture = Fixture::new();
    // Scripted observations expose the read order; they are not a fixture
    // scheduler. A scheduler's clock remains fixed until its owner advances it.
    let samples = RefCell::new([4_000_000, 4_000_073].into_iter());
    let clock = || MonoTime::from_micros(samples.borrow_mut().next().unwrap());
    let cycle = start_cycle(
        1,
        &fixture.serve,
        &mut fixture.metadata,
        &mut fixture.cues,
        &clock,
    )
    .unwrap();
    assert_eq!(cycle.base_us, 4_000_073);
    assert_eq!(cycle.boot, BOOT_BYTES);
    assert_eq!(samples.borrow_mut().next(), None);
    assert_eq!(
        fixture.metadata.records,
        vec![
            json!({"kind":"service_waiting_for_privacy","at_us":4_000_000,
                "next_cycle":1,"audio_workers_open":false,"service_sequence":1}),
            json!({"kind":"service_privacy_state","at_us":4_000_000,
                "state":"Allowed","audio_workers_open":false,"service_sequence":2}),
            json!({"kind":"service_cycle_start","at_us":4_000_000,"cycle":1,
                "fresh_runtime_required":true,"context_continuity_claimed":false,
                "service_sequence":3}),
            json!({"kind":"provider_ready","at_us":3_850_000,
                "coordinator_observed_at_us":3_850_000,"link":{"session":1},
                "note":"retained during startup","service_sequence":4}),
        ]
    );
}

#[test]
fn restarted_cycle_resamples_time_and_keeps_a_fresh_runtime_identity() {
    let mut fixture = Fixture::new();
    let samples = RefCell::new([1_000_000, 1_000_010, 9_000_000, 9_000_031].into_iter());
    let clock = || MonoTime::from_micros(samples.borrow_mut().next().unwrap());
    let first = start_cycle(
        1,
        &fixture.serve,
        &mut fixture.metadata,
        &mut fixture.cues,
        &clock,
    )
    .unwrap();
    let restarted = start_cycle(
        2,
        &fixture.serve,
        &mut fixture.metadata,
        &mut fixture.cues,
        &clock,
    )
    .unwrap();
    assert_eq!(first.base_us, 1_000_010);
    assert_eq!(restarted.base_us, 9_000_031);
    assert_ne!(restarted.boot, first.boot);
    assert_eq!(restarted.boot, CYCLE_TWO_BOOT);
    assert_eq!(restarted.runtime.now_us(), 0);
    assert_eq!(samples.borrow_mut().next(), None);
    assert_eq!(fixture.metadata.records.len(), 6);
    assert_eq!(
        &fixture.metadata.records[4..],
        &[
            json!({"kind":"service_cycle_start","at_us":9_000_000,"cycle":2,
                "fresh_runtime_required":true,"context_continuity_claimed":false,
                "service_sequence":5}),
            json!({"kind":"provider_ready","at_us":8_850_000,
                "coordinator_observed_at_us":8_850_000,"link":{"session":2},
                "note":"retained during startup","service_sequence":6}),
        ]
    );
}

#[test]
fn retained_startup_ready_still_saturates_at_zero() {
    let mut fixture = Fixture::new();
    let samples = RefCell::new([149_999, 150_073].into_iter());
    let clock = || MonoTime::from_micros(samples.borrow_mut().next().unwrap());
    let cycle = start_cycle(
        1,
        &fixture.serve,
        &mut fixture.metadata,
        &mut fixture.cues,
        &clock,
    )
    .unwrap();
    assert_eq!(cycle.base_us, 150_073);
    assert_eq!(fixture.metadata.records[2]["at_us"], 149_999);
    assert_eq!(fixture.metadata.records[3]["at_us"], 0);
    assert_eq!(fixture.metadata.records[3]["coordinator_observed_at_us"], 0);
    assert_eq!(samples.borrow_mut().next(), None);
}
