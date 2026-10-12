//! Production core and protocol emulator. No servo or physical-stop measurement.
use super::*;
use lamp_motor::{
    clearance::{ClearancePolicy, SafeCell},
    command::JointSelection,
    control::{HoldFailure, HoldOutcome, HoldReport, StopHoldPolicy},
};

fn drain(r: &mut Rig) -> HoldReport {
    for _ in 0..60 {
        if !r.motor.physical_stop_pending() {
            let report = r.motor.physical_stop_report().unwrap();
            report.validate(r.clock.now()).unwrap();
            return report;
        }
        r.clock.advance(2_000);
        r.motor.physical_stop_tick();
    }
    panic!("measured stop exceeded its 100 ms bound");
}
fn moving() -> (Rig, lamp_motor::control::MotionId) {
    let mut r = Rig::new();
    r.ready();
    r.wire.borrow_mut().hold_position = true;
    let id = r.start();
    r.first_goal();
    assert_ne!(r.wire.borrow().goals.last(), Some(&[2000; 5]));
    (r, id)
}

#[test]
fn cancellation_uses_a_new_measurement_not_the_old_goal_or_cached_position() {
    let (mut r, id) = moving();
    r.wire.borrow_mut().positions = [2003; 5];
    let before = r.wire.borrow().goals.len();
    let at = r.clock.now().as_micros();
    let inhibited = r.motor.cancel(id).unwrap();
    assert!(
        !inhibited.discard_attempted,
        "must preserve pending read framing"
    );
    assert!(r.motor.physical_stop_pending());
    assert!(matches!(r.motor.tick(), Err(Error::Stopped)));
    let report = drain(&mut r);
    assert_eq!(report.outcome, HoldOutcome::ObservedAtRest);
    assert_eq!(report.source.unwrap().joints.map(|s| s.counts), [2003; 5]);
    assert_eq!(r.wire.borrow().goals.len(), before + 1);
    assert_eq!(r.wire.borrow().goals.last(), Some(&[2003; 5]));
    assert_eq!(report.goal_accepted_bytes, 23);
    assert!(
        report
            .source
            .unwrap()
            .joints
            .iter()
            .all(|s| s.received_at_us > at)
    );
    assert!(report.finished_at_us.unwrap() - at <= 100_000);
    assert_eq!(r.wire.borrow().discards, 1);
    let packets = r.wire.borrow().packets.len();
    r.motor.physical_stop_tick();
    r.motor.shutdown();
    assert_eq!(r.wire.borrow().packets.len(), packets);
    assert_eq!(r.wire.borrow().discards, 1);
}

#[test]
fn no_accepted_goal_requires_no_measured_hold_or_new_packet() {
    let mut r = Rig::new();
    r.ready();
    let id = r.start();
    let packets = r.wire.borrow().packets.len();
    r.motor.cancel(id).unwrap();
    assert_eq!(drain(&mut r).outcome, HoldOutcome::NoAcceptedGoal);
    assert_eq!(r.wire.borrow().packets.len(), packets);
    assert_eq!(r.wire.borrow().discards, 1);
}

#[test]
fn selected_head_stop_writes_only_the_previously_active_id() {
    let mut r = Rig::new();
    r.ready();
    r.wire.borrow_mut().hold_position = true;
    let (p, place) = r.authority();
    let selection = JointSelection::from_mask(16).unwrap();
    let id = r
        .motor
        .start_selected_segment(p, place, [n(10.0); 5], 80_000, selection)
        .unwrap();
    r.first_goal();
    r.motor.cancel(id).unwrap();
    let report = drain(&mut r);
    assert_eq!(report.outcome, HoldOutcome::ObservedAtRest);
    assert_eq!(report.selection, Some(selection));
    assert_eq!(report.goal_accepted_bytes, 11);
    for packet in r.wire.borrow().packets.iter().filter(|p| p[4] == 0x83) {
        assert_eq!(packet.len(), 11);
        assert_eq!(packet[7], 5);
    }
}

#[test]
fn a_pending_telemetry_reply_is_drained_before_fresh_stop_reads() {
    let (mut r, id) = moving();
    // Wait until a read request is on the wire but no reply has been consumed.
    while r.wire.borrow().rx.is_empty() {
        r.step().unwrap();
    }
    r.wire.borrow_mut().positions = [2007; 5];
    r.motor.cancel(id).unwrap();
    let report = drain(&mut r);
    assert_eq!(report.outcome, HoldOutcome::ObservedAtRest);
    assert_eq!(report.source.unwrap().joints.map(|s| s.counts), [2007; 5]);
}

#[test]
fn a_partial_position_packet_never_gets_a_hold_packet_appended() {
    let (mut r, id) = moving();
    r.wire.borrow_mut().write_max = 7;
    for _ in 0..30 {
        r.step().unwrap();
        let w = r.wire.borrow();
        if w.partial.len() >= 5 && w.partial[4] == 0x83 {
            break;
        }
    }
    assert_eq!(r.wire.borrow().partial[4], 0x83);
    let bytes = r.wire.borrow().partial.clone();
    let packets = r.wire.borrow().packets.len();
    r.motor.cancel(id).unwrap();
    assert_eq!(drain(&mut r).failure, Some(HoldFailure::PartialPosition));
    assert_eq!(r.wire.borrow().partial, bytes);
    assert_eq!(r.wire.borrow().packets.len(), packets);
}

#[test]
fn stale_source_refuses_the_hold_instead_of_reusing_or_clipping_it() {
    let (mut r, id) = moving();
    r.motor.cancel(id).unwrap();
    while r.motor.physical_stop_report().unwrap().source.is_none() {
        r.clock.advance(2_000);
        r.motor.physical_stop_tick();
        assert!(r.motor.physical_stop_pending());
    }
    let before = r.wire.borrow().goals.len();
    r.clock.advance(20_001);
    r.motor.physical_stop_tick();
    let report = drain(&mut r);
    assert_eq!(report.failure, Some(HoldFailure::StaleSample));
    assert_eq!(report.goal_accepted_bytes, 0);
    assert_eq!(r.wire.borrow().goals.len(), before);
}

#[test]
fn source_expiring_inside_final_write_keeps_bytes_but_cannot_claim_rest() {
    let (mut r, id) = moving();
    r.motor.cancel(id).unwrap();
    while r.motor.physical_stop_report().unwrap().source.is_none() {
        r.clock.advance(2_000);
        r.motor.physical_stop_tick();
        assert!(r.motor.physical_stop_pending());
    }
    let source = r.motor.physical_stop_report().unwrap().source.unwrap();
    r.clock.0.set(source.joints[0].received_at_us + 19_000);
    r.wire.borrow_mut().write_delay_us = 2_000;
    r.motor.physical_stop_tick();
    let report = drain(&mut r);
    assert_eq!(report.failure, Some(HoldFailure::StaleSample));
    assert_eq!(report.goal_accepted_bytes, 23);
    assert!(report.goal_host_accepted_at_us.is_some());
    assert!(report.observed.is_none());
    report.validate(r.clock.now()).unwrap();
}

#[test]
fn abandoning_cleanup_flushes_once_and_never_reports_measured_rest() {
    let (mut r, id) = moving();
    r.motor.cancel(id).unwrap();
    let packets = r.wire.borrow().packets.len();
    r.motor.abandon_physical_stop();
    assert_eq!(drain(&mut r).failure, Some(HoldFailure::Abandoned));
    assert_eq!(r.wire.borrow().packets.len(), packets);
    assert_eq!(r.wire.borrow().discards, 1);
    r.motor.shutdown();
    r.motor.abandon_physical_stop();
    assert_eq!(r.wire.borrow().discards, 1);
}

#[test]
fn a_stalled_clock_cannot_keep_an_unsettled_hold_pending_forever() {
    let (mut r, id) = moving();
    r.wire.borrow_mut().moving = 1;
    r.motor.cancel(id).unwrap();
    for _ in 0..513 {
        r.motor.physical_stop_tick();
    }
    assert!(!r.motor.physical_stop_pending());
    assert_eq!(drain(&mut r).failure, Some(HoldFailure::OperationLimit));
    assert_eq!(r.wire.borrow().discards, 1);
}

#[test]
fn failed_profile_missing_readback_and_persistent_motion_never_claim_a_stop() {
    for case in 0..4 {
        let (mut r, id) = moving();
        match case {
            0 => r.wire.borrow_mut().torque = 0,
            1 => r.wire.borrow_mut().missing_id = Some(3),
            2 => r.wire.borrow_mut().moving = 1,
            _ => r.wire.borrow_mut().load = 101,
        }
        r.motor.cancel(id).unwrap();
        let report = drain(&mut r);
        assert_eq!(report.outcome, HoldOutcome::Failed, "case {case}");
        assert_eq!(
            report.failure,
            Some(if case == 0 {
                HoldFailure::Profile
            } else {
                HoldFailure::Deadline
            })
        );
        assert_eq!(r.wire.borrow().discards, 1);
    }
}

#[test]
fn bad_bus_and_missing_clearance_have_no_recovery_motion() {
    let (mut r, id) = moving();
    r.wire.borrow_mut().checksum_fault = true;
    r.motor.cancel(id).unwrap();
    let report = drain(&mut r);
    assert_eq!(report.failure, Some(HoldFailure::Protocol));
    assert_eq!(report.goal_accepted_bytes, 0);

    let mut r = Rig::new();
    r.motor
        .qualify(
            QualifiedEnvelope::new([7; 32], calibration(), [envelope(); 5], t(0), t(1_000_000))
                .unwrap(),
        )
        .unwrap();
    r.inspect();
    let (p, place) = r.authority();
    assert_eq!(
        r.motor.start_segment(p, place, [n(10.0); 5], 80_000),
        Err(Error::Qualification)
    );
    assert!(r.wire.borrow().goals.is_empty());
}

#[test]
fn combined_pose_gate_rejects_an_individually_valid_target_before_any_write() {
    let mut r = Rig::new();
    let q = qualification().with_motion_protection(
        ClearancePolicy::new(&[SafeCell::new([-35.0; 5], [35.0; 5]).unwrap()]).unwrap(),
        StopHoldPolicy::new(100_000, 20_000).unwrap(),
    );
    r.motor.qualify(q).unwrap();
    r.inspect();
    let (p, place) = r.authority();
    // 10 is within every joint limit; 10 + the qualified 30 tracking margin
    // is outside this combined-pose cell.
    assert!(matches!(
        r.motor.start_segment(p, place, [n(10.0); 5], 80_000),
        Err(Error::Clearance(_))
    ));
    assert!(r.wire.borrow().goals.is_empty());
}
