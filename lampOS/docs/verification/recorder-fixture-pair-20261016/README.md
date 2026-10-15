# Recorder fixtures on the shared serve clock

The complete `serve_session` binary passed the same **14 tests and 22
attempts** before and after: **117.106 s → 90.202 s** (22.97% less nextest
execution time). The three newly coordinated recorder cases total
**38.914 s → 7.689 s** (80.24% less). No failures, retries, ignored or skipped
cases occurred in either paired invocation.

This is an observed serial debug pair on x86_64 macOS. It does not establish
native ARM64 timing, device latency, PR wait time or the complete 123-test
under-two-minute target. A subsequent full local evaluator gate passed **457
tests**, with strict clippy, formatting and the workspace build also passing.
Native ARM64 and dependency qualification remain pending.

## Pair and preserved coverage

Before already coordinated ten serve identities and 18 attempts. This change
replaces one constructor predicate and its explanatory module comment:
recorder cases now use the same controlled branch, while the delayed transport
case keeps its original host-clock branch. After therefore coordinates **13
identities / 21 attempts**, retaining **one host identity / one attempt**.

[Exact source diff](constructor-migration.diff) reverses byte for byte to the
prior ten-case migration. All 14 original test bodies and assertion helpers,
all **114 static assertion macros**, all 18 `run` call sites and all 22 expanded
attempts remain. The five-fault loop, recorder durations/exit codes, declared
capture end, 100 ms bounds and original assertions are unchanged. Complete
14-PASS results plus that unchanged source establish the attempt count;
fourteen result lines alone would not.

| Subset | Identities / attempts | Before (s) | After (s) |
|---|---:|---:|---:|
| Newly coordinated recorders | 3 / 3 | 38.914 | 7.689 |
| Previously coordinated cases | 10 / 18 | 55.355 | 59.904 |
| Retained host transport / first preparer | 1 / 1 | 22.832 | 22.605 |

These subsets sum rounded individual PASS durations, not separate invocations.
The previously coordinated cases took 4.549 s longer in this pair; all
individual rows are retained in [comparison.json](comparison.json) and
[timings.tsv](timings.tsv), including that variation.

| Newly coordinated test | Before (s) | After (s) |
|---|---:|---:|
| `a_room_recording_that_ends_early_withholds_the_rest` | 7.125 | 2.162 |
| `a_recorder_that_exits_zero_mid_conversation_withholds_the_rest_and_certifies_nothing` | 7.047 | 2.016 |
| `a_declared_capture_end_before_the_next_prompt_and_its_window_withholds_it` | 24.742 | 3.511 |

The remaining host case is
`startup_cues_stay_fresh_through_a_slow_transport_and_a_slow_first_preparer`.
Its complete 400 ms transport delay and 2300 ms first-preparer delay remain.
The recorder lifetimes remain 6000 ms, 6000 ms and 23000 ms in logical time;
the last case still declares capture end at 20000 ms, without shortening its
23000 ms lifetime.

Both compiled listings are identical, with 14 selected tests and zero ignored
or filtered entries. The evaluator helper was identical across the pair:
`5c14437ca171ce73c3d2450d1e410926641630b784311a8cc9826d4cf65d0835`.
The current test file matches the restored after source:
`485ef6bc04c292666b633c819baa7c16465aaa756e97271c9c0708cd2c3e6186`.
[Raw helper proof](pair-helper-proof.json), both source snapshots and the
[original source](original-serve-source.rs) are retained. The test binaries
differ through constructor selection; no binaries or Python scripts are
included in this directory.

The later workspace build recompiled the helper; its current shared-target
hash differs from the paired executable. That later artifact is recorded
separately in the comparison and does not replace the saved before/after
helper proof.

## Commands, dates and host load

| Run | Started / finished from `date` (+0700, 2026-10-16) | Nextest execution |
|---|---|---:|
| Before | 00:45:20 / 00:47:18 | 117.106 s |
| After | 00:47:21 / 00:48:52 | 90.202 s |

Before ran first. Both used the following commands from `lampOS/`, with
`CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0` and
`CARGO_TARGET_DIR=/private/tmp/lampos-tests-relay-clock/lampOS/target`:

```sh
cargo nextest list --locked --offline -p lamp-voice-eval --test serve_session --message-format json
cargo nextest run --locked --offline -p lamp-voice-eval --test serve_session --profile realtime --test-threads 1 --retries 0 --no-fail-fast --status-level pass --final-status-level fail --color never
```

Nextest summaries exclude compilation/listing and shell launch overhead. Raw
logs, compiled lists, command/date/exit records are copied byte for byte;
[the manifest](manifest.json) gives their source paths and hashes.

The host was shared and load was uncontrolled. The raw helper proof says the
independent recorder-seam rebase build "may overlap". Its saved timestamps
resolve that uncertainty: it ran **00:44:03–00:45:16**, overlapping the before
listing (**00:45:05–00:45:20**), and finished before the measured before run
started. Those records establish no overlap with either measured test
invocation. They also do not establish a quiet host or equal background load.
The original broad note is preserved, with this correction in the derived
comparison.

## Qualification and real exit boundary

The implementation gate preceding the constructor swap passed formatting,
strict evaluator all-target clippy (`-D warnings`) and all **255 library tests**
in 8.67 s, with zero failures, ignores or filters. The nine scheduler tests,
recorder command/protocol tests and eleven backend recorder tests are included.
The [earlier gate records](gates.json) and raw logs show their precise scope.
The complete before/after serve binary ran afterward.

A subsequent gate on the combined local source passed **457 tests**: 255
library tests and 202 tests across 22 integration binaries. All 25 Cargo
summary blocks, including zero-test main and doc-test targets, report zero
failures, ignores, measured tests or filters. The 457 named results match those
counts. Tests ran serially within each binary from **00:51:39–00:55:33 +0700**
on 2026-10-16; the workspace build finished at **00:57:01 +0700**. Formatting
and strict all-target clippy passed before the tests. The retained
[full gate records](full-gates.json) and [test log](full-tests.log) cover:

```sh
cargo fmt --all --check
cargo clippy --locked --offline -p lamp-voice-eval --all-targets -- -D warnings
cargo test --locked --offline -p lamp-voice-eval --no-fail-fast -- --test-threads=1
cargo build --locked --offline --workspace
```

This full gate is separate from the paired timing measurement. It qualifies
the local evaluator package and workspace build; it is not a workspace-wide
test run, native ARM64 gate or complete realtime-group timing result.

The recorder's first Ack follows the real `ready.json` write. Its terminal Ack
reports completion of the full original wait; it supplies no OS-exit status,
recording, metadata or coverage decision. The backend holds logical time and
checks its actual owned `Child::try_wait` under the host deadline before
notifying the scheduler. Original ready-file parsing, coverage/refusal rules,
exit-code handling and cleanup remain authoritative. An exit code of zero
still cannot certify absent audio.

Scheduler tests use explicitly mock peer receipts over real local control
sockets; those are protocol assertions, not real-child evidence. Separate
backend tests use real children whose exits are controlled through pipes.
The [cross-boundary review](cross-review.md) reports no source blocker and
claims no runtime gate itself. Its nine inspected source hashes match the
current implementation; execution evidence comes from the retained gate and
pair logs above.
