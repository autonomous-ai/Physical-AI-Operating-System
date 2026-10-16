# #906's two gated motor fixtures on one stepped clock

Eighth, 2026-10-16. Linux x86_64 laptop, 20 cores. Every loaded series ran 10
at a time, beside 24 busy loops (`run.sh`; `load.txt` holds `uptime` at its
start and end). Each `*.txt` series is every run's full output, unedited, and
each run starts with `=== <dir>/<run>`. A run without `test result: ok`
counts as failed. The counts below are computed from these files.

## Cause

This is the first boundary of #949 (#996), in two more `gate_segment` tests:
`late_started_callback_cannot_restore_an_offer_after_owner_replacement` and
`same_owner_input_end_cancels_queued_gesture_before_inert_bus_write`. The child
worker is held at its gate. The test replaces the owner, or ends the input,
and publishes, which stops the motor and starts the supervisor's deadlines.
Only then does it write `motor/go`, so on the host clock the release's host
time counted against them.

## Evidence

| File | Binary | What | Passed |
|---|---|---|---|
| `0-host-clock-pause-experiment.txt` | not kept; #996 head `0a4d30832` plus `0-…diff` | each test 10 times with no pause, then 10 times with a 30 ms host pause before `go`, no load | 10/10 and 10/10 with no pause; **0/10 and 0/10 at 30 ms**, every one a missing `child-position-writes` (#906's second symptom) |
| `before-*.txt` | `before.bin` (#996 head `af97a4a4c`'s tests; these two still on the host clock) | each original identity, 400 runs, interleaved batch by batch with `after-` | late_started 396/400, same_owner 399/400 |
| `after-*.txt` | `after.bin` (built at `4ebb8daf8`; that commit became `630b51ad3` only by adding a line to `voice-test-fixtures.md`, so its source is the same) | the two originals and their two 30 ms-pause twins, 400 runs each | **400/400** each |

The five failures on the host clock are #906's kinds. Four were the shared
wait at `tests.rs:841` ending `motor worker progress timed out` (3) or `motor
worker ended independently` (1). One was a missing `child-position-writes`.

## The change (tests only)

`630b51ad3`: both tests run on `stepped_fixture`, with #996's stepped gate
and exit wait. Each gets a twin with a 30 ms host pause before `go`. No
production code, deadline, assertion, retry or reap changes. The
`child-position-writes` file is still required.

## #906's third test (the reviewer's 15:57 P2 on #1000)

`pending_inspection_is_revoked_before_worker_data_and_release_is_separate`
had the same boundary on the host clock. It runs on `stepped_fixture("gate")`
now, with a twin,
`pending_inspection_revoke_survives_a_host_pause_before_the_gated_worker_is_released`
(a 30 ms host pause before `go`). The reviewer showed the cause: a 30 ms pause
before `go` failed 3 of 3 on the host clock and passed 3 of 3 stepped. Here,
under 24 busy loops (`run-third.sh`, `after-third.bin`), each passed
**400/400** (`after-pending_inspection_*.txt`). Every assertion is kept,
including the privacy release confirmed and `ControlLost` in `child-finish`.

## Not shown here

- The ARM64 runners, which are in the PR's own CI run.
- Any other host-clock `gate` fixture. `gated_exit_failure`, and the `gate`
  mode after Ready, are unchanged.
