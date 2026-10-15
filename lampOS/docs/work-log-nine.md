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
