# Motor timeout diagnostic copy plan

Prepared on 2026-10-15 local time. This is scratch-only diagnostic work,
not a repository patch proposal or a qualification pass. No Cargo or tests
were run while preparing it. The normal working tree and final source copy
must stay untouched.

## Artifacts and guard

- `/private/tmp/lamp-third-schedule-motor-diagnostics.patch` changes only
  `lampOS/crates/voice/src/coordinator/motor/tests.rs` in an isolated copy.
- `/private/tmp/lamp-third-schedule-motor-diagnostics-tests.rs` is the complete
  replacement test file for review/application without patch tooling.
- `/private/tmp/lamp-third-schedule-motor-diagnostics-manifest.json` records
  original/replacement SHA-256 and explicitly marks compiled/executed false.
- `/private/tmp/lamp-third-schedule-motor-diagnostics-build.py` deterministically
  generates both artifacts and refuses an unexpected original file hash.

The original file's SHA-256 is
`62d1f0b5746ecca1167bc02ce0377cf24d2a94ff3e1e73b92aa972d2ab67af33`.
It is identical on d5 and 7f. This is a review-ready diagnostic draft;
compilation is still unverified and must occur only in Main's allotted slot.

## What changes, and what stays measured

Set `LAMP_MOTOR_DIAGNOSTICS` to a short, existing absolute mode-0700 directory,
for example a fresh `/private/tmp/lm709-diag-<run>` on this Mac. Use a different
root for each source/attempt. The generated per-fixture directory names contain
the parent PID and monotonic start time. Their JSON records include the exact
fixture mode, so crash and hang cannot be confused.

The fixture gets one extra private nonblocking datagram socket. Its shell
wrapper passes that diagnostic socket path to the exact inert child. Child
markers cover entry; Config read/validate entry and exit; Channels bind and
connected; MotorControl::new entry and exit; core.qualify entry and exit;
worker-run entry; Ready send entry/success; crash-exit or hang entry; Stopped;
and worker-run success/error. Each marker carries sequence, monotonic time and
child PID. There are at most 32 sends, no retry and no file I/O in that path.

The parent records its state before/after startup, at wait entry/iteration,
after authority publication, after poll and on completion. Each sample includes
the preceding sample gap, connection/readiness/reap state, OptionalMotor's
failure and the Supervisor's stored failure. First 16 and last 256 samples are
kept; counts and maximum inter-sample gap cover the entire wait. It separately
retains up to 64 lifecycle-event observations and 32 child markers. Every
diagnostic receive is nonblocking with a fixed 32-datagram drain budget.

Only after a predicate succeeds or its original deadline has already expired
does the helper write a bounded JSON record, at most 256 KiB, without fsync.
The record path is printed into the diagnostic test log. A private case may
have multiple `wait-N.json` files. Keep failed and passing cases alike.

Every original timeout, assertion, mode, fake motor behavior, authority cadence,
poll budget and production function remains unchanged. No real device opens.
The extra channel and allocations still perturb execution; all resulting
timings are diagnostic, not an unchanged-source pass or latency benchmark.
Best-effort markers can be lost under socket pressure: sequence gaps or absent
markers mean unknown, never proof that a phase did not execute. The mode in
the parent record does not depend on child-marker delivery.

## Application and comparison, for Main to execute later

1. Preserve raw logs, source manifests and any still-identifiable original test
   binaries first. The same Cargo output filename can be overwritten by a later
   build; filename alone does not establish binary identity.
2. Make a dedicated source copy of the chosen source. Do not use the root tree,
   the final qualification copy, or the existing preserved clean baseline.
   Verify the test-file SHA-256 above, then apply the patch at the copied repo
   root, or copy the complete replacement into that one file. Save an explicit
   source map showing exactly this diagnostic test-file difference.
3. Keep Cargo serial. Use the established Rust 1.96, offline/locked dependencies
   and the authorized Unix-socket test environment. First compile/run only the
   failing exact case in that diagnostic copy:

```sh
LAMP_MOTOR_DIAGNOSTICS=<private-short-run-directory> RUST_BACKTRACE=full cargo test --offline --locked -p lamp-voice --lib coordinator::motor::tests::crash_and_stalled_control_never_allow_replacement_or_false_cleanup -- --test-threads=1 --nocapture --exact
```

4. If needed, run the same exact command for
   `failed_reap_after_stopped_is_latched_and_reported_once`, then
   `explicit_current_segment_reaches_inert_counts_without_completing_voice`.
   Retain complete output, command/exit/source identity and all diagnostic JSON.
   Avoid another full gate until the traces identify a narrower question.
5. Baseline comparison option: apply these identical diagnostic bytes to a
   separate clean 7f source copy without Task 31, then run the same exact cases
   with a distinct diagnostic directory. No clean/Task31 diagnostic timing is
   interchangeable with the corresponding uninstrumented qualification run.

## Reading the result

- Long startup before child-entry, with short parent poll gaps: investigate
  shell/exec/test-harness startup; an absent child marker alone is insufficient.
- Connected child plus long new/qualify interval: localize to the fake motor
  initialization/qualification phase, not exec or readiness transport.
- Child Ready sent plus parent long gap: parent service delay is measurable.
- Ready sent but consistently short parent gaps and never observed: inspect
  real control-channel delivery/validation; compare both stored failure fields.
- Crash/hang marker plus continuing parent polls without stored exit/progress
  failure: inspect supervisor enforcement/try_wait with those exact times.

Only then propose a scoped correction supported by evidence. Do not increase
the two-second helper, weaken assertions, or label a pass as proof of a cause.
