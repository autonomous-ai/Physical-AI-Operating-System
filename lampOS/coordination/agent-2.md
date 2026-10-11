# Agent 2 status

Owner: Agent 2, branch `lamp-a2-endpoint` for MAIN-043 (faster, reliable
end-of-turn detection). Earlier evaluator work stays on `lamp-v2-agent2`
(`95029a03`) and `lamp-v2-agent2-sim` (`1a89a9cb`); neither is merged here.

## MAIN-043: acknowledged

- Base: `1a089863a1bfdfa532a11e8967e6ed87f94ea1d5` (`origin/lamp-v2-chat`).
- State: interface checkpoint. No coordinator, CLI, wire or provider change yet;
  no device action.

### Initial findings (read before code)

- Current Rust: `TurnDetector::v1_baseline()` ends after 120 consecutive 10 ms
  blocks with VAD probability < 0.35. The 1,200 ms wait applies to every turn.
- Pinned V1 `d5efe9d7`, standard Lamp profile: the gated path with
  `HAL_REALTIME_TURN_DETECTION=off`, so Gemini automatic activity detection is
  disabled and the endpoint is local.
  - A silence clock proposes the end after 1.2 s (Lamp `SILENCE_TIMEOUT`), or
    0.8 s after an STT final.
  - `TurnEndpoint` then asks Smart Turn v3 (Pipecat `LocalSmartTurnAnalyzerV3`,
    single-flight, tokens per capture and generation) for confirmation.
  - Complete closes the turn. Incomplete holds until 6 s. A missing or failed
    model closes after 2.5 s.
  - Text hints (fillers, greetings) depend on STT and are not ported.
- Model, pinned:
  - `pipecat-ai/smart-turn-v3` revision `f766f81d3cfdf7737ac64aad813d91bbfd56bf93`,
    BSD-2.
  - Pipecat `53d313b1` bundles the same `smart-turn-v3.2-cpu.onnx`
    (SHA256 `2bb02631…967e4f`).
  - Preprocessing contract: up to 8 s at 16 kHz, zeros padded at the front,
    Whisper-tiny log-mel (n_fft 400, hop 160, 80 Slaney mels), waveform
    zero-mean/unit-variance first.
- Inference dependency: `tract-onnx =0.23.8`, pure Rust, MIT/Apache, MSRV 1.91;
  `ort` is still a release candidate and needs native ONNX Runtime binaries.
  - Reproduced defect: tract 0.23.8 mis-executes the int8 QDQ CPU model. Every
    uint8 softmax quantize/dequantize output becomes 255 × scale, so it returns
    0.321 where ONNX Runtime 1.31 returns 0.981 on the same input.
  - The unquantized `smart-turn-v3.2-gpu.onnx` (SHA256
    `ab8dc64b88713f90b571c15b714bd1330e6c883cad8763dacf65c9376dc539be`, 32 MB)
    matches ONNX Runtime to 1e-7 under tract. It is selected, pending the
    measurements below.

### Interface proposal (first checkpoint)

- New crate `crates/endpoint` (`lamp-endpoint`) containing:
  - Rust Whisper features;
  - a model loader that checks the SHA256 before parsing;
  - a single-flight background worker (bounded request slot, token-tagged
    results, load/failure state).
- `TurnDetector::v1_baseline().with_confirmation(classifier, policy)`:
  - Capture and `push()` stay synchronous and non-blocking.
  - After a short low-VAD run, an 8 s snapshot of the retained utterance
    (pre-roll plus resumed speech) is submitted.
  - A complete verdict for the current utterance and generation ends the turn
    early.
  - Resumed speech, reset, suppression, discontinuity and privacy loss
    invalidate pending results.
  - A missing, loading, failed, busy or timed-out model falls back to the
    unchanged 1,200 ms silence endpoint. An incomplete verdict has a hard bound.
- Coordinator activation and trace fields will be a separate integration patch
  for Main; the existing `Activity::End` path is unchanged.
- Workspace member, `lamp-live` dependency and `Cargo.lock` (about 70
  third-party packages from tract) also need Main's agreement. They will be
  isolated in their own commit.
- First tested checkpoint estimate: same day (features, model and worker with
  parity tests, then the detector seam with deterministic policy regressions).
  Synthetic macOS `say` replay measurements follow. Device latency is
  unmeasured until Main schedules it.
