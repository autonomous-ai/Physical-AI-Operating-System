owner: eighth; shared with ten (ask the other before each use, hold `lamp-lock.sh lamp-75c9` while using it)
runs: V2, lampOS build 1335 (lamp-voice a35d2a3f5) by `lamp-update install` from a test-signed local channel; up: voice, eyes, name, expression, actions, memory, policy, health, controls, ears, air, schedule, skills; off: body (no measured poses), tasks (no Harness at /usr/local/bin); V1 disabled, files kept
since: 2026-10-16 17:41 +07, installed by eighth at its owner's request (#705, 17:5x)

- Board: Orange Pi, Debian 12 (Orange Pi 1.0.6), the home image; the board user `orangepi`.
- V1 back: `sudo ~orangepi/eighth-backup-20261016/restore-v1.sh`. The backup there holds V1's units and their earlier states.
- The update key on the lamp, `/etc/lampOS/update/key.pub`, is a test key (fingerprint `17077f6ba8c12c3d`), not the owner's: only a channel signed with that key installs here.
- The body waits for `/etc/lampOS/body/lamp.env`: this lamp's poses, read at rest with torque off, with its person at the lamp. Until then `lamp-body.service` is disabled, and nothing moves.
- Shared with the tenth builder for `lamp-tasks` and Harness, by the lamp lock (`lampOS/tools/lamp-lock.sh`).
