# The whole lamp from power on lamp-52e6, 2026-10-15 (#702)

lamp-52e6 runs the parts lamp-4ace runs, as `deploy/lamp-4ace/` has them at
`lamp-voice` 6573ce6c1, with this lamp's name and its own body options: the
units in `deploy/lamp-52e6/`, from the binaries of the green ARM64 run
38615783378 (`c1b8c7591`), carried by the keeper's release `0.2.0-fifth`
(`/opt/lampOS/lamp-os/0.2.0-fifth`, `lamp-os.service` enabled at boot).
The voice's end-of-turn model and ONNX Runtime 1.30.0 are installed beside
it as `docs/endpoint-pilot.md` says; `lamp-turn-end --check-model --fast`
gave 0.96844 against the recorded 0.96823.

| Part | On lamp-52e6 |
|---|---|
| voice, body, eyes, name, ears, expression, health, memory, schedule, actions, controls, air | running, parts of `lampos.target` |
| policy | not installed: this lamp has no key for it (`/etc/lampOS/policy/jev.key`) |
| tasks | not installed: nothing to hand work to on this lamp (no Claude Code or harness pairing); the voice's `LAMP_TASKS_DIR` is left out |
| records | not installed: no `/etc/lampOS/records.json` |

## A soft reboot, 02:42 (nobody at the lamp)

`sudo systemctl reboot`, then from the journal and systemd's own clock
(seconds after the kernel started):

| | s |
|---|---|
| kernel + userspace (`systemd-analyze`) | 14.5 |
| `lamp-os` (the keeper) active | 10.6 |
| every other part active (`ActiveEnterTimestampMonotonic`) | 18.6 |
| the body active (`Type=notify`: after its checks and its wake) | 19.1 |
| `lamp/health` at 47 s: every expected part reporting, none missing | yes |

No part restarted (`NRestarts` 0 for all twelve). Two parts warn:

- **air:** "the sensor does not answer: word 0 fails its CRC", and once
  "command 0xd014: Invalid argument". The sensor answers at 0x6b on bus 0
  (`i2cdetect`), but not to the part. Found, not mended (not this task's).
- **expression:** "waiting for light setting, fresh room state".

One thing learnt on the way: `systemctl restart lamp-os` while the units
change restarts the lamp before the keeper links the new units (the target
is bound to it), so the parts came up on the old definitions until the
reboot. Installing a release is a reboot, or a restart of `lamp-os`
followed by one of `lampos.target`.

## For the person tomorrow: the unplug (two minutes)

1. Say when you are at the lamp; the builder says "ready" and watches the
   lamp through the Mac's camera.
2. Pull the lamp's power plug, wait 10 s,
   put it back. Hands off the lamp after that.
3. That is all. The builder reads, from the journal of that boot: when
   each part became active, when the keeper said ready, the body's wake
   (where every servo was found, how far and how long it rose), and
   `lamp/health`, and says whether the lamp came back by itself.

## The unplug, 07:44–07:51 (a colleague at the lamp)

The plug pulled and put back by hand, twice. The first time the board did
not start: the journal has no boot between the last one and the next
(power or plug, not lampOS). The second time it booted (boot `3754253f…`),
and from the journal and systemd's clock (seconds after the kernel
started):

| | s |
|---|---|
| kernel + userspace (`systemd-analyze`) | 5.378 + 9.445 |
| `lamp-os` (the keeper) active | 11.2 |
| every other part active, but the voice and the body | 19.06 to 19.15 |
| `lampos.target` active | 19.6 |
| voice `listening_ready`, after 3 restarts | 36.5 |
| body active (stood up) | 54.9 |

- **Body:** its first start ended at 19.5 s, "taking hold: wrist_roll did
  not acknowledge a write": the servos had only just got power. systemd
  started it again at 34.6 s; it took hold, stood, and was active at
  54.9 s. Mended in a separate pull request (`fifth/cold-start`): the body
  waits for servos that are slow to answer after a power cut.
- **Voice:** exited 1 at 19.8 s; at 24.7 s "Connection refused (os error
  111)" from its provider; at 31.0 s "snd_pcm_open failed with errno 16";
  then up, `listening_ready` at 36.5 s. Not this task's: the first
  builder's, noted for them on #702.
- The keeper's answer then: `{"ready":true,"version":"0.2.0-fifth","waiting_for":[]}`.

The lamp came back by itself from a power cut, every part running and the
body standing, with nobody touching it.
