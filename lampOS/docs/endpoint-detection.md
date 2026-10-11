# End-of-turn detection (MAIN-051, MAIN-052)

Status: opt-in candidates, not defaults, never run on a Lamp. Main owns
activation, device measurement and every physical cohort.

- `lamp-a2-endpoint-runtime` (`350bd572`): the MAIN-051 checkpoint, kept
  unchanged for review.
- `lamp-a2-endpoint-review` (`052eb703`): that checkpoint plus Main's four
  review fixes.
- `lamp-a2-continuation`: both of those, composed with integration
  `6d81ab49`, plus the MAIN-052 recoverable early end of input.
- The earlier MAIN-043 candidate and its evidence remain on `lamp-a2-endpoint`
  (`ee1eaad8`).

## Summary

- **Separate inference process.** The pinned Smart Turn v3.2 model runs only in
  `lamp-endpoint-worker`. The interaction core links `lamp-endpoint-ipc`, which
  has no model runtime; `lamp-live`'s dependency tree contains no tract.
- **Unchanged behaviour without a usable model.** A missing executable or
  model, loading, a crash, a hang, a malformed answer or a busy worker leaves
  every pause on the existing 1,200 ms silence window. A test shows that the
  fallback ends every turn on exactly the block the baseline does.
- **Resumed speech is safe.** Any speech-scored block ends a pause and makes
  its verdict obsolete. The utterance continues with its opening and resumed
  audio retained, and a late verdict can never end a later turn.
- **Audio-only endpointing cannot give 200–400 ms decisions without cutting
  people off.** On recorded human speech the model calls many pauses between
  finished sentences "complete", and the person then continues. Ending those
  pauses at 400 ms adds a cutoff in 23% of held-out clips.
- **Proposed opt-in profile ("parity").** It ends a confident complete verdict
  at 1,000 ms, holds a clearly unfinished one to 1,500 ms and otherwise keeps
  1,200 ms. On 1,167 fresh held-out human clips:
  - 77% of finished turns end 200 ms earlier;
  - it adds 6 cutoffs (0.5% of clips), 1 of them in 391 short-form clips;
  - 3 finished turns wait 1,500 ms instead of 1,200 ms.
- **Endpoint savings are separate from reply startup.** Main's runs measured
  1.1–1.6 s from the local endpoint to the first ALSA write, in addition to
  the 1.2 s wait. This profile removes at most 200 ms of that wait. Endpointing
  alone cannot reach the 300–500 ms p50 reply target.
- **Recoverable early end (MAIN-052, opt-in).** A confident verdict ends input
  at 500 ms, so the provider starts answering about 700 ms sooner. Nothing is
  spoken until the baseline's 1.2 s of quiet confirms the end. If the person
  continues first, the existing replacement path supersedes the unspoken
  answer.
  - Held-out human clips, with first audio 1.1 s after the end of input: the
    median from speech end to the reply's first audio falls from 2.3 s to
    1.6 s.
  - Whole thoughts are kept only if Gemini settles each interrupted request.
    That is unqualified, and 17% of clips depend on it. See
    [Recoverable early end](#recoverable-early-end-of-input-main-052).

## Architecture

| Crate | Linked by | Content |
| --- | --- | --- |
| `lamp-endpoint-ipc` | `lamp-live` (core) | Request and verdict types, the `Classifier` trait, the bounded frame format, the worker's serving loop and `ProcessClassifier`, the core-side supervisor. Depends only on `lamp-ipc`. |
| `lamp-endpoint` | the worker only | Whisper features, pinned model loading with tract, and the `lamp-endpoint-worker` binary. |
| `lamp-endpoint-eval` | nothing | Offline probe, real-detector replay, synthetic cohort and profiling tools. |

### Process boundary

- **Transport.** The worker's stdin and stdout are pipes carrying
  length-prefixed little-endian frames. Every frame kind has an exact size or
  an explicit bound, checked before allocation. A request holds at most the
  last 8 s of 16 kHz PCM (256 KB); an answer body is at most 523 bytes. Stderr
  is inherited for diagnostics; the protocol never depends on it.
- **One request at a time.** `submit` refuses a second request as `Busy`. Each
  request carries the caller's token (utterance, generation, check) and a
  request number; each worker process has an incarnation number. An answer
  that does not match the outstanding request is a protocol fault.
- **Nothing waits on the caller's thread.** `submit`, `poll`, `availability`
  and `cancel` take one short lock. A writer thread copies the request into the
  pipe, a reader thread validates answers, and a supervisor thread owns the
  child process.
- **Privacy and cancellation.** `cancel` drops queued audio and any unread
  answer, and `close` does the same for an answer already on its way: no
  outcome is delivered after either. The worker writes no files and zeroes its
  request and feature-extraction buffers after every answer. Freed
  allocations and the inference runtime's internal memory are not scrubbed.
- **Lifetime.** On Linux the worker is killed if the core exits first
  (`PR_SET_PDEATHSIG`, after checking the parent PID). Closing the client
  closes the worker's input, allows a 300 ms grace period, then kills the
  worker and reaps it on the supervisor thread. Runtime teardown waits within
  the shared shutdown bound (at least 500 ms) for a confirmed reap. Without
  one, cleanup fails, so no later service cycle can start beside the old
  worker. Losing the worker while running only means the silence fallback.
- **Identity.** `WorkerCommand` pins the model SHA256 the worker must report in
  `Ready`; the core refuses any other identity before sending a request.

### Failure handling

| Condition | Effect | Test |
| --- | --- | --- |
| Worker executable missing | `Failed` at once; every pause uses the baseline window | `missing_executable_fails_at_once` |
| Model missing or wrong bytes | The worker reports a startup error; `Failed`, never restarted | `worker_process_refuses_missing_or_wrong_models_without_blocking` (real binary), `missing_model_session_ends_on_the_unchanged_silence_fallback` (full session) |
| Not ready within 10 s, or a crash or protocol violation before ready | `Failed`, never restarted | `startup_failures_are_final` |
| `Ready` reports a model other than the command's pinned SHA256 | `Failed` before any request is sent; never restarted | `a_worker_reporting_another_model_is_refused_before_any_request` |
| Crash, malformed or unexpected answer, or inference error while serving | Killed if needed and reaped; the outstanding request returns an error to its own token; a replacement starts after a 1 s backoff, at most twice | `serving_failures_are_replaced_within_budget`, `protocol_violations_are_replaced` |
| No answer within 2 s | Treated as hung: killed, reaped and replaced within the same budget. The detector has already fallen back after 500 ms | `hung_worker_never_delays_capture_endpoint_or_session_end` (full session) |
| Close during inference | The core returns at once; the worker is killed after the grace period | `close_during_inference_never_waits` |
| Cancel with an answer in flight | The answer is discarded; the slot frees when it arrives | `cancelled_answers_are_never_delivered` |
| Close with an answer on its way | Nothing is delivered after close; the answer counts as discarded or lost | `no_outcome_is_delivered_after_close_even_for_an_answer_on_its_way` |
| Worker exit not confirmed by a native wait, or not within the teardown bound | No replacement starts; runtime cleanup fails, so a physical mute is not restartable | `unconfirmed_endpoint_exit_fails_mute_cleanup_and_never_reopens_service`, with the control `confirmed_endpoint_exit_keeps_the_restartable_mute` |

The process tests (`crates/endpoint-ipc/tests/process.rs`) re-execute their
own binary as a scripted worker, so every path above runs through a real child
process and real pipes.

Endpoint authority never depends on trace metadata. Decisions come only from
detector state and the client's in-memory availability; the trace events below
are records written afterwards.

## Decision policy (`crates/live/src/activity.rs`)

`TurnDetector::v1_baseline().with_confirmation(classifier, ConfirmPolicy::default())`

1. **Pauses.** A pause is a run of consecutive blocks scoring below 0.35,
   counted exactly as the baseline counts its 1,200 ms window. Any
   speech-scored block, including a breath or a click, ends the pause and makes
   its verdict obsolete. The MAIN-043 candidate counted short blips as pause;
   on recorded human speech that alone added cutoffs, so it was removed.
2. **One request per pause.** After 200 ms of pause the detector submits the
   utterance's latest 8 s: pre-roll, opening and any resumed speech, in order.
   A busy slot is retried each block, but the request always ends 200 ms into
   the pause, because the model's verdict drifts as trailing silence grows.
3. **Three bands** (defaults in parentheses):
   - complete (probability ≥ 0.9): the turn ends at 1,000 ms of pause, or on
     the verdict's arrival if later (`model_complete`);
   - clearly unfinished (< 0.01): held until 1,500 ms (`model_incomplete`);
   - anything between: ends at 1,200 ms (`model_uncertain`).
4. **No usable verdict** (missing, loading, failed, recovering, closed, busy,
   error, or no answer within 500 ms): ends at 1,200 ms (`silence_fallback`,
   with the reason).
5. **Invalidation.** A new utterance, `clear_prospective`, `reset`, a capture
   fault or a speech-scored block makes pending verdicts obsolete and cancels
   the client's queued audio.

`ConfirmPolicy::responsive()` ends a complete verdict at 400 ms. It exists for
evaluation and must not be used without a path that lets a person continue.

Each `Activity::End` records an `EndpointDecision` with these fields:

- `basis`, `silence_ms`, `probability` and `fallback`;
- `verdict`: the worker incarnation and request number, the pause length at
  submission and on arrival, and the snapshot, delivery, features, inference,
  return and round-trip costs.

## Evidence

The cohorts are kept separate. Every result below is a digital replay on a
development host; no audio passed through Lamp microphones or a loudspeaker.

| Cohort | Status |
| --- | --- |
| Digital model-reference parity | Unchanged from MAIN-043: features within 1.2e-6 of Pipecat, probabilities within 8.7e-6 of ONNX Runtime on 829 clips. Rerun through the worker process. |
| Offline replay of recorded human clips | This section |
| Offline replay of synthetic `say` speech | This section |
| Synthetic loudspeaker replay, recorded-human loudspeaker replay, direct human at the Lamp | Not run; Main schedules physical cohorts |

### Recorded human clips

- **Source.** `pipecat-ai/smart-turn-data-v3.2-test`, revision
  `dfc179241fee65b0efe81ca37786f41ce26f38ec`, CC-BY-4.0. Shards were
  downloaded with SHA256 checks and decoded locally; no audio is committed.
- **Clips.** Only human clips (English and Spanish), each labelled finished or
  unfinished at its end. Each clip is replayed with 600 ms of lead-in, a
  2.5 s tail and a −63 dBFS noise floor.
- **What is measured.**
  - A split is an endpoint followed by a new speech onset in the same clip:
    the person continued.
  - The delay is the detector's pause length at the final endpoint.
  - An unfinished clip ending before 1,200 ms means the model committed on an
    unfinished sentence.
  - All counts are against the unchanged baseline on the same audio.
- **Partitions.**
  - Development: shards 0 and 1 (1,180 clips). Every choice was made here.
  - Fresh held-out: shards 4 and 5 (1,167 clips), probed only after the
    profiles were frozen.
  - Shards 2 and 3 were held out from an earlier selection whose pause
    counting did not match the detector. That selection was discarded, so
    they are not reported as held-out evidence.
- **Method.** An offline probe scored every pause at several trailing lengths
  for selection. All reported numbers come from the real `TurnDetector` with
  the model in process and each verdict released 130 ms after submission.

Fresh held-out, real detector, 130 ms verdict latency:

| Profile | Splits (added) | Clips with an added split | Finished turns: mean delay | Ended before 1,200 ms | Waited longer | Unfinished ended early |
| --- | --- | --- | --- | --- | --- | --- |
| baseline (1,200 ms) | 13 | — | 1,200 ms | 0% | 0 | 0 / 569 |
| parity (default) | 19 (+6) | 6 | 1,047 ms | 77% | 3 | 19 / 569 |
| strict (≥ 0.99 at 150 ms, ends at 800 ms) | 15 (+2) | 2 | 1,136 ms | 16% | 0 | 2 / 569 |
| responsive (400 ms) | 351 (+338) | 271 | 583 ms | 77% | 4 | 23 / 569 |
| MAIN-043 candidate | 447 (+434) | 333 | 545 ms | 82% | 0 | 30 / 569 |

Development, same method:

- baseline: 20 splits;
- parity: 21 splits (+4, −3), with 76% of finished turns ending earlier and a
  mean delay of 1,049 ms;
- responsive: 408 added splits.

By source on fresh held-out data, parity versus baseline:

| Source | Clips | Added splits | Finished turns ending earlier |
| --- | --- | --- | --- |
| Short-form English (`human_5`, `midcentury_1`, `human_convcollector_1`) | 300 | 0 | 75–90% |
| Spanish (`mundo_1`) | 91 | 1 | 80% |
| Long-form English (`liva_1`, 12–16 s recordings) | 776 | 5 | 73% |

**Why earlier commits cut people off.**

- At internal pauses of 0.8 s or longer, after which the person kept speaking,
  the model scores 43% as complete 200 ms in.
- At true turn ends it scores 86% complete, with a median of 0.98. At
  unfinished clip ends it scores only 5% complete.
- A pause after a finished sentence looks like a finished turn in the audio.
  On the development data, neither pause history, utterance length nor the
  early score trajectory separated the two cases.

**When a person continues after an early endpoint.**

- Under the responsive profile on development data, 98% of split speakers
  resumed within 1 s of the endpoint (median 240 ms, p90 550 ms).
- Main's measured endpoint-to-first-ALSA time is 1.1–1.6 s, so most of those
  continuations would begin before Lamp speaks.
- Whether the coordinator and Gemini Live can treat such a continuation as
  part of the same request is untested. It is the condition for any profile
  faster than parity.

### Synthetic `say` cohort

`crates/endpoint-eval/endpoint-cases.json` holds 55 spliced macOS `say` cases,
50 of them single-turn. Their final intonation is artificial, so they remain a
stress test, not a tuning target.

| Profile | False splits | Never ended | Fragment cut by the model | Delay p50 / p90 |
| --- | --- | --- | --- | --- |
| baseline | 7 | 2 | 0 | 1,196 / 1,586 ms |
| parity | 6 (adds `planet-slnc-1200`; avoids `filler-1200`, `filler-slnc-1200`) | 2 | 1 | 1,192 / 1,519 ms |
| strict | 7 | 2 | 0 | 1,196 / 1,586 ms |
| responsive | 11 | 1 | 1 | 1,154 / 1,669 ms |
| MAIN-043 candidate | 13 | 1 | 1 | 1,116 / 1,228 ms |

"Never ended" means noise blips kept resetting the baseline's window within the
3 s tail. The fallback behaves identically.

### Preserved earlier results

- The MAIN-043 candidate (400 ms commit, threshold 0.8, blips counted as pause)
  adds 434 cutoffs on the fresh held-out set. It is not proposed.
- A first selection on this branch counted a short blip as pause. The real
  detector replay showed that this differs from the baseline window, so the
  selection was redone with exact baseline counting and evaluated on fresh
  held-out shards. Its files are kept in the local evaluation directory.

## Latency and resources

Neither environment is Lamp hardware:

- **macOS:** M2 Max, Rust 1.96.0. tract uses Apple AMX here.
- **Linux:** an ARM64 Colima VM with 6 vCPUs on the same host, running Docker
  `rust:1.96.0-bookworm`. NEON only.

The measurements come from `lamp-endpoint-eval profile`: 5 cold starts, then
100 requests each for 1, 4 and 8 s windows of one recorded human clip, all
through `ProcessClassifier`.

| Measurement | macOS | Linux ARM64 VM |
| --- | --- | --- |
| Spawn to ready, observed by the core | 164–165 ms (the first launch of a new binary took 1,022 ms) | 241–257 ms |
| Model load (hash check included) / cold inference | 125–157 ms / 35–36 ms | 148–158 ms / 91–98 ms |
| Submit call on the core's thread, 8 s window, p50 / max | 67 / 346 µs | 72 / 148 µs |
| Snapshot copy / pipe delivery / answer return, p50 | 64 / 83 / 14 µs | 65 / 199 / 36 µs |
| Features / inference, p50 | 2.8 / 34.6 ms | 3.0 / 91.2 ms |
| Round trip, p50 / p90 / max | 37.6 / 40.9 / 44.3 ms | 94.5 / 97.2 / 100.9 ms |
| Worker CPU per verdict | about 37 ms (one thread) | about 95 ms (one thread) |
| Worker memory | 169–191 MB resident after ready; 177 MB after 300 requests | 59 MB resident, 102.5 MB peak; unchanged after 300 requests |
| Binary sizes | n/a | `lamp-live` 8,402,816 → 8,504,488 bytes (stripped 6,165,560 → 6,231,096); `lamp-endpoint-worker` 24,226,752 (stripped 17,776,360) |

Other measurements:

- **Real-time replay.** A real-time replay of every tenth fresh held-out clip
  through real worker processes made all 234 decisions (117 clips, two
  profiles) exactly as the virtual replay did. The verdict round trip was a
  38.8 ms median and 72.5 ms maximum on macOS.
- **Core memory.** The core's added memory is the client: three threads per
  worker and at most one 256 KB request frame.
- **Feature reuse.** Feature extraction is 3% of each verdict's cost, and the
  parity profile does not act until 1,000 ms. Reusing features or classifying
  while speech still arrives would not change its decisions, so neither is
  implemented. The responsive profile's 400 ms commit is not limited by
  inference either: the verdict arrives about 100 ms after the 200 ms check.

## Recoverable early end of input (MAIN-052)

### What the current runtime does with resumed speech

Read from integration `6d81ab49`, and unchanged by this work:

1. **A local endpoint sends the provider's end of input at once.** It becomes
   Gemini's `activityEnd`, and the model starts answering; there is no undo.
2. **Later speech is a new candidate, not a continuation.**
   - If no speaker sample has been dispatched, `commit_admitted_input` cancels
     the pending reply (`user_interrupted`) and admits the speech as
     `AdmittedInput::Interruption`.
   - The new request carries the speech's pre-roll, up to 300 ms before the
     onset.
   - The provider worker turns the old request into a barrier and sends
     `activityStart`, the protocol's only interruption.
3. **The barrier depends on Gemini.**
   - The new request's own end is held until Gemini settles the old request
     (`turnComplete`, idle).
   - Under the production `BarrierPolicy::Require`, 2 s without that
     settlement fails the connection.
   - One real-device run hit exactly this: a continuation 241 ms after an
     endpoint lost the second request (`real-device-audio-pilot.md`).
4. **Context retention is unverified.** Nothing qualifies whether the model
   keeps the earlier user activity in context, or answers the whole thought.
5. **Stale output is already filtered.** Old-request output is dropped by
   request number in the actor, the worker and the coordinator.
6. **Agent 1's proposal changes none of this.** `lamp-a1-duplex` (`feb56076`)
   handles this window the same way.

### The proposal

- **Detector** (`activity.rs`, `EarlyEnd`):
  - A verdict of at least 0.9 at 200 ms ends input at 500 ms of pause
    (`model_early`). The detector then marks the endpoint provisional.
  - It is confirmed (`EndpointEvent::Confirmed`) by the baseline's own 1.2 s
    of consecutive quiet. While noise blips keep resetting that count, it is
    confirmed anyway 3 s after the early end.
  - Speech onset before confirmation reports `EndpointEvent::Continued`. The
    provisional state is then held until the continuing utterance ends, or
    until its candidate is rejected and input is cleared.
  - Without a confident verdict nothing changes: a test shows that a missing
    or undecided model ends every turn on the baseline's exact block.
- **Coordinator** (`--endpoint smart-turn-early`, requires `v1-baseline`):
  - Speaker dispatch is skipped while the endpoint is provisional.
  - No deadline is extended: the provider's response clocks are unchanged,
    and the hold is bounded by the detector.
  - Each outcome is traced as `endpoint_provisional`.
- **Not done.**
  - No topic change is inferred. The available evidence cannot separate a
    topic change from a continuation, and input transcripts carry no
    reliable request ordering. Every resumption keeps the whole utterance:
    the earlier part has reached the provider, and the continuation follows.
  - No audio is replayed, and no provider confirmation is assumed.
- **Provider dependency, named.**
  - A continuation after an early end relies on Gemini settling the
    interrupted request within the 2 s barrier, and on keeping the earlier
    activity in context.
  - Until a scheduled probe qualifies both, the mode stays opt-in and must
    not be used in a pilot.
  - The barrier path is not new. The baseline already takes it whenever a
    person resumes after 1.2 s but before the reply is audible; on held-out
    data that is 9 of 1,167 clips.

### Why the hold is required

With the gate removed, four session tests fail. That includes a continuation
whose superseded answer arrived 300 ms after the early end. Admitting the
continuation takes a few decision blocks, and the old answer was dispatched in
between. The hold makes the outcome independent of provider speed.

### Runtime regressions (`coordinator/tests/endpoint_session.rs`)

The real `conversation()` loop runs with emulated capture, privacy, provider
and speaker peers:

- the provider is credit-limited, emits `Started`, and answers after a set
  delay;
- the speaker returns genuine acceptance receipts.

Tests:

- `early_end_starts_the_answer_sooner_but_stays_silent_until_confirmed`;
- `continuation_while_the_old_answer_arrives_replaces_it_and_keeps_both_parts`:
  the provider's audio for the first part arrives during the resumed speech;
- `answer_held_by_the_gate_is_dropped_when_the_person_continues`;
- `two_continuations_then_a_genuine_end`;
- `privacy_during_the_provisional_end_stops_without_any_answer`;
- `early_mode_without_a_usable_model_is_the_baseline`: missing and hung
  workers.

What they check:

- only the final request is ever spoken;
- the continuation's request carries its opening and every resumed block;
- the superseded turn ends `user_interrupted` and never reaches the speaker;
- late `TurnComplete` and audio for a superseded request do nothing.

### End-to-end evaluation

`lamp-endpoint-eval turns` runs the production detector, model and VAD over
each clip. It then applies the coordinator and provider seam above as a model:
the first reply audio is ready a fixed time L after the end of input. These
are digital replays, not loudspeaker or room cohorts.

Metrics:

- **Whole thought:** every part reached the provider and no answer became
  audible before the person finished. It does not mean the model answered the
  whole thought; that is the provider dependency.
- **Split:** an answer was audible before the person continued.
- **Interruption:** a request superseded before any audible output, which is
  the barrier path.
- **Opening lost:** a resumed burst began more than the 300 ms pre-roll
  before its onset.

Fresh held-out human clips (shards 4 and 5, 598 finished turns), L = 1.1 s, as
in Main's measurements:

| Profile | Whole thoughts if Gemini settles | if it never settles | Splits | Interruptions (clips) | Openings lost | Latency mean / p50 |
| --- | --- | --- | --- | --- | --- | --- |
| baseline | 592 | 586 | 4 | 9 (9) | 4 / 13 | 2,300 / 2,300 ms |
| parity | 594 | 585 | 3 | 16 (16) | 4 / 19 | 2,147 / 2,100 ms |
| early, 0.9 at 500 ms | 588 | 495 | 9 | 222 (194) | 8 / 231 | 1,838 / 1,600 ms |
| early, 0.98 at 600 ms | 589 | 559 | 8 | 61 (58) | 6 / 69 | 1,979 / 2,100 ms |

Latency is measured from the first quiet block to the first audible block.

- **Development** (shards 0 and 1, 589 finished turns), same L:
  - baseline: 583 whole thoughts, 3 splits, 17 interruptions, 2.3 s;
  - early 0.9 at 500 ms: 582 whole thoughts, 5 splits, 287 interruptions, a
    1.6 s median.
- **Faster provider** (L = 600 ms): the early mode reaches a 1.2 s median,
  against the baseline's 1.8 s, with 12 splits against 8. The hold then binds.
- **Synthetic `say` cohort** (54 cases, L = 1.1 s): baseline 52 and early 51
  whole thoughts if Gemini settles. The voices rarely draw a confident
  verdict, so latency barely moves.

Reading:

- The early mode is a real latency gain: a 700 ms lower median at today's
  provider speed.
- The cost is a few extra audible splits. Answering sooner leaves less time
  for someone who resumes 1.6–2.3 s later.
- Its safety rests entirely on the unqualified provider settlement:
  - if Gemini never settles an interrupted request, 91 more held-out thoughts
    fail than with the baseline;
  - at 0.98 and 600 ms, 27 more fail.

## Pilot configuration (MAIN-055)

For a supervised pilot, use either of these; both keep today's endpoint as the
fallback whatever happens to inference:

1. Today's behaviour: `--turn-taking v1-baseline`. No endpoint worker is
   started.
2. The parity profile: `--turn-taking v1-baseline --endpoint smart-turn`, with
   `lamp-endpoint-worker` and the verified model beside `lamp-live`
   ([Installation](#installation)). Expect at most 200 ms less endpoint wait
   on confident finished turns.

Do not use `--endpoint smart-turn-early` in the pilot until a scheduled probe
shows that Gemini settles a request interrupted before it answered.

Bounded behaviour with `smart-turn`:

| Situation | What happens |
| --- | --- |
| Worker or model missing, wrong model bytes, or a startup error | No restart; every pause uses the 1,200 ms window; `endpoint_worker` records the reason |
| Loading (about 0.25 s on the Linux VM; the A733 is unmeasured) | Pauses before ready use the 1,200 ms window |
| Inference slower than 500 ms | That pause uses the 1,200 ms window (`timeout`) |
| No answer within 2 s, a crash or a malformed answer | The worker is killed and reaped, and at most two replacements start, each after 1 s. Meanwhile pauses use the 1,200 ms window |
| Mute, stop or session end | The client closes at once; the worker is reaped within the shutdown bound, or the cleanup fails so no new service cycle starts |

Check these trace fields after each run:

- `endpoint_policy`;
- `endpoint_worker` (availability changes);
- `local_endpoint.endpoint.basis`, `fallback` and `verdict`;
- `endpoint_worker_final`: `reaped` and the counters.

## Integration proposal

Shared files are in separate commits for Main to compose:

| Commit | Content |
| --- | --- |
| `d8d98b2e` | New `crates/endpoint-ipc` and `crates/endpoint`: worker binary, model, features and process tests |
| `da4824f5` | Workspace members, the `lamp-live` → `lamp-endpoint-ipc` dependency and the lockfile (65 new packages, no existing version changed) |
| `a3194f92`, `2021d665` | Detector seam, baseline-exact pauses, three-band policy and regressions (`activity.rs`, `tests/endpoint_confirmation.rs`) |
| `58d20b00`, `8d204921` | Model identity constants, read-only monitor, teardown wait |
| `5b9f7164`, `d3e92a59`, `492cfd9c` | Coordinator proposal: `--endpoint smart-turn`, worker start in `spawn_rig`, endpoint traces, teardown reap and full-session tests (`coordinator.rs`, `coordinator/endpoint.rs`, `options.rs`, test `Rig` literals) |
| `a0980e98`, `7c16828c`, `61112832` | Evaluation crate and its workspace entry |
| `201510f2` (review) | Pinned worker model identity, typed confirmed reap, no outcome after close, cleared feature buffers |
| `052eb703` (review, shared) | An unconfirmed endpoint worker teardown fails runtime cleanup, with physical-mute regressions |
| `425a76a7` | lampOS stays Rust-only: Python fixture generator and shell fetch script removed; `--check-model`; fixture provenance |
| `f90771bb`, `8e13d660` | `EarlyEnd`, provisional confirmation, continuation events and the continuation hold (`activity.rs`, tests) |
| `31ca4be2`, `c8f5329f` (shared) | `--endpoint smart-turn-early`, the speaker hold, `endpoint_provisional` traces and full-session continuation tests |
| `196e0e3a`, `ae682db1`, `bddacf48` | End-to-end turn evaluation, human and `say` cohorts |

- `--endpoint smart-turn` requires `--turn-taking v1-baseline`; `silence`
  stays the default.
- The worker and model are found beside the `lamp-live` executable
  (`lamp-endpoint-worker`, `smart-turn-v3.2-gpu.onnx`). The client starts at
  the beginning of `spawn_rig`, so loading overlaps the other workers' startup.
- Traces:
  - `endpoint_policy`: the policy, worker command and model hash;
  - `endpoint_worker`: each availability change, with the worker report;
  - `local_endpoint.endpoint`: the decision and its verdict provenance;
    `includes_silence_wait_ms` is the actual pause;
  - `endpoint_worker_final`: counters, and whether the worker was reaped.

## Installation

```sh
cargo build --locked --release -p lamp-live -p lamp-endpoint
curl -fsSL --max-filesize 33000000 -o target/release/smart-turn-v3.2-gpu.onnx \
  https://huggingface.co/pipecat-ai/smart-turn-v3/resolve/f766f81d3cfdf7737ac64aad813d91bbfd56bf93/smart-turn-v3.2-gpu.onnx
target/release/lamp-endpoint-worker --check-model target/release/smart-turn-v3.2-gpu.onnx
target/release/lamp-live directed CONFIG SECONDS OUT --turn-taking v1-baseline --endpoint smart-turn
```

Delete the file if `--check-model` reports a mismatch. The worker refuses any
other bytes at runtime as well, and lampOS ships no download script: the
owner's rule is that every source file and script in lampOS is Rust.

- **Model.** `pipecat-ai/smart-turn-v3` revision
  `f766f81d3cfdf7737ac64aad813d91bbfd56bf93`, `smart-turn-v3.2-gpu.onnx`:
  - 32,411,198 bytes, SHA256
    `ab8dc64b88713f90b571c15b714bd1330e6c883cad8763dacf65c9376dc539be`;
  - BSD-2-Clause;
  - the worker refuses any other bytes.
- **Runtime.** tract 0.23.8, MIT or Apache-2.0.
- **int8 model not used.** tract mis-executes the int8 export's uint8
  quantize/dequantize pairs.

## Commands

```sh
cargo test -p lamp-endpoint-ipc                       # process supervision, frames
cargo test -p lamp-endpoint -- --include-ignored      # parity and the real worker (needs the model)
cargo test -p lamp-live --test endpoint_confirmation  # policy and baseline equivalence
cargo test -p lamp-live --lib endpoint_session        # full sessions; add --include-ignored with
                                                      # LAMP_ENDPOINT_WORKER and LAMP_SMART_TURN_MODEL
EVAL=target/release/lamp-endpoint-eval
$EVAL probe   --model MODEL --out PROBE.jsonl SHARD_DIR...
$EVAL vad     --out VAD.jsonl SHARD_DIR...
$EVAL replay  --model MODEL --latency-ms 130 --out REPLAY.jsonl SHARD_DIR...
$EVAL replay  --mode paced --worker WORKER --model MODEL --out REPLAY.jsonl SHARD_DIR...
$EVAL cases   --cases crates/endpoint-eval/endpoint-cases.json --cache .cache/endpoint/speech --model MODEL --out CASES.json
$EVAL profile --worker WORKER --model MODEL --speech WAV_AT_LEAST_8S --out PROFILE.json
```

A shard directory holds 16 kHz mono WAVs and a `manifest.json`. The manifest
records each clip's label, language, dataset and PCM SHA256, as decoded from
the pinned parquet shard.

## Limitations

1. **No device evidence.** Startup, inference and memory on the A733 are
   unmeasured; the host and VM numbers above do not predict them. A slower
   verdict does not change the parity profile's 1,000 ms commit unless it
   exceeds the 500 ms result timeout.
2. **Digital replay only.** None of the human clips passed through the Jieli
   voice board or a loudspeaker. Physical cohorts may score differently.
3. **Labels and context.**
   - The upstream labels describe only each clip's end, and there are no
     transcripts.
   - The long-form recordings' conversational context is unknown.
   - Only English and Spanish are covered.
4. **Small gain.** The parity profile saves at most 200 ms of the endpoint
   wait. Larger gains need a continuation path that lets a person keep
   speaking after an early endpoint; that is Main's and Agent 1's domain.
5. **Memory and size.** The worker is a separate 24 MB binary with its own
   resident memory; see the table above. A smaller runtime or a corrected int8
   path is future work.
6. **Endpointing only.** This does not identify an addressee and is not an
   interruption gate.
7. **The early end depends on the provider.** Its safety needs a probe that
   shows Gemini settling a request interrupted before it answered, and the
   model keeping both parts of the thought. Neither has been run; no cloud
   action belongs to this work.
8. **Soft continuations.** The continuation's pre-roll is 300 ms. In 8 of 231
   held-out resumptions the resumed burst began earlier than that, so the
   earliest audio was not part of the new request.
