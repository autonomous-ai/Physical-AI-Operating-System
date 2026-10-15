# The keeper's pause on lamp-52e6, 2026-10-15 (#701, pull request #767)

Shown by the fifth builder from the Mac at 11:08 to 11:14, nobody at the
lamp. Binaries of the green ARM64 run 38675666653 (`b9fccef52`): the body
`b6e9915abeeb…` run from a drop-in with lamp-52e6's recorded options and
nothing else, the keeper `c2a4dd715eda…` run by hand from
`/opt/lampOS/trial/keeper-pause/` (`LAMP_BUS_ROOT=/run/lampos-bus`). The
lamp's own keeper (`lamp-os`, release `0.2.0-fifth`) kept running. The body
was put back on its recorded unit afterwards.

## Pause and resume: shown

| Step | Seen |
|---|---|
| body started from the #767 build | "it may stand", active |
| `lamp-keeper pause` (11:09:08) | exit 0; `lamp/pause` kept `{"paused":true}`, source `keeper` |
| the body's `diagnostics` | `state: paused`, `mood: resting` |
| `lamp-keeper resume` | exit 0; `lamp/pause` `{"paused":false}` |
| the body's `diagnostics` | `state: ready`, `mood: resting` |

That a mood, a look or a call is not acted on while paused was not tried:
nobody was there to give one. The host tests cover it.

## `stop lamp`: the keeper's part shown, the body's not

`sudo lamp-keeper stop lamp` at 11:09:43 returned in 0.10 s with exit 0.
The voice already said it was resting, so there was nothing to wait for.
`lampos.target`, the body, the voice and the eyes were stopped in the same
second, and `lamp/pause` was cleared afterwards (`{"paused":false}`).
`systemctl start lampos.target` brought every part back (`lamp-keeper
status`: all active, the body's build `b6e9915abeeb`).

**The body did not fold or report.** It was standing (`standing 1.00`).
systemd said "Stopping" at 11:09:43.005 and "Deactivated successfully" 45
ms later. There was no report line and no fold. A recorded body stopped
the same way takes 8 to 15 s to fold and prints `"stop":{"kind":"cancelled"}`
(10:51, 10:52, 10:59, 11:08 and 11:11 today). The body built from
`lamp-voice` itself (`ffc2859bc`, run 38673492097, `a6c4dbe0e388…`) does
the same as #767's: stopped at 11:13:14.363, gone at 11:13:14.382, no
report. So this is not the pause. It came onto `lamp-voice` with
`895177f31` ("every lampOS part puts its log lines through the relay").

**Why:** `log_relay::guard()` is the first line of `main`, and it spawns
the relay thread there, before the body blocks SIGTERM and SIGINT in its
main thread to read them from a signalfd. The relay thread keeps both
unblocked. The kernel gives a process-directed SIGTERM to a thread that
does not block it, so it reaches the relay thread, where the default
action ends the whole process at once. The body therefore no longer
eases back or folds before it leaves, and it never prints its report.
Every part that reads its signals from a blocked mask is affected the
same way. The fix belongs in the relay thread: block every signal at its
start, or have each part block the signals before `guard()`.
