# A glance from rest on lamp-52e6, 2026-10-14

Task 32 (#710), the fifth builder. The body built from `fifth/rest-glance`
(SHA-256 `dd217f5512812933e7ac14d68f321dc8213c6ee617911a4ef65a711e1c1b992a`)
ran on lamp-52e6 from 18:05 to 18:12 under a systemd drop-in that added
`--rest-glance on` and `LAMP_BUS=on LAMP_EXPRESSION_BODY=on` to the
lamp's recorded unit (`deploy/lamp-52e6/lamp-body.service`). The drop-in
was then taken away and the recorded unit runs again, unchanged.

Nobody was by the lamp. lamp-52e6 runs no Expression and no ears, so the
requests Expression makes for a sudden sound were published by hand on
`expression/body` (`lamp-bus pub ... --type lamp_msgs/ExpressionBody`),
every 20 to 25 ms, each lasting 150 ms, in the shape of lamp-4ace's
"glance at a sound" (`deploy/lamp-4ace/expression-rest-lamp-4ace.json`):
the head 0.07 rad up in 250 ms, 0.03 rad of tilt, held, back by 1.6 s.
The lamp was made to stand by one `listening` datagram on its mood socket
and was otherwise resting.

`journal.txt`: the body's own journal (a line every half second: goals,
positions and loads of head pitch, head roll and base yaw, then the arm),
for the two runs with this build.

| At rest | Head pitch before | Goal at most | Head at most | Back | Roll | Base, arm |
|---|---|---|---|---|---|---|
| Standing | 1,736 | 1,670 (46 asked, 40 led, at `pitch_floor`) | 1,695: 41 counts, about 3.6 degrees, within 0.5 s | 1,733 to 1,740, at rest by 8 s | goal 1 count, position 0 | not written |
| Folded | 1,770 | 1,740 (`pitch_window`) | did not move (load 132) | goal 1,771 | goal 1 count, position 0 | not written |

What that shows: on lamp-52e6 a standing lamp at rest glances at a sound
and settles again, and nothing but the head moves. A folded lamp-52e6
does not: while folded the head's goal may not go above `pitch_window`
(1,740), 30 counts over where this head rests, and from below it does not
start for 30. Widening that window for a folded lamp is not done here.

Seen on the Mac's camera (frames not kept: people and screens were in
them): the head tips up a little and back, as a glance, not a twitch.

Not shown: the glance from Expression itself on a real knock. That is the
first builder's run on lamp-4ace, as #710 says.

Found on the way: with `LAMP_BUS=on` the body said "nothing is published
on the bus: bus topic /run/lampos-bus/diagnostics: Permission denied" on
lamp-52e6 (that folder is root:root 0770), so `body/joint_states` was not
published there and the journal is what was read.
