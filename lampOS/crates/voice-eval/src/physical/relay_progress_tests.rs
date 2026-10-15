use super::*;
use std::{
    cell::{Cell, RefCell},
    fs::OpenOptions,
    io::{self, Cursor},
    os::unix::net::UnixStream,
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::Sender,
    },
};

const WATCHDOG: Duration = Duration::from_secs(10);

/// Each request proves that the framer has consumed the preceding chunk. The
/// producer sends chunks explicitly; no elapsed wait stands for reader progress.
struct Chunks {
    source: Receiver<Vec<u8>>,
    requested: Sender<()>,
}

impl Read for Chunks {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.requested
            .send(())
            .map_err(|error| io::Error::other(error.to_string()))?;
        let bytes = self
            .source
            .recv_timeout(WATCHDOG)
            .map_err(|error| io::Error::other(error.to_string()))?;
        assert!(bytes.len() <= buffer.len());
        buffer[..bytes.len()].copy_from_slice(&bytes);
        Ok(bytes.len())
    }
}

fn chunks() -> (Chunks, Sender<Vec<u8>>, Receiver<()>) {
    let (send, source) = mpsc::channel();
    let (requested, requests) = mpsc::channel();
    (Chunks { source, requested }, send, requests)
}

#[test]
fn partial_bytes_do_not_satisfy_the_complete_frame_barrier() {
    let (source, send, requests) = chunks();
    let reader = line_reader(source, 32, 1024, "test", true);
    requests.recv_timeout(WATCHDOG).unwrap();
    send.send(b"par".to_vec()).unwrap();
    requests.recv_timeout(WATCHDOG).unwrap();
    assert_eq!(
        reader
            .enqueued
            .as_ref()
            .unwrap()
            .state
            .lock()
            .unwrap()
            .frames,
        0
    );
    assert!(matches!(
        reader.receiver.try_recv(),
        Err(TryRecvError::Empty)
    ));

    send.send(b"tial\nsecond\n".to_vec()).unwrap();
    requests.recv_timeout(WATCHDOG).unwrap();
    reader
        .wait_until(2, 0, Instant::now() + WATCHDOG, "test")
        .unwrap();
    for expected in ["partial", "second"] {
        let frame = reader.receiver.recv_timeout(WATCHDOG).unwrap();
        assert!(frame.complete);
        assert_eq!(frame.item.unwrap(), expected);
    }
    send.send(Vec::new()).unwrap();
}

#[test]
fn invalid_complete_frames_count_but_truncated_eof_does_not() {
    let reader = line_reader(
        Cursor::new(b"yes\nlonger\n\xff\ncut"),
        3,
        1024,
        "test",
        true,
    );
    reader
        .wait_until(3, 0, Instant::now() + WATCHDOG, "test")
        .unwrap();
    let frames: Vec<_> = (0..4)
        .map(|_| reader.receiver.recv_timeout(WATCHDOG).unwrap())
        .collect();
    assert_eq!(
        frames
            .iter()
            .map(|frame| frame.complete)
            .collect::<Vec<_>>(),
        [true, true, true, false]
    );
    assert_eq!(frames[0].item.as_ref().unwrap(), "yes");
    assert!(
        frames[1]
            .item
            .as_ref()
            .unwrap_err()
            .contains("exceeds the 3-byte limit")
    );
    assert!(
        frames[2]
            .item
            .as_ref()
            .unwrap_err()
            .contains("not valid UTF-8")
    );
    assert!(
        frames[3]
            .item
            .as_ref()
            .unwrap_err()
            .contains("without a newline")
    );
    let error = reader
        .wait_until(4, 3, Instant::now() + WATCHDOG, "test")
        .unwrap_err();
    assert!(error.to_string().contains("ended after 3 complete frames"));
}

#[test]
fn failed_enqueue_never_announces_a_frame() {
    let (source, send, requests) = chunks();
    let reader = line_reader(source, 32, 1024, "test", true);
    let progress = reader.enqueued.as_ref().unwrap().clone();
    requests.recv_timeout(WATCHDOG).unwrap();
    drop(reader);
    send.send(b"undelivered\n".to_vec()).unwrap();
    let error = progress
        .wait_until(1, Instant::now() + WATCHDOG, "test")
        .unwrap_err();
    assert!(error.to_string().contains("ended after 0 complete frames"));
}

#[test]
fn targets_cannot_wait_for_more_than_the_bounded_queue_can_hold() {
    let reader = line_reader(Cursor::new(b"x\n".repeat(257)), 32, 1024, "test", true);
    reader
        .wait_until(256, 0, Instant::now() + WATCHDOG, "test")
        .unwrap();
    let error = reader
        .wait_until(257, 0, Instant::now() + WATCHDOG, "test")
        .unwrap_err();
    assert!(error.to_string().contains("exceeds the 256-frame queue"));
    let first = reader.receiver.recv_timeout(WATCHDOG).unwrap();
    assert_eq!(first.item.unwrap(), "x");
    reader
        .wait_until(257, 1, Instant::now() + WATCHDOG, "test")
        .unwrap();
}

#[test]
fn enqueue_wait_has_a_host_deadline_even_without_logical_clock_reads() {
    let progress = EnqueuedFrames::default();
    let error = progress.wait_until(1, Instant::now(), "test").unwrap_err();
    assert!(error.to_string().contains("at the host reader deadline"));
}

struct Private(PathBuf);
impl Private {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lve-poll-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
}
impl Drop for Private {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug)]
enum Event {
    Clock,
    Flushed(SessionLine),
    Completed(RelayPollProgress),
}

struct Output {
    pending: Vec<u8>,
    events: Rc<RefCell<Vec<Event>>>,
}
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line = serde_json::from_slice(&self.pending[..end])?;
            self.pending.drain(..=end);
            self.events.borrow_mut().push(Event::Flushed(line));
        }
        Ok(())
    }
}

#[test]
fn completed_polls_follow_receipts_and_deadline_checks_before_final_output() {
    let directory = Private::new();
    let fifo = directory.0.join("commands");
    assert!(
        Command::new("mkfifo")
            .args(["-m", "600"])
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let _child_commands = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&fifo)
        .unwrap();
    // The only child is an inert shell blocked on its owned FIFO. It cannot
    // exit normally before the test advances to the hard deadline.
    let child = r#"exec 3< "$1"; printf 'ready:%s\n' "$$"; IFS= read -r command <&3"#;
    let options = SessionOptions {
        runtime: "/bin/sh".into(),
        runtime_prefix: vec![
            "-c".into(),
            child.into(),
            "relay-poll-child".into(),
            fifo.display().to_string(),
        ],
        work: directory.0.join("work"),
        seconds: 1,
        mode: SessionMode::Fixture {
            reply: "/unused/reply.wav".into(),
            sha256: "unused".into(),
        },
        noise_suppression: "off".into(),
        diagnostics: false,
        allowance_seconds: 5,
        ring_channel_ceiling: None,
        runtime_flags: RuntimeFlags::default(),
    };
    let (mut runner, input) = UnixStream::pair().unwrap();
    let at_us = Cell::new(1_000_000);
    let steps = Cell::new(0);
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut out = Output {
        pending: Vec::new(),
        events: Rc::clone(&events),
    };
    let result = run_with_controlled_clock(
        &options,
        input,
        &mut out,
        || {
            events.borrow_mut().push(Event::Clock);
            MonoTime::from_micros(at_us.get())
        },
        || {
            let step = steps.get() + 1;
            assert!(step <= 2, "the terminal poll must not request another step");
            steps.set(step);
            if step == 2 {
                at_us.set(7_000_000);
            }
            writeln!(runner, "{{\"type\":\"ping\",\"seq\":{step}}}")?;
            Ok(RelayPollTargets { stdout_frames: 1, runner_inputs: step })
        },
        |progress| {
            assert_eq!(progress.poll_seq, steps.get());
            assert_eq!(progress.stdout_frames, 1);
            assert_eq!(progress.runner_inputs, steps.get());
            assert_eq!(progress.terminal, steps.get() == 2);
            let mut events = events.borrow_mut();
            // The final hard-deadline read follows the Pong flush. Merely
            // seeing that Pong would therefore be an early acknowledgement.
            assert!(matches!(events.last(), Some(Event::Clock)), "{events:?}");
            assert!(matches!(events.get(events.len() - 2), Some(Event::Flushed(SessionLine::Pong { seq, .. })) if *seq == steps.get()));
            assert!(!events.iter().any(|event| matches!(event, Event::Flushed(SessionLine::SessionEnd { .. }))));
            if progress.terminal {
                let pid = events.iter().find_map(|event| match event {
                    Event::Flushed(SessionLine::Stdout { line, .. }) => line.strip_prefix("ready:").and_then(|pid| pid.parse().ok()),
                    _ => None,
                }).unwrap();
                let pid = rustix::process::Pid::from_raw(pid).unwrap();
                assert!(rustix::process::test_kill_process(pid).is_err(), "terminal deadline poll must kill and reap its child first");
            }
            events.push(Event::Completed(progress));
            Ok(())
        },
    ).unwrap();
    assert_eq!(result, 3);
    let events = events.borrow();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Completed(_)))
            .count(),
        2
    );
    assert!(matches!(
        events.last(),
        Some(Event::Flushed(SessionLine::SessionEnd { killed: true, .. }))
    ));
    let terminal = events
        .iter()
        .position(|event| {
            matches!(
                event,
                Event::Completed(RelayPollProgress { terminal: true, .. })
            )
        })
        .unwrap();
    let end = events
        .iter()
        .position(|event| matches!(event, Event::Flushed(SessionLine::SessionEnd { .. })))
        .unwrap();
    assert!(terminal < end);
}
