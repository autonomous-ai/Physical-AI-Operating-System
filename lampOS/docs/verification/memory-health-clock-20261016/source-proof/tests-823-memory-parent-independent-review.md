# Memory parent control independent review

Date from `date`: 2026-10-16 02:48:47 +0700. Read-only review after author confirmed stable source. No Cargo, builds, tests, Git, network, devices or implementation edits.

## Finding first

**No remaining source-review blocker under root's explicitly accepted cleanup contract.** `tests/support/health_control.rs:487–490` takes the owned child, ignores `Child::kill` errors and calls unbounded `Child::wait`. Drop invokes this after any partial-start failure, including an expired eight-second startup deadline. Therefore startup/control I/O is cumulatively bounded, but cleanup is not covered by that host bound. The real-child test at `tests/health_control.rs:808` proves successful SIGKILL/reap only. This is an evidenced missing deadline/error check, not an observed hanging child. Do not discard or fabricate child status to make this look bounded. Root reviewed this finding and explicitly chose the original actual kill/wait ownership: no detached reaper, no dropped live child and no fabricated bounded success. The eight-second guarantee covers cumulative control/marker waiting, excluding potentially blocking kernel kill/reap and filesystem operations. The author has qualified the final handoff accordingly. This is an accepted stated limitation, not a remaining protocol failure.

No concrete correctness blocker found in the reviewed parent protocol, clock or marker path. The separate child implementation was compared by its shared protocol contract, not independently re-certified here.

## Verified source behavior

- `start_until` changes only Command stdin, validates private resources, writes exactly one bootstrap frame then closes its pipe, and accepts Connected under the same absolute startup deadline. It returns the real Child separately. It neither injects runtime argv nor calls Connected readiness. Private listener cleanup is scoped. Partial-start ownership is retained by StartupChild, with the cleanup-bound limitation above.
- `Window` binds one controller through Rc identity and fixes both original logical start/+8s endpoint and one absolute host deadline. `advance_inner` steps at each next_due, so a target across three full periods cannot skip them. All Step write/read, receipt validation and file marker observation use that same Window deadline. `wait_cadence` creates a new Window per independent call; future multi-call scenario loops must retain one Window when their overall host budget must remain cumulative.
- `step` commits round/time/cadence/Checked only after the exact complete marker is observed from the opened actual regular log file and the deadline remains valid. Any operation error latches failure and shuts down the channel, including an error after the child may already have advanced. No rollback followed by continued use is permitted.
- Validation checks nonce, round, time and marker identity; the initial cycle/start/due; unchanged pre-due counters and outcome; and exactly one due cycle with its matching attempt delta. Counter arithmetic is checked. Zero-left receipts and non-whole-microsecond, backwards or skipped targets are rejected.
- The scanner bounds retained line storage, rejects matching substrings/incomplete lines, tracks partial next lines across chunks, and checks inode/device/truncation. Its one-millisecond sleep is bounded polling for actual regular-file visibility, not a simulated report-period sleep or flaky-test retry. The future caller must supply the actual fresh stderr destination; the API cannot introspect Command's stderr handle.
- Neither Checked nor EOF becomes a Diagnostic/Event, readiness or OS exit. Checked is private parent state; actual reports and Child::try_wait remain caller-owned evidence. The real-process test separately waits for an actual successful exit.

## What the 23 new tests prove

The count increased from the requested 22 to 23 while the author finished the wrong-Connected-identity boundary. There are 93 assertion sites. All are source reviewed, **none executed by this reviewer**. A detailed per-test matrix is in the adjacent JSON report.

The mock receipt generator mirrors the valid contract, but the mutation tests then independently corrupt identity, counters, time, deadlines and outcomes and exercise rejection/closure. This is useful validator coverage, not proof that a Reporter really ran. The full target test checks the exact requested 0/1/2/3/3.2-second sequence; it supports retaining the full three-period future loop. File tests use actual append/read/rename/truncate operations and Unix-channel tests use actual queued bytes, EOF and backpressure. The missing-marker test models an advanced child via a valid receipt and independently proves no parent time commit without real complete bytes.

Two tests use actual subprocesses: one kills/reaps a shell, and one launches this test executable with explicit test-only mode, checks actual unchanged argv/bootstrap EOF/stderr marker, and calls actual try_wait. The latter intentionally supplies a protocol-model receipt; it is not an end-to-end Memory application/Reporter test. None of these tests substitutes for the original health scenarios, downstream Subscriber assertions or the pending child/application integration gates. The short 20ms negative-I/O deadlines are test watchdog cases, not production or scenario timeout changes; no retries were added.

All 27 original Memory Rust files still match the pre-slice hashes, including the original health scenario source. The parent owns only its two new test/helper files.

## Stable source hashes

- `lampOS/crates/memory/tests/support/health_control.rs`: `a06cb3e241ac901f5729eefe89abedf96cf5c8f7eb580bfcc715540ee67f97db`
- `lampOS/crates/memory/tests/health_control.rs`: `e90c17dd703d4d9879732d4b2ead58d725d9e7f712f53fd2e56bd8198c771325`
- `lampOS/crates/memory/src/fixture_protocol.rs`: `21f2d5ca2fc309c8b63989ff6a889f37edd5e3eb8a4fc5de305348d11a0efb0c`

Author handshake and recorded hashes match the inspected files. The report applies only to these hashes.

Cleanup decision incorporated at `2026-10-16 02:49:11 +0700` from `date`; source hashes remain unchanged.
