use super::*;
use lamp_turn_owner::MonoTime;

fn serve_options() -> SessionOptions {
    SessionOptions {
        runtime: std::env::current_exe().unwrap(),
        runtime_prefix: [
            "fake-clock-serve-runtime",
            "--fixture-control",
            "/private/fixture/serve.sock",
        ]
        .map(str::to_owned)
        .to_vec(),
        work: "/private/fixture/serve-work".into(),
        seconds: 240,
        mode: SessionMode::Serve {
            provider_config: "/unused/provider.json".into(),
        },
        noise_suppression: "on".into(),
        diagnostics: false,
        allowance_seconds: 10,
        ring_channel_ceiling: None,
        runtime_flags: Default::default(),
    }
}

#[test]
fn serve_relay_accepts_only_its_local_explicit_serve_command() {
    let mut options = serve_options();
    validate_serve_relay_options(&options).unwrap();
    options.mode = SessionMode::Directed {
        provider_config: "/unused/provider.json".into(),
    };
    assert!(
        validate_serve_relay_options(&options)
            .unwrap_err()
            .to_string()
            .contains("requires serve mode")
    );
    options.mode = SessionMode::Fixture {
        reply: "/unused/reply.wav".into(),
        sha256: "0".repeat(64),
    };
    assert!(validate_serve_relay_options(&options).is_err());
    options.mode = serve_options().mode;
    for command in ["fake-clock-lamp-voice", "fake-lamp-voice", "serve"] {
        options.runtime_prefix[0] = command.into();
        assert!(
            validate_serve_relay_options(&options)
                .unwrap_err()
                .to_string()
                .contains("fake-clock-serve-runtime")
        );
    }
    options.runtime_prefix[0] = "fake-clock-serve-runtime".into();
    options.runtime_prefix[1] = "--other".into();
    assert!(validate_serve_relay_options(&options).is_err());
    options.runtime_prefix[1] = "--fixture-control".into();
    options.runtime_prefix[2] = "relative.sock".into();
    assert!(validate_serve_relay_options(&options).is_err());
    options.runtime_prefix[2] = "/private/fixture/serve.sock".into();
    options.runtime = "/bin/sh".into();
    assert!(
        validate_serve_relay_options(&options)
            .unwrap_err()
            .to_string()
            .contains("this evaluator executable")
    );
}

#[test]
fn controlled_runtime_modes_are_rejected_before_clock_reads_or_hooks() {
    for command in ["directed", "directed-fixture"] {
        let arguments = [command.to_owned()];
        let error = fake_lamp::run_serve_with_controlled_clock(
            &arguments,
            || panic!("rejected mode read the clock"),
            || panic!("rejected mode entered a serve poll"),
            |_| panic!("rejected mode completed a serve poll"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("only serve runs"));
    }
    let error = fake_lamp::run_with_clock_and_poll(
        &["serve".into()],
        || panic!("finite entrypoint read the serve clock"),
        |_| panic!("finite entrypoint entered a serve poll"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("only finite directed runs"));
}

#[test]
fn all_fake_entrypoints_share_original_option_errors() {
    for arguments in [
        vec!["--fake-seed", "invalid", "serve"],
        vec!["--fake-unknown", "value", "serve"],
        vec!["--fake-room"],
    ] {
        let arguments: Vec<_> = arguments.into_iter().map(str::to_owned).collect();
        let expected = fake_lamp::run(&arguments).unwrap_err().to_string();
        let finite = fake_lamp::run_with_clock_and_poll(
            &arguments,
            || panic!("invalid arguments read the finite clock"),
            |_| panic!("invalid arguments entered a finite poll"),
        )
        .unwrap_err();
        let serve = fake_lamp::run_serve_with_controlled_clock(
            &arguments,
            || panic!("invalid arguments read the serve clock"),
            || panic!("invalid arguments entered a serve poll"),
            |_| panic!("invalid arguments completed a serve poll"),
        )
        .unwrap_err();
        assert_eq!(finite.to_string(), expected);
        assert_eq!(serve.to_string(), expected);
    }
}

struct ArgvWriter {
    bytes: Vec<u8>,
    flushes: usize,
    fail_write: bool,
    fail_flush: bool,
}

impl Write for ArgvWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.fail_write {
            return Err(io::Error::other("argv write failed"));
        }
        let count = bytes.len().min(3);
        self.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        if self.fail_flush {
            Err(io::Error::other("argv flush failed"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn argv_receipt_requires_the_original_complete_json_line_to_flush() {
    let arguments =
        ["serve", "a path with spaces", "line\nbreak", "--cue-socket"].map(str::to_owned);
    // Independent attempts exercise failures; no failed write or flush is retried.
    for (fail_write, fail_flush) in [(true, false), (false, true), (false, false)] {
        let mut output = CountingWriter::new(ArgvWriter {
            bytes: Vec::new(),
            flushes: 0,
            fail_write,
            fail_flush,
        });
        let receipt = fake_lamp::emit_serve_argv(&mut output, &arguments);
        if fail_write || fail_flush {
            assert!(receipt.is_err());
            assert_eq!(output.flushed_frames.get(), 0);
        } else {
            assert_eq!(receipt.unwrap(), 1);
            assert_eq!(output.flushed_frames.get(), 1);
        }
        assert_eq!(output.inner.flushes, usize::from(!fail_write));
        if fail_write {
            assert!(output.inner.bytes.is_empty());
        } else {
            assert_eq!(output.inner.bytes.last(), Some(&b'\n'));
            assert_eq!(
                output
                    .inner
                    .bytes
                    .iter()
                    .filter(|byte| **byte == b'\n')
                    .count(),
                1
            );
            let value: serde_json::Value = serde_json::from_slice(&output.inner.bytes).unwrap();
            assert_eq!(
                value,
                serde_json::json!({"kind":"fake_lamp_argv","argv":arguments})
            );
        }
    }
}

#[test]
fn serve_runtime_mapping_retains_consumption_injection_cycle_and_signal() {
    for terminal in [false, true] {
        let poll = ServePollProgress {
            poll_seq: 7,
            at: MonoTime::from_micros(510_000),
            cycle: 2,
            cycle_base_us: 500_000,
            runtime_us: 10_000,
            stdout_lines: 4,
            room_consumed: 5,
            room_injected: 3,
            observed_signal: signal_hook::consts::SIGTERM as usize,
            terminal,
        };
        assert_eq!(
            serve_runtime_progress(poll),
            Progress::ServeRuntime {
                poll_seq: 7,
                stdout_frames: 4,
                room_consumed: 5,
                room_injected: 3,
                cycle: 2,
                cycle_base_us: 500_000,
                runtime_us: 10_000,
                observed_signal: signal_hook::consts::SIGTERM as usize,
                logical_end: terminal,
            }
        );
    }
}

#[test]
fn serve_relay_mapping_keeps_signal_receipt_separate_from_inputs_and_end() {
    for terminal in [false, true] {
        for stop_signal_sent_us in [None, Some(510_000)] {
            let poll = RelayPollProgress {
                poll_seq: 8,
                stdout_frames: 4,
                runner_inputs: 3,
                stop_signal_sent_us,
                terminal,
            };
            assert_eq!(
                serve_relay_progress(poll, 19),
                Progress::ServeRelay {
                    poll_seq: 8,
                    stdout_frames: 4,
                    runner_inputs: 3,
                    output_frames: 19,
                    stop_signal_sent_us,
                    logical_end: terminal,
                }
            );
        }
    }
}
