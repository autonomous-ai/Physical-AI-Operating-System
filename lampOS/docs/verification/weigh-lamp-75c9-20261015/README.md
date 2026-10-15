# Weighing and measuring lamp-75c9 (#704)

How lamp-75c9 was weighed and measured for the lean check
([lean.md](../../lean.md), "Measurements the first builder needs"), and the
readings as taken, unedited. Written by the eighth builder before the session
as the plan, then filled in at the desk. Nothing here is a reading until a
row in [readings.md](readings.md) says it was taken.

## Instruments

| Instrument | Resolution | Range | Checked against |
|---|---|---|---|
| Kitchen scale | _to fill_ | _to fill_ | _a known weight, if any_ |
| Ruler / tape | _to fill_ | | |
| Caliper | _to fill_ | | |

## Names used here

The five moving parts, in the order of the chain, with the joint that moves
each and the joint at its far end. The model's names are in `crates/lean`
(`data.rs`); the first software's are in `robots/lamp/urdf/lamp.urdf`.

| Part | Moved by (servo) | Far joint | First software's link |
|---|---|---|---|
| Base (fixed) | | base yaw (1) | `base` |
| Swivel | base yaw (1) | shoulder (2) | `swivel` |
| Lower arm | shoulder (2) | elbow (3) | `lower_arm` |
| Upper arm | elbow (3) | head pitch (5) | `upper_arm` |
| Neck | head pitch (5) | head roll (4) | `neck` |
| Head | head roll (4) | none | `head` |

A motor's case belongs to the part it is bolted to; its horn and what the
horn drives belong to the next part. A cable is counted once, with the part
it is tied to; where it hangs free between parts, half to each, and that is
written down.

## Order of the session

Nothing below is done with power on except steps 3 and 9, and the lamp is
never balanced near tipping. The person's hands do every step; the agent
says what to do and writes what is read.

1. **Whole lamp, as it stands on the desk.** Power cable unplugged. Three
   weighings, lifting it off and back each time; then three more with the
   power cable held up so the scale carries none of it, and the cable alone.
2. **The base where it touches the desk.** On a sheet of paper, the lamp at
   rest: trace every pad that touches, mark the centre of the base's turning
   axis (the yaw servo's shaft) and the direction the arm points at yaw
   centre (x forward). Measure each pad's outermost points from that centre
   in mm. Press on each pad to see that it really touches under load. Note
   any rocking, soft feet, and the desk's slope if a level is at hand.
3. **The reference pose and its counts** (needs the lamp, ssh, the body
   stopped; done when the lamp is free). A pose that can be found again by
   eye or with a square: arm straight up, head level, facing x. The five
   counts are read from the servos at that pose; this gives each joint's
   `angle_zero_rad` and the sign of each axis (one small push by hand on
   each joint, reading which way the count goes).
4. **Joint-to-joint lengths, assembled.** At the reference pose, with a
   ruler and the caliper: the height of the yaw shaft's top above the desk,
   then from each joint's axis centre to the next, along the arm and across
   it. These are compared with the first software's drawing
   (`lean-first-software.json`); the drawing is kept where they agree
   within the reading.
5. **Reactions before taking it apart** (validation, lean.md step 7). The
   lamp on a stiff board resting on two narrow supports `L` apart, one
   support on the scale; board alone first. Folded, and standing if the
   gears hold it unpowered (else in step 9). Then turned 90 degrees on the
   board for the other horizontal axis. This gives the whole lamp's centre
   of mass over the base in those poses, to check the model against.
6. **Taking it apart, from the head down.** Photograph each joint first
   (lamp only), and note which cables go where. Head off at head roll; neck
   off at head pitch; upper arm off at the elbow; lower arm off at the
   shoulder; swivel off the base, if it comes off without harm. Stop at any
   joint that does not come apart cleanly; the parts above it are then
   weighed together and that is written down.
7. **Each part weighed** three times, alone on the scale, nothing else
   carrying it. Then the base. The sum is compared with step 1.
8. **Each part's centre of mass.** On two narrow supports (two pencils, or
   the edges of two books) of the same height, one on the scale: first with
   the supports under the part's two joint axes, so the scale's reading `m_B`
   and the part's mass `m` give the centre along the line from near axis to
   far axis, `L * m_B / m`; then turned, across that line, and lying on its
   side for the third direction. The head, which has no far joint, uses its
   joint axis and the front of the shade.
9. **Put back together**, the reference pose found again and its counts read
   again to see that nothing moved (needs the lamp). Then with power: the
   lean check at rest and standing with the lamp's own readings, and one
   pose near the predicted edge, slowly, with a hand ready and nothing
   allowed to fall.

## What was not shown

Filled in after the session.
