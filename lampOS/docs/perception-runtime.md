# Camera frame link and perception worker

Status: implemented and host/Linux-container tested. **Not run on Lamp.**
- Nothing here has opened a camera or seen a real person.
- Nothing is wired into the coordinator yet (see integration requirements in
  the [Agent 2 handoff](agent2-handoff.md)).
- Face detection is verified only on drawn scenes, never on photographs.

## What it does

```text
camera worker ──(camera.frames stream, pull + credits)──► perception worker ──(data datagrams)──► coordinator
   (lamp-live worker camera --frame-consumer perception=BOOT)      (lamp-perception worker perception ...)
```

- **Frame link (`lamp_camera::frames`).** A private Unix stream socket
  `SESSION_DIR/camera.frames`, used only for JPEG bytes. Each permitted consumer
  sends a request (at most one outstanding). It receives the newest frame
  captured under the current authority, with `FrameMeta`:
  - original camera worker boot, capture epoch and frame sequence;
  - the original `CameraGrant`;
  - mode, driver sequence and timestamp (domain and point kept);
  - host dequeue start and end times;
  - copy time.

  Bounds and failure behavior:
  - A frame is written nonblockingly from one buffer per consumer, allocated
    once. At most 256 KiB is written per loop step.
  - A frame unsent for 500 ms closes that consumer.
  - Privacy closure, stop and faults abandon partly sent frames and close every
    consumer, so a cut-off frame is never completed.
  - Unknown, reused or wrong-controller consumer identities are refused.
  - Malformed requests close only the offending consumer.
- **Camera worker.** Unchanged without `--frame-consumer`; metadata-only
  `camera-inspect` behaves as before. With consumers:
  - a frame leaves the library slot only when a consumer is waiting, and is
    copied into that consumer's buffer before the slot is released;
  - `CameraEvent::Frame` metadata is still published for each delivered frame;
  - link changes are published as `CameraEvent::FrameLink`.
- **Perception worker (`crates/perception`, binary `lamp-perception`).** It
  verifies and loads the model before `Ready`. It requests a frame at most every
  `--interval-ms` (default 250) and only while it holds a current camera grant,
  then processes it:
  1. **Recheck on arrival.** A different or closed grant gives
     `Discarded{privacy_changed}`; more than 200 ms of host dequeue age gives
     `Discarded{stale}`.
  2. **Decode** strictly with `zune-jpeg`. The end-of-image marker is required,
     and the standard Annex K Huffman tables are inserted when the camera omits
     them.
  3. **Exposure.** Measure brightness. Dark or washed-out frames report
     presence and attention as `unknown`, and inference is skipped.
  4. **Detect.** YuNet runs through `tract-onnx` on a 320-pixel-wide input
     (320×192 for 720p), with score ≥ 0.5 and NMS 0.3.
  5. **Attention.** For the largest face, yaw is computed from the landmarks
     with V1's 25° cone.
  6. **Recheck before publishing.** Privacy is checked again, and the frame
     must be no more than 1 s old.

  Inference runs on its own thread with one job slot, so control and privacy
  are drained every 2 ms. The process lowers its priority (`nice 10`).
- **Output.** `PerceptionEvent`s, each well under the 8 KiB datagram bound:
  - `Observation` carries `lamp_interaction::Observation` provenance (perception
    worker boot, sequence, `acquired_at` = host dequeue time, confidence = best
    face score or unknown), the original frame identity and grant, exposure,
    presence (`faces`, `no_face_detected`, `unknown`), up to four faces with
    normalized bounds, landmarks and yaw, attention (`toward`, `away`,
    `unknown`), timings and the model identity.
  - `Discarded`, `CameraLink` and a 1 s `Progress`.

## Meaning and limits of the evidence

- `no_face_detected` describes one frame and one model, not an empty desk.
- `toward` means facing the camera within 25°. It is not addressee evidence,
  and `away` (for example facing the monitor) must not reject clearly
  addressed speech.
- A face smaller than 24 input pixels gives `unknown{face_too_small}` for
  attention.
- No identity, recognition or memory is computed.
- Host dequeue time bounds local age; it is not exposure time.
- Darkness and overexposure use unqualified thresholds. Tune them only from
  recorded desk frames.
- **Partial frames.** A JPEG cut inside its scan but still ending in an
  end-of-image marker decodes partly, with the missing area grey. Frames
  without the marker are refused. Driver-flagged corrupt buffers are already
  terminal in the camera backend.

## Model asset

`assets/models/face_detection_yunet_2023mar.onnx`:
- 232,589 bytes, SHA-256 `8f2383e4…52fa4`, MIT;
- identical to OpenCV Zoo and to V1's checked-in copy (see
  [assets README](../assets/models/README.md)).

The worker accepts only those exact bytes, and only if the file is:
- a regular file, not a symlink;
- singly linked;
- owned by root or the worker user;
- not group- or world-writable;
- in a directory that is also not writable by others.

Install and verify:

```text
lamp-perception models install /path/to/lampOS/assets/models/face_detection_yunet_2023mar.onnx /var/lib/lampOS/models
lamp-perception models verify /var/lib/lampOS/models
lamp-perception analyze --model-dir /var/lib/lampOS/models FRAME.jpg   # one frame, no lineage
```

Verification is deliberately strict about the *installed* copy. A source
checkout made with umask 002 (for example `git archive`, which exports 0664
files) is refused as group-writable. Install from the checkout instead of
pointing the worker at it; the tests do the same.

V1's SCRFD and EdgeFace identity models are not reused. Identity is out of
scope for this release, and InsightFace's pretrained weights are for
non-commercial research.

## Dependencies

- **Direct dependencies:** `tract-onnx =0.23.8` (pure-Rust ONNX runtime; tract
  compiles its own assembly kernels with the system C toolchain; no native
  inference library) and `zune-jpeg =0.5.15` (pure Rust).
- **Lockfile impact:** they add 66 third-party packages. Notable ones include
  `ndarray`, `prost`, `rustfft`, `tar`, `minijinja` and `zerocopy`, all through
  `tract`. `jpeg-encoder =0.7.1` is a dev-dependency for synthetic test frames
  only. All Lamp-owned code keeps `unsafe_code = deny`.

## Tests

| Test | What it covers |
|---|---|
| `crates/camera/tests/frames.rs` | Frame link over real sockets: whole frame with original lineage, identity rejection and reuse, age bound, protocol violations, a stalled consumer (2 MiB unread, no blocking), privacy close mid-frame, client rejection of unrequested frames, private-directory binding |
| `crates/live/tests/camera_runtime.rs` `frame_link::*` | The real `CameraLoop` with a synthetic port: frames leave the library only for a waiting consumer; privacy revoke closes the link before stop; a worker fault closes consumers; strict flag parsing |
| `crates/perception/src/*` unit tests | OpenCV decode formulas, clamping and NaN handling, NMS, the V1 yaw port (symmetry, in-plane rotation, degenerate input), input sizing and padding, exposure verdicts |
| `crates/perception/tests/analysis.rs` | The real model:<ul><li>a drawn face is found where drawn, with ordered landmarks;</li><li>dark and glare frames give unknown and skip inference;</li><li>an empty scene gives `no_face_detected`;</li><li>small faces give unknown attention;</li><li>corrupt, unterminated and oversized frames are refused;</li><li>MJPEG without Huffman tables decodes to identical pixels;</li><li>a different frame size rebuilds the plan;</li><li>model tampering, symlinks, hard links, permissions and install refusal;</li><li>worst-case event size.</li></ul> |
| `crates/perception/tests/worker_process.rs` | The `lamp-perception` process with the runtime's datagram channels and a real frame server:<ul><li>observations carry the original lineage, paced by the interval;</li><li>a frame from before a privacy close/reopen is discarded;</li><li>closed privacy gives no requests;</li><li>camera loss is reported and the worker keeps running;</li><li>heartbeat loss ends the worker;</li><li>a missing model gives a fault and never `Ready`.</li></ul> |

## Measurements (not device results)

The table measures the release build, decode plus inference of one 1280×720
drawn-scene frame, n=20:

| Host | Decode p50 / p95 | Inference p50 / p95 | Total p50 / p95 |
|---|---|---|---|
| Development Mac (Apple silicon) | 1.34 / 1.47 ms | 8.56 / 8.86 ms | 9.94 / 10.22 ms |
| Linux ARM64 container (Colima on the same Mac) | 1.22 / 1.29 ms | 8.30 / 8.71 ms | 9.49 / 9.92 ms |

Lamp's
CPU, thermal state and concurrent audio load are unmeasured. The proposed
target is p95 ≤ 150 ms from frame receipt to publication on Lamp.
