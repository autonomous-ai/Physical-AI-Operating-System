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
