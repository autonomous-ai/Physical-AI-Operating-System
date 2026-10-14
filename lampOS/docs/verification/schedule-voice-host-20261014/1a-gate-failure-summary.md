# Failed 1a complete host gate

Task 31 remains **not ready**. On base `1a38f2f3e6b8afd1e7896761a0da6149a2400f67`, the complete affected-package test command exited **101** after **683.341811104 s**: **1,116 passed, 5 failed, 8 ignored across 16 completed harnesses**. Voice lib reported **923 passed, 5 failed, 8 ignored in 491.89 s**. Formatting passed in **3.745649047 s** and strict all-target Clippy in **36.722263178 s**. The subsequent workspace build was **not reached**.

These are host-only results on Intel i7-10700K x86_64 Mac, macOS 26.6.2 / Darwin 25.6.0, 64 GiB RAM; rustc 1.96.0 (`ac68faa20`) and Cargo 1.96.0. No lamp, live provider, acoustic path or ARM64 execution was involved. Command wall time includes compilation and execution; it is not lamp latency.

The [original result](raw/lamp-third-schedule-voice-1a-gate-results.json), [raw test log](raw/lamp-third-schedule-voice-1a-gate-tests.txt), and [source map](raw/lamp-third-schedule-voice-1a-gate-source.json) are retained byte-for-byte. The result records all **676 frozen source hashes unchanged** after each completed stage. The source map matches the existing `source-1a.json`; neither that map nor the rebase evidence was overwritten. [The retention index](1a-gate-failure-retention.json) records original paths, retained paths, byte lengths and hashes.

All five failures are in `coordinator::motor::tests`. Their backtraces report the same two-second wait helper at `crates/voice/src/coordinator/motor/tests.rs:441` with `owned child timed out: None`. The caller locations and predicates below were checked against the exact recorded source-file hash:

| Failed case | Caller in `motor/tests.rs` | Predicate and reached phase |
| --- | ---: | --- |
| `crash_and_stalled_control_never_allow_replacement_or_false_cleanup` | 645 | `OptionalMotor::reaped`, inside the `crash`/`hang` loop; the failing mode is unrecorded. |
| `explicit_current_segment_reaches_inert_counts_without_completing_voice` | 905 | Initial `OptionalMotor::is_ready`, before inspection or segment offer. |
| `failed_reap_after_stopped_is_latched_and_reported_once` | 1001 | Initial `OptionalMotor::is_ready`, before shutdown or the later Stopped/reap assertions. |
| `frozen_config_and_executable_are_exact_and_source_changes_are_not_adopted` | 782 | Initial `OptionalMotor::is_ready`, before config/executable identity assertions. |
| `idle_ready_never_inspects_moves_or_claims_listening` | 503 | Initial `OptionalMotor::is_ready`, before the idle/no-listening assertions. |

`None` is the recorded failure field at the deadline; it does not establish whether a child had spawned, identify the blocking phase inside the child, or establish a root cause. This record does not classify the failures as flakes, attribute them to Task 31 or an upstream change, or weaken any assertion. No test, diagnostic or Cargo rerun was performed while retaining this evidence.

The completed commands ran sequentially from `/Users/d/code/autonomous-os/lampOS` with `RUST_BACKTRACE=1`:

```sh
cargo fmt --all --check
cargo clippy --offline --locked -p lamp-actions -p lamp-schedule -p lamp-voice -p lamp-voice-eval --all-targets -- -D warnings
cargo test --offline --locked -p lamp-actions -p lamp-schedule -p lamp-voice -p lamp-voice-eval -- --test-threads=1
```

The eight ignored cases and their reasons remain in the original report. The command stopped in Voice lib, so later harnesses and the workspace build are not claimed as completed.

An after-run copy of the executable path named by the test log is kept **outside the repository**. It is 42,826,408 bytes with SHA256 `1a773923516b7c48789d09e72fd5e09f5a657d45fbe615c18bb2c9e1a080a1f6`. [Its provenance](raw/lamp-third-schedule-voice-1a-failed-test-binary-after-run.json) records the original and retained paths, capture time, stable source stat/hash across the copy, and the limit: the gate log has no pre-run binary hash, so this after-run identity does not independently establish the exact bytes loaded during the failure. The copied binary was not executed and is not included in repository evidence.

Earlier dcc/7f failed gates and the successful [bc3 standalone subset](bc3-standalone/README.md) retain their own source identities. The broader bc3 `tools` filter passed 19 tests; the separate bc3 focused `tools::` filter passed 18. Neither result overrides this failed complete 1a gate. Subsequent incoming Touch/Voice changes await their own integration and verification.
