# lamp-52e6's folded head window, 2026-10-14

Task 38 (#719), the fifth builder. Nobody was by the lamp; it was watched
through the Mac's camera beside it (frames not kept).

## The sweeps, 20:30 to 20:35

The body was stopped (`systemctl stop lamp-body`), the lamp folded at rest,
and a measurement script outside the repository moved only the head's
pitch (servo 5): a goal one count further every step, at about six counts a
second (each step waits for a reading), the servo's force limit (48) held
to 500, its acceleration 20 and goal speed 50, all three put back after.
A sweep stopped at once, holding where the head was, on a load over 450
(350 going up), on a head that had moved and then moved under 2 counts in
10 steps while lagging its goal by more than the stall bound, on a lag over
100, or 350 counts from where it began. The servo's own limits read 1,451
and 3,067. Then the body was started again from its recorded unit.

| File | Sweep | What stopped it |
|---|---|---|
| `sweep-down.txt` | 1,791 down (counts rising) | the base: still at 2,009 to 2,013 while the goal went on to 2,029, load 20 rising to 76 |
| `sweep-up-stall-15.txt` | 1,813 up, stall bound 15 | not a stop: a head lifted from below lags its goal 20 to 30 counts before it moves (as waking found), and the bound of 15 took that for one at 1,793 |
| `sweep-up.txt` | 1,811 up, stall bound 90 | nothing: it rose freely to 1,474, lagging its goal 15 to 25 counts at a load of 76 to 120 all the way, until the sweep's reach at the servo's lower limit |

Seen on the camera: at 2,013 the head leans forward onto the base; at
1,474 it faces up and back.

So the window is `--pitch-window 1670,1995`: the top 15 counts short of
where the head met the base, as lamp-4ace's 2,030 is of its 2,045; the
bottom at the head's floor (1,670), which the servo's 1,451 allows.

## The glance from rest, folded, 20:41

The body built from this branch (SHA-256
`852880034d1bf756c3eeb79724ef128419dc114c3d4d95a55145925eeab0eb57`) ran
under a drop-in that added `--pitch-window 1670,1995 --rest-glance on` and
`LAMP_BUS=on LAMP_EXPRESSION_BODY=on` to the recorded unit. The request
Expression makes at a sudden sound (0.07 rad up in 250 ms, 0.03 rad of
tilt, back by 1.6 s) was published by hand on `expression/body`, 69 times
over 1.61 s, as for #718.

`journal-folded-glance.txt` is the body's journal; `joints-folded-glance.jsonl`
is `body/joint_states` meanwhile. The head's goal went from 1,764 to 1,679
(50 asked and the lead), the head from 1,764 to 1,707 within half a second:
57 counts, about 5 degrees. It was back at 1,758 two seconds later. The
roll's goal moved one count; the base and the arm were not written. With
lamp-4ace's window the same request moved nothing (#718's readings).

The drop-in was then taken away and the recorded unit
(`deploy/lamp-52e6/lamp-body.service`, with `--pitch-window 1670,1995` and
this build) installed and started at 20:43.
