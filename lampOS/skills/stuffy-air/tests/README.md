# Stuffy air's tests

**How they were made.** Not on a lamp: lamp-4ace's air sensor is stuck at
about 2,000 ppm (#1021, the first builder, 12:29), so there is no real day to
replay. Each case's `.jsonl` files were written by `lamp-bus record` on a
private bus while its messages were published with `lamp-bus pub ...
--at-us` at the case's times, as Welcome back's were (#938). The air is a
series made by hand: a reading every 30 s, CO2 from 700 ppm up to 1,400 in
30 minutes, an hour at about 1,400 (±15), and back down to 700 in 30
minutes. `voice/state` is `resting` from the start unless the case says
otherwise. Each `expect.toml` sets the local time of the case's first
message (`clock`): the skill keeps still from 22:00 to 08:00. Re-recording a
case on a lamp with a working sensor, with its owner's yes, replaces the
synthetic one.

**Run** with `lamp-skills test lampOS/skills/stuffy-air` (or `cargo test -p
lamp-skills`, which runs every skill's cases).

| Case | What it holds | Result (lamp-skills, #1021) |
|---|---|---|
| `once-a-spell` | One spell from 10:00, then 10 minutes at 700 | Pass: `sleepy` and a dim once, at 31.5 minutes (10 minutes after the air passed 1,200); the dim ends at 99.5 minutes, a minute after the air fell under 1,200 |
| `once-across-a-dip-to-1100` | A spell down to 1,100 only, 20 minutes at 1,100, a second spell from 1,100, then 700 | Pass: once, at 31.5 minutes; the dim ends in the dip (110.5 minutes); the second spell does nothing, since the air never stayed under 1,000 |
| `twice-after-good-air` | A spell, 20 minutes at 700, a second spell | Pass: twice, at 31.5 and 171.5 minutes, each dim ending as its air clears (99.5 and 239.5 minutes) |
| `waits-for-the-talk-to-end` | One spell from 10:00; `voice/state` listening at 28 minutes, speaking at 29, resting at 35 | Pass: nothing during the talk, then `sleepy` and a dim once, at the first reading after it, 35.5 minutes (`wait = "10m"`, #1089); the dim ends at 99.5. Was `quiet-while-talking`, when a chance held back was spent |
| `ends-when-the-sensor-goes-silent` | From 10:00, up to 1,400 and held, the last reading at 59.5 minutes; `voice/state` each minute from 60 | Pass: once at 31.5; the dim ends at 74.5, 15 minutes after the last reading (`air/state` names no `expires_ms` yet) |
| `quiet-at-night` | One spell from 23:00 | Pass: nothing |
| `quiet-on-a-short-rise` | From 10:00, up to 1,300 and back: 8 minutes above 1,200 | Pass: nothing |

The pause has no case: it is the Policy's gate (#939), which `lamp-skills
test` skips, as Stretch break's `quiet-when-paused` notes.
