# Endpoint detection candidate (MAIN-043)

Status: an opt-in Rust candidate on `lamp-a2-endpoint`, built on base
`1a089863a1bfdfa532a11e8967e6ed87f94ea1d5`. It is not activated: no device run
and no physical A/B have taken place. Main decides activation after review and
the one-variable physical comparison.

## Summary

- **What the candidate does.** The local Smart Turn v3.2 model confirms V1-baseline pauses in Rust.
  - A pause the model judges finished ends after 400 ms.
  - Every other case keeps today's 1,200 ms silence window, including a pause
    judged unfinished and a missing, loading, busy, slow or failed model.
  - Capture never waits for the model.
- **Rust matches the upstream reference.**
  - The features match Pipecat's Python reference within 1.2e-6.
  - Probabilities match ONNX Runtime within 8.7e-6 across 829 labelled
    upstream test clips, with no decision flips at 0.5.
- **The model works on people.** On 538 English human clips from the upstream
  test set:
  - At the default threshold of 0.8, it misjudges 1.9% of unfinished clips as
    finished.
  - It recognizes 96.4% of finished clips as finished.
- **The model does not work on the rig's `say` voices.** The physical rig plays
  macOS `say` voices, which the model misjudges.
  - On 50 single-turn synthetic cases the candidate splits 7 questions that the
    1,200 ms baseline keeps whole.
  - Median end delay improves only from 1,196 to 1,116 ms.
  - The current `say` stimuli therefore cannot qualify this candidate. See
    [Limitations](#limitations-and-open-items).
- **Device latency is unknown.**
  - Inference takes 88–134 ms in a Linux ARM64 VM on an M2 Max host, without
    Apple's AMX matrix unit.
  - On the Lamp's Cortex-A76/A55 cores it will be slower.
  - It is bounded by a 500 ms result timeout that falls back to silence.

## V1 audit (pinned `d5efe9d7`)

The standard Lamp profile (`robots/lamp/rootfs/opt/hal/.env`) uses the gated
path:

- `HAL_REALTIME_TURN_DETECTION=off` and `HAL_LIVE_MODE=false`, so Gemini
  automatic activity detection is disabled and the endpoint is local.
- Capture runs in 64 ms frames. A frame is silent when its RMS is below 500 or
  Silero (threshold 0.15, 192 ms windows) judges it non-speech.
- `turn_should_close` proposes the end after 1.2 s of silence, or 0.8 s after
  an STT final.
- `TurnEndpoint` then consults Smart Turn: Pipecat
  `LocalSmartTurnAnalyzerV3`, the bundled `smart-turn-v3.2-cpu.onnx`,
  single-flight, tokens per capture and generation.
  - A complete verdict closes the turn.
  - An incomplete verdict holds it until 6 s.
  - A missing or failed model closes it after 2.5 s.
- Text hints (fillers, greetings) need STT text and are not ported.
- Gemini input transcripts are not assumed here.

Kept from V1: verdicts tied to a token for the current utterance and
generation, one inference at a time, a sticky failure state, a silence fallback,
and resumed speech discarding a verdict. Changed: a verdict is requested after
200 ms of pause instead of after the 0.8–1.2 s proposal, and incomplete holds
for 1.2 s instead of 6 s.

## Model and inference

| Item | Value |
| --- | --- |
| Model | `pipecat-ai/smart-turn-v3` revision `f766f81d3cfdf7737ac64aad813d91bbfd56bf93`, `smart-turn-v3.2-gpu.onnx`, 32,411,198 bytes, SHA256 `ab8dc64b88713f90b571c15b714bd1330e6c883cad8763dacf65c9376dc539be`, BSD-2-Clause |
| Fetch | `crates/endpoint/fetch-model.sh` writes it to `.cache/endpoint/` after checking the hash. It is never committed. |
| Runtime | `tract-onnx =0.23.8`: pure Rust, MIT/Apache-2.0, MSRV 1.91. Default features are off. 64 new lockfile packages; no existing versions changed. |
| Input | Up to the last 8 s at 16 kHz, zero-padded at the front. The waveform is normalized to zero mean and unit variance, then Whisper-tiny log-mel features: 400-point periodic Hann window, hop 160, 80 Slaney mel bands, trailing frame dropped, 8-decade clamp, `(x + 4) / 4`. This follows Pipecat `53d313b1`'s `_whisper_features.py` and `local_smart_turn_v3.py`. |

**Why the unquantized model.** Pipecat ships `smart-turn-v3.2-cpu.onnx`
(int8, SHA256 `2bb02631…967e4f`), but tract 0.23.8 mis-executes it:

- Every uint8 softmax quantize/dequantize pair outputs 255 × scale.
- One input yields 0.321 under tract and 0.981 under ONNX Runtime 1.31.

The fp32 export has no such operations and matches ONNX Runtime. On the
labelled human clips below it is also more accurate than int8: a 4.6% versus
5.7% false-complete rate at 0.5, and 26 of 829 decisions differ between the two.

**Parity tests.** `crates/endpoint/tests/fixtures/pipecat-reference.json` comes
from `make_reference.py`, a test oracle that runs Pipecat's pinned feature code
and ONNX Runtime; it is not used at runtime.

- Four integer-generated signals (padded, truncated, short and silent) match
  within 1.2e-6 for features and 5.1e-6 for probability.
- The worker test checks single-flight operation, token tagging and window
  truncation.
- An unexpected model size or hash is refused before parsing.
- A missing model fails without blocking the caller.

## Decision policy (`activity.rs`)

`TurnDetector::v1_baseline().with_confirmation(classifier, ConfirmPolicy::default())`.
The detector's own silence window (1,200 ms) is the fallback.

1. **Snapshot.** Each utterance keeps the latest 8 s of its PCM: the pre-roll
   prefix, the opening, and resumed speech, in order.
2. **One classification per pause.** The first submission happens after 200 ms
   of pause. A busy slot is retried each block, but the snapshot always ends
   200 ms into the pause, because the model's verdict drifts as trailing
   silence grows (see the synthetic table below).
3. **Complete verdict** (probability ≥ 0.8). The turn ends once the pause
   reaches 400 ms, or when the verdict arrives if that is later. The 200 ms gap
   absorbs comma pauses before they can end a turn.
4. **Incomplete verdict.** The turn holds until 1,200 ms of pause, the same as
   today.
5. **Fallback.** These cases end at 1,200 ms with `silence_fallback` and a
   recorded reason:
   - not loaded (`loading`), failed (`failed`) or closed (`closed`);
   - a slot that stays busy (`busy`);
   - an inference error (`failed`);
   - no result within 500 ms (`timeout`).
6. **Resumed speech.** Three consecutive blocks scoring at least 0.35 bump the
   generation, discarding any pending or arrived verdict and resetting the
   pause.
   - Shorter blips count as pause, so periodic noise cannot keep a turn open.
     (The strict baseline never ended two synthetic cases because of such
     blips.)
   - A turn never ends on a block that scored as speech.
7. **Invalidation.** `reset`, `clear_prospective` (suppression or
   cancellation), capture faults, and starting a new utterance all invalidate
   pending verdicts. A late result can never close a later utterance.
8. **Bounds.** At most one request (8 s, 256 KB copy) and one result exist at a
   time. `push()` does one non-blocking poll and at most one non-blocking
   submit. Dropping the worker never waits for a running inference.

Each `Activity::End` records an `EndpointDecision`:

- `basis`: `silence`, `model_complete`, `model_incomplete` or
  `silence_fallback`;
- `silence_ms`, `probability`, `fallback`, `features_us` and `inference_us`.

The proposed coordinator commit copies these fields into `local_endpoint`.

Unchanged: playback suppression, multi-speaker admission, and interruption or
barge-in behaviour. Endpointing does not identify an addressee.

## Integration proposal (separate commit, Main's decision)

- `--endpoint smart-turn` is valid only with `--turn-taking v1-baseline`.
  `silence` is the default.
- The worker loads the model from beside the `lamp-live` executable at session
  start. Until the model is ready, and after any failure, every pause uses the
  silence fallback.
- `endpoint_policy` records the policy and the model hash.
  `local_endpoint.includes_silence_wait_ms` now reports the actual pause.
- Not done:
  - a coordinator-level regression for a session with a missing model;
  - deployment of the model file;
  - a choice between a cargo feature or a separate worker binary, to avoid the
    binary growth below.

## Measurements

Environments:

- **macOS:** M2 Max, macOS 26.6.2, Rust 1.96.0. tract uses Apple AMX there.
- **Linux:** Linux ARM64 Docker container in a Colima VM (6 vCPU) on the same
  host. NEON only, no AMX.

Neither is Lamp hardware.

| Measurement | macOS host | Linux ARM64 VM |
| --- | --- | --- |
| Model load | 121–169 ms | 161–164 ms |
| First inference after load (run on silence while loading) | 35–36 ms | 89–94 ms |
| Warm features + inference, isolated test | 2.6 + 32 ms | 2.9 + 88.5 ms |
| Decisive verdicts in a real-time replay (71) | p50 49.7, max 78 ms | p50 118, max 134 ms |
| Memory | 156 MB after load, 173 MB after 20 inferences, 182 MB peak (model probe) | 118 MB peak for the whole replay process |
| `lamp-live` release binary | n/a | 7.85 → 31.42 MB (stripped 5.77 → 23.09 MB) |

Real-time paced replays with the real background worker made the same
decisions, with identical delays, as virtual replays with emulated 100 ms and
250 ms latency.

### Labelled human speech (classification, not policy replay)

Source: `pipecat-ai/smart-turn-data-v3.2-test` revision `dfc17924`, shard 0
(SHA256 `a87c7580…37257a`), CC-BY-4.0. It was downloaded locally and is not
committed. The Rust features and model each scored every clip once.

| English human, 538 clips (277 finished) | Accuracy | Unfinished judged finished | Finished recognized |
| --- | --- | --- | --- |
| threshold 0.5 | 0.965 | 4.6% | 97.5% |
| threshold 0.8 (default) | 0.972 | 1.9% | 96.4% |
| threshold 0.9 | 0.957 | 1.5% | 93.1% |

Other subsets at 0.5:

- English modern synthetic voices (251 clips): accuracy 0.928.
- Human speech in other languages (40 clips): accuracy 0.900.

The labels describe only each clip's end. Clips are conversational, with a
median length of 8.9 s and internal sentence boundaries, so they cannot
measure false splits of the full policy.

### Synthetic `say` replay (55 cases)

Inputs:

- `crates/live/examples/endpoint-cases.json`, rendered with the acoustic
  cache's `say` arguments, plus a deterministic ±40 LSB noise floor.
- Spliced cases join separately rendered fragments, which end with
  sentence-final intonation.
- `[[slnc N]]` cases insert silence inside one rendered sentence.
- 50 single-turn cases, 4 ambiguous, 1 fragment.

| Variant | False splits | Splits the 1,200 ms baseline avoids | Never ended | End delay p50 / p90 |
| --- | --- | --- | --- | --- |
| `vad-600` | 22 | 15 | 0 | 592 / 628 ms |
| `v1-1200` (current) | 7 | — | 2 (noise blips) | 1,196 / 1,586 ms |
| candidate (default) | 14 | 7: `planet-600`, `planet-900`, `planet-slnc-600/900/1200`, `hesitant-slnc-1500`, `thinking-slnc-800` | 0 | 1,116 / 1,219 ms |

- In 16 single-turn cases the model judged the end finished. Those ended at a
  median of 326 ms after the end of speech.
- In 20 cases the model judged the end unfinished. That includes complete
  questions such as "Lamp, can you tell me a short story about a friendly
  robot?" (p = 0.11), which then waited the full 1,200 ms.

Pipecat's Python reference confirms the model's behaviour on these voices, so it
is not a Rust defect:

- "hello" scores 0.80 with 200 ms of trailing silence and 0.015 with 1,200 ms.
- "story" goes from 0.12 to 0.98 over the same range.
- The fragment "Lamp, what's a good name for," scores 0.98.

These macOS voices appear to fall outside the model's training data.

## Limitations and open items

1. **No device evidence.** The Lamp's Allwinner A733 has Cortex-A76 and A55
   cores, and the worker thread is not pinned. Inference may exceed 200 ms, or
   occasionally the 500 ms timeout, which makes it fall back to silence. Main
   should measure load, cold and warm latency and coordinator loop impact on
   the device.
2. **The rig's stimuli are out of distribution.**
   - A physical A/B that plays only the existing `say` stimuli will mostly
     measure this mismatch.
   - Prompts read by people, or recorded human speech played through the
     loudspeaker, are needed to qualify the candidate.
   - Changing the stimuli is an owner decision.
3. **Memory and binary size.** The model costs about 100–150 MB of resident
   memory, and `lamp-live` grows by 17 MB stripped. A smaller runtime, or the
   int8 model once tract handles uint8 QDQ correctly, is future work.
4. **Policy tuning has little evidence behind it.** The 0.8 threshold comes
   from one upstream test shard. The 400 ms commit pause and 1,200 ms
   incomplete hold are conservative choices, not tuned results. Pauses longer
   than 1,200 ms still split, as they do today.
5. **Endpointing only.** This is not addressee detection, natural barge-in or an
   interruption gate. "Yes." followed 600–900 ms later by more speech ends after
   "Yes." (2 of 3 such cases).

## Commands

```sh
crates/endpoint/fetch-model.sh
cargo test -p lamp-endpoint -- --include-ignored          # parity, worker, model refusal
cargo test -p lamp-live --test endpoint_confirmation       # 19 policy regressions, no model
cargo run --release -p lamp-live --example endpoint_replay -- \
  --cases crates/live/examples/endpoint-cases.json --cache .cache/endpoint/speech \
  --model .cache/endpoint/smart-turn-v3.2-gpu.onnx --mode virtual --latency-ms 100 --out REPORT.json
# --mode paced uses the real worker in real time; --threshold, --check-ms,
# --commit-ms and --incomplete-ms override the policy.
cargo run --release -p lamp-live --example endpoint_replay -- \
  --classify DIR_WITH_labels.json --model .cache/endpoint/smart-turn-v3.2-gpu.onnx --out SCORES.json
```
