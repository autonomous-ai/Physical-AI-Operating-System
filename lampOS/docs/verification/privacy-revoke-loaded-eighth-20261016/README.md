# The motor privacy-revoke fixture on one stepped clock (#949)

Eighth, 2026-10-16. Linux x86_64 laptop, 20 cores. Every loaded series ran 10
at a time, beside 24 busy loops; `load.txt` holds `uptime` at each series'
start and end. Each `*.txt` series is every run's full output, unedited, and
each run starts with `=== <dir>/<run>`. A run without `test result: ok`
counts as failed. The counts below are computed from these files.

## Two boundaries, both in the fixture, both on host time

1. **Inhibit acknowledgement (#949's own failure).** In `gate_segment` mode the
   child worker is held at its gate inside `publish(Inspected)` and reads no
   control. The test revokes camera permission and publishes, which starts the
   supervisor's 20 ms `INHIBIT_DEADLINE_US`. Only then does it write `motor/go`;
   the child wakes from its 1 ms park and acknowledges. On the host clock,
   that release counts against the 20 ms.
2. **Teardown after the child has finished (found while qualifying 1).** On
   the stepped clock (#900), the child writes `FINISHED` and then exits. From
   then on every parent step passed at once, so scenario time ran on, about
   125 steps to 250 ms, while the child was still exiting in host time. At
   `TEARDOWN_US` (250 ms) the supervisor killed it. A killed child has no exit
   code, so `privacy_release_confirmed` failed: `motor cleanup unconfirmed:
   Some("motor inhibited: ControlLost")`. The worker's `ControlLost` with
   `privacy_revision: Some(23)` is the normal end of a revocation. The worker
   refuses the denied snapshot in `authority()`, and the supervisor accepts
   that only as a confirmed privacy release.

## Evidence

| File | Binary (`binaries.txt`) | What | Passed |
|---|---|---|---|
| `0-host-clock-pause-experiment.txt` | not kept | `origin/lamp-voice` as fetched then (`ede9ce8ef`, from the remote-tracking reflog), plus `0-…diff`: an optional host pause before `go`. 10 runs with no pause, then 10 with 30 ms, no load | 10/10 no pause; **0/10 at 30 ms**, all `motor inhibit acknowledgement timed out` |
| `1-host-clock-before.txt` | `before.bin` (#900 head `a98fcef66`; this test still on the host clock) | the original identity, 400 runs | 399/400; the 1 failure is #949's inhibit-ack timeout |
| `2-first-commit-after.txt` | `after.bin` (`c4b6ea31b`) | the original identity, 400 runs, interleaved batch by batch with series 1 | 400/400 |
| `3-first-commit-pause.txt` | `after.bin` | the 30 ms-pause regression, 400 runs | 399/400; the 1 failure is boundary 2 |
| `4-diagnostic-pause.txt` | `diag.bin` | the 30 ms-pause regression, 4,000 runs, with diagnostic lines (`exp949 …`): the child logs gate enter/leave, the parent logs each event it receives and any step that ends without the child's answer. That source was not kept; it was `c4b6ea31b` plus those `eprintln!` lines and the child's output in the `finish()` message | 3,998/4,000; both failures are boundary 2. The parent received the same `Ready, Inspected, Inhibited, Stopped` as in passing runs, and no step ended early |
| `6-slow-exit-without-change.txt` | `without.bin` (`af97a4a4c` minus its `step_child` change, `6-…diff`) | the 300 ms slow-exit regression, 10 runs, no load | **0/10**, all boundary 2 |
| `5-final-*.txt` | `final.bin` (`af97a4a4c`) | each of the three privacy-revoke identities, 400 runs | **400/400** each |

Two earlier diagnostic series of 2,000 runs, with fewer diagnostic lines, are
not kept. Each had 1 or 2 failures of boundary 2; the child lines printed from
them are quoted on #949.

## The change (tests only)

- `c4b6ea31b`: `pending_segment_privacy_revoke_wins_before_any_position_write`
  runs on `stepped_fixture`. The gate answers each step without doing work
  and takes the next step before looking for `go` again, so the held child
  never works at an instant it has already finished.
  `privacy_revoke_survives_a_host_pause_before_the_gated_worker_is_released`
  holds a 30 ms host pause before `go`.
- `af97a4a4c`: `step_child` keeps scenario time still after the child reports
  `FINISHED`, until it has exited. The exit is observed with `waitid`'s
  `NOWAIT`, so the supervisor still reaps it. The bound is the existing 30 s
  host watchdog. `privacy_revoke_survives_a_finished_worker_that_takes_300_ms_to_exit`
  uses mode `gate_segment_slow_exit`: the child sleeps 300 ms after its
  worker has finished, before it exits.

No production code, deadline, assertion, retry or reap changes. The
zero-position-write check is still required. The host-clock fixtures and
their 2 s gate bound are unchanged.

## Not shown here

- The ARM64 runners, which are in the PR's own CI run.
- #906's two identities. They use the same `gate_segment` gate, but on the
  host clock; whether they share these boundaries is not shown.
