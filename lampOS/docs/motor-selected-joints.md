# Selected-joint position commands

This is a protocol and authority correction, not a supplied head gesture or a
physical safety qualification. V1 main `d5efe9d7b73cc529b34cd4abe97624682a82ca94`
uses position-only Feetech sync-write with an explicit set of joint IDs; its
`hal/follower/hal_follower.py` caps relative target changes from current
position. The Rust writer keeps its independently qualified range, velocity,
acceleration, fresh readback and original Motion permit checks. It now also
preserves which joints the caller actually authorizes for transmission.

`JointSelection` is a nonempty subset of IDs 1 through 5; zero and unknown bits
are rejected both by construction and deserialization. `SelectedPositionFrame`
encodes only those IDs, in ascending order, using register 42, two little-endian
position bytes per ID, and the same checksum as the audited protocol. Packet
length is `8 + 3 * selected_count`, at most 23 bytes. No speed, acceleration,
time, homing, torque or arbitrary-register write is added.

`MotorControl::start_selected_segment` and the Linux adapter accept the original
permit, placement, target, duration and selection. Existing `start_segment`
and wire `Segment` explicitly select all five, retaining their old behavior.
The additive worker command `SelectedSegment` carries the validated selection.
The worker's backend must implement selected commands or rejects a subset; it
cannot silently turn a subset into an all-joint write.

All five servos still require current model, EEPROM binding, profile, stationary
readback and a complete qualified envelope. Unselected target slots do not
command movement: their exact current encoder counts are retained internally,
without a normalize/denormalize round trip. Their IDs are absent from every
position packet. Readback checks still observe the whole mechanism. Selection
is not permission to ignore unexpected movement, load or stale data elsewhere.

`HostAccepted.selection` identifies the IDs actually sent. `counts` for omitted
IDs are retained readbacks, not emitted goals. A complete host write is not a
servo ACK or proof of physical movement. Existing original-owner, privacy,
expiry, partial-I/O discard and Stop boundaries remain unchanged. Cancellation
never adds a replacement position, park, torque operation or replay.

The existing local target remains less than 20 ms from accepted command to
first bounded host write in the inert harness. The deterministic worker test
measures this virtual-clock boundary only; kernel stalls and physical travel
remain unmeasured. Real use needs root's separate native compilation, exclusive
serial ownership, fresh exact configuration/profile binding, current placement
and a qualified head-only range. This change supplies none of those facts and
creates no automatic listening, response or idle gesture.
