# Explicit selected-joint profiles and ID5 preparation

This opt-in contract permits a separately authorized, finite setup of ID5's
SRAM profile. It does not qualify a head gesture, choose numeric settings, enable
torque, move to a startup pose, or send a position. Defaults remain unchanged:
`QualifiedEnvelope::new`, an absent `selected_profiles`, and all-joint motion
require all five nonzero bounded profiles. The read-only inspector is unchanged.

`qualification.selected_profiles` explicitly supplies a `selection` bitmask.
Only selected joints are qualified for position writes. Unselected joints retain
exact observed acceleration/time/speed bindings, which may include zero. Their
identity, EEPROM binding, torque, current encoder position, stationary/velocity,
load and configured envelope checks remain active. They never enter position
packets. A later request including one of these joints is rejected both at
admission and by the active/final-write selection checks; it cannot inherit ID5's
qualification. Shared V1 normalization stays separate from per-unit EEPROM.

The optional `prepare_from` contains five exact original raw profile bindings
(`acceleration`, `goal_time`, `speed`). It enables preparation only when
`selection == 16`, and every inactive binding equals its configured observed
profile. The desired ID5 values are the existing qualification's `joints[4]`
raw profile. Desired acceleration and speed must be nonzero; speed must not use
the sign bit. These are representation and policy checks, not qualified physical
units or numeric safety limits. No values are defaulted. Goal time must already
match: it is never changed by preparation.

## Explicit worker operation

Use the existing exclusive worker/Supervisor and original controller evidence:

1. Install current authority; require a successful explicit all-five inspection.
2. Submit `PrepareHeadProfile { request, requested_at_us, permit, placement,
   speed_raw, acceleration_raw }`. Values must equal the provisioned desired
   ID5 profile. The original Motion permit, camera lineage, current actual
   clearance observation and qualification record remain mandatory. An ordinary
   context image, idle readiness, synthetic owner or old placement is insufficient.
3. Match `ProfileStarted` by request, worker and original owner. The core issues
   only individual ID5 WRITE(46, two little-endian bytes), waits for the exact
   zero-parameter/status-zero ACK, reads all-five profile/telemetry and verifies
   speed changed while acceleration and all other fields remain bound. It then
   issues ID5 WRITE(41, one byte), requires ACK and repeats the full checked
   profile/telemetry read. All five goal registers must equal actual stationary
   encoder readback before and after; encoder counts must remain exactly unchanged.
4. Match `ProfilePrepared` by the same Motion identity. It contains the final
   actual encoder readback and bounded `ProfileProgress`; this is setup evidence,
   not `ReadbackReached`, playback retirement or an executed gesture. Afterward a
   distinct fresh placement/permit and the finite controller's explicit inspection
   are required for a position segment. There is no automatic segment or return.

Only one operation/transaction can exist. Preparation cannot overlap inspection
or motion, requalify itself, retry, or run twice in the same worker. Each tick has
at most one bounded read and one write. Original authority/placement expiry is
checked before every write and after I/O, including partial writes and each
subsequent stage. Worker priority control is reread before its bus tick. The local
accept-to-first-write target is under 20 ms; the controlled 2 ms host-loop test
checks this bound. It excludes kernel stalls and physical travel; no target
hardware measurement has qualified this new operation.

The ordinary optional gesture coordinator explicitly rejects setup-only events.
A trusted finite qualification caller must separately correlate this operation;
it may not hide preparation in an input/answer/Ready callback. No public arbitrary
register writer or new serial owner is provided.

## Failure and evidence

Matching Cancel, Stop, privacy denial, ownership change, expiry, malformed/extra
ACK, missing reply, changed binding, unexpected motion/load or I/O failure inhibits
future writes. There is no reset-to-zero, torque release, park, retry or fault
return. A full request already accepted by the host may have changed the device,
even if its ACK was lost. `StopEvidence.profile_preparation` preserves each
request's accepted bytes, distinct ACK and verified-readback times, the original
raw profiles, and the latest complete observed profiles/time (including a rejected
mismatch). Missing fields remain unknown. It never claims the servo stopped.
Cleanup still independently reports discard/serial close/reap failures.

An exact ACK proves packet shape/ID/status, not a wire transaction timestamp.
STS3215 replies have no request sequence. Exclusive ownership, initial observed
quiet, rejection of unexpected traffic and no retries reduce ambiguity; only the
subsequent exact readback establishes observed register values. Parser reset does
not fabricate freshness. A driver that wedges still needs the existing supervisor;
software deadlines do not preempt a blocked kernel call.

The recorded 4ace inspection showed torque 1, goal==present and acceleration,
time, speed all zero on every ID. That is an input binding, not a motion profile
or qualification. Preparation can change persistent-until-reset SRAM state; an
aborted attempt requires explicit new read-only inspection/provisioning. No old
pose/profile is automatically restored.

## Reuse and raw-unit evidence

Pinned V1 main `d5efe9d7b73cc529b34cd4abe97624682a82ca94`:
`hal/drivers/motors/animation_service.py` around lines 209–231 writes GoalSpeed 46
with the SDK's two-byte write. Its comment states zero removes the speed cap;
`_SERVO_REST_SPEED = {1: 0}` around line 256 applies only to base yaw. Its
approximately 175 / 0.062 degrees-per-second comment is a device observation for
that path, not a model777/ID5 unit contract. `hal/follower/hal_follower.py::configure` uses SDK
configuration and mode/PID setup; no proven application ID5 nonzero profile
setting was found. We reuse explicit register addressing and qualification
separation, not startup torque/mode/PID writes or a base-yaw value for the head.

Primary manufacturer sources checked 2026-10-12:

- [Feetech SDK SMS_STS.h](https://raw.githubusercontent.com/ftservo/FTServo_Linux/main/src/SMS_STS.h),
  lines 26–37, identifies SRAM acceleration 41, position 42–43, time 44–45 and
  speed 46–47 separately from torque/lock.
- [Feetech SDK SMS_STS.cpp](https://raw.githubusercontent.com/ftservo/FTServo_Linux/main/src/SMS_STS.cpp),
  lines 98–110, shows `WriteSpe` writes all seven bytes 41–47, including position
  and time. This combined helper is deliberately not used. The bounded packets
  instead follow separate `writeWord`/`writeByte` framing and ACK semantics in
  [SCS.cpp](https://raw.githubusercontent.com/ftservo/FTServo_Linux/main/src/SCS.cpp),
  lines 57–94, 145–159 and 287–316.
- [Manufacturer STS3215 evaluation](https://www.feetechrc.com/20210430-56680.html)
  documents 4096 positions per revolution and mode-0 acceleration-controlled
  position operation; it does not establish a raw speed or acceleration scale for
  the installed firmware. The
  [STS3215 A/0 datasheet](https://www.feetechrc.com/Data/feetechrc/upload/file/20200611/6372749961523760249976542.pdf)
  describes operating modes/protection, not a verified register scale for this unit.

The official [Linux WritePos example](https://raw.githubusercontent.com/ftservo/FTServo_Linux/main/examples/SMS_STS/WritePos/WritePos.cpp),
lines 16–24, passes speed 60/acceleration 50 and describes 60×0.732 rpm and
50×8.7 degrees/s². Its timing expression implies 50 counts/s per speed unit and
100 counts/s² per acceleration unit. But the official
[Arduino WritePos example](https://raw.githubusercontent.com/ftservo/FTServo_Arduino/main/examples/SMS_STS/WritePos/WritePos.ino),
lines 14–19, passes speed 2400 while retaining the same 60-based comment/formula.
These are generic SMS/STS examples, not an installed model777 firmware contract.
The core SDK passes raw integers through without unit conversion. Its wheel-mode
example uses speed zero to stop, which cannot be substituted for mode-0 semantics.
The differing examples and absence of an unambiguous installed-firmware zero-value
contract are preserved as uncertainty. Generic family tutorial tables and
third-party steps/s claims are not promoted into a hardware contract.
Model777 alone does not identify voltage/gearing/firmware variant. Raw speed and
acceleration therefore remain explicitly unqualified. Root must select and verify
values with actual firmware/register evidence and measured travel/load, current
placement, bounded count envelope and an attended stop plan. No physical safety,
calibrated optical behavior or effective physical stop has been demonstrated by
these inert tests.

## Integration evidence, 2026-10-12

Main applied only the reviewed 16-file delta onto the corrected `605ebce7e`
motor composition, after the provider ordering integration. The old full source
export is not a release candidate. This preserves the newer supervisor identity
regression and terminal finite-controller behavior.

The author retained 160 passing motor tests and 16 passing live motor tests,
formatting and strict package Clippy. A separate production-code review checked
per-write guards, ID5-only register selection, exact readback transitions,
cancellation evidence and OptionalMotor rejection of setup-only operations; no
additional build or device result is claimed by that review. Patch SHA-256:
`b7addb1a3af18de91fb28b561af0632ec68310ed9bcc78cc4e96ebcfbe51d146`.
Independent review JSON SHA-256:
`5ea70e4350a19255ca8fb01ee69b41c3e56bfbb859b902c931b5666b8a736aee`.
The combined ARM gate, explicit native caller and physical qualification remain
pending. Ordinary chat still creates no motor command.
