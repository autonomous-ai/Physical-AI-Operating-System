# The speaking-speed capacity test on a busy host (Intel Mac, 2026-10-16)

Taken by twelve on its person's Intel Mac (i9-9980HK, macOS, Rust 1.96.0,
debug). No lamp: a host test, and no result here is a result on a lamp.
Base `origin/lamp-voice` at the branch's cut.

Test: `provider_worker::tests::twenty_second_answer_through_a_five_second_reply_buffer_follows_capacity`,
red in the reviewer's #985 gate at 11:42 (ReadTimeout logged, then
ConnectionReset at the rig heartbeat's unwrap, `provider_worker.rs:5765` at
`5358fcae0`).

## The cause, measured

Run alone on the idle Mac, the base test failed 2 of 8 runs, about 6 s in,
with `connection lost (Gemini Live BarrierTimeout)` and then ConnectionReset
at the heartbeat's unwrap. A temporary probe (not committed) printed every
gap over 60 ms between two coordinator heartbeats of the rig, every
authority the worker installed with no owner, and every barrier the session
set for output after a retirement. In 8 runs:

- every run had heartbeat gaps of 60–99 ms;
- the three that failed had a gap of 101 ms, 116 ms or 144 ms, then
  "authority without owner: retiring 1" within 2 ms, then the lost
  connection: BarrierTimeout in two, ReadTimeout in one (that run took
  74 s, the shape of the reviewer's log);
- the five with no gap over 100 ms passed in 24 s.

The rig's leases are 100 ms. The coordinator, the worker and the scripted
service run on the test's one thread, and the worker's and service's work on
the largest messages between two ticks is what held the tick back.

## Before and after, on a busy CPU

24 processes of `yes > /dev/null` ran throughout. Ten runs each of the test
binary with `--exact` and `--nocapture`, no retries:

- `base-cpuload.txt`, base: 0 of 10 passed, all ten `connection lost
  (Gemini Live BarrierTimeout)`.
- `change-cpuload.txt`, this change: 10 of 10 passed. Each prints its
  `MEASURED speaking_speed` line: hold ~4.0 s, playback finished ~24.0 s,
  starved 1–2 ms, peak backlog 5.00 s, as before the change.

## Whole voice suite

`voice-full.txt`: `cargo test -p lamp-voice` on the change, one run: 1,292
passed, 40 failed, 10 ignored. The capacity test passed. None of the 40 is
this test: 28 `coordinator::motor::tests`, `ring_startup_tests` and
`optional_ring` (the #739 and #893 timed tests on this Mac), one
`coordinator::tests::duplex`, and one `reminder_roundtrip` (#935, mended
separately in #1009, which this branch does not contain).

## Not shown

- The reviewer's exact ReadTimeout run was not repeated; the same chain
  ended in ReadTimeout once here.
- No ARM64 or lamp run in this folder; the pull request names its ARM64 run.
