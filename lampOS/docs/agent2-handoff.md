# Agent 2 handoff: camera frames, perception, relay hardening

Workstream branch: `lamp-v2-agent2` (Agent 2). The main agent owns
integration into `lamp-v2-chat`; this branch never pushes there.

**Checkpoint 1:**
- **Base:** `2535fb565` (`lamp-v2-chat`, "Assign remaining Rust runtime
  implementation work").
- **Code checkpoint:** `9bfd5824`.
- **This document:** the commit that adds it, directly on top of `9bfd5824`.

Nothing on this branch has run on Lamp, opened a camera, played audio or seen
a real person. All results are host (macOS, Apple silicon) or Linux ARM64
container results.

## Commits

| Commit | Scope | Behavior |
|---|---|---|
| `f7700f0b` | `crates/voice-eval/**` (relay), `docs/voice-eval.md` | Bounded incremental relay framing at both ends: 64 KiB per line, 96 MiB per session, 16 KiB runtime stdout, 63 KiB per trace record, a 64 MiB trace file checked before reading, 1 KiB runner lines. Oversized, non-UTF-8 and cut-off lines are explicit failures, never truncated. The trace digest covers the whole file. The runtime runs in its own process group: a deadline, relay error or closed runner transport kills the whole group, and stragglers after a normal exit are killed and reported. The Mac backend kills its transport, recorder and players on drop. |
| `9dd4ce61` | `docs/agent2-interfaces.md` | Interface proposal: frame link, perception worker, Gemini visual context and process-simulation seams. |
| `7e165869` | `crates/camera/**`, `Cargo.lock` (serde for lamp-camera) | `lamp_camera::frames`: bounded pull-based frame link with consumer identity, stall closure and privacy closure (details in [perception runtime](perception-runtime.md)). |
| `197fa0b9` | Camera-specific live files: `camera_worker.rs`, `camera_worker/linux.rs`, `camera_inspect.rs`, `tests/camera_runtime.rs`. Shared: the `worker camera` arm in `crates/live/src/main.rs` | The camera worker serves frames only to waiting permitted consumers (`--frame-consumer ROLE=BOOT`). Without the flag it behaves as before. |
| `9588dab9` | `assets/models/**` | Vendored YuNet 2023mar (MIT), with license and provenance. |
| `5663f75a` | `crates/perception/**`. Shared: workspace member line in `Cargo.toml`, `Cargo.lock` | `lamp-perception`: verified model, strict JPEG decoding (with Annex K tables for MJPEG), exposure, YuNet via `tract-onnx`, V1's yaw/attention, the generic worker loop, and the `lamp-perception` binary (`worker`, `models verify/install`, `analyze`). |
| `9bfd5824` | `crates/perception/tests/**` | Tests install the model into a private directory first. The Linux export (umask 002) showed that reading the checkout copy depends on checkout permissions. |

## Checks run on `9bfd5824`

| Platform | Command (from `lampOS/`) | Result |
|---|---|---|
| Source-only export (`git archive` outside the repository), macOS host | `cargo fmt --all -- --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`; `cargo test --locked --workspace --no-fail-fast` | Clean; clean; **898 passed, 0 failed, 3 ignored** |
| macOS host, in tree, on `5663f75a` (code identical apart from test helpers) | Same three commands | Clean; clean; 898 passed, 0 failed, 3 ignored |
| Linux ARM64 container (Debian bookworm, Rust 1.96.0, Colima VM on Apple silicon; not emulated, not Lamp) | `cargo fmt --all -- --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`; `cargo test --locked --workspace --no-fail-fast` | Clean; clean; **937 passed, 0 failed, 3 ignored** (more than macOS because Linux-only audio and camera tests compile) |
| Linux ARM64 container, on `5663f75a` | `cargo build --locked --release -p lamp-perception -p lamp-live -p lamp-voice-eval` | Built in 1 m 17 s |

Release-build perception timing, decode plus inference of one 1280×720 drawn
frame (n=20): Mac p50 9.94 ms, p95 10.22 ms; Linux ARM64 container p50 9.49 ms, p95 9.92 ms (decode 1.22 ms, inference 8.30 ms at p50). These are not Lamp
measurements.

Regressions the new tests caught during development:

- `zune-jpeg` silently decoded a frame truncated to a third of its length;
  strict mode and an end-marker check now refuse it.
- `zune-jpeg` rejected MJPEG without Huffman tables, as many UVC cameras send
  it. Standard tables are inserted, and a test requires identical pixels.
- The old relay accepted a 20,000-byte stdout line by truncating it. It now
  emits `stdout_invalid` and fails the attempt.
- The first Linux run failed all eight model-loading analysis tests. A
  `git archive` export has group-writable files, which model verification
  correctly refuses. The tests now provision the model, as a device would.

## Not tested

- No physical camera, real faces, real lighting or Lamp CPU load.
- The yaw cone has not been validated on people: drawn faces cannot exercise
  the landmark model's yaw, so only the arithmetic is tested.
- The Linux V4L2 camera worker with `--frame-consumer` has been compiled and
  unit-tested, but never run against a device.
- The perception worker under the real coordinator: there is no coordinator
  integration yet.
- Plan item 3 (the actual coordinator graph with synthetic hardware) is not
  implemented. It needs the seams in [the proposal](agent2-interfaces.md#4-synthetic-hardware-ports-for-process-level-tests).
  The perception process tests use the real worker binary, real runtime
  channels and a real frame server, but a test supervisor in place of the
  coordinator.
- The relay hardening has not run over a real SSH transport.

## Integration requirements (main agent unless noted)

1. **Shared edits made on this branch; accept or rewrite them.**
   - The `worker camera` arm in `crates/live/src/main.rs` now passes
     `--frame-consumer` flags.
   - The `crates/perception` workspace member.
   - `Cargo.lock`: 66 third-party packages from `tract-onnx` and `zune-jpeg`,
     plus the dev-only `jpeg-encoder`.
2. **`transport.rs`** (not edited here): add `"perception"` to the
   `WorkerChannels::bind` role list. Until then the binary binds
   `perception.{c,d}.w` with `Channel` directly, using the same names and
   envelopes, so `Worker::spawn` interoperates once the role is allowed.
3. **Coordinator wiring.**
   - Spawn `lamp-perception` (a separate executable path) with
     `worker perception DIR PARENT WORKER --model-dir ABS_DIR [--interval-ms N]`.
   - Spawn the camera worker *after* its consumers, with
     `--frame-consumer perception=BOOT` (and later `context=BOOT`).
   - Send `Authority` heartbeats to perception only after `Ready`.
   - Decode `lamp_perception::observation::PerceptionEvent` from its data
     channel.
   - Track evidence with `ObservationTracker::new(perception_boot)`.
   - A restarted consumer needs a new camera worker incarnation: the frame link
     accepts each consumer identity once.
4. **Policy (main agent).** Use presence and attention as evidence only.
   - Missing or stale observations stay missing.
   - `no_face_detected` is not an empty desk.
   - Facing away must not reject clearly addressed speech.
5. **Gemini visual context (Agent 1 for Gemini input; main agent for the
   control).**
   - Proposed control: `Control::VisualContext { owner, max_age_us }`.
   - The provider uses `FrameClient` with `Role::Context`. It rechecks the grant
     against its own `current_camera_grant(now)` and sends original JPEG bytes
     under the turn owner.
6. **Process-simulation seams (main agent and Agent 1).**
   - `HardwareProfile` (worker executable, legacy-owner check).
   - `provider_worker::run_with_connector`.
   - `physical_privacy::run_with`.
   - Agent 1's `PcmPort` audio device trait.
7. **Deployment.**
   - Build and install the `lamp-perception` binary.
   - Provision the model with `lamp-perception models install` into a root-owned
     directory that is not group- or world-writable, then verify it with
     `models verify`.

## Remaining defects and open items

| # | Item | Owner | Status |
|---|---|---|---|
| A2-1 | Actual coordinator/worker graph tests (stalls, disconnects, mute/reopen, worker death/restart, blocked output, stale camera, motion inhibition, cancellation while audio continues) | Agent 2, after seams from main agent/Agent 1 | Not implemented; seams proposed |
| A2-2 | A JPEG cut inside its scan but ending in an end marker decodes partly (missing area grey) | Agent 2 | Known limit; frames without the marker are refused, and driver-flagged corrupt buffers are terminal upstream |
| A2-3 | Exposure, face-size and yaw thresholds are unqualified | Agent 2 with owner's recorded desk frames | Open |
| A2-4 | `FrameClient::connect` uses a blocking local `connect`; it returns immediately unless the listener backlog is full | Agent 2 | Accepted risk; at most two consumers |
| A2-5 | Perception CPU on Lamp alongside audio is unmeasured (`nice 10` applied) | Agent 2 at physical qualification | Open |
| A2-6 | Gemini context consumer not implemented | Agent 1 / main agent | Proposed |
| D1–D9 | Voice defects from the evaluator | Codex / Agent 1 | Unchanged; see [voice-eval defects](voice-eval-defects.md) |

## Exact commands

```text
cargo test --locked -p lamp-camera --test frames
cargo test --locked -p lamp-live --test camera_runtime frame_link
cargo test --locked -p lamp-perception
cargo test --locked --release -p lamp-perception --test analysis measured -- --nocapture
cargo test --locked -p lamp-voice-eval --test relay_hardening
mkdir -m 700 /tmp/lamp-models && target/debug/lamp-perception models install "$PWD/assets/models/face_detection_yunet_2023mar.onnx" /tmp/lamp-models
target/debug/lamp-perception models verify /tmp/lamp-models
```
