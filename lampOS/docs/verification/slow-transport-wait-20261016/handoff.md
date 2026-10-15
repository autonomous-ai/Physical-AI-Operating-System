# Slow-transport wait seam for #823

Worktree: `/private/tmp/lampos-tests-slow-transport-clock`.
Assigned branch/base: `tests/823-slow-transport-clock` / `6b08a70d5`.
No Git commands were used to alter or independently re-check that assignment.

Started `2026-10-16 01:13:04 +0700`; source audit completed
`2026-10-16 01:20:21 +0700`. Both timestamps came from `date`.

Only these repository files were changed:

- `lampOS/crates/voice-eval/src/physical/fake_lamp.rs`
- New `lampOS/crates/voice-eval/src/physical/slow_transport_clock_tests.rs`
- `lampOS/docs/realtime-tests.md`

## API and preserved default

```rust
pub fn slow_transport_with_wait(
    arguments: &[String],
    wait: impl FnOnce(Duration) -> Result<()>,
) -> Result<i32>
```

`Result` remains the crate's existing boxed-error result. The original parser
runs before the wait. A valid delay reaches the callback exactly once as the
complete `Duration::from_millis(delay_ms)`, including zero and the full u64
millisecond range. A callback error returns unchanged before exec. Success
continues through the original argument slice and actual `CommandExt::exec`;
exec errors retain their original boxed I/O error without added context.

The original `slow_transport` now delegates with one closure that calls
`thread::sleep(duration)` once and returns `Ok(())`. No clock source, clock
sample, shortened duration, CLI option, environment lookup, fixture protocol,
driver behavior, integration constructor, or global transport switch was
introduced. This is a seam only; the original delayed integration attempt
still takes its host delay and uses its original assertions.

The supplied callback owns any coordination and host watchdog it requires.
Returning from it establishes only wait completion; actual exec success is
still a later process boundary. The seam makes no process-exit or readiness
claim and adds no retry.

## Source-preservation evidence

`audit_source.py` performs exact replacements of the known new API prefix,
single wait call, and test-module declaration. The reconstructed entire
`fake_lamp.rs` matches its pre-edit snapshot byte-for-byte. Therefore the old
parser, every existing error, the complete real exec tail, recorder/runtime
code, and original test-module declaration are unchanged outside this seam.

Removing the one exact documentation insertion also restores its original
bytes. The assigned heading `## Relay clock source` does not exist on this
base; the actual heading is `## Relay clock preparation (#823)`. The new small
section was inserted immediately before that heading without reordering any
upstream material; the parent was informed before the edit.

All 23 integration Rust files, including the complete original serve-session
and loopback sources, plus the existing `fake_runtime_clock_tests.rs` retain
their recorded SHA-256 hashes. `before-tests-sha256.json` and `audit.json`
contain the exact hashes, existing test identities, and static assertion
counts. None of these old test sources was edited.

The evidence directory also includes exact before snapshots, reconstructed
files, and unified source/documentation diffs. These checks are source
preservation proofs, not test execution.

## New tests and boundaries

Six new tests contain 23 static assertion sites:

1. Malformed structural arguments retain the original usage error before any
   callback, in both the default and injected entrypoints.
2. Empty, signed-negative, fractional, nonnumeric and overflowing delay values
   retain the original `ParseIntError` before waiting.
3. Zero, 400 ms, beyond-u32, and u64-max millisecond delays arrive intact at one
   callback. Each callback returns deliberately, without sleeping or exec.
4. An FnOnce-only closure consumes its owned value and returns a typed error;
   the same error type/payload escapes before the impossible directory exec.
5. An explicitly owned unit-test child compares real directory-exec failure
   with the original direct `CommandExt::exec`, including kind, OS code and
   display string after the full requested callback duration.
6. Another owned unit-test child performs actual exec into `/bin/sh`. Its
   distinctive exit code, unchanged PID, literal argv (spaces, option text and
   semicolon), inherited queued stdin, and completed-wait marker establish
   that the exec path ran. The parent unit-test process is never replaced.

Each child selects one exact libtest case using `Command::env` only on that
owned child; neither transport entrypoint reads the marker, and no parent
environment is modified. The test strips the crate prefix from `module_path!`
when constructing libtest's exact filter. The owned child is observed with
`try_wait`/yield under a ten-second host watchdog, reaped after real exit, and
killed/reaped on guard failure or early unwinding. There are no sleep calls,
ignored cases, test reruns, unsafe forks, process-global signal handlers, or
new dependencies in the new tests. The subprocess tests require a normal
Unix `/bin/sh`; they use builtins only and do not access a device or network.

## Verification and limitations

Completed, exit zero:

```text
/Users/autonomous/.cargo/bin/rustfmt +1.96.0 --edition 2024 --config skip_children=true lampOS/crates/voice-eval/src/physical/fake_lamp.rs lampOS/crates/voice-eval/src/physical/slow_transport_clock_tests.rs
/Users/autonomous/.cargo/bin/rustfmt +1.96.0 --edition 2024 --config skip_children=true --check lampOS/crates/voice-eval/src/physical/fake_lamp.rs lampOS/crates/voice-eval/src/physical/slow_transport_clock_tests.rs
python3 /private/tmp/tests-823-slow-transport-clock-evidence/audit_source.py
```

No Cargo, build, clippy, test binary, Git, network, escalation, or device
operation was run. The six new tests are authored but not compiled/executed.
The parent owns those gates and publication. This handoff is ready for those
gates, not a runtime qualification or speedup claim.

## Stable SHA-256

```text
376656656dbf8635b0d0c7b2e51308529f56002e85fc38834d6465bb6ef6bf26  lampOS/crates/voice-eval/src/physical/fake_lamp.rs
7478dae0e6ca89f1cae6b1aee14c22f5aa0871e5151ea50681a480d48baa1dea  lampOS/crates/voice-eval/src/physical/slow_transport_clock_tests.rs
cd6ffdd3420d3318135bb4728548b9b55520ba2e1cde28f5f40ab5baae629cb8  lampOS/docs/realtime-tests.md
```

Original full-file hashes restored by the reversal:

```text
fecb70fc79a85675749bd4208cfa5ec760d2ea5c56b2b6865acd56caee57649d  fake_lamp.rs
987cff5b07bec9c98150f8ffc072ecf1a0b4d490e762400c2529d56e82c4aabe  realtime-tests.md
```
