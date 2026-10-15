//! Explicit CLI entrypoints for locally coordinated finite and serve fakes.
//!
//! The ordinary fake runtime and physical relay keep their original entrypoints.
//! These handlers require a leading control-socket argument, use pure clock
//! reads and acknowledge completed work only. Logical completion never replaces
//! the caller's actual child-exit, EOF and trace-cleanup checks.
use super::{
    fake_lamp,
    fake_serve::{ServePollProgress, ServePollTargets},
    fixture_control::{Participant, Progress, Role},
    session::{self, RelayPollProgress, RelayPollTargets, SessionMode, SessionOptions},
};
use crate::{Result, invalid};
use std::{
    cell::{Cell, RefCell},
    io::{self, Write},
    path::{Path, PathBuf},
    rc::Rc,
};

/// `fake-clock-lamp-voice --fixture-control SOCKET` followed by the unchanged
/// fake runtime arguments. Its existing controlled seam rejects serve mode.
pub fn runtime(arguments: Vec<String>) -> Result<i32> {
    let (path, arguments) = controlled_arguments(arguments)?;
    let mut participant = Participant::connect(&path, Role::Runtime)?;
    let clock = participant.clock();
    fake_lamp::run_with_clock_and_poll(
        &arguments,
        || clock.now(),
        |poll| {
            if poll.at.as_micros() != participant.current_step().at_us {
                return Err(invalid(
                    "runtime poll did not use its current fixture Step time",
                ));
            }
            let progress = Progress::Runtime {
                poll_seq: poll.poll,
                stdout_frames: poll.stdout_lines,
                room_inputs: poll.room_inputs,
                logical_end: poll.ended,
            };
            if poll.ended {
                // The real fake-runtime implementation writes and syncs its
                // final trace after this callback returns, then exits normally.
                participant.final_ack(progress)?;
            } else {
                participant.complete(progress)?;
            }
            Ok(())
        },
    )
}

/// `fake-clock-serve-runtime --fixture-control SOCKET` followed by the original
/// fake serve arguments. This role waits for real signal observation when the
/// parent requires it; a terminal receipt follows the engine's final output.
pub fn serve_runtime(arguments: Vec<String>) -> Result<i32> {
    let (path, arguments) = controlled_arguments(arguments)?;
    let participant = RefCell::new(Participant::connect(&path, Role::ServeRuntime)?);
    let clock = participant.borrow().clock();
    fake_lamp::run_serve_with_controlled_clock(
        &arguments,
        || clock.now(),
        || {
            Ok(ServePollTargets {
                wait_for_sigterm: participant.borrow().current_step().wait_for_sigterm,
            })
        },
        |poll| {
            let mut participant = participant.borrow_mut();
            if poll.at.as_micros() != participant.current_step().at_us {
                return Err(invalid(
                    "serve runtime poll did not use its current fixture Step time",
                ));
            }
            let progress = serve_runtime_progress(poll);
            if poll.terminal {
                // The engine already finished metadata, flushed final stdout
                // and unlinked its room. The actual child exit still follows.
                participant.final_ack(progress)?;
            } else {
                participant.complete(progress)?;
            }
            Ok(())
        },
    )
}

/// `fake-clock-session --fixture-control SOCKET` followed by the unchanged
/// relay arguments. Only a finite controlled fake in this executable may run.
pub fn relay(arguments: Vec<String>) -> Result<i32> {
    let (path, arguments) = controlled_arguments(arguments)?;
    let options = SessionOptions::parse(&arguments)?;
    validate_relay_options(&options)?;
    controlled_relay(options, &path, Role::Relay, relay_progress)
}

/// `fake-clock-serve-session --fixture-control SOCKET` followed by relay serve
/// arguments. The child must be the explicit serve fake in this executable.
pub fn serve_relay(arguments: Vec<String>) -> Result<i32> {
    let (path, arguments) = controlled_arguments(arguments)?;
    let options = SessionOptions::parse(&arguments)?;
    validate_serve_relay_options(&options)?;
    controlled_relay(options, &path, Role::ServeRelay, serve_relay_progress)
}

fn controlled_relay(
    options: SessionOptions,
    path: &Path,
    role: Role,
    progress: fn(RelayPollProgress, u64) -> Progress,
) -> Result<i32> {
    let participant = RefCell::new(Participant::connect(path, role)?);
    let clock = participant.borrow().clock();
    let terminal = Cell::new(None);
    let stdout = io::stdout();
    let mut output = CountingWriter::new(stdout.lock());
    let flushed_frames = output.flushed_frames.clone();
    let code = session::run_with_controlled_clock(
        &options,
        io::stdin(),
        &mut output,
        || clock.now(),
        || {
            let step = participant.borrow().current_step();
            Ok(RelayPollTargets {
                stdout_frames: step.stdout_target,
                runner_inputs: step.runner_input_target,
            })
        },
        |poll| {
            if terminal.get().is_some() {
                return Err(invalid("relay polled after terminal fixture progress"));
            }
            if poll.terminal {
                // Final trace/SessionEnd output and child cleanup still follow
                // this hook. Record the poll; do not acknowledge it yet.
                terminal.set(Some(poll));
            } else {
                participant
                    .borrow_mut()
                    .complete(progress(poll, flushed_frames.get()))?;
            }
            Ok(())
        },
    )?;
    let terminal = terminal
        .get()
        .ok_or_else(|| invalid("relay returned without a terminal fixture poll"))?;
    output.finish()?;
    participant
        .borrow_mut()
        .final_ack(progress(terminal, flushed_frames.get()))?;
    Ok(code)
}

fn relay_progress(poll: RelayPollProgress, output_frames: u64) -> Progress {
    Progress::Relay {
        poll_seq: poll.poll_seq,
        stdout_frames: poll.stdout_frames,
        runner_inputs: poll.runner_inputs,
        output_frames,
        logical_end: poll.terminal,
    }
}

fn serve_runtime_progress(poll: ServePollProgress) -> Progress {
    Progress::ServeRuntime {
        poll_seq: poll.poll_seq,
        stdout_frames: poll.stdout_lines,
        room_consumed: poll.room_consumed,
        room_injected: poll.room_injected,
        cycle: poll.cycle,
        cycle_base_us: poll.cycle_base_us,
        runtime_us: poll.runtime_us,
        observed_signal: poll.observed_signal,
        logical_end: poll.terminal,
    }
}

fn serve_relay_progress(poll: RelayPollProgress, output_frames: u64) -> Progress {
    Progress::ServeRelay {
        poll_seq: poll.poll_seq,
        stdout_frames: poll.stdout_frames,
        runner_inputs: poll.runner_inputs,
        output_frames,
        stop_signal_sent_us: poll.stop_signal_sent_us,
        logical_end: poll.terminal,
    }
}

fn controlled_arguments(arguments: Vec<String>) -> Result<(PathBuf, Vec<String>)> {
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some("--fixture-control") {
        return Err(invalid(
            "a controlled fixture requires leading --fixture-control SOCKET",
        ));
    }
    let path = arguments
        .next()
        .ok_or_else(|| invalid("--fixture-control needs an absolute socket path"))?;
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err(invalid("--fixture-control needs an absolute socket path"));
    }
    Ok((path, arguments.collect()))
}

fn validate_relay_options(options: &SessionOptions) -> Result<()> {
    if matches!(options.mode, SessionMode::Serve { .. }) {
        return Err(invalid(
            "a controlled fixture relay supports only finite sessions",
        ));
    }
    match options.runtime_prefix.as_slice() {
        [command, flag, path, ..]
            if command == "fake-clock-lamp-voice"
                && flag == "--fixture-control"
                && Path::new(path).is_absolute() => {}
        _ => {
            return Err(invalid(
                "a controlled fixture relay requires runtime prefix fake-clock-lamp-voice --fixture-control SOCKET",
            ));
        }
    }
    if options.runtime.canonicalize()? != std::env::current_exe()?.canonicalize()? {
        return Err(invalid(
            "a controlled fixture relay must launch this evaluator executable",
        ));
    }
    Ok(())
}

fn validate_serve_relay_options(options: &SessionOptions) -> Result<()> {
    if !matches!(options.mode, SessionMode::Serve { .. }) {
        return Err(invalid("a controlled serve relay requires serve mode"));
    }
    match options.runtime_prefix.as_slice() {
        [command, flag, path, ..]
            if command == "fake-clock-serve-runtime"
                && flag == "--fixture-control"
                && Path::new(path).is_absolute() => {}
        _ => {
            return Err(invalid(
                "a controlled serve relay requires runtime prefix fake-clock-serve-runtime --fixture-control SOCKET",
            ));
        }
    }
    if options.runtime.canonicalize()? != std::env::current_exe()?.canonicalize()? {
        return Err(invalid(
            "a controlled serve relay must launch this evaluator executable",
        ));
    }
    Ok(())
}

/// Count LF-terminated frames only after every accepted byte preceding them
/// has been flushed. Partial writes and partial lines never fabricate frames.
struct CountingWriter<W> {
    inner: W,
    flushed_frames: Rc<Cell<u64>>,
    pending_frames: u64,
    partial_line: bool,
}

impl<W: Write> CountingWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            flushed_frames: Rc::new(Cell::new(0)),
            pending_frames: 0,
            partial_line: false,
        }
    }

    fn finish(&mut self) -> io::Result<()> {
        self.flush()?;
        if self.partial_line {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "fixture relay output ended inside a frame",
            ));
        }
        Ok(())
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(bytes)?;
        let accepted = &bytes[..written];
        let frames = accepted.iter().filter(|byte| **byte == b'\n').count() as u64;
        self.pending_frames = self.pending_frames.checked_add(frames).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "fixture output pending-frame counter overflow",
            )
        })?;
        if let Some(last) = accepted.last() {
            self.partial_line = *last != b'\n';
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()?;
        let flushed = self
            .flushed_frames
            .get()
            .checked_add(self.pending_frames)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "fixture output flushed-frame counter overflow",
                )
            })?;
        self.flushed_frames.set(flushed);
        self.pending_frames = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Writer {
        bytes: Vec<u8>,
        max_write: usize,
        fail_write: bool,
        fail_flush: bool,
    }

    impl Writer {
        fn new() -> Self {
            Self {
                bytes: Vec::new(),
                max_write: 2,
                fail_write: false,
                fail_flush: false,
            }
        }
    }

    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_write {
                return Err(io::Error::other("deliberate write failure"));
            }
            let written = bytes.len().min(self.max_write);
            self.bytes.extend_from_slice(&bytes[..written]);
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.fail_flush {
                Err(io::Error::other("deliberate flush failure"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn frames_count_after_flush_across_short_writes_and_partial_lines() {
        let mut output = CountingWriter::new(Writer::new());
        output.write_all(b"one\n").unwrap();
        assert_eq!(output.flushed_frames.get(), 0);
        output.flush().unwrap();
        assert_eq!(output.flushed_frames.get(), 1);
        output.write_all(b"two\nthree\npartial").unwrap();
        output.flush().unwrap();
        assert_eq!(output.flushed_frames.get(), 3);
        assert!(output.partial_line);
        output.write_all(b" done\n").unwrap();
        assert_eq!(output.flushed_frames.get(), 3);
        output.finish().unwrap();
        assert_eq!(output.flushed_frames.get(), 4);
        assert_eq!(output.inner.bytes, b"one\ntwo\nthree\npartial done\n");
    }

    #[test]
    fn failed_writes_and_flushes_do_not_publish_completed_frames() {
        let mut output = CountingWriter::new(Writer::new());
        output.inner.fail_write = true;
        assert!(output.write_all(b"one\n").is_err());
        assert_eq!(output.pending_frames, 0);
        assert_eq!(output.flushed_frames.get(), 0);
        assert!(output.inner.bytes.is_empty());

        output.inner.fail_write = false;
        output.inner.fail_flush = true;
        output.write_all(b"one\n").unwrap();
        assert!(output.flush().is_err());
        assert_eq!(output.pending_frames, 1);
        assert_eq!(output.flushed_frames.get(), 0);
        // Deliberately changing the fake writer's behavior proves the same
        // pending frame is published once; the CLI never retries failed I/O.
        output.inner.fail_flush = false;
        output.finish().unwrap();
        output.flush().unwrap();
        assert_eq!(output.flushed_frames.get(), 1);
    }

    #[test]
    fn final_output_must_end_on_a_complete_frame() {
        let mut output = CountingWriter::new(Vec::new());
        output.write_all(b"one\npartial").unwrap();
        let error = output.finish().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(output.flushed_frames.get(), 1);
    }

    #[test]
    fn frame_counters_refuse_to_wrap() {
        let mut output = CountingWriter::new(Vec::new());
        output.pending_frames = u64::MAX;
        assert_eq!(
            output.write(b"\n").unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(output.flushed_frames.get(), 0);

        let mut output = CountingWriter::new(Vec::new());
        output.flushed_frames.set(u64::MAX);
        output.write_all(b"\n").unwrap();
        assert_eq!(
            output.flush().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(output.flushed_frames.get(), u64::MAX);
    }

    #[test]
    fn only_the_leading_control_pair_is_removed_from_runtime_arguments() {
        let arguments = [
            "--fixture-control",
            "/private/fixture/control.sock",
            "--runtime-prefix-arg",
            "--fixture-control",
            "--runtime-prefix-arg",
            "/private/fixture/runtime.sock",
            "--work",
            "/private/fixture/work with spaces",
        ]
        .map(str::to_owned)
        .to_vec();
        let (path, rest) = controlled_arguments(arguments.clone()).unwrap();
        assert_eq!(path, Path::new("/private/fixture/control.sock"));
        assert_eq!(rest, arguments[2..]);
        for arguments in [
            vec![],
            vec!["--fixture-control"],
            vec!["--other", "/tmp/control.sock"],
            vec!["--fixture-control", "relative.sock"],
            vec!["--fixture-control", "--work"],
        ] {
            assert!(
                controlled_arguments(arguments.into_iter().map(str::to_owned).collect()).is_err()
            );
        }
    }

    fn options() -> SessionOptions {
        SessionOptions {
            runtime: std::env::current_exe().unwrap(),
            runtime_prefix: [
                "fake-clock-lamp-voice",
                "--fixture-control",
                "/private/fixture/runtime.sock",
            ]
            .map(str::to_owned)
            .to_vec(),
            work: "/private/fixture/work".into(),
            seconds: 12,
            mode: SessionMode::Fixture {
                reply: "/unused/reply.wav".into(),
                sha256: "0".repeat(64),
            },
            noise_suppression: "on".into(),
            diagnostics: false,
            allowance_seconds: 10,
            ring_channel_ceiling: None,
            runtime_flags: Default::default(),
        }
    }

    #[test]
    fn relay_accepts_only_finite_controlled_fake_runtime_configuration() {
        let mut options = options();
        validate_relay_options(&options).unwrap();
        options.mode = SessionMode::Directed {
            provider_config: "/unused/provider.json".into(),
        };
        validate_relay_options(&options).unwrap();
        options.mode = SessionMode::Serve {
            provider_config: "/unused/provider.json".into(),
        };
        assert!(
            validate_relay_options(&options)
                .unwrap_err()
                .to_string()
                .contains("finite")
        );
        options.mode = SessionMode::Fixture {
            reply: "/unused/reply.wav".into(),
            sha256: "0".repeat(64),
        };
        options.runtime_prefix[0] = "fake-lamp-voice".into();
        assert!(
            validate_relay_options(&options)
                .unwrap_err()
                .to_string()
                .contains("fake-clock-lamp-voice")
        );
        options.runtime_prefix[0] = "fake-clock-lamp-voice".into();
        options.runtime_prefix[2] = "relative.sock".into();
        assert!(validate_relay_options(&options).is_err());
        options.runtime_prefix[2] = "/private/fixture/runtime.sock".into();
        options.runtime = "/bin/sh".into();
        assert!(
            validate_relay_options(&options)
                .unwrap_err()
                .to_string()
                .contains("this evaluator executable")
        );
    }

    #[test]
    fn relay_terminal_progress_keeps_the_last_real_poll_and_final_output_count() {
        assert_eq!(
            relay_progress(
                RelayPollProgress {
                    poll_seq: 5,
                    stdout_frames: 2,
                    runner_inputs: 3,
                    stop_signal_sent_us: None,
                    terminal: true
                },
                19
            ),
            Progress::Relay {
                poll_seq: 5,
                stdout_frames: 2,
                runner_inputs: 3,
                output_frames: 19,
                logical_end: true
            }
        );
    }
}

#[cfg(test)]
#[path = "serve_fixture_commands_tests.rs"]
mod serve_tests;
