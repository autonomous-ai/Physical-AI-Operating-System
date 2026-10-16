# Supervised motor worker

`lamp-body-worker` is an opt-in, separate Rust process over the existing
`MotorControl` and `linux::motion::LinuxMotor`. It adds no gesture policy. `lamp-voice` can explicitly opt into an idle worker;
see [live motor integration](live-motor-integration.md). No default invocation launches it. The read-only `lamp-body-inspect` is unchanged.
Opening the explicitly provisioned port starts idle: no probes, goals, park,
torque, speed, acceleration, mode or EEPROM writes occur automatically.

This implementation reuses the [audited motor contract](motor-runtime-contract.md)
and the V1 position-only register-42 broadcast, exact profile readback, calibration,
load mask and inhibition rules recorded there. It replaces neither the protocol
nor the range/velocity rules. Cancellation clears local work and discards host
output; it **cannot recall an accepted servo goal or prove physical stopping**.
There is no automatic torque-off, reopen, retry or recovery gesture.

## Executable and provisioning

The supervisor library launches the dedicated executable with exactly:

```text
lamp-body-worker --config PRIVATE_JSON --directory FRESH_PRIVATE_DIRECTORY \
  --controller CONTROLLER_BOOT_HEX --worker WORKER_BOOT_HEX --parent-pid PID
```

These are process arguments, not an interactive movement command. Linux is
required for real serial ownership. The macOS binary refuses device execution.
Use `runtime::supervisor::Supervisor::spawn(executable, config, fresh_directory,
controller_boot, fresh_worker_boot)` from a trusted Rust supervisor. The directory
must be new and short enough for Unix socket paths. Configuration is a private,
single-link regular file, owned by the caller, at most 16 KiB. It is read without
following a final symlink or blocking on a FIFO. Freeze it before spawning.

`runtime::config::Config` has no defaults. Provision all of:

- Absolute device and trusted private lock directory; observed filesystem device,
  inode, character-device number and major/minor identity.
- Explicit unit label, five software normalization records and five separately
  observed EEPROM `register_binding` records keyed by joint ID. File homing
  values are preserved as provenance; runtime never writes them. Missing
  binding evidence has no fallback. See [register binding](motor-register-binding.md).
- Nonzero qualification-record identity, original monotonic validity interval,
  and five qualified normalized ranges, velocity/acceleration bounds, exact raw
  profiles, load/speed caps, tracking error and endpoint tolerance.
- Existing `Timing` limits, expected placement-source worker boot, and a finite
  worker lifetime (at most 630 seconds).

A qualification record is a trusted provisioner's assertion, not mechanical
proof. Root must verify actual placement, clearance, combined-joint paths,
calibration, adapter open/close effects and existing foreign handles before
allowing an instance. The existing private per-device lock, tty flock/TIOCEXCL,
identity checks, 1 Mbaud settings and CAP_SYS_ADMIN restriction are retained.
A new worker must have a fresh boot; a new placement source requires explicit
reprovisioning. No source file supplies real device defaults or qualification.

## Control, commands and evidence

Two connected private Unix datagram channels have independent finite kernel
queues and an 8192-byte payload limit. Serialization uses a fixed output buffer.
Every envelope retains expected peer boot, checked sequence and original shared
host-monotonic send time. Future, reordered and older-than-100-ms packets fault.
A packet's age is measured on a clock read after the packet is dequeued, and
the worker reads its time for an accepted control after that read; a time read
before it would refuse a packet sent in between as from the future.
There is no reconnect/rebind or unbounded application queue.

Priority control accepts only `Authority { snapshot }`, `Cancel { motion }` and
`Stop {}`. Data accepts `Inspect`, `Segment` and explicit `SelectedSegment`, each with a strictly increasing
request ID and original request timestamp. Unknown fields and variants fail
closed. The worker handles at most 16 controls and one data command per tick,
checking control again after receiving the command and immediately before a core
tick. Exhausting the control slice defers bus work. At most one original command
can be held; it is never restamped or used to renew authority.

Only fresh control snapshots grant authority. Both physical microphone and camera
permissions must remain allowed, including during open/inspection. Old snapshots
cannot override newer state; conflicting revisions and newer expired revocations
fault. Missing initial authority expires after 250 ms. Later idle/active authority
expires with the actual snapshot, independently of the progress heartbeat. A
hardware-open callback consumes priority controls between the existing bounded
setup operations. Stop before open performs no serial action.

After `Ready` (exclusive open completed, still idle), `Inspect` requests the core's
20-ms quiet interval and five exact all-joint register transactions. A `Segment`
requires that completed inspection, original `OutputPermit`, original placement
camera grant, configured placement-worker boot, a new observed frame sequence,
matching qualification record and five valid targets. Segment duration, profile,
range, velocity, health, freshness and endpoint checks run in `MotorControl`.
An active segment is never replaced or queued behind another. Each bus tick still
performs at most one 128-byte read and one position/request write of at most
23 bytes. Partial writes/readbacks preserve the existing accounting and parser
failure rules. No arbitrary-byte/register API is exposed by this runtime.

Receipts retain original worker/turn owner/local motion sequence:

- `Started` acknowledges local segment acceptance, with its command request ID.
- `HostAccepted` contains the emitted selection, counts and host-acceptance time. Counts for omitted joints are retained readbacks, not transmitted goals. It is
  not a servo broadcast acknowledgement or physical-arrival measurement.
- `ReadbackReached` reports the core's five stationary, in-tolerance readbacks.
  It does not establish optical arrival, collision clearance or force.
- `Rejected`/`CancelRejected` preserve the rejected identity. A stale cancel does
  not stop a successor. A matching cancel, privacy/ownership loss, bus failure,
  malformed control or expired authority terminally inhibits this bus instance.
- `Inhibited.at_us` is the publication observation used for receipt ordering.
  `Inhibited.report.stopped_at_us` remains the original core stop/fault time,
  even when an admission rejection was published between the fault and inhibition
  receipt. The supervisor requires spawn ≤ original stop ≤ publication ≤ receive;
  it does not rewrite the original measurement to make receipt times monotonic.
  The report preserves accepted/partial byte counts, original stop reason and
  any output-discard failure. `Stopped` separately records serial restoration
  and close; cleanup errors remain errors. Neither asserts physical halt.

Feedback backpressure is a fault, not a reason to keep moving without receipts.
Local inhibition precedes receipt attempts; failed feedback cannot revive work.
Any failed priority send from the supervisor, including Authority or Cancel,
immediately latches failure, removes readiness and command admission, and signals
only its owned child. It does not wait for the progress watchdog or retry the
revocation through a full queue. The supervisor remains owned until observed reap.
A successfully delivered stale cancellation remains a harmless rejected identity.

## Supervision and timing

`Supervisor::poll` is nonblocking. It checks deadlines and `Child::try_wait` even
when receipt parsing fails. It never waits for a camera, provider, audio worker or
another command. Call it from the normal bounded coordinator tick; do not put
startup or a blocking final wait in the audio hot path.

- Startup has a 3-second parent deadline. Both peer socket files must become
  same-user private sockets before connecting; the short bind-to-chmod interval
  is observed without accepting broad permissions.
- The worker loop sleeps 2 ms between bounded steps and emits progress at least
  every 10 ms while scheduled. The parent faults after 50 ms without source-timed
  progress. This is a watchdog, not an authority extension.
- Stop expects local inhibition acknowledgement within 20 ms, then at most
  250 ms for driver teardown/exit. If a syscall hangs, the parent signals only its
  owned child and polls reaping for a further 250 ms. Forced termination or missed
  cleanup remains failure. An unreaped child remains owned and cannot be treated
  as released. `clean_exit()` requires inhibition, close evidence, successful
  exit, and no timeout/forced termination.
- Explicit `poll`/reap is mandatory. Dropping a supervisor only issues best-effort
  kill and one nonblocking reap attempt; Drop is not successful shutdown evidence.
  `replace_after_reap` rejects a live/unreaped predecessor, incomplete peer cleanup,
  or a reused worker boot or directory, then creates an idle child with no copied
  authority or commands. The existing bus lock remains the second line of exclusion.
- The supervisor records the fresh private directory and each child's socket
  device/inode/UID, including partial startup. After observing the exact owned
  child's reap, it unlinks only the two matching socket paths. A changed directory,
  symlink, file or replacement socket is retained and reported through
  `ipc_cleanup_error()` separately from the original motor failure. Cleanup never
  recursively deletes a directory, and a failed identity check is not retried by
  adopting new paths. Parent endpoints keep their existing identity-checked Drop;
  the original directory is removed only if still matching and empty. This assumes
  trusted same-user code obeys exclusive ownership of the private namespace;
  filesystem metadata checks are not an atomic defense against hostile same-user
  path replacement during the check/unlink interval. Drop without observed reap
  does not clean child paths or establish release.
- Linux also arms a parent-death SIGKILL with before/after parent-PID checks.
  This prevents normal parent loss from leaving a motor process running, including
  during a stuck bus call. SIGKILL does not restore settings or cancel physical
  servo goals. Linux SIGTERM/SIGINT take the normal local Stop path via signalfd.

The software targets remain first host-accepted position in under 20 ms after a
valid segment request and priority cancellation on the next tick (at most 20 ms).
Host scheduling, kernel driver calls, bus transmission and mechanical arrival are
separate. A process supervisor cannot guarantee prompt physical stopping.

## Opt-in live integration

See [the live integration contract](live-motor-integration.md) for explicit CLI
provisioning, pre-capture startup, original movement offers, optional degradation,
and separate verified privacy release. This adds no default head nod, body
motion, room scan or failed-worker restart.

## Verification boundaries

Tests exercise the real datagram codec/queues and unchanged control core with a
fake STS3215 wire, plus actual inert child processes using the production worker
loop. They cover idle startup, explicit inspection, partial read/write, cancel
across the control/data boundary, retained command expiry, duplicate/busy requests,
privacy and input-end revocation, stale cancel, bad profile/reply, bus loss,
feedback congestion, missing parent, crash, malformed receipts, hung child,
cleanup evidence and replacement only after observed reap. Lifecycle regressions
also reproduce failed priority delivery with a full real queue, preserve an earlier
core fault time through the real supervisor receipt validator, and cover partial
bind/crash, owned kill, replaced sockets/directories and a still-active successor.
Test fixtures use short `/tmp` paths on macOS and Linux; native Linux execution is
still a separate gate. The fake wire asserts
that writes are only the existing read requests or register-42 position packets.

The first-host-write timing assertion uses real IPC with a virtual monotonic clock
and 2-ms ticks. The inert subprocess timing sample is host IPC → local inhibition
only, with no serial device and no movement. Neither establishes physical latency
or safety. No serial port, Lamp or cloud was accessed for this implementation. Root also
qualified this exact composed source with Linux ARM64 compilation and QEMU tests
in the reserved build VM: format, strict all-target motor Clippy, 134 tests
passed with zero failed or ignored, and a release worker build. All 344 source
files were verified before and after the gates. Host qualification passed
129 tests; an independent review passed 40 focused test executions. These counts
are separate runs, not independent unique cases. Real adapter and servo
qualification remains required before enabling the worker on a Lamp.

The compiled source manifest SHA-256 is
`524e059c83e5c37ad251b9cfddfd97bd34cced24a274697ae1d271bb5530f9c2`.
The extracted AArch64 worker SHA-256 is
`a1377c0f94f8eb9033d728bba5b78740faca3870ca4c8ae63773dedc106172d2`.
ELF inspection and inert QEMU CLI usage passed without opening hardware. Exact
commands, logs and extraction receipts are retained locally under
`/private/tmp/lamp-motor-composed-_8riuiv1/arm/evidence/`. The worker is integrated
as an opt-in component; default `lamp-voice` invocations do not start it. No on-device motion or
physical stop latency has been measured for this worker.

The additive `SelectedSegment` command is described in the [selected-joint contract](motor-selected-joints.md). Existing `Segment` retains all-five behavior; host receipts identify the emitted selection.
