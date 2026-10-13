# The units lamp-4ace runs

Copies of the three systemd units on lamp-4ace, the office lab prototype, as
they were installed on 2026-10-13: its voice (`lamp-voice.service`), body
(`lamp-body.service`) and eyes (`lamp-eyes.service`). The working copies are
beside the deploy scripts on the machine that installs them; these are here
so that what the lamp runs is in the repository. They name one lamp
(`ConditionHost=lamp-4ace`), its devices and its measured poses, and are not
a template for another unit. No credential is in them: the provider's are in
a private file the voice unit only names.

The long hexadecimal directories in `ExecStart` are the SHA-256 of the binary
installed there. The character file the voice unit's configuration names is
[`../character/lamp.txt`](../character/lamp.txt).
