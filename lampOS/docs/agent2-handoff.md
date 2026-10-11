# Agent 2 handoff: camera frames, perception, relay hardening

Workstream branch: `lamp-v2-agent2` (Agent 2). The main agent owns
integration into `lamp-v2-chat`; this branch never pushes there.

## Checkpoint 2: review corrections, context frames, provider failure

- **Head:** `8a1cc9b3`, a merge of `lamp-v2-chat` `51bd815e9` into this branch.
  Agent 2's code is unchanged by the merge.
- **Base:** checkpoint 1 `0239ac27`.
- Nothing ran on Lamp; no device was used.

| Commit | Scope | Behavior and evidence |
|---|---|---|
| `6da0916f` | `crates/camera/src/frames.rs` | `FrameServer::offer_meta` offers derived images that keep the original identity and grant (`FrameMeta.derived`). The request age bound is now up to 2 s (A1-006). |
| `43310241` | `camera_worker.rs`, `camera_worker/linux.rs` | One `serve` driver for the Linux port and simulated ports. |
| `5130aa21` | `crates/perception/**`, docs | **A1-006 context frames.** Perception serves the provider (`perception.frames`, role `context`) the newest analysed frame. It is reduced to at most 640 pixels wide, encoded to at most 64 KiB, and no older than 2 s; it is dropped on any grant change. Process tests cover the bound, size, lineage and privacy. |
| `5220ce20` | `crates/voice-eval/src/physical/**`, `tests/relay_hardening.rs` | **MAIN-006 and MAIN-005 item 4.** The relay kills the runtime's process group even after the leader exited. Every relay stream counts raw bytes before framing, so an endless unterminated line ends at its bound. Both regressions failed before the fix: a worker survived, and the reader consumed for 60 s without stopping. |
| `343f9a41` | `crates/perception/src/worker.rs`, `tests/control_priority.rs` | **MAIN-005 item 1.** Drains distinguish exhausted from empty. Submission, publication and context offers each happen only after a drain reaches empty, and interrupted work is held and rechecked. Regressions bury a Stop or a privacy close behind six control slices of duplicates. Before the fix both published an observation while the message was unread. |
| `206813b5` | `crates/camera/**`, `crates/perception/src/worker.rs`, docs | **MAIN-005 item 3.** The frame client uses a nonblocking `rustix` socket; the hello is completed within 100 ms or abandoned. On Linux the old blocking client hung for more than 60 s on a full listen queue and was killed by `timeout` (exit 124); the new one returns at once. **MAIN-005 item 5.** The freshness contract is stated exactly: the newest frame held at copy time, no older than `max_age_us` from host dequeue. It may predate the request. A test pins the boundary at 50,000 versus 50,001 µs. |
| `7912fa07` | `crates/live/src/transport.rs` | `perception` worker role (approved in MAIN-005). |
| `29c17173` | `crates/perception/**`, `Cargo.lock` | **MAIN-005 item 2, part 1.** The perception library no longer depends on `lamp-live`. Test support moves behind the `testing` feature. The binary keeps only `models verify/install` and `analyze`. |
| `0253e5a5` | Narrow registration: `crates/live/src/lib.rs` (module), `main.rs` (role arm), `Cargo.toml` (dependency), `Cargo.lock`, plus new `crates/live/src/perception_worker.rs` and `tests/perception_worker.rs` | **Part 2.** `lamp-live worker perception ...` hosts the role over ordinary `WorkerChannels`. The tests include the production `Worker::spawn` and `shutdown` path. |
| `8bfff000` | `crates/voice-eval/**`, `docs/voice-eval.md` | **MAIN-009, provisional names.** `provider_failed` is a readable terminal failure: `ProviderFailed { partial_audio }`. It is never in the success numerator and never a silent missing terminal. Played audio is kept, and every started occurrence must be retired first. `input_discarded_endpoint` is recorded but is never an endpoint. A failed terminal is never a trigger; the waiting step becomes `provider_unavailable`. |
| `8a1cc9b3` | merge | `lamp-v2-chat` `51bd815e9` (Gemini recovery and coordination files). The only conflict was main's placeholder `coordination/agent-2.md`, resolved to Agent 2's file. |

### Checks on `8a1cc9b3`

| Platform | Command (from `lampOS/`) | Result |
|---|---|---|
| macOS host | `cargo fmt --all -- --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`; `cargo test --locked --workspace --no-fail-fast` | Clean; clean; **921 passed, 0 failed, 4 ignored** |
| Linux ARM64 container (Debian bookworm, Rust 1.96.0, Colima on Apple silicon; not Lamp) | Same commands, plus `cargo build --locked --release -p lamp-live -p lamp-perception -p lamp-voice-eval` | Clean; clean; **960 passed, 0 failed, 4 ignored**; release build OK. Release perception figures: model verify plus plan 28.9 ms, frame p50 8.8 ms, p95 9.0 ms, spawn to `Ready` 31 ms |
| Linux ARM64 container, on `0253e5a5` (before MAIN-009 and the merge) | Same commands | Clean; clean; 948 passed, 0 failed, 3 ignored |
| macOS host | `lamp-voice-eval self-test` | 21 of 21 canaries behave as expected |

Measured, not Lamp, figures:
- **Perception worker `Worker::spawn` to `Ready`, release:** 32 ms in the Linux
  container. On macOS it is 600–840 ms, because `nice 10` moves the process to
  efficiency cores there.
- **Model verify plus plan build:** 59 ms on the Mac.
- **Lamp:** unmeasured. The supervisor's startup bound is 3 s.

### Still open

- **A2-1:** coordinator process-graph tests. Next: `run_directed_session` with
  scripted Gemini, per MAIN-005 C. This waits on the proposed test-support
  seams; production defaults stay unchanged.
- **A2-2:** a JPEG cut short but ending in an end marker decodes partly.
- **A2-3:** exposure, face-size and yaw thresholds are unqualified.
- **A2-5:** CPU use on Lamp is unmeasured.
- **MAIN-009 names:** align with main's frozen producer schema when it is
  published.

The original checkpoint 1 report follows unchanged.

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
