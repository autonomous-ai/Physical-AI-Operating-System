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
