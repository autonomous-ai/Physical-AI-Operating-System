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

The long hexadecimal directories in `ExecStart` are the SHA-256 of the binary
installed there.

Back to the old runtime on this lamp:

```text
sudo systemctl disable --now lamp-voice lamp-body lamp-eyes
sudo systemctl enable --now hal os-server
```
