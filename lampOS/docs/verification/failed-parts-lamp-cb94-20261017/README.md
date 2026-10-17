# lamp-cb94's failed parts, 2026-10-17

Read by twelve under `lamp-lock.sh lamp-cb94`, 03:14 to 03:35 UTC. Everything was read-only, and the voice was left alone (Thirteen's soak). The first builder asked on #705, 10:08.

- `journals.txt`: `systemctl show -p Result -p ExecMainStatus ...` for controls, body, expression, name and tasks. None has a journal line, because the boot's journal begins at 15:22 UTC and they failed at 11:06 UTC.
- `journals3.txt`: the journal's boots and each part's unit (`systemctl cat`).
- `files.txt`, `perms.txt`: the files each unit names, their modes, and whether `lampos` can read them.
- `history.txt`: the settings were all written at 2026-10-16 04:03:32 UTC, before install 1314 ran at 11:05. Install's records then took them as its own settings.
- `repro.txt`, `repro2.txt`: controls, expression and tasks run for 3 s as `lampos`, with temporary bus and state folders.
  - controls and expression: `Permission denied (os error 13)`.
  - tasks: `No such file or directory (os error 2)`, because there's no Harness.

The causes:
- **controls and expression:** settings at `root 0600`. Install never set the mode of a default whose bytes were already its own, which this PR fixes.
- **name:** its models are missing, since build 1314 predates the model placing.
- **body:** no `lamp.env`.
- **tasks:** no Harness.
