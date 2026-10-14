# Turning to a call by default, lamp-52e6, 2026-10-15

`lamp-body` from the ARM64 run 38613233031 (commit 765893260, SHA-256
`9ccbbf94…7c15`) put on lamp-52e6 at 01:44 through a drop-in that only
changed the program's path: the recorded unit's options, with no
`--look-at-name`. Its journal at start: `lamp-body: it turns to a call, from
name/heard`, which it says only when the call is taken.

Then, for this trial only and taken away afterwards: the voice with
`LAMP_BUS=on` and `LAMP_BUS_AUDIO=on`, and `lamp-name` as a transient unit.
Two lines said aloud from the Mac beside the lamp with
[`call_runner.py`](../look-at-name-lamp-52e6-20261014/call_runner.py) (two of
its three cases), nobody in view; each was a real turn. `voice-calls.jsonl`
has the runner's times (lamp wall clock, unix ms, mapped within 13 ms). The
body did not publish `body/joint_states` (no `LAMP_BUS` on its unit), so the
body's times are its journal's.

| Line | `name/heard`, after "Hey Lamp" ended | Lamp then | `gesture called`, after `name/heard` | Base yaw read, one way and the other |
|---|---|---|---|---|
| "Hey Lamp, what time is it in Tokyo?" | 954 ms | folded | 5,622 ms: it rose (stand 0.00 to 1.00 in 6 s) and glanced once up | — (journal every 0.5 s missed it) |
| "Hey Lamp, what color is the sky at sunset?" | 991 ms | standing | 9 ms | 2,082 and 1,993 (+45, -44 counts from 2,037) |

Both calls heard, both taken, one glance each. The folded lamp's glance came
once it stood, as `docs/body.md` says, which was while it was already
answering.

Afterwards lamp-52e6 runs its recorded units again (body `852880034d1b…`, no
drop-ins) and `lamp-name` is stopped.
