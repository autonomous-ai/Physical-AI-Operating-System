# lamp-cb94's failed parts, 2026-10-17

Read by twelve under `lamp-lock.sh lamp-cb94`, 03:14 to 03:35 UTC. Everything was read-only, and the voice was left alone (Thirteen's soak). The first builder asked on #705, 10:08.

- `journals.txt`: `systemctl show -p Result -p ExecMainStatus ...` for controls, body, expression, name and tasks. None has a journal line, because the boot's journal begins at 15:22 UTC and they failed at 11:06 UTC.
- `journals3.txt`: the journal's boots and each part's unit (`systemctl cat`).
- `files.txt` (its first line, `2026-10-16 10:08:12`, is `uptime -s`: when the lamp booted, not when this was read), `perms.txt`: the files each unit names, their modes, and whether `lampos` can read them.
- `history.txt`: the settings were all written at 2026-10-16 04:03:32 UTC, before install 1314 ran at 11:05. Install's records then took them as its own settings.
- `repro.txt`, `repro2.txt`: controls, expression and tasks run for 3 s as `lampos`, with temporary bus and state folders.
  - controls and expression: `Permission denied (os error 13)`.
  - tasks: `No such file or directory (os error 2)`, because there's no Harness.

The causes:
- **controls and expression:** settings at `root 0600`. Install never set the mode of a default whose bytes were already its own, which this PR fixes.
- **name:** its models are missing, since build 1314 predates the model placing.
- **body:** no `lamp.env`.
- **tasks:** no Harness.

## The fix on the lamp, 06:21 to 06:25 UTC

This is the first builder's 11:42 yes. Every step was logged in the lamp's changes file, and the voice was not restarted.

- `step1.txt`: the modes before and after. `controls.json`, `controls-pins.json`, `expression.json` and `zone.env` go from 600 to 644, while `credential` and `jev.key` stay 600. `lampos` now reads all three settings. Controls and Expression started at 06:21:27, with their first journal lines.
- `step2.txt`:
  - two minutes later, both are still active with NRestarts 0, and the voice is unchanged (NRestarts 1, from its 04:53:58 restart);
  - there are no mmc or usb kernel lines;
  - the three name models are placed, root 0644, and their SHA-256 on the lamp matches `deploy/product/models.json`;
  - Name started at 06:23:37.
- `step3.txt`: `voice/state` is `resting`, the units are active, and Name's journal is clean. Its CPU part is void, a stale file read from an earlier run, so it was taken again in `step4.txt`.
- `step4.txt`: the voice at 39.1% of a core with the room quiet, and `controls/state` showing `enabled: true, ready: true`.
- `final.txt`: Controls, Expression and Name are active. Body (no `lamp.env`) and Tasks (no Harness) are still failed.
