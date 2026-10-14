# Turning to a call on lamp-52e6, 2026-10-14 (#715)

`lamp-body --look-at-name on` on lamp-52e6, built from `fifth/look-at-name`
(sha256 `0de54b01…55963`), for one trial: the recorded unit with the switch
added and `LAMP_BUS=on` so that the body published `body/joint_states`,
through a drop-in that was taken away afterwards. lamp-52e6 runs no name
listener, so each call was published by hand, as the listener would:

```text
lamp-bus pub name/heard event '{"score":0.9,"ago_ms":200}' --type lamp_msgs/NameHeard
```

Four calls, at 0, 4, 16 and 30 s (`calls-unix-ms.txt`, the lamp's clock
before each publish), while `lamp-bus record` kept `body/joint_states`
(`body_joint_states.jsonl`, every 80 ms, radians; counts below are radians
times 4096/2π). Nobody was in view: the eyes reported no face in any look
that evening, and the camera was closed for most of it. Nobody was by the
lamp, and no real name was said to it (each real turn costs a model's).

| Call | Lamp | Command changed | First joint moved 3 counts | Glance (commanded base yaw) | Base yaw, read |
|---|---|---|---|---|---|
| 0 s | folded, at rest | 78 ms | 1,358 ms (the arm, rising) | 6.80 to 7.60 s after, once it stood | -10 to +35, -62, back to -20 |
| 4 s | rising | — | — | let go: within ten seconds of the last | — |
| 16 s | standing, at rest | 55 ms | 135 ms | 0.14 to 0.94 s after | -8 to +34, -61, back to -21 |
| 30 s | standing, at rest | 44 ms | 204 ms | 0.20 s after | -3 to +34, -61, back to -20 |

The listening pose (the head up 25 to 30 counts) began on the step the call
was taken; the folded lamp rose in its 6 s as it does when spoken to and
glanced once up. The base reached about +40 and -55 counts (3.5 and 5
degrees) of the clip's 60 and came back to 10 to 15 counts short of where it
was, where it rested, as the base on lamp-52e6 does after any move. The
body's own report when the trial ended: `"calls":3`, `"gestures":3`.

An earlier run the same evening, before a call was let replace the shift a
standing lamp makes at rest: the folded lamp's head moved 186 ms after the
call and it glanced once stood, and the call to the standing lamp, which was
shifting, got no glance. Mended in the same commit (test
`a_call_is_glanced_for_over_a_shift_at_rest`).

## Said aloud: "Hey Lamp, ..." from the Mac beside the lamp

The same evening, end to end: `call_runner.py` (here) renders a line with
`say -v Daniel` and plays it with `afplay` from the MacBook beside the lamp,
adapted from the Mac-to-lamp voice kit. For this trial only, through drop-ins
taken away afterwards: the voice with `LAMP_BUS=on` and `LAMP_BUS_AUDIO=on`
(it put its microphone's sound on `mic2/audio`), `lamp-name` built from this
branch running beside it as a transient unit (`--models` the three files of
`docs/name.md`, classifier SHA-256 `1d238ca7…f50f`), and the body as above.
Each line was a real turn: the lamp answered aloud. No sound was kept, and
nobody was in view. Times are on the lamp's wall clock; the Mac's was mapped
to it over one kept SSH connection, to within 13 to 14 ms in the second run
(`voice-calls-2.jsonl`) and only 300 to 333 ms in the first, made before the
connection was kept (`voice-calls-1.jsonl`, its glance window also took in the
look away to think; read its times as rough).

| Line (second run) | Voice listening, after the line began | `name/heard`, after "Hey Lamp" ended | First write for the call, after `name/heard` | Glance began, after `name/heard` | Base yaw read: from, one way, the other, at rest | Voice's end of speech / first sound, after the line ended |
|---|---|---|---|---|---|---|
| "Hey Lamp, what time is it in Tokyo?" | 471 ms | 828 ms | 55 ms | 135 ms | -6, +37, -61, -19 | 782 / 3,023 ms |
| "Hey Lamp, what color is the sky at sunset?" | 452 ms | not heard | — | — | — | 809 / 2,765 ms |
| "Hey Lamp, tell me a very short joke." | 472 ms | 946 ms | 37 ms | 117 ms | -11, +37, -62, -19 | 775 / 2,442 ms |

In the first run the name was heard 551 and 606 ms after it ended, in two
lines of three, and the glance began 93 and 83 ms after it.

What this shows:

- **The voice gets there first.** It goes to listening as soon as speech
  begins, 0.45 s into the line, and the head lifts for that, by the
  voice's own mood as before; the name is heard 0.55 to 0.95 s after "Hey
  Lamp" ends, while the person is still speaking. So on a standing lamp,
  with the voice's microphone open, the call's own part is the glance: the
  body wrote for it 37 to 55 ms after `name/heard`, and the base began to
  turn about 0.1 s after that. The head goal and movement before it were
  the voice's.
- **A call missed is no glance.** The name was not heard in two lines of
  six (the classifier's own rate, `docs/name.md`, is 17 of 20); the voice
  answered them as it always does.
- **How it looked.** Watched from the Mac's camera, which faces the lamp at
  one frame a second: the head lifts and turns as its ring goes to
  listening, and is back as it answers. The one-second glance falls between
  two frames and cannot be judged from them; nothing in them reads as a
  start or a twitch. No frame was kept: people and screens are in them.

## Someone in view

Tried with a face on the Mac's screen, not a person: a public-domain
portrait, full screen, in front of the lamp. The lamp's eyes did not report
it in any look. The Mac is to one side of the lamp's picture and its screen
is small in it: the face filled at most about 15% of the picture's height,
and the eyes take a face from 19.2% (`crates/eyes/src/sight.rs`, someone at
the lamp, not across the room). So a call with somebody in view, where the
gaze should turn the head to their face instead of the glance, is left for a
person at the lamp.
