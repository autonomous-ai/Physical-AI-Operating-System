# Supervised-pilot fault qualification

Status: in progress (Agent 2, branch `lamp-a2-pilot-faults`, base
`lamp-v2-chat` `e63f3152`). Main owns scheduling, integration and release.

## What this is, and is not

Every result here comes from an injected fault in a test. Nothing is a Lamp,
microphone, speaker, cloud or acoustic measurement, and no runtime flag was
enabled. The tests show what the production coordinator does when one
subsystem fails in a specific way. Whether real hardware fails in that way is
not shown.

## Harness (`crates/live/src/coordinator/tests/pilot_faults.rs`)

- **Real loop.** Each test runs the production `conversation()` service loop
  (`Lifetime::Serve`, default options, V1-baseline as in the pilot) and then
  `finish_active_run`.
- **Real children and IPC.**
  - Every worker is a real child process (a cooperative fixture) on the
    production Unix-socket channels.
  - One peer thread answers for capture (10 ms frames), privacy (fresh every
    5 ms), the provider (credit-limited output) and the speaker.
- **Production speaker check.** The speaker peer accepts PCM at the device
  rate (240 samples per 10 ms). Before every write it applies the production
  `BoundaryGuard` to the chunk's permit, as the real speaker does.
- **One fault per test, at an observed event.** Faults fire on events such as
  "the speaker accepted 3 chunks", "the provider received End" or "the
  authority snapshot shows no owner", not after fixed sleeps.
- **No widened bounds.** Production leases and freshness bounds are
  unchanged. The emulated sessions run one at a time so parallel test
  threads cannot starve them.

**Invariants asserted after every run:**

| Invariant | How it is checked |
| --- | --- |
| No stale speech | No chunk dispatched under an authority view issued after its owner finished, privacy was observed muted, or Stop was requested |
| No duplicate answer | One `turn_finished` per turn; no request started twice; no chunk sequence sent twice |
| No false readiness | No `listening_ready` after a mute, a Stop or a terminal provider end |
| No orphan | Every role observed Stop, unless its child was abandoned by design. The supervisor reaped every child, with no forced kill beyond abandoned ones. No answer outlives its run |
| Bounded | Each run, including cleanup, ends within 12 s. Otherwise a test-only privacy fault ends it and the test fails |

## Matrix

| Fault | Injected when | Outcome | How it ends | Test |
| --- | --- | --- | --- | --- |
| None (control) | — | answered once, `audio_written_unscored` | mute after settle | `control_one_question_is_answered_once_then_mute_ends_the_run` |
| Provider TurnFailed before output | after End | `provider_failed`, no speech | mute | `provider_failure_before_output_settles_failed_without_speech` |
| Provider TurnFailed during output | after the first accepted chunk | `provider_failed`; audio that arrived before the failure plays; never a completed answer | mute | `provider_failure_during_output_never_becomes_a_completed_answer` |
| Provider process lost before output | after End | `runtime_failed`, no speech | run error "provider worker exited" | `provider_process_lost_before_output_ends_the_run_without_speech` |
| Provider process lost during output | after 3 accepted chunks | `runtime_failed`; answer cut | run error | `provider_process_lost_during_output_stops_speech_and_fails_the_turn` |
| Speaker stalls (reads control, stops acknowledging) | after 3 accepted chunks | `runtime_failed`, never completed | "speaker write evidence expired", 201–210 ms after the stall | `stalled_speaker_during_output_is_bounded_and_never_completes_the_answer` |
| Capture worker fault | during output | `runtime_failed` | run error | `capture_fault_during_output_cancels_the_answer_and_ends_the_run` |
| Capture stops sending frames | mid-question | `runtime_failed`; no End sent, no answer | run error | `capture_stall_during_a_question_ends_the_run_without_an_answer` |
| Privacy revoked while answer speech is queued | after 3 accepted chunks | `privacy_muted`; no dispatch after the mute; nothing resumes | `PhysicalMute` | `mute_while_answer_speech_is_queued_cuts_it_and_never_resumes` |
| Real SIGTERM while answer speech is pending | after 3 accepted chunks | `service_stopped` | clean `Ok` stop, all children reaped | `sigterm_while_answer_speech_is_pending_stops_cleanly` |
| Failure notice (pilot policy) | provider fails before output, then the person speaks during the notice | `provider_failed`; one notice attempt; the next owner is admitted only after the notice retires (V1 mask), then answered; the notice is never replayed | mute | `failure_notice_plays_once_keeps_the_failure_and_yields_to_the_next_owner` |
| Failure notice, `vad-only` cross-check | as above | the notice yields mid-play: the next owner is admitted, and a notice chunk in flight is refused by the guard (`speaker_rejected`); see N1 | mute | `failure_notice_yields_mid_play_to_a_new_owner_under_vad_only` |

Still to run in this package:

- optional ring and camera loss in the loop;
- rapid revoke and reopen through `service::supervise`, checking for stale
  speech after reopen;
- privacy revoke and Stop while a notice is playing;
- Stop while metadata is stalled;
- a provider worker `Fault` event;
- a terminal provider link during output.

## Findings

- **F1. A stalled speaker is unbounded under `vad-only`.** Not the pilot
  policy, but it is the option default.
  - The fault: a speaker that keeps reading control but stops acknowledging
    chunks leaves the answer open indefinitely.
  - In the test, only the 12 s scenario limit ended it, on macOS and on the
    Linux VM. V1-baseline, which the pilot runs, ends the same stall in about
    200 ms through its write-evidence lease.
  - The reproducer is `stalled_speaker_under_vad_only_is_bounded_too`,
    ignored by default and failing.
  - Not corrected here: bounding it changes `vad-only` behaviour, which is
    Main's decision.
- **N1. An interrupted notice changes the failed turn's outcome.** When a new
  owner interrupts a playing failure notice (`vad-only`), the failed turn
  ends `user_interrupted`, not `provider_failed`.
  - It never becomes a completed answer.
  - `provider_turn_failed` and the notice evidence stay in the trace.
  - Under V1-baseline this cannot happen, because the mask holds speech over
    the notice.
- **FL1. One freshness loss, retained.**
  - In one serial macOS run of the first batch, one test ended with "service
    privacy freshness lost before next observation", before End. The
    production bound is 50 ms.
  - It was not reproduced in 16 later serial runs of those ten scenarios, or
    in any later macOS or Linux VM run.
  - I then instrumented the peer's longest gap between privacy sends: 7–12 ms
    in every later run. A recurrence would show whether the peer or the
    coordinator loop was late.

## Commands and results

From a source-only export of the tested tree:

```sh
cargo fmt --all -- --check
cargo clippy --locked -p lamp-live --all-targets -- -D warnings
cargo test --locked -p lamp-live --lib pilot_faults            # 12 passed, 1 ignored (F1)
cargo test --locked -p lamp-live --lib pilot_faults -- --ignored   # F1 fails, as documented
cargo test --locked -p lamp-live --lib                         # whole live library
```

| Host | Result |
| --- | --- |
| macOS, M2 Max | fmt and Clippy clean. The first ten scenarios passed in 16 of 17 serial runs; the one failure is FL1. The final set of 12 passed in each of its last three runs, two serial and one parallel. F1 failed whenever it ran |
| Linux ARM64, my own M2 Colima VM (Rust 1.96.0) | fmt and Clippy clean; `pilot_faults` 12/12 in 3 of 3 runs; F1 fails as documented; full `lamp-live` library 489 passed, 0 failed, 5 ignored |
