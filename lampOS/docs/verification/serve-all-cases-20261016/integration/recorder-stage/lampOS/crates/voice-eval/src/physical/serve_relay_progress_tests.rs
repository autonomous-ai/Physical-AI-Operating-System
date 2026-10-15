use super::*;
use std::{
    cell::{Cell, RefCell},
    fs::OpenOptions,
    io,
    os::unix::net::UnixStream,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

struct Private(PathBuf);

impl Private {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lve-serve-poll-{}-{}",
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
fn successful_signal_receipt_survives_repeated_stop_and_exact_grace_kill() {
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
    // Only this owned, inert child ignores TERM. Its ready line proves that
    // the disposition is installed before the relay sends the real signal.
    let child = r#"trap '' TERM; exec 3< "$1"; printf 'ready:%s\n' "$$"; IFS= read -r command <&3"#;
    let options = SessionOptions {
        runtime: "/bin/sh".into(),
        runtime_prefix: vec![
            "-c".into(),
            child.into(),
            "serve-relay-poll-child".into(),
            fifo.display().to_string(),
        ],
        work: directory.0.join("work"),
        seconds: 1,
        mode: SessionMode::Serve {
            provider_config: "/unused/provider.json".into(),
        },
        noise_suppression: "off".into(),
        diagnostics: false,
        allowance_seconds: 5,
        ring_channel_ceiling: None,
        runtime_flags: RuntimeFlags::default(),
    };
    let (mut runner, input) = UnixStream::pair().unwrap();
    let signal_at = 1_002_000;
    let poll_times = [
        1_000_000,
        signal_at,
        signal_at + SERVE_STOP_GRACE_US - 1,
        signal_at + SERVE_STOP_GRACE_US,
    ];
    let input_targets = [0, 1, 2, 2];
    let at_us = Cell::new(poll_times[0]);
    let polls = Cell::new(0_usize);
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut output = Output {
        pending: Vec::new(),
        events: Rc::clone(&events),
    };
    let result = run_with_controlled_clock(
        &options,
        input,
        &mut output,
        || MonoTime::from_micros(at_us.get()),
        || {
            let poll = polls.get();
            assert!(poll < poll_times.len(), "terminal poll requested another step");
            at_us.set(poll_times[poll]);
            polls.set(poll + 1);
            match poll {
                1 => writeln!(runner, "{{\"type\":\"stop\",\"reason\":\"first stop\"}}")?,
                2 => writeln!(runner, "{{\"type\":\"stop\",\"reason\":\"repeated stop\"}}")?,
                _ => {}
            }
            Ok(RelayPollTargets {
                stdout_frames: 1,
                runner_inputs: input_targets[poll],
            })
        },
        |progress| {
            let poll = polls.get() - 1;
            assert_eq!(progress.poll_seq, polls.get() as u64);
            assert_eq!(progress.stdout_frames, 1);
            assert_eq!(progress.runner_inputs, input_targets[poll]);
            assert_eq!(progress.stop_signal_sent_us, (poll > 0).then_some(signal_at));
            assert_eq!(progress.terminal, poll == 3);
            let mut events = events.borrow_mut();
            assert!(!events.iter().any(|event| matches!(
                event,
                Event::Flushed(SessionLine::SessionEnd { .. })
            )));
            let pid = events
                .iter()
                .find_map(|event| match event {
                    Event::Flushed(SessionLine::Stdout { line, .. }) => line
                        .strip_prefix("ready:")
                        .and_then(|pid| pid.parse().ok()),
                    _ => None,
                })
                .unwrap();
            let pid = rustix::process::Pid::from_raw(pid).unwrap();
            assert_eq!(
                rustix::process::test_kill_process(pid).is_err(),
                progress.terminal,
                "the child survives until the exact grace boundary, then is killed and reaped before terminal progress"
            );
            events.push(Event::Completed(progress));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(result, 3);
    assert_eq!(polls.get(), 4);
    let events = events.borrow();
    let terminal = events
        .iter()
        .position(|event| matches!(event, Event::Completed(progress) if progress.terminal))
        .unwrap();
    assert!(terminal < events.len() - 1);
    let Some(Event::Flushed(SessionLine::SessionEnd {
        killed,
        serve: Some(end),
        ..
    })) = events.last()
    else {
        panic!("final output must follow the terminal poll: {events:?}");
    };
    assert!(*killed);
    assert_eq!(end.stop_cause, "runner_stop");
    assert_eq!(end.stop_reason.as_deref(), Some("first stop"));
    assert_eq!(end.stop_requested_us, Some(signal_at));
    assert_eq!(end.signal_sent_us, Some(signal_at));
    assert_eq!(end.signal_error, None);
    assert!(end.forced_kill);
    assert!(end.group_reaped);
    assert_eq!(end.exited_us, None);
}
