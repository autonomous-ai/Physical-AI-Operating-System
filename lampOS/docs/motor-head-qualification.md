# Finite ID5 qualification controller

This private proposal adds actual readback receipts and a finite controller over
`lamp-body`'s existing worker and `runtime::Supervisor`. It makes no hardware
claim, supplies no physical envelope or profile, and performs no automatic
motion. The opt-in live lifecycle and selected-joint patches remain prerequisites.

## Readback evidence

`MotorControl::readback()` exports the five checked samples only after a complete
transaction. `Inspected` and `ReadbackReached` now carry these fixed-size samples:
encoder counts, original host monotonic receipt time, raw signed velocity, raw
load magnitude, moving and status flags. The worker obtains them from the actual
core; there is no second serial reader, fresh timestamp substitution, requested
pose substitution, or readiness claim. Partial transactions, expired samples and
terminal cores cannot export a valid snapshot. The live motor consumer validates
sample age both at the original event and at receipt. Mixed old/new binaries fail
deserialization rather than silently accepting an absent receipt.

`ReadbackReached` still means the core's configured tolerance. The qualification
controller separately requires **exact counts** for all five stationary joints.
A one-count difference does not qualify an exact return. Times describe host
receipt, not simultaneous device acquisition, actuator travel or optical pose.

## Four explicit actions, one worker

`runtime::qualification::HeadQualification` accepts a `HeadPlan`, the exact
calibration/binding, controller/worker boots and placement-source boot. It never
creates authority. The trusted caller supplies original Motion permits and
placement observations for each action under the same admitted owner:

1. `send_inspection`: check the complete five-joint initial snapshot against the
   explicit expected counts. Nothing is sent automatically on Ready.
2. `send_segment`: one selected-ID5 segment inside the provisioned count envelope.
3. After an exact `ReadbackReached`, explicitly request another full inspection.
4. Explicitly request the return segment to the initial ID5 count, then require
   an exact stationary reached receipt and Stop the worker.

The fixed count-to-normalized conversion verifies its inverse. Where ordinary
floating-point truncation loses a count, it selects the center of the same encoder
cell and checks the resulting exact count. It does not change normalization/sign
or provide a raw unchecked writer. Body IDs1–4 are absent from all position
packets. Their readback/profile/envelope checks remain active.

The controller has at most two segments, two inspections, one current receipt,
no retry or movement queue, and an explicit overall deadline capped at5 seconds.
Each segment retains the existing1–200ms limit. `poll` uses the existing
Supervisor and processes at most8 ready messages without waiting. The caller must
service original authority/Stop before polling every2ms; the local control target
remains20ms, not a kernel or physical stopping guarantee. Continue Supervisor
polling after inhibition until actual child exit, serial close and IPC cleanup
are reported. Preserve an unsuccessful operation separately from confirmed
resource release.

Any privacy/owner/epoch/freshness/endpoint mismatch, rejected command, timeout,
Stop or cancellation ends the sequence. **There is no automatic return after a
fault, cancellation or privacy change.** Returning is a new explicitly permitted
motion, never cleanup. No torque, mode, EEPROM or raw-profile write is added.

Every send and poll checks the supplied Supervisor's original worker identity
before touching its channels. A mismatch permanently inhibits this controller
without sending authority, data or even Stop to the unrelated child. Public
inspection/segment construction errors also latch inhibition; a fresh permit
cannot revive the failed sequence. Explicitly stale or duplicate authority
snapshots retain their existing ignore-with-current-state-check behavior.

## Offline command and missing native wiring

`docs/provisioning/head-qualification.template.json` deliberately contains null
approval fields. Its counts are an old observed snapshot, not current permission.
It cannot pass validation without explicit provisioning and fresh verification.

After provisioning, the offline-only preflight command is:

```sh
cargo run --locked --offline -p lamp-body --example head_plan -- \
  --dry-run /ABS/PRIVATE/worker.json /ABS/PRIVATE/head-plan.json
```

It validates both private regular files and their qualification binding; it has
no execute mode, serial open or worker-spawn path. Passing it is structural only.

The native worker is already runnable as `lamp-body-worker` under Supervisor.
The finite sequence is executable through its `send_authority`, `send_inspection`,
`send_segment` and `poll` methods; tests run the commands through real local IPC,
the production Worker and the actual control core with an inert bus. There is
**not yet a live CLI producer** attaching this sequence to an admitted listening
turn and a genuinely current reviewed placement observation. That narrow caller
must forward the real physical-privacy snapshot, retain its original CameraGrant,
resolve the approved qualification record and stop/reap on any missing evidence.
Do not replace this with an `allowed=true` command-line flag or a saved photo.

Native preparation still needs: exact worker/config hashes and private ownership;
no competing serial descriptor; qualified adapter effects; fresh five-servo
identity, EEPROM and stationary telemetry; operator-approved ID5-only clearance,
count envelope, profile, load/tracking tolerances and duration; unchanged body
positions; and the explicit profile proposal below resolved. Build/strict Clippy
and inert tests must run on Linux before a separately reviewed physical test.
A listening cue is optional; it must not delay audio or be emitted every turn.

## Recorded inspection and profile blocker

The read-only schema2 native receipt SHA256
`b40ca4cc9152850434af7bcbfaff79f50f4b989e86c420da15a6d6ee11181f44`
used inspector SHA256
`d2de9aae3864e9f3483ee94d521f59be5328e8084eb40d880a78b4409eee70a6`.
It completed10 transactions in79,677us. All five torque-enable bytes were1;
acceleration/goal-time/speed were0; current goal equaled position
`[2015,2048,2047,2039,1765]`; moving/velocity/load/current/status were0.
The close interval was50,830us, settings restored exactly, exclusive release and
close attempted, errors empty. HUPCL was already clear, so this receipt did not
need to clear it. These are observations, not qualified profiles or placement.

The existing `RawProfile::new` continues to reject zero speed/acceleration. This
candidate therefore cannot move the observed4ace configuration. The separately
reviewed profile proposal is excluded from this integration; no numeric choice,
profile write or physical authorization is supplied here.
