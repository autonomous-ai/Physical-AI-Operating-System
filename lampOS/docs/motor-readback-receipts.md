# Original encoder readback receipts

The motor worker's `Inspected` and `ReadbackReached` events now include a bounded
`Readback` of all five actual checked telemetry samples in ID order. Each sample
contains counts, original host receive time, raw signed velocity, raw load
magnitude, moving and status flags. A requested target is never substituted.

`MotorControl::readback` does no bus I/O. Before a complete inspection, during a
partial transaction, after sample expiry or after terminal inhibition it cannot
export valid readback. Reading the cached snapshot again preserves the original
time. No device clock, simultaneous sampling, physical halt or placement claim
is introduced. The Linux wrapper forwards the same core method.

The worker obtains the snapshot only from the actual backend after inspection
or reached transition. A backend that lacks this method fails closed. The live
motor consumer checks each sample against both event time and present host time.
Old messages lacking the mandatory field are rejected, so worker and consumer
must be deployed together. Readback packet transport remains under the existing
bounded original envelope/boot/sequence checks.

Core `ReadbackReached` retains its configured tolerance semantics. Any caller
requiring exact encoder return must compare the actual counts itself; a reached
label alone is insufficient. The finite qualification controller performs that
additional check. All existing profile/torque/authority guards remain unchanged.
