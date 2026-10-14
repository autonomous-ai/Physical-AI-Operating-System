# The body on lamp-52e6, 2026-10-14: raw readings

What [body.md](../../body.md), "lamp-52e6", and the fifth builder's log say,
as the lamp gave it. Nothing here was edited.

- `moods-run1-installed-build.log`, `moods-run2-torque-build.log`: the body's
  journal while a script on the lamp sent it moods on its socket as the voice
  does (one datagram every 40 ms: listening 6 s, thinking 4 s, speaking 5 s,
  resting 4 s, three times; then rest until it folds, listening until it
  stands, speaking with a nod, rest until it folds; the second run three more
  exchanges after that). The first on the build installed that morning
  (`3837b82c…`), the second on `22e595c4…` with `--torque on`.
- `torque-off-without-option.json`, `torque-off-with-option.json`: the body's
  reports, yaw and roll found with torque off, without and with `--torque on`.
- `fr-*.csv`: one servo's goal, position and load (`t_ms,goal,pos,load`,
  sampled about every 16 ms) with the body stopped and the lamp folded, the
  goal stepped or ramped by 40 or 60 counts and back, at the body's own
  servo profile (speed 1000, acceleration 60), by a measuring script outside
  this repository.
- The gaze figures were found from two pairs of 640 by 480 frames of the
  lamp's own camera, which are not kept here: the office and its people are
  in them. Their shift was found by phase correlation and checked by eye.
