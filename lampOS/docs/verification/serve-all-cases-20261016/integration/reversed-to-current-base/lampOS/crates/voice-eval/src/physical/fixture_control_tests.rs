use super::*;
use std::io::Read;
use std::sync::mpsc;
use std::thread;

const WATCHDOG: Duration = Duration::from_secs(2);

fn step(round: u64, at_us: u64) -> Step {
    Step {
        round,
        at_us,
        stdout_target: 0,
        runner_input_target: 0,
    }
}

fn runtime(poll_seq: u64, stdout_frames: u64, room_inputs: u64, logical_end: bool) -> Progress {
    Progress::Runtime {
        poll_seq,
        stdout_frames,
        room_inputs,
        logical_end,
    }
}

fn relay(poll_seq: u64, stdout_frames: u64, runner_inputs: u64, output_frames: u64) -> Progress {
    Progress::Relay {
        poll_seq,
        stdout_frames,
        runner_inputs,
        output_frames,
        logical_end: false,
    }
}

fn hello(role: Role) -> Frame {
    Frame::Hello {
        version: VERSION,
        role,
    }
}

fn pair() -> (Channel, Channel) {
    let (left, right) = UnixStream::pair().unwrap();
    (
        Channel::new(left, WATCHDOG).unwrap(),
        Channel::new(right, WATCHDOG).unwrap(),
    )
}

#[test]
fn a_frame_at_the_total_bound_is_accepted_and_one_byte_more_is_rejected() {
    let (mut reader, mut writer) = pair();
    let mut encoded = serde_json::to_vec(&hello(Role::Runtime)).unwrap();
    encoded.resize(MAX_FRAME_BYTES - 1, b' ');
    encoded.push(b'\n');
    writer.writer.write_all(&encoded).unwrap();
    assert!(matches!(
        reader.read_frame(deadline(WATCHDOG).unwrap()).unwrap(),
        Frame::Hello {
            version: VERSION,
            role: Role::Runtime
        }
    ));

    // No LF and no EOF: reject as soon as there is no room for a terminator.
    writer.writer.write_all(&[b' '; MAX_FRAME_BYTES]).unwrap();
    let error = reader.read_frame(deadline(WATCHDOG).unwrap()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("4096-byte limit"));
}

#[test]
fn a_partial_frame_at_eof_is_not_a_complete_message() {
    let (mut reader, mut writer) = pair();
    writer.writer.write_all(b"{\"type\":\"hello\"").unwrap();
    writer.writer.shutdown(Shutdown::Write).unwrap();
    let error = reader.read_frame(deadline(WATCHDOG).unwrap()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    assert!(error.to_string().contains("truncated frame"));
}

#[test]
fn buffered_complete_frames_are_kept_distinct_and_unknown_fields_are_rejected() {
    let (mut reader, mut writer) = pair();
    writer.writer.write_all(b"{\"type\":\"hello\",\"version\":1,\"role\":\"runtime\"}\n{\"type\":\"hello\",\"version\":1,\"role\":\"relay\"}\n").unwrap();
    assert!(matches!(
        reader.read_frame(deadline(WATCHDOG).unwrap()).unwrap(),
        Frame::Hello {
            role: Role::Runtime,
            ..
        }
    ));
    assert!(matches!(
        reader.read_frame(deadline(WATCHDOG).unwrap()).unwrap(),
        Frame::Hello {
            role: Role::Relay,
            ..
        }
    ));
    writer
        .writer
        .write_all(b"{\"type\":\"hello\",\"version\":1,\"role\":\"runtime\",\"exit\":true}\n")
        .unwrap();
    let error = reader.read_frame(deadline(WATCHDOG).unwrap()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn invalid_utf8_and_missing_fields_are_protocol_errors() {
    for bytes in [
        b"\xff\n".as_slice(),
        b"{\"type\":\"hello\",\"role\":\"runtime\"}\n",
    ] {
        let (mut reader, mut writer) = pair();
        writer.writer.write_all(bytes).unwrap();
        assert_eq!(
            reader
                .read_frame(deadline(WATCHDOG).unwrap())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
}

#[test]
fn outgoing_frames_cannot_cross_the_same_bound() {
    let (mut reader, mut writer) = pair();
    let error = writer
        .write_frame(&"x".repeat(MAX_FRAME_BYTES), deadline(WATCHDOG).unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    // Rejected before any bytes are written: the next complete frame is intact.
    writer
        .write_frame(&hello(Role::Relay), deadline(WATCHDOG).unwrap())
        .unwrap();
    assert!(matches!(
        reader.read_frame(deadline(WATCHDOG).unwrap()).unwrap(),
        Frame::Hello {
            role: Role::Relay,
            ..
        }
    ));
}

#[test]
fn hello_must_match_the_requested_role_and_version() {
    for frame in [
        hello(Role::Relay),
        Frame::Hello {
            version: VERSION + 1,
            role: Role::Runtime,
        },
        Frame::Step {
            role: Role::Runtime,
            step: step(1, 0),
        },
    ] {
        let (parent, raw) = UnixStream::pair().unwrap();
        let mut raw = Channel::new(raw, WATCHDOG).unwrap();
        raw.write_frame(&frame, deadline(WATCHDOG).unwrap())
            .unwrap();
        let error =
            ParentPeer::from_stream(parent, Role::Runtime, WATCHDOG, deadline(WATCHDOG).unwrap())
                .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("Hello"));
    }
}

#[test]
fn a_participant_does_not_start_until_the_first_step_arrives() {
    let (parent, child) = UnixStream::pair().unwrap();
    let (started, observed) = mpsc::channel();
    let worker = thread::spawn(move || {
        let mut participant = Participant::from_stream(child, Role::Runtime, WATCHDOG).unwrap();
        started.send(participant.clock().now().as_micros()).unwrap();
        participant.final_ack(runtime(1, 0, 0, true)).unwrap();
    });
    let mut parent = Channel::new(parent, WATCHDOG).unwrap();
    assert!(matches!(
        parent.read_frame(deadline(WATCHDOG).unwrap()).unwrap(),
        Frame::Hello {
            role: Role::Runtime,
            ..
        }
    ));
    assert_eq!(observed.try_recv(), Err(mpsc::TryRecvError::Empty));
    parent
        .write_frame(
            &Frame::Step {
                role: Role::Runtime,
                step: step(1, 73),
            },
            deadline(WATCHDOG).unwrap(),
        )
        .unwrap();
    assert_eq!(observed.recv_timeout(WATCHDOG).unwrap(), 73);
    assert!(matches!(
        parent.read_frame(deadline(WATCHDOG).unwrap()).unwrap(),
        Frame::Ack {
            round: 1,
            at_us: 73,
            ..
        }
    ));
    worker.join().unwrap();
}

#[test]
fn repeated_clock_reads_and_new_rounds_at_the_same_time_never_advance_time() {
    let (parent, child) = UnixStream::pair().unwrap();
    let (clock_tx, clock_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let mut participant = Participant::from_stream(child, Role::Runtime, WATCHDOG).unwrap();
        let clock = participant.clock();
        clock_tx.send(clock.clone()).unwrap();
        assert_eq!(participant.current_step(), step(1, 900));
        for _ in 0..1000 {
            assert_eq!(clock.now(), MonoTime::from_micros(900));
        }
        assert_eq!(
            participant.complete(runtime(1, 1, 0, false)).unwrap(),
            step(2, 900)
        );
        assert_eq!(clock.now(), MonoTime::from_micros(900));
        assert_eq!(
            participant.complete(runtime(2, 2, 1, false)).unwrap(),
            step(3, 902)
        );
        assert_eq!(clock.now(), MonoTime::from_micros(902));
        participant.final_ack(runtime(3, 2, 1, true)).unwrap();
        assert!(participant.final_ack(runtime(4, 2, 1, true)).is_err());
        clock
    });
    let mut parent =
        ParentPeer::from_stream(parent, Role::Runtime, WATCHDOG, deadline(WATCHDOG).unwrap())
            .unwrap();
    assert_eq!(parent.step(step(1, 900)).unwrap(), runtime(1, 1, 0, false));
    let clock = clock_rx.recv_timeout(WATCHDOG).unwrap();
    for _ in 0..1000 {
        assert_eq!(clock.now(), MonoTime::from_micros(900));
    }
    assert_eq!(parent.step(step(2, 900)).unwrap(), runtime(2, 2, 1, false));
    assert_eq!(clock.now(), MonoTime::from_micros(900));
    assert_eq!(parent.step(step(3, 902)).unwrap(), runtime(3, 2, 1, true));
    assert!(parent.step(step(4, 902)).is_err());
    assert_eq!(worker.join().unwrap().now(), MonoTime::from_micros(902));
}

#[test]
fn duplicate_skipped_and_regressing_rounds_or_time_are_rejected() {
    let first = step(1, 50);
    assert!(validate_step(Role::Runtime, None, step(0, 50)).is_err());
    assert!(validate_step(Role::Runtime, None, step(2, 50)).is_err());
    for bad in [step(1, 50), step(0, 50), step(3, 50), step(2, 49)] {
        assert!(validate_step(Role::Runtime, Some(first), bad).is_err());
    }
    assert!(validate_step(Role::Runtime, Some(first), step(2, 50)).is_ok());
    assert!(validate_step(Role::Runtime, Some(step(u64::MAX, 50)), step(0, 50)).is_err());
    let targeted = Step {
        stdout_target: 3,
        runner_input_target: 2,
        ..first
    };
    assert!(validate_step(Role::Runtime, None, targeted).is_err());
    assert!(
        validate_step(
            Role::Relay,
            Some(targeted),
            Step {
                round: 2,
                stdout_target: 2,
                ..targeted
            }
        )
        .is_err()
    );
    assert!(
        validate_step(
            Role::Relay,
            Some(targeted),
            Step {
                round: 2,
                runner_input_target: 1,
                ..targeted
            }
        )
        .is_err()
    );
}

#[test]
fn a_wrong_role_step_does_not_reach_the_participant_engine() {
    let (parent, child) = UnixStream::pair().unwrap();
    let worker = thread::spawn(move || {
        Participant::from_stream(child, Role::Runtime, WATCHDOG).unwrap_err()
    });
    let mut parent = Channel::new(parent, WATCHDOG).unwrap();
    parent.read_frame(deadline(WATCHDOG).unwrap()).unwrap();
    parent
        .write_frame(
            &Frame::Step {
                role: Role::Relay,
                step: step(1, 0),
            },
            deadline(WATCHDOG).unwrap(),
        )
        .unwrap();
    assert!(
        worker
            .join()
            .unwrap()
            .to_string()
            .contains("expected Runtime Step")
    );
}

#[test]
fn acknowledgement_must_match_role_round_time_and_complete_progress() {
    let bad = [
        Frame::Ack {
            role: Role::Relay,
            round: 1,
            at_us: 50,
            progress: runtime(1, 0, 0, false),
        },
        Frame::Ack {
            role: Role::Runtime,
            round: 2,
            at_us: 50,
            progress: runtime(1, 0, 0, false),
        },
        Frame::Ack {
            role: Role::Runtime,
            round: 1,
            at_us: 49,
            progress: runtime(1, 0, 0, false),
        },
        Frame::Ack {
            role: Role::Runtime,
            round: 1,
            at_us: 50,
            progress: relay(1, 0, 0, 0),
        },
        Frame::Ack {
            role: Role::Runtime,
            round: 1,
            at_us: 50,
            progress: runtime(0, 0, 0, false),
        },
        hello(Role::Runtime),
    ];
    for ack in bad {
        let (parent, raw) = UnixStream::pair().unwrap();
        let worker = thread::spawn(move || {
            let mut raw = Channel::new(raw, WATCHDOG).unwrap();
            raw.write_frame(&hello(Role::Runtime), deadline(WATCHDOG).unwrap())
                .unwrap();
            raw.read_frame(deadline(WATCHDOG).unwrap()).unwrap();
            raw.write_frame(&ack, deadline(WATCHDOG).unwrap()).unwrap();
            // A failed endpoint closes immediately, releasing the other side.
            let mut byte = [0];
            assert_eq!(raw.reader.read(&mut byte).unwrap(), 0);
        });
        let mut parent =
            ParentPeer::from_stream(parent, Role::Runtime, WATCHDOG, deadline(WATCHDOG).unwrap())
                .unwrap();
        assert_eq!(
            parent.step(step(1, 50)).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(parent.step(step(1, 50)).is_err());
        worker.join().unwrap();
    }
}

#[test]
fn progress_counters_cannot_regress_or_claim_unconsumed_relay_targets() {
    let current = Step {
        stdout_target: 4,
        runner_input_target: 3,
        ..step(2, 20)
    };
    let old = relay(1, 4, 3, 7);
    for bad in [
        relay(2, 3, 3, 7),
        relay(2, 4, 2, 7),
        relay(2, 4, 3, 6),
        relay(1, 4, 3, 7),
        relay(3, 4, 3, 7),
    ] {
        assert!(validate_progress(Role::Relay, Some(old), current, bad).is_err());
    }
    assert!(validate_progress(Role::Relay, Some(old), current, relay(2, 4, 3, 7)).is_ok());
    assert!(validate_progress(Role::Relay, None, current, relay(1, 3, 3, 9)).is_err());
    assert!(validate_progress(Role::Relay, None, current, relay(1, 4, 2, 9)).is_err());
    let old = runtime(1, 4, 3, false);
    assert!(
        validate_progress(
            Role::Runtime,
            Some(old),
            step(2, 20),
            runtime(2, 3, 3, false)
        )
        .is_err()
    );
    assert!(
        validate_progress(
            Role::Runtime,
            Some(old),
            step(2, 20),
            runtime(2, 4, 2, false)
        )
        .is_err()
    );
    assert!(
        validate_progress(
            Role::Runtime,
            Some(runtime(1, 4, 3, true)),
            step(2, 20),
            runtime(2, 4, 3, false)
        )
        .is_err()
    );
}

#[test]
fn a_missing_ack_fails_under_the_host_watchdog_and_closes_the_peer() {
    let watchdog = Duration::from_millis(40);
    let (parent, raw) = UnixStream::pair().unwrap();
    let worker = thread::spawn(move || {
        let mut raw = Channel::new(raw, WATCHDOG).unwrap();
        raw.write_frame(&hello(Role::Runtime), deadline(WATCHDOG).unwrap())
            .unwrap();
        // The short parent budget may expire before its first write. A
        // complete Step must match exactly; EOF with no bytes is also valid.
        match raw.read_frame(deadline(WATCHDOG).unwrap()) {
            Ok(Frame::Step {
                role: Role::Runtime,
                step: received,
            }) => assert_eq!(received, step(1, 100)),
            Err(error)
                if error.kind() == io::ErrorKind::UnexpectedEof
                    && error.to_string().contains("missing frame: peer closed") => {}
            other => panic!("expected the exact Step or closure before its write: {other:?}"),
        }
        let mut byte = [0];
        raw.reader
            .get_ref()
            .set_read_timeout(Some(WATCHDOG))
            .unwrap();
        assert_eq!(raw.reader.read(&mut byte).unwrap(), 0);
    });
    let mut parent =
        ParentPeer::from_stream(parent, Role::Runtime, watchdog, deadline(WATCHDOG).unwrap())
            .unwrap();
    let error = parent.step(step(1, 100)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(error.to_string().contains("host watchdog"));
    assert!(parent.step(step(1, 100)).is_err());
    worker.join().unwrap();
}

#[test]
fn a_missing_first_step_cannot_leave_a_participant_waiting_forever() {
    let (parent, child) = UnixStream::pair().unwrap();
    let watchdog = Duration::from_millis(40);
    let worker = thread::spawn(move || {
        let error = Participant::from_stream(child, Role::Runtime, watchdog).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    });
    let mut parent = Channel::new(parent, WATCHDOG).unwrap();
    // The participant can exhaust its budget before sending Hello. This
    // remains a timeout and closes both socket handles; it is not a bad Hello.
    match parent.read_frame(deadline(WATCHDOG).unwrap()) {
        Ok(Frame::Hello {
            version: VERSION,
            role: Role::Runtime,
        }) => {}
        Err(error)
            if error.kind() == io::ErrorKind::UnexpectedEof
                && error.to_string().contains("missing frame: peer closed") => {}
        other => panic!("expected the exact Hello or closure before its write: {other:?}"),
    }
    let mut byte = [0];
    parent
        .reader
        .get_ref()
        .set_read_timeout(Some(WATCHDOG))
        .unwrap();
    assert_eq!(parent.reader.read(&mut byte).unwrap(), 0);
    worker.join().unwrap();
}

#[test]
fn a_peer_that_stops_reading_cannot_block_a_write_forever() {
    let (mut sender, _receiver) = UnixStream::pair().unwrap();
    sender.set_nonblocking(true).unwrap();
    let block = [b'x'; 4096];
    let mut filled = 0;
    loop {
        match sender.write(&block) {
            Ok(count) => {
                assert_ne!(count, 0);
                filled += count;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => panic!("fill socket: {error}"),
        }
        assert!(
            filled < 16 * 1024 * 1024,
            "fixture socket did not reach its bound"
        );
    }
    // A large nonblocking write may encounter a boundary with a few bytes
    // left. Fill those too so even the short Hello must encounter pressure.
    loop {
        match sender.write(b"x") {
            Ok(count) => {
                assert_eq!(count, 1);
                filled += count;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => panic!("fill socket tail: {error}"),
        }
        assert!(
            filled < 16 * 1024 * 1024,
            "fixture socket did not reach its bound"
        );
    }
    let mut sender = Channel::new(sender, WATCHDOG).unwrap();
    let error = sender
        .write_frame(
            &hello(Role::Runtime),
            deadline(Duration::from_millis(40)).unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
}

#[test]
fn watchdogs_must_be_positive_and_an_expired_deadline_is_not_replaced() {
    assert_eq!(
        deadline(Duration::ZERO).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    let (mut reader, mut writer) = pair();
    writer
        .write_frame(&hello(Role::Runtime), deadline(WATCHDOG).unwrap())
        .unwrap();
    let error = reader.read_frame(Instant::now()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
}

#[test]
fn invalid_followup_steps_close_the_participant_without_changing_its_clock() {
    for bad in [
        Frame::Step {
            role: Role::Runtime,
            step: step(1, 50),
        },
        Frame::Step {
            role: Role::Runtime,
            step: step(3, 50),
        },
        Frame::Step {
            role: Role::Runtime,
            step: step(2, 49),
        },
        Frame::Step {
            role: Role::Relay,
            step: step(2, 50),
        },
    ] {
        let (parent, child) = UnixStream::pair().unwrap();
        let worker = thread::spawn(move || {
            let mut participant = Participant::from_stream(child, Role::Runtime, WATCHDOG).unwrap();
            let clock = participant.clock();
            let error = participant.complete(runtime(1, 1, 0, false)).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert_eq!(clock.now(), MonoTime::from_micros(50));
            assert!(participant.complete(runtime(2, 1, 0, false)).is_err());
        });
        let mut parent = Channel::new(parent, WATCHDOG).unwrap();
        parent.read_frame(deadline(WATCHDOG).unwrap()).unwrap();
        parent
            .write_frame(
                &Frame::Step {
                    role: Role::Runtime,
                    step: step(1, 50),
                },
                deadline(WATCHDOG).unwrap(),
            )
            .unwrap();
        assert!(matches!(
            parent.read_frame(deadline(WATCHDOG).unwrap()).unwrap(),
            Frame::Ack {
                round: 1,
                at_us: 50,
                ..
            }
        ));
        parent
            .write_frame(&bad, deadline(WATCHDOG).unwrap())
            .unwrap();
        let mut byte = [0];
        parent
            .reader
            .get_ref()
            .set_read_timeout(Some(WATCHDOG))
            .unwrap();
        assert_eq!(parent.reader.read(&mut byte).unwrap(), 0);
        worker.join().unwrap();
    }
}

#[test]
fn terminal_acknowledgement_is_distinct_from_a_continuing_poll() {
    for terminal_on_continue in [false, true] {
        let (parent, child) = UnixStream::pair().unwrap();
        let worker = thread::spawn(move || {
            let mut participant = Participant::from_stream(child, Role::Runtime, WATCHDOG).unwrap();
            let result = if terminal_on_continue {
                participant.complete(runtime(1, 0, 0, true)).map(|_| ())
            } else {
                participant.final_ack(runtime(1, 0, 0, false))
            };
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
        });
        let mut parent =
            ParentPeer::from_stream(parent, Role::Runtime, WATCHDOG, deadline(WATCHDOG).unwrap())
                .unwrap();
        assert_eq!(
            parent.step(step(1, 50)).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        worker.join().unwrap();
    }
}

#[test]
fn poll_sequence_overflow_cannot_wrap_to_an_initial_acknowledgement() {
    let error = validate_progress(
        Role::Runtime,
        Some(runtime(u64::MAX, 1, 1, false)),
        step(2, 0),
        runtime(0, 1, 1, false),
    )
    .unwrap_err();
    assert!(error.to_string().contains("expected completed poll None"));
}

#[test]
fn an_expired_caller_deadline_closes_the_peer_without_sending_a_step() {
    let (parent, raw) = UnixStream::pair().unwrap();
    let mut raw = Channel::new(raw, WATCHDOG).unwrap();
    raw.write_frame(&hello(Role::Runtime), deadline(WATCHDOG).unwrap())
        .unwrap();
    let mut parent =
        ParentPeer::from_stream(parent, Role::Runtime, WATCHDOG, deadline(WATCHDOG).unwrap())
            .unwrap();
    let error = parent.step_until(step(1, 70), Instant::now()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(parent.step(step(1, 70)).is_err());
    let mut byte = [0];
    raw.reader
        .get_ref()
        .set_read_timeout(Some(WATCHDOG))
        .unwrap();
    assert_eq!(raw.reader.read(&mut byte).unwrap(), 0);
}

#[test]
fn a_later_caller_deadline_cannot_extend_the_endpoint_watchdog() {
    // Explicit instants prove budget selection without depending on how soon
    // this test receives CPU time. Actual socket timeout/closure has its own
    // tests above, including an already-expired caller deadline.
    let origin = Instant::now();
    let endpoint = origin + Duration::from_secs(10);
    let earlier = origin + Duration::from_secs(3);
    let later = origin + Duration::from_secs(20);
    assert_eq!(selected_deadline(endpoint, None), endpoint);
    assert_eq!(selected_deadline(endpoint, Some(later)), endpoint);
    assert_eq!(selected_deadline(endpoint, Some(earlier)), earlier);
    assert_eq!(selected_deadline(endpoint, Some(origin)), origin);
}
