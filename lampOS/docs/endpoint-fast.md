# Opt-in fast endpoint (`--endpoint smart-turn-early`)

Status: a proposal for Main to review and compose. It is opt-in and has never
run on a Lamp, the shared office VM or a cloud service. Every result here comes
from M2. **A simulated latency win is a candidate result, not evidence of what
a person experiences.**

- **Branch.** `lamp-a2-endpoint-fast`, base `lamp-v2-chat` `7e28dc7ac`
  (it includes the report fixes `f9c6b0ea8`), plus the endpoint pilot through
  `ad0b2e329`.
- **Defaults unchanged.** `silence` stays the default endpoint. The pilot's
  `smart-turn` profile (1,000 / 1,200 / 1,500 ms) keeps its decisions and its
  exact `endpoint_policy` and `local_endpoint` records.
- **Assignment.**
  [coordination/endpoint-fast-followthrough.md](../coordination/endpoint-fast-followthrough.md).
- **Pilot rule unchanged.** [supervised-pilot.md](supervised-pilot.md) keeps
  `smart-turn-early` disabled until the actual whole-thought path qualifies.
  A simulated provider that settles interrupted requests is not enough, and
  that is all the scripted service below is.

## What it does

The early end of input lets provider work overlap the existing 1.2 s silence
confirmation, while nothing is heard before that confirmation.

1. **Early end.** On a verdict of at least 0.9, the detector
   (`EarlyEnd::CANDIDATE`) ends input after 500 ms of pause. The endpoint is
   then provisional.
   - The last audio and the provider `End` are sent at once, so Gemini starts
     answering 700 ms before the pause is confirmed. At the A733's measured
     verdict cost the verdict arrives later, and the overlap is about 600 ms.
   - The early end waits for the verdict. A verdict later than the unchanged
     500 ms result timeout ends nothing early.
   - A verdict below 0.9, or no usable verdict, ends input exactly as the
     pilot does, at 1,200 or 1,500 ms. Nothing is held then.
2. **Confirmation.** The same 1,200 ms of consecutive quiet that the baseline
   uses confirms the endpoint. If noise blips keep resetting that count, the
   endpoint is confirmed anyway 3 s after the early end.
3. **Hold.** Until confirmation, the reply is marked `input_end_held`:
   - the controller's input stays open, so no speech permit can be minted;
   - no speaker chunk is dispatched;
   - the duplex mask does not count the held answer as the Lamp speaking;
   - the turn cannot complete.

   The provider's audio is buffered meanwhile. At confirmation the hold is
   released (`endpoint_confirmed`, with `held_us`). From then on the answer
   plays through the unchanged start gate.
4. **Continuation.** Speech that starts before confirmation is reported as
   `Continued`. The held answer stays held while the new candidate is
   decided.
   - **Admitted.** The existing replacement path cancels the held reply
     (`user_interrupted`). It admits the new utterance with its 300 ms
     pre-roll, and no authority is published in between.
     - The `kept-start` worker keeps the successor's start and input until
       the superseded request's first output or idle (patience 2.5 s). It
       then writes them whole, with their real capture timestamps, inside the
       finite ledger.
     - `endpoint_hold_ended` records the retired, never-heard answer.
   - **Not admitted.** The held answer is released once that utterance ends
     or the detector is cleared, as when the duplex mask suppresses it.
     - The detector tests cover this
       (`a_rejected_continuation_releases_the_hold` in
       `tests/endpoint_confirmation.rs`).
     - No real-loop case does: the emulated peers cannot make admission
       reject a burst.
5. **Stale verdicts.** Any speech-scored block makes the pause's verdict
   obsolete. A verdict carrying an earlier token is ignored, and that pause
   then falls back to the 1,200 ms window.

### Refused before any process starts

- `--endpoint smart-turn-early` requires `--provider-continuation kept-start`,
  so a continuation never takes the historical immediate Start.
- It also requires `--turn-taking v1-baseline` or `duplex`; `vad-only` has no
  silence fallback.
- A reply fixture refuses `kept-start`, and therefore the early end.
- voice-eval validates these rules with lamp-live's own parser before it
  starts a child.

### What does not change

- **No lease or deadline is relaxed.**
  - The controller ties no deadline to open input; open input only withholds
    speech permits.
  - The provider's response clocks start at the early `End`, which is sent at
    once.
  - Capture and admission leases are published as before.
- **No replay, and no unconfirmed barrier is trusted.** Successor input exists
  only once, in the worker's kept ledger, under the existing `Require` policy.
- **No topic inference.** A topic change and a continuation look the same.
  Either way the unspoken answer is retired and the whole new utterance is
  sent.

### Bug found by these tests, and fixed

At first the hold reused `local_input_ended`. That flag also means "the
microphone stream still feeds this request". A provider outage during the
hold then made the submission gate drain input until the next endpoint, which
silently swallowed the person's next question.

The hold is now a separate `input_end_held` flag:

- the stream ends at the early end, as it really does;
- only the owner's input end, dispatch, the duplex speaking test and
  completion wait.

`a_provider_failure_while_held_…` covers this case.

## Shared changes

| Commit | Files | Content |
| --- | --- | --- |
| `209ae425f`, `9e23f60bf` | `crates/live/src/activity.rs`, `crates/live/tests/endpoint_confirmation.rs` | `EarlyEnd`, provisional confirmation, continuation events and the continuation hold (cherry-picked from `lamp-a2-continuation`) |
| `97d939b7b` | `coordinator.rs`, `coordinator/cloud.rs`, `coordinator/endpoint.rs`, `options.rs`, `activity.rs` (policy record), `coordinator/tests/turn_taking.rs` (one field), `voice-eval/src/physical/session.rs` | `--endpoint smart-turn-early`, refusal rules, `Reply::input_end_held`, the hold and release, traces and a test-only removed-hold switch |
| `d7e164df8`, `57ed58110` | `coordinator/tests/endpoint_fast.rs`, `coordinator/tests/endpoint_session.rs` (visibility only), one `mod` line in `coordinator.rs`, `termination.rs` (test-only flag) | Real-loop sessions through the Coordinated provider worker |
| `1b7cf291b` | `crates/endpoint-eval/**`, `Cargo.toml`, `Cargo.lock` | The evaluation crate, with the kept-start seam |
| docs | this file, `endpoint-detection.md`, `endpoint-pilot.md`, `coordination/agent-2.md` | |

The commit hashes are in [coordination/agent-2.md](../coordination/agent-2.md).

## Real-loop tests (`crates/live/src/coordinator/tests/endpoint_fast.rs`)

**Boundary.**

- The real `conversation()` loop runs with the real Coordinated provider
  worker (`serve_coordinated_test`), Supervisor and Gemini session actor.
  They are started as `--provider-continuation kept-start` starts them, over
  private IPC and an in-memory WebSocket.
- A scripted service plays Gemini:
  - after each `activityEnd` it answers L later with five 100 ms packets,
    40 ms apart, and then idle;
  - an `activityStart` while an answer is under way gets `interrupted` and
    idle;
  - it offers a resumption handle, and it can close with 1011.
- Capture, privacy and speaker are the emulated 10 ms peers of
  `endpoint_session`. The classifier is an in-process double that answers in
  60 ms.
- No microphone, speaker, model or cloud is used. Times are wall-clock on an
  M2 debug build.

Speech ends after capture block 199 in the single-question cases, so the
early end falls at block 249 and confirmation at block 319.

| Test | What it shows |
| --- | --- |
| `an_unambiguous_finish_ends_input_at_half_a_second_and_its_answer_waits_for_confirmation` | The service has `activityEnd` 500 ms into the pause and the answer before block 318. The first speaker chunk comes at or after block 318, at least 600 ms after the end of input. `Confirmed` at 1,200 ms of quiet and 700 ms after the early end; held at least 650 ms; all 150 speech blocks arrived |
| `without_the_hold_the_same_answer_is_heard_before_confirmation` | **Negative control.** With the hold removed, the same session is heard before block 310, so the bound above can fail |
| `a_breath_retires_the_unspoken_answer_and_the_whole_continuation_reaches_the_service` | A 600 ms breath. The kept start waits for the first answer to begin, which is then interrupted. All 100 resumed speech blocks arrive in the second activity. Only the second turn is heard, after its own confirmation; the first ends `user_interrupted` with `endpoint_hold_ended`; no input discarded |
| `repeated_continuations_keep_every_part_and_only_the_last_answer_is_heard` | Three parts with two breaths: 100, 70 and 80 speech blocks arrive; only the third turn is heard |
| `a_new_topic_just_before_confirmation_drops_a_fully_received_answer` | A new question 1.1 s into the pause, after the whole first answer arrived and settled. Its start is not kept and nothing is interrupted. The buffered answer is dropped; the new question's 140 blocks arrive and only its answer is heard |
| `an_a733_verdict_cost_ends_input_at_its_arrival_and_still_overlaps_the_confirmation` | A 400 ms verdict (Main's A733 measurement, below) arrives at 600 ms of pause. Input ends then, inside the unchanged 500 ms result timeout, and the end still precedes the first chunk by at least 500 ms |
| `a_late_answer_plays_after_confirmation` | L = 1.5 s: confirmation comes before any provider audio, and the answer plays when it arrives |
| `an_old_answer_later_than_the_continuation_is_never_heard_and_the_kept_words_arrive` | L = 1.5 s with a breath. The continuation is kept for about 1.4 s; it starts only after the old answer began, which is interrupted; all 100 blocks arrive; no `provider_turn_failed` |
| `a_classifier_timeout_keeps_the_unchanged_window`, `a_missing_model_keeps_the_unchanged_window`, `a_worker_crash_during_its_request_keeps_the_unchanged_window` | `silence_fallback` at 1,200 ms (`timeout`, `failed`, crash), no provisional event and nothing held |
| `a_provider_failure_while_held_is_never_heard_and_the_next_question_is_answered` | The service closes with 1011 on the early `End`. The turn stays held until confirmation, then ends `provider_failed`; any notice would wait for confirmation too. The worker resumes, and the next question is admitted, sent whole and answered after its own confirmation. Nothing is `input_not_submitted` or drained |
| `stop_while_the_answer_is_held_speaks_nothing` | SIGTERM at block 305, with the answer received and held: the run ends with the signal and no chunk |
| `mute_while_the_answer_is_held_speaks_nothing` | The service lifetime, muted at block 305: `PhysicalMute` and no chunk |
| `a_stale_verdict_never_ends_a_later_pause_early` | **Negative control.** A double that answers every request with the first request's token at 0.99. The later pause falls back to 1,200 ms (`timeout`) and is not ended early |
| `duplex_early_end_holds_its_answer_until_confirmation` | The same hold under `duplex` |
| `the_policy_record_names_the_early_end_only_when_chosen` | The pilot's `endpoint_policy` record keeps its fields; the early record adds `confirmation.early` |

Option tests: `early_endpoint_is_explicit_and_requires_the_baseline_detector_and_the_kept_start`
(lamp-live), and `the_early_endpoint_reaches_serve_after_the_pilot_settings`
with the refusal cases in
`values_and_combinations_lamp_live_refuses_are_refused_before_any_child`
(voice-eval).

## Held-out evaluation

**Inputs.**

- The same held-out manifests as the pilot: shards 4 and 5 of
  `pipecat-ai/smart-turn-data-v3.2-test`, 1,167 human clips, 598 of them
  finished turns.
  - `s4/manifest.json`: SHA256 `830b268a…5a90`.
  - `s5/manifest.json`: SHA256 `6ee8be54…8c7a`.
- The production `TurnDetector`, VAD and Smart Turn model run in process, each
  verdict released 130 ms after submission.
- Three profiles only:
  - the baseline;
  - the present pilot (`parity`, 1,000 / 1,200 / 1,500 ms);
  - the early candidate (0.9 at 500 ms, confirmed at 1,200 ms).

  The candidate is the earlier proposal's `EarlyEnd::CANDIDATE`, first
  evaluated on development shards 0 and 1, and is unchanged. Nothing was
  tuned in this work. The `turns` command now runs only these three
  profiles.

**Method.** `lamp-endpoint-eval turns` applies the coordinator and provider
seam as a model: the first answer audio is ready L after the provider has the
end of input.

- **Kept start** (current). A continuation's start, input and end are written
  at the superseded request's first output.
- **Immediate Start** (historical, 11 October). The successor start is
  written at once.
- **Never settles.** Under either seam, the continued thought is lost.

**Results.** The output JSONL has SHA256 `16e54173…6fbe`.

**Definitions.**

- **Intact thought:** every part reached the provider and nothing was audible
  before the person finished.
- **False cut:** an answer became audible and then the person went on.
- **Dropped prefix:** a resumed burst began more than the 300 ms pre-roll
  before its onset.
- **Latency:** from the first quiet block to the first audible block, over
  intact thoughts.

Kept start, with the provider settling:

| L | Profile | Intact thoughts / 598 | False cuts (clips) | Continuations (clips) | Dropped prefixes / resumptions | Latency mean · p10 · p50 · p75 · p90 · p95 · max (ms) |
| --- | --- | --- | --- | --- | --- | --- |
| 300 | baseline | 588 | 10 | 3 (3) | 4 / 13 | 1,500 · 1,500 · 1,500 · 1,500 · 1,500 · 1,500 · 1,500 |
| 300 | pilot | 589 | 12 | 7 (7) | 4 / 19 | 1,347 · 1,300 · 1,300 · 1,300 · 1,500 · 1,500 · 1,800 |
| 300 | early | 587 | 12 | 219 (192) | 8 / 231 | 1,396 · 1,200 · 1,200 · 1,500 · 1,860 · 2,210 · 3,500 |
| 600 | baseline | 590 | 8 | 5 (5) | 4 / 13 | 1,800 for all |
| 600 | pilot | 590 | 10 | 9 (9) | 4 / 19 | 1,646 · 1,600 · 1,600 · 1,600 · 1,800 · 1,800 · 2,100 |
| 600 | early | 587 | 12 | 219 (192) | 8 / 231 | 1,464 · 1,200 · 1,200 · 1,800 · 1,890 · 2,210 · 3,500 |
| 1,100 | baseline | 592 | 4 | 9 (9) | 4 / 13 | 2,300 for all |
| 1,100 | pilot | 594 | 3 | 16 (16) | 4 / 19 | 2,147 · 2,100 · 2,100 · 2,100 · 2,300 · 2,300 · 2,600 |
| 1,100 | early | 588 | 9 | 222 (194) | 8 / 231 | 1,838 · 1,600 · 1,600 · 2,300 · 2,300 · 2,300 · 3,500 |
| 1,500 | baseline | 592 | 3 | 10 (10) | 4 / 13 | 2,700 for all |
| 1,500 | pilot | 594 | 3 | 16 (16) | 4 / 19 | 2,547 · 2,500 · 2,500 · 2,500 · 2,700 · 2,700 · 3,000 |
| 1,500 | early | 591 | 5 | 226 (195) | 8 / 231 | 2,197 · 2,000 · 2,000 · 2,450 · 2,700 · 2,700 · 3,500 |

**Current strict kept start against the historical failures.**

- **Provider settles.** The immediate-Start rows equal the kept-start rows
  above in every column.
  - The kept start delayed a request's end only twice, both in the early
    profile at L = 1.5 s, by 140 ms in total.
  - A continuation's own speech and pause nearly always outlast the old
    answer's latency.
  - This reproduces the earlier published numbers exactly, for example early
    588 / 495 and pilot 594 / 585 at L = 1.1 s.
- **Provider never settles.** Intact thoughts fall to 586 (baseline), 585
  (pilot) and 495 (early) at every L.
  - The historical immediate Start failed this way on 11 October.
  - The current kept start ends the same case in its bounded barrier timeout.
    It loses the thought too, but reports it.
  - The early profile depends on settlement in 192–195 clips (16.5%),
    against 7–16 for the pilot.

**Reading, as candidate results only.**

- **Gain.** The median falls by 100 ms at L = 0.3 s, by 400 ms at 0.6 s and by
  500 ms at 1.1 s and 1.5 s against the pilot.
  - At L of 0.6 s or less, the hold, not the provider, sets the median at
    1,200 ms.
- **Cost against the pilot.**
  - 2–6 fewer intact thoughts and 0–6 more false cuts.
  - 4 more dropped prefixes.
  - A heavier tail: p75 is 200 ms higher at L up to 1.1 s, and the maximum
    is 3.5 s (confirmation bounded at 3 s).
  - At L = 0.3 s the mean is 49 ms worse than the pilot.
  - A likely contributor, not separately tested: after a continuation the
    detector's model window starts again (`Confirmation::begin`). The
    resumed fragment is then judged without the earlier part, so its final
    pause more often falls back to 1,200 ms.
- **Not established.** Whether this is better for a person is open. It rests
  entirely on Gemini settling interrupted requests and keeping both parts in
  context.

**At the A733's verdict cost.** Main measured the pinned worker on a real
4ace A733 (`b1f43e50a`, glibc 2.36, three `--check-model` runs, idle device
with stationary legacy services, no conversation):

- warm inference medians 367.7–371.8 ms, maxima 368.8–373.7 ms;
- load 373–432 ms, cold 433–561 ms;
- peak RSS 102,348 KiB.

The M2 replies in about 37 ms. On the device a verdict submitted at 200 ms of
pause therefore arrives at about 600 ms, and the 500 ms result timeout falls
at 700 ms. The same held-out run, same profiles, was repeated at two verdict
costs. The timeout was not widened.

| Verdict cost | L | Pilot: intact · false cuts · p50 | Early: intact · false cuts · continuations (clips) · dropped prefixes · p50 · p90 · max |
| --- | --- | --- | --- |
| 400 ms (idle A733) | 300 | 589 · 12 · 1,300 | 587 · 12 · 121 (113) · 7 / 133 · 1,200 · 1,800 · 3,600 |
| 400 ms | 600 | 590 · 10 · 1,600 | 587 · 12 · 121 (113) · 7 / 133 · 1,200 · 1,840 · 3,600 |
| 400 ms | 1,100 | 594 · 3 · 2,100 | 589 · 8 · 125 (117) · 7 / 133 · 1,700 · 2,300 · 3,600 |
| 400 ms | 1,500 | 594 · 3 · 2,500 | 591 · 4 · 129 (120) · 7 / 133 · 2,100 · 2,700 · 3,600 |
| 550 ms (beyond the timeout) | all | equal to the baseline | equal to the baseline |

- **At 400 ms the target still holds.** Input ends at about 600 ms of pause,
  so provider work overlaps about 600 ms of the confirmation; the real-loop
  test above checks at least 500 ms.
  - The median gain against the pilot is 100 ms at L = 0.3 s and 400 ms at
    L = 0.6–1.5 s.
  - Fewer pauses end early, so continuations fall from about 220 to about
    125.
- **Past 500 ms every pause times out.** Both the pilot and the early mode
  then behave exactly like the baseline. That is no gain and no harm.
- **The margin is small.** Main measured about 370 ms on an idle device. The
  result timeout leaves roughly 100 ms for features, IPC and load from a
  conversation, and loaded-device timing is unmeasured.
- Output SHA256: `4b9486ef…397d` (400 ms) and `323cb8fd…3f83` (550 ms).

**Reproduce.**

```sh
cargo build --locked --release -p lamp-endpoint-eval
target/release/lamp-endpoint-eval turns --model smart-turn-v3.2-gpu.onnx \
  --provider-ms 300,600,1100,1500 --out TURNS.jsonl SHARD_4_DIR SHARD_5_DIR
# the verdict cost defaults to 130 ms; add --latency-ms 400 or 550 for the
# A733 sensitivity
```

A shard directory holds the decoded 16 kHz WAVs and the `manifest.json` above.
Provider latencies of 2.5 s or more are refused, because beyond the
patience the kept start is written without first output, which the model does
not describe. The run took 54 s on the M2 with 11 threads.

## Remaining assumptions

1. **Gemini settling a request interrupted before output.**
   - The scripted service settles every interrupted request (first output or
     idle) and answers each request L after its end. One service-only probe
     settled and answered a before-output continuation with `kept-start`; that
     is one case, not a rate.
   - If Gemini never settles, the kept start ends in the bounded barrier
     timeout and the thought is lost. The evaluation reports that column
     separately.
2. **Context.** Nothing shows that the model answers both parts of a
   continued thought, or only the new topic.
3. **Provider latency.** L is fixed per run. A real L varies, and a faster
   provider makes the hold, not the provider, decide when the answer is heard.
4. **Acoustics.** All clips are digital replays: no Lamp microphone, voice
   board, echo, room or loudspeaker. Lamp audio can draw different verdicts.
5. **A733 under load.** Main measured about 370 ms per verdict on an idle
   A733. A loaded device is unmeasured. A verdict later than 500 ms falls
   back to the 1,200 ms window, which means no gain but no harm. The timeout
   stays at 500 ms.
6. **Openings.** A continuation carries a 300 ms pre-roll. Speech that began
   earlier than that is not part of the new request (the "openings lost"
   column).
7. **Physical Stop and mute** are exercised only through emulated peers here.
