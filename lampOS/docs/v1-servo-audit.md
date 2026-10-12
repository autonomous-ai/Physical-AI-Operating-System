# V1 servo behavior to preserve in the Rust motor runtime

Reviewed 12 October 2026. V1 source is main
`170a1b85e7e6197097019e4175efdcbe416e3c2c`, fetched for this audit.
The earlier comparison used `d5efe9d7b73cc529b34cd4abe97624682a82ca94`.
V1 gained relevant protections between those revisions; this document includes
them. Rust comparison: `lamp-v2-chat` at `4b07877922d542033cb1970179c7946975ba35e2`
and the retained, uncommitted manual-head candidate described below.

This is a source audit with offline checks, not a device qualification or a
completed motion port. No bus was opened, servo moved, service restarted, or
calibration changed for this audit. Agent 1 owns voice; Main owns this motor work.

## Conclusion

Preserve V1's per-unit calibration, smooth following, recording timing, overlapping
hold ownership, and contact/overload response. Preserve its distinction between
holding the measured pose and releasing torque. Port these behaviors into one
Rust motor worker with bounded requests and explicit completion/fault evidence.

Do not transplant its numerical limits as physical degrees. The normal V1 path
uses dimensionless calibrated positions while multiple callers describe and use
them as degrees. V1 also has no head-to-base collision test. Its new joint and
posture limits would allow the retained head-contact trajectory in an offline
calculation. Those gaps must be corrected, not inherited.

## Actual execution path

```text
Move / nudge / gesture / tracking intent
                  |
       route or behavior-specific checks
                  |
          AnimationService
        /          |           \
   recordings   direct move   tracking follower
   ~30 Hz       ~30 Hz        ~33 Hz, latest goal
        \          |           /
          shared serial-bus lock
                  |
         LeLampFollower.send_action
       write inhibition + joint clamps
                  |
      LeRobot calibration / Feetech bus
                  |
       five STS3215 servos, IDs 1–5

   20 Hz load/contact monitor -> hold or torque-off

   Raw startup / zero / park -> SDK register writes
       (bypass send_action's normalized joint clamps)
```

The factory selects `AnimationService` for the `feetech` driver. The other
`motors_service.py`, Reachy and StackChan implementations are not the Lamp path.
V1 uses threads inside HAL, a shared bus lock and shared state, not one isolated
process per joint. Retain a single bus owner in Rust; give the worker its own
process rather than reproducing competing callers and an unbounded event list.
[Factory](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/drivers/motors/factory.py),
[service](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/drivers/motors/animation_service.py#L88).

## Joints, coordinates and limits

These are the current `SAFETY.md` values. On V1's normal path they act on
normalized position units, **not verified physical angles**.

| ID | Joint | Current V1 software clamp |
|---|---|---|
| 1 | Base yaw | −105 to +105; SDK normalization subsequently clips to ±100 |
| 2 | Base pitch | −2 to +67 |
| 3 | Elbow pitch | −11 to +77 |
| 4 | Wrist roll | −60 to +60 |
| 5 | Wrist pitch / head tilt | −89 to +57 |

The declarations were derived from the range used by recordings, presets and
tracking, with a margin. They are not a collision envelope for every combined
pose. Tracking also has its own narrower travel table measured on lamp-ac82;
range-demo waypoints use another set of bounds. Do not silently combine these
into universal limits for every unit.
[Safety declarations](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/robots/lamp/SAFETY.md#L20),
[tracking constants](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/drivers/tracking/constants.py#L76),
[range demo](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/drivers/motors/range_demo.py#L51).

`LeLampFollowerConfig.use_degrees` defaults to false. `AnimationService` does not
override it. `LeLampFollower` consequently selects `RANGE_M100_100`. The pinned
LeRobot implementation maps a calibrated raw count to
`200 * (count - range_min) / (range_max - range_min) - 100`, with optional sign
reversal. Degrees use a separate branch. The CoG helper nevertheless applies
`math.radians()` directly to these normalized values. Likewise the named
120 deg/s policy stretches time using normalized deltas. Neither label establishes
the physical angle or speed.
[Configuration](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/follower/config_hal_follower.py#L49),
[mode selection](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/follower/hal_follower.py#L53),
[LeRobot conversion](https://github.com/huggingface/lerobot/blob/55198de096f46a8e0447a8795129dd9ee84c088c/src/lerobot/motors/motors_bus.py#L776).

Calibration belongs to each assembled unit and survives OTA under
`/var/lib/hal/calibration/robots/hal_follower`. V1 falls back to the repository's
`hal.json` if the unit file is absent. Preserve persistent per-unit calibration;
reject missing/mismatched calibration in Rust instead of inheriting that fallback.
Rust's existing [calibration types](motor-runtime.md) already distinguish encoder
counts, dimensionless positions and EEPROM bindings. A physically qualified
angle/zero/sign mapping and pose-dependent envelope remain necessary.

## Useful movement behavior

| V1 behavior | What to preserve; what to qualify |
|---|---|
| Direct moves default to 2 s and interpolate at 30 Hz. The route stretches duration for the declared speed ceiling and refuses an unknown pose when that ceiling applies. | Begin from fresh measured positions and retain duration stretching, in correct units. Driver read failures currently permit cached positions or a direct final target; Rust must not substitute those for fresh pose evidence. |
| Recordings preserve authored timestamps, stretch fast intervals, then linearly resample to the playback grid. Transitions blend into the recording. | Reuse authored gestures and timing as candidate assets, bound memory and validate every trajectory including its entry transition. Timestamp-less recordings currently bypass this resampling/check path. |
| Tracking runs independently of vision. A critically damped follower uses elapsed monotonic time, 30 ms wakes, a 60 ms maximum integration step, and coalesces changes below 0.08 position units. | Preserve smooth acceleration/deceleration and small-command suppression. Current smooth-time is 0.32 s and named max speed is 55; convert and qualify these rather than treating them as physical defaults. |
| Tracking has deadbands, prediction, anti-windup and decaying velocity for rejected observations. | Preserve settled stillness and deceleration. Do not port automatic search sweeps into the desk release. Stale/missing vision must not keep driving motion. |
| Explicit, scene, tracking and look holds have separate owners; overlapping camera freezes release only after the last consumer exits. | Port this ownership behavior to bounded typed leases. One consumer ending must not release another consumer's hold. |
| PID gains differ by joint; tracking uses a hardware acceleration setting of 30. | Retain configuration/provenance as candidates, not universal defaults. Qualify register units, per-unit holding requirements and readbacks before changing hardware configuration. |

Sources: [movement and playback](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/drivers/motors/animation_service.py#L675),
[recording timing](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/drivers/motors/recording_timing.py),
[smooth follower](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/drivers/tracking/servo_follow.py#L278),
[hold owners](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/drivers/motors/hold.py),
[freeze ownership](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/drivers/motors/freeze_lease.py).

## Contact, overload and physical stopping

The `orangepi_sun60` profile currently configures the following. `Present_Load`
is driver duty in tenths of a percent, with a direction bit; it is not measured
force. These numbers are source defaults, not newly qualified limits for 4ace.

| Mechanism | Current V1 behavior |
|---|---|
| Monitor | Poll every 50 ms; bus work and scheduling add delay. |
| Fixed contact floors | Base yaw 550, wrist roll 750, wrist pitch 650; sustained for 50 ms. |
| Contact stop | Set halt, read actual pose, write that pose as the new goal, then block further goals under one bus-lock hold. Torque stays on. |
| Continued contact | After sustained contact during the pause for 1 s, cut torque with no parking move. |
| Contact pause | 3 s, then normally unblock and return to idle. |
| Overload | Load 800 sustained for 1.5 s; inhibit goals before torque-off; retry failed torque-off writes. Lockout is 120 s. |
| Torque ceiling | 600 on base yaw, base pitch and elbow pitch. A load threshold of 800 alone cannot protect a stalled joint capped below it; lag evidence matters. |
| Per-unit learned contact profile | Per-recording maximum load/lag over nearby frames (±3), plus load margin 120 and lag margin 35. No shared-unit profile fallback in current main. |
| Direct-move exception | Extra off-playback load/lag floors are bypassed during a direct move and for 0.5 s afterward. Fixed contact floors and global overload remain. |

Do not collapse `hold`, `halt`, `release` and `zero` into one stop operation.
`hold` suppresses ordinary idle behavior; `halt` attempts to hold the actual
pose; `release` normally parks before dropping torque; `zero` moves the whole
body and reconfigures servos. Overload torque-off deliberately does not park.
[Configuration](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/robots/lamp/servo_overload.json),
[contact/overload implementation](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/drivers/motors/animation_service.py#L1139),
[pure load guard](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/drivers/motors/overload.py).

The current Rust `MotorControl::stop` invalidates authority and discards pending
host output. It explicitly does **not** replace an already accepted servo goal
or prove physical stop. This also applies to the retained manual-head candidate
at `/private/tmp/lamp-manual-head-build-4cw_v0r2/source/lampOS`. Porting V1's
measured-pose stop is a substantive missing behavior. It needs bounded read/write
transactions, truthful failure reporting and subsequent position/load evidence.
Torque release needs its own policy because it can let the arm fall.
[Current stop boundary](../crates/motor/src/control.rs),
[motor contract](motor-runtime-contract.md).

V1's failure handling needs tightening during the port: missing load reads reset
the dwell timer without stopping; failed pose-pin writes are logged but the code
can still say it halted; contact recovery can return to idle by elapsed time;
overload recovery can clear unconfirmed torque-off state and re-energize. Rust
must not convert a timer, attempted write or missing observation into proof that
an obstruction cleared or a joint stopped.

## Why joint limits did not cover the head contact

The retained 4ace trial started at head count 1896. The last measured head position
was 2073 and the last accepted goal was 2094. Requested target 2238 was never
written. The user reported that the head touched the base. These are historical
trial observations, not a new movement attempt.

Using the pinned SDK conversion and exact current V1 stability functions offline:

- All 154 accepted goal poses fit the current five joint clamps.
- Head positions 1896 / 2073 / 2094 / 2238 map to approximately
  −46.24 / −24.21 / −21.59 / −3.67 normalized units, all inside −89…+57.
- V1's modeled radial CoG stays below its 22 mm recording ceiling on every
  accepted goal. Its direct-move limiter returns the full requested move.

This establishes that those **software gates** would allow the recorded goals.
It does not establish how V1's contact monitor would have reacted on the device.
The numerical CoG output is not validated physical CoG: the model assumes link
masses at link origins, uses estimated masses, and receives the wrong angular
units. The URDF expressly omits collision meshes. A tip-over check cannot replace
head/base, head/arm or cable-clearance checks.
[Model and limiter](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/hal/drivers/motors/recording_stability.py#L139),
[URDF caveats](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/170a1b85e7e6197097019e4175efdcbe416e3c2c/robots/lamp/urdf/lamp.urdf#L1).

Other gaps: absent geometry disables the posture gate; raw startup/zero/park
writes bypass the normalized final clamp; the posture checks do not certify the
whole swept path. Endpoint or authored-frame checks alone do not establish that
intermediate poses are safe. Rust must validate the combined pose and swept
movement, including startup, manual movement and recovery, through one boundary.

## Port order and acceptance evidence

1. **Physical stop first.** Keep output inhibition, add an explicit bounded
   measured-pose hold/stop transaction and truthful outcome. Faults must not
   resume an old target. Prove cancellation at each write/read boundary and retain
   accepted-goal versus actual-position evidence.
2. **Units and pose-dependent limits.** Bind the five joints to this unit's
   calibration, sign, physical zero and hardware readbacks. Add a conservative
   head/arm/base clearance envelope for combined poses; reject unknown posture
   or missing required geometry. Retain the contact trial as a regression that
   individual joint clamps are insufficient.
3. **Port motion quality.** Reuse smooth following, measured time steps, timestamped
   gestures and independent hold/freeze ownership. Bound queues, duration, rate,
   acceleration and stale-goal lifetime. Stillness is a valid successful outcome.
4. **Qualify contact protection per unit.** Reuse load plus lag and per-unit profile
   ideas, with bounded profiles tied to calibration and trajectory identity.
   Missing feedback fails closed; recovery requires fresh evidence and an explicit
   policy, not automatic idle or re-energizing when a timer expires.
5. **Integrate choreography after the motor boundary works.** One supervisor and
   bus writer accepts cancellable intents from the interaction owner. An optional
   motor fault inhibits movement without killing healthy audio.

Initial software targets for later implementation: motor control/revocation is
serviced at least every 10 ms; no new ordinary goal starts after revocation is
observed; a stop/hold attempt reports accepted, refused or failed within 100 ms.
These are proposed local budgets, not measured mechanical stopping times. Test
first use, repeated motion, maximum qualified load, serial stalls, missing
readback and cancellation. On hardware, measure receipt-to-goal acceptance and
receipt-to-observed stopping separately, with fresh clearance and pose evidence.
Do not repeat or widen the known colliding trajectory to obtain those results.

## Verification and retained evidence

Nine offline checks passed against exact production helpers: load-direction
decoding; overload dwell/retry; timer reset on missing feedback; contact dwell;
learned neighboring-frame margins; smooth convergence without overshoot;
overlapping freeze ownership; independent hold ownership; and recording timeline
stretching. The harness uses a fake clock/service and a supplied unused tracking
session-duration setting. It does not import or boot HAL, open serial, or prove
hardware timing. The recording check uses `policy=None` and the module's 250
position-unit/s fallback; it does not claim to test the full route.

The full V1 pytest suite was not run: the inspected Python 3.12 environment has
no pytest. No Rust runtime source changed, so no Rust build or device run is
claimed for this audit. The separate incident calculation was independently
inspected and rerun using the same immutable inputs.

The [evidence manifest](verification/v1-servo-audit-20261012.json) pins source,
helper results, incident calculation and local script/input hashes. Private
analysis scripts stay outside the standalone Rust repository:
`/private/tmp/lamp-v1-servo-audit-riohv10v/probe.py` and
`/private/tmp/lamp-v1-servo-audit-b38uyfoa/incident_guard_calculation.py`.
Run either with `/usr/local/bin/python3.12`; the latter uses the retained incident
files and SDK checkout named in its manifest. No recordings, binaries or Python
runtime dependency are added to lampOS.
