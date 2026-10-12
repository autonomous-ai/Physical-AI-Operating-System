# Optional motor integration: source qualification

The 38-file composition on `7bfe408a3` connects the existing separate motor
worker to `lamp-live` through explicit paired options. Default runs remain
motionless. It adds selected-joint commands, actual encoder readback receipts
and a finite head-only qualification controller. It does not supply an automatic
gesture producer, physical clearance inference or safe numeric device defaults.

The root checked the patch SHA-256
`03bab2a48fa537751e6b5f8dc5c769ffa095943bcd95b1d369dd4e9775c838f1`
and matched all 445 exported files byte for byte after applying it. Only
documentation changed afterward. The source manifest is
`2be649c7cecc067c4d35d599fb013ad605e03b7b04774011853dd76f81a97f24`.

Host gates passed: formatting, strict `lamp-live` and `lamp-motor` all-target
Clippy, 16 live-motor tests, 9 provider-pair controls and 153 motor tests:
178 passed, zero failed or ignored. The retained receipt and command report
record exact commands, logs and limits. There is no ARM or physical claim
for this combined composition yet.

Independent review reproduced and corrected two controller defects before
integration: a caller could dispatch to a different Supervisor before worker
identity was checked, and a rejected direct command constructor could leave the
sequence reusable. Identity now precedes all sends/polls; rejection inhibits the
sequence. An unrelated worker receives neither a command nor a Stop.

Selected commands serialize only selected IDs while all-joint stationary, health,
profile and calibration checks remain. Readback receipts keep actual encoder
counts and original timestamps. Explicit inspection, outbound segment, second
inspection and return are four separate actions; no fault, mute or Stop schedules
a return. Accepted serial goals cannot be recalled or claimed physically stopped.

The native inspection found uncapped zero profiles on the device. Selected-profile
preparation is separate work and is not included here. Hardware movement remains
inactive until actual placement, the explicit profile and a bounded native caller
are qualified. All evidence is local; no room recordings or binaries are committed.
