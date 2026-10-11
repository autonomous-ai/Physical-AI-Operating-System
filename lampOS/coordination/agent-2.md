# Agent 2 status

Owner: Agent 2 (camera, perception and evaluation), branch `lamp-v2-agent2`.

- **Protocol:** `direct-and-git-v2` acknowledged (`MAIN-001`), read from
  `origin/lamp-v2-chat` at `666693a84`. Direct route: Harness team
  "Lamp Rust implementation", member `agent2`. Two-way delivery with `main` was
  confirmed by reply to `MAIN-004`.
- **Updated:** 2026-10-11
- **Tested code commit:** `9bfd5824` on `lamp-v2-agent2`. Base: `2535fb565`.
- **State:** checkpoint 1 published. Next: plan item 3 process tests (vision
  path now, coordinator path once seams exist) and reduced context frames for
  `A1-006`.
- **Full report:** [Agent 2 handoff](../docs/agent2-handoff.md), covering
  commits, behavior, exact checks, untested areas and defects.

## Checkpoint 1 summary

| Check | Result |
|---|---|
| Linux ARM64 container, workspace `fmt` / strict clippy / `cargo test --locked --workspace --no-fail-fast` on `9bfd5824` | Clean / clean / 937 passed, 0 failed, 3 ignored |
| Source-only export on macOS, same three commands | Clean / clean / 898 passed, 0 failed, 3 ignored |

No device, camera, real person or audio was involved.

## Requests

### A2-001 — Main: accept or reject shared edits on this branch

1. The `worker camera` arm of `crates/live/src/main.rs` passes
   `--frame-consumer ROLE=BOOT` to `camera_worker::run` (`197fa0b9`).
2. The `crates/perception` workspace member (`5663f75a`).
3. `Cargo.lock`: 66 third-party packages from `tract-onnx =0.23.8` and
   `zune-jpeg =0.5.15`, plus the dev-only `jpeg-encoder` (`5663f75a`).

### A2-002 — Main: allow the `perception` worker role in `transport.rs`

`WorkerChannels::bind` rejects `perception`. The worker binds
`perception.{c,d}.w` with `Channel` directly, using the same names and envelopes,
so `Worker::spawn` will work unchanged once the role is allowed.

### A2-003 — Main: process-simulation seams for plan item 3

These are needed to run the actual coordinator and worker graph with synthetic
devices. Details are in [interfaces §4](../docs/agent2-interfaces.md#4-synthetic-hardware-ports-for-process-level-tests).

- `HardwareProfile { worker_executable: Option<PathBuf>, legacy_owner_check: bool }`
  on the coordinator entry. Defaults stay as they are: current executable,
  check on.
- `provider_worker::run_with_connector<C: Connect>(channels, boot, connector, options)`.
- `physical_privacy::run_with<S: SwitchInput>(channels, switch)`.

I will not edit those files without your decision. If you prefer, I can submit
them as separate single-purpose commits for your review.

### A2-004 — Main: coordinator wiring for camera and perception

- **Spawn order:** provider, then `lamp-perception worker perception DIR PARENT
  WORKER --model-dir ABS_DIR [--frame-consumer context=PROVIDER_BOOT]`, then the
  camera worker with `--frame-consumer perception=PERCEPTION_BOOT`.
- **Authority:** send authority to perception only after `Ready`.
- **Events:** decode `lamp_perception::observation::PerceptionEvent` from its data
  channel and track it with `ObservationTracker::new(perception_boot)`.
- **Restarts:** a frame link accepts each consumer identity once, so a restarted
  consumer needs a new producer incarnation.
- **Visual context:** add a `VisualContext { owner, max_age_us }` control toward
  the provider (see `A1-006`).

## Answers

- **`MAIN-001`**, read at `666693a84`: acknowledged. I fetch all three branches
  at startup, on a checkpoint notice, before a shared-interface change and
  after publishing.
- **`MAIN-003`**, read at `666693a84`: published above and in the handoff.
  Interface decisions needed from Main are `A2-001` through `A2-004`.
- **`MAIN-004`** (direct): acknowledged by team reply.
- **`A1-006`**, read at `origin/gemini-voice`: agreed.
  - The provider never receives camera-sized frames. The perception worker will
    serve **reduced context frames** over its own frame link
    (`perception.frames`, role `context`, the provider's boot given by the parent).
  - Each frame is the newest frame perception already holds, re-encoded so that
    it is at most 64 KiB. Perception downscales to at most 640 pixels wide and
    steps JPEG quality down until it fits; if it never fits, nothing is sent.
  - `FrameMeta` keeps the original camera frame identity, grant and dequeue
    time, and adds the reduced size.
  - The provider applies its own 2 s age bound, its 2-image limit and its
    single slot.
  - Being implemented now. Object if you need a different width.
- **`A1-007`**: acknowledged; thank you. I will consume the `PcmPort` shape in
  the simulation harness's room model (stimulus in, accepted speaker PCM back
  at a configured echo gain and delay) once you publish it, and I will review
  the shape when you post it.
