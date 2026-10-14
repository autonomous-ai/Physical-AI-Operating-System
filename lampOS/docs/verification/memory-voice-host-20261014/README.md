# Memory Voice client: development-host verification

Task 29 / [issue #698](https://github.com/autonomous-ai/lampOS/issues/698),
2026-10-14. These are development-host results, not lamp measurements.
The final task head needs a green native ARM64 run before handoff; its run
and readiness belong in Fourth's log and pull request. The changed
manifest/source/test hashes at the first measurement checkpoint are in
`source-sha256.json`, based on `3a8d0ffbdce969d238d4152533e2b89897315706`.
Code and limits are documented in [memory-days.md](../../memory-days.md).

Raw tool output is retained byte for byte. `baseline-tests.txt`,
`linux-tests-final.txt`, `mac-timing.txt` and `mac-post-eye-tests.txt` retain
Cargo's final blank line; `linux-link-environment-failed.txt` and
`mac-fixture-sandbox-failed.txt` contain raw diagnostic trailing whitespace.
These six initial artifacts are excluded from source/documentation whitespace
checks; exceptions for later raw attempts are listed with those attempts.

## Machines and boundaries

Mac: macOS 26.6.2 (25G83), x86_64 Intel Core i7-10700K at 3.80 GHz,
16 logical CPUs, 64 GiB RAM; Rust/Cargo 1.96.0. Linux host checks use the
existing Podman VM named `lampos-arm64`, which actually runs x86_64 Linux
6.12.13 with six virtual CPUs and 8 GiB configured RAM. Its name does not
make these ARM64 tests. The cross-build targets aarch64 Linux but does not
execute the result. No provider, model, real credential or lamp was used.

The 300 ms note and 1,500 ms tool deadlines were selected before building,
matching the existing Voice caller budgets. Socket tests exercise missing
or saturated listeners, stalled and trickling replies, malformed/oversized
responses, failed writes, late results, queue saturation and forget races.
Real-store tests cover frozen notes, ranked/day retrieval, linked deletion,
no-model handoff and the legacy path. These do not establish end-to-end
audio latency, storage performance on flash or provider-context deletion.

## First and repeated requests

Exact command from `lampOS/`, after compiling the release test binary:

```sh
RUST_TEST_THREADS=1 cargo test -p lamp-memory --release --test voice_timing \
  --offline --locked -- --ignored --nocapture
```

Passed once, 1 test, 0 failures, 13.47 seconds. No other local build/test
was running during this measurement. `mac-timing.txt` is unedited output;
`mac-timing.json` extracts its JSON records, including all 50 raw repeated
samples per case. Four fresh matched Days stores each contain 30 synthetic
UTC days, 6,000 exchanges and 200 kept facts. This compares old/new Voice
requests against the same already-opted-in store, not JSONL versus SQLite.
Fixture import/count validation precedes each first call; caches were not
evicted, so first use is not a cold-cache storage result.

Milliseconds, nearest-rank percentiles over the 50 repeated calls only;
p99 is their maximum:

| Production path | First | Repeated p50 | p95 | p99/max |
|---|---:|---:|---:|---:|
| Legacy kept request | 0.863 | 0.722 | 0.767 | 0.799 |
| Days session open | 73.251 | 132.608 | 148.211 | 150.513 |
| Legacy recall tool | 1.550 | 0.880 | 1.043 | 1.223 |
| Days ranked-search tool | 1.026 | 0.821 | 0.949 | 1.106 |

The legacy note boundary is `tools::exchange`: connect, serialize/send,
read, JSON decode and close, excluding kept-text extraction and prompt
formatting. The Days note boundary is `DaysClient::open_session` through
return, including validation and provenance bookkeeping. It durably creates
a new unique frozen snapshot; asynchronous cleanup of the prior session
can overlap later calls. These operations have different semantics and
costs: the new note is not a faster legacy read. Legacy returned 27 facts
under its text cap; the JSON-framed Days note returned 13 in 1,191 bytes.

Tool boundaries are the actual legacy/new `run` methods through return,
including time/day lookup, the full exchange, decode, response transform
and provenance where applicable; ToolCall allocation is excluded. The new
client's cached-timezone/nonce/thread initialization was 0.167 ms and
0.166 ms in its two cases, reported separately. The worker need not yet
have been scheduled when initialization returns. Store import and durability
are also reported separately, about 1.62–1.65 seconds per synthetic store.

Every first and repeated sample met the exact 300/1,500 ms target. The test
also allows 100 ms of host scheduling slack for assertions, but the reported
exact-target flags are all true and do not depend on that allowance. No
number includes provider connection, audio, the existing clock prompt or
final instruction assembly. The original store's 50 ms read aspiration
does not describe durable session creation: its p95 here is 148 ms.

## Gate results and retained failures

The pre-change Mac formatting, strict Clippy, tests and a clean source-only
workspace build passed. `baseline-tests.txt`, `baseline-clippy.txt` and
`baseline-workspace.txt` retain them. An earlier baseline build conflicted
with concurrent lockfile editing; `baseline-lockfile-failed.txt` retains
the error, and the clean source-only run supplies the valid baseline.

Final Mac formatting and strict Clippy for Memory, Voice and Voice Eval
passed (`mac-clippy.txt`). Development failures retained here are the
Darwin socket-flags compilation correction, a constant assertion in the
manual timing test, and an executable documentation test that needed to
verify both conversation-end examples. Each was corrected before advancing
the corresponding gate; no lint was suppressed.

The first complete affected-crate Mac test run passed Memory but failed one
existing Voice shutdown fixture: 830 Voice library tests passed, one failed,
six manual tests were ignored. `mac-tests-contended-failed.txt` preserves
the run. The fixture reported loss of privacy freshness and a 47,440 μs
privacy-update gap while other builds were running. Its production deadline
was not changed. Ten isolated repetitions with other builds stopped passed
(`mac-fixture-isolated.txt`), with roughly 6–7 ms privacy-update gaps. This
supports scheduling sensitivity, not a proof of its exclusive cause. An
initial isolated invocation was denied local socket setup by the sandbox;
`mac-fixture-sandbox-failed.txt` retains that environment failure. The next complete serial run with competing builds stopped failed a different
unchanged endpoint recovery fixture, again on privacy freshness; 830 Voice
library tests passed, one failed, six were ignored. The original shutdown
fixture passed. `mac-tests-quiet-failed.txt` preserves this result. That rules
out treating competing builds as the only established explanation. Five isolated repetitions of the recovery fixture then passed
(`mac-recovery-isolated.txt`). Source inspection confirms the fixture uses
emulated peers, no Memory socket or new provider spawn, and unchanged
endpoint/privacy/coordinator files. It fails before checking endpoint recovery.
Its nominal 10 ms privacy publisher must satisfy both observation age and
acquisition continuity below 50 ms; the existing error does not identify which
was lost. The precise scheduling cause remains unknown, and neither full run
is a passing gate. Production freshness limits and fixture assertions were
not changed.

The earlier Linux strict Clippy failed in inherited Body `inspection.rs:615`, unused
`run`. The same failure was reproduced without task 29 source changes and
reported to First; see [the baseline record on the log branch](https://github.com/autonomous-ai/lampOS/blob/lamp-fourth/lampOS/docs/verification/linux-clippy-baseline-20261014/README.md).
`linux-clippy-failed.txt` preserves the task attempt. A separate Linux
functional run mistakenly used the cross-build image, which lacks native
x86_64 `libasound.so`; link failure is retained in
`linux-link-environment-failed.txt`. No passing Linux test result is inferred
from it. The later completed native-image check is recorded below.

ARM64 release cross-build of Memory and Voice passed in the existing Debian
12 cross image, retaining the Body warning (`arm64-cross-build.txt`).
Both outputs are aarch64 ELF and were not run:

| Binary | SHA-256 |
|---|---|
| `lamp-memory` | `a64ded93580390854c704373d1fe34857d44602f721a39fcd97de5d9ac8fb024` |
| `lamp-voice` | `df04e1a057b8a804d24ed85601d5c2c5b6511d01a12c58efe89356373cb24b8d` |

This cross-build does not replace the mandatory green native ARM64 CI run.
At that checkpoint both native ARM64 CI and the strict Linux dependency gate
were outstanding. Later outcomes are recorded below.

## What First must run and what was not shown

After the complete gate, use the quiet-room staged checks in
[memory-days.md](../../memory-days.md#played-scenes-for-first): switch
absent, switch enabled, prior-day recall, a fresh-session preference,
45-second idle handoff, forget plus fresh provider context, and Memory
unavailable. Record the lamp/build/settings, first/repeated note and call
boundaries and first audible response separately. Keep Memory's migrated
store and existing `--memory-days` opt-in; disabling the Voice switch rolls
back the caller, not the store format.

Most likely differences on the lamp are flash sync cost, scheduler pressure,
Memory's serialized socket loop, local-day configuration and already sent
cloud context. No physical power-loss test, real model extraction, acoustic
latency, natural multi-day conversation, speaker identity or on-lamp privacy
qualification was performed here. No lamp was accessed or deployed.

## Native Linux completion after the upstream rebase

The final source-only copy is `/private/tmp/lamp-fourth-memory-voice-source-final-rgzt3s5e`,
created after rebasing onto `ce7abae79c06c53c8ea1dfb03c42622c41f8f135`.
The task source files are byte-identical to those in the original measurement
manifest; the inherited Cargo.lock changed for upstream Air dependencies.
`linux-tests-final.txt` retains the complete final command output: 60 test
suites, **1,459 passed, zero failed, 11 ignored**, command exit 0. This includes
832 passed Voice library tests on Linux (six additional tests ignored), and the Memory and VoiceEval
integration suites. Existing ignored manual/device checks were not counted as
executed. No concurrent local build or test was deliberately run during it.

Command inside the existing native image:

```sh
RUST_TEST_THREADS=1 CARGO_BUILD_JOBS=4 cargo test -p lamp-memory -p lamp-voice -p lamp-voice-eval --offline --locked --target x86_64-unknown-linux-gnu
```

Machine: Podman `lampos-arm64` VM on the Mac described above; despite its name,
this VM and the execution target are x86_64 Linux, not ARM64 or a lamp. Image
`localhost/lampos-fourth-native-check:latest`, SHA-256
`203a72b687c4c5a96633420af4eccf839ff34fa448a26258ca5c89a077d7e6ad`, has native
ALSA. Source mounted read-only at `/source`; registry/git caches mounted
read-only under `/root/.cargo`, the image's actual Cargo home. Target volume
`lamp-fourth-controls-target` mounted at `/target`; networking disabled.
An earlier attempt mounted caches under `/usr/local/cargo` and stopped at an
offline cpal checkout error before compilation; that environment failure is
retained separately as `linux-cache-environment-failed.txt`.

This native functional pass does not clear the separately retained inherited
Linux Body strict-Clippy failure, the two Mac full-suite failures, or the
mandatory green native ARM64 CI requirement. At that checkpoint the task
was not ready.

The final Mac compile checks on the same `ce7abae79` task source also passed:
`cargo fmt --all --check`, then strict Clippy for Memory/Voice/VoiceEval
(all targets, offline, locked), then `cargo build --workspace --offline --locked`.
Full output is `mac-final-compile-checks.txt`; the final workspace build took
37.13 s. This is a compile check after the Linux functional pass, not a new
full Mac test pass. At that checkpoint the two earlier Mac test failures
remained unresolved, and the branch was not ready for integration.

## Task-only branch on the updated upstream

Under the new one-task pull-request rule, the branch is directly based on
`1733307d7672ad302154984952a77772f8ebd6e5`, with no log-branch ancestry.
Upstream includes First's ordinary-user Records file-open repair, newer Body
behavior and Voice wording changes. Formatting, strict Mac Clippy for
Memory/Voice/VoiceEval and workspace build all passed again;
[mac-pr-base-compile.txt](mac-pr-base-compile.txt) is the complete output.
This command did not repeat the full Mac or Linux functional suites.

ARM64 run 38558922911 on the earlier `aae385f6c` checkpoint passed the release
build but failed the same two inherited Records JSON helper tests as the
judge and Records branches. First's `21c24fc0a` repair is now included; the
earlier failure remains part of the evidence. The required native ARM64 run
must be green on the eventual task head before handoff. At that checkpoint
the Linux Body lint and Mac privacy-fixture findings remained unresolved;
the current results follow.

## Checks after the upstream Body lint repair

The task was then based directly on
`a9a7262a904d802fa54043fe43cf9e368837349b`. Upstream `652a196e1` makes Body's
inspection helper test-only and removes an unnecessary mutable binding. No
task runtime or fixture was changed for this rebase. All nine task source,
test and crate-manifest hashes still match the original timing manifest;
only the inherited workspace lockfile differs. The current hashes are in
[source-current-sha256.json](source-current-sha256.json). The current Mac compile
checks passed, exit 0: formatting, strict Clippy for Memory/Voice/VoiceEval
with all targets, and the workspace build. They used `CARGO_INCREMENTAL=0`
and four build jobs after disposable debug caches had been reclaimed.
[mac-current-base-compile.txt](mac-current-base-compile.txt) is full output.
The complete serial Mac functional run then exited 101: **1,460 passed,
one failed, 11 ignored**, across 60 suites. Voice's library had 836 passed,
one failed and six ignored. Both earlier privacy cases passed this time;
the failure was the unchanged
`coordinator::motor::tests::late_started_callback_cannot_restore_an_offer_after_owner_replacement`.
At `coordinator/motor/tests.rs:896`, reading `child-position-writes` after
the inert child was reaped returned ENOENT. The fixture uses no Memory
service or new provider path. Missing cleanup evidence does not establish
why the child exited. No fixture or production deadline was changed.
[mac-current-base-motor-failed.txt](mac-current-base-motor-failed.txt) retains
the entire run and [mac-current-base-results.json](mac-current-base-results.json)
the exact counts. Earlier privacy failures remain part of the record too.
Five isolated repetitions of the failing motor fixture, using the same test
binary with other local builds/tests stopped, then passed
([mac-motor-isolated.txt](mac-motor-isolated.txt)). That does not identify the
cause or turn the failed full gate into a pass. The finding is reported on
[#698](https://github.com/autonomous-ai/lampOS/issues/698#issuecomment-6169724864).

The matching fresh source-only copy,
`/private/tmp/lamp-fourth-memory-final-lint-uyjcoxnc`, was mounted read-only
at `/source_memory`, a distinct Cargo source path with fresh copy timestamps.
On the native Linux image identified above, networking was disabled and
all capabilities dropped; UID remained 0. Registry/git caches were read-only,
target volume was `/target`, and build jobs were four. In order:

```sh
cargo fmt --all --check
cargo clippy -p lamp-memory -p lamp-voice -p lamp-voice-eval --all-targets --offline --locked --target x86_64-unknown-linux-gnu -- -D warnings
cargo build --release -p lamp-memory -p lamp-voice --offline --locked --target aarch64-unknown-linux-gnu
sha256sum /target/aarch64-unknown-linux-gnu/release/lamp-memory /target/aarch64-unknown-linux-gnu/release/lamp-voice
```

All passed without warnings. The full compile output is
[linux-current-base-compile.txt](linux-current-base-compile.txt); an incremental
repeat of the same command confirms exit 0 in
[linux-current-base-confirmed.txt](linux-current-base-confirmed.txt).
This clears the Body dependency lint finding for this task. The resulting
ARM64 binaries were cross-built, not executed:

| Binary | SHA-256 |
|---|---|
| `lamp-memory` | `7bfc2e05eff1dba74ca8fbd1700a48e5530f2fd83b678abe31be048820dbed87` |
| `lamp-voice` | `f09d9325d1e52d7e72ef1810628b6658a4a19900be4c0e13f12aab0e95c38cad` |

Native ARM64 run 38561301420 passed on earlier head
`fed759ee7557243802b704e296e1cbef11c82071`: release workspace build,
3,823 passed tests, zero failures, 30 declared manual/device checks ignored,
and artifact upload complete. Full output is retained on `lamp-fourth` in
`docs/verification/arm64-ready-20261014/`. That success does not certify this
later rebase: a green run on the final task head is still required.

## Rebase including the upstream eye behavior and merged tasks

The next base is `a9992f272d302e3ce79a889faa2a88c5867af79d`. It contains
First's eye behavior, Eyes' lint repair and the merged Records/judge tasks.
Task source continuity is in
[source-post-eye-sha256.json](source-post-eye-sha256.json): the nine task
source/test/crate-manifest hashes still match the original timing source.

Mac formatting, strict Memory/Voice/VoiceEval Clippy and workspace build
pass, exit 0, with four build jobs and incremental compilation disabled;
full output is [mac-post-eye-compile.txt](mac-post-eye-compile.txt).
The same native Linux fmt/Clippy/cross-build commands recorded above also
pass, exit 0, without warnings. They use the same restricted native image
and target volume, from a fresh source-only copy at
`/private/tmp/lamp-fourth-memory-eyes-fixed-e7s03qjr`, read-only at
`/source_memory_fixed`. Output is [linux-post-eye-compile.txt](linux-post-eye-compile.txt).
Memory's cross-built SHA-256 is unchanged (`7bfc2e05…`); Voice, including
the upstream eye behavior, is now
`1309207973726b3eeb14d335273761302c4a25c827827eebc15930be80681fd2`.
These binaries have not been executed on a lamp. The subsequent complete
serial Mac functional command was:

```sh
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=1 cargo test -p lamp-memory -p lamp-voice -p lamp-voice-eval --offline --locked --no-fail-fast
```

It completed with exit 0 on task source
`e65c522545eaf381c7a62b1445c8b1af706af0e4`: **1,469 passed, zero failed,
11 ignored**, across 60 suites. Other local builds/tests were stopped.
Complete output is [mac-post-eye-tests.txt](mac-post-eye-tests.txt), with
counts in [mac-post-eye-results.json](mac-post-eye-results.json). All three
previously failing privacy/motor cases passed in this run. No deadline,
assertion or fixture was changed. This is a full passing gate on that source,
not proof that the earlier intermittent failures have been explained or
eliminated. Those failures remain retained and reported to First.

## Rebase including First's installed Voice capture

This tested checkpoint is based directly on
`724541a957e5e66bb4c0f4a8de537466c8a9fe55`. This includes First's
first-output capture, its background-hashing repair, and the subsequent
time-tool wording. Those inherited Voice changes must be present in the
binary handed back for installation. All nine task source/test/manifest
hashes and the inherited lockfile still match the preceding full Mac gate;
see [source-capture-base-sha256.json](source-capture-base-sha256.json).

The source tested after rebase is
`d89beda40652b10c0e57c13548326b5457eca8c5`. On the same x86_64 Mac, with
four build jobs, incremental compilation disabled and no competing local
build/test, these commands passed, exit 0:

```sh
cargo fmt --all --check
cargo clippy -p lamp-memory -p lamp-voice -p lamp-voice-eval --all-targets --offline --locked -- -D warnings
for module in first_output:: attention:: provider_worker:: tools:: process:: remember:: thread::; do
  RUST_TEST_THREADS=1 cargo test -p lamp-voice --lib --offline --locked "$module" -- --test-threads=1
done
cargo build --workspace --offline --locked
```

The seven focused module runs passed **167**, zero failed, two existing
manual real-time checks ignored. This checks the new upstream capture path
and the interacting provider/Memory paths; it is not another full workspace
test run. Complete output and counts are
[mac-capture-base-gate.txt](mac-capture-base-gate.txt) and
[mac-capture-base-results.json](mac-capture-base-results.json). The final
native ARM64 CI gate runs the entire workspace before readiness, with its
exact task head and run recorded in Fourth's log and PR.

The source-only copy contains 1,391 tracked files, copied with fresh
timestamps to `/private/tmp/lamp-fourth-memory-capture-source-e4_nuu26`,
read-only at `/source_memory_capture` in the same native x86_64 Linux image.
Networking is disabled, all capabilities dropped, registry/git caches are
read-only, UID remains 0, and the target volume/build jobs are unchanged.
The following passed in order, exit 0, with no warnings:

```sh
cargo fmt --all --check
cargo clippy -p lamp-memory -p lamp-voice -p lamp-voice-eval --all-targets --offline --locked --target x86_64-unknown-linux-gnu -- -D warnings
cargo build --release -p lamp-memory -p lamp-voice --offline --locked --target aarch64-unknown-linux-gnu
sha256sum /target/aarch64-unknown-linux-gnu/release/lamp-memory /target/aarch64-unknown-linux-gnu/release/lamp-voice
```

Full output is [linux-capture-base-gate.txt](linux-capture-base-gate.txt).
These are cross-built outputs, not native ARM64 executions or lamp results:

| Binary | SHA-256 |
|---|---|
| `lamp-memory` | `7bfc2e05eff1dba74ca8fbd1700a48e5530f2fd83b678abe31be048820dbed87` |
| `lamp-voice` | `4d3a3fe454dcf30f63702546ac6b0d1598672bc54e3853aa1516baf6eb9a66b2` |

The resulting final task head
`68606e78e75369393168f184ae00ae210ecd80de` failed native ARM64 run
38582911230 in the Linux-only `capture_hold_worker` fixture, before its
audio assertions. The workspace release build passed; tests exited 101
after 3,429 passed, 1 failed and 30 ignored across 188 completed suites.
Later suites and artifact upload were not reached. Full raw evidence and
the source diagnosis are retained on `lamp-fourth` in
`docs/verification/arm64-capture-connect-20261014/`; the owner question is
[#698, comment 6171516888](https://github.com/autonomous-ai/lampOS/issues/698#issuecomment-6171516888).
No unchanged native CI rerun was requested to obtain green. These totals
include the custom `turn-end-wire` process harness; earlier reports
counted standard libtest lines only and omitted its 12 passing tests.
Raw output and pass/fail outcomes are unchanged.

## Rebase including upstream opt-in Voice tasks

The next tested source is `52c1f698319460c4079715ba4ddf0811e804cd1a`,
based on `c2ac775f5062fb082fefb562361d75d15cadd6d0`. Upstream added
opt-in task dispatch and quiet-room short utterances. The task's eight
other source/test/manifest files are byte-identical to the preceding head;
`thread.rs` has only upstream's `tasks: None` test-constructor addition.
The inherited lockfile is unchanged. [source-tasks-base-sha256.json](source-tasks-base-sha256.json)
records that comparison. A read-only compatibility review checked tool
routing, frozen-note assembly, conversation-end submission and the exact
provider-only Memory environment switch; it found no interaction requiring
a task change. This review does not qualify external task execution or
model behavior.

On the same Mac, with four build jobs, incremental compilation disabled,
one test thread and no competing local build/test, these passed in order:

```sh
cargo fmt --all --check
cargo clippy -p lamp-memory -p lamp-voice -p lamp-voice-eval -p lamp-tasks --all-targets --offline --locked -- -D warnings
cargo test -p lamp-memory -p lamp-voice -p lamp-voice-eval -p lamp-tasks --offline --locked --no-fail-fast -- --test-threads=1
cargo build --workspace --offline --locked
```

Exit 0: **1,581 passed, zero failed, 11 ignored**, across 68 completed
suites including doctests. The declared manual timing/device cases were
not executed. This includes the upstream Tasks fixtures using local
stand-ins, not an external agent or service. Raw output is
[mac-tasks-base-gate.txt](mac-tasks-base-gate.txt); counts, tested source and
raw SHA-256 are in [mac-tasks-base-results.json](mac-tasks-base-results.json).
Earlier failures remain retained; this pass does not explain their causes.

A fresh source-only copy of 1,396 tracked files at
`/private/tmp/lamp-fourth-memory-tasks-source-m45wmjri` was mounted read-only
at `/source_memory_tasks`. The same native x86_64 Linux container, offline
registry/git caches, target volume and four build jobs were used; networking
was disabled and all capabilities dropped, with UID 0. After the Mac gate
finished, these passed in order with exit 0 and no warnings:

```sh
cargo fmt --all --check
cargo clippy -p lamp-memory -p lamp-voice -p lamp-voice-eval -p lamp-tasks --all-targets --offline --locked --target x86_64-unknown-linux-gnu -- -D warnings
cargo build --release -p lamp-memory -p lamp-voice -p lamp-tasks --offline --locked --target aarch64-unknown-linux-gnu
sha256sum /target/aarch64-unknown-linux-gnu/release/lamp-memory /target/aarch64-unknown-linux-gnu/release/lamp-voice /target/aarch64-unknown-linux-gnu/release/lamp-tasks
```

Full output is [linux-tasks-base-gate.txt](linux-tasks-base-gate.txt).
This build selects three packages, including upstream Tasks. These are
cross-built artifacts, not ARM64 executions or lamp measurements:

| Binary | SHA-256 |
|---|---|
| `lamp-memory` | `63ac25d663b539645781a526e5600ae49c74fe81997e66ddc2f76b32106993c5` |
| `lamp-voice` | `715fc2aa5886bc999fd3db89691c3b08ec501e3c1cb2ace3b60831e7404e1cdf` |
| `lamp-tasks` | `d97ee80e3d0fa7a842ac46a568f827c2aab44a182466bc47483812a405d39aaa` |

The capture fixture remains unchanged and outside Fourth's authorized
Voice scope. Memory is not ready: its final task head still requires a
passing native ARM64 gate after that finding is addressed.

## Merged Mood, Expression, Body and Gemini base, before the socket repair

Source `2d081316b44f81dfc50589a3e6b554af2c88d993`, based on
`d5d765862ab1f73629ee7a2c5a9a8086f0246d11`, passed the x86_64 Mac gate:
**2,363 passed / 0 failed / 15 ignored**, 112 completed suites, exit 0.
This is macOS 26.6.2 on the same Intel i7-10700K / 64 GiB host and Rust
1.96.0 described above, not a lamp. All nine task source/test/manifest
files match the preceding host checkpoint; upstream added the Mood package
to the inherited lockfile.

From `lampOS/`, with `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=4` and
`RUST_TEST_THREADS=1`, sequentially:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo test -p lamp-memory -p lamp-voice -p lamp-voice-eval -p lamp-body -p lamp-expression -p lamp-gemini -p lamp-msgs -p lamp-mood --offline --locked --no-fail-fast -- --test-threads=1
cargo build --workspace --offline --locked
```

Unmodified output is `mac-merged-base-gate.txt` (SHA-256
`47c36348a03a8c66c2e3777d72659f297b929ccb4ebb5d7c8baf06f300fe226a`);
counts are in `mac-merged-base-results.json`, source identities in
`source-merged-base-sha256.json`. A Linux source copy was prepared for this
head but its gate was not started: First's socket-readiness repair arrived
while the Mac gate was finishing. The next Linux gate uses that repaired
base. No claim of native ARM64 or lamp success is made for this checkpoint.

## Socket readiness repair: Linux verification

First answered the capture failure with
`dd6d512b1bcaf37bfa0dfa0a305b12268f5b854b` and documented it in
`7fbb03e1f2e61ad6dfaf184cd768bad6524c2d76`. The socket must still be owned
by the current user and private before connection. An own-user socket
whose chmod is not finished returns `WouldBlock`, which both reference
channels now retry within their existing startup deadline. Fourth did not
change these outside-scope files. The earlier CI failures and source audit
remain on the log branch in `verification/arm64-capture-connect-20261014/`.

Memory rebased to that documented source as
`10e4ba509f2d1cbb97a52979c09deb0d2d1a371e`. Its nine task
source/test/manifest files and lockfile match the preceding Mac checkpoint;
`source-socket-base-sha256.json` records the hashes. A fresh standalone copy
of 1,483 tracked files at
`/private/tmp/lamp-fourth-memory-socket-source-o59t__yj` was mounted
read-only at `/source_memory_socket`. On the same native x86_64 Linux
container/image and caches described above, with network disabled, all
capabilities dropped, UID 0 and four build jobs, these commands passed
sequentially with exit 0:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --offline --locked --target x86_64-unknown-linux-gnu -- -D warnings
cargo test -p lamp-voice --test capture_hold_worker --offline --locked --target x86_64-unknown-linux-gnu -- --test-threads=1
cargo build --release -p lamp-memory -p lamp-voice -p lamp-tasks --offline --locked --target aarch64-unknown-linux-gnu
sha256sum /target/aarch64-unknown-linux-gnu/release/lamp-memory /target/aarch64-unknown-linux-gnu/release/lamp-voice /target/aarch64-unknown-linux-gnu/release/lamp-tasks
```

Strict workspace lint passed without warnings. All three capture fixtures
passed, including the formerly failing case. This was one local x86_64
execution of that fixture target; it does not replace ordinary-user native
ARM64 CI. Raw output is `linux-socket-base-gate.txt` (SHA-256
`fd773c254f61acf5865dedcd67e5134eef4faa6a788d063aaa0584ef2f66e5ce`),
with counts and identities in `linux-socket-base-results.json`.

Cross-built binaries, not executed on ARM64 or a lamp:

| Binary | SHA-256 |
|---|---|
| `lamp-memory` | `f2907b9024a21e373f723f97342daacc475ac9c4e51d236365ab5e40c1ef557f` |
| `lamp-voice` | `3f49bdf06bac92ccec7f4b36792639a45fce0fdca53c681b0b1617643ff674c4` |
| `lamp-tasks` | `47f105d7d79ba7216daf3bbf42be2b65432008c80ee4b31faaaf822d0f9c5d89` |

## Retained Mac Health failure and interrupted workspace gate

The full Mac workspace gate at `10e4ba509f2d1cbb97a52979c09deb0d2d1a371e`
passed formatting and strict workspace Clippy, then failed the unchanged
Health process fixture at `crates/health/tests/program.rs:536`. Its actual
journal line reported a disk warning at 9% free; the assertion always
expected missing Eyes, although earlier assertions in that test account
for the board level. The run later stopped mid-Surround suite with its
handle/process absent and no final exit status available. Its interruption
cause is unknown. Completed result lines total 2,257 passed / 1 failed /
16 ignored across 166 suites. It is not a successful gate, and the final
workspace build was not reached.

`mac-health-full-interrupted.txt` retains the complete partial output,
with counts in `mac-health-results.json` and an explicitly labelled excerpt
in `mac-health-failure-excerpt.txt`. Cargo also reported inherited output
filename collisions for Controls/Expression's `replay_cost` examples; no
warning was suppressed.

With the gate stopped, Fourth removed only its inactive Mood target cache.
`mac-health-cache-cleanup.json` records free space rising from 8.8804% to
12.4454%, with 17,823,891,456 bytes reclaimed. The unchanged
`cargo test -p lamp-health --test program --offline --locked -- --test-threads=1`
then passed all six tests, exit 0; `mac-health-focused-after-cleanup.txt`
retains the output. No Health source, threshold or assertion was changed.
The outside-scope fixture still needs its owner to handle the below-10%
case; details and source hashes are on `lamp-fourth` in
`verification/mac-health-disk-20261015/`. These results justify a fresh
full gate after the environmental repair, not a pass for the old attempt.

Raw whitespace in the three `mac-health-*.txt` artifacts is preserved;
exclude those raw files from documentation whitespace checks.

## Repaired upstream checkpoints

The full Mac workspace run at `5fa0f78d67442107bd5b9c000a9ff2e57d7c67d1`,
based on `dcc2e6b1ee67e11e2de89f893696432e47c4d8c5`, finished with
exit 101: **3,940 passed / 4 failed / 30 ignored, 269 completed suites**,
including the custom process harness. Formatting and strict workspace lint
passed; workspace build was not reached. Health passed after the disk
cleanup. Tasks failed before its descendant assertions; three Voice cases
failed on motor child output, an authority message and privacy freshness.
Five isolated executions per failure passed using the same binaries. Those
20 passes do not make the full gate green or establish a cause.

Unmodified full output, exact diagnostics, counts, source/binary hashes and
all isolated results are retained on `lamp-fourth` in
`verification/mac-workspace-20261015/`. The three Voice fixtures are
outside this task's scope and have not been changed by Fourth.

First answered the Tasks report with `1a38f2f3e6b8afd1e7896761a0da6149a2400f67`
and requested a rebase. That checkpoint is
`56b458966ffd10a7938d551de0449cf9fbc723bd`; the Linux and native ARM64
attempts at that checkpoint are recorded separately on `lamp-fourth`,
not inferred from the failed Mac attempt.

The handoff was then rebased on
`d8e9556dea3f1867a5af5297d1ebddbd165d9102`, including the merged touch
controls, body response to a call and corrected task-news behavior.
All nine task source/test/manifest files remain unchanged. This keeps
those upstream features in the Voice binary offered for the lamp.
Final source identities, gate commands/counts and native ARM64 run number
are recorded in the work log and PR. A passing earlier checkpoint does
not qualify a different branch head; the final head's native run must
succeed before the PR is ready.

## Not shown

No lamp execution, physical response, deployed Memory migration, real provider
context cleanup or natural multi-day conversation was qualified. Native
x86_64 results and cross-built ARM64 binaries do not establish those results.
