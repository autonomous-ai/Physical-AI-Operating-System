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

## 2026-10-14 — head-follow — measured on lamp-52e6, the person's eye still to come

- Task: 25. Base `origin/lamp-voice` at `e93b9684929d62248394d61c87ec57120db43229`; branch
  `fifth/head-follow`. No code: numbers in [body.md](body.md), "How the head
  follows (lamp-52e6, 2026-10-14)", raw in
  `docs/verification/head-follow-lamp-52e6-20261014/`.
- Run on lamp-52e6, standing, the body stopped (killed and its restart
  cancelled, so that it did not fold), each step and each of twelve nods
  once, read from the servos about every 16 ms.
- What came out: pitch starts 85 to 95 ms after the goal, 90% of a 40-count
  step down in 181 ms, up in 727 ms; roll and yaw rest 10 to 19 counts short
  (gain 16). A nod asked 45 counts for 0.15 s reaches 34 to 35 counts
  (3 degrees) in about 0.2 s and is back in 0.31 to 0.37 s; asked 25 counts
  it moves about half of that. Acceleration 60 to 150 changed little.
- Proposal for Expression's catalogue (the third builder's; not changed
  here): a nod at a pause is head pitch 45 counts toward the base, held
  0.15 s, then back: about 3 degrees in about 0.35 s on this lamp. The body
  as it stands cannot pass it that fast: its commanded pose moves at
  `--speed` (120 on both lamps), so the nod takes 0.75 s there; a nod asked
  through Expression would need the body to allow about 300 counts a second
  for that one movement (its cap is 300). Smaller than 20 counts is never
  asked by the body at all, so a nod under about 2 degrees cannot be made.
- Not shown: how it looks to a person; heat over many nods; folded.

## 2026-10-14 — weigh — first figure

- Task 26, not taken yet: the fifth builder's person weighed the whole lamp
  at 6.6 lb, 3.0 kg, on 2026-10-14 (how, and whether with its cable, to be
  written when the task is done). The specification quoted in the task says
  7 kg; the old runtime's model adds up to 1.5 kg.

## 2026-10-14 — wake — code on lamp-fifth, the unplugs still to come

- `fifth/wake`: `lampOS: let the body wake from wherever it is found, slowly
  and never away from home, behind a switch` (`--wake on`, off unless given;
  without it the packets are unchanged, a test says so). Gate on the Mac:
  fmt and clippy clean, `cargo test -p lamp-body` 247 passed, 0 failed,
  workspace builds; built for the lamp.
- On lamp-52e6, without a person: (1) folded, every servo's torque switched
  off: nothing drooped in 10 s; the installed body (`--torque on`) started
  from there to ready in 0.18 s with no jump. (2) `--wake on` with the base's
  home moved 300 counts: woke 311 counts in 24.3 s, and back 283 in 22.2 s,
  no missed read, nothing gave way. Raw in
  `docs/verification/wake-lamp-52e6-20261014/`.
- Kind (builder-with-a-lamp.md): off unless set.
- Not shown, waiting for the person: the ten unplugs (what the servos do
  at power-up, then with `lamp-os` enabled); a power cut while standing.

## 2026-10-14 — give-way — code on lamp-fifth, the hand still to come

- `fifth/give-way`: `lampOS: let the body keep a joint where a hand puts it
  and stop pushing a stalled one, behind a switch` (`--give-way
  return|stay`, absent = as before). Rebased onto the wake; both side by
  side. Gate on the Mac: fmt, clippy clean; `cargo test -p lamp-body` 253
  passed, 0 failed; workspace builds; Linux arm64 build with only the two
  old warnings.
- On lamp-52e6, no hand: two runs standing 26 s, without and with
  `--give-way return`. With it the shoulder (standing 50 to 60 counts short,
  pushing) was counted stalled once and stopped pushing; nothing gave way and
  nothing else changed. Raw in `docs/verification/give-way-lamp-52e6-20261014/`.
- The old runtime switched every servo's torque off for 120 s when one read
  80% for 1.5 s, and the arm dropped. This body never switches the arm's
  torque off: it stops pushing first. The servos' own protection (80% for
  2 s, then 20% of their force, read on lamp-4ace and lamp-52e6) still lets
  the arm down under a hand that keeps forcing it; the hardware team's view,
  through the fifth builder's person, is that dropping is the right
  protection for the servo. Kept as the last layer; asked of them whether the
  software should also let go, and why (heat, gears, current).
- Kind: off unless set.
- Not shown: any hand; `stay`; heat over minutes.

## 2026-10-14 — wake — the first two unplugs

- On lamp-52e6, folded, the person's hand on it, power off about 10 s, lampOS
  disabled at boot and only a read-only script reading every servo from the
  port's appearance for 90 s. Twice; the person saw it stay still both
  times. Raw in `docs/verification/wake-lamp-52e6-20261014/`.
- What came out: the port appears 14.2 to 14.6 s after power; every servo
  reads **torque 0 and goal 0**, and nothing moves for the 90 s. The head
  droops on the cut, toward the base: pitch 1773 before, 2022 and 2034 after;
  the others move 1 to 82 counts (yaw 2038 to 1958 the second time).
- So torque written on without a goal first would send every joint to 0;
  `--torque on` writes the goal first.
- A failure: the body from `fifth/give-way` with `--wake on`, started on the
  second cut's pose, did not wake (`woke: null`) and stopped on tracking
  (pitch asked 2000, at 2032). By hand: from that pose the pitch servo lifts
  the head only with its goal 40 to 80 counts ahead (a 40-count step moved
  it 7 counts; a 260-count ramp over 3 s lagged 40 to 80 at loads 200 to
  320). Being mended (`fifth/wake`, next entry); the standing unplug and
  the two with `lamp-os` wait for it.
- The task asks for ten unplugs; the fifth builder's person asked for
  fewer. Planned: these two, one standing, two with `lamp-os` enabled.

## 2026-10-14 — wake — five unplugs done; the wake mended and on lamp-52e6

- `fifth/wake-fix`: `lampOS: let the body wake a head that drooped in a
  power cut, leading the servo enough to lift it` and `lampOS: let a wake
  that settles a little short of home end there and run from where the
  joints are`. Gate on the Mac: fmt, clippy clean; `cargo test -p lamp-body`
  256 passed, 0 failed; workspace and the Linux arm64 build as before.
  Installed on lamp-52e6 under `lamp-os` with `--torque on --wake on`
  (body `255e618b…`).
- Five unplugs on lamp-52e6, the person's hand on it (fewer than the ten the
  task asks, at the person's request):
  1, 2: folded, read only (above). 3: the same, by mistake: the install made
  just before it never reached the flash (the journal of that boot ended
  before it), so the lamp booted as for 1 and 2; one more reading of the
  same kind. 4: folded, `lamp-os` enabled: `lamp-os` started 10.7 s after the
  kernel, the voice 13.3 s, the body lifted the head 2029 to about 1800 in
  about 21 s and said ready at 34.8 s, the eyes with it; `health` ready.
  5: standing: the shoulder stayed up (2633, held by its gears, load 0), the
  elbow fell to folded (2074), the head drooped. The body refused to stand
  ("elbow_pitch is 507 counts off the way"), woke the head in 21.4 s, and
  then stopped on tracking three times running (head pitch held 30 to 37
  counts short of goals 1759 and 1775 in that pose), so systemd started it
  again each time; stopped by hand. The shoulder was then lowered by hand
  script to folded (2633 to 2059 in 15 s, load at most 172) and the body ran
  on and stood up.
- Learnt: an install must be synced before a power cut can be taken; the
  journal of a boot cut short is lost (journald writes every 5 minutes).
- To mend next (`fifth/wake-fix`): lower a shoulder found up with its elbow
  folded, the pose a standing power cut leaves; and a head held a little
  short of a small goal at rest should rest there, not end the run.

## 2026-10-14 — the state of tasks 22 to 27, by the ready line

Ready for lamp-voice: as each line says (`fifth-P4`, proposed).

- 22 body-second-lamp: yes (`568a27c6a`).
- 27 keeper: yes for what is built (`82a3b8752`); the update choreography and
  the pause are not built.
- 23 wake: no. On `lamp-fifth` up to `e4d1180e4` (the drooped head, the
  settle) and running on lamp-52e6; being mended for a shoulder left up by a
  standing power cut, a head held short at rest, and the two ends lamp-4ace
  showed (the base sticking at a slow goal, the shoulder's 7-count sway).
- 24 give-way: no. `--give-way` and `--overload release` are on
  `lamp-fifth` (`3a148c740`); no hand has tried either.
- 25 head-follow: no. Measured; the nod's speed switch is still to make.
- 26 weigh: no. Only the whole lamp's weight.

## 2026-10-14 — wake — done for lamp-52e6

Ready for lamp-voice: yes (the commit that adds this entry)

- `fifth/wake`: `lampOS: let a woken lamp lower a shoulder left up by a
  power cut and rest a head held a little short` (both behind `--wake on`),
  with the two ends lamp-4ace showed: the base now gets the lifting lead,
  and an arm may sway 15 counts during a wake. Gate on the Mac: fmt, clippy
  clean; `cargo test -p lamp-body` 264 passed, 0 failed; workspace and the
  Linux arm64 build as before. Body `d673c448…` installed on lamp-52e6 under
  `lamp-os` (`deploy/lamp-52e6/lamp-body.service`).
- On lamp-52e6, the pose a standing power cut left (shoulder up, elbow
  folded) made again by a script: shoulder 2588, elbow 2085. The body
  lowered the shoulder to 2059 before it said ready and ran on: no stop, no
  restart. The unplug with `lamp-os` enabled (entry above) is the one at
  power-up; `lamp-os` is enabled at boot on lamp-52e6, which is the unit's
  `[Install]` the task asks for.
- Kind: off unless set (`--wake on`). Not shown on lamp-4ace: its base and
  arm mends; not shown: a standing power cut with this build.

## 2026-10-14 — the old runtime's leftovers, under task 23

Ready for lamp-voice: yes (the commit that adds this entry)

- `fifth/keeper`: `lampOS: let the keeper clear the old runtime's leftovers
  and make the bus root before it starts the lamp` (package mode only:
  masks then stops whichever of hal, os-server, led-boot, led-shutdown is
  active, the four `lamp-voice` refuses to start beside; makes
  `/run/lampos-bus` root's with the parts' group, 2770 with setgid, as on
  lamp-4ace). The two "Found, not mine" items, which the first builder gave
  back to task 23. Gate on the Mac and in the Linux arm64 build: green,
  `cargo test -p lamp-keeper` 18 passed.
- On lamp-52e6: installed as the keeper's `bin/service` and `lamp-os`
  restarted: ready again in 7.5 s; `/run/lampos-bus` went from `root:root`
  2770 to `root:orangepi` 2770 with setgid; `legacy_stopped` empty, since
  nothing of the old runtime was running here any more.
- Not shown: a lamp that still runs the old software (none here).
- Kind: changes only what the keeper does when it is the updater's
  service; nothing on lamp-4ace.

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
| fifth-P1 | `builder-with-a-lamp` | Not a task: how a builder with its own lamp proves a result and when a piece is taken into `lamp-voice` without running it again on lamp-4ace (off unless set, with a test that it is unchanged when off). Written so that later builders with lamps work the same way. | [builder-with-a-lamp.md](builder-with-a-lamp.md); work-split.md, if taken | |
| fifth-P2 | `lamp-owner-file` | Not a task: so that any number of builders and lamps can tell from Git alone who owns a lamp and what it runs: a `LAMP.md` of three lines in each `deploy/<lamp>/`. | [builder-with-a-lamp.md](builder-with-a-lamp.md), "Which lamp is whose"; one file per lamp folder | |
| fifth-P3 | `tasks-as-issues` | Not a task: track the builders' tasks as GitHub issues (one per task, `slot:` and `status:` labels, questions as comments) instead of one 1,400-line file every builder reads each loop. At the fifth builder's person's request. Make no issue or label until the owner says yes. | [tasks-as-issues.md](tasks-as-issues.md); work-split.md would shrink to the rules and loops | |

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
