# Finite Rust motor control contract

This batch extends `lamp-motor` with a concrete serialized control core. It
encodes and executes STS3215 read transactions and five-joint position broadcasts
through one owned nonblocking `BusPort`. The existing Linux inspection executable
remains read-only. An explicit Linux motion adapter now reuses the same serial
owner and exposes only the guarded control API. The opt-in
[motor worker](motor-worker.md) adds a supervised executable and IPC boundary.
There is still no automatic gesture or device-qualified motion configuration.
No serial device was opened for this work.

The later [V1-based clearance/stop batch](motor-clearance-stop.md) requires
combined-pose provisioning for motion and adds a bounded measured-position hold
after inhibition. The original wire, calibration and per-joint limits below
remain in force; inhibition and stationary readback are distinct receipts.

The path is an explicitly accepted movement segment → original interaction
permit and placement checks → bounded serialized writes → checked five-joint
readback. Cancellation takes priority before the next tick. The local dispatch
target is **less than 20 ms from accepted intent to the first host-accepted command**,
with cancellation processed on the next priority tick (at most 20 ms). The
intended actor should tick every 2 ms. Tests
exercise that policy with a synthetic bus and clock. They do not measure USB
delivery, actuator stopping, optical posture, holding torque, or cabinet safety.

## Reuse and deliberate changes

The pinned V1 source is commit `d5efe9d7b73cc529b34cd4abe97624682a82ca94`:

- `hal/follower/hal_follower.py:258` uses position-only `sync_write`; retain that
  narrow command rather than inventing goal-time or acceleration writes.
- `hal/drivers/motors/animation_service.py:209–229` identifies register 46 and
  warns that writing zero lifts the speed cap. Require explicit nonzero raw
  speed and acceleration settings already qualified for this unit; read and
  compare them, without rewriting either setting.
- `hal/drivers/motors/overload.py:6–12` masks load magnitude with `03FF`.
  Preserve the direction-bit distinction. Unknown upper bits, nonzero status,
  excessive load, velocity or tracking error are explicit faults.
- `hal/drivers/motors/animation_service.py:1153–1185` blocks goals before its
  overload action. Preserve atomic local inhibition. Do **not** copy automatic
  retry or torque release into an unqualified load-bearing posture.

The packet framing, little-endian words, five IDs, register map and calibration
semantics reuse the local SDK audit in [hardware-contract.md](hardware-contract.md)
and the existing tested Rust parser. `scservo_sdk/protocol_packet_handler.py`
was audited with SHA256
`9932c85b9e2ac671a7e33f39e8e80b05d6592280d9f2bcebd85103845e36e655`;
`lerobot/motors/feetech/tables.py` with SHA256
`3030a82ee3e4775993792a22f4ed4385c798fa3e5385049a77a5c17196445fc8`.
These are source provenance, not firmware or device observations.

## Public integration boundary

`control::MotorControl<P, C>` owns one `BusPort`, one monotonic `Clock`, one
`BoundaryGuard`, one transaction, and at most one segment. It starts idle and
performs no I/O until requested. Its calibration must match the explicit expected
unit label. It also requires the supervisor's fresh worker boot ID; occurrence
IDs include that boot, the original turn owner, and a checked local sequence.
An old worker's cancellation cannot target a new worker's same-turn segment.
There is no default device, calibration, envelope, or motion.

| Call | Result and requirement |
|---|---|
| `install(snapshot)` | Uses the shared interaction guard. Old/duplicate state is rejected without replacing newer authority. Malformed trusted state, privacy or active-owner revocation inhibits local work. |
| `qualify(evidence)` | Requires the exact calibration and a nonzero qualification record with current validity. The record covers five explicit envelopes and raw motion profiles. It is a trusted provisioner claim, not authenticated physical proof. |
| `inspect()` | Observes 20 ms quiet, then serially reads all five models, position limits, homing/mode, raw motion profiles, and telemetry. No probe fallback or retry. |
| `start_segment(permit, placement, target, duration_us)` | Requires completed current inspection, current original `Motion` permit, matching original camera grant, a fresh placement frame identity/time bound to the qualification record, and five stationary valid readbacks. A second segment is rejected as busy. |
| `tick()` | Performs bounded work and reports progress, inspection completion, host position acceptance, or checked endpoint readback. There are no sleeps, background threads, or hidden queue. |
| `cancel(motion_id)` | Cancels only the matching active occurrence; an old occurrence cannot cancel a successor. Inhibits ordinary commands before the provisioned measured-stop attempt. |
| `control_lost()` / `shutdown()` | Terminal ordinary-motion inhibition; these calls alone do not establish physical rest. |
| `physical_stop_tick()` / `physical_stop_report()` | Poll the bounded stop sequence after inhibition; report accepted bytes and fresh stationary readback separately. |
| `abandon_physical_stop()` | End an unfinished attempt without new motion, flush once and retain failure. Used by early cleanup and Drop. |

The opt-in worker installs priority authority/control before each tick, owns
the serial endpoint exclusively, and supplies fresh OS monotonic time in the same
domain as the interaction controller. Port calls must be nonblocking, with no
internal retry or waiting. The core observes elapsed call time and faults on an
overrun; it cannot preempt a blocking syscall or an incorrectly implemented port.
The concrete `linux::motion::LinuxMotor` adapter provides that port lifecycle;
process supervision and physical qualification remain required.

## Exact wire and bounds

`command::PositionFrame` encodes only register 42, two little-endian encoder
bytes per joint, IDs 1–5, instruction `83`, broadcast ID `FE`:

```text
FF FF FE 13 83 2A 02 01 LO HI 02 LO HI 03 LO HI 04 LO HI 05 LO HI CHECKSUM
```

The frame is **23 bytes**. `13` is the hexadecimal length byte (19 decimal).
The checksum is the complement of the wrapping sum from ID through the final
data byte. This pure codec is not authority and must not be used as an alternate
runtime writer. It changes no torque, speed, acceleration, goal time, homing,
mode, limit, EEPROM or other register.

Inspection uses five separate all-five sync reads: `(3,2)`, `(9,4)`, `(31,3)`,
`(40,8)`, `(56,15)`. Model 777, position mode 0 and the explicitly bound
EEPROM limits/homing must match. Shared software normalization ranges/sign remain
separate; no offset is applied twice. Every endpoint, generated command and
readback must fit both the software qualification and independent EEPROM bounds. In the profile read, torque-enable is byte 0, acceleration byte 1, goal time
bytes 4–5, and speed bytes 6–7; the unrelated current goal position is ignored.
Admission requires torque-enable exactly 1; zero or any unknown value latches a
fault. It never enables torque. These settings are read once per explicit
inspection; telemetry is refreshed during the segment. Exclusive ownership must
prevent any other process from changing configuration between those operations. Goal-time semantics
remain unqualified: compare its explicit qualified raw value exactly. A speed
of zero or a sign-bit-set speed cannot qualify.

Each transaction has a caller-configured fault deadline at most 200 ms and at
most 512 operations. Each tick performs at most one 128-byte read plus one
23-byte write, and never pipelines requests. It checks for unexpected prior
traffic before every partial write. Partial input is kept in the existing fixed
250-byte parser. The controller uses fixed arrays; ingest/tick does not allocate.
Failed, partial, duplicate, corrupt, noisy, missing-ID and overdue transactions
are terminal. It never resets a parser and calls that a fresh reply.

The serial protocol has no transaction ID or echoed register address. Quiet
time, exclusive ownership and deadlines cannot prove that a same-ID/same-length
late reply is fresh. Read completion is a host timestamp, not acquisition time.
Sync write has no individual servo ACK. `PositionAccepted` means host acceptance
only. `Reached` means five checked stationary host readbacks within the explicit
endpoint tolerance, not independently verified physical pose or clearance.

## Units, trajectories and failure behavior

Calibrated positions use `RANGE_M100_100`: dimensionless normalized units, never
degrees. Existing per-unit software sign and hardware-adjusted homing semantics
remain unchanged. Each joint envelope explicitly supplies position bounds,
normalized units/s, normalized units/s², raw load and measured-velocity limits,
tracking/endpoint tolerances, and the qualified raw profile. No inherited span or
test-fixture limit is presented as a qualified safe range.
The qualification record must cover combined five-joint poses and swept paths
inside that envelope, not merely five independent range measurements. This
module has no kinematic collision or tipping model; per-joint numeric checks
alone cannot establish whole-body safety.

A segment is cubic smoothstep, rest to rest, lasting 1–200 ms. Duration must fit
the **original** permit, placement, qualification and readback budget. There is
no automatic extension or permit renewal. Continuous peak velocity
`1.5 * distance / duration` and acceleration `6 * distance / duration²` are
checked. Encoded goal steps are additionally checked against actual host-acceptance
intervals for range, finite-difference velocity and acceleration. These describe
generated goals, not measured actuator acceleration.

Encoder quantization is checked against the envelope. The endpoint is encoded
once and retained; converting it through normalized units again can lose another
count. A regression reproduced `[656; 5]` becoming `[655; 5]` before this fix.
Intermediate goals are followed by telemetry; the final goal must also settle
within the explicit deadline and tolerance. Successful writes alone never finish
a segment. Excess load immediately inhibits future goals; no threshold, delay,
confidence score, or automatic fault recovery is invented.

Nonzero device-status flags are faults even during unqualified read-only
inspection. A fault discovered when admitting a segment also latches inhibition;
replacing a qualification record cannot heal a cached overload or profile fault.

All terminal paths clear retained motion and authority before one nonblocking
host-output discard attempt. Failure or overrun of that cleanup remains visible
in `StopReport`. No torque command or park movement is sent. Host-accepted
request bytes are counted even if a complete position packet was accepted just
before an I/O overrun was detected. Zero partial bytes does not mean no packet
was accepted. Already transmitted bytes and the last accepted servo goal cannot
be recalled by this module.
**Local cancellation does not establish physical stopping or safe holding.**
An aborted bus transaction requires separate recovery review; this controller
instance cannot restart or replay it.

## Remaining integration and qualification

This is a finite control core and Linux port, not the completed motion runtime.
A supervised actor and priority IPC, ongoing idle-state monitoring, provisioned
per-unit qualification,
fresh placement-review production, and a qualified physical fault/stop/holding
policy are still missing. Multi-segment motions and permit renewal need their own
reviewed contract. The existing read-only inspector must stay read-only.

Do not enable real motion using synthetic fixtures. On-device work must establish
identity, calibration sign/zero, actual raw speed/acceleration behavior, pose and
edge clearance, tracking/load limits, bus contention, callback/tick timing, and
what happens after cancellation, process exit, USB loss, and servo faults. No
room, cabinet, posture, or actuator safety is claimed by host tests.

The only added dependency is the sibling standalone Rust crate `lamp-interaction`.
There is no Python/Go runtime, new external crate, driver I/O in tests, or network
dependency. Validation commands are `cargo fmt -p lamp-motor -- --check`,
`cargo test --locked --offline -p lamp-motor`, and
`cargo clippy --locked --offline -p lamp-motor --all-targets -- -D warnings`.

## Opt-in Linux writer boundary

`linux::motion::LinuxMotor<C>` owns the existing `Owner` transport and the control
core. `open` requires an explicit absolute device path, unit label, private lock
directory, provisioned device identity, controller/worker boots, calibration,
timing and caller-supplied OS monotonic clock. The expected inode/filesystem/rdev
is checked **before** tty open and checked again against the opened descriptor.
A changed USB/device-node incarnation requires fresh provisioning. Opening sends
no protocol packets and starts idle; `inspect`, `qualify` and a valid motion permit
are separate explicit operations. There is no activation through the read-only CLI.

The adapter reuses `rustix` 1.1.5 for nonblocking Linux I/O and termios and the
existing `nix` close path; no new serial implementation or external dependency
was added. The pinned SDK provides framing/register semantics; a fixture generated
by its `syncWriteTxOnly` against an inert memory port is included in the Rust tests.
There is no runtime Python/SDK import. This is an offline source audit, not a claim
that those package versions are currently the latest upstream release.

Inspector and writer use the same `motor-MAJOR-MINOR.lock`, an exclusive
nonblocking flock acquired before open, then fd flock and `TIOCEXCL`. The fd and
owner are private and non-cloneable. No public method exposes raw writes or fds.
All goals go through `MotorControl`, whose original Motion permit, placement,
qualification, telemetry age, range and derivatives are checked immediately
before every partial write. A rejected old cancellation never restores a pose.
A gesture cannot queue control behind a sequence: the API stores one finite
segment, one transaction and no command queue.

`TIOCEXCL` does **not** evict pre-existing handles and privileged processes can
bypass it. The supervisor must stop other owners and prevent privileged device
access for the trial/runtime. The API rejects a worker holding `CAP_SYS_ADMIN`,
but cannot constrain other processes. Neither the lock test nor `OpenReport`
claims that foreign handles were excluded on a real Lamp.

`shutdown(self)` first invalidates motion and performs one output discard, then
returns both local `StopReport` and serial `CleanupReport`. It restores prior
termios with HUPCL intentionally cleared, releases exclusivity and closes each
fd once. Drop is best effort only; explicit shutdown is required to retain
cleanup errors. No torque-off, torque-on, park, old-owner restore or retry occurs.
Open/close may still affect an unqualified USB adapter. A host output flush does
not recall transmitted bytes, cancel the last servo goal or prove physical hold.

The focused local dispatch fixture uses a 2 ms tick and 10 ms goal interval and
requires the first complete host write below 20 ms. Other caller-supplied timing
or partial I/O may fault or miss that target; no hard kernel deadline is claimed.
Linux inert-file tests verify common device locks conflict and release across
explicit cleanup/Drop; they never open a serial device. Numerical, simulated and
ARM/QEMU tests do not establish actuator speed, stopping or placement safety.

Explicit [selected-joint segments](motor-selected-joints.md) preserve the same authority and whole-mechanism readback gates while omitting every unselected ID from position packets. They do not provide a qualified gesture or automatic movement policy.
