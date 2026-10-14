# One serial prefix-plus-motor diagnostic

The one authorized invocation passed: **157 passed, 0 failed, 0 ignored**,
691 filtered out. The test harness reported **21.10 seconds**; the external
runner measured **21.147252753 seconds** from process launch through exit.
The run started at **2026-10-14T18:28:29Z** (2026-10-15, 01:28:29 UTC+7),
exited 0, and did not reach the 180-second external cap. There was no retry,
rebuild, test/assertion/deadline change, or main repository edit. The execution
slot was released when the final exit status was received.

This is a passing, instrumented **clean-7f prefix diagnostic**, not a Task 31
gate, an explanation of the original failures, or permission to repair motor
code. No failing child phase or failing parent-gap trace was captured.

## Exact selection and command evidence

The retained binary's full `--list` enumerated 848 tests. The specified modules
select exactly the first 157 tests, through the final motor test: **129
predecessors plus all 28 motor tests**. An additional `--exact <all 157 names>
--test-threads=1 --nocapture --list` invocation confirmed the exact names and
order before execution. Both listing processes exited 0 and executed no tests.
The single execution used the same explicit names without `--list`.

- `selection.json` contains the complete command array and exact names.
- `selected-test-names.txt` is the same 157-name list, one per line.
- `all-tests-list.txt` and `selected-tests-list.txt` preserve both raw listings.
- `results.json` preserves every exact command, cwd, timestamp, exit code,
  external elapsed time, and source/binary identity check.
- `tests.txt` is the unmodified combined stdout/stderr. Its final result is
  at line 243; the earlier socket-using task test is at line 18; motor begins
  at line 133. Parsed executed names/order match `selection.json` exactly.

| Module | Selected tests |
|---|---:|
| admission | 15 |
| agent_tasks | 4 |
| air_told | 3 |
| attention | 12 |
| body_link | 10 |
| bus_out | 9 |
| called | 6 |
| camera_inspect | 3 |
| camera_worker | 5 |
| choreography | 23 |
| config | 12 |
| coordinator::camera | 7 |
| coordinator::cloud | 14 |
| coordinator::early_end | 5 |
| coordinator::endpoint | 1 |
| coordinator::motor | 28 |

The runner used local Unix IPC with elevated sandbox permission. It did not
use Cargo, access the lamp, contact a provider, or enter hardware paths.

## Source, binary, environment, and host

Base: `7fbb03e1f2e61ad6dfaf184cd768bad6524c2d76`.
Source: `/private/tmp/lm709-src-qhqu_pyi/lampOS`.
Only the existing scratch `crates/voice/src/coordinator/motor/tests.rs` patch
adds bounded diagnostics. Production source is unchanged. All 1,436 source
files matched the prepared manifest before/after listing and before/after
execution; the four `source-*.json` snapshots retain hashes, sizes and modes.
`instrumentation.diff` is a verbatim copy of the previously used patch.

The binary was run at its original path:
`/private/tmp/lm709-src-qhqu_pyi/target/debug/deps/lamp_voice-a98444cc1c3f37e5`.
Size: 33,316,136 bytes. SHA-256 before and after execution:
`e0a5caa128cd7912f507a7e716fc24bde94cb8148480a68a28482341b1a77b5a`.
Task 31 changes are absent. There is no binary payload in this evidence
directory. Script and binary identities are metadata only in
`retained-outside-repository.json`; their executable payloads stay outside
the repository.

`environment-list.json` and `environment-execution.json` deliberately retain
only an allowlist: PATH, TMPDIR, locale/TZ, Cargo target/toolchain labels,
Rust backtrace/test settings, loader-path settings, and the private diagnostic
root. These records matched between listing and execution. Loader-path
overrides, Rust test environment overrides, inherited child-only motor
variables, and other LAMP/Rust-test/loader variable names were absent.
`RUST_BACKTRACE=full`, `RUSTUP_TOOLCHAIN=1.96.0`, the retained target directory,
and the fresh `LAMP_MOTOR_DIAGNOSTICS` path were explicit overrides. An
inherited diagnostic socket variable would have been removed; none existed.
Other inherited environment values are intentionally **not retained**. No
full environment dump, credential, token, private key, or provider value is
included in the retention manifest.

All measurements are from the development host: Intel Core i7-10700K at
3.80 GHz, 16 logical CPUs, 64 GiB RAM, macOS 26.6.2 build 25G83 / Darwin 25.6.0
x86_64, Rust 1.96.0 (`ac68faa20`) and Cargo 1.96.0. `host-record.json` is the
verbatim existing machine provenance record; machine properties were not
queried again for this run. Nothing was measured on a lamp.

## Observed motor phases

There are **78 saved wait records from 29 recorded fixture instances**. Every
record has outcome `predicate_satisfied`; every captured child-marker sequence
is contiguous. `diagnostic-record-manifest.json` names and hashes every raw
record, and `analysis.json` maps each fixture to its test and raw log line.
The counts are recorded fixtures, not a count of every process spawned by all
selected tests.

Recorded modes: normal 16; crash 1; hang 2; gated_exit_failure 1; gate_segment
3; startup 2; profile 1; gate 1; discard 1; bad_cleanup 1.

| Measurement boundary | Observed range | Count |
|---|---:|---:|
| Fixture start call returned to captured child-entry marker | 512.248–877.122 ms | 24 |
| `after_start` checkpoint to child entry, also including the manually started startup fixture | 509.030–877.122 ms | 25 |
| Captured child entry to Ready-sent marker | 1.048–4.702 ms | 25 |
| Ready-sent marker to parent Ready-event observation | 0.126–1.650 ms | 25 |
| Core construction enter to done | 18–20 microseconds | 25 |
| Qualification enter to done | 16–60 microseconds | 25 |

The fixture's `after_start` checkpoint follows a real start call only when
mode is not `startup`. For the initial-privacy test, it precedes that test's
later explicit start, so the 509.030 ms value is a checkpoint-to-entry
interval, not a measured start-call-return interval.

The relevant earlier-failure cases all passed here. Timings below are from
this invocation only; full names and raw paths are in `analysis.json`.

| Test / mode | Checkpoint to entry | Entry to Ready sent | Ready sent to parent | Captured final child marker |
|---|---:|---:|---:|---|
| crash/stalled / crash | 877.122 ms | 4.138 ms | 1.650 ms | crash_exit_7 |
| crash/stalled / hang | 519.513 ms | 4.291 ms | 0.225 ms | hang_enter |
| explicit-current-segment / normal | 533.498 ms | 4.107 ms | 1.524 ms | worker_run_ok |
| failed-reap-after-Stopped / gated_exit_failure | 524.071 ms | 1.080 ms | 1.276 ms | ready_sent |
| late-started-callback / gate_segment | 519.020 ms | 3.459 ms | 1.464 ms | worker_run_ok |
| initial-privacy / startup (checkpoint caveat above) | 509.030 ms | 4.208 ms | 1.131 ms | worker_run_ok |

The crash trace records exit 7, followed by reaping and the expected stored
failure. The hang trace records `hang_enter`, then the expected progress
timeout and reaping. These are intended test scenarios, not failed tests.
The gated-exit-failure trace ends at initial readiness: its later custom
shutdown/reap loop is outside the sampled wait helper. Its assertions passed,
but this record does not provide detailed later phase timing.

For each first saved wait, the global maximum sampled gap was the initial
fixture setup before the `before_start` checkpoint: 38.464–42.813 ms. The
largest gap among the retained first-wait samples after setup was 1.659 ms;
this is **not** the complete-loop maximum, because only the first 16 and last
256 parent samples are retained. The global maximum accounts for all sampled
gaps, including time outside the poll loop. The largest later global gap,
402.545 ms in the profile fixture, is recorded at `wait_enter` after work
between waits; it does not establish a poll-loop stall.

Four fixture records lack child markers. Three are the permission/config
test's `normal` fixtures: each first spawns an original child and immediately
shuts it down, then tests a separate denied candidate. The originals' child
phases are unknown. The fourth is startup-announcement, which never starts
the motor and asserts that fact. Missing best-effort markers alone never
prove that no child spawned or no phase executed.

## Interpretation and limits

The preceding task-helper test ran and passed in the same process before
motor. Adding this prefix did not reproduce a timeout in this one invocation.
This does not measure retained listener threads/descriptors or clear that
helper's source-level lifetime issue. Thread counts, descriptors, host load,
loader work, and filesystem stalls were not measured.

The earlier motor-only pass used this same source/binary/path and recorded
0.515–0.839 seconds from the ordinary start-call return to child entry,
1.120–3.918 ms entry to Ready, and 0.073–1.313 ms Ready to parent observation.
This run's passing measurements again locate its largest startup interval
before child entry. They do not explain the old failing traces. Host state
and time differ; the two listing invocations also execute this image before
the selected tests and may affect caches. There is no controlled cold-load
comparison, statistical rate estimate, or causal attribution here.

The clean-7f binary lacks later dcc/Task 31 tests and changes. A failed clean
motor-only invocation already shows that the full-suite prefix is not
necessary for the inherited failure. This passing prefix cannot qualify
Task 31, erase a failing required gate, or authorize motor edits.

## Retention

Use the exact safe-file mapping in
`/private/tmp/lamp-third-motor-prefix-retention-manifest.json`, not a recursive
copy of this directory. The safe set contains only UTF-8 JSON, text, Markdown,
and the diagnostic Rust diff. `content-check.json` describes the bounded
credential-pattern and file-type checks; it is not a claim that regex alone
proves absence of every possible secret. The explicit environment allowlist
and review of its actual values provide the principal control.

Python scripts, Unix socket files, and all binary payloads remain outside
the repository. `retained-outside-repository.json` records their paths and
hashes without embedding executable content. Original diagnostic reports,
raw failures, and previous bundles were left unchanged.
