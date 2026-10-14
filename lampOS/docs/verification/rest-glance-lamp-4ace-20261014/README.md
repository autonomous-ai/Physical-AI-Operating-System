# A glance from rest on lamp-4ace, 2026-10-14

Task 32 (#710, pull request #718), the first builder's run. The body built
from `fc22bb107` for the lamp under emulation (its tests passed there),
SHA-256 `638cb63c39d199b45d17edaef4eacbd5c6d5dbf0d754d49ba2251e61c957ed87`,
installed on lamp-4ace with the recorded unit (`deploy/lamp-4ace/lamp-body.service`).

1. **Switch off, 18:40:08 to 18:41:20.** Started once with the unit as it
   was. It ran without a restart, said it takes Expression's head requests
   and may stand, and the health part reported all 13 parts `ok`.
2. **Switch on, from 18:41:28.** `--rest-glance on` added to the unit.
3. **Played sounds, 18:41:53 and 18:42:33.** A synthetic knock (three
   thuds) and a bang from the Mac's loudspeaker, output volume 65, about a
   metre away. The ears published no sudden sound for either (their
   threshold is a 30 dB rise on the first microphone), so Expression asked
   nothing and the head did not move. A loudspeaker there does not reach
   the threshold; a knock on the desk is for a person to try.
4. **One sudden sound published by hand, 18:43:42.354,** on `ears/sudden`
   (`lamp-bus pub ears/sudden event '{"peak_db":-12.0,"floor_db":-52.0,"loud_ms":60,"short":true,"ago_ms":20}' --type lamp_msgs/SuddenSound`),
   with the lamp folded and resting. Expression's "glance at a sound"
   (`deploy/lamp-4ace/expression-rest-lamp-4ace.json`) asked for 0.07 rad
   of head pitch: 27 of the 697 messages `expression/body` carried in the
   next 12 s held -0.07.

`journal.txt`: the body's journal lines (every half second: goals,
positions and loads of head pitch, head roll and base yaw, then the arm)
from 18:43:40 to 18:43:54, and the first message of `expression/body`.

| Time | Head pitch goal | Position | Load |
|---|---|---|---|
| 18:43:42.102 | 1,952 | 1,952 | 88 |
| 18:43:42.602 | 1,941 | 1,948 | 130 |
| 18:43:43.102 | 1,908 | 1,913 | 127 |
| 18:43:43.602 | 1,913 | 1,903 | 18 |
| 18:43:44.602 | 1,946 | 1,946 | 22 |
| 18:43:49.102 | 1,955 | 1,955 | 42 |
| 18:43:52.102 | 1,951 | 1,956 | 163 |
| 18:43:53.602 | 1,943 | 1,943 | 25 |

What that shows: folded at rest, lamp-4ace's head lifted 49 counts (about
4.3 degrees) and was highest about 1.2 s after the event was published,
and was back within 6 counts of where it rested about a second later. Its
roll and the arm were not written, and the base's goal went on as it did
before the event (a cycle of 2,016 to 2,023 every 2.5 s with the base
still at 2,023, there with the switch off too). Its home pitch is 1,950,
210 counts inside the folded `pitch_window`, so the window did not limit
it here, unlike lamp-52e6.

Seen, and not explained: from about 7 s after the event the resting head
was held 4 to 5 counts from its goal (1,956 against 1,951 and 1,952) with
its load rising from 30 to 163, and at 18:43:52.6 its goal was moved to
1,944 and the load fell to 25. Before the event it rested at 1,952 with a
load of 88. One run; not seen without the switch.

Not shown: a glance at a real sound (no sudden sound was heard from a
loudspeaker), what it looks like (no picture taken), a glance while
standing at rest on lamp-4ace, and more than one event.
