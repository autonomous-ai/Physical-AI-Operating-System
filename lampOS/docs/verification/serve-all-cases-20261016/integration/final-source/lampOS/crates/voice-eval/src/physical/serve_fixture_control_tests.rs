//! Wire checks use socket pairs and reported observations, never process signals.
use super::*;
use std::io::Read;
use std::thread;

const WATCHDOG: Duration = Duration::from_secs(2);
const ROLES: [Role; 4] = [
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

fn runtime(poll_seq: u64) -> Progress {
    Progress::ServeRuntime {
        poll_seq,
        stdout_frames: 3,
        room_consumed: 4,
        room_injected: 2,
        cycle: 1,
        cycle_base_us: 100,
        runtime_us: 50,
        observed_signal: 0,
        logical_end: false,
    }
}

fn relay(poll_seq: u64, stop_signal_sent_us: Option<u64>) -> Progress {
    Progress::ServeRelay {
        poll_seq,
        stdout_frames: 3,
        runner_inputs: 2,
        output_frames: 5,
        stop_signal_sent_us,
        logical_end: false,
    }
}

fn hello(role: Role) -> Frame {
    Frame::Hello {
        version: VERSION,
        role,
    }
}

#[test]
fn finite_frames_keep_their_wire_bytes_and_missing_signal_targets_default_to_false() {
    let encoded = r#"{"type":"step","role":"runtime","step":{"round":1,"at_us":150,"stdout_target":0,"runner_input_target":0}}"#;
    assert_eq!(
        serde_json::to_string(&Frame::Step {
            role: Role::Runtime,
            step: step(1, 150),
        })
        .unwrap(),
        encoded
    );
    assert!(matches!(
        serde_json::from_str::<Frame>(encoded).unwrap(),
        Frame::Step { role: Role::Runtime, step: received } if received == step(1, 150)
    ));
    assert_eq!(
        serde_json::to_string(&hello(Role::Relay)).unwrap(),
        r#"{"type":"hello","version":1,"role":"relay"}"#
    );
    assert_eq!(
        serde_json::to_string(&Progress::Runtime {
            poll_seq: 1,
            stdout_frames: 3,
            room_inputs: 2,
            logical_end: false,
        })
        .unwrap(),
        r#"{"role":"runtime","poll_seq":1,"stdout_frames":3,"room_inputs":2,"logical_end":false}"#
    );
    assert_eq!(
        serde_json::to_string(&Progress::Relay {
            poll_seq: 1,
            stdout_frames: 3,
            runner_inputs: 2,
            output_frames: 5,
            logical_end: false,
        })
        .unwrap(),
        r#"{"role":"relay","poll_seq":1,"stdout_frames":3,"runner_inputs":2,"output_frames":5,"logical_end":false}"#
    );
    let targeted = Step {
        wait_for_sigterm: true,
        ..step(1, 150)
    };
    let encoded = serde_json::to_value(targeted).unwrap();
    assert_eq!(encoded["wait_for_sigterm"], true);
    assert_eq!(serde_json::from_value::<Step>(encoded).unwrap(), targeted);
}

#[test]
fn serve_roles_are_distinct_in_hello_and_nested_progress() {
    let progress = [
        Progress::Runtime {
            poll_seq: 1,
            stdout_frames: 0,
            room_inputs: 0,
            logical_end: false,
        },
        Progress::Relay {
            poll_seq: 1,
            stdout_frames: 0,
            runner_inputs: 0,
            output_frames: 0,
            logical_end: false,
        },
        runtime(1),
        relay(1, None),
    ];
    for expected in ROLES {
        for actual in ROLES {
            let (parent, raw) = UnixStream::pair().unwrap();
            Channel::new(raw, WATCHDOG)
                .unwrap()
                .write_frame(&hello(actual), deadline(WATCHDOG).unwrap())
                .unwrap();
            let result =
                ParentPeer::from_stream(parent, expected, WATCHDOG, deadline(WATCHDOG).unwrap());
            assert_eq!(
                result.is_ok(),
                expected == actual,
                "{expected:?}/{actual:?}"
            );
        }
        for candidate in progress {
            let result = validate_progress(expected, None, step(1, 150), candidate);
            assert_eq!(result.is_ok(), expected == candidate.role());
        }
    }
    assert_eq!(
        serde_json::to_string(&Role::ServeRuntime).unwrap(),
        "\"serve_runtime\""
    );
    assert_eq!(
        serde_json::to_string(&Role::ServeRelay).unwrap(),
        "\"serve_relay\""
    );
}

#[test]
fn serve_participants_reject_wrong_outer_step_roles_before_starting() {
    for expected in [Role::ServeRuntime, Role::ServeRelay] {
        for actual in ROLES.into_iter().filter(|role| *role != expected) {
            let (raw, participant) = UnixStream::pair().unwrap();
            let mut raw = Channel::new(raw, WATCHDOG).unwrap();
            raw.write_frame(
                &Frame::Step {
                    role: actual,
                    step: step(1, 150),
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
fn only_serve_runtime_steps_can_request_the_signal_barrier() {
    let targeted = Step {
        wait_for_sigterm: true,
        ..step(1, 150)
    };
    for role in ROLES {
        assert_eq!(
            validate_step(role, None, targeted).is_ok(),
            role == Role::ServeRuntime
        );
    }
    for role in [Role::Runtime, Role::ServeRuntime] {
        for input in [
            Step {
                stdout_target: 1,
                ..step(1, 150)
            },
            Step {
                runner_input_target: 1,
                ..step(1, 150)
            },
        ] {
            assert!(validate_step(role, None, input).is_err());
        }
    }
    // The signal target is per poll, so a completed observation can be followed
    // by an ordinary step at the same instant.
    assert!(validate_step(Role::ServeRuntime, Some(targeted), step(2, 150)).is_ok());
}

#[test]
fn a_signal_target_requires_the_reported_sigterm_observation() {
    let targeted = Step {
        wait_for_sigterm: true,
        ..step(1, 150)
    };
    for signal in [
        0,
        signal_hook::consts::SIGINT as usize,
        signal_hook::consts::SIGTERM as usize,
    ] {
        let mut progress = runtime(1);
        if let Progress::ServeRuntime {
            observed_signal, ..
        } = &mut progress
        {
            *observed_signal = signal;
        }
        assert_eq!(
            validate_progress(Role::ServeRuntime, None, targeted, progress).is_ok(),
            signal == signal_hook::consts::SIGTERM as usize
        );
    }
    // A successful send receipt belongs to the relay. It cannot satisfy a
    // runtime observation barrier or authorize that barrier for another role.
    assert!(validate_progress(Role::ServeRelay, None, targeted, relay(1, Some(150))).is_err());
}

#[test]
fn observed_signals_cannot_be_cleared_but_signal_numbers_are_not_counters() {
    let mut previous = runtime(1);
    if let Progress::ServeRuntime {
        observed_signal, ..
    } = &mut previous
    {
        *observed_signal = signal_hook::consts::SIGTERM as usize;
    }
    assert!(
        validate_progress(Role::ServeRuntime, Some(previous), step(2, 150), runtime(2)).is_err()
    );
    let mut next = runtime(2);
    if let Progress::ServeRuntime {
        observed_signal, ..
    } = &mut next
    {
        *observed_signal = signal_hook::consts::SIGINT as usize;
    }
    assert!(validate_progress(Role::ServeRuntime, Some(previous), step(2, 150), next).is_ok());
    let mut invalid = runtime(1);
    if let Progress::ServeRuntime {
        observed_signal, ..
    } = &mut invalid
    {
        *observed_signal = usize::MAX;
    }
    assert!(validate_progress(Role::ServeRuntime, None, step(1, 150), invalid).is_err());
}

#[test]
fn runtime_counters_never_regress_and_only_a_new_cycle_can_reset_runtime_time() {
    let previous = runtime(1);
    for counter in 0..6 {
        let mut next = runtime(2);
        if let Progress::ServeRuntime {
            stdout_frames,
            room_consumed,
            room_injected,
            cycle,
            cycle_base_us,
            runtime_us,
            ..
        } = &mut next
        {
            match counter {
                0 => *stdout_frames -= 1,
                1 => *room_consumed -= 1,
                2 => *room_injected -= 1,
                3 => *cycle -= 1,
                4 => {
                    *cycle += 1;
                    *cycle_base_us -= 1;
                }
                5 => *runtime_us -= 1,
                _ => unreachable!(),
            }
        }
        assert!(
            validate_progress(Role::ServeRuntime, Some(previous), step(2, 200), next).is_err(),
            "counter {counter}"
        );
    }
    let mut changed_base = runtime(2);
    if let Progress::ServeRuntime { cycle_base_us, .. } = &mut changed_base {
        *cycle_base_us += 1;
    }
    assert!(
        validate_progress(
            Role::ServeRuntime,
            Some(previous),
            step(2, 200),
            changed_base
        )
        .is_err()
    );

    let mut restarted = runtime(2);
    if let Progress::ServeRuntime {
        cycle,
        cycle_base_us,
        runtime_us,
        ..
    } = &mut restarted
    {
        *cycle = 2;
        *cycle_base_us = 200;
        *runtime_us = 0;
    }
    assert!(validate_progress(Role::ServeRuntime, Some(previous), step(2, 200), restarted).is_ok());
    assert!(
        validate_progress(Role::ServeRuntime, Some(previous), step(2, 150), runtime(2)).is_ok()
    );
}

#[test]
fn muted_consumption_is_valid_but_injection_cannot_exceed_consumption() {
    let previous = runtime(1);
    let mut muted = runtime(2);
    if let Progress::ServeRuntime { room_consumed, .. } = &mut muted {
        *room_consumed += 2;
    }
    assert!(validate_progress(Role::ServeRuntime, Some(previous), step(2, 150), muted).is_ok());
    let mut impossible = runtime(1);
    if let Progress::ServeRuntime { room_injected, .. } = &mut impossible {
        *room_injected = 5;
    }
    assert!(validate_progress(Role::ServeRuntime, None, step(1, 150), impossible).is_err());
}

#[test]
fn relay_input_targets_and_all_output_counters_are_preserved_for_serve() {
    let targeted = Step {
        stdout_target: 3,
        runner_input_target: 2,
        ..step(2, 150)
    };
    let previous = relay(1, None);
    for counter in 0..3 {
        let mut next = relay(2, None);
        if let Progress::ServeRelay {
            stdout_frames,
            runner_inputs,
            output_frames,
            ..
        } = &mut next
        {
            match counter {
                0 => *stdout_frames -= 1,
                1 => *runner_inputs -= 1,
                2 => *output_frames -= 1,
                _ => unreachable!(),
            }
        }
        assert!(validate_progress(Role::ServeRelay, Some(previous), targeted, next).is_err());
    }
    assert!(validate_progress(Role::ServeRelay, Some(previous), targeted, relay(2, None)).is_ok());
    for missing_target in [
        Step {
            stdout_target: 4,
            ..step(1, 150)
        },
        Step {
            runner_input_target: 3,
            ..step(1, 150)
        },
    ] {
        assert!(validate_progress(Role::ServeRelay, None, missing_target, relay(1, None)).is_err());
    }
    for regressing in [
        Step {
            round: 3,
            stdout_target: 2,
            ..targeted
        },
        Step {
            round: 3,
            runner_input_target: 1,
            ..targeted
        },
    ] {
        assert!(validate_step(Role::ServeRelay, Some(targeted), regressing).is_err());
    }
}

#[test]
fn a_successful_send_receipt_is_not_an_exit_claim_and_never_changes() {
    assert!(validate_progress(Role::ServeRelay, None, step(1, 0), relay(1, Some(0))).is_ok());
    assert!(validate_progress(Role::ServeRelay, None, step(1, 150), relay(1, Some(151))).is_err());
    let previous = relay(1, None);
    let sent = relay(2, Some(150));
    assert!(validate_progress(Role::ServeRelay, Some(previous), step(2, 150), sent).is_ok());
    assert!(!sent.logical_end());
    for changed in [None, Some(149), Some(151)] {
        assert!(
            validate_progress(
                Role::ServeRelay,
                Some(sent),
                step(3, 200),
                relay(3, changed)
            )
            .is_err()
        );
    }
    assert!(
        validate_progress(
            Role::ServeRelay,
            Some(sent),
            step(3, 150),
            relay(3, Some(150))
        )
        .is_ok()
    );
}

#[test]
fn serve_relay_receipts_cross_the_wire_without_changing_same_instant_time() {
    let (parent, child) = UnixStream::pair().unwrap();
    let first = Step {
        stdout_target: 3,
        runner_input_target: 2,
        ..step(1, 150)
    };
    let second = Step { round: 2, ..first };
    let mut terminal = relay(2, Some(150));
    if let Progress::ServeRelay { logical_end, .. } = &mut terminal {
        *logical_end = true;
    }
    let worker = thread::spawn(move || {
        let mut participant = Participant::from_stream(child, Role::ServeRelay, WATCHDOG).unwrap();
        assert_eq!(participant.current_step(), first);
        assert_eq!(participant.complete(relay(1, None)).unwrap(), second);
        assert_eq!(participant.clock().now().as_micros(), 150);
        participant.final_ack(terminal).unwrap();
    });
    let mut parent = ParentPeer::from_stream(
        parent,
        Role::ServeRelay,
        WATCHDOG,
        deadline(WATCHDOG).unwrap(),
    )
    .unwrap();
    assert_eq!(parent.step(first).unwrap(), relay(1, None));
    assert_eq!(parent.step(second).unwrap(), terminal);
    assert!(parent.step(Step { round: 3, ..first }).is_err());
    worker.join().unwrap();
}

#[test]
fn serve_rounds_and_completed_poll_sequences_remain_exact() {
    for role in [Role::ServeRuntime, Role::ServeRelay] {
        let first = step(1, 150);
        for bad in [step(1, 150), step(3, 150), step(2, 149)] {
            assert!(validate_step(role, Some(first), bad).is_err());
        }
        assert!(validate_step(role, Some(first), step(2, 150)).is_ok());
        for poll_seq in [0, 2, u64::MAX] {
            let progress = if role == Role::ServeRuntime {
                runtime(poll_seq)
            } else {
                relay(poll_seq, None)
            };
            assert!(validate_progress(role, None, first, progress).is_err());
        }
        let previous = if role == Role::ServeRuntime {
            runtime(1)
        } else {
            relay(1, None)
        };
        for poll_seq in [1, 3] {
            let progress = if role == Role::ServeRuntime {
                runtime(poll_seq)
            } else {
                relay(poll_seq, None)
            };
            assert!(validate_progress(role, Some(previous), step(2, 150), progress).is_err());
        }
    }
}

#[test]
fn same_instant_serve_rounds_preserve_pure_time_and_terminal_ack_still_requires_exit_tracking() {
    let (parent, child) = UnixStream::pair().unwrap();
    let first = step(1, 150);
    let second = Step {
        wait_for_sigterm: true,
        ..step(2, 150)
    };
    let mut terminal = runtime(2);
    if let Progress::ServeRuntime {
        observed_signal,
        logical_end,
        ..
    } = &mut terminal
    {
        // Reported fixture evidence only. This test never sends an OS signal.
        *observed_signal = signal_hook::consts::SIGTERM as usize;
        *logical_end = true;
    }
    let worker = thread::spawn(move || {
        let mut participant =
            Participant::from_stream(child, Role::ServeRuntime, WATCHDOG).unwrap();
        let clock = participant.clock();
        assert_eq!(participant.current_step(), first);
        for _ in 0..1000 {
            assert_eq!(clock.now().as_micros(), 150);
        }
        assert_eq!(participant.complete(runtime(1)).unwrap(), second);
        for _ in 0..1000 {
            assert_eq!(clock.now().as_micros(), 150);
        }
        participant.final_ack(terminal).unwrap();
        // Logical completion returns normally while this thread is still alive.
        assert!(participant.closed);
        clock
    });
    let mut parent = ParentPeer::from_stream(
        parent,
        Role::ServeRuntime,
        WATCHDOG,
        deadline(WATCHDOG).unwrap(),
    )
    .unwrap();
    assert_eq!(parent.step(first).unwrap(), runtime(1));
    assert_eq!(parent.step(second).unwrap(), terminal);
    assert!(parent.step(step(3, 150)).is_err());
    assert_eq!(worker.join().unwrap().now().as_micros(), 150);
}

#[test]
fn forged_serve_progress_closes_the_endpoint_without_retrying() {
    for progress in [relay(1, None), runtime(1)] {
        let (parent, raw) = UnixStream::pair().unwrap();
        let worker = thread::spawn(move || {
            let mut raw = Channel::new(raw, WATCHDOG).unwrap();
            raw.write_frame(&hello(Role::ServeRuntime), deadline(WATCHDOG).unwrap())
                .unwrap();
            raw.read_frame(deadline(WATCHDOG).unwrap()).unwrap();
            raw.write_frame(
                &Frame::Ack {
                    role: Role::ServeRuntime,
                    round: 1,
                    at_us: 150,
                    progress,
                },
                deadline(WATCHDOG).unwrap(),
            )
            .unwrap();
            raw.reader
                .get_ref()
                .set_read_timeout(Some(WATCHDOG))
                .unwrap();
            let mut byte = [0];
            assert_eq!(raw.reader.read(&mut byte).unwrap(), 0);
        });
        let mut parent = ParentPeer::from_stream(
            parent,
            Role::ServeRuntime,
            WATCHDOG,
            deadline(WATCHDOG).unwrap(),
        )
        .unwrap();
        let targeted = Step {
            wait_for_sigterm: true,
            ..step(1, 150)
        };
        assert_eq!(
            parent.step(targeted).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(parent.step(targeted).is_err());
        worker.join().unwrap();
    }
}

struct SocketDirectory(PathBuf);

impl SocketDirectory {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "serve-control-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}

impl Drop for SocketDirectory {
    fn drop(&mut self) {
        fs::remove_dir(&self.0).unwrap();
    }
}

#[test]
fn an_expired_accept_budget_does_not_consume_a_queued_hello_or_reset_itself() {
    let directory = SocketDirectory::new();
    let path = directory.0.join("control.sock");
    let listener = ParentListener::bind_with_watchdog(&path, WATCHDOG).unwrap();
    let mut raw = Channel::new(UnixStream::connect(&path).unwrap(), WATCHDOG).unwrap();
    raw.write_frame(&hello(Role::ServeRuntime), deadline(WATCHDOG).unwrap())
        .unwrap();
    let expired = Instant::now();
    for _ in 0..2 {
        let error = listener
            .accept_until(Role::ServeRuntime, expired)
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(error.to_string().contains("accept peer"));
        assert_eq!(listener.accepted_roles.get(), 0);
    }
    // The queued peer is untouched by both rejected calls.
    let peer = listener
        .accept_until(Role::ServeRuntime, deadline(WATCHDOG).unwrap())
        .unwrap();
    assert_eq!(peer.role, Role::ServeRuntime);
}

#[test]
fn the_listener_tracks_four_roles_independently_and_rejects_duplicates() {
    let directory = SocketDirectory::new();
    let path = directory.0.join("control.sock");
    let listener = ParentListener::bind_with_watchdog(&path, WATCHDOG).unwrap();
    let mut mask = 0;
    for (role, bit) in ROLES.into_iter().zip([1, 2, 4, 8]) {
        let mut raw = Channel::new(UnixStream::connect(&path).unwrap(), WATCHDOG).unwrap();
        raw.write_frame(&hello(role), deadline(WATCHDOG).unwrap())
            .unwrap();
        let peer = listener
            .accept_until(role, deadline(WATCHDOG).unwrap())
            .unwrap();
        assert_eq!(peer.role, role);
        mask |= bit;
        assert_eq!(listener.accepted_roles.get(), mask);
        let error = listener.accept_until(role, Instant::now()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("duplicate"));
    }
    assert_eq!(listener.accepted_roles.get(), 15);
}
