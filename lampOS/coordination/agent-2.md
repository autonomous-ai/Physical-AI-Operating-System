# Agent 2 status

Owner: Agent 2 (camera, perception and evaluation), branch `lamp-v2-agent2`.

- **Protocol:** `direct-and-git-v2` acknowledged (`MAIN-001`), read from
  `origin/lamp-v2-chat` at `666693a84`, re-read at `51bd815e9` (MAIN-010).
  Direct route: Harness team "Lamp Rust implementation", member `agent2`;
  two-way delivery with `main` is confirmed.
- **Updated:** 2026-10-11
- **Tested head:** `d5b848b3` on `lamp-v2-agent2`. Gate: Linux ARM64
  container workspace `fmt` and strict clippy clean, 972 passed, 0 failed,
  4 ignored. The previous checkpoint `8a1cc9b3` merged `lamp-v2-chat`
  `51bd815e9`.
- **State:**
  - MAIN-014 and MAIN-015 fixes are published: runner P2, MAIN-013 evaluator
    alignment and context-frame privacy P1.
  - Priority is the first audio-only Lamp run: the directed runner and the
    acoustic report.
  - Process simulation on kernel loopback hardware is a local draft
    (`agent2-sim-wip`), not published.
- **Full report:** [Agent 2 handoff](../docs/agent2-handoff.md), checkpoint 2
  section, covering commits, red/green evidence, exact checks and open items.

## Checkpoint 2 summary

| Check on `8a1cc9b3` | Result |
|---|---|
| macOS: `fmt` / strict workspace clippy / `cargo test --locked --workspace --no-fail-fast` | Clean / clean / 921 passed, 0 failed, 4 ignored |
| Linux ARM64 container: same | Clean / clean / 960 passed, 0 failed, 4 ignored |
| Linux ARM64 container on `0253e5a5` (corrections only, before the merge) | Clean / clean / 948 passed, 0 failed, 3 ignored |

No device, camera, real person or audio was involved.

## Requests

### A2-001 — Main: accept or reject shared edits on this branch

1. The `worker camera` arm of `crates/live/src/main.rs` passes
   `--frame-consumer ROLE=BOOT` to `camera_worker::run` (`197fa0b9`).
2. The `crates/perception` workspace member (`5663f75a`).
3. `Cargo.lock`: 66 third-party packages from `tract-onnx =0.23.8` and
   `zune-jpeg =0.5.15`, plus the dev-only `jpeg-encoder` (`5663f75a`).

### A2-002 — Main: allow the `perception` worker role in `transport.rs` (done here as approved: `7912fa07`)

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

- **`MAIN-005`**: all five findings fixed, each with a regression shown
  failing first.
  - (1) Exhausted perception drains: `343f9a41`.
  - (2) Dependency cycle: `29c17173` and `0253e5a5`, with the shared
    registration kept narrow.
  - (3) Blocking connect: `206813b5`.
  - (4) Raw-byte relay bound: `5220ce20`.
  - (5) Freshness contract: `206813b5`.
  - B: the seams remain proposals, behind an explicit test-support feature.
  - C: the next target is `run_directed_session` with scripted Gemini.
- **`MAIN-006`**: the relay group kill after an early return is fixed in
  `5220ce20`. On the old code the regression left a worker running.
- **`MAIN-009`**: accepted and implemented provisionally in `8bfff000`. Final
  field names wait for the frozen producer schema. Please include the exact
  names for `provider_failed` and `input_discarded_endpoint`. (My direct reply
  expired before delivery; this is the durable answer.)
- **`MAIN-014`**: acknowledged. Camera, perception and image work stays
  inactive and does not gate the first audio-only run. The narrow runner fix is
  `b010aa0b`.
- **`MAIN-015`** (review of `bb7e61ae`):
  - **P2:** `4ea4fb49`. A provider failure latches for the attempt. It is
    rechecked while waiting for a trigger, during the delay and before
    dispatch. `PhysicalBackend` also checks before the player starts. Your
    exact repro is a test.
  - **MAIN-013 alignment:** `9b82031c`. `provider_session` and
    `provider_outcome` are kept. `provider_failed` is scored before Late,
    and lateness stays a separate finding. Raw failure and drain records are
    kept as evidence.
  - **P1:** `ddc771a9` and `d5b848b3`. A context tail whose grant changed,
    closed or lapsed is abandoned before any continuation write. The
    reconnect policy is defined, the send buffers are small, and there is
    `ContextServed` provenance. Denial, newer-grant, lapse and queued-Stop
    regressions cover it.
  - **MAIN-013 itself was never received.** Please resend it or commit it to
    `main.md`.
- **`MAIN-010`**: acknowledged by direct reply. `51bd815e9` is merged in
  `8a1cc9b3`.

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
