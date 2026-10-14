# The units lamp-52e6 runs

Copies of the systemd units on lamp-52e6, the fifth builder's lamp: its voice
(`lamp-voice.service`), body (`lamp-body.service`) and eyes
(`lamp-eyes.service`). The voice and eyes units were installed on 2026-10-14
from lamp-4ace's, with the old runtime (`hal`, `os-server`, `led-boot`)
stopped and disabled and its files kept in `/opt/hal`. The body unit is this
lamp's own: its home, standing pose and folded arm as read at rest, its
`--gaze` measured on this lamp, and `--torque on`
([body.md](../../docs/body.md), "lamp-52e6"). They name one lamp
(`ConditionHost=lamp-52e6`), its devices and its measured poses, and are not
a template for another unit. No credential is in them: the provider's are in
a private file the voice unit only names.

Since 2026-10-14 the three are parts of `lampos.target` (here as
`lampos.target`), brought up by one service, `lamp-os`, which is the keeper
installed as the old runtime's updater installs a service package
([keeper.md](../../docs/keeper.md), "One service for the lamp"): its release
under `/opt/lampOS/lamp-os/<version>` carries these units in `units/`.

The long hexadecimal directories in `ExecStart` are the SHA-256 of the binary
installed there.

Since 2026-10-15 it runs every part lamp-4ace runs but three (#702): the
units here for actions, air, controls, ears, expression, health, memory,
name and schedule, and the voice and eyes, are lamp-4ace's with this lamp's
name and build, made by one rule (a line at the top of each says so); the
body is this lamp's own, now on the bus. Not here: policy (no key on this
lamp), tasks (nothing to hand work to) and records (no settings). The
keeper's release `0.2.0-fifth` carries them, so a boot brings up the whole
lamp ([whole-lamp-lamp-52e6-20261015](../../docs/verification/whole-lamp-lamp-52e6-20261015/README.md)).

Back to the old runtime on this lamp:

```text
sudo systemctl disable --now lamp-os
sudo systemctl enable --now hal os-server
```
