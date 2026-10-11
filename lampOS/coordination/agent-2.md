# Agent 2 status

Owner: Agent 2, branch `lamp-a2-endpoint` for MAIN-043 (faster, reliable
end-of-turn detection). Earlier evaluator work stays on `lamp-v2-agent2`
(`95029a03`) and `lamp-v2-agent2-sim` (`1a89a9cb`); neither is merged here.

## MAIN-043: first tested checkpoint

- Base: `1a089863a1bfdfa532a11e8967e6ed87f94ea1d5`.
- Tested code head: `c79a7a49`. This status file and
  [the endpoint detection doc](../docs/endpoint-detection.md) follow it.
- State: an opt-in candidate. No activation, no device action and no deploy.
- Acknowledgment: `e7c491c9`. The Harness channel is unavailable, so Git is the
  record.

### Commits (exact files, one line each)

| Commit | Content | Integration note |
| --- | --- | --- |
| `65bd206a` | New `crates/endpoint` (`lamp-endpoint`): Rust Whisper features, a pinned tract model loader that checks the hash before parsing, a single-flight worker, parity tests and a fixture generator, and `fetch-model.sh`. Also the workspace member and `Cargo.lock` (64 new packages, no existing version changes). | Workspace/lockfile change for Main's agreement |
| `df8f4b57` | `TurnDetector::with_confirmation`, `ConfirmPolicy`, `EndpointDecision` and `last_endpoint()`. The `lamp-live` → `lamp-endpoint` dependency. 18 policy regressions. | Delegated file plus a manifest line |
| `097869d8` | Default threshold 0.8, plus a regression for it. | |
| `b3ae6e4f` | `examples/endpoint_replay.rs` (virtual and paced replay, plus `--classify`) and 55 synthetic `say` cases. | |
| `c79a7a49` | Proposal only: `--endpoint smart-turn` (requires `--turn-taking v1-baseline`, model beside the executable) and `endpoint_policy` / `local_endpoint` trace fields. | Coordinator/CLI change: Main decides |

### Gates

- **macOS** (M2 Max, Rust 1.96.0): `cargo fmt --all --check`; strict Clippy
  for `lamp-endpoint` and `lamp-live` (`--all-targets -D warnings`).
  - `lamp-live`: 435 passed, 0 failed, 2 ignored.
  - `lamp-endpoint` with `--include-ignored` and the model: 8 passed.
- **Linux ARM64** (Docker on Colima VM `lampos`, 6 vCPU, `rust:1.96.0-bookworm`)
  at `c79a7a49`:
  - `cargo fmt --all -- --check` passed.
  - `cargo clippy --locked --workspace --all-targets -- -D warnings` was clean.
  - `cargo test --locked --workspace --no-fail-fast`: 1,192 passed, 0 failed,
    6 ignored. Two of the ignored tests need the model.
  - With the model mounted read-only, `cargo test --locked --release -p
    lamp-endpoint -- --include-ignored`: 8 passed.
- **Failures:** the first Linux run at `c79a7a49` failed 25 tests because the
  VM's disk was full (58/58 GB, from my own stale gate build cache). I removed
  that cache volume directory and re-ran; the result above is that clean run.
  `b3ae6e4f` had passed 1,191/0/6 earlier.

### Measurements

These are not Lamp hardware. Full tables are in the doc.

- **Parity:**
  - Rust features match Pipecat `53d313b1` within 1.2e-6.
  - Probabilities match ONNX Runtime 1.31 within 8.7e-6 over 829 labelled
    upstream clips, with 0 decision flips.
- **Inference:**
  - Linux ARM64 VM: warm 88.5 ms, cold 89–94 ms, load about 160 ms; real-time
    replay p50 118 ms, max 134 ms.
  - macOS uses Apple AMX, so it runs at about 32 ms warm.
- **Memory and size:**
  - Memory is 156–182 MB on the macOS model probe and 118 MB peak for the whole
    Linux replay.
  - The `lamp-live` release binary grows from 7.85 to 31.42 MB (17.3 MB more
    when stripped).
- **Upstream human English test clips** (538, shard 0): at threshold 0.8, 1.9%
  of unfinished clips are judged finished and 96.4% of finished clips are
  recognized. This is classification only.
- **Synthetic `say` replay** (50 single-turn cases):

  | Variant | False splits | p50 delay |
  | --- | --- | --- |
  | `v1-1200` | 7 | 1,196 ms |
  | candidate | 14 (7 new against `v1-1200`) | 1,116 ms |

  The model misjudges these voices: fragments score up to 0.98, and complete
  questions score as low as 0.11. Pipecat's Python reference confirms this, so
  the target of no false splits is not met on this set.

### Requests to Main

1. Agree or reject the workspace/lockfile and dependency choice: tract 0.23.8,
   the fp32 model fetched by hash, 64 packages, 17 MB of binary growth.
   Alternatives: a cargo feature, or a separate worker binary.
2. Decide whether to take the `c79a7a49` CLI/trace wiring. It lacks a
   coordinator-level test for a session with a missing model.
3. Before the physical A/B: the current `say` stimuli cannot qualify this
   candidate. Human-read prompts, or recorded human speech played through the
   loudspeaker, are needed. That is an owner decision.
4. When scheduled: device load, cold and warm inference on the A733, and
   coordinator-loop impact. If inference exceeds 200 ms, 400 ms early endpoints
   become later. Past 500 ms it times out to the silence fallback.

### Remaining gaps

- No device evidence; no tuning on human pauses inside a single turn.
- Pauses longer than 1.2 s still split, as they do today.
- int8 is blocked by the tract QDQ defect. A short answer followed by more
  speech still ends after the short answer.
