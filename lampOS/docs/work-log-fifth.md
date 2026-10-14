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

## 2026-10-14 — keeper — taken

- Task: 27, The keeper: one lamp of many parts, started, watched, stopped
  and resumed. Taken before the rest of task 22, as the list says; task 22's
  install of the body on lamp-52e6 is its first step and is done
  (`fifth/body-second-lamp`).
- Base: `origin/lamp-voice` at `44543db0234c63306e1a59f3f678779baf3e09d8`.
- Branch: `fifth/keeper`, based on `lamp-fifth` after this claim.
- Lamp: lamp-52e6. Its voice, body and eyes run as lampOS units; the old
  runtime is stopped and disabled.
- Questions: none yet.

## 2026-10-14 — body-second-lamp — done on lamp-fifth

- Task branch `fifth/body-second-lamp`. Code: `lampOS: let the body take
  hold of a joint found without torque, goal first, behind a switch`
  (`--torque on`, off unless given). Documents and units: [body.md](body.md),
  "lamp-52e6", and `deploy/lamp-52e6/`.
- Gate on the Mac (macOS, arm64), from `lampOS/`: `cargo fmt --all --check`
  clean; `cargo clippy -p lamp-body --all-targets -- -D warnings` clean;
  `cargo test -p lamp-body` 240 passed, 0 failed; `cargo build --workspace`
  clean. Built for the lamp in `rust:1.96.0-bookworm` (linux/arm64): two
  warnings that are already on `lamp-voice` (`let mut eyes` in
  `src/bin/lamp-body.rs`, `inspection::run` unused on Linux).
- On lamp-52e6: what it ran written down and backed up to
  `~/fifth-backup-20261014` (units, `/var/lib/hal`); the body installed from
  this branch (`22e595c4…`) with `--torque on` and `--gaze 782,557`.
  - `--torque on`, once: yaw and roll with torque off; without the option
    refused ("wrist_roll torque is not on"); with it two torque writes, no
    joint moved (2061, 2049 before and after), 282 steps, stopped by hand.
  - Gaze, once each axis, with the eyes stopped and the face told on the
    body's socket: 782 and 557 counts a half-picture, both positive.
  - Driven by mood, twice (13:46 to 13:51 on the installed build, 14:05 to
    14:11 on this one): eleven exchanges, five nods, folded four times and
    stood twice; no stop, nothing gave way, no retreat.
  - Steps by hand with the body stopped, folded: roll moves 23 and 27
    counts and stops 13 to 17 short of a goal 40 away; the gains in the
    servos are the old runtime's (P 16/16/32/16/32, D 32, I 10 on 3 and 5).
- Failures and oddities kept: a goal written to a servo with torque off
  turns its torque back on (so the first try at a stale goal moved yaw and
  roll 10 to 18 counts); single warmth readings of 65 to 109; the shoulder
  standing 22 to 34 counts short and pushing at 100 to 150; the elbow folded
  at its goal reading 120 to 140.
- Not shown: a power cut (task 23), a hand (task 24), roll in use, leaning.
- For lamp-4ace: nothing changes without `--torque on`; with it, a lamp
  whose old runtime is gone starts by itself.

## 2026-10-14 — keeper — done on lamp-fifth, in part

- Task branch `fifth/keeper`. Crate `crates/keeper` (`lamp-keeper`, new
  workspace member, no new dependency), the body wired to it, units and
  templates in `deploy/units/` and `deploy/lamp-52e6/`, [keeper.md](keeper.md).
  No message type added: a part's state rides in its `diagnostics` under
  `state`.
- One service for the lamp, as agreed between the owner and the fifth
  builder's person: the old runtime's updater (parent repository,
  `docs/bootstrap-ota.md`, explicit service replacement, PR #674) installs
  one package whose `bin/service` is `lamp-keeper`; `serve` links the units
  the package carries, ties `lampos.target` to its own unit and starts it;
  `health` answers `{ready, version, pid}`. systemd still runs every part.
- Gate on the Mac, from `lampOS/`: `cargo fmt --all --check` clean;
  `cargo clippy -p lamp-keeper -p lamp-body --all-targets -- -D warnings`
  clean; `cargo test -p lamp-keeper -p lamp-body` 257 passed, 0 failed;
  `cargo build --workspace` clean. In `rust:1.96.0-bookworm` (linux/arm64)
  both build; clippy there stops on the two warnings already on
  `lamp-voice` in `lamp-body`.
- On lamp-52e6 (`lamp-keeper` `2f112fbb…` then rebuilt with the stale-answer
  mend, body `91a9d458…`), the package laid out as the updater lays it, the
  updater itself not run: start to ready 2.56 s and 2.53 s (5.8 s once, the
  body still folding from the stop before); stop to every part stopped about
  12 s, in order; body killed, back in 15.3 s; body hung, back in 19.6 s
  through its watchdog. Journal in
  `docs/verification/keeper-lamp-52e6-20261014/`.
- Failures kept: the first start skipped the target (its host was still the
  template's placeholder) while the parts ran anyway; `health` once answered
  ready at once from the run before, mended by writing "not ready" first.
- Not built: task 12's update and reset choreography, the pause in any part,
  a command topic on the bus. Not shown: the updater's own activation and
  rollback, a power cut with `lamp-os` enabled, the voice in its own slice.
- For the first builder: on lamp-4ace nothing changes until its units are
  made parts of the target (`deploy/units/README.md`); the body's
  `Type=notify` needs this build of the body.

## 2026-10-14 — wake — taken

- Task: 23, Waking up after the power comes back.
- Base: `origin/lamp-voice` at `a0512240ef7c12ea869a9144b045510615f01a60`; branch `fifth/wake` from
  `lamp-fifth`.
- First without a person: the state a power cut leaves stood in for by
  switching every servo's torque off on the folded lamp; the ten unplugs
  wait for the fifth builder's person.

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
