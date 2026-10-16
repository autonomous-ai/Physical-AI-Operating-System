# #935: the reminder Ack round trip on a busy disk (Intel Mac, 2026-10-16)

Taken by twelve on its person's Intel Mac (i9-9980HK, macOS, Rust 1.96.0,
debug). No lamp: this is a host test fixture, and no result here is a result
on a lamp. Base `bc68dec87` (`origin/lamp-voice`).

## The cause, measured

A temporary probe (not committed) printed the time and the budget left at
each step of `Consumer::recover_one` in
`coordinator::tests::reminder_roundtrip::actual_consumer_ipc_coordinator_and_two_retirements_ack_only_after_whole_completion`.

Idle host, three runs:

| step | elapsed | budget left |
|------|---------|-------------|
| Get answered | 0.8–1.7 ms | 248–249 ms |
| Ack plan committed to the ledger | 78–81 ms | 169–172 ms |
| Ack answered by Schedule | 158–161 ms | 89–92 ms |

With four writers each looping 64 KiB writes and `F_FULLFSYNC` on the same
disk (a Python script in twelve's scratch folder, not committed), the plan
committed at 318 ms and 340 ms, with no budget left, and the Ack exchange
failed at once (`ok=false`). The test then failed at line 336 (`pending`
still true), which is the same boundary as the reported line 338: had the
deadline passed after Schedule received the Ack instead of before, Schedule
would have committed it and the test would have seen one Ack request and no
`AckConfirmed`.

## Before and after, on a busy disk

The same four fsync writers ran throughout. Each run is one
`cargo test -q -p lamp-voice --lib -- coordinator::tests::reminder_roundtrip:: reminders::consumer::tests::completed_playout_survives_both_real_store_reopens_then_acks_without_respeech`
(five tests), ten runs each, no retries.

- `base-loaded.txt`, base: 0 of 10 runs passed. Every run failed
  `reminders/consumer/tests.rs:644` (the line the first builder named), and
  all four round-trip tests at `reminder_roundtrip.rs:73` (the 750 ms
  reminder lease ran out during the fixture's setup, on the same disk).
- `branch-loaded.txt`, this change: 10 of 10 runs passed, all five tests.

## Whole voice suite

`voice-full.txt`: `cargo test -p lamp-voice` on the change, one run, idle
apart from the build: 1,274 passed, 33 failed, 8 ignored. None of the 33 is
a #935 test. 22 are `coordinator::motor::tests`, the others are
`process::ring_startup_tests`, `optional_ring`, one `pilot_faults`, one
`provider_worker` and `reminders::transport::tests::in_process_test_helper_transfers_and_releases_the_real_claim`:
the timed process tests of #739 and #893 on this Mac, and a claim test that
also fails on base (2 of 20 `cargo test -p lamp-voice --lib reminder` runs
on base, 4 of 20 on the change; not mended here, see twelve's log).

## Not shown

- No ARM64 or lamp run in this folder; the pull request's ARM64 run is named
  in it.
- The reported line-338 failure itself was not reproduced; its boundary was
  (above).
