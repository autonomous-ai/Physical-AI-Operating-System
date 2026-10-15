# The self-test

`lamp-selftest` ([#812](https://github.com/autonomous-ai/lampOS/issues/812))
checks whether each part of the lamp works, in two minutes, at the factory,
at the first boot after setup (#806) and when asked. For each part it says
pass, fail, not fitted or not checked, with one plain reason. Nothing leaves
the lamp.

## What a run gives

- `lamp/selftest`, type `lamp_msgs/SelfTest` (`crates/msgs/src/selftest.rs`),
  kind latest, published once at the end of a run: the occasion, when it
  started, how long it took, every part's result in the order it ran, and
  the run's verdict.
- `/var/lib/lampOS/selftest/<started>.json` for each run, the newest 20
  kept, with `latest.json` and `latest.txt` (one line a part) beside them.
  Each file is written whole and renamed, so a power cut leaves no half
  report.

A part's verdict is one of:

| Verdict | Means |
|---------|-------|
| `pass` | the part did what it was asked |
| `not_fitted` | this lamp has no such part and is not meant to; not a fault |
| `not_checked` | it could not be checked this time (its owner was not running, no person to touch it, out of time); not a pass |
| `fail` | the part is there and did not do what it was asked |

The run's verdict is the worst of its parts, `fail` before `not_checked`
before `pass`; `not_fitted` makes no run worse. A part reached after the
two minutes are spent is `not_checked` and says so.

The self-test publishes its own topic and no `diagnostics`: `lamp-health`
keeps every part it has ever heard and would show a one-off `selftest`
part as stale until it restarts (`crates/health/src/parts.rs`).

## How each part is checked

One writer per device: the self-test never opens a device another part
owns. It listens on the bus, or asks the owner through the interface the
owner already has.

| Part | Owner | How | State |
|------|-------|-----|-------|
| voice microphone (Jieli) | lamp-voice | `mic2/audio` blocks arrive, level from the PCM | to do |
| room microphone (C-Media) | lamp-ears | `ears/level` arrives, `ears` diagnostic not error | to do |
| speaker | lamp-voice | a played tone heard by both microphones | needs a tone request in lamp-voice; none exists |
| servos 1 to 5 | lamp-body | `body/joint_states` fresh for all five joints; a small move through `expression/body` (±0.35 rad at most, lease-held) and back | to do; the move to be agreed with the fifth builder |
| ring | lamp-voice | each colour on `light/command`, seen on `light/state`; the person's own setting restored after | to do |
| camera | lamp-eyes | a permit on `/run/lampos-eyes/permit.sock`, a new `sight.jpg` | to do |
| touch pads, button | lamp-controls | `controls/touch_state` ready and not held; a swipe only at the factory | to do |
| air | lamp-air | `air/state` fresh and in range | to do |
| board | (none) | temperature and free disk as `lamp-health` reads them; MMC errors since boot from the kernel log (#784) | to do; nothing reads the kernel log yet |

## Not shown yet

No check has run on a lamp. The first builder runs it on lamp-4ace.
