# Agent 2 interface proposal: frames, perception and process simulation

Status: **proposal for review** by the main agent (coordinator, wire, CLI,
workspace) and Agent 1 (audio, Gemini). Sections 1 and 2 are now implemented
on `lamp-v2-agent2`; see [perception runtime](perception-runtime.md) and the
[handoff](agent2-handoff.md). Sections 3 and 4 remain proposals. Agent 2 implements only the parts marked
"Agent 2" on branch `lamp-v2-agent2`. Shared files are not edited there except
where a separate, single-purpose commit says so. Nothing here is a physical,
optical or acoustic result.

## 1. Camera frame link (Agent 2, `lamp_camera::frames`)

JPEG frames do not fit the 8 KiB control/data datagrams. They use a separate
private stream socket that carries no control traffic. Capture and its privacy
control never wait for it.

- **Endpoint.** The camera worker binds `SESSION_DIR/camera.frames` as a mode-0600
  Unix stream listener inside the existing 0700 session directory.
- **Allowed consumers.** The parent supplies each permitted consumer on the camera
  worker's command line: `--frame-consumer perception=BOOT` and/or
  `--frame-consumer context=BOOT`. The parent spawns consumers before the camera
  worker, so their boots are already known. A consumer boot is never learned
  from the socket.
- **Hello.** A consumer connects and sends a fixed 40-byte hello: magic `LFH1`,
  role, its own boot and the controller boot. The camera worker accepts at most one
  connection per role, and only with the expected boot and role. Anything else is
  closed and counted. A restarted consumer has a new boot and needs a new
  camera-worker incarnation.
- **Pull with credits.** A consumer sends `Request { id, max_age_us }`. Each
  consumer has at most one outstanding request. When a request is pending, the
  worker copies the newest current frame into that consumer's own buffer: one
  frame of at most 2 MiB plus header, allocated at construction. It then releases
  the library slot immediately and writes nonblockingly across loop iterations.
  Frames that arrive meanwhile replace the library's pending slot, so there is no
  queue.
- **Frame message.** The header is magic `LFF1`, request id, metadata length
  (at most 2048) and JPEG length (at most 2 MiB). It is followed by JSON
  `FrameMeta` and the original MJPEG bytes. `FrameMeta` holds the camera worker
  boot, capture epoch, frame sequence, the original `CameraGrant`, mode, driver
  sequence and timestamp (domain and point kept), host dequeue start and
  completion times, and the send time.
- **Failure behavior.**
  - Privacy close, stop or fault discards any partly sent frame and closes every
    consumer connection. A cut-off frame is never completed.
  - A frame unsent for 500 ms closes that consumer (`consumer_stalled`).
  - Malformed requests, a second outstanding request or oversized input close
    that consumer only. Capture continues.
  - Consumers must recheck the grant in `FrameMeta` against their own current
    authority before use. Bytes already sent cannot be retracted.
- **No consumers.** Without `--frame-consumer`, the camera worker behaves exactly
  as the current metadata-only worker (`camera-inspect` is unchanged).

## 2. Perception worker (Agent 2: `crates/perception`; registration: main agent)

- **Process.** A separate process with role `perception`:
  `worker perception DIR PARENT WORKER --model-dir PATH`. Its loop is generic in
  `lamp-perception`; `crates/live/src/perception_worker.rs` adapts it to
  `WorkerChannels`.
- **Control it accepts.** `Authority` and `Stop`; any other control is a fault.
  Without authority for 250 ms, it exits like the other workers.
- **`Ready` meaning.** The model is verified and loaded and the loop is running.
  `Ready` does not mean a camera frame or sight exists.
- **Work it does.** It decodes JPEG (`zune-jpeg`) and measures exposure. It then
  runs YuNet face detection, with five landmarks, on a 320-pixel-wide input
  through `tract-onnx`, a pure-Rust ONNX runtime with no native dependency.
- **Attention.** Head yaw comes from the landmarks using V1's measure and its
  25° cone. This is attention *toward the camera*, not proof of addressing.
  Looking at the monitor must not become a rejection rule.
- **Output.** It sends `PerceptionEvent`s on its data channel. Each fits one
  datagram.
  - `Observation`:
    - the `lamp_interaction::Observation` provenance: source boot = perception
      worker, a sequence, `acquired_at` = host dequeue completion (not exposure
      time), and confidence;
    - the original frame identity and `CameraGrant`;
    - exposure verdict (`usable`, `too_dark`, `overexposed`);
    - presence (`faces`, `no_face_detected`, `unknown{reason}`);
    - at most four faces with normalized box, landmarks, score and yaw;
    - attention (`toward`, `away`, `unknown{reason}`);
    - timings and the model id/hash prefix.
  - `Discarded{frame, reason}`, where the reason is stale, privacy changed,
    decode failed or inference failed.
  - `CameraLink{connected}`.
  - `Progress`, once per second, with counters.
- **Unknown stays unknown.** Darkness or overexposure gives `unknown`, never
  "nobody". Missing frames produce no observation, and the coordinator's
  `ObservationTracker` reports them as `Missing`/`Stale`. Perception failure
  cannot affect capture, admission or playback, because it is a separate process
  with no audio dependency.
- **Model asset.** `face_detection_yunet_2023mar.onnx` (232,589 bytes, SHA-256
  `8f2383e4…52fa4`, MIT) is byte-identical to OpenCV Zoo and to V1's checked-in
  copy. It is vendored under `assets/models/` with its license. A provisioning
  binary (`lamp-models verify|install`) checks size, hash, file type and
  permissions. The worker refuses any other bytes.

## 3. Gemini visual context (Agent 1 implements Gemini input; main agent decides policy)

**Update after `A1-006`:** the provider does not receive original camera
frames. It consumes reduced frames (at most 64 KiB, at most 640 pixels wide,
at most 2 s old) served by the perception worker at `perception.frames`, as
implemented and described in [perception runtime](perception-runtime.md#visual-context-frames-for-the-provider-a1-006).
The text below is the original proposal.

- **Consumer.** The provider worker is the `context` frame consumer: it uses
  `lamp_camera::frames::Consumer` and receives original JPEG bytes plus `FrameMeta`.
- **Proposed control (main agent's wire).** Add
  `Control::VisualContext { owner: TurnOwner, max_age_us: u32 }`, sent when the
  interaction owner wants Gemini to see the current scene.
- **What the provider does on that control.**
  1. Request one frame.
  2. Reject it if `CameraGrant` differs from its own `current_camera_grant(now)`
     or if the frame is older than `max_age_us`.
  3. Send it as realtime image input under that owner.
  4. Trace the frame identity.
- **Isolation.** Frame transfer never blocks audio input, cancellation or output.
  The consumer is nonblocking and has one bounded buffer.

## 4. Synthetic hardware ports for process-level tests

Goal: run the **actual** coordinator and worker processes with only the device
layer replaced. The coordinator already spawns workers as
`current_exe() worker ROLE ...`. A test harness binary that calls the real
`coordinator::run_*` therefore becomes the worker executable. It can then run
the real worker loops over synthetic devices. Proposed seams:

| Owner | Seam | Minimal change |
|---|---|---|
| Main agent | Coordinator options | `HardwareProfile { worker_executable: Option<PathBuf>, legacy_owner_check: bool }` (default: current exe, check on). Device strings are already plain arguments. |
| Main agent | Provider worker | `pub async fn run_with_connector<C: Connect>(channels, boot, connector, options)` beside `run`; the harness passes `lamp_gemini::testing::FakeConnector` driven by a scenario script. |
| Main agent | Privacy worker | `physical_privacy::run_with<S: SwitchInput>(channels, switch)`; the harness supplies a scripted switch. |
| Agent 1 | Audio devices | A `PcmPort` trait covering the ALSA calls `Microphone`/`Speaker` make (open/prepare/start/readi/writei/avail/delay/status/drop). Keep ownership, guards and timing logic unchanged above it. |
| Agent 2 | Camera | `CameraLoop<P: InspectPort>` is already generic; the harness supplies a synthetic MJPEG port. |
| Agent 2 | Harness | `lamp-sim` (in `crates/voice-eval`):<ul><li>a room model (cached stimulus PCM into capture; accepted speaker PCM back into capture at a configured echo gain and delay);</li><li>a scripted Gemini service;</li><li>privacy and camera scripts;</li><li>worker-kill and stall injection;</li><li>evaluation through the existing scorer under a new `process_simulation` stratum, never pooled with `fake_policy` canaries.</li></ul> |

Until those seams exist, Agent 2's process tests run the real camera worker
loop, the real perception worker process and real `lamp-live` supervision code
(`process::Worker`, authority snapshots from `lamp_interaction::Controller`). The
coordinator-level cases are written against the seams above and stay disabled
until the seams merge. They are never replaced by a reimplementation of the
audio workers.

## Timing targets (to measure, not claims)

| Boundary | Target |
|---|---|
| Camera dequeue to frame fully written to a consumer socket | p95 ≤ 10 ms on Lamp |
| Perception: frame receipt to published observation (320×180, one thread) | p95 ≤ 150 ms on Lamp; ~8 ms measured on the development Mac for inference alone |
| Maximum observation age the coordinator should accept for presence | 500 ms (policy is the main agent's) |
| Privacy close to camera frame link closed | ≤ 20 ms after the camera worker drains the revocation |
