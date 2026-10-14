# Task 33: touch strip and head, development-host evidence

This is historical evidence from before First's 20:05 +07 working-electrode
requirement. The later per-unit working-pad implementation and its focused
evidence are in [touch-working-host-20261014](../touch-working-host-20261014/).
The raw results and source manifests here remain attached to the earlier
implementation; they are not relabelled as tests of that update.

This folder records Third's work without a lamp. The machine is an Intel
i7-10700K development Mac, x86_64 macOS 26.6.2 / Darwin 25.6.0, Rust 1.96.0;
exact tool output is in `machine.json`. No GPIO, I²C, lamp, microphone, speaker,
camera or motor was accessed. Unix sockets, child processes and temporary files
are real host operations; supplied input clocks/masks are fixtures.

The tested implementation snapshot is
`42bdf65a2144e2168d2643494d8b9e9598404a35`, based on `lamp-voice` at
`7afa6876f`. `source-sha256.json` covers every changed manifest, lockfile,
runtime/test source and fixture. The first native run then found a Linux-only
type-inference error in `strip_worker.rs`; the child-loop result now has an
explicit `io::Result<()>` annotation. That change is outside the Mac's compiled
path, but it changes the source bytes and requires another native run. Final
source hashes and any rebase checks are recorded separately below. The native
ARM64 run on the final PR head must pass before readiness; Third's log and PR
name that run.

`final-source-sha256.json` records the corrected implementation bytes. The
final native run and any checks after rebasing are recorded with the handoff
in [Third's log](https://github.com/autonomous-ai/lampOS/blob/lamp-third/lampOS/docs/work-log-third.md)
and its `touch-final-host-20261014` verification folder on that log branch.
Those later results do not retroactively change the source identity of the
first host or standalone run saved here.

## What exists and what was tested

The implementation and required lamp procedure are in
[Controls](../../controls.md#touch-and-slides-task-33). The
[reuse decision](../../reuse-decisions.md#touch-strip-and-head-task-33-2026-10-14)
records the inspected V1 safeguards and primary NXP/Linux/Rust sources.

The host gate uses `cargo fmt --all --check`, strict all-target Clippy and
serial tests for Controls, Msgs and every transitive reverse dependency
(the twenty packages in `dependent-packages.json`), then
`cargo build --offline --locked --workspace`. All four stages passed:
**3,213 passed, zero failed, fifteen inherited ignored tests in 203
harnesses**. `gate-counts.json` lists the ignored cases (benchmarks, extended
simulations and one pinned-model test); none was newly disabled by this task.
Exact commands, exit codes and durations are in `gate-results.json`, with the
raw `gate-*.txt` streams. `gate-source-notes.json` distinguishes the original
compiled Mac tests from the later workspace build after the Linux annotation.
Durations are wall time on this development host, not latency or a lamp
performance claim.

After the native compile failure was corrected, the same Mac also passed
`cargo clippy --offline --locked --target aarch64-unknown-linux-gnu
-p lamp-controls -p lamp-msgs --all-targets -- -D warnings` in 13.368 seconds,
using the Rust 1.96.0 ARM64 Linux standard library and an independent temporary
target directory. This compiles and strictly lints the Linux-only code and
test targets; it executes no ARM64 binary. `linux-clippy-first.json` and
`linux-clippy-first.txt` retain the exact command, source delta and raw output.

An independent source-only copy at
`/private/tmp/lamp-third-touch-standalone-h2e3p2u8` resolved all **32 local
workspace packages inside the copy**, built the entire workspace and passed
**405 Controls/Msgs checks in 29 harnesses, zero failed/ignored**, including
the existing message doctest. The target directory was inside that copy;
ordinary Cargo registry/git caches were reused. Raw commands/results are in
`standalone-results.json`, `standalone-build.txt`, `standalone-tests.txt` and
`standalone-counts.json`. No parent source, legacy runtime or running service
was required.
`standalone-local-sources.json` also checks all **33 local packages including
the patched vendor dependency**, resolving every manifest inside that copy.

## Off without the selector

`default-off.json` records the exact binary hashes and comparison against
the unchanged pre-task Controls build from `d672844cc`. Both binaries ran
the existing `fixtures/hand-controls.jsonl` with `--clock-id written-controls`,
once in shadow and once with legacy `--enable`, **without `--touch`**.
Stdout and stderr were byte-identical in both cases: 901 records, 900
observations, zero rejected, one explicit fixture fault, six gestures,
4,601 steps and 98 frames. The retained output copies are
`legacy-{shadow,enabled}-{stdout,stderr}.jsonl`; `default-off.json` contains
their hashes. The old topic inventory, schemas and recognizers also retain
their existing tests. This is a host packet comparison; First still runs the
required switch-off minute on lamp-4ace before merging.

The new CLI tests prove that a file containing `enabled:true` still produces
shadow requests without the explicit `--enable`, replay opens no bus/hardware,
wrong clocks and malformed/legacy pin schemas are rejected, and record/output
files are private and never overwritten. The `--touch` selector itself opens
and configures the sensor in serving mode, even without `--enable`.

## Recorded versus authored input and timing

[Fixture provenance](../../../crates/controls/fixtures/strip-711-README.md)
preserves First's **57 changed masks** from lamp-4ace. Those wall timestamps
are not acquisition sequence or continuity evidence. Their strict replay
produces nine stale rejections and zero events. The 1,052-row reconstruction
adds **995 explicitly authored unchanged samples**, plus authored monotonic
coordinates, sequence/session and equal receipt time. Its exact result is
54 events: one Touch, one awake Sleep alternative, 52 Slides, zero Stroke;
zero rejected, 9,975 steps and 1,050 frames through 19,948,000 µs.

The same recognizer is exercised for first and repeated contact at five
authored sampling phases, 20 ms acquisitions and a fixed 1.7 ms receipt delay.
The tested maxima are 41.699 ms to sampled position and 61.699 ms to qualified
Touch from the authored digital edge. These are assertions under an injected
clock, not measured capacitance, GPIO, person-to-output or acoustic latency.
No interpolation, timer tick or low TTP223 gap proves a physical resting hand.

## Failures retained, and what changed

`development/` retains the raw development runs, including failed attempts.
They are diagnostic history and overlap the gate; do not sum them as unique
checks or use a later isolated pass to erase an earlier gate failure.

- A filtered/current-mask intersection created a false disconnected footprint
  during a legitimate handoff. Raw contact continuity and stable filtered
  geometry now have distinct roles.
- The same intersection could make a centroid reverse when electrodes lifted.
  The regression keeps direction tied to the complete stable footprint.
- A 20/25/35 ms qualified brief contact was lost on its release. The unchanged
  regression now preserves its one Touch without inventing a tap at slide end.
- A delayed older Hold could follow a newer Touch of the same source. The
  receiver now fences actual consumed sequence per source, while allowing
  unrelated-source and advisory events in their valid order.
- Review found queued fresh packets rescuing expired worker deadlines,
  no global I²C device claim across bus roots, malformed GPIO retaining a
  primary action, and strip-spawn failure terminating healthy GPIO. Tests now
  cover each. The final startup error route also avoids synchronous stderr
  output before an independent reader can start.
- The first Mac oversized-packet fixture tried to send 2,049 bytes, exceeding
  the host Unix datagram bound before the parent could inspect it. The wire
  protocol is now bounded to 1 KiB, fault text to 256 UTF-8 bytes, and the
  actual oversized datagram rejection passes on this Mac and is required in
  the native ARM64 gate too.
  That initial tool output reported five passing tests and one failure; its
  complete raw stream was not retained. The passing nine-test worker rerun is.
- The first complete Controls/Msgs run stopped at **329 passed, one failed**:
  a test still queried `strip_error` after the diagnostic was renamed to
  `last_strip_error`. It now checks the correctly named real sanitized error
  while still requiring working GPIO/Stop; all eleven runtime tests passed
  on recheck, then the source-only complete 405-test suite passed.
- Intermediate compile/lint failures include a diagnostic `Option<String>`
  without `Display`, three test-only cloned-reference suggestions, one
  assertion simplification, strict tagged-unit schema handling and formatting.
  Wrong-working-directory attempts and a corrected speed-fixture observation
  phase are retained as harness/expectation failures, not claimed runtime bugs.
- The first source-copy extraction helper failed before compiling because
  Python 3.9 lacks `tarfile.extractall(filter=...)`; the retry uses host `tar`.
  A restricted `sysctl` call also failed before writing machine metadata; its
  authorized read-only retry supplied the exact CPU record. No runtime was
  changed for either helper.

The earlier native **baseline** failures (missing `/bin/kill`, then root
ignoring a read-only test folder) predated this touch source. First repaired
the workflow, now native Ubuntu 22.04 ARM as an ordinary user with glibc 2.35.
Third did not weaken either inherited test. The baseline evidence remains on
`lamp-third`; it is not a green gate for this task.

The first task run, **38559498265**, on `42bdf65a`, failed before tests at
`strip_worker.rs:412` with Linux-only E0282. The explicit result annotation
above fixes the inference ambiguity; no test passed in that run. Separately,
First's corrected-runner baseline, **38556357172**, built successfully but
stopped at two unchanged Records tests with `Operation not permitted` during
`O_NOATIME` ancestor traversal: 1,866 passed, two failed and twelve ignored
through 126 harnesses, with later suites not reached. Both raw streams and the
counts are retained in `native-first.txt`, `native-baseline-first.txt` and
`native-first-results.json`. Upstream's `21c24fc0a` subsequently addresses that
Records permission failure; Third does not change Records or the workflow.

A separate read-only review of `touch.rs`, `touch_receiver.rs` and the new
message contracts found no further code defect. A second review checked the
Linux-only I²C/GPIO calls, locking, packet bounds, timeout ordering and teardown
against the pinned dependencies after the result annotation; it found no
additional concrete defect. `review.json` records the scopes and limits. The
topic spelling correction is included in the docs. Final owners still must persist deduplication, validate
the bus declaration/envelope/publisher and bind lifecycle history once at
contact start; the helper cannot establish those facts itself.

## What was not shown

All twelve calibrated physical electrodes, head wiring/polarity, a sustained
TTP223 hold, threshold/noise margins, boot-held baseline behavior, electrical
fault recovery, real I²C contention with Air, service-user lock permissions,
physical first/repeated latency, Voice applying Stop/Sleep/Wake, privacy
closure, safe body following or light quality remain for First on lamp-4ace.
The raw changed-mask trace came from First; none of its physical observations
was made by Third. No test result here is presented as a result on the lamp.
