# Attended native head qualification

This opt-in finite caller uses the live interaction owner, physical privacy
worker, current capture/admission leases, validated camera metadata, and the
existing single motor Supervisor. Ordinary chat remains stationary: it creates
no operator channel, retains no qualification frame, and sends no setup or head
action. This is preparation for an attended hardware experiment, not physical
qualification, collision inference, an automatic gesture producer, or permission
to move based on a model's words.

The trusted provisioner must supply an explicit `Plan` JSON matching
`lamp_voice::head_qualification::Plan`. It includes a complete numeric `HeadPlan`,
SHA-256 of the original private motor and camera configurations, the original
motor `placement_source`, a nonzero placement-review SHA-256, the actual Linux
kernel boot ID, and current-boot monotonic review/expiry times. No numeric pose,
profile, time, or clearance default exists. A fresh operator photo/review must
establish placement, support, cable clearance, bounded raw-count envelope,
initial all-five counts, and selected ID5 profile. The runtime checks record and
configuration identity, not the visual content or truth of that review.

Before any worker spawn, preparation assigns the camera's actual new boot ID,
verifies the pinned original motor configuration, and writes a private derived
configuration with **only** `placement_source` changed to that boot. Both hashes,
the old/new source, qualification record, and review hash are traced. The source
configuration is not changed. Frozen config/binary cleanup retains the existing
inode and ownership protections. Camera metadata goes through the existing
inspector's worker, USB identity, negotiated mode, epoch, sequence, original
grant, driver sequence and timestamp checks. Only one latest frame is retained;
the inspector retains at most its existing 64 metadata samples. No image is
saved by this feature. A fresh dequeue is not a measured optical acquisition time
or proof of obstacle clearance.

## Explicit actions and authority

Create a new owned mode-0700 channel directory. Start the same-host Rust
`head_qualification_operator` example first; it binds `operator.sock`. Start a
finite `lamp-voice directed` session with all existing explicit settings plus:

```text
--camera-config ABS_PRIVATE_CAMERA_CONFIG
--motor-worker ABS_MOTOR_WORKER --motor-config ABS_PRIVATE_MOTOR_CONFIG
--head-qualification-plan ABS_PRIVATE_PLAN
--head-qualification-channel ABS_PRIVATE_CHANNEL_DIR
```

The paired head flags require camera and motor options. `serve` rejects them.
The operator example accepts either the exact four-action sequence:

```text
head_qualification_operator ABS_PRIVATE_CHANNEL_DIR NEW_RECEIPTS.jsonl inspect outbound reinspect return
```

or, only when `selected_profiles.prepare_from` is provisioned, the exact
six-action sequence:

```text
head_qualification_operator ABS_PRIVATE_CHANNEL_DIR NEW_RECEIPTS.jsonl inspect_profile prepare_profile inspect outbound reinspect return
```

Every action is explicitly named in argv; omitting actions does not select a
default. This consent schedules the named finite sequence after observed phase
completion. It does not synthesize a conversational owner or camera observation.
A person must cause a normal admitted real turn, under current microphone and
camera permission. After connecting, the operator waits for a first-live-slice control announcement
before sending timed status requests; the pre-spawn socket alone is not service
readiness. The announcement carries no ticket or input/movement authority.
Initial privacy acquisition withholds tickets; a later privacy revoke inhibits.
The operator waits for a runtime receipt and refuses another
session/owner, expired receipt, unexpected phase, stale/duplicate reply or an
uncertain send. Status is not listening readiness and never opens capture.
There is no prompt, speech, model, network, raw serial or torque command in this
operator. The runtime reads at most two bounded datagrams per normal live slice.

The receipt holds the original frame and current owner, expires within 100 ms,
and can be consumed once. Status with no owner, no input retention, or absent or
stale frame withholds it. Action dispatch rechecks original grant/owner/phase and
physical privacy freshness, then asks the actual Controller for a Motion permit.
The permit remains clipped to its real capture and admission deadlines. The
live capture lease is currently 100 ms: a requested segment that cannot finish
inside the remaining permit is rejected, not extended. Playback presentation
changes, owner cancellation, privacy denial, expiry or failed evidence stop the
optional motor path; they do not cancel healthy voice. A new camera frame is
required for each action. The finite plan deadline starts at the first action.

Profile setup uses request IDs 1 and 2 with exact `ProfileStarted` and
`ProfilePrepared` Motion correlation. It requires initial all-five stationary
readback, explicit provisioned desired raw values, accepted-byte counts, ordered
ACK/readback evidence and unchanged encoder counts. The later HeadQualification
starts at request 3, then owns only its two inspections and two ID5 segments.
Setup does not masquerade as an inspection or reached endpoint. See
[motor-selected-profile.md](motor-selected-profile.md) for raw-unit uncertainty,
partial-write evidence and preparation failure behavior.

## Stop, cleanup and evidence limits

Return is a separately named action. Stop, expired evidence, cancellation,
privacy loss, wrong phase, malformed control or lost reply never schedules it.
A profile change may persist even when its ACK/cleanup is lost; the caller does
not restore it or replay any action. An unavailable operator cannot create a new
permit. After the first action, absence of subsequent commands reaches the plan
deadline and inhibits. With no action, the motor stays idle until the enclosing
finite session ends.

Successful return triggers only normal owned-child Stop and bounded reap, not
another position command. The operator reports success only after `Returned`,
actual reaping and verified existing cleanup. Host command/write/ACK/readback
receipts and exact owner/camera/permit data are distinct from physical arrival,
optical clearance and stopped motion. Original lifecycle evidence stays in the
normal runtime trace; attempted operator actions and replies stay in its new
JSONL file. Missing or malformed evidence is a failed attempt, never a reason to
retry. SIGTERM/session termination still uses the existing live cleanup path.

The operator has separate 30-second connection, control-startup, and observation
bounds, at most 4096 requests; the provisioned HeadPlan limits actual
qualification to at most five seconds. Logging/OS scheduling can make a receipt
expire; this withholds/aborts safely rather than widening its 100 ms authority.
Same-UID IPC is process isolation, not authentication against a malicious peer
running as that user. One worker transaction already accepted by the host may
complete after a revoke; neither local tests nor cleanup receipts prove immediate
physical cutoff.

Host validation uses production Controller, Driver, Supervisor and motor worker
with inert protocol/encoder fixtures, including all six explicit actions and
fault controls. Those tests do not establish target hardware duration, raw-speed
units, mechanical clearance, privacy switch behavior or safe numeric settings.
Native Linux/ARM compilation and a fresh attended placement/profile review remain
required before root executes this helper. No fixture receives production approval.
