//! Synthetic lifecycle and stalled-storage tests; no hardware or cloud.
use super::*;
use crate::service_trace::ServiceTrace;

#[test]
fn stalled_metadata_cannot_erase_cancellation_lineage_or_block_privacy() {
    let (writer, entered, release) = ServiceTrace::stalled();
    let (_, reply, original, event, observed) = cancelled_tail_fixture();
    let mut state = SessionState::with_writer(writer);
    state.receipts = original.receipts;
    record(&mut state, json!({"kind":"block_writer"})).unwrap();
    flush_cues(&mut state, &mut None);
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    for n in 0..400 {
        record(&mut state, json!({"kind":"synthetic_tick","n":n})).unwrap();
        flush_cues(&mut state, &mut None);
    }
    assert!(state.is_empty());
    // The JSON cancellation/admission rows no longer exist, but the exact
    // original owner can settle once without touching the successor reply.
    let encoded = crate::wire::encode(&event).unwrap();
    handle_cancelled_tail_retirement(
        &reply,
        &mut state,
        crate::wire::decode(&encoded).unwrap(),
        observed,
    )
    .unwrap();
    assert!(handle_cancelled_tail_retirement(&reply, &mut state, event, observed).is_err());
    let (mut controller, active, permit) = active_reply();
    let mut active = Some(active);
    cancel_reply(&mut controller, &mut active, &mut state, "user_interrupted").unwrap();
    controller
        .set_microphone_permission(now(), Permission::Denied)
        .unwrap();
    let snapshot = controller.snapshot(now()).unwrap();
    assert_eq!(snapshot.owner(), None);
    let mut boundary = BoundaryGuard::new(snapshot.boot(), now());
    boundary.install(now(), snapshot).unwrap();
    assert!(boundary.check(now(), permit, OutputKind::Speech).is_err());
    flush_cues(&mut state, &mut None);
    release.send(()).unwrap();
    let health = state.finish_logging().unwrap();
    assert!(health.acknowledged && health.dropped_events > 0);
}

#[test]
fn service_batch_bound_drops_observation_instead_of_growing_or_failing() {
    // The authority/presentation tests still validate every frame. This checks
    // that rotating away metadata cannot leave a mutable summary index behind.
    let (writer, _entered, release) = ServiceTrace::stalled();
    let mut state = SessionState::with_writer(writer);
    assert!(state.continuous());
    for _ in 0..3 {
        for n in 0..300 {
            record(&mut state, json!({"kind":"bounded_batch","n":n})).unwrap();
        }
        assert!(state.len() <= 256);
        flush_cues(&mut state, &mut None);
        assert!(state.is_empty());
    }
    release.send(()).unwrap();
    assert!(state.finish_logging().unwrap().degraded);
}

#[test]
fn finite_capacity_remains_strict_while_service_drops_only_observation() {
    let mut finite = SessionState::default();
    for _ in 0..crate::session_state::MAX_FINITE_EVENTS - 1 {
        record(&mut finite, json!({"kind":"synthetic"})).unwrap();
    }
    assert!(record(&mut finite, json!({"kind":"too_many"})).is_err());
}

#[test]
fn metadata_submission_cost_is_observed_without_waiting_for_a_stalled_disk() {
    let (writer, entered, release) = ServiceTrace::stalled();
    let mut state = SessionState::with_writer(writer);
    record(&mut state, json!({"kind":"block_writer"})).unwrap();
    flush_cues(&mut state, &mut None);
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut elapsed = Vec::with_capacity(1_000);
    for index in 0..1_000 {
        let started = Instant::now();
        record(
            &mut state,
            json!({"kind":"synthetic_bounded_metadata","at_us":index,"value":17}),
        )
        .unwrap();
        flush_cues(&mut state, &mut None);
        elapsed.push(started.elapsed().as_nanos());
    }
    elapsed.sort_unstable();
    println!(
        "local host metadata record+flush, stalled writer, n=1000: median={} ns, p95(order statistic)={} ns, max={} ns; no hardware/acoustic claim",
        elapsed[499], elapsed[949], elapsed[999]
    );
    release.send(()).unwrap();
    let report = state.finish_logging().unwrap();
    assert!(report.acknowledged && report.dropped_events > 0);
}

#[test]
fn metadata_initialization_failure_is_explicit_and_does_not_abort_voice_state() {
    let directory = SessionDirectory::create().unwrap();
    fs::write(
        directory.path.join("metadata"),
        b"existing output is never overwritten",
    )
    .unwrap();
    let mut state = SessionState::service(&directory.path);
    let (mut owner, reply, _) = active_reply();
    let mut reply = Some(reply);
    cancel_reply(&mut owner, &mut reply, &mut state, "user_interrupted").unwrap();
    flush_cues(&mut state, &mut None);
    let report = state.finish_logging().unwrap();
    assert!(report.initialization_failed && report.degraded && !report.acknowledged);
    assert_eq!(report.dropped_events, 1);
    assert_eq!(report.written_events, 0);
    assert!(reply.is_none());
    assert_eq!(
        fs::read(directory.path.join("metadata")).unwrap(),
        b"existing output is never overwritten"
    );
    fs::remove_file(directory.path.join("metadata")).unwrap();
}
