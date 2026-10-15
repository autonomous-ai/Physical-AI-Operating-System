# Optional motor integration

This integration adds an explicit, independently supervised motor worker to
`lamp-voice directed` and `serve`. It composes with the current camera, reference,
provider-continuation, ring and optional-notice paths; it changes none of their
defaults. Default invocations remain motionless and launch no motor process.
There is no gesture generator, automatic inspection, nod, park, home, room scan,
torque write, or movement attached to Ready, input or an answer.

```text
lamp-voice directed PRIVATE_PROVIDER.json SECONDS NEW_OUTPUT \
  --camera-config PRIVATE_CAMERA.json \
  --motor-worker ABSOLUTE_LAMP_MOTOR_WORKER --motor-config PRIVATE_MOTOR.json
```

Both motor arguments are required together. `directed-fixture` rejects them.
These flags authorize opening the configured idle bus, not arbitrary motion.
The dedicated executable is built by `cargo build --release -p lamp-body
--bin lamp-body-worker`; there is no executable search or legacy fallback.
Real serial execution remains Linux-only and unqualified on hardware.

## Startup and ownership

Before mandatory audio workers start, the coordinator reads an owned, private,
single-link regular configuration file (at most 16 KiB) and an owned executable
(at most 64 MiB, not group/world writable), without following their final
symlinks. It copies them into its fresh private session directory, validates the
frozen configuration and original qualification times, and records SHA-256
identities. File copying, hashing and sync are not in the audio loop. Sources
changing during copying are rejected. Same-user malicious replacement is outside
this trusted provisioning boundary; digests identify copied bytes, not a signed
hardware qualification.

The one spawn attempt occurs in the pre-capture startup slice, after the actual
privacy worker observation and configured camera path have produced an original
snapshot with microphone and camera permissions Allowed. It is forbidden after
StartCapture. Missing camera, failed provisioning or unavailable permissions
leaves movement absent while voice proceeds. No denied snapshot is suppressed to
reach Ready. A camera permission is not a current frame or cabinet clearance.
The process opens idle and receives no automatic inspection or segment.

After spawn, the existing `motor::runtime::Supervisor` owns the exact child,
worker boot, private sockets and serial writer. Authority updates, including
denial, go to motor priority control before mandatory audio sends, so a failed
audio channel cannot skip revocation. Polling handles at most eight receipts per
conversation tick and does not wait/reap synchronously. A failed motor is disabled
before any later offer or authority renewal; healthy voice retains its owner and
playback. Stop is one nonblocking attempt; there is no retry, replacement, copied
command or generation relabeling.

Ready proves only the worker's accepted authority and idle serial open. It never
creates capture/admission/listening readiness. The motor may remain idle for its
explicit finite configured lifetime (maximum 630 seconds); expiry disables it,
and does not extend the live service or any authority lease. This is not an
all-day motor service yet.

## Trusted offer seam

`coordinator::motor::{Evidence, MovementOffer, OptionalMotor}` exposes the
trusted choreography boundary for one explicit, already qualified segment. The current conversation does not call
it. A real producer of placement/clearance evidence is still required; ordinary
camera context and Gemini output cannot supply it automatically.

`offer_inspection` is an explicit request with original request time, Motion
permit and placement/qualification evidence. `offer_segment` additionally
requires a completed fresh inspection, an explicit nonempty joint selection, five calibrated normalized target slots and original duration. Unselected IDs are never emitted; the core retains their exact current read counts. These are dimensionless `RANGE_M100_100` values, not degrees.
The worker independently checks actual stationary readbacks, profile, overload,
range, trajectory velocity/acceleration, camera lineage and all expiry bounds at
the final bus boundary. A successful inspection receipt is not a substitute for
those final checks. No fallback calibration or numeric physical safety limits
are supplied here.

There is one pending request or active segment, no application retry queue, and
at most 128 transmitted offers per instance. Request identities and times are
preserved. An expired, wrong-source, mismatched grant/qualification, reused or
out-of-range offer is rejected; a stale cancellation cannot affect another
motion. Owner or privacy revocation clears the pending offer before sending any
later work. A late callback cannot restore it. Segment receipts retain the original interaction owner. An inspection receipt
retains its request identity and original observation time; it does not mint
a movement permit or complete a voice reply. Input end and actual playback
transitions invalidate older nonverbal permits even if the turn owner stays the
same. Those queued segments are inhibited rather than relabelled for the new
phase.

Only bounded lifecycle changes become trace events (at most 520 per instance);
10-ms progress and per-position host write traffic are not logged repeatedly.
The existing global finite trace limit is unchanged. No receipt asserts optical
pose, actuator halt or clearance. Optional failure leaves one degradation record.
The live adapter also adopts a stored supervisor exit, deadline or cleanup fault
on the same poll even when that poll returns no receipt or error, including
during teardown. Reporting does not wait for another authority publication;
the first failure and single degradation record remain separate from voice
ownership and from successful cleanup evidence.

## Privacy release correction, separate patch

The existing worker reports explicit privacy closure as `ControlLost` and a
failed operation. That evidence is retained. `StopEvidence.privacy_revision`
adds only the exact fresh denied snapshot consumed by the worker; missing legacy
evidence remains unknown. The supervisor checks it against the original denial
it actually sent and its timestamp.

`Supervisor::privacy_release_confirmed(revision)` is separate from `clean_exit()`.
It requires matching consumed denial, successful output discard, explicit serial
close/settings restoration, exact child reap with a normal exit code, private
IPC cleanup, and no lifecycle fault or forced kill. A nonzero operation exit
alone neither proves nor disproves resource release. Missing receipts, unknown
permissions, expiry, protocol errors, failed discard/close, signal termination,
forced kill and unreaped children cannot establish this proof.

At final shutdown the coordinator revokes authority and stops all children before
waiting. It polls motor in the existing shared shutdown budget (300 ms, or 750 ms
with audio diagnostics); it does not widen motor's 20-ms inhibition, 250-ms driver
teardown or 250-ms reap bounds. A shared deadline may therefore expire before a
late motor reap; this remains an explicit cleanup failure. Unconfirmed cleanup
removes the physical-mute restart token and blocks reopening. A matched ordinary
physical-mute release may reopen a fresh idle worker only after cleanup; no
inspection, placement frame, pending command or permit is carried over.
Other motor degradation latches movement off for the rest of the live service,
even across later audio mute/reopen cycles. This does not restart the failed
optional component. Unexpected Drop is best-effort kill only; it retains frozen
launch files when reaping is unknown and is not clean shutdown evidence.

## Reuse, targets and limits

The protocol, calibrated unit conversion, core, trajectory checks, Linux bus
ownership and supervisor remain the existing Rust motor implementation. The
separate supervisor change adds release evidence only, not a new scheduler or
transport. Pinned V1 main `d5efe9d7b73cc529b34cd4abe97624682a82ca94` was inspected:

- `hal/follower/hal_follower.py:258` keeps the position-only synchronized write;
  this integration adds no register or torque commands.
- `hal/drivers/motors/animation_service.py:1153–1185` blocks future goals before
  overload action. The Rust final gate retains that ordering; automatic torque
  release/retry is deliberately absent on an unqualified load-bearing posture.
- V1 disconnect's configurable torque release is not copied into live shutdown.
  See [the core audit](motor-runtime-contract.md) and [worker contract](motor-worker.md).

Target: an already validated offer reaches the existing nonblocking command
channel within 20 ms, and local cancellation is processed by the next bounded
coordinator/worker tick (target at most 20 ms). Kernel spawn/I/O and physical
servo travel/stop are not hard deadlines. Startup preparation is not included in
that command boundary. Host tests measure local calls and exercise real Unix
channels/subprocesses with an inert STS3215 emulator. Synthetic close receipts
are explicitly test resources, never proof of actual serial or mechanical safety.

The loaded-host fixture setup is described in
[Voice fixtures](voice-test-fixtures.md): an inert preflight executes and reaps
the exact frozen wrapper before protocol timing, and voice ownership begins
after unrelated setup. Production deadlines, real Ready and cleanup checks
remain unchanged; preflight is not runtime readiness or cold-start evidence.

The selected-joint core keeps an explicit subset of IDs 1 BaseYaw, 2 BasePitch,
3 ElbowPitch, 4 WristRoll and 5 WristPitch. Host receipts must match the original
offered selection. An ID5-only connected inert test verifies body IDs never
appear in position packets; it does not qualify a physical head envelope.

The schema-2 read-only inspector has explicit `--device`, `--unit-id` and
`--lock-directory` arguments. Its five register windows cover model, limits,
homing/mode, raw torque/profile and telemetry. Shared V1 normalization/sign and
actual unit EEPROM limits/homing are explicitly separate, as described in
[motor register binding](motor-register-binding.md). No fixture calibration or
raw profile observation supplies qualified travel, speed, acceleration or
collision clearance on a real unit.

There is no current automatic trusted placement/gesture producer in the
conversation loop. Until such a producer and real qualification exist, the
opt-in live process can be exercised only idle. This is a working ownership and
write boundary, not a claim that physical social gestures are ready.

Hardware checks still required: per-unit calibration/qualification and combined
joint clearance, fresh verified placement source, foreign ownership, adapter
open/close effects, observed bus readbacks, overload behavior and measured physical
latency. There is no device movement, cloud call or ARM qualification in this
proposal. Root owns native qualification and any later physical activation.
