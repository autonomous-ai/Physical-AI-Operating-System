# Ninth builder's work log

Nine works on `lamp-nine`, with one `nine/<task>` branch per task. The
running list belongs to the first builder (the Queue issue, #705). Nine has
no lamp; no result below is a result on a lamp.

## Proposed tasks

| Id | Proposal | Answer |
|----|----------|--------|

## Found, not mine

## 2026-10-15 — joined

Ready for lamp-voice: no

- Role file `lampOS/builders/nine.md` made from the template, on
  `lamp-nine` from `origin/lamp-voice` at `4eec32958`. No lamp, no tasks
  yet; nine takes none until the first builder has taken the file onto
  `lamp-voice` and labelled issues `builder:nine`.

## 2026-10-15 — one install for a new lamp (#811) — taken

Ready for lamp-voice: no

- Task: #811, given by the first builder at 16:43 (Queue #705) when it took
  nine's role file onto `lamp-voice` (`5fbbc744f`). Claimed 16:55.
- Base: `origin/lamp-voice` at `462d04cee`. Branch: `nine/install`.
- First: read `deploy/lamp-4ace/`, `deploy/units/`, the updater
  (`docs/updates.md`, `lamp-update`, #741) and setup (#806), then write
  `docs/install.md`.

## 2026-10-15 — one install for a new lamp (#811) — draft pull request

Ready for lamp-voice: no (draft #815, waiting on the second's answer on #811)

- Commits on `nine/install`, on `lamp-voice` at `414401267`: `d1240063f`
  (docs/install.md), `50be414b8` (deploy/product/), `e729bf45a`
  (`lamp-update install` and its tests).
- Kind: new files and one new command; nothing on a lamp changes.
- Gate on the Mac: fmt clean, clippy `-p lamp-update` clean, `cargo test -p
  lamp-update` 33 + 6 + 15 passed, 1 ignored; workspace builds. Linux ARM64
  in Docker: clippy clean, same counts. `systemd-analyze verify` (systemd
  252, bookworm arm64) on every product unit: no complaint.
- Not shown: anything on a lamp (nine has none); `useradd`, `systemctl`,
  `systemd-tmpfiles` themselves; a round updating a lamp installed this way
  (needs rounds to know `current`, asked on #811).
- Found, not mine: `deploy/units/lamp-surround.service` names
  `/etc/lampos/surround.json` (lower case); `deploy/units/README.md` says
  `lamp-records` has no unit on any lamp, while `deploy/lamp-4ace/` has it.

## 2026-10-15 — one install for a new lamp (#811) — rounds know the link

Ready for lamp-voice: no (draft #815, until its ARM64 run on `73a1fffa8` is green)

- Answers on #811: the first builder (17:37) and the second (17:46) said
  yes to `current` links, with both paths kept and a test for each; the
  third (17:07) gave setup's marker, `/var/lib/lampos-setup/done`.
- `nine/install` is two commits on `lamp-voice`: `7a8323235` (product units,
  install.md) and `73a1fffa8` (`install`, and rounds that switch a product
  part by its link). Settings moved to `/etc/lampOS/<part>/` as decided.
- Gate on the Mac: fmt clean, clippy `-p lamp-update` clean, tests 33 + 7 +
  19 passed, 1 ignored; workspace builds. Linux ARM64 in Docker: clippy
  clean, same counts. With the link never moved on purpose, the 4 new
  rounds fail and the 15 old pass.
- Not shown: anything on a lamp; a real `systemctl`; a power cut on a
  lamp's storage during a link switch.
- lamp-75c9 (17:52, the person's own command under the lock, read-only):
  model `sun60iw2`, HAL active, no `HAL_BOARD`, `/dev/spidev3.0` and `3.1`.
  For #639 in the parent repository, not a lampOS task.

## 2026-10-15 — one install for a new lamp (#811) — ready

Ready for lamp-voice: yes (#815)

- Head `73a1fffa8` (two commits on `lamp-voice`). ARM64 run 38724084947
  green on it. Kind: changes nothing on a lamp running today (the 15 old
  rounds pass unchanged; the link path is taken only by product units, on
  no lamp yet). The second reads `crates/update` before it merges.

## 2026-10-15 — no internet at home (#830) — taken

Ready for lamp-voice: no

- Task: #830, given in the Queue after #811 went ready; with the first
  builder's addition of 18:18 (no battery clock: a clock not set is said,
  and reminders and the night wait for it). Claimed 18:21.
- Base: `origin/lamp-voice` at `4ad0ec5c3`. Branch: `nine/offline`.
- First: each part offline, read in its code, then `docs/offline.md`.

## 2026-10-15 — no internet at home (#830) — step 1, then paused

Ready for lamp-voice: no

- `docs/offline.md` on `nine/offline` (`44b1aed9c`): each part with no
  internet and with a clock not set, read from the code (not run). Found:
  the voice exits after 25 s when it starts with no internet, and lamp-4ace's
  and lamp-52e6's units restart it every ~29 s without end; nothing is said
  or shown; schedule's `--trust-clock` on both lamps uses a clock not set;
  `clock_synced` is never known on a real board.
- Paused at the person's request (end of the day). Next: steps 2 to 4 as
  offline.md says. #815 (#811) waits for the second's review.
- lamp-75c9: lock free, not held.

## 2026-10-16 — no internet at home (#830) — the voice starts offline

Ready for lamp-voice: no (draft #884)

- Session b418fcd1. `nine/offline` rebased on `lamp-voice` `416efbc43`;
  new commit `4a257f515`: with `LAMP_START_OFFLINE=on` a voice service
  started with no internet keeps listening past its 25 s startup budget once
  its microphone is retained, and becomes ready when the link comes up.
  Kind: off unless set; with it off the budget is unchanged (tested).
- Gate on the laptop (Linux x86_64): fmt clean, clippy `-p lamp-voice
  --all-targets` clean, workspace builds; `lamp-voice` lib 1149 passed,
  8 failed, 8 ignored. All 8 also fail on unchanged `lamp-voice`: 5
  `coordinator::motor::tests` (#739's family), 3 `provider_worker::tests::
  reminders` (shared `room_told::ROOM`, filed as #882; 12/12 serially).
- Not shown: a run with the network cut, on a host or a lamp.
- lamp-75c9: the person gave it to nine for this session (role file still
  says none; `builders/eighth.md` names it the eighth's; no `LAMP.md`).
  At 00:18:33 nine's `try` with `eighth/lamp-lock`'s script replaced ten's
  lock as "expired (1792084713s without renewal)"; the eighth's own test
  then removed the lock at 00:19:50. Nine read only (units: `hal`,
  `os-server`, `lamp-cmedia-speaker`; no `/opt/lampOS`), changed nothing,
  and does not hold the lock. Told the eighth.

## 2026-10-16 — no internet at home (#830) — the clock, and lamp/online

Ready for lamp-voice: no (draft #884)

- On `nine/offline`, each off unless set: `lamp-health --clock kernel`
  (`a8d3c752a`), `lamp-schedule --kernel-clock` with one reader in
  `lamp_ipc::clock_synced` (`c468d2121`), `lamp-expression --kernel-clock`
  (`b14606c51`), the voice's `LAMP_KERNEL_CLOCK=on` (`311a1d6c6`),
  `LAMP_VOICE_LINK=on` publishing `voice/link` (`c5f68dc96`), and
  `lamp-health --online on` publishing `lamp/online` (`28103ef52`).
- ARM64 runs green on `c468d2121`, `b14606c51`, `311a1d6c6`, `c5f68dc96`.
- lamp-75c9, read-only under the lock at 00:25: chrony keeps the clock, no
  `/run/systemd/timesync`, `timedatectl` `NTPSynchronized=yes`.
- Found, not mine: `lamp-schedule`'s storage tests fail about 1 run in 6 on
  unchanged `lamp-voice` (#887).
- Asked the first builder on #830: the notice's wording and recording, where
  it is said (a local-only turn), and the offline look on the ring. Nothing
  visible built until answered.

## 2026-10-16 — one install for a new lamp (#811) — setup at first boot

Ready for lamp-voice: no (draft #896)

- `nine/install-boot` from `lamp-voice`, `8fdfec844`: `lamp-setup.service`,
  `lamp-setup-reset.service` and `lampos-setup-done.path` in the product,
  written and enabled by `install`. Gate: update clippy clean, 34 + 16 + 19
  passed, 1 ignored; `systemd-analyze verify` clean on the three.
- Asked on #811: setup's settings (third), the keeper on a product lamp
  (fifth), Harness at boot as a system unit for `lampos` (first builder).

## 2026-10-16 — merges, the build mend, the mute guard (#960), the offline face (#830)

Ready for lamp-voice: #974 yes (ARM64 green); #978 when its ARM64 run is green

- Merged into `lamp-voice` in the 10:33 round and before: #884 (#830
  offline), #896 (#811 setup units), #934 (#811 Harness at boot). #811 stays
  open until a first boot on a wiped lamp.
- #884's offline turns and #954's `Reply.say` merged side by side and broke
  the build at `8da7d7ee1`. My mend #977 was the same tree as the second's
  #976 (`f5a20ba28`); closed as superseded.
- #974 (`nine/mute-guard`, `071f79c11`): a requested stop releases the
  restart guard even if cleanup faults, writing the latch as it was. ARM64
  run 38845579433 green. Point 2 (what a lamp restored muted shows and says)
  asked on #960.
- #978 (`nine/offline-face`, `a87fd0b47`): `lamp-expression --offline-face`
  draws listening as not ready while `lamp/online` says offline. Expression
  tests and workspace build clean; ARM64 run 38847656338.

## 2026-10-16 — the afternoon's merges and what waits

Ready for lamp-voice: #997, #1102 (with #1120, #1102 first), #1052 green

- Merged into `lamp-voice` since the morning: #989, #986, #1008, #1043,
  #1046, #1050, #1055, #1018, #1041, #1063, #1118 (lookup) and #1105 (heat).
- #997 (`ba41bbe0f`): a restored mute's marker names its guard record
  (`clock_id`, `through_us`), so a marker left by a failed unlink never opens
  a person's mute after a restart. Voice suite 1404 passed. Awaits the
  reviewer's re-check.
- #1102: install never copies V1's key after a reset. To merge as a pair
  with the third's #1120, #1102 first.
- #1052: models from the `models-2026-10-16` release, rebased to
  `9ef2faf43`, green.
- #990 waits on DevOps. Proved #811's Harness-at-boot on lamp-75c9 and undid it.
