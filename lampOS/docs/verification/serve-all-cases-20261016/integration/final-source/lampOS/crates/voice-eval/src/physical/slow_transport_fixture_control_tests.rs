//! Delay receipts never claim successful exec, a new relay Hello, or OS exit.
use super::*;
use std::{os::unix::fs::DirBuilderExt, sync::atomic::AtomicUsize, thread};

const WATCHDOG: Duration = Duration::from_secs(2);
const OTHER_ROLES: [Role; 5] = [
    Role::Runtime,
    Role::Relay,
    Role::ServeRuntime,
    Role::ServeRelay,
    Role::Recorder,
];

fn step(round: u64, at_us: u64) -> Step {
    Step {
        round,
        at_us,
        stdout_target: 0,
        runner_input_target: 0,
        wait_for_sigterm: false,
    }
}

fn transport(poll_seq: u64, start: u64, due: u64, ended: bool) -> Progress {
    Progress::SlowTransport {
        poll_seq,
        wait_started_us: start,
        exec_due_us: due,
        logical_end: ended,
    }
}

fn hello(role: Role) -> Frame {
    Frame::Hello {
        version: VERSION,
        role,
    }
}

fn other_progress(role: Role) -> Progress {
    let encoded = match role {
        Role::Runtime => {
            r#"{"role":"runtime","poll_seq":1,"stdout_frames":0,"room_inputs":0,"logical_end":false}"#
        }
        Role::Relay => {
            r#"{"role":"relay","poll_seq":1,"stdout_frames":0,"runner_inputs":0,"output_frames":0,"logical_end":false}"#
        }
        Role::ServeRuntime => {
            r#"{"role":"serve_runtime","poll_seq":1,"stdout_frames":0,"room_consumed":0,"room_injected":0,"cycle":1,"cycle_base_us":100,"runtime_us":0,"observed_signal":0,"logical_end":false}"#
        }
        Role::ServeRelay => {
            r#"{"role":"serve_relay","poll_seq":1,"stdout_frames":0,"runner_inputs":0,"output_frames":0,"stop_signal_sent_us":null,"logical_end":false}"#
        }
        Role::Recorder => {
            r#"{"role":"recorder","poll_seq":1,"ready_written":true,"wait_started_us":100,"exit_due_us":200,"logical_end":false}"#
        }
        _ => unreachable!("this helper describes only pre-existing roles"),
    };
    serde_json::from_str(encoded).unwrap()
}

#[test]
fn transport_frames_add_only_the_role_and_fixed_wait_boundary() {
    assert_eq!(VERSION, 1);
    assert_eq!(
        serde_json::to_string(&hello(Role::SlowTransport)).unwrap(),
        r#"{"type":"hello","version":1,"role":"slow_transport"}"#
    );
    assert_eq!(
        serde_json::to_string(&Frame::Step {
            role: Role::SlowTransport,
            step: step(1, 100),
        })
        .unwrap(),
        r#"{"type":"step","role":"slow_transport","step":{"round":1,"at_us":100,"stdout_target":0,"runner_input_target":0}}"#
    );
    assert_eq!(
        serde_json::to_string(&transport(1, 100, 400_100, false)).unwrap(),
        r#"{"role":"slow_transport","poll_seq":1,"wait_started_us":100,"exec_due_us":400100,"logical_end":false}"#
    );
    for field in [
        "exec_succeeded",
        "relay_hello",
        "exit_code",
        "pid",
        "output_frames",
    ] {
        let mut value = serde_json::to_value(transport(1, 100, 100, true)).unwrap();
        value[field] = 1.into();
        assert!(
            serde_json::from_value::<Progress>(value).is_err(),
            "{field}"
        );
    }
}

#[test]
fn transport_role_is_isolated_from_every_existing_hello_and_progress() {
    for other in OTHER_ROLES {
        for (expected, actual) in [(Role::SlowTransport, other), (other, Role::SlowTransport)] {
            let (parent, raw) = UnixStream::pair().unwrap();
            Channel::new(raw, WATCHDOG)
                .unwrap()
                .write_frame(&hello(actual), deadline(WATCHDOG).unwrap())
                .unwrap();
            assert_eq!(
                ParentPeer::from_stream(parent, expected, WATCHDOG, deadline(WATCHDOG).unwrap())
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
        }
        assert!(
            validate_progress(other, None, step(1, 100), transport(1, 100, 200, false)).is_err()
        );
        assert!(
            validate_progress(
                Role::SlowTransport,
                None,
                step(1, 100),
                other_progress(other)
            )
            .is_err()
        );
    }
}

#[test]
fn transport_and_other_roles_reject_each_others_outer_steps() {
    for other in OTHER_ROLES {
        for (expected, actual) in [(Role::SlowTransport, other), (other, Role::SlowTransport)] {
            let (raw, child) = UnixStream::pair().unwrap();
            let mut raw = Channel::new(raw, WATCHDOG).unwrap();
            raw.write_frame(
                &Frame::Step {
                    role: actual,
                    step: step(1, 100),
                },
                deadline(WATCHDOG).unwrap(),
            )
            .unwrap();
            let error = Participant::from_stream(child, expected, WATCHDOG).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert!(
                error
                    .to_string()
                    .contains(&format!("expected {expected:?} Step"))
            );
        }
    }
}

#[test]
fn transport_rejects_input_signal_and_unrepresentable_domain_targets() {
    for invalid_step in [
        Step {
            stdout_target: 1,
            ..step(1, 100)
        },
        Step {
            runner_input_target: 1,
            ..step(1, 100)
        },
        Step {
            wait_for_sigterm: true,
            ..step(1, 100)
        },
        step(1, u64::MAX / 1000 + 1),
    ] {
        assert!(validate_step(Role::SlowTransport, None, invalid_step).is_err());
        assert!(
            validate_progress(
                Role::SlowTransport,
                None,
                invalid_step,
                transport(1, invalid_step.at_us, invalid_step.at_us, true),
            )
            .is_err()
        );
    }
    let last = u64::MAX / 1000;
    assert!(validate_step(Role::SlowTransport, None, step(1, last)).is_ok());
    assert!(
        validate_progress(
            Role::SlowTransport,
            None,
            step(1, last),
            transport(1, last, last, true)
        )
        .is_ok()
    );
    assert!(
        validate_progress(
            Role::SlowTransport,
            None,
            step(1, 100),
            transport(1, 100, last + 1, false)
        )
        .is_err()
    );
}

#[test]
fn first_transport_wait_starts_at_the_current_held_step() {
    for progress in [
        transport(1, 99, 200, false),
        transport(1, 101, 200, false),
        transport(1, 100, 99, true),
    ] {
        assert!(validate_progress(Role::SlowTransport, None, step(1, 100), progress).is_err());
    }
    assert!(
        validate_progress(
            Role::SlowTransport,
            None,
            step(1, 100),
            transport(1, 100, 200, false)
        )
        .is_ok()
    );
}

#[test]
fn transport_start_and_due_stay_fixed_in_both_directions() {
    let previous = Some(transport(1, 100, 200, false));
    for progress in [
        transport(2, 99, 200, false),
        transport(2, 101, 200, false),
        transport(2, 100, 199, false),
        transport(2, 100, 201, false),
    ] {
        assert!(validate_progress(Role::SlowTransport, previous, step(2, 150), progress).is_err());
    }
    assert!(
        validate_progress(
            Role::SlowTransport,
            previous,
            step(2, 150),
            transport(2, 100, 200, false)
        )
        .is_ok()
    );
}

#[test]
fn terminal_transport_progress_is_required_if_and_only_if_the_full_delay_elapsed() {
    for at in [199, 200, 201] {
        for terminal in [false, true] {
            assert_eq!(
                validate_progress(
                    Role::SlowTransport,
                    Some(transport(1, 100, 200, false)),
                    step(2, at),
                    transport(2, 100, 200, terminal),
                )
                .is_ok(),
                terminal == (at >= 200)
            );
        }
    }
    assert!(
        validate_progress(
            Role::SlowTransport,
            None,
            step(1, 100),
            transport(1, 100, 100, true)
        )
        .is_ok()
    );
    assert!(
        validate_progress(
            Role::SlowTransport,
            Some(transport(1, 100, 100, true)),
            step(2, 100),
            transport(2, 100, 100, true),
        )
        .is_err()
    );
}

#[test]
fn transport_round_poll_and_time_validation_remain_exact() {
    for round in [0, 1, 3] {
        assert!(validate_step(Role::SlowTransport, Some(step(1, 100)), step(round, 100)).is_err());
    }
    assert!(validate_step(Role::SlowTransport, Some(step(1, 100)), step(2, 99)).is_err());
    assert!(validate_step(Role::SlowTransport, Some(step(u64::MAX, 100)), step(0, 100)).is_err());
    for poll in [0, 1, 3] {
        assert!(
            validate_progress(
                Role::SlowTransport,
                Some(transport(1, 100, 200, false)),
                step(2, 100),
                transport(poll, 100, 200, false),
            )
            .is_err()
        );
    }
    assert!(
        validate_progress(
            Role::SlowTransport,
            Some(transport(u64::MAX, 100, 200, false)),
            step(2, 100),
            transport(0, 100, 200, false),
        )
        .is_err()
    );
}

#[test]
fn repeated_transport_rounds_leave_clock_reads_pure_until_the_next_step() {
    let (parent, child) = UnixStream::pair().unwrap();
    let worker = thread::spawn(move || {
        let mut participant =
            Participant::from_stream(child, Role::SlowTransport, WATCHDOG).unwrap();
        let clock = participant.clock();
        for _ in 0..32 {
            assert_eq!(clock.now().as_micros(), 100);
        }
        assert_eq!(
            participant.complete(transport(1, 100, 200, false)).unwrap(),
            step(2, 100)
        );
        for _ in 0..32 {
            assert_eq!(clock.now().as_micros(), 100);
        }
        assert_eq!(
            participant.complete(transport(2, 100, 200, false)).unwrap(),
            step(3, 200)
        );
        participant.final_ack(transport(3, 100, 200, true)).unwrap();
        assert_eq!(clock.now().as_micros(), 200);
        assert!(participant.final_ack(transport(4, 100, 200, true)).is_err());
    });
    let mut parent = ParentPeer::from_stream(
        parent,
        Role::SlowTransport,
        WATCHDOG,
        deadline(WATCHDOG).unwrap(),
    )
    .unwrap();
    for (round, at) in [(1, 100), (2, 100), (3, 200)] {
        assert_eq!(
            parent.step(step(round, at)).unwrap(),
            transport(round, 100, 200, at == 200)
        );
    }
    assert!(parent.step(step(4, 200)).is_err());
    worker.join().unwrap();
}

#[test]
fn an_invalid_transport_domain_step_never_changes_the_pure_clock() {
    let (raw, child) = UnixStream::pair().unwrap();
    let mut raw = Channel::new(raw, WATCHDOG).unwrap();
    for next in [step(1, 100), step(2, u64::MAX / 1000 + 1)] {
        raw.write_frame(
            &Frame::Step {
                role: Role::SlowTransport,
                step: next,
            },
            deadline(WATCHDOG).unwrap(),
        )
        .unwrap();
    }
    let mut participant = Participant::from_stream(child, Role::SlowTransport, WATCHDOG).unwrap();
    let clock = participant.clock();
    assert!(participant.complete(transport(1, 100, 200, false)).is_err());
    assert_eq!(clock.now().as_micros(), 100);
    assert!(participant.closed);
}

#[test]
fn missing_transport_progress_has_a_bounded_failure_without_an_exec_claim() {
    let (parent, raw) = UnixStream::pair().unwrap();
    let mut raw = Channel::new(raw, WATCHDOG).unwrap();
    raw.write_frame(&hello(Role::SlowTransport), deadline(WATCHDOG).unwrap())
        .unwrap();
    let mut parent = ParentPeer::from_stream(
        parent,
        Role::SlowTransport,
        Duration::from_millis(20),
        deadline(WATCHDOG).unwrap(),
    )
    .unwrap();
    assert_eq!(
        parent.step(step(1, 100)).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert!(parent.failed);
    assert!(parent.last_progress.is_none());
    assert!(parent.step(step(1, 100)).is_err());
}

#[test]
fn a_transport_listener_slot_never_substitutes_for_a_fresh_relay_hello() {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = PathBuf::from(format!(
        "/tmp/lve-transport-wire-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let path = root.join("control.sock");
    let listener = ParentListener::bind_with_watchdog(&path, WATCHDOG).unwrap();
    for role in [
        Role::SlowTransport,
        Role::ServeRelay,
        Role::Recorder,
        Role::Runtime,
    ] {
        let raw = UnixStream::connect(&path).unwrap();
        let mut raw = Channel::new(raw, WATCHDOG).unwrap();
        raw.write_frame(&hello(role), deadline(WATCHDOG).unwrap())
            .unwrap();
        assert!(listener.accept(role).is_ok());
    }
    assert_eq!(listener.accepted_roles.get(), 32 | 8 | 16 | 1);
    assert!(listener.accept(Role::SlowTransport).is_err());
    drop(listener);
    fs::remove_dir(&root).unwrap();
}
