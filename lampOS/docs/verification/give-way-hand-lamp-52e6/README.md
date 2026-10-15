# Giving way to a hand on lamp-52e6: the protocol (#703)

Written before the test, for the person at the lamp and the builder at the
Mac. About 15 minutes. The builder starts and stops the body and the
recording; the person only pushes and says what they did. Nothing but
numbers is kept: no picture, no sound.

## Before

- The builder installs the body with `--give-way return` (a drop-in on
  `lamp-body.service`, the recorded options otherwise), says "it can move
  now", and starts `record.sh start` on the lamp.
- The person keeps a kitchen scale at hand and pushes through it where
  asked, reading the grams out when the joint starts to move.

## The pushes

Each push is about three seconds, firm but never hard enough to fear for
the lamp; the person lets go and waits five seconds before the next. The
builder marks each with `record.sh mark "<what>"`.

| # | Lamp | Push |
|---|---|---|
| 1 | standing, still | head down, then head up |
| 2 | standing, still | head to the left, then to the right (roll) |
| 3 | standing, still | base round each way |
| 4 | standing, still | arm down at the elbow, then up |
| 5 | standing, while it speaks or looks around | head down; base round |
| 6 | folded, at rest | head up from the base; base round |
| 7 | standing | hold the head still against a glance for 5 s (a stall) |

Then the same seven with `--give-way stay`, and then with the V1 rule
(each joint lets go by itself, then holds again; `fifth/give-way`,
`493199861`), each after the builder says it has restarted.

## What is read afterwards

For each push and each mode: the force (grams) at which it moved; the
joint's load and how long it stayed high; whether the joint held, sprang
back, stayed where it was put or came back gently, and after how long;
the hottest temperature. From these: which mode ships, and with what
settings.

## After

`record.sh stop`; the body goes back to its recorded unit; the three
files in `~/fifth-give-way-hand/` are summarised into numbers here.

## Results, 2026-10-15

Three runs with the person at the lamp and the builder at the Mac, about
08:00 to 11:00. Loads are of 1,000 (lamp-52e6 runs `--force-limit 600`,
so 600 is the most a joint can read); angles are the most a joint moved
in the window, in degrees, from `body/joint_states`. Numbers only: the
three files of each run are in `~/fifth-give-way-hand-1`, `-2` and
`~/fifth-give-way-hand` on the lamp, not here.

| Run, body | Mode | Push | Peak load | Moved | What the lamp did |
|---|---|---|---|---|---|
| 1, recorded (`852880034`) | `return` | head down, then up | pitch 212 | pitch 23° | gave way up, back about 30 s later; down it met the floor of its window |
| 1 | `return` | head roll each way | roll 248 | roll 15° | gave way, then **the run ended on tracking** (roll 45 counts past where it gave) |
| 1 | `return` | base each way | yaw 216 | yaw 25° (its own glances) | **did not give way**: 38 counts by hand read 120, under its 220 bound |
| 1 | `return` | arm down, then up | elbow 600 | shoulder 56°, elbow 46° | the lamp sat down; then **the run ended on tracking** (base 51 off); third end in ten minutes, no body until restarted |
| 2, #761 (`0727a1e94`) | `return` | head roll each way, held 3 s | roll 264 | roll 19.5° | gave way six times, the run went on; let go, back to what Expression and the gaze wanted |
| 2 | `return` | base hard to one side, held 3 s | yaw 476 | yaw 12° (30° in the window) | gave way; let go, it turned to the face in view (gaze at its 480 limit), not to where it was |
| 2 | `return` | arm pressed down 3 s | elbow 540 to 600 | shoulder 53°, elbow 46° | the lamp sat down and rested ("cannot hold itself up"); the run went on |
| 2 | `return` | folded elbow lifted, held 3 s | elbow 360 | elbow 103 counts | **held where the hand left it** (goal 2,172 under torque); the next start refused to stand, elbow 99 counts off its way. Fixed in `16a95b6b6` |
| 3, #761 (`16a95b6b6`) | `stay` | head lifted 2 s | pitch 132 to 160 | pitch 6° | gave way, but **came back**: a hand on this head reads 120 to 130, under the 240 bound a hand must pass for a second to count as one |
| 3 | `stay` | base turned 2 s | yaw 384 | yaw 12° | gave way and stayed |
| 3 | `return` + `--overload release` | base hard, held 3 s | yaw 600 | yaw 17° | **gave way within a second; no slump** (release needs 480 for 1.5 s) |
| 3 | `--overload release` alone | base hard, held 3 s | yaw 388 | yaw 24° | **no slump**: the run ended on load (base 232, eased and still loaded); the restart 15 s later was refused, the hand still on it (308) |

In all three runs: no joint grew warm. Of 1,126 arm temperature reads in
the body's log today, 1,112 were 55 °C or under; the 14 others were single
reads (68 to 130 °C) between normal ones, never two in a row, so the
temperature stop (50 reads in a row) never came near.

**What this shows.**

- `--give-way return` (with #761) keeps the lamp's body through every push
  tried: roll, base, arm, held or not. Without #761 two of four pushes
  ended the run, and one left the lamp with no body.
- Coming back goes to what the lamp wants then (a face in view, an
  expression), not to where it was before the hand: that reads as alive.
- `stay` works for the base; for the head it needs the hand bound lowered
  under `--give-way` (the head is light: 120 to 160 by hand).
- `--overload release` cannot trip on the base as built: with give-way the
  joint gives way first; without it the load stop ends the run first. The
  base never read more than 600, and only for under a second.

**Not shown:** `stay` for the head with a lower hand bound; lamp-4ace,
whose joints read other loads.
