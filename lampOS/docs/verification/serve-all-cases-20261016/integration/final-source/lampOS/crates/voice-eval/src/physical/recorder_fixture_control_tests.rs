//! Recorder receipts describe real write/wait boundaries, never child exit.
use super::*;
use std::{os::unix::fs::DirBuilderExt, sync::atomic::AtomicUsize, thread};

const WATCHDOG: Duration = Duration::from_secs(2);
const OTHER_ROLES: [Role; 4] = [
    Role::Runtime,
    Role::Relay,
    Role::ServeRuntime,
    Role::ServeRelay,
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

fn recorder(poll_seq: u64, start: u64, due: u64, ended: bool) -> Progress {
    Progress::Recorder {
        poll_seq,
        ready_written: true,
        wait_started_us: start,
        exit_due_us: due,
        logical_end: ended,
    }
}

fn other_progress(role: Role) -> Progress {
    match role {
        Role::Runtime => Progress::Runtime {
            poll_seq: 1,
            stdout_frames: 0,
            room_inputs: 0,
            logical_end: false,
        },
        Role::Relay => Progress::Relay {
            poll_seq: 1,
            stdout_frames: 0,
            runner_inputs: 0,
            output_frames: 0,
            logical_end: false,
        },
        Role::ServeRuntime => Progress::ServeRuntime {
            poll_seq: 1,
            stdout_frames: 0,
            room_consumed: 0,
            room_injected: 0,
            cycle: 1,
            cycle_base_us: 100,
            runtime_us: 0,
            observed_signal: 0,
            logical_end: false,
        },
        Role::ServeRelay => Progress::ServeRelay {
            poll_seq: 1,
            stdout_frames: 0,
            runner_inputs: 0,
            output_frames: 0,
            stop_signal_sent_us: None,
            logical_end: false,
        },
        Role::Recorder | Role::SlowTransport => unreachable!(),
    }
}

fn hello(role: Role) -> Frame {
    Frame::Hello {
        version: VERSION,
        role,
    }
}

#[test]
fn recorder_wire_has_no_status_or_extra_step_fields() {
    assert_eq!(
        serde_json::to_string(&Role::Recorder).unwrap(),
        "\"recorder\""
    );
    assert_eq!(
        serde_json::to_string(&hello(Role::Recorder)).unwrap(),
        r#"{"type":"hello","version":1,"role":"recorder"}"#
    );
    let progress = recorder(1, 100, 200, false);
    let encoded = r#"{"role":"recorder","poll_seq":1,"ready_written":true,"wait_started_us":100,"exit_due_us":200,"logical_end":false}"#;
    assert_eq!(serde_json::to_string(&progress).unwrap(), encoded);
    assert_eq!(serde_json::from_str::<Progress>(encoded).unwrap(), progress);
    assert_eq!(
        serde_json::to_string(&Frame::Step {
            role: Role::Recorder,
            step: step(1, 100),
        })
        .unwrap(),
        r#"{"type":"step","role":"recorder","step":{"round":1,"at_us":100,"stdout_target":0,"runner_input_target":0}}"#
    );
    let mut extended = serde_json::to_value(progress).unwrap();
    extended["exit_code"] = 0.into();
    assert!(serde_json::from_value::<Progress>(extended).is_err());
}

#[test]
fn recorder_role_is_isolated_in_hello_and_nested_progress() {
    for other in OTHER_ROLES {
        for (expected, actual) in [(Role::Recorder, other), (other, Role::Recorder)] {
            let (parent, raw) = UnixStream::pair().unwrap();
            Channel::new(raw, WATCHDOG)
                .unwrap()
                .write_frame(&hello(actual), deadline(WATCHDOG).unwrap())
                .unwrap();
            let result =
                ParentPeer::from_stream(parent, expected, WATCHDOG, deadline(WATCHDOG).unwrap());
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
        }
        assert!(
            validate_progress(other, None, step(1, 100), recorder(1, 100, 200, false)).is_err()
        );
        assert!(
            validate_progress(Role::Recorder, None, step(1, 100), other_progress(other)).is_err()
        );
    }
    let (parent, raw) = UnixStream::pair().unwrap();
    Channel::new(raw, WATCHDOG)
        .unwrap()
        .write_frame(&hello(Role::Recorder), deadline(WATCHDOG).unwrap())
        .unwrap();
    assert!(
        ParentPeer::from_stream(
            parent,
            Role::Recorder,
            WATCHDOG,
            deadline(WATCHDOG).unwrap()
        )
        .is_ok()
    );
}

#[test]
fn recorder_and_other_participants_reject_each_others_outer_steps() {
    for other in OTHER_ROLES {
        for (expected, actual) in [(Role::Recorder, other), (other, Role::Recorder)] {
            let (raw, participant) = UnixStream::pair().unwrap();
            let mut raw = Channel::new(raw, WATCHDOG).unwrap();
            raw.write_frame(
                &Frame::Step {
                    role: actual,
                    step: step(1, 100),
                },
                deadline(WATCHDOG).unwrap(),
            )
            .unwrap();
            let error = Participant::from_stream(participant, expected, WATCHDOG).unwrap_err();
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
fn recorder_requires_zero_input_and_signal_targets_and_representable_nanoseconds() {
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
    ] {
        assert!(validate_step(Role::Recorder, None, invalid_step).is_err());
        assert!(
            validate_progress(
                Role::Recorder,
                None,
                invalid_step,
                recorder(1, 100, 200, false)
            )
            .is_err()
        );
    }
    let last_us = u64::MAX / 1000;
    assert!(validate_step(Role::Recorder, None, step(1, last_us)).is_ok());
    let error = validate_step(Role::Recorder, None, step(1, last_us + 1)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("microseconds-to-nanoseconds overflow")
    );
    for other in OTHER_ROLES {
        assert!(validate_step(other, None, step(1, u64::MAX)).is_ok());
    }
}

#[test]
fn first_recorder_ack_requires_ready_and_the_held_post_write_start() {
    assert!(
        validate_progress(
            Role::Recorder,
            None,
            step(1, 100),
            recorder(1, 100, 200, false)
        )
        .is_ok()
    );
    let mut not_ready = recorder(1, 100, 200, false);
    if let Progress::Recorder { ready_written, .. } = &mut not_ready {
        *ready_written = false;
    }
    for progress in [
        not_ready,
        recorder(1, 99, 200, false),
        recorder(1, 101, 200, false),
        recorder(1, 100, 99, true),
    ] {
        assert!(validate_progress(Role::Recorder, None, step(1, 100), progress).is_err());
    }
}

#[test]
fn recorder_ready_start_and_due_never_change_after_the_first_write() {
    let previous = Some(recorder(1, 100, 200, false));
    let next = recorder(2, 100, 200, false);
    assert!(validate_progress(Role::Recorder, previous, step(2, 150), next).is_ok());
    let mut not_ready = next;
    if let Progress::Recorder { ready_written, .. } = &mut not_ready {
        *ready_written = false;
    }
    for progress in [
        not_ready,
        recorder(2, 99, 200, false),
        recorder(2, 101, 200, false),
        recorder(2, 100, 199, false),
        recorder(2, 100, 201, false),
    ] {
        assert!(validate_progress(Role::Recorder, previous, step(2, 150), progress).is_err());
    }
}

#[test]
fn recorder_terminal_is_required_exactly_at_or_after_its_due_time() {
    let previous = Some(recorder(1, 100, 200, false));
    for at_us in [100, 199, 200, 201] {
        for ended in [false, true] {
            assert_eq!(
                validate_progress(
                    Role::Recorder,
                    previous,
                    step(2, at_us),
                    recorder(2, 100, 200, ended),
                )
                .is_ok(),
                ended == (at_us >= 200),
                "at {at_us}, ended {ended}"
            );
        }
    }
    assert!(
        validate_progress(
            Role::Recorder,
            None,
            step(1, 100),
            recorder(1, 100, 100, true)
        )
        .is_ok()
    );
    assert!(
        validate_progress(
            Role::Recorder,
            None,
            step(1, 100),
            recorder(1, 100, 100, false)
        )
        .is_err()
    );
    assert!(
        validate_progress(
            Role::Recorder,
            Some(recorder(2, 100, 200, true)),
            step(3, 200),
            recorder(3, 100, 200, true),
        )
        .is_err()
    );
}

#[test]
fn recorder_rounds_polls_and_time_cannot_skip_repeat_regress_or_wrap() {
    for round in [0, 1, 3] {
        assert!(validate_step(Role::Recorder, Some(step(1, 100)), step(round, 100)).is_err());
    }
    assert!(validate_step(Role::Recorder, Some(step(1, 100)), step(2, 99)).is_err());
    assert!(validate_step(Role::Recorder, Some(step(u64::MAX, 100)), step(0, 100)).is_err());
    for poll_seq in [0, 1, 3] {
        assert!(
            validate_progress(
                Role::Recorder,
                Some(recorder(1, 100, 200, false)),
                step(2, 100),
                recorder(poll_seq, 100, 200, false),
            )
            .is_err()
        );
    }
    assert!(
        validate_progress(
            Role::Recorder,
            Some(recorder(u64::MAX, 100, 200, false)),
            step(2, 100),
            recorder(0, 100, 200, false),
        )
        .is_err()
    );
}

#[test]
fn same_time_recorder_rounds_do_not_advance_pure_clock_reads() {
    let (parent, child) = UnixStream::pair().unwrap();
    let worker = thread::spawn(move || {
        let mut participant = Participant::from_stream(child, Role::Recorder, WATCHDOG).unwrap();
        let clock = participant.clock();
        for _ in 0..32 {
            assert_eq!(clock.now().as_micros(), 100);
        }
        assert_eq!(
            participant.complete(recorder(1, 100, 200, false)).unwrap(),
            step(2, 100)
        );
        for _ in 0..32 {
            assert_eq!(clock.now().as_micros(), 100);
        }
        assert_eq!(
            participant.complete(recorder(2, 100, 200, false)).unwrap(),
            step(3, 200)
        );
        assert_eq!(clock.now().as_micros(), 200);
        participant.final_ack(recorder(3, 100, 200, true)).unwrap();
        assert!(participant.final_ack(recorder(4, 100, 200, true)).is_err());
    });
    let mut parent = ParentPeer::from_stream(
        parent,
        Role::Recorder,
        WATCHDOG,
        deadline(WATCHDOG).unwrap(),
    )
    .unwrap();
    assert_eq!(
        parent.step(step(1, 100)).unwrap(),
        recorder(1, 100, 200, false)
    );
    assert_eq!(
        parent.step(step(2, 100)).unwrap(),
        recorder(2, 100, 200, false)
    );
    assert_eq!(
        parent.step(step(3, 200)).unwrap(),
        recorder(3, 100, 200, true)
    );
    assert!(parent.step(step(4, 200)).is_err());
    worker.join().unwrap();
}

#[test]
fn overflowing_recorder_steps_never_update_the_participant_clock() {
    let (raw, child) = UnixStream::pair().unwrap();
    let mut raw = Channel::new(raw, WATCHDOG).unwrap();
    for next in [step(1, 100), step(2, u64::MAX / 1000 + 1)] {
        raw.write_frame(
            &Frame::Step {
                role: Role::Recorder,
                step: next,
            },
            deadline(WATCHDOG).unwrap(),
        )
        .unwrap();
    }
    let mut participant = Participant::from_stream(child, Role::Recorder, WATCHDOG).unwrap();
    let clock = participant.clock();
    let error = participant
        .complete(recorder(1, 100, 200, false))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("microseconds-to-nanoseconds overflow")
    );
    assert_eq!(clock.now().as_micros(), 100);
    assert!(participant.closed);
}

#[test]
fn a_missing_recorder_ack_expires_and_closes_instead_of_claiming_exit() {
    let watchdog = Duration::from_millis(20);
    let (parent, raw) = UnixStream::pair().unwrap();
    let mut raw = Channel::new(raw, WATCHDOG).unwrap();
    raw.write_frame(&hello(Role::Recorder), deadline(WATCHDOG).unwrap())
        .unwrap();
    let mut parent = ParentPeer::from_stream(
        parent,
        Role::Recorder,
        watchdog,
        deadline(WATCHDOG).unwrap(),
    )
    .unwrap();
    let error = parent.step(step(1, 100)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(parent.failed);
    assert!(parent.last_progress.is_none());
    assert!(parent.step(step(1, 100)).is_err());
}

#[test]
fn listener_accepts_recorder_once_without_consuming_another_roles_slot() {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = PathBuf::from(format!(
        "/tmp/lve-recorder-wire-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let path = root.join("control.sock");
    let listener = ParentListener::bind_with_watchdog(&path, WATCHDOG).unwrap();
    for role in [Role::Recorder, Role::Runtime] {
        let path = path.clone();
        let worker = thread::spawn(move || {
            let mut participant =
                Participant::connect_with_watchdog(&path, role, WATCHDOG).unwrap();
            let progress = if role == Role::Recorder {
                recorder(1, 100, 100, true)
            } else {
                Progress::Runtime {
                    poll_seq: 1,
                    stdout_frames: 0,
                    room_inputs: 0,
                    logical_end: true,
                }
            };
            participant.final_ack(progress).unwrap();
        });
        let mut peer = listener.accept(role).unwrap();
        assert!(peer.step(step(1, 100)).unwrap().logical_end());
        worker.join().unwrap();
        assert!(
            listener
                .accept(role)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
    }
    drop(listener);
    fs::remove_dir(root).unwrap();
}
