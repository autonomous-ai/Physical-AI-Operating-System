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
| voice microphone (Jieli) | lamp-voice | a second of `mic2/audio` blocks (given with `LAMP_BUS=on` and `LAMP_BUS_AUDIO=on`), their level above digital silence (-90 dBFS) and blocks lost counted; none in 2 s is `not_checked` (closed by the privacy switch, or not given). The played tone is still to do | listens: done |
| room microphone (C-Media) | lamp-ears | `ears/level` arrives, above digital silence (-90 dBFS) | listens: done |
| speaker | lamp-voice | a played tone heard by both microphones | needs a tone request in lamp-voice; none exists |
| servos 1 to 5 | lamp-body | `body/joint_states` fresh for all five joints; a small move through `expression/body` (±0.35 rad at most, lease-held) and back | readback done; the move to be agreed with the fifth builder |
| ring | lamp-voice | listened to only: `light/state` fresh (3 s) is `not_checked` ("listened only"), stale or unreadable fails. The self-test asks nothing on `light/command`: asking each colour there and putting the person's light back cannot be made safe from outside the owner (a request between the self-test's read and write is overwritten). Next: the owner takes test colours on a topic of its own that never touches the person's light, and says what it drew on `light/drawn`; the ring passes when what it drew matches | listening done; colours need lamp-voice |
| camera | lamp-eyes | `humans/faces/tracked` published within 2 s (the eyes publish each look while the camera is open); otherwise the `eyes` item of `lamp/health`: a warning ("does not open", "stopped giving pictures") fails, ok means closed because seeing is not permitted now and is `not_checked`. The self-test never opens the camera itself, so the privacy switch stays authoritative | listens: done |
| touch pads, button | lamp-controls | `controls/touch_state` ready, no fault, nothing held 10 s or more; and at the factory only, `touch_by_hand`: the person is asked on the console to swipe the strip, touch the head and press the button, and all 20 s are watched (a swipe is read to its end) for each source to feel the hand; the electrodes each felt are listed, the strip's from the bits of its `active_mask` (`E0` to `E11`), the head's by pad name, so one that never fires shows; a source not felt is `not_checked` and named | idle done; by hand done |
| air | lamp-air | `air/state` fresh (15 s) and in range | done |
| board | (none) | `board/state` from `lamp-health`: hottest under 85 °C, 3% of the disk free or more | done |
| storage | (none) | `/dev/kmsg`, read to its end: no SD/MMC re-tune that timed out ("dma hold bit clear timeout", #784) and no I/O error on `mmcblk` since boot, each counted; unreadable (no `CAP_SYSLOG` under `dmesg_restrict`) is `not_checked`, and so is a log that no longer holds the whole boot (its first record kept is not sequence 0, or records were overwritten while read) with no fault in what is left; a fault kept still fails | done |

A reading never given is `not_checked` (its owner may not run on this
lamp); one older than its freshness, or unreadable, is `fail`. A topic
never given a reading is not the end of it: the air, the room microphone
and the joints then ask their owner's item in `lamp/health` (`air`,
`ears`, `body`), and the owner's own warning fails the part with its words
(lamp-4ace, 2026-10-16: `lamp-air` alive since a power cut and no reading
yet). Touch with no `controls/touch_state` but a fresh `controls/state` is
`not_checked`: Controls runs, and whether it reads the pads is not known. Each joint
of the five is its own part. Run it with

```sh
lamp-selftest --occasion factory|first-boot|on-demand [--dir ABSOLUTE] [--bus-root ABSOLUTE]
```

The storage part reads `/dev/kmsg`, which a lamp with `kernel.dmesg_restrict = 1`
(lamp-75c9, 2026-10-16) opens only to a process with `CAP_SYSLOG`; the
journal is no substitute there (two kernel lines for the whole boot). A unit
that runs the self-test gives it `AmbientCapabilities=CAP_SYSLOG`; run by
hand without it, storage is `not_checked`.

Exit status: 0 every part passed or is not fitted, 1 a part failed, 3 a
part could not be checked, 2 the arguments were wrong.

## Shown on lamps

- **lamp-75c9**, 2026-10-16, V1 runtime only, an empty bus: every part
  `not_checked` with its reason, exit 3, 2.0 s, nothing lit or moved
  ([evidence](verification/selftest-lamp-75c9-20261016/)).
- **lamp-4ace**, 2026-10-16 01:14, by the first builder, on its bus with
  sudo (#812): both microphones, board, camera, the five joints and the ring
  (ten colours seen, the light put back) pass; storage fails on a real
  fault, 9 SD/MMC re-tunes timed out since boot, as `dmesg` agrees; air and
  touch were `not_checked` for the two reasons mended above.

The speaker's tone and the servos' small move are not built yet.
