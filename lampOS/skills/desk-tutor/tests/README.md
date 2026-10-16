# Desk tutor's tests

Desk tutor is started by its tool (`check_my_work`) when someone asks, not by
a trigger. A case says when the model calls it with `[[call_tool]]` in
`expect.toml`, beside the room's recordings.

| Case | Made from | What it holds |
|---|---|---|
| `a-page-held-too-far-is-asked-closer` | lamp-4ace, 2026-10-16 16:47:40 to 16:50:10, **made from the journal, not recorded** (the bus was not recorded then): the body's face lines as face boxes, the voice's turn events as `voice/state`; no words | `check_my_work` at 16 s: the look, the reading pose held 2 min, the white light for 2 min, then the tutoring note. The pose and the look come before any answer. On the lamp the answer asked for the page closer, as it was too blurred to read |

A replayed call is a fresh run each time. What the program does with a call
while the last run is still under way (#1036, #1051: the look and the note
again, the pose held again, nothing more lit) is tested in `lamp-skills`'
own tests, not in a case.

What its call does is in `lamp-skills`' own tests:
`a_declared_tool_is_in_the_tools_file_and_a_call_runs_the_skills_steps`
calls a skill of Desk tutor's shape twice while the page is lit. Both calls
are answered with the look and the note. The light is set once and given
back once (#1036).

Checked by hand on a laptop with this folder (#1002): it loads, `tools.json`
lists `check_my_work` with the description, and a call answers at once with
"Say only \"Let me take a look.\" now, in those words. When the picture
comes: Tutor, don't solve: …".

The tutoring note was run against the model with a blurred and a sharp
page on lamp-52e6 (#1053): it never guessed. The lean is held again on a
repeat call (`a_repeat_tool_call_holds_the_runs_pose_again_and_only_that`,
#1051).
