# Mood host checks, 2026-10-14

These are development-host results, not lamp results. The full Mac test run
used base `1733307d7672ad302154984952a77772f8ebd6e5`; the task was then rebased
onto `a9a7262a904d802fa54043fe43cf9e368837349b`. All 18 task code, test and
manifest hashes in [source-sha256.json](source-sha256.json) stayed unchanged.
No deployment or consumer wiring changed. The later rebase onto
`a9992f272d302e3ce79a889faa2a88c5867af79d` also preserves all 18 hashes and
clears strict Linux workspace lint. Mac checks for that base are recorded
below. A green native ARM64 run on the final task head is required before
handoff; its exact run and readiness belong in Fourth's log and pull request.

Raw outputs are retained byte for byte. The generated SVG has Matplotlib's
trailing path whitespace, and the two timing captures and final Mac test
capture end with Cargo's blank line; a full `git diff --check` reports those
artifact formatting details. The source/documentation whitespace check
excludes just those four files and
passes. No measurement or plot was reformatted to hide that check's output.

## Machine and commands

Mac: macOS 26.6.2 (25G83), x86_64 Intel Core i7-10700K at 3.80 GHz,
16 logical CPUs, 64 GiB RAM. Rust/Cargo 1.96.0. Commands run from
`/private/tmp/lamp-fourth-mood/lampOS` with this worktree's target directory.

The initial scoped gate ran, in order:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
RUST_TEST_THREADS=1 cargo test -p lamp-mood -p lamp-msgs --offline --locked
cargo build --workspace --offline --locked
```

[mac-initial-gate.txt](mac-initial-gate.txt): exit 0, 152 tests passed,
zero failed, two explicitly ignored manual timing tests. After repairing Pet's
acquisition-age bound and adding three regressions, the same gate passed
155 tests, zero failures, the same two ignored tests:
[mac-pet-gate.txt](mac-pet-gate.txt). These runs predate the final timing-fixture
session checks and the most recent upstream rebase. The full Mac coverage and
post-rebase checks are reported below; a scoped pass is not a dependent gate.

## Release-mode timing

Built the timing executable before measurement, stopped other local builds
and tests, then ran once with the final measurement fixture:

```sh
cargo test -p lamp-mood --release --test timing --offline --locked --no-run
RUST_TEST_THREADS=1 cargo test -p lamp-mood --release --test timing --offline --locked -- --ignored --nocapture
```

[mac-timing-build.txt](mac-timing-build.txt) and
[mac-timing-final.txt](mac-timing-final.txt) retain full outputs. Both manual
tests passed, none ignored. Raw arrays contain all 2,000 samples per reducer
case and all 101 bus cues; percentiles use the nearest-rank definition.
`Instant` measures elapsed host monotonic time. Sub-microsecond values below
are nanoseconds as reported by that timer, not an assertion of clock accuracy.

| Reducer boundary (nanoseconds) | First | p50 | p95 | p99 | Maximum |
|---|---:|---:|---:|---:|---:|
| New engine + apply event + returned snapshot | 3,759 | 353 | 364 | 375 | 16,047 |
| Existing engine + apply event + returned snapshot | 519 | 307 | 320 | 332 | 16,620 |
| Advance 24-hour gap + returned snapshot | 825 | 335 | 347 | 355 | 29,845 |

Fresh construction runs every iteration; it is not a cold-cache or new-process
measurement. Events alternate name and ordinary touch. The long-gap case
measures bounded analytic advancement, not one day of CPU use. Each case
meets the authored under-1-ms p95 host target.

The process fixture creates an isolated temporary bus and starts the actual
`lamp-mood` binary with `--enable --zone UTC --clock-id mood-timing-host`.
Before spawn to the first parsed, shape-checked matching-session state was
**308.050 ms**, one process startup. For 101 alternating authored name/touch
cues, before cue publication to subscriber receipt of matching Mood state:

| Unit: milliseconds | First cue | p50 | p95 | p99 | Maximum |
|---|---:|---:|---:|---:|---:|
| Cue to observed state | 22.272 | 23.561 | 29.580 | 30.408 | 31.337 |

Matching checks shape, clock ID, the initial Mood session against header boot,
increasing output sequence, monotonic sample time at/after the cue, and the
alternating attention target. The bus has only this fixture's producers.
The measurement includes input serialization, filesystem work, process/queue
scheduling and receiver parsing. It starts before input admission, so it is
a broader proxy for accepted-event-to-observed-state latency, meeting the
50-ms p95 host target for this fixture.

This is **not** `publish()` return latency or durable retained-file latency:
the bus sends datagrams before its retained-file write and API return. Review
corrected the original design target wording accordingly. No sensor, Pet
recognition delay, provider, actuator, acoustic feedback or consumer animation
is included. First/repeated bus cues share a running process; only the separate
startup value includes initial attachment.

The earlier timing run is retained in
[mac-timing-initial.txt](mac-timing-initial.txt). It passed both assertions,
with 552.990-ms startup and 28.828-ms cue p95, but matched only sample time and
attention and rounded reducer results to integer microseconds. The final
fixture adds session/sequence/shape checks and reports reducer nanoseconds.
These are two host observations with different measurement fixtures, not a
before/after runtime performance improvement. No cache flushing was performed.

## Failures and incomplete gates

The first strict Clippy attempt rejected the large `Payload::Controls` enum
variant. Boxing that variant fixed the finding; both the failure and the
successful repeat are retained as `mac-initial-clippy-failed.txt` and
`mac-clippy-repaired.txt`. Initial sandboxed Unix-socket program tests were
denied; the explicitly permitted local-socket run passed. One timing-build
invocation used the repository root instead of `lampOS` and stopped before
compilation; `timing-build-directory-failed.txt` retains that error. It was
rerun from the correct directory before collecting samples.

The initial Linux workspace strict-Clippy attempt stopped in inherited
Body `inspection.rs:615`, unused `pub(crate) fn run`, as already reported to
First for task #698. No lint was suppressed and no Body file was edited.
The local Linux source copy is `/private/tmp/lamp-fourth-mood-linux-i51f10rk`,
mounted read-only. The existing Podman VM named `lampos-arm64` is actually
x86_64 Linux 6.12.13, six virtual CPUs and 8 GiB RAM. Image
`localhost/lampos-fourth-native-check:latest`, SHA-256
`203a72b687c4c5a96633420af4eccf839ff34fa448a26258ca5c89a077d7e6ad`, runs with
network disabled and all capabilities dropped; its UID remains 0. This is
not an ordinary-user native ARM64 runner or a lamp. Build jobs are four,
tests serial, target volume `lamp-fourth-controls-target`; offline registry
and git caches are mounted read-only at the image's actual `/root/.cargo`.
The failed whole-workspace lint output is retained in
[linux-workspace-clippy-failed.txt](linux-workspace-clippy-failed.txt).
After that failure, a separate diagnostic check of Mood and Msgs themselves
passed strict Clippy and all 155 tests (zero failures, two manual timing
checks ignored). This scoped pass does not replace the failed whole-dependent
lint. The ARM64 release cross-build also passed without warnings. Commands:

```sh
cargo clippy -p lamp-mood -p lamp-msgs --all-targets --offline --locked --target x86_64-unknown-linux-gnu -- -D warnings
cargo test -p lamp-mood -p lamp-msgs --offline --locked --target x86_64-unknown-linux-gnu
cargo build --release -p lamp-mood --offline --locked --target aarch64-unknown-linux-gnu
file /target/aarch64-unknown-linux-gnu/release/lamp-mood
sha256sum /target/aarch64-unknown-linux-gnu/release/lamp-mood
```

[linux-focused-cross.txt](linux-focused-cross.txt) is complete output, exit 0.
The ARM64 ELF SHA-256 is
`d99a2b381e841bbc686cd60e6186ccddbddcf84089a666b0da85aa2ef8920b07`.
It was cross-built on x86_64, not executed, installed or measured on a lamp.

The full Mac workspace attempt passed formatting and strict workspace Clippy,
then stopped in unchanged Health
`a_part_that_is_expected_and_silent_or_has_gone_quiet_ends_show_with_3`,
`crates/health/tests/program.rs:536`. Health correctly reported that the host
disk was only 6% free; this fixture's journal assertion expected an initial
silent-Eyes status despite that warning. Five other Health program tests
passed. The complete failed command output is retained in
[mac-workspace-health-disk-failed.txt](mac-workspace-health-disk-failed.txt).

Fourth removed only its disposable judge/Records Cargo targets and Memory
debug/incremental outputs. Source, release timing artifacts and all evidence
were retained. [host-cache-reclaim.txt](host-cache-reclaim.txt) records
26.3 GiB available before and 56.5 GiB after, above Health's 10% free threshold.
No Health assertion, threshold or runtime code was changed.

To avoid repeating completed long suites, the test artifact JSON was matched
against each successful executable in the failed run. Twelve packages had
finished every ordinary test executable; 21 packages, including Health, still
needed execution or retest. Cargo metadata verified their union equals all
33 workspace members, with no unmapped passed executable. The exact plan and
commands are [mac-test-continuation-plan.json](mac-test-continuation-plan.json)
and [mac-test-continuation-command.txt](mac-test-continuation-command.txt).
The continuation retested all 21 remaining packages, ran the 12 completed
packages' outstanding doctests, then built the workspace: exit 0. The effective
coverage is **3,837 passed, zero failed, 29 ignored**, across all 33 packages:
1,205 passed / four ignored from the first 12 packages' ordinary tests, plus
2,632 passed / 25 ignored from the continuation and all doctests. The complete
continuation is [mac-workspace-continuation.txt](mac-workspace-continuation.txt);
[mac-workspace-results.json](mac-workspace-results.json) records the accounting.
The failed first run remains a failure; these commands do not skip any required
suite or count duplicate Health passes twice.

## Checks after the upstream Body repair

Upstream `652a196e1` makes Body's inspection helper test-only and removes an
unnecessary mutable binding. Those are the only runtime source changes between
the two bases above; the other upstream changes are documentation and deployment
records. On the rebased task, Mac formatting, strict workspace Clippy,
Body/Mood/Msgs tests and workspace build all passed, exit 0. The tests passed
**422**, with zero failures and two manual timing checks ignored. Full raw tool
output chunks, commands and base are in [mac-rebased-gate.json](mac-rebased-gate.json).

The next Linux workspace strict-Clippy attempt used a fresh source copy at
`/private/tmp/lamp-fourth-mood-final-linux-zwmfnq7h`, mounted read-only at
`/source_mood`, with fresh copy timestamps and a distinct Cargo source path.
The VM, image, capabilities, UID and target volume are as stated above.
Body now passes; the command stops in inherited Eyes
`crates/eyes/src/main.rs:284`, `clippy::nonminimal_bool`, on
`!permitted_at.is_some_and(|at| at.elapsed() < PERMIT_STALE)`. Both the binary
and binary-test targets report it under `-D warnings`. Exit 101 and complete
output are retained in [linux-eyes-clippy-failed.txt](linux-eyes-clippy-failed.txt).
The later tests and cross-build in that command were not reached. Eyes is
unchanged between the two bases. No Eyes file or lint suppression was added;
the finding is logged for its owner. The earlier focused Linux pass and ARM64
cross-build above cover the unchanged Mood source but do not clear this gate.

## Checks after the upstream Eyes repair

First repaired the Eyes expression in `dc5774013`. The task was rebased onto
`a9992f272d302e3ce79a889faa2a88c5867af79d`, also containing the merged Records
and judge tasks and First's new eye behavior. The changed upstream crate
closure is Eyes, Policy, Records, Ring, Voice and Voice Eval; Cargo metadata
verified the dependency closure, recorded in
[mac-post-eye-plan.json](mac-post-eye-plan.json). Mood's own 18 source hashes
are unchanged.

The new source-only copy is `/private/tmp/lamp-fourth-mood-eyes-fixed-ygiqib4e`,
mounted read-only at `/source_mood_fixed`, with fresh file timestamps. The
native Linux image, VM, UID, capability restrictions and caches are unchanged
from above. This ordered command passed, exit 0:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --offline --locked --target x86_64-unknown-linux-gnu -- -D warnings
cargo test -p lamp-mood -p lamp-msgs --offline --locked --target x86_64-unknown-linux-gnu
cargo build --release -p lamp-mood --offline --locked --target aarch64-unknown-linux-gnu
sha256sum /target/aarch64-unknown-linux-gnu/release/lamp-mood
```

Strict workspace Clippy is now clean. The focused tests passed 155, zero
failed, two manual timings ignored, across 16 suites. The cross-built binary
has the same `d99a2b38…` SHA-256 recorded above. Complete raw output is in
[linux-eyes-fixed-gate.txt](linux-eyes-fixed-gate.txt). Mac formatting, strict
workspace Clippy and workspace build also passed, exit 0, with four build
jobs and incremental compilation disabled; full output is
[mac-post-eye-compile.txt](mac-post-eye-compile.txt). The new serial Mac test
run covers Eyes, Policy, Records, Ring, Voice, Voice Eval, Mood and Msgs:

```sh
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=1 cargo test -p lamp-eyes -p lamp-policy -p lamp-records -p lamp-ring -p lamp-voice -p lamp-voice-eval -p lamp-mood -p lamp-msgs --offline --locked --no-fail-fast
```

It completed with exit 0: **1,764 passed, zero failed, 11 ignored**, across
85 suites. Other local builds and tests were stopped for this run. Complete
output is [mac-post-eye-tests.txt](mac-post-eye-tests.txt), with count
accounting in [mac-post-eye-results.json](mac-post-eye-results.json).
The unchanged motor fixture that failed the separate Memory worktree's
preceding full Mac run passed here. That does not identify that failure's
cause or erase it. No test or deadline was relaxed. These host results do
not establish native ARM64 execution.

The earlier task checkpoint `9b00b4b7ba23930b7030b4722c244e00ad345cee`
passed native GitHub Ubuntu 22.04 ARM64 run 38570324967: 3,880 tests passed,
zero failed, 31 ignored, across 261 suites, with release build and artifact
upload successful. Its full output is retained on `lamp-fourth` under
`docs/verification/arm64-ready-20261014/`. It predates this rebase; the final
head still needs its own green run, recorded in Fourth's log and PR.

## Not shown

No lamp execution, real-day replay, physical wake/sleep/mute operation, cold-device startup,
filesystem deadline, durable-publication timing or end-to-end response was
qualified by these measurements. The blocked-transport tests establish
bounded owner progress and refusal on unfinished shutdown, not a bounded
kernel syscall. Native ARM64 CI on an earlier checkpoint does not certify
the final head; its exact run must be green before a ready handoff.
