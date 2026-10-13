//! Production Driver, Controller, Supervisor, worker and motor core over inert
//! bus/Unix IPC. These frames and PCM authority are test inputs, never native proof.
use super::*;
use crate::{
    camera_worker::{FrameMetadata, Mode, Timing},
    head_qualification::{Action, Driver, Plan, Reply, Request, Stage},
};
use lamp_ipc::Endpoint;

fn plan() -> Plan {
    Plan {
        head: HeadPlan {
            unit: "inert".into(),
            qualification_record: [7; 32],
            expected_initial_counts: [2000; 5],
            minimum_count: 1990,
            maximum_count: 2010,
            outbound_count: 2004,
            segment_duration_us: 30_000,
            maximum_duration_us: 5_000_000,
            readback_age_us: 200_000,
        },
        motor_config_sha256: "aa".repeat(32),
        camera_config_sha256: "bb".repeat(32),
        original_placement_source: BootId::new([3; 16]).unwrap(),
        placement_review_sha256: "cc".repeat(32),
        kernel_boot_id: "test-only".into(),
        reviewed_at_us: monotonic_us(),
        valid_until_us: monotonic_us() + 20_000_000,
    }
}
fn frame(c: &mut Controller, sequence: u64) -> FrameMetadata {
    let at = monotonic_us();
    FrameMetadata {
        worker: BootId::new([3; 16]).unwrap(),
        capture_epoch: 1,
        sequence,
        grant: c.camera_grant(time()).unwrap(),
        mode: Mode {
            source: "desk-camera".into(),
            format: *b"MJPG",
            width: 640,
            height: 480,
            size_image: 4096,
            buffers: 2,
            interval: None,
        },
        bytes_used: 20,
        driver_sequence: sequence as u32,
        driver_timestamp: None,
        dequeue: Timing {
            started_at_us: at,
            completed_at_us: at,
        },
        published_at_us: at,
    }
}
fn refresh(m: &mut OptionalMotor, c: &mut Controller) {
    let at = time();
    let until = at.checked_add(100_000).unwrap();
    c.set_capture(at, CaptureState::RetainingUntil(until))
        .unwrap();
    c.set_admission(at, AdmissionState::OpenUntil(until))
        .unwrap();
    m.publish(c.snapshot(time()).unwrap());
}
fn exchange(
    d: &mut Driver,
    m: &mut OptionalMotor,
    c: &mut Controller,
    peer: &Endpoint,
    r: Request,
    f: Option<&FrameMetadata>,
) -> Reply {
    peer.try_send(&serde_json::to_vec(&r).unwrap()).unwrap();
    d.tick(c, m, f, true);
    let mut bytes = [0; lamp_ipc::MAX_DATAGRAM_BYTES];
    let n = peer.try_recv(&mut bytes).unwrap();
    serde_json::from_slice(&bytes[..n]).unwrap()
}
fn attached(
    mode: &str,
) -> (
    crate::process::SessionDirectory,
    OptionalMotor,
    Controller,
    Driver,
    Endpoint,
) {
    let (dir, mut m, mut c) = fixture(mode);
    wait(&mut m, &mut c, OptionalMotor::is_ready);
    let (runtime, peer) = Endpoint::pair().unwrap();
    let session = c.snapshot(time()).unwrap().boot();
    m.attach_head(Driver::test_driver(
        plan(),
        BootId::new([3; 16]).unwrap(),
        session,
        runtime,
    ))
    .unwrap();
    let d = m.head.take().unwrap();
    (dir, m, c, d, peer)
}
#[test]
fn native_head_four_explicit_actions_use_original_owner_and_reap() {
    run_sequence("normal", false);
}
#[test]
fn native_head_six_explicit_actions_prepare_profile_then_new_requests() {
    run_sequence("profile", true);
}
fn run_sequence(mode: &str, profile: bool) {
    let (dir, mut m, mut c, mut d, peer) = attached(mode);
    refresh(&mut m, &mut c);
    let owner = c.admit(time(), AdmittedInput::NewTurn).unwrap();
    let mut actions = vec![
        Action::Inspect,
        Action::Outbound,
        Action::Reinspect,
        Action::Return,
    ];
    if profile {
        actions.splice(0..0, [Action::InspectProfile, Action::PrepareProfile]);
    }
    let mut seq = 0;
    let mut frame_seq = 0;
    for action in actions {
        refresh(&mut m, &mut c);
        frame_seq += 1;
        let f = frame(&mut c, frame_seq);
        seq += 1;
        let r = exchange(
            &mut d,
            &mut m,
            &mut c,
            &peer,
            Request::Status {
                sequence: seq,
                sent_at_us: monotonic_us(),
            },
            Some(&f),
        );
        let t = r.ticket.expect("live owner/fresh frame ticket");
        assert_eq!(t.owner, owner);
        assert_eq!(t.stage, action.stage());
        seq += 1;
        let r = exchange(
            &mut d,
            &mut m,
            &mut c,
            &peer,
            Request::Act {
                sequence: seq,
                sent_at_us: monotonic_us(),
                ticket: t.id,
                owner,
                action,
            },
            Some(&f),
        );
        assert!(r.error.is_none(), "{:?}", r.error);
        let until = Instant::now() + Duration::from_millis(400);
        let target = match action {
            Action::InspectProfile => Stage::ProfileInspected,
            Action::PrepareProfile => Stage::New,
            Action::Inspect => Stage::Initial,
            Action::Outbound => Stage::AtOutbound,
            Action::Reinspect => Stage::ReturnReady,
            Action::Return => Stage::Returned,
        };
        while m.qualification_stage() != target {
            assert!(
                Instant::now() < until,
                "phase {:?}, failure {:?}",
                m.qualification_stage(),
                m.failure()
            );
            refresh(&mut m, &mut c);
            m.poll(|_| {});
            std::thread::park_timeout(Duration::from_millis(1));
        }
    }
    wait(&mut m, &mut c, OptionalMotor::reaped);
    assert!(m.finish().is_ok());
    assert_eq!(
        fs::read_to_string(dir.path.join("child-emitted-mask")).unwrap(),
        "16"
    );
    let counts: Vec<u16> =
        serde_json::from_slice(&fs::read(dir.path.join("child-final-counts")).unwrap()).unwrap();
    assert_eq!(counts, vec![2000; 5]);
    cleanup(m, c, dir);
}
#[test]
fn native_head_no_owner_no_capture_or_no_frame_never_grants() {
    let (dir, mut m, mut c, mut d, peer) = attached("normal");
    let f = frame(&mut c, 1);
    let r = exchange(
        &mut d,
        &mut m,
        &mut c,
        &peer,
        Request::Status {
            sequence: 1,
            sent_at_us: monotonic_us(),
        },
        Some(&f),
    );
    assert!(r.ticket.is_none());
    refresh(&mut m, &mut c);
    let r = exchange(
        &mut d,
        &mut m,
        &mut c,
        &peer,
        Request::Status {
            sequence: 2,
            sent_at_us: monotonic_us(),
        },
        Some(&f),
    );
    assert!(r.ticket.is_none());
    c.admit(time(), AdmittedInput::NewTurn).unwrap();
    let r = exchange(
        &mut d,
        &mut m,
        &mut c,
        &peer,
        Request::Status {
            sequence: 3,
            sent_at_us: monotonic_us(),
        },
        None,
    );
    assert!(r.ticket.is_none());
    cleanup(m, c, dir);
}
#[test]
fn native_head_owner_cancel_between_ticket_and_action_never_dispatches() {
    let (dir, mut m, mut c, mut d, peer) = attached("normal");
    refresh(&mut m, &mut c);
    let owner = c.admit(time(), AdmittedInput::NewTurn).unwrap();
    let f = frame(&mut c, 1);
    let t = exchange(
        &mut d,
        &mut m,
        &mut c,
        &peer,
        Request::Status {
            sequence: 1,
            sent_at_us: monotonic_us(),
        },
        Some(&f),
    )
    .ticket
    .unwrap();
    c.cancel_turn(time(), owner).unwrap();
    let r = exchange(
        &mut d,
        &mut m,
        &mut c,
        &peer,
        Request::Act {
            sequence: 2,
            sent_at_us: monotonic_us(),
            ticket: t.id,
            owner,
            action: Action::Inspect,
        },
        Some(&f),
    );
    assert!(r.error.is_some());
    assert_eq!(m.qualification_stage(), Stage::Inhibited);
    wait(&mut m, &mut c, OptionalMotor::reaped);
    assert_eq!(
        fs::read_to_string(dir.path.join("child-position-writes")).unwrap(),
        "0"
    );
    cleanup(m, c, dir);
}
#[test]
fn native_head_privacy_at_dispatch_closes_optional_only() {
    let (dir, mut m, mut c, mut d, peer) = attached("normal");
    refresh(&mut m, &mut c);
    let owner = c.admit(time(), AdmittedInput::NewTurn).unwrap();
    let f = frame(&mut c, 1);
    let t = exchange(
        &mut d,
        &mut m,
        &mut c,
        &peer,
        Request::Status {
            sequence: 1,
            sent_at_us: monotonic_us(),
        },
        Some(&f),
    )
    .ticket
    .unwrap();
    peer.try_send(
        &serde_json::to_vec(&Request::Act {
            sequence: 2,
            sent_at_us: monotonic_us(),
            ticket: t.id,
            owner,
            action: Action::Inspect,
        })
        .unwrap(),
    )
    .unwrap();
    d.tick(&mut c, &mut m, Some(&f), false);
    assert_eq!(m.qualification_stage(), Stage::Inhibited);
    assert_eq!(c.snapshot(time()).unwrap().owner(), Some(owner));
    cleanup(m, c, dir);
}
#[test]
fn native_head_stale_or_foreign_frame_cannot_grant() {
    let (dir, mut m, mut c, mut d, peer) = attached("normal");
    refresh(&mut m, &mut c);
    c.admit(time(), AdmittedInput::NewTurn).unwrap();
    for sequence in 1..=3 {
        let mut f = frame(&mut c, sequence);
        match sequence {
            1 => f.worker = BootId::new([99; 16]).unwrap(),
            2 => {
                f.dequeue.completed_at_us = f
                    .dequeue
                    .completed_at_us
                    .saturating_sub(lamp_camera::MAX_DEQUEUE_AGE_US)
            }
            _ => f.dequeue.completed_at_us += 1_000_000,
        }
        let r = exchange(
            &mut d,
            &mut m,
            &mut c,
            &peer,
            Request::Status {
                sequence,
                sent_at_us: monotonic_us(),
            },
            Some(&f),
        );
        assert!(r.ticket.is_none());
    }
    cleanup(m, c, dir);
}
#[test]
fn native_head_binding_changes_only_explicit_camera_boot() {
    let mut c = config();
    let before = serde_json::to_value(&c).unwrap();
    let old = c.placement_source;
    let b = PlacementBinding {
        original_sha256: "aa".repeat(32),
        original_source: old,
        assigned_source: BootId::new([9; 16]).unwrap(),
        record: [7; 32],
        review_sha256: "bb".repeat(32),
    };
    bind_placement(&mut c, &b.original_sha256, &b).unwrap();
    let mut after = serde_json::to_value(&c).unwrap();
    after["placement_source"] = before["placement_source"].clone();
    assert_eq!(before, after);
    assert_eq!(c.placement_source, b.assigned_source);
    assert!(bind_placement(&mut c, "wrong", &b).is_err());
}

#[test]
fn native_head_replayed_receipt_and_changed_camera_grant_are_terminal() {
    for changed_grant in [false, true] {
        let (dir, mut m, mut c, mut d, peer) = attached("normal");
        refresh(&mut m, &mut c);
        let owner = c.admit(time(), AdmittedInput::NewTurn).unwrap();
        let f = frame(&mut c, 1);
        let t = exchange(
            &mut d,
            &mut m,
            &mut c,
            &peer,
            Request::Status {
                sequence: 1,
                sent_at_us: monotonic_us(),
            },
            Some(&f),
        )
        .ticket
        .unwrap();
        if changed_grant {
            c.set_camera_permission(time(), Permission::Denied).unwrap();
            c.set_camera_permission(time(), Permission::Allowed)
                .unwrap();
        }
        let r = exchange(
            &mut d,
            &mut m,
            &mut c,
            &peer,
            Request::Act {
                sequence: if changed_grant { 2 } else { 1 },
                sent_at_us: monotonic_us(),
                ticket: t.id,
                owner,
                action: Action::Inspect,
            },
            Some(&f),
        );
        assert!(r.error.is_some());
        assert_eq!(m.qualification_stage(), Stage::Inhibited);
        cleanup(m, c, dir);
    }
}
#[test]
fn native_head_absence_of_explicit_next_action_never_schedules_return() {
    let (dir, mut m, mut c, mut d, peer) = attached("normal");
    refresh(&mut m, &mut c);
    let owner = c.admit(time(), AdmittedInput::NewTurn).unwrap();
    let f = frame(&mut c, 1);
    let t = exchange(
        &mut d,
        &mut m,
        &mut c,
        &peer,
        Request::Status {
            sequence: 1,
            sent_at_us: monotonic_us(),
        },
        Some(&f),
    )
    .ticket
    .unwrap();
    let r = exchange(
        &mut d,
        &mut m,
        &mut c,
        &peer,
        Request::Act {
            sequence: 2,
            sent_at_us: monotonic_us(),
            ticket: t.id,
            owner,
            action: Action::Inspect,
        },
        Some(&f),
    );
    assert!(r.error.is_none());
    wait(&mut m, &mut c, |m| {
        m.qualification_stage() == Stage::Initial
    });
    c.cancel_turn(time(), owner).unwrap();
    m.publish(c.snapshot(time()).unwrap());
    assert_eq!(m.qualification_stage(), Stage::Inhibited);
    wait(&mut m, &mut c, OptionalMotor::reaped);
    assert_eq!(
        fs::read_to_string(dir.path.join("child-position-writes")).unwrap(),
        "0"
    );
    cleanup(m, c, dir);
}

#[test]
fn native_head_initial_privacy_acquisition_is_not_a_revoke_or_grant() {
    let (dir, mut m, mut c) = fixture("startup");
    let (runtime, peer) = Endpoint::pair().unwrap();
    let session = c.snapshot(time()).unwrap().boot();
    m.attach_head(Driver::test_driver(
        plan(),
        BootId::new([3; 16]).unwrap(),
        session,
        runtime,
    ))
    .unwrap();
    let mut d = m.head.take().unwrap();
    peer.try_send(
        &serde_json::to_vec(&Request::Status {
            sequence: 1,
            sent_at_us: monotonic_us(),
        })
        .unwrap(),
    )
    .unwrap();
    d.tick(&mut c, &mut m, None, false);
    assert!(
        !m.stopping,
        "initial unknown privacy is not a prior grant to revoke"
    );
    let mut bytes = [0; lamp_ipc::MAX_DATAGRAM_BYTES];
    let n = peer.try_recv(&mut bytes).unwrap();
    let r: Reply = serde_json::from_slice(&bytes[..n]).unwrap();
    assert!(r.ticket.is_none());
    m.start_before_capture(c.snapshot(time()).unwrap(), false, true);
    wait(&mut m, &mut c, OptionalMotor::is_ready);
    refresh(&mut m, &mut c);
    c.admit(time(), AdmittedInput::NewTurn).unwrap();
    let f = frame(&mut c, 1);
    let r = exchange(
        &mut d,
        &mut m,
        &mut c,
        &peer,
        Request::Status {
            sequence: 2,
            sent_at_us: monotonic_us(),
        },
        Some(&f),
    );
    assert!(r.ticket.is_some());
    cleanup(m, c, dir);
}

#[test]
fn native_head_startup_announcement_has_no_input_or_motion_authority() {
    let (dir, mut m, mut c) = fixture("startup");
    let (runtime, peer) = Endpoint::pair().unwrap();
    let session = c.snapshot(time()).unwrap().boot();
    m.attach_head(Driver::test_driver(
        plan(),
        BootId::new([3; 16]).unwrap(),
        session,
        runtime,
    ))
    .unwrap();
    let mut d = m.head.take().unwrap();
    d.test_unannounced();
    d.tick(&mut c, &mut m, None, false);
    let mut bytes = [0; lamp_ipc::MAX_DATAGRAM_BYTES];
    let n = peer.try_recv(&mut bytes).unwrap();
    let hello: Reply = serde_json::from_slice(&bytes[..n]).unwrap();
    assert_eq!(hello.sequence, 0);
    assert_eq!(hello.session, session);
    assert!(hello.ticket.is_none());
    assert!(!hello.cleanup_verified);
    assert!(!m.started());
    assert!(!m.stopping);
    assert!(c.snapshot(time()).unwrap().owner().is_none());
    d.tick(&mut c, &mut m, None, false);
    assert_eq!(
        peer.try_recv(&mut bytes).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    cleanup(m, c, dir);
}
