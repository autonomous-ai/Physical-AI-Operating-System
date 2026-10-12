# Combined-pose clearance and measured stop

This is the first implementation batch from the [V1 servo audit](v1-servo-audit.md).
It belongs to the separate servo/motor output worker, `crates/motor`. It supplies
no new gesture, real-device clearance preset, torque change, or automatic recovery.
Host tests use a synthetic protocol peer; no servo was moved for this batch.

## Clearance admission

`QualifiedEnvelope::with_motion_protection` binds a `ClearancePolicy` and
`StopHoldPolicy` to the envelope's existing unit calibration, record and validity.
An inspection may still run without them; motion admission cannot. Runtime
configuration now requires `qualification.clearance` and
`qualification.measured_stop`. Old motion configurations are refused until
explicitly reprovisioned. Parent and worker must be updated together.

Clearance contains 1–16 cells, each with five `minimum` and `maximum` coordinates
in the calibrated, dimensionless −100..100 domain. These are declared verified
regions, not physical degrees. No built-in region is considered safe for Lamp.
The provisioner must establish that every joint combination inside each cell is
clear for this exact mechanism and placement.

The complete Cartesian hull between the measured starting pose and quantized
target, expanded by every joint's qualified tracking error, must fit inside one
cell. This covers unequal progress of the joints within that allowance. Safe
endpoints in separate cells are insufficient; cells are not stitched together.
The same check runs from measured pose to each generated goal before writing,
and complete active telemetry is checked against the cells. Joint, EEPROM,
velocity, acceleration, load, freshness and authority checks still apply.
This cannot certify bad provisioning, unobserved contact or motion outside the
tracking allowance. The known head/base contact is not cleared by this change.

## Stop sequence

Ordinary motion authority is latched closed before any stopping work. Subsequent
commands and stale callbacks cannot restart it. Unlike an interaction goal, the
explicitly provisioned stop attempt can finish after privacy or ownership is
revoked; it uses no camera or microphone. This narrow stopping authority permits
only fresh reads and one measured-position hold of previously commanded joints.

```text
Cancel / Stop / revocation / eligible motion fault
                  |
         Inhibit ordinary commands ----> Inhibited receipt immediately
                  |
       Any previously accepted position goal?
             no --+--> NoAcceptedGoal (no new packet)
             yes  |
       Known serial framing and eligible failure?
             no --+--> Failed (no guessed recovery packet)
                  |
       Drain only the existing read, if any
                  |
       Read all-five torque/profile, then fresh positions
                  |
       Validate profile, positions, clearance, freshness
                  |
       One selected-joint hold at those exact encoder counts
                  |
       Fresh telemetry until stationary and within tolerance
                  |
       ObservedAtRest OR bounded Failed
                  |
       One output discard; close; Stopped with separate evidence
```

No cached goal is substituted for missing positions. No position is clipped into
a different hold target. A partial position frame or uncertain stream does not
receive another packet. Existing read framing and reply tracking are preserved;
an old reply is drained, never promoted to the new source measurement. Profile,
status, calibration and clearance failures refuse the hold. There are no torque,
profile, mode or EEPROM writes in this path. It cannot rescue an already trapped
head, release contact force, or recall bytes already accepted by the adapter.

Provisioned `deadline_us` is 1–100,000 µs and `sample_age_us` is 1–20,000 µs,
no larger than the deadline. The sample age is checked both before and after a
hold write; an over-age write retains accepted-byte evidence but cannot certify
rest. Nonblocking I/O retains the existing call bound. The attempt has at most
512 steps and 512 I/O operations, including a bound when the supplied clock does
not advance. The worker services it on its normal 2 ms loop, independently of
cloud, audio and receipt-delivery success. The supervisor's existing 20 ms
inhibition and 250 ms teardown bounds are unchanged.

These are software budgets. A blocked driver call or unscheduled process cannot
be preempted by the library. No mechanical stopping latency is claimed.

## Evidence and cleanup

`Inhibited` means no further ordinary motion, not physical rest. Its original
accepted/partial bytes and stop time remain unchanged; `discard_attempted`
distinguishes an actual flush from deferred stop work.

`Stopped.physical_stop` is a separate `HoldReport`: outcome, refusal/failure,
selected joints, fresh source readbacks, bytes accepted by the host, acceptance
time, final readbacks, completion time and discard result. `ObservedAtRest`
requires all five joints stationary, within the source-position tolerance and
below their load caps. Those are host-received servo reports, not acquisition
timestamps, force measurements or optical confirmation. Broadcast writes have
no acknowledgement from the servos.

A failed/unfinished stop or discard cannot produce a clean supervisor exit or
verified privacy release. `Worker::finish`, direct `LinuxMotor::shutdown` and
Drop do not secretly wait or move. If called before the polling sequence has
finished, they abandon it, flush once and retain failure rather than imply rest.
Direct Linux shutdown exposes `physical_stop` separately from serial cleanup.

## Verification boundary

Focused tests cover whole-pose admission, disjoint cells, unequal joint progress,
fresh measured hold, selected-joint bytes, cancellation before any goal, pending
read drainage, partial-frame refusal, missing/corrupt feedback, stale source,
source expiring inside a write, persistent motion/load, cleanup abandonment and
stalled clocks. Worker tests exercise the production core through Unix channels.
The host motor suite passed 187 tests; Linux/ARM64 under QEMU passed 192,
including the Linux-only adapter tests. Strict motor Clippy and the ARM64 release
binary build passed. These are software/emulation results, not device results.
Hardware qualification requires fresh clearance evidence, controlled stopping
observations and measured latency. V1 smoothing, continuous tracking and gesture
migration are later batches.
