# Agent 2 status

Owner: Agent 2. End-of-turn workstream: MAIN-051, its review, MAIN-052 and the
MAIN-055 pilot preparation. Details: [endpoint detection](../docs/endpoint-detection.md).

| Branch | Head | Content |
| --- | --- | --- |
| `lamp-a2-endpoint-runtime` | `350bd572` (tested `61112832`) | MAIN-051 checkpoint, unchanged for Main's review |
| `lamp-a2-endpoint-review` | `052eb703` | That checkpoint plus the four review fixes |
| `lamp-a2-continuation` | this file's commit | Review fixes, Rust-only cleanup, integration `6d81ab49`, MAIN-052 recoverable early end |
| `lamp-a2-endpoint` | `ee1eaad8` | MAIN-043 evidence |

## Review fixes (`052eb703`)

1. **Teardown.** `Monitor::wait_finished` returns a typed `Teardown`
   (`Reaped`, `Pending` or `Unreaped`).
   - An exit not confirmed by a native wait starts no replacement.
   - `Rig::shutdown` fails cleanup when the endpoint worker is not confirmed
     reaped within its bound (at least 500 ms), so a physical mute is not
     restartable.
   - Regressions: `unconfirmed_endpoint_exit_fails_mute_cleanup_and_never_reopens_service`
     (grace 1.5 s, wait 500 ms) and the control
     `confirmed_endpoint_exit_keeps_the_restartable_mute`. Both use
     cooperative fixtures, so only the endpoint can fail.
2. **Identity.** `WorkerCommand.model_sha256` is required. A `Ready` that
   reports another model is a startup failure before any request.
   `smart_turn()` pins `ab8dc64b…`. Test:
   `a_worker_reporting_another_model_is_refused_before_any_request`.
3. **Close.** `close()` cancels a request already sent, and `poll()` returns
   nothing after close. Counters are kept: `discarded` + `lost` = 1. Test:
   `no_outcome_is_delivered_after_close_even_for_an_answer_on_its_way`, using a
   file handshake with a real child.
4. **Retention.** The worker zeroes `FeatureExtractor`'s waveform, FFT and
   power buffers after each request, with a test. The claims are narrowed:
   freed allocations and the runtime's internal memory are not scrubbed.

Gates at `052eb703`:

- **Linux ARM64**: my own Colima VM, Docker `rust:1.96.0-bookworm`:
  - fmt passes and strict workspace Clippy is clean;
  - `cargo test --locked --workspace --no-fail-fast`: 1,369 passed, 0 failed,
    8 ignored, plus 11 self-hosted process tests;
  - the real-model and real-worker session tests pass.
- **macOS**: the same, except Main's two known `optional_ring_tests`, which
  fail on macOS only and were not touched.

## Rust-only lampOS (`425a76a7`)

- `crates/endpoint/tests/fixtures/make_reference.py` and
  `crates/endpoint/fetch-model.sh` are removed.
- The reference data stays, and `crates/endpoint/tests/fixtures/README.md`
  records its pinned generation: inputs, versions, signals, steps and hashes.
  The generator is kept outside lampOS.
- `lamp-endpoint-worker --check-model PATH` verifies an installed model.
- `tools/observer-bundle/build.sh` (not mine) is the remaining non-Rust script.

## MAIN-052: recoverable early end of input

- **Detector.** `EarlyEnd`: a confident verdict ends input at 500 ms, and the
  end stays provisional until the baseline's 1.2 s of quiet. Speech before
  then is a continuation; the hold lasts until that utterance ends.
- **Coordinator.** `--endpoint smart-turn-early` (opt-in, requires
  `v1-baseline`) skips speaker dispatch while the end is provisional. A
  continuation replaces the unspoken answer through the existing path.
- **Provider dependency.** Recovery needs Gemini to settle each interrupted
  request within the 2 s barrier and to keep the earlier part in context.
  Both are unqualified, so the mode stays opt-in and is not for the pilot.
- **Full-session regressions** (real loop, credit-limited provider, speaker
  receipts):
  - early end, then confirmation;
  - continuation with the old answer arriving during the resumed speech;
  - an answer held by the gate, then dropped;
  - two continuations;
  - privacy during the provisional end;
  - missing and hung workers.

  Removing the gate makes four of them fail.
- **End-to-end** (digital replay, fresh held-out human clips, provider first
  audio 1.1 s after the end of input):
  - the median from speech end to the reply's first audio falls from 2.3 to
    1.6 s;
  - whole thoughts: 588 against 592 for the baseline if Gemini settles, but
    495 against 586 if it never does;
  - 9 audible splits against 4.

  The synthetic `say` cohort is reported separately.

## MAIN-055 pilot configuration

- Use `--turn-taking v1-baseline` alone, or add `--endpoint smart-turn` with
  the worker and verified model beside `lamp-live`. Both fall back to today's
  1,200 ms window independently of inference.
- Do not use `smart-turn-early` until a scheduled probe qualifies Gemini's
  settlement of a request interrupted before it answered.
- Bounded fallback and recovery are tabulated in the doc's pilot section.

## Requests to Main

1. Compose `052eb703` for the pilot if the endpoint worker is used. Shared
   files: `coordinator.rs`, `coordinator/endpoint.rs`, `options.rs`, and the
   test `Rig` and `WorkerCommand` literals.
2. Schedule the provider probe the early mode depends on: `activityEnd`, then
   an interrupting `activityStart` with continuation audio 200–800 ms later,
   before any reply audio. Record whether request 1 settles within 2 s, and
   what the answer covers.
3. A733 worker measurements when scheduled.

## Limits

- No device, loudspeaker or direct-human evidence; human clips are digital
  replays.
- The provider model in the end-to-end evaluation is an assumption.
- No topic change is inferred.
- In 8 of 231 held-out continuations the opening began earlier than the 300 ms
  pre-roll.
