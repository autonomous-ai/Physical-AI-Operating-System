# Schedule in Voice: Task 31 / #709 host verification

Ready for lamp-voice: no

The current task is rebased on
`1337e7e1d8b88e102ec9fd12f544841569856f75`, incorporating the new Days
Memory client and Tasks news/reply-bound changes. Memory now depends on Voice
and joins the affected-package gate. A new isolated process regression covers
a kept-out reminder followed by an ordinary exchange through the real Days
writer queue. New-source validation and readiness are recorded in
[Third's qualification log](https://github.com/autonomous-ai/lampOS/blob/lamp-third/lampOS/docs/verification/schedule-voice-qualification-20261015/README.md);
the earlier runs below retain their original identities.
[The 1337 integration record](integration-1337/README.md) retains the new
700-path source map and completed focused sequence: formatting, strict
Clippy, **136 test executions / 0 failed / 2 ignored**, and workspace build.

The preceding settled-policy runtime was based on
`6573ce6c1b594bf9c8cf7836bbd0e162c09e4cd1`.
Due reminders now use the next qualified local silence in every room state,
following [First's answer](https://github.com/autonomous-ai/lampOS/issues/709#issuecomment-6176285386).
The [room-policy record](room-policy-6573/README.md) retains the decision,
697-path frozen source map, rebase, reviews and completed focused checks:
formatting, strict all-target Clippy, **113 test executions / 0 failed / 2
ignored**, and the workspace build passed on Third's Intel x86_64 Mac.
Its complete affected-package gate failed with **1,602 passed / 1 failed /
8 ignored**; the unchanged motor authority assertion passed once alone.
The fresh standalone comparison passed **287 / 0 / 2**, a workspace build
and 144 exact stdout comparisons across 72 default-off cases. Native ARM64
run [38626209078](https://github.com/autonomous-ai/lampOS/actions/runs/38626209078)
passed for `01192ba0fbfda04afb19e1db63aa86423c21fd6d` with **4,338 / 0 / 33**
across 279 harnesses. [Retained terminal evidence](https://github.com/autonomous-ai/lampOS/blob/lamp-third/lampOS/docs/verification/schedule-voice-qualification-20261015/README.md)
separates the failed gate, isolated pass, standalone result and native result.
None of these is a test on the newer `1337`-based source.

[Native ARM64 run 38620759412](https://github.com/autonomous-ai/lampOS/actions/runs/38620759412)
passed for the preceding `c0bd596ac15ca64d7dc09534a65165e2be1eab03` checkpoint,
which still used the earlier room policy. Its
[terminal result and original log](native-c0/README.md) are retained. It does
not qualify the revised runtime; native CI must pass on the offered task head.
First assigned the untouched host motor fixtures to Fourth's
[#739](https://github.com/autonomous-ai/lampOS/issues/739) and permits a
handoff to list full-suite failures that pass alone, with raw evidence.
That exception does not turn any failed attempt into a pass. No ready pull
request has been opened at this checkpoint. Later readiness and validation
are recorded in [Third's log](https://github.com/autonomous-ai/lampOS/blob/lamp-third/lampOS/docs/work-log-third.md).

The preceding c0 task checkpoint was based on
`c1b8c75916cb2d6861a0c5c38d178f9dffeed4bb`. That rebase adds only upstream
`docs/tasks.md`; [all 671 task/evidence files](rebase-c1.json) survived
byte-for-byte. Its [697-path source map](source-c1.json) has exactly the same
Rust/manifest hashes as the tested d8 source. [Retention](c1-integration-retention.json)
records that comparison. No command below is relabeled as a c1 run.

The earlier completed integration checks used source based on
`d8e9556dea3f1867a5af5297d1ebddbd165d9102`, including the merged Touch
task and newer Voice/Body changes. Its [697-path frozen map](source-d8.json)
identifies the post-integration source. The [rebase record](rebase-d8.json)
preserves 505 of 508 Task 31/evidence files byte-for-byte; only the lockfile,
coordinator and shared reuse document incorporated upstream edits. The
[focused integration sequence](focused-d8-summary.md) passed: formatting,
strict all-target Clippy, 870 test executions / zero failures / two ignored
timing probes, and the workspace build. All 697 source hashes stayed unchanged.
The [integration review](merge-review-d8.md) found no Task 31 rebase defect;
the separately reported upstream early-end/news-presence defect was still
present in that source. First fixed it in the newer 6573 integration.
Those focused checks did not establish a passing complete host gate or a
native ARM64 result; the later c0 run above has its own identity.

The [d8 standalone comparison](d8-standalone/README.md) also passed once:
the independent workspace/helper builds, 282 test executions / zero failures /
two ignored probes, and 72 switch-off cases / 144 exact stdout comparisons
against a fresh clean d8 copy. Both copies resolve all 34 local manifests
inside their own trees. The implementation copy matches all 697 frozen paths;
no invalid relative Schedule path was created. Its exact readings and hashes
are retained separately from the earlier bc3 comparison.

The preceding development source was based on
`1a38f2f3e6b8afd1e7896761a0da6149a2400f67`; its
[676-path frozen map](source-1a.json) differs from the bc3 map only in
upstream `crates/tasks/tests/harness.rs`. All Task 31 bytes survived the
[rebase](rebase-1a.json) unchanged. Its complete gate **failed**: **1,116
passed, 5 failed, 8 ignored across 16 completed harnesses**, exit 101 after
683.341811104 s. Voice lib reported 923 passed, 5 failed and 8 ignored in
491.89 s. Formatting (3.745649047 s) and strict all-target Clippy
(36.722263178 s) passed; the workspace build was not reached. All 676 frozen
source hashes stayed unchanged. **Task 31 remains not ready.** The
[failure summary](1a-gate-failure-summary.md),
[original result](raw/lamp-third-schedule-voice-1a-gate-results.json),
[test log](raw/lamp-third-schedule-voice-1a-gate-tests.txt), and
[retention index](1a-gate-failure-retention.json) preserve this attempt.

The preceding `bc3fccb546a22baaef180fcd57b3291a44e0e4c1` source includes
the reminder ordering correction and incoming task-news/CHECK integration. Its
[676-path frozen source map](source-bc3.json) stayed unchanged throughout
[the focused checks](focused-bc3.json): formatting and strict all-target
Clippy passed; the singular `reminder` filter passed 70 tests with two
ignored timing probes, `second_look::` passed eight, and `tools::` passed
18, with no failures. These are separate filtered runs, not a complete
gate. The [merge review](merge-review-bc3.md) records its narrower source
inspection limits. Its isolated source copy also passed the workspace and
helper builds, 282 test executions (zero failed, two ignored probes), and
72 default-off cases with 144 exact stdout comparisons. The
[source-copy evidence bundle](bc3-standalone/README.md) retains its bc3 identity.
The initial restricted
copy run remains failed: 16 Actions tests passed and 13 failed, nine with
explicit socket PermissionDenied and four with readiness failures. The
unchanged permitted run does not retroactively qualify that first attempt.

The earlier complete Task 31 gate on base
`dcc2e6b1ee67e11e2de89f893696432e47c4d8c5` **failed**: 1,111 passed, 3 failed,
8 ignored across 16 completed harnesses; test command exit 101 after
658.218369088 s. Voice lib reported 918 passed, 3 failed and 8 ignored in
491.25 s. Formatting and strict all-target Clippy passed; the later workspace
build stage was not reached. **Task 31 is not ready.** The original
[gate result](raw/lamp-third-schedule-voice-dcc-gate-results.json),
[test log](raw/lamp-third-schedule-voice-dcc-gate-tests.txt), and
[675-path source map](raw/lamp-third-schedule-voice-dcc-gate-source.json) are
retained verbatim.

Separately, source review found a real Task 31 priority defect involving
pending camera/SecondLook work. The bc3 source corrects it and the two new
sequence regressions passed within the focused checks above. The recorded
dcc map belongs to the failed attempt, not to subsequent edits. The incoming
short-sound contract was reviewed in
[merge-review-dcc.md](merge-review-dcc.md); that review's stated scope does not
qualify the separate camera-priority behavior.

The first dcc `reminders`-filtered run had passed 62 tests, failed none and
ignored two timing probes; its command took 47.502554326 s (18.36 s reported
test time). It did not replace the complete gate. A fresh dcc baseline and
source-copy comparison never ran; their prepared scripts remain unexecuted.
The newer clean bc3 baseline and implementation-copy comparison both passed
their 72 cases; these are narrower checks than the required complete gate.

The earlier 7f source-only copy passed its workspace build, 280 test executions
and 72 default-off cases comprising 144 exact stdout comparisons. The whole
7f evidence folder is preserved unchanged under
[earlier-7fbb03e1f](earlier-7fbb03e1f/README.md). Those checks describe the earlier
source, before the incoming short-sound/provider-end contract. The 7f full gate
and focused motor failures remain retained, including failures on a clean 7f
baseline without Task 31. Later isolated diagnostic passes did not reproduce
or explain those timeouts.

The later [ordering audit](motor-fullsuite-order-audit.md) narrows two dcc
failures to initial Ready waits, without identifying a cause. It also
corrects the earlier report's claim that four markerless fixtures spawned
no child: [three original children were spawned](motor-diagnostic-correction.md),
but their entry phase was not captured. Original report bytes, counts and
timings remain unchanged. The proposed single
[prefix-plus-motor diagnostic](motor-prefix-clean7f/README.md) subsequently
passed 157 tests, including all 28 motor tests, in 21.147252753 seconds
externally measured. Its 78 saved waits all reached their predicates. It
used the already-built, instrumented clean-7f binary, with the original
assertions/deadlines, and ran no retry. It captured no failing phase and
does not explain or clear the complete-gate failures. The
[pre-copy inventory](motor-prefix-clean7f/retention-manifest.json) and
[copy receipt](motor-prefix-clean7f/copy-receipt.json) retain 98 original
safe text files unchanged; scripts, sockets and binaries remain outside
the repository.

This records host checks performed on 2026-10-14 UTC, extending into
2026-10-15 in Asia/Ho_Chi_Minh. No result qualifies a lamp, real provider,
acoustic path, privacy switch or power-cut recovery. Only the explicitly
identified CI runs above qualify their own native release builds and tests.
The existing files named `source-final.json`, `standalone-final.json`, and
`default-off-*` keep their original 7f identities; no dcc result is assigned
to them. Prior evidence indexes and their count assertions remain historical,
unchanged records. [dcc-evidence-retention-first.json](dcc-evidence-retention-first.json)
indexes the new checkpoint, preserved 7f folder and diagnostic bundle.

## Source and gate status

| Evidence | Source and result |
|---|---|
| [Revised room-policy source and checks](room-policy-6573/README.md) | Base 6573; 697 frozen Rust/manifest paths. Formatting, strict all-target Clippy, 113 test executions / 0 failed / 2 ignored, and workspace build passed. Complete affected-package tests, standalone comparison and native CI on the offered head remain pending at this checkpoint. |
| [Earlier native c0 run](native-c0/README.md) | Run 38620759412 passed for c0 on Linux ARM64. It does not qualify the later room-policy changes. |
| [Earlier c1 source map](source-c1.json), [rebase record](rebase-c1.json), and [retention](c1-integration-retention.json) | Only upstream `docs/tasks.md` changed from d8; all 697 recorded Rust/manifest hashes match the tested source. All 671 task/evidence files survived unchanged. The c0 native run later passed with this source. |
| [Tested d8 source map](source-d8.json), [rebase record](rebase-d8.json), [focused checks](focused-d8-summary.md), and [retention](d8-focused-retention.json) | 697 frozen paths at `d8e9556dea3f1867a5af5297d1ebddbd165d9102`, unchanged through the 12-stage sequence. Formatting, strict all-target Clippy, 870 passed test executions / 0 failed / 2 ignored across 60 harnesses, and workspace build passed. These are defined post-integration checks, not a complete Voice gate or native ARM64 result. |
| [d8 standalone evidence](d8-standalone/README.md) and [retention index](d8-standalone/retention-index.json) | 1,614 copied source files plus two exact helpers, 34 locally resolved package manifests. Both runners passed on their first permitted executions: workspace/helper builds, 282 test executions / 0 failed / 2 ignored, and 72 cases / 144 exact stdout comparisons. These checks do not clear the retained complete-gate failures. |
| [Failed 1a source map](source-1a.json), [gate result](raw/lamp-third-schedule-voice-1a-gate-results.json), [failure summary](1a-gate-failure-summary.md), and [retention index](1a-gate-failure-retention.json) | 676 frozen paths at `1a38f2f3e6b8afd1e7896761a0da6149a2400f67`, unchanged through the gate. **FAILED:** 1,116 passed / 5 failed / 8 ignored, 16 completed harnesses; test command 683.341811104 s, exit 101. Formatting and strict Clippy passed; workspace build not reached. |
| [bc3 standalone evidence](bc3-standalone/README.md) and [retention index](bc3-standalone/retention-index.json) | 1,455 copied source files plus the two exact helpers; 34 local manifests resolve inside the independent copy. Permitted run: workspace/helper builds, 282 test executions / 0 failures / 2 ignored probes, and 72 cases / 144 exact stdout comparisons passed. The first restricted run failed 13 Actions tests and stopped; its unedited output remains separate. |
| [Preceding bc3 source map](source-bc3.json), [focused results](focused-bc3.json), [rebase record](rebase-bc3.json), and [retention map](bc3-focused-retention-first.json) | 676 frozen Rust/manifest paths. Formatting passed in 3.760165569 s; strict all-target Clippy for Actions, Schedule, Voice and Voice-eval passed in 33.976055956 s. Three filtered Voice runs: `reminder` 70 passed / 0 failed / 2 ignored, `second_look::` 8 / 0 / 0, and `tools::` 18 / 0 / 0. These do not clear the inherited motor gate failures. |
| [Failed-attempt dcc source map](source-dcc.json), [gate result](raw/lamp-third-schedule-voice-dcc-gate-results.json), and [rebase record](rebase-dcc.json) | 675 frozen Rust/manifest paths at `dcc2e6b1ee67e11e2de89f893696432e47c4d8c5`. Gate **FAILED:** 1,111 passed, 3 failed, 8 ignored, 16 completed harnesses; workspace build not reached. No dcc standalone-copy result. The rebase record preserves the original stash-apply exit 1; this map identifies the checked source after integration and before the subsequent camera-priority correction. |
| [First dcc focused result](focused-dcc.json) and [raw log](raw/lamp-third-schedule-dcc-focused-first.txt) | `cargo test --offline --locked -p lamp-voice --lib reminders -- --test-threads=1`: 62 passed, 0 failed, 2 ignored; exit 0. `RUST_BACKTRACE=1`; all recorded source hashes unchanged. The plural `reminders` filter is narrower than the prior singular `reminder` run, so the counts are not directly comparable. |
| [First source map](source-first.json) | Base `3773c1fb88190fb5289bf1378eaa1f5fe5d134cb`; 652 recorded source paths. This is the earlier implementation snapshot. |
| [Intermediate d5 source map](source-d5-intermediate.json) | Base `d5d765862ab1f73629ee7a2c5a9a8086f0246d11`; 675 recorded source paths, including the Linux child-scheduling correction. |
| [d5 rebase record](rebase-d5.json) and [7f rebase record](rebase-7f.json) | Preserve the transitions between bases. Earlier source maps and results retain their original identities. |
| [First complete gate](raw/lamp-third-schedule-voice-gate-first-results.json) | Failed: 1,111 passed, 1 failed, 8 ignored across 16 completed harnesses; exit 101, command 656.960286296 s. Voice lib had 918 passed, 1 failed, 8 ignored. |
| [Intermediate complete-package gate on d5](raw/lamp-third-schedule-voice-final-gate-results.json) | **FAILED:** 1,111 passed, 2 failed, 8 ignored across 16 completed harnesses; exit 101, command 676.865360202 s. Voice lib had 918 passed, 2 failed, 8 ignored in 491.59 s. The build stage was not reached. All 675 source paths were unchanged. The original `final-gate` filenames describe this intermediate attempt, not a passed final gate. |
| [Earlier 7f frozen source map](source-final.json) | Base `7fbb03e1f2e61ad6dfaf184cd768bad6524c2d76`; 675 Rust/manifest paths. All matched both the source copy and the main workspace during evidence verification. The complete-package gate on this source is **not qualified**; focused motor failures are retained below. |
| [Earlier 7f standalone source copy](standalone-final.json) and [byte comparison](default-off-comparison.json) | **PASSED:** offline locked workspace build; 280 passed test executions, 0 failed, 2 ignored timing probes across four test commands; 72 default-off cases / 144 stdout comparisons exactly equal. This is a defined subset, not the complete host gate. |

All four retained failed complete gates used:

```sh
cargo test --offline --locked -p lamp-actions -p lamp-schedule -p lamp-voice -p lamp-voice-eval -- --test-threads=1
```

Those four packages are the changed packages and the identified reverse
dependencies. Earlier successful formatting and strict all-target Clippy do
not override the failed test stage. Final gate claims must use the final
source map and the final run, not the first snapshot.

Upstream `7fbb03e1f` adds an own-socket bind/chmod startup-race correction in
IPC/Voice transport with tests/docs. The [motor audit](motor-d5-failure-audit.md)
finds no demonstrated causal link to the retained motor timeouts. A clean 7f
baseline still failed focused motor tests; successful earlier narrow retries
do not identify or fix their cause.

## Host and measurement provenance

[machine.json](machine.json) records Intel Core i7-10700K at 3.80 GHz,
16 logical CPUs, 64 GiB RAM, macOS 26.6.2 / Darwin 25.6.0, x86_64, rustc
1.96.0 (`ac68faa20`) and Cargo 1.96.0. Ordinary commands ran from `lampOS`;
scratch-copy commands record their distinct working directories. No lamp,
provider account, microphone, speaker, camera or actuator was used here.
Cargo elapsed times are build/test durations, not interaction latency.

The timing probes used debug test binaries. Their first samples are separated
from warm samples. They assert functional behavior, not a hard OS or flash
latency guarantee. Source hashes and authored queue conditions are retained
with the poll measurements. These are development measurements, not a claim
that the final source set was rebenchmarked after every later edit.

## Earlier 7f default-off comparison

The retained earlier [baseline capture](default-off-baseline.json) is the clean
`7fbb03e1f2e61ad6dfaf184cd768bad6524c2d76` source. The earlier
[d5](earlier-d5d765862/default-off-baseline.json) and
[3773](earlier-3773c1fb/default-off-baseline.json) captures remain intact with
their original reading maps and payloads. The 7f baseline's source-only helper
build passed offline/locked in 32.623818552 s. Cargo metadata resolved all 34
local package manifests inside that copy; external crates came from the host
cache. The baseline contains 1,436 tracked source files plus two scratch
helpers; the implementation copy contains 1,454 source files after 46 exact
task overlays, plus the same two helpers.

All 72 baseline cases passed: two existing-tool profiles, each with the full
6 × 6 product of absent, `off`, `ON`, `true`, `1`, and ` on ` values for
`LAMP_SCHEDULE_TOOLS` and `LAMP_SCHEDULE_SPEAK`. One profile includes existing
Memory/clock tools; the other also includes existing Tasks declarations.
Both Schedule paths are deliberately invalid relative paths and remain inert.
No case enables either Schedule switch.

The probes capture actual public Actions catalog bytes and Voice environment
arguments, tool declarations, route choices, and ordinary Written serialization.
They do not execute an Actions tool, start a provider or Schedule service,
contact Harness, or run audio. They do not prove runtime silence or latency by
themselves; separate runtime tests cover absent speaking-switch behavior and
optional-child isolation.

[default-off-probes.md](default-off-probes.md) contains the exact two scratch
Rust helpers and reproduction steps. Both revisions must use identical helper
bytes, the recorded per-case environments, and the same fixture catalog path;
path bytes are part of the output. Build each source copy with:

```sh
cargo build --offline --locked -p lamp-actions -p lamp-voice --example schedule_off_actions --example schedule_off_voice
```

All 72 implementation cases completed with exit zero, empty stderr, no
created invalid relative Schedule paths and exact byte equality against the
matching 7f baseline. [default-off-baseline-files.json](default-off-baseline-files.json)
and [default-off-final-files.json](default-off-final-files.json) map all 144
readings from each stage to the three shared raw payloads in
`default-off-bytes/`. Each entry identifies stage, profile, both switch values,
role, original path, retained path, byte count, SHA-256 and empty-stderr result.
Nothing was normalized. The [comparison report](default-off-comparison.json)
retains every case; [standalone-evidence-index.json](standalone-evidence-index.json)
maps all 594 retained original files and classifies all 346 absolute paths in
the original final report. Excluded archives, scripts, binaries and target trees
are identified rather than copied into the repository.

## Functional evidence already obtained

These suites overlap. Do not sum them into a unique-test total or attribute
their earlier source to the final frozen map.

| Focused scope | Result and raw evidence |
|---|---|
| Actions server and adapter | [33 passed](raw/lamp-third-709-actions-first.txt), including 14 Schedule cases: all six mappings, malformed arguments before IPC, escaped/bounded pagination, CAS conflict, lost reply, slow dependencies and persistence refusal/ambiguity. |
| Shared bounded Schedule client | [7 passed](raw/lamp-third-709-client-first.txt). The Linux-only full Unix accept-queue case was not compiled or run on this Mac. |
| Durable delivery ledger | [24 passed](raw/lamp-third-schedule-delivery-second.txt), including restart, exact old-Ack replay, and recurring-occurrence separation. |
| Real Consumer with Schedule/Store fixtures | [20 passed](raw/lamp-third-709-consumer-first.txt), covering freshness, cancellation/snooze/store withdrawal, persistence and lost-Ack recovery. |
| Voice tools through real Actions | Five new integration cases passed within [16 tools tests](raw/lamp-third-schedule-voice-tools-first.txt). |
| Consumer/manager/coordinator/speaker-control round trip | [4 passed, 0 failed, 0 ignored](raw/lamp-third-709-roundtrip-first.txt), 3.29 s. |
| Combined reminder filter after the earlier rebase | [68 passed, 0 failed, 2 ignored](raw/lamp-third-schedule-post-rebase-reminders-first.txt), 15.17 s. Both ignored timing probes were separately run and passed; see the timing records below. |

The four round-trip tests traverse a real Schedule Engine and Store, a real
durable delivery Store, Bus reads, claim descriptor transfer and datagrams,
the actual Reminders manager, coordinator admission, and real speaker-control
handling of authored partial multi-generation PCM/retirement receipts. No
`QualifiedCompletion` is fabricated by the fixture. They establish:

- no Ack after only the first generation retires or after final PCM acceptance;
- Ack only after the qualified whole reply completes;
- interruption or microphone denial after final acceptance leaves the occurrence
  pending;
- durable completion survives reopen, and a lost committed Ack is reconciled
  without another speech offer.

The speaker receipts are authored test input, not physical acoustic evidence.
Successful provider acceptance, an idle model, or a bus send is not treated
as completed speech. No exactly-once-heard guarantee is claimed.

Exact focused commands/exit records are in [focused-results.json](focused-results.json)
and the raw logs. The parent and transport polling probes were explicitly run
with `--ignored`; their passes are not hidden inside the 68-test count.

## Timings and their exact boundaries

The Actions fixture measures client entry before connecting to a real Actions
socket through parsing its durable Schedule response. It uses a real Schedule
service, private SQLite `FULL` persistence and an injected Schedule clock.
Each run has one first mutation after both newly started services are ready,
then 20 repeated calls; **21 successes and 0 failures per run**. Service/store
startup, model time, provider calls and audio are outside the interval.

| Actions → Schedule round trip, ms | First | Repeated p50 | Repeated p95 | Repeated maximum |
|---|---:|---:|---:|---:|
| [Initial run](raw/lamp-third-709-actions-first.txt) | 135.160 | 88.388 | 143.321 | 143.420 |
| [Repeated fixture run](raw/lamp-third-709-actions-fixture-second.txt) | 134.044 | 89.074 | 146.202 | 146.297 |

The healthy-local adapter target is 250 ms. Its absolute request deadline is
1,250 ms across connect/write/read and all Status/Get/mutation hops. These
numbers do not make a stalled kernel/filesystem operation impossible or imply
that spoken confirmation takes 250 ms.

[parent-poll.json](parent-poll.json) measures `Instant` immediately before
`Reminders::poll` through return, including bounded datagram work, JSON,
state checks, and settlement encoding/sending. Each case has one first and
100 warm samples. Idle/incoming first samples include claim descriptor receipt;
outgoing setup already received it. Peer filling is outside the interval.

| Parent poll, µs | First | Warm p50 | Warm p95 | Warm maximum |
|---|---:|---:|---:|---:|
| Idle | 73.175 | 2.002 | 2.717 | 3.912 |
| Full incoming offers | 338.384 | 72.7705 | 75.312 | 125.553 |
| Full outgoing queue, completion waiting | 43.945 | 28.0785 | 28.663 | 65.043 |

[transport-poll.json](transport-poll.json) measures the smaller
`Transport::poll` call against anonymous in-process datagram peers. First
samples include claim receipt/CLOEXEC; it excludes JSON encode/decode and
coordinator work. Again there are one first and 100 warm samples per case.

| Transport poll, µs | First | Warm p50 | Warm p95 | Warm maximum |
|---|---:|---:|---:|---:|
| Idle | 66.747 | 0.9555 | 1.073 | 1.586 |
| Saturated incoming | 14.256 | 2.3105 | 2.694 | 3.007 |
| Peer withholds outgoing reads | 10.123 | 0.974 | 1.021 | 1.392 |

Both poll measurements exclude child process creation, claim/file opening,
ledger commits, Schedule socket calls, providers, the complete coordinator
iteration and hardware. The cooperative 2 ms poll target is not a hard timing
assertion. No other Third build/test ran during the parent probe; unrelated
host load was not controlled. No measurement establishes acoustic latency or
cancel-to-silence time.

## Linux scheduler correction and verification limit

The source audit found that the optional child could inherit the checked-in
Voice unit's RR20 scheduler: a new process group does not demote it. The child
now uses safe `thread-priority = 3.1.1` only on Linux to select `SCHED_OTHER`,
static priority 0 and nice 19, then reads the result back. This happens after
the exact speaking enable gate and before paths, claim or storage. Any error
leaves the optional child unavailable; parent/audio scheduling is untouched.
The non-Linux path is a no-op. This gives persistence less CPU under load;
it is not a measured audio improvement. See [the source/API audit](linux-scheduler-audit.md).

[scheduler-host-check.json](scheduler-host-check.json) records successful
`cargo check -p lamp-voice --tests`, command 33.01705593 s. It resolved only
one new locked package, thread-priority3.1.1; the Mac check does not compile
the Linux branch.

[linux-scheduler-compile.json](linux-scheduler-compile.json) records a
byte-identical copy of the scheduling module and Linux-only isolated-thread
test, SHA-256 `1e168a42221dddd2352a99ef636311156c98bcf1f693e4a31b081aae7f34caac`,
in a standalone Rust helper. These ARM64-target commands passed:

```sh
cargo +1.96.0 check --offline --target aarch64-unknown-linux-gnu --tests
cargo +1.96.0 clippy --offline --locked --target aarch64-unknown-linux-gnu --all-targets -- -D warnings
```

They took 3.887204222 s and 0.246023949 s respectively. This is compilation
and Clippy of that module/test only: no Linux execution, linking, complete
ARM64 Voice build, actual RR20 transition, or lamp result. The authored test
checks real set/readback on an isolated thread and unchanged parent scheduling;
it has not run in this record. It is not counted as a passed Linux test.

## Failures retained

The latest 1a complete attempt adds five motor fixture timeouts, all at
`motor/tests.rs:441` with `owned child timed out: None`. Four backtraces reach
the initial Ready waits (callers 905, 1001, 782 and 503); the crash/stalled
case reaches the reaped wait at caller 645, with the loop mode unrecorded.
The [source-checked failure summary](1a-gate-failure-summary.md) records the
exact cases and phases. No cause is established. Its
[raw gate files](1a-gate-failure-retention.json) and after-run binary identity
are separate from earlier failures and bc3 subset checks.

The dcc complete attempt adds three failures under
`coordinator::motor::tests`, all at `motor/tests.rs:441` with
`owned child timed out: None`:

| Failed case | Backtrace wait call |
| --- | --- |
| `crash_and_stalled_control_never_allow_replacement_or_false_cleanup` | `motor/tests.rs:645` |
| `late_started_callback_cannot_restore_an_offer_after_owner_replacement` | `motor/tests.rs:865` |
| `native_head::native_head_initial_privacy_acquisition_is_not_a_revoke_or_grant` | `motor/tests/native_head.rs:491` |

Its [exact command/exit record](raw/lamp-third-schedule-voice-dcc-gate-results.json)
retains `RUST_BACKTRACE=1`, formatting 3.833277004 s, Clippy 34.057569004 s,
and the failed test command 658.218369088 s. Each stage reports the frozen
source unchanged during that attempt. No cause is inferred from `None`, no
failed result is relabeled as a pass, and no later workspace build is implied.
[dcc-gate-failure-retention.json](dcc-gate-failure-retention.json) maps these
exact files and the merge review to their retained copies. The separate
pending-camera/SecondLook finding was a source-review defect; this record does
not invent a pre-fix failing test transcript for it.

[early-raw-files.json](early-raw-files.json) indexes retained development logs;
the original red files remain beside their later passes in `raw/`. Failures
include incomplete shared-file compilation; a missing test module; unavailable
Darwin `mknodat` in a test; a fake-provider fixture missing its asking-turn
terminal; stale fixture ownership and a wrong authored recurrence offset;
strict Clippy findings; restricted-sandbox socket denial; and scratch archive
preparation using an unsupported Python keyword. Do not present an environment
denial as an automatic approval rejection or a production failure.

One substantive transport defect was observed: a saturated Darwin datagram
pair returned ENOBUFS. The production adapter now retains that unsent packet
as backpressure. [First run](raw/lamp-third-schedule-consumer-transport-first.txt):
16 passed, 1 failed, 1 ignored. [Corrected run](raw/lamp-third-schedule-consumer-transport-second.txt):
17 passed, 0 failed, 1 ignored. The ignored cost probe passed separately.

Source review also corrected reminder camera-continuation fencing, use of the
current admission timestamp, typed-message freshness ordering and same-revision
lost-Ack recovery. Dedicated passing regressions exist; no pre-fix red run is
invented for these findings. The scheduler finding above is likewise a
source-review correction rather than an observed audio failure.

The first full-gate failure is retained in
[the original test log](raw/lamp-third-schedule-voice-gate-first-tests.txt):
`coordinator::motor::tests::frozen_config_and_executable_are_exact_and_source_changes_are_not_adopted`
timed out at `motor/tests.rs:441` with `owned child timed out: None`.
The [read-only audit](motor-failure-audit.md) found the inherited motor path
unchanged and the fixture directly launching `inert_child`, outside the
reminder path. Its two-second test wait and the three-second production
startup allowance do not prove the cause.

[Exact retry](raw/lamp-third-schedule-motor-exact-first.txt) passed 1 test in
1.77 s; [module retry](raw/lamp-third-schedule-motor-module-first.txt) passed
28 tests in 37.20 s, both on unchanged source. [Retry metadata](raw/lamp-third-schedule-motor-retry.json)
records command times 3.67886018 s and 38.499790905 s. These passes preserve
the original failure and do not turn that complete gate green.

The later d5 complete gate also failed, in two different inherited cases:
`crash_and_stalled_control_never_allow_replacement_or_false_cleanup` and
`failed_reap_after_stopped_is_latched_and_reported_once`, under
`coordinator::motor::tests`. Both printed the same line-441 timeout and `None`.
The earlier `frozen_config...` case passed in this run. The original d5
[test log](raw/lamp-third-schedule-voice-final-gate-tests.txt),
[result](raw/lamp-third-schedule-voice-final-gate-results.json), and
[source map](raw/lamp-third-schedule-voice-final-gate-source.json) are retained
under their original filenames as intermediate evidence. The
d5 formatting and Clippy stages passed in 3.910192944 s and 43.095510111 s;
neither qualifies its failed tests or the unattempted later build stage.

The follow-up [d5 motor audit](motor-d5-failure-audit.md) records that the
`failed_reap_after_stopped...` failure occurs before shutdown, at its initial
readiness wait. The crash/hang test waits for reaping but the failed subcase
is not logged. Its shared 2 s helper checks time before polling; the supervisor
allows 3 s startup. Delayed startup or delayed polling is consistent with the
reported `None`, but neither is measured or established. The d5 motor channel
already guarded the ordinary bind/chmod interval, so do not attribute these
failures to the new 7f IPC fix without further evidence.

The 7f focused motor run then failed with 27 passed, 1 failed, 0 ignored
(40.38 s tests; 70.988669401 s command). Its backtrace locates the crash/hang
case's initial reaped wait. A separate clean 7f source copy without Task31
also failed: 25 passed, 3 failed, 0 ignored (41.22 s tests; 80.425533549 s
command), in that reaped wait and two initial readiness waits. Both runs are
retained: [integrated log](raw/lamp-third-schedule-7f-motor-first.txt),
[integrated metadata](raw/lamp-third-schedule-7f-motor-first.json),
[clean log](raw/lamp-third-schedule-7f-clean-motor-first.txt), and
[clean metadata](raw/lamp-third-schedule-7f-clean-motor-first.json).
The clean result establishes an inherited failure without Task 31, while its
cause and correction remain unresolved in this record. The baseline source
hashes remained unchanged. Diagnostic instrumentation and any future findings
must retain their own source identity and cannot substitute for a passing
complete gate. The separately completed 7f standalone subset is recorded above.

The later [clean-7f motor diagnostics](motor-diagnostics-clean7f/README.md)
retain three exact diagnostic passes and one 28-test module pass from an
isolated clean source copy with test-only instrumentation. The earlier timeout
was not reproduced and its cause remains unproven. No production code, deadline
or assertion was changed. The complete 127-file, 8,991,986-byte text bundle is
copied exactly according to its
[retention manifest](motor-diagnostics-retention-manifest.json), including the
original failed logs, 88 wait records, instrumented source diff and source maps.
Those results qualify neither dcc nor the Task 31 complete gate.

The [chronological failure inventory](failure-inventory-first.md) is retained
byte-for-byte from its original scratch record. Its statements that the final
standalone result was pending describe that earlier checkpoint; the completed
result above supersedes them. The failed attempts and original logs themselves
are not rewritten. An earlier inventory audit found all 64 referenced scratch
files present, counts matching across 24 test logs, and all 26 restricted-sandbox
failed names matching the original log. The newer scheduler host-check JSON
supplied the exact invocation previously absent. The
[evidence retention index](evidence-retention-index.json) maps retained raw
files and audit reports to their originals and hashes.

## First's lamp handoff

Use [Task 31 lamp qualification](../../schedule.md#task-31-lamp-qualification)
for the exact existing service paths, per-unit environment snippets, disable
procedure, spoken cases and measurement boundaries. The
[Actions adapter contract](../../actions.md#optional-reminders-and-timers) and
[Voice tool contract](../../tools.md#schedule-tools-and-due-speech) describe the six functions and truthful
uncertain-commit response. The base service's
[build/run instructions](../../schedule.md#task-6-service-verification-and-handoff)
remain separate from Task 31 qualification.

After the required checks and native ARM64 CI on the offered head pass, with
any unchanged host-fixture exception recorded under First's #739 decision,
First should use that reviewed CI artifact, keep both switches absent for an
initial ordinary-conversation minute, then
enable the exact `on` values with the installed Schedule socket and a private
persistent delivery ledger. The documented lamp-4ace paths are
`/run/lampos-schedule/call.sock`, `/run/lampos-actions`, and
`/var/lib/lampOS/voice/schedule-delivery`; actual UID ownership must be checked.
The delivery ledger belongs in the personal-data reset manifest. Retain the
ledger and persistent claim file through uncertain delivery and restarts.

Qualify first/repeated timers; list/cancel/snooze during preparation; delivery
at local silence in Busy, Unknown and Quiet; privacy and short-utterance
deferral; interruption/mute across multiple
PCM generations; restart before and after durable completion; lost Ack with
another mutation; Schedule/storage failure; and reminder-text tool/camera
injection refusal. Record installed SHA-256, switch/path settings, raw logs,
actual due-to-first-audio and request-to-confirmation times, every failure,
and cancel-to-output-revocation time. The 750 ms maximum offer lease is a
protocol bound plus existing output-revocation work, not a measured acoustic
bound. Verify the optional child is Other/nice 19 while Voice retains its
audio policy.

Most likely wrong on the lamp: provider schema acceptance and faithful spoken
wording, timely local-silence admission in a busy office,
actual ALSA generation/cursor retirement, and flash stalls beyond an offer's
lease. Nice 19 under contention also needs target observation. No host fixture
has established audible completion, exactly-once hearing, real privacy behavior,
or interruption timing on the lamp.
