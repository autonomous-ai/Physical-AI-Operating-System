# Optional camera context in directed chat

This candidate adds `--camera-config /absolute/private-camera.json` to `lamp-live directed`.
Camera context is **off by default**. There is no camera worker or device open
without that option. `directed-fixture` rejects it. This is direct, fixed-view
context for an admitted conversation turn; no inference service, face tracking,
room scan, movement, or visual readiness indication is added.
Opening requires both that private opt-in configuration and the same fresh
physical privacy gate used by the microphone in the directed runtime. This
slice does not add a separate camera UI switch. Camera-specific failure can
still revoke camera permission without revoking microphone permission.

The private configuration uses the existing camera provisioning schema:
explicit Linux device path, exclusive lock directory, USB identity, source
label, MJPEG width/height, frame interval, buffer count and maximum driver frame
size. The parent hashes it and the worker verifies that same hash. Chat mode
requires an explicitly configured resolution no larger than 640×480 and a
capture buffer no larger than 1 MiB. Start with 320×240 during hardware
qualification. The negotiated mode must exactly match the request. There is no
resizer or silent resolution fallback. Each cloud image remains limited to
64 KiB; a larger JPEG is skipped with an `oversize` receipt. Provisioning must
confirm that actual frames fit, including difficult lighting and busy scenes.

## Data and authority

The supervisor preallocates independent random camera and provider worker IDs.
It supplies both identities; neither worker learns the other's trusted identity
from an incoming frame. The camera serves its existing private Unix stream via
`FrameServer`; the provider is the sole allowed `Context` client. Control and
PCM continue to use their existing bounded datagrams. JPEG bytes never enter
those datagrams, runtime traces, or diagnostic files.

After a Start is successfully submitted to the coordinator-selected Gemini
session, the provider attempts one fresh frame for that request while its input
is open. It has one outstanding camera request, one retained frame, and a bounded
stream-parser buffer. Both frame buffers are bounded by the configured limit.
The attempt expires after 250 ms without delaying Audio or End. Final export
rechecks that original deadline after draining control. A frame retains its
original camera grant, camera worker/epoch/sequence, configured source and
host dequeue/copy timestamps. Its host dequeue-start age must be below 250 ms
at export. This is not an exposure timestamp or proof that the image depicts the
instant the user finished speaking.

The provider drains priority control before reading and again before exporting.
Unread priority control defers export. Final checks reject a revoked grant,
wrong worker/mode/source, reused frame sequence or changed capture epoch,
changed request/session, future timestamp, expired
frame, or oversized JPEG. A conservative host-monotonic to `Instant` bracket
maps the original frame age; a bracket over 10 ms is skipped, not restamped.
`Supervisor::try_image` supplies the existing last boundary, with its one-slot
queue, JPEG envelope/size/age checks and original submitted-session check.
Images are never replayed across a reconnect or a later turn.

Camera permission changes, including a coalesced close/reopen, revoke the old
provider-local export token. That token maps to the original `CameraGrant`; it
is not a new capture permission. Already queued, unsent images are removed. If
an image is already being written to Gemini, revocation abandons that connection
because a partial network write cannot be undone. The existing supervisor then
reports the request lost exactly once and may recover. Local capture and
cancellation stay live; answer continuity is not guaranteed for this case.
Bytes already delivered to a provider cannot be recalled.

## Optional failure and reporting

A missing/invalid camera configuration or failed camera worker is reported as
unavailable; the audio conversation continues. Camera failure closes its grant,
requests a stop, and leaves mandatory audio ownership and input leases intact.
The worker has its own authority/progress bounds. Final shutdown uses the same
shared worker deadline, drains camera metadata, and retains forced-cleanup
failures. There is no automatic camera restart, permission reopening, or retry
of an unsuccessful image attempt for the same request.

`visual_context` trace entries are bounded metadata receipts. `queued` means
Gemini accepted the JPEG into its local image slot, **not** that it was written,
seen, understood, or used in an answer. Camera skips and revocations are explicit.
One pending receipt is kept if control backpressure occurs; replacement increments
`coalesced_receipts`. A missing/coalesced receipt is not success evidence. Camera
lifecycle events never grant listening readiness or change the conversation state.

## Verification and limits

Host tests use the real private stream and datagram transports with synthetic
JPEG envelopes and an in-memory Gemini service. They cover one image per original
request/session; wrong source, future/stale/oversized frames; post-read privacy
changes; revocation; no reconnect replay; and missing/closed-camera failures that
still complete an ordinary audio answer. Existing camera and Gemini image tests
cover partial-stream closure and abandonment during an in-flight cloud write.
Mandatory worker faults and termination observed while the optional camera is
starting remain fatal; only camera-owned startup errors become unavailable sight.

The cooperative target is one bounded nonblocking camera service step per 2 ms
provider tick, independent of PCM credit; no camera wait is added to voice input.
These tests do not prove a 2 ms worst-case scheduler or kernel latency. The camera
loop is a separate process. Frame buffers and reads are bounded, but allocation
and a bounded copy still cost CPU. First-use camera startup occurs independently
of voice readiness. Real negotiated resolution, USB identity/permissions, actual
JPEG sizes, host frame age, cloud sight quality, and voice timing under camera
load remain hardware qualification tasks. No physical or cloud image test is
claimed by this candidate.
Gemini's existing audio/image WebSocket serializes an image write before later
cloud-bound audio, with a 400 ms image-write deadline. Local microphone handling
does not wait for that write, but the cloud audio may; zero added voice latency
is not claimed.

This implementation reuses Agent2's reviewed private frame transport and camera
worker, and the integrated Gemini image/revocation API. It does not activate
Agent2's reduced-image/perception service or change Gemini model, voice, VAD,
resumption, Require barrier, or audio processing settings. The camera-enabled
instruction only states that an attached image may provide visual evidence;
it does not assert sight when no frame is provided.

The V1 provenance comparison is main
`d5efe9d7b73cc529b34cd4abe97624682a82ca94`: `hal/realtime/orchestrator.py`
`_capture_frame` checks camera privacy before and after capture, and
`hal/realtime/voice_agent/gemini_live.py` invalidates cancelled `look` calls.
Those privacy and cancellation principles are retained. This first Rust path
instead attaches bounded fixed-view context while the admitted input is open;
it does not implement V1's later `look` tool response, servo settling, resizing,
or persisted look-frame file. Historical paths are provenance only, not build
or runtime dependencies.

### Combined integration checkpoint, 2026-10-11

Main integrated the reviewed r4 frame-deadline correction together with the
committed motor core, retaining both in the lockfile. The combined source-only
export has 304 files and manifest SHA256
`38bc990e436aa8e43e171b6b51f20facbe4d44d8f4bcca8310c3784bb75860cf`.
Formatting, strict all-target workspace Clippy and the serial workspace suite
passed on the macOS host: **1,166 passed, zero failed, four existing ignored**.
The ignored cases are the two prolonged recovery soaks and the 60/180-second
provider timing tests; they were not rerun in this checkpoint.

Commands and log identities are preserved in
[host evidence](verification/camera-context-r4-20261011/host-gates.json).
A combined Linux/ARM64 build is pending at this checkpoint. Earlier candidate
ARM results do not stand in for this exact combined build. No camera has yet
been opened by this integration on Lamp, and no visual answer is qualified.
