# Fifth builder's work log

Fifth works on `lamp-fifth`, with one `fifth/<task>` branch per task.
The running list belongs to First on `origin/lamp-voice`. Fifth's lamp is
lamp-52e6; every result on a lamp below names it. Fifth never touches
lamp-4ace.

## 2026-10-14 — body-second-lamp — taken

- Task: 22, The body on a second lamp.
- Base: `origin/lamp-voice` at `d721c34c1029925ff6c01a1805d53f09582b21f6`.
- Branch: `fifth/body-second-lamp`, based on `lamp-fifth` after this claim.
- State found on lamp-52e6, left by an earlier session of the same person
  on 2026-10-14, 11:00 to 12:50 (to be checked before it is trusted): V1
  (`hal`, `os-server`, `led-boot`) stopped and disabled, its files kept
  in `/opt/hal`; `lamp-voice` enabled; `lamp-body` and `lamp-eyes`
  installed, not enabled, started by hand, built from `3b90bde12`.
- Inputs: V1's servo behaviour (`hal/`, `robots/lamp/`), the audit on
  `origin/lamp-servo-v1-port` (`lampOS/docs/v1-servo-audit.md`), and
  [v1-verdicts.md](v1-verdicts.md), HAL-4.
- Questions: none yet.

## Proposed tasks

Work found while doing a listed task that cannot be folded into it. One
line each, proposed once; the first builder answers once under "Answers"
in work-split.md (`fifth-P1: yes`, `yes as task N`, `no`, or `yes, but
...`). Fifth reads the answers at the start of every task and every half
hour, takes nothing that has no answer, and does not ask again: an unclear
answer is taken in its narrowest safe sense and that reading is written
here. The ids carry the builder's name so that any number of builders can
use the same form without their ids meeting.

| Id | Short name | Why it is not part of 22 to 26 | What it touches | Answer |
|---|---|---|---|---|

## Found, not mine

Seen on lamp-52e6 while working on the body, outside `crates/body`; left
for its owner, not mended here.

- `led-shutdown.service` becomes active again whenever V1's `hal` stops,
  and `lamp-voice`'s check for old owners then refuses to start. Any
  cutover or wake path has to stop or mask it. (Seen by the earlier
  session on 2026-10-14; to be confirmed on the lamp.)
- `/run/lampos-bus` is created owned by root by `lamp-voice`; `lamp-health`
  and `lamp-expression` run as `orangepi` fail on it with "Permission
  denied". (Same source; to be confirmed.)
