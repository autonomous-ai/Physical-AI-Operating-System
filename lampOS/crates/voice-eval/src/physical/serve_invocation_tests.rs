//! The explicit command reports its actual received vector, without rewriting
//! the suffix parsed by the existing fake runtime or inventing another line.
use super::*;
use std::io;

#[test]
fn only_the_leading_control_pair_is_removed_from_the_parser_view() {
    let received = [
        "--fixture-control",
        "/private/a control socket.sock",
        "--fake-plan",
        "a plan; kept literally.json",
        "--fake-scenario",
        "serve-conversation",
        "serve",
        "/private/provider.json",
        "/private/work with spaces",
        "--cue-socket",
        "/private/cue.sock",
    ]
    .map(str::to_owned);
    let original = received.clone();
    let parsed = controlled_serve_arguments(&received).unwrap();
    assert_eq!(parsed, &received[2..]);
    assert!(std::ptr::eq(parsed.as_ptr(), received[2..].as_ptr()));
    assert_eq!(received, original);

    let mut output = Vec::new();
    assert_eq!(emit_serve_argv(&mut output, &received).unwrap(), 1);
    assert_eq!(output.iter().filter(|byte| **byte == b'\n').count(), 1);
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value, json!({"kind":"fake_lamp_argv","argv":received}));
    assert_eq!(received, original);
}

#[test]
fn invalid_invocation_prefixes_fail_before_any_clock_or_hook() {
    for arguments in [
        vec![],
        vec!["--fixture-control"],
        vec!["--fixture-control", "relative.sock", "serve"],
        vec!["--other", "/private/control.sock", "serve"],
        vec!["serve", "--fixture-control", "/private/control.sock"],
    ] {
        let arguments: Vec<_> = arguments.into_iter().map(str::to_owned).collect();
        let error = run_serve_with_controlled_invocation(
            &arguments,
            || panic!("invalid prefix sampled the clock"),
            || panic!("invalid prefix entered a poll"),
            |_| panic!("invalid prefix completed a poll"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("leading --fixture-control"));
    }
}

#[test]
fn invocation_and_direct_paths_keep_the_same_parser_and_mode_errors() {
    for arguments in [
        vec!["--fake-seed", "invalid", "serve"],
        vec!["--fake-unknown", "value", "serve"],
        vec!["--fake-room"],
        vec!["directed"],
        vec!["directed-fixture"],
    ] {
        let arguments: Vec<_> = arguments.into_iter().map(str::to_owned).collect();
        let expected = run_serve_with_controlled_clock(
            &arguments,
            || panic!("invalid direct arguments sampled the clock"),
            || panic!("invalid direct arguments entered a poll"),
            |_| panic!("invalid direct arguments completed a poll"),
        )
        .unwrap_err();
        let mut received = vec!["--fixture-control".into(), "/private/control.sock".into()];
        received.extend(arguments);
        let error = run_serve_with_controlled_invocation(
            &received,
            || panic!("invalid invocation sampled the clock"),
            || panic!("invalid invocation entered a poll"),
            |_| panic!("invalid invocation completed a poll"),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), expected.to_string());
    }
}

struct FlushFailure {
    bytes: Vec<u8>,
    flushes: usize,
}

impl Write for FlushFailure {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Err(io::Error::other("invocation flush failed"))
    }
}

#[test]
fn a_written_invocation_without_a_successful_flush_has_no_receipt() {
    let received = ["--fixture-control", "/private/socket\nname", "serve"].map(str::to_owned);
    let mut output = FlushFailure {
        bytes: Vec::new(),
        flushes: 0,
    };
    assert_eq!(
        emit_serve_argv(&mut output, &received)
            .unwrap_err()
            .to_string(),
        "invocation flush failed"
    );
    assert_eq!(output.flushes, 1);
    assert_eq!(
        output.bytes.iter().filter(|byte| **byte == b'\n').count(),
        1
    );
    let value: Value = serde_json::from_slice(&output.bytes).unwrap();
    assert_eq!(value["argv"], json!(received));
}
