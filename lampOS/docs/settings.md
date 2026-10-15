# Runtime settings

Ready for lamp-voice: no

This is the unfinished #699 checkpoint, paused on 2026-10-15 when the
Queue assigned Third the microphone privacy switch (#755) and a fallback
voice proposal (#754). It has not passed its gate and is not offered for
installation. No existing component reads this service yet.

## What exists and what we take

[ROS 2 parameters](https://docs.ros.org/en/rolling/Concepts/Basic/About-Parameters.html)
provide declared names, types, descriptions and ranges, owned by a node.
Keep those contracts and an atomic batch operation. Reject duplicate names
instead of silently choosing the last value. Validation must have no side
effects. ROS runtime updates and saving a parameter file are separate;
Settings instead acknowledges a mutation only after durable persistence.
ROS parameter events are currently reliable, keep-last 1000 and volatile,
as the [rmw defaults](https://github.com/ros2/rmw/blob/rolling/rmw/include/rmw/qos_profiles.h)
show. A new reader needs the authoritative snapshot, not just events.

V1's `system/server/config/config.go` serializes mutations and notifies
readers after writing, but mutates memory before a direct `WriteFile`.
Keep serialization, change the durability ordering. Records already owns
bounded, descriptor-relative JSON reads and atomic file-sync, rename,
directory-sync writes. Settings reuses those operations and adds a private
lifetime lock, strict caller-owned paths, revision guards and retry receipts.
A storage error can follow rename; it is an indeterminate result, not a
rollback guarantee. Recovery never silently replaces corrupt state with
defaults. No new database, async framework or configuration dependency is
needed for this small document.

## Current implementation

`crates/settings` contains a library and `lamp-settings`. `serve` without
`--enable` returns a disabled result without files, sockets, workers or Bus
activity. Enabled startup requires an existing private store directory and
an explicit declaration file. It validates recovery before serving.

The catalog declares one to sixteen names, each `owner.key`, with a default,
description, unit, read-only flag and fixed kind: Boolean, integer, finite
number, or bounded text with optional choices. Values retain their kinds;
an integer is not silently coerced to a floating-point setting.

`read` returns the catalog, current desired values, store identity, revision,
pending-write state and reliability. `set` and atomic `apply` require the
previously read store ID and revision and a caller-generated request ID.
One worker validates and saves mutations in order. The socket loop serves
reads from the last confirmed snapshot while storage waits. An uncertain
save poisons the owner; its old in-memory snapshot is returned only beside
`reliable:false`, and further mutations require restart and recovery.

There are eight client slots and four queued mutations, 8192 request bytes,
a 300 ms incomplete-input/write deadline and a 1100 ms mutation response
deadline. Expiry before persistence begins refuses the queued operation.
After persistence begins, timeout cannot cancel it; a lost response must be
reconciled with the same request ID and guard. A bounded shutdown reports
whether its storage owner and health reporter actually stopped.

Optional `--bus-root` reports `settings` on `diagnostics` from its own worker.
It publishes operational counts, not values. Slow storage and failed recovery
are distinct from successful reads. No Bus message applies a setting.

Success means **saved desired values**, not that Ears, the light or Records
adopted them. Their owner integrations must preserve their own safety,
privacy, arbitration and reset rules. Credentials, provider generations,
physical privacy authority, motor limits and calibration do not belong here.

## The first three useful settings

`lamp-settings catalog` prints proposals based on current reader defaults.
They are not connected to those readers by this checkpoint.

| Name | Declared default and limits | Prospective reader and application boundary |
|---|---|---|
| `ears.rise_db` | 30 dB above learned room floor; 10–60 | Ears' `Listener`, currently startup-only. Its owner must define reset/retained-floor behavior and preserve own-speech suppression and absolute floor. |
| `light.level` | Integer 1; 1–5 | Voice RingLight and Expression already read `Light`. Future integration must arbitrate the complete on/colour/level tuple with Actions and preserve privacy. |
| `records.voice_max_age_seconds` | 1,209,600 s (14 days); 1–31,622,400 | Records reads its root config at cleanup start. Adoption affects the next authorized cleanup; it must never itself enable removal. |

V1's raw sound threshold 8000 cannot be migrated numerically into Ears'
relative decibels. Speaker volume is not presented as applied: lampOS Audio
does not own mixer volume, so it needs a separately assigned reader.

## Verification and work still required

Third's Intel development Mac is the only machine used for this checkpoint.
The first `cargo test --offline -p lamp-settings --no-run` failed on a
Diagnostic values type mismatch and a nonportable FIFO test call. Those
edits were corrected, and the three-package test compilation subsequently
passed; no test was executed. [Raw checkpoint evidence](verification/shared-settings-checkpoint-20261015/README.md)
retains both runs. This is not a passing full gate.

The intended healthy target is 250 ms from request admission through queueing,
validation, durable save and response. `last_save_ms` currently measures only
owner-start through the engine result, excluding queue and socket time.
Neither target nor a lamp timing result has been measured.

Still required before ready: strict operation-specific client reply validation
(a bare `ok:true` must not count as a commit), canonical client path checks,
complete overload/slow-reader/shutdown/crash coverage, latency measurements,
default-off comparison, all affected-package gates, source-only portability
and an exact-head native ARM64 run. The current root/new code must be reviewed
as a whole; written tests are not claimed as executed. Add the concrete First
lamp runbook after these pass. No service unit, image, device, key or provider
was touched. Hardware adoption and optical/acoustic latency are unverified.
