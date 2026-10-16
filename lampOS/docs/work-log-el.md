# El's work log

El works on `lamp-el`, with one `el/<task>` branch per task. The running
list belongs to the first builder (the Queue issue, #705). El's lamp is
`lamp-06c1` (blue-lamp); a result on it says so, and any other is not a
result on a lamp.

## Proposed tasks

| Id | Proposal | Answer |
|----|----------|--------|

## Found, not mine

## 2026-10-16 — joined

Ready for lamp-voice: no

- Role file `lampOS/builders/el.md` made from the template, on `lamp-el`
  from `origin/lamp-voice`. No `builder:el` issues yet; el takes none until
  the first builder has taken the file onto `lamp-voice`.
- Corrected the same day: el's person named `lamp-06c1` (blue-lamp) as
  el's lamp. `deploy/lamp-06c1/LAMP.md` names el as owner; its `runs:` is
  not yet read, since el has not ssh'd to it. That needs the person's key
  set up on the lamp (`ssh-copy-id orangepi@lamp-06c1.local`) and their yes
  before each ssh. The owner line in work-split.md is the first builder's.
- El's person asked it to work on #820 (the third's, pull request #844).
  Read at 10:20 +07: #844 at `9af776366` is green on ARM64 and reviewed;
  what remains is the reviewer's P3 (Expression's and Memory's start lines
  do not say the zone) and the first builder's lamp check. El makes the P3
  on `el/820-zone-start-lines`, cut from #844's head, and offers it to the
  third on #844. It does not push to `third/system-zone`.

## 2026-10-16 — #820's start lines, for the third (#844)

Ready for lamp-voice: yes (#844, the third's)

- Off unless set: the start lines gain ` zone=<as given>` only with
  `--zone`; without it they are byte-identical, as each test asserts.
- On `el/820-zone-start-lines`, from #844's head `9af776366`: Expression
  `12db9ac7b`, Memory `a2302ffea`. Tests
  `the_start_line_names_the_zone_only_when_one_is_given` (Expression) and
  `the_memory_days_start_line_names_the_zone_only_when_one_is_given` (Memory).
- Gate on el's Mac (Homebrew Rust 1.99, no rustup or Docker): fmt clean;
  `cargo build --workspace` and `--locked` clean; Expression and Memory 441
  passed, 1 failed (Memory's
  `a_timed_out_mutation_may_commit_but_cannot_seed_unlinked_journal_history`,
  passes alone). Clippy `-D warnings` clean for both with three 1.99-only
  lints allowed (`chunks_exact_to_as_chunks`, `byte_char_slices`,
  `deprecated`), which fire in files neither change touches. The 1.96 gate
  is ARM64 run 38846985609, started by hand.
- The third cherry-picked both as they were into #844 (head `2a4168cc5`,
  10:56) and keeps #820 for the first builder's lamp check. Not on a lamp.

## 2026-10-16 — lamp-06c1 onto V2, set up the way a home is (#811, #806)

Ready for lamp-voice: no (a lamp's own folder, `el/lamp-06c1-bringup`
`a61f35324`; findings for nine and the third on #705)

- Hold `el holding lamp-06c1 (whole lamp: bring-up to V2)` 11:50–13:30,
  agreed with jarvis. Person at the lamp from 12:30.
- V1 off and kept (`~/el-backup-20261016/`). `lamp-update install` from a
  test-signed local channel of ARM64 run 38851087915 (`6d2bea2e1`, build
  1009): 64 changes, nothing started. Models from published copies, each
  hash equal to the pinned one; `lamp-turn-end --check-model --fast` warm
  median 126 ms. Body poses read at rest with torque off (nothing written).
- Setup with the phone: hotspot up 26 s after the reboot; "Could not look
  for networks" until "Look again" a few times; Setup to `Connected` and
  the marker 14 s (12:40:12 → 12:40:26); back on the office network 12:40:48.
- First start: body and voice failed on two install bugs (lock folders
  0775; no voice sessions folder), both fixed by hand on the lamp and told
  to nine (#705, 12:46). Body then held still at home; voice reached the
  service through V1's proxy route. name, policy, tasks down as expected.
- Mac in front of the lamp: its speaker is heard (request logged 1–2 s
  after the line ended); "Lamp, what is two plus two?" answered aloud, head
  nodding; "Hello lamp, can you hear me?" judged not for it (no name part).
  Its camera frames the whole lamp; stills described, none kept.
- On lamp-06c1, once each. Not shown: the gaze and pitch window measured
  here (lamp-52e6's are used); a power cut; anything of #995 yet.
