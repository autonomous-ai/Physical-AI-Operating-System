# Schedule in Voice: Task 31 / #709 host verification

The source-only implementation copy at base `7fbb03e1f` passed its workspace
build, 280 test executions, and 72 default-off cases comprising 144 exact
stdout comparisons on the development host. **Task 31 is not ready:** the
complete host gate has not passed. Earlier complete attempts and focused
7f motor tests failed, including a clean 7f baseline without Task 31. Those
failures remain retained and unresolved here. The passing source-copy subset
does not override them.

This records host checks performed on 2026-10-14 UTC, extending into
2026-10-15 in Asia/Ho_Chi_Minh. No result qualifies a lamp, real provider,
acoustic path, privacy switch, power-cut recovery, or a native ARM64 release.
Read [standalone-summary.md](standalone-summary.md) for the completed copy
check and the failure section below for the outstanding gate.

## Source and gate status

| Evidence | Source and result |
|---|---|
| [First source map](source-first.json) | Base `3773c1fb88190fb5289bf1378eaa1f5fe5d134cb`; 652 recorded source paths. This is the earlier implementation snapshot. |
| [Intermediate d5 source map](source-d5-intermediate.json) | Base `d5d765862ab1f73629ee7a2c5a9a8086f0246d11`; 675 recorded source paths, including the Linux child-scheduling correction. |
| [d5 rebase record](rebase-d5.json) and [7f rebase record](rebase-7f.json) | Preserve the transitions between bases. Earlier source maps and results retain their original identities. |
| [First complete gate](raw/lamp-third-schedule-voice-gate-first-results.json) | Failed: 1,111 passed, 1 failed, 8 ignored across 16 completed harnesses; exit 101, command 656.960286296 s. Voice lib had 918 passed, 1 failed, 8 ignored. |
| [Intermediate complete-package gate on d5](raw/lamp-third-schedule-voice-final-gate-results.json) | **FAILED:** 1,111 passed, 2 failed, 8 ignored across 16 completed harnesses; exit 101, command 676.865360202 s. Voice lib had 918 passed, 2 failed, 8 ignored in 491.59 s. The build stage was not reached. All 675 source paths were unchanged. The original `final-gate` filenames describe this intermediate attempt, not a passed final gate. |
| [7f frozen source map](source-final.json) | Base `7fbb03e1f2e61ad6dfaf184cd768bad6524c2d76`; 675 Rust/manifest paths. All matched both the source copy and the main workspace during evidence verification. The complete-package gate on this source is **not qualified**; focused motor failures are retained below. |
| [Standalone source copy](standalone-final.json) and [byte comparison](default-off-comparison.json) | **PASSED:** offline locked workspace build; 280 passed test executions, 0 failed, 2 ignored timing probes across four test commands; 72 default-off cases / 144 stdout comparisons exactly equal. This is a defined subset, not the complete host gate. |

Both failed complete gates used:

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

## Default-off comparison

The current [baseline capture](default-off-baseline.json) is the clean
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
complete gate. The separately completed standalone subset is recorded above.

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

After the outstanding complete host gate and exact-head native ARM64 CI pass,
First should use that reviewed CI artifact, keep both switches absent for an
initial ordinary-conversation minute, then
enable the exact `on` values with the installed Schedule socket and a private
persistent delivery ledger. The documented lamp-4ace paths are
`/run/lampos-schedule/call.sock`, `/run/lampos-actions`, and
`/var/lib/lampOS/voice/schedule-delivery`; actual UID ownership must be checked.
The delivery ledger belongs in the personal-data reset manifest. Retain the
ledger and persistent claim file through uncertain delivery and restarts.

Qualify first/repeated timers; list/cancel/snooze during preparation; Busy,
Unknown, privacy and short-utterance deferral; interruption/mute across multiple
PCM generations; restart before and after durable completion; lost Ack with
another mutation; Schedule/storage failure; and reminder-text tool/camera
injection refusal. Record installed SHA-256, switch/path settings, raw logs,
actual due-to-first-audio and request-to-confirmation times, every failure,
and cancel-to-output-revocation time. The 750 ms maximum offer lease is a
protocol bound plus existing output-revocation work, not a measured acoustic
bound. Verify the optional child is Other/nice 19 while Voice retains its
audio policy.

Most likely wrong on the lamp: provider schema acceptance and faithful spoken
wording, the existing five-minute Unknown-to-Quiet behavior after restart,
actual ALSA generation/cursor retirement, and flash stalls beyond an offer's
lease. Nice 19 under contention also needs target observation. No host fixture
has established audible completion, exactly-once hearing, real privacy behavior,
or interruption timing on the lamp.
