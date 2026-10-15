//! A final frame remains valid after the producer closes. These tests also run
//! against the finite protocol: no serve roles or added Step fields are needed.
use super::*;
use std::thread;

const WATCHDOG: Duration = Duration::from_secs(2);

fn hello() -> Frame {
    Frame::Hello {
        version: VERSION,
        role: Role::Runtime,
    }
}

fn terminal() -> Progress {
    Progress::Runtime {
        poll_seq: 1,
        stdout_frames: 3,
        room_inputs: 2,
        logical_end: true,
    }
}

fn ack() -> Frame {
    Frame::Ack {
        role: Role::Runtime,
        round: 1,
        at_us: 90,
        progress: terminal(),
    }
}

fn raw_frame(writer: &mut UnixStream, frame: &Frame) {
    let mut bytes = serde_json::to_vec(frame).unwrap();
    bytes.push(b'\n');
    writer.write_all(&bytes).unwrap();
}

#[test]
fn a_queued_hello_is_read_after_the_peer_closed_before_channel_creation() {
    let (reader, mut writer) = UnixStream::pair().unwrap();
    raw_frame(&mut writer, &hello());
    drop(writer);
    let parent =
        ParentPeer::from_stream(reader, Role::Runtime, WATCHDOG, deadline(WATCHDOG).unwrap())
            .unwrap();
    assert_eq!(parent.role, Role::Runtime);
}

#[test]
fn a_final_ack_is_read_after_the_participant_has_actually_closed() {
    let (parent, participant) = UnixStream::pair().unwrap();
    let worker = thread::spawn(move || {
        let mut participant =
            Participant::from_stream(participant, Role::Runtime, WATCHDOG).unwrap();
        assert_eq!(participant.current_step().round, 1);
        assert_eq!(participant.clock().now().as_micros(), 90);
        participant.final_ack(terminal()).unwrap();
    });
    let mut parent = Channel::new(parent, WATCHDOG).unwrap();
    assert!(matches!(
        parent.read_frame(deadline(WATCHDOG).unwrap()).unwrap(),
        Frame::Hello {
            version: VERSION,
            role: Role::Runtime
        }
    ));
    // Old finite Step JSON remains accepted when serve adds optional fields.
    let first: Step = serde_json::from_value(serde_json::json!({
        "round": 1, "at_us": 90, "stdout_target": 0, "runner_input_target": 0
    }))
    .unwrap();
    parent
        .write_frame(
            &Frame::Step {
                role: Role::Runtime,
                step: first,
            },
            deadline(WATCHDOG).unwrap(),
        )
        .unwrap();
    // Join before reading: the successful final write and peer close are both
    // complete. Keeping the producer alive would hide the Darwin regression.
    worker.join().unwrap();
    assert!(
        matches!(parent.read_frame(deadline(WATCHDOG).unwrap()).unwrap(),
        Frame::Ack { role: Role::Runtime, round: 1, at_us: 90, progress } if progress == terminal())
    );
    let error = parent.read_frame(deadline(WATCHDOG).unwrap()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    assert!(error.to_string().contains("missing frame: peer closed"));
}

#[test]
fn an_expired_deadline_leaves_a_buffered_final_frame_intact() {
    let (reader, mut writer) = UnixStream::pair().unwrap();
    raw_frame(&mut writer, &hello());
    raw_frame(&mut writer, &ack());
    drop(writer);
    let mut reader = Channel::new(reader, WATCHDOG).unwrap();
    assert!(matches!(
        reader.read_frame(deadline(WATCHDOG).unwrap()).unwrap(),
        Frame::Hello { .. }
    ));
    // Ensure unread bytes are in BufReader, whether the first socket read took
    // one frame or both. No assumption about kernel read chunking is needed.
    let buffered = reader.reader.fill_buf().unwrap().to_vec();
    assert!(!buffered.is_empty());
    reader
        .reader
        .get_ref()
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    let error = reader.read_frame(Instant::now()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert_eq!(reader.reader.buffer(), buffered.as_slice());
    assert!(
        matches!(reader.read_frame(deadline(WATCHDOG).unwrap()).unwrap(),
        Frame::Ack { progress, .. } if progress == terminal())
    );
}

#[test]
fn a_closed_peer_with_a_partial_final_frame_is_still_truncated() {
    let (reader, mut writer) = UnixStream::pair().unwrap();
    raw_frame(&mut writer, &hello());
    writer.write_all(b"{\"type\":\"ack\"").unwrap();
    drop(writer);
    let mut reader = Channel::new(reader, WATCHDOG).unwrap();
    assert!(matches!(
        reader.read_frame(deadline(WATCHDOG).unwrap()).unwrap(),
        Frame::Hello { .. }
    ));
    let error = reader.read_frame(deadline(WATCHDOG).unwrap()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    assert!(
        error
            .to_string()
            .contains("truncated frame: peer closed before LF")
    );
}

#[test]
fn poll_hangup_allows_remaining_bytes_and_then_real_eof() {
    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    writer.write_all(b"tail").unwrap();
    drop(writer);
    reader.set_nonblocking(true).unwrap();
    let end = deadline(WATCHDOG).unwrap();
    wait_ready(&reader, PollFlags::IN, end, "read frame").unwrap();
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"tail");
    wait_ready(&reader, PollFlags::IN, end, "read frame").unwrap();
    assert_eq!(reader.read(&mut bytes).unwrap(), 0);
}

#[test]
fn raw_reader_timeout_configuration_is_local_even_after_peer_close() {
    let (reader, writer) = UnixStream::pair().unwrap();
    drop(writer);
    let mut reader = Channel::new(reader, WATCHDOG).unwrap();
    reader
        .reader
        .get_ref()
        .set_read_timeout(Some(WATCHDOG))
        .unwrap();
    let mut byte = [0];
    assert_eq!(reader.reader.read(&mut byte).unwrap(), 0);
    assert_eq!(
        reader
            .reader
            .get_ref()
            .set_read_timeout(None)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        reader
            .reader
            .get_ref()
            .set_read_timeout(Some(Duration::ZERO))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn raw_reader_without_a_frame_still_has_a_host_watchdog() {
    let (reader, _live_peer) = UnixStream::pair().unwrap();
    let mut reader = Channel::new(reader, Duration::from_millis(40)).unwrap();
    let mut byte = [0];
    let error = reader.reader.read(&mut byte).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(error.to_string().contains("host watchdog"));
}

#[test]
fn a_closed_writer_reports_the_write_failure_without_setting_socket_timeouts() {
    let (writer, reader) = UnixStream::pair().unwrap();
    drop(reader);
    let mut writer = Channel::new(writer, WATCHDOG).unwrap();
    let error = writer
        .write_frame(&hello(), deadline(WATCHDOG).unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
}

#[test]
fn an_expired_write_budget_emits_no_frame_bytes() {
    let (writer, mut reader) = UnixStream::pair().unwrap();
    let mut writer = Channel::new(writer, WATCHDOG).unwrap();
    let error = writer.write_frame(&hello(), Instant::now()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    reader.set_nonblocking(true).unwrap();
    let mut byte = [0];
    assert_eq!(
        reader.read(&mut byte).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    writer
        .write_frame(&hello(), deadline(WATCHDOG).unwrap())
        .unwrap();
    let mut reader = Channel::new(reader, WATCHDOG).unwrap();
    assert!(matches!(
        reader.read_frame(deadline(WATCHDOG).unwrap()).unwrap(),
        Frame::Hello { .. }
    ));
}

#[test]
fn poll_timeouts_preserve_small_budgets_and_cap_darwins_millisecond_range() {
    assert_eq!(
        poll_timeout(Duration::from_nanos(1)).unwrap(),
        Timespec {
            tv_sec: 0,
            tv_nsec: 1
        }
    );
    let maximum = Timespec {
        tv_sec: 2_147_483,
        tv_nsec: 647_000_000,
    };
    assert_eq!(
        poll_timeout(Duration::from_millis(i32::MAX as u64)).unwrap(),
        maximum
    );
    assert_eq!(poll_timeout(Duration::MAX).unwrap(), maximum);
}
