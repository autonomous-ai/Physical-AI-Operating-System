# Tenth builder's work log

Ten works on `lamp-ten`, with one `ten/<task>` branch per task. The
running list belongs to the first builder (the Queue issue, #705). Ten has
no lamp; no result below is a result on a lamp.

## Proposed tasks

| Id | Proposal | Answer |
|----|----------|--------|

## Found, not mine

## 2026-10-15 — joined

Ready for lamp-voice: no

- Role file `lampOS/builders/ten.md` made from the template, on
  `lamp-ten` from `origin/lamp-voice` at `eb2ba928e`. No lamp, no tasks
  yet; ten takes none until the first builder has taken the file onto
  `lamp-voice` and labelled issues `builder:ten`.

## 2026-10-15 — #812 taken

Ready for lamp-voice: no

- Took [#812](https://github.com/autonomous-ai/lampOS/issues/812), every lamp
  checks itself (`lamp-selftest`), given by the first builder at 16:46. No
  lamp: built against the parts' interfaces with fakes; the first builder
  runs it on lamp-4ace.

## 2026-10-15 — #812, stopped for the day

Ready for lamp-voice: no

- Draft #827 on `ten/selftest`: the report and `lamp/selftest`, the runner
  and kept files, and the checks that only listen (room microphone, air,
  touch idle, board, five joints). Gate on a Mac: fmt and clippy clean,
  `lamp-selftest` and `lamp-msgs` 101 tests passed; ARM64 green on the
  first commit (38724936523), running on the second.
- To do: ring, camera, kernel MMC errors, voice microphone; the speaker
  needs a tone request in lamp-voice; the servos' small move is for the
  fifth to agree.
- Asked on #812 whether ten may use lamp-75c9 (beside its person's Mac,
  shared with the eighth by a lock); no answer yet. Nothing was run on any
  lamp.

## 2026-10-16 — #812, storage and camera; lamp-75c9 given

Ready for lamp-voice: no

- `ten/selftest` rebased on `origin/lamp-voice` (`416efbc43`), with
  `4de686543`: the `storage` part reads `/dev/kmsg` to its end and fails on
  any SD/MMC re-tune that timed out ("dma hold bit clear timeout", #784) or
  `mmcblk` I/O error since boot; unreadable is `not_checked`. The `camera`
  part passes on a look published by the eyes within 2 s, fails on the
  eyes' own warning in `lamp/health`, and is `not_checked` when the camera
  is closed. It never opens the camera, so the privacy switch stays
  authoritative. Gate on a Linux x86_64 laptop: fmt clean, clippy
  `-D warnings` clean, `lamp-selftest` 19 + 1 tests passed,
  `cargo build --workspace` clean. ARM64 run 38775438906 started.
- My person gave me lamp-75c9 for this task at 00:05 (beside their
  laptop, taken in turns under the eighth's on-lamp lock, eighth-P1). Its
  key is not yet installed; nothing has run on any lamp.
- Found, not mine: `lamp-lock.sh status` says a lamp is free when ssh to
  it fails (#880, eighth).
- To do: ring, voice microphone; the speaker's tone and the servos' small
  move as before.

## 2026-10-16 — #812, ring and voice microphone; first run on lamp-75c9

Ready for lamp-voice: no (draft #827, to be marked ready when its ARM64 run on `090bfb98e` is green)

- On `ten/selftest`: `d7d2cc2e3` the ring (ten colours at the factory and
  on demand, the person's own light put back; listened to only at first
  boot), `31c31fa5d` the voice microphone (a second of `mic2/audio`),
  `f427f3573` storage needs `CAP_SYSLOG` where `dmesg_restrict` is set,
  `090bfb98e` the evidence. Gate on a Linux x86_64 laptop: fmt and clippy
  clean, `lamp-selftest` 25 + 1 tests passed, workspace built. ARM64
  38776559424 green at `31c31fa5d`.
- **On lamp-75c9**, once, under its lock: that run's binary (SHA-256
  `c1b06fbd…11ff6e`) in `~orangepi/ten-selftest/bin/<sha256>/`, not
  `/opt/lampOS/` (sudo needs a password there), run as `orangepi` on an
  empty bus of its own, first-boot and on-demand. Both: every part
  `not_checked` with its reason, exit 3, 2.0 s; nothing lit or moved. The
  lamp runs only V1 (`hal.service`), so no part had a reading to judge.
  `docs/verification/selftest-lamp-75c9-20261016/`.
- Not shown: any pass or fail against a real reading; the ring's colours
  on a ring; the storage count with `CAP_SYSLOG`. Speaker and servo move
  still to do.
- Found, not mine: the lease lock (`91a75ad64`) took an old-format lock as
  expired, so nine replaced ten's lock 2 s after it was taken (on #880);
  fixed in `1ccfa74b6`.

## 2026-10-16 — #812, review answered; the speaker tone

Ready for lamp-voice: yes (#827), yes (#895)

- #827 (`78fef010c`, ARM64 38790704955 green): the reviewer's findings
  answered: storage `not_checked` when the kernel's log lost part of the
  boot; short bus fixture paths; the ring `not_checked` when every colour
  is only taken (nothing says what it drew) and yielding to a light asked
  for during it; touch's mode called unknown; air asks its owner in
  `lamp/health` (lamp-4ace's SEN63C fails its CRC since 17:17, a hardware
  fault). Rebased on `lamp-voice`. lamp-selftest 30 + 1 tests passed.
- #895 (`ce396a967`, ARM64 38790826013 green): the speaker tone in
  lamp-voice, off unless `LAMP_SELFTEST_SPEAKER=on`, as agreed on #812 at
  01:27. Gate: fmt, clippy clean; new tests pass. The full lamp-voice run on
  this laptop at load 35–42 failed 18 timed tests; 13 pass alone, 2 pass one
  at a time, and 3 in `provider_worker` fail on unchanged `lamp-voice`
  (`587a361a3`) too (#739).
- Asked the first builder (#812) for a "drawn" acknowledgement from the
  ring so the ring can pass. Not on a lamp: the tone, the self-test's
  request side (after #827).

## 2026-10-16 — #812, the tone heard on lamp-4ace; ring and touch by hand

Ready for lamp-voice: yes (#895), yes (#899), not yet (#907, ARM64 running)

- #827 merged by the reviewer at 02:52 (`08d0c04b5`): the passive
  self-test, the ring listened to only.
- #895, the speaker tone: **played on lamp-4ace by the first builder at
  03:46** (#895): pass, rise 61.05 dB, delay 133 ms, cancelled 14.8 dB; no
  face, body move or speech; the voice went on. Not my lamp, not my run.
  The reviewer's findings answered: the ring told before the tone's turn is
  admitted; the check's own bus failures switch it off; only a tone played
  to its end is judged, learned in the settle step; the delay from the
  speaker's first acceptance. ARM64 38801121849 green on `772f5dcc7`.
- #899, `light/drawn` and `light/test` in the ring owner, as the first
  builder chose: a test asked no later than the person's newest light is
  never shown. ARM64 38803204302 green on `ba57716a4`.
- #907, touch by hand at the factory: the strip's electrodes from its mask,
  the whole 20 s watched.
- Next, once #895 and #899 are in: the self-test's side of the tone and
  of the ring's colours.
