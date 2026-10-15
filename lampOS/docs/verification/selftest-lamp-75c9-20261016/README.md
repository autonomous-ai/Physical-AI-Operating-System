# lamp-selftest on lamp-75c9, 2026-10-16

Ten, session 942239d4, under lamp-75c9's lock (`eighth/lamp-lock`). Raw,
not edited: `console.txt` is the ssh session's output, `runs/` the files
the self-test kept.

- **Binary:** `lamp-selftest` from ARM64 run 38776559424 (green) of PR #827
  at `31c31fa5d`, SHA-256
  `c1b06fbd1f1ae818c1bd11bd3787288b5d47e6a38b856de3572587568311ff6e`, checked
  on the lamp. Put in `~orangepi/ten-selftest/bin/<sha256>/`, not
  `/opt/lampOS/`: sudo on this lamp asks for a password.
- **The lamp:** Orange Pi, Debian bookworm, aarch64, running the old V1
  runtime only (`hal.service`); no lampOS part, no `/run/lampos-bus`.
  `kernel.dmesg_restrict = 1`.
- **How:** as `orangepi`, once with `--occasion first-boot` and once with
  `--occasion on-demand`, each with `--bus-root ~/ten-selftest/bus` (an
  empty bus of its own) and `--dir ~/ten-selftest/runs/<occasion>`.

## What came out

Both runs: every part `not_checked` with its reason, the run `not_checked`,
exit 3, in 2.0 s (the voice microphone's 2 s wait for a first block). The
report was kept (`<started>.json`, `latest.json`, `latest.txt`) and
`lamp/selftest` declared and published on the bus. Storage: `/dev/kmsg`
refused (`Operation not permitted`), as `dmesg_restrict` makes it without
`CAP_SYSLOG`. With no `light/state` the ring asked for nothing: nothing lit,
moved or sounded.

## What was not shown

No part was checked against a real reading: lamp-75c9 runs no lampOS
part to publish one. Pass and fail on a lamp, the ring's colours and the
storage count with `CAP_SYSLOG` are for lamp-4ace (first builder) or a
lamp running lampOS.
