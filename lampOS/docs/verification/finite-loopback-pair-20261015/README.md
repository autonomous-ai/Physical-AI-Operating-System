# Finite loopback paired local result

Part of #823, recorded before the final gate on `tests/823-fixture-progress`.
The serial x86_64 macOS debug nextest pair contains the same **12 tests**, all
PASS, no skips or retries: **248.575 s before, 28.471 s after**. Build time is
excluded. `comparison.json` checks compiled identities/ignored flags and
retains per-test results. Both invocations used the same evaluator binary;
only the test construction selected the coordinated clock for the after run.

The independent source review restores the full original test file after
reversing four construction/comment/import regions. All 100 original
assertion macros, all 12 identities and both fault loops are retained. Real
relay/runtime processes, sockets, CueSink, grants, trace output and actual
child reaping remain in the test path. The two quick original-clock cases
retain their construction. See `final-peer-review.md` for scheduling and
queue review, including the earlier queue deadlock corrected before testing.

This is a paired local result, not a complete final gate, ARM64 speed result
or evidence that all 123 tests fit in two minutes. The implementation is
rebased onto `a3dba0c0a`; the final evaluator gate is running. The prepared
serve-player, recorder and signal coordination remain later work.

Commands from `lampOS/` (Cargo/nextest on PATH):

```sh
cargo nextest list --locked --offline -p lamp-voice-eval --test physical_loopback --message-format json
cargo nextest run --locked --offline -p lamp-voice-eval --test physical_loopback --profile realtime --test-threads 1 --retries 0 --no-fail-fast --status-level pass --final-status-level fail --color never
```

The pair used `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0` and the same shared
target directory. No production timing, assertion, profile or workflow was
changed to obtain this result.
