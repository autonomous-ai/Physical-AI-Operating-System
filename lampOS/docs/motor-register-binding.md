# Software normalization and EEPROM binding

V1 main `d5efe9d7b73cc529b34cd4abe97624682a82ca94` calls
`connect(calibrate=False)`. Its `hal/calibration/calibration.md` and
`hal/follower/config_hal_follower.py` describe the normal unset `HAL_DEVICE_ID`
case: shared JSON ranges/sign define `RANGE_M100_100`; each assembled unit's
EEPROM homing remains in its servos. Shared JSON homing is not a hardware reading
and must never be written over a unit's actual offset.

The old Rust comparison treated normalization and EEPROM limits/homing as one
record. That could not faithfully admit the observed 4ace configuration.
`UnitCalibration::with_register_bindings` now takes explicit software records and
five independently keyed `RegisterBindingRecord`s. The private worker config
requires both `calibration` and `register_binding`. No discovery, fallback,
EEPROM write, re-homing or range substitution occurs. The old pure constructor
retains its strict same-values contract for callers that explicitly have that
configuration; production file loading always uses the separate binding.

The actual 2026-10-11 read-only 4ace fixture is:

| ID | Shared software span | Observed EEPROM span | Observed homing | Observed present count |
|---|---|---|---|---|
| 1 | 847..3275 | 832..3268 | -1754 | 2015 |
| 2 | 0..4095 | 0..4095 | 1434 | 2048 |
| 3 | 0..4095 | 0..4095 | 1339 | 2047 |
| 4 | 621..3254 | 823..3057 | -1823 | 2039 |
| 5 | 1464..3071 | 1657..3098 | -1880 | 1765 |

All shared `drive_mode` and JSON `homing_offset` values were zero. Tests retain
these original values and show that normalization uses the shared span without
subtracting EEPROM homing again. Mutated EEPROM evidence, duplicate IDs, invalid
representations and absent bindings remain errors. The control core checks each
endpoint, every emitted count and each readback against independent hardware
bounds as well as the configured normalized envelope. It never clamps a target
into apparent validity.

These observations are a regression fixture, **not a motion qualification**.
They do not supply safe velocity, acceleration, load, contact, joint/cable
clearance, combined-body travel, or a measured mapping to angles. A unit label
is not authenticated identity. Existing all-joint calls retain all five IDs; the separate [selected-joint contract](motor-selected-joints.md) omits unselected IDs explicitly.

The finite inspector adds only a `(40,8)` sync read to its existing bounded plan.
Report schema 2 contains ten transactions (five pings and five all-joint reads),
at most five replies each. Torque, acceleration, current goal position, goal
time and goal speed remain raw. The same decoder is reused by motion inspection.
The 200 ms transaction fault deadline, 2 ms poll slice, cancellation, exclusive
serial ownership, and explicit close remain unchanged. Freshness is still
limited by the protocol's lack of transaction IDs.

Build the standalone Linux inspector from this source with:

```text
cargo build --release --locked --offline -j2 -p lamp-body --bin lamp-body-inspect
```

Root must qualify the Linux binary and hash, exclude every foreign bus handle,
verify exact device identity and trusted private lock directory, and retain the
single finite report and close result. The CLI remains:

```text
lamp-body-inspect --device ABSOLUTE_DEVICE --unit-id EXPLICIT_UNIT \
  --lock-directory PRIVATE_DIRECTORY
```

This proposal itself performs no hardware action. Reading the new profile can
supply missing evidence; it cannot establish a safe gesture by itself. The
inspector's relative host timestamps must not be reused as current controller
or camera authority timestamps.

## Verification of this integration

The frozen binding change passed host format and strict motor all-target Clippy
with 133 motor tests. On Linux ARM64 under emulation, the same source passed
format, strict all-target Clippy, 138 motor tests (zero failed or ignored), and
a release inspector build. No tests were skipped in the motor command. The
exported inspector SHA-256 is
`d2de9aae3864e9f3483ee94d521f59be5328e8084eb40d880a78b4409eee70a6`.
These gates do not qualify motion or measure on-device latency. The new profile
read has not yet run on a device at this checkpoint.
