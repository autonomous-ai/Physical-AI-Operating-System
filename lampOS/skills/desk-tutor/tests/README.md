# Desk tutor's tests

Desk tutor is started by its tool (`check_my_work`) when someone asks, not by
a trigger, so there is no recording of the room to replay: `lamp-skills
test` has no case to run for it yet.

What its call does is in `lamp-skills`' own tests:
`a_declared_tool_is_in_the_tools_file_and_a_call_runs_the_skills_steps`
calls a skill of Desk tutor's shape twice while the page is lit. Both calls
are answered with the look and the note. The light is set once and given
back once (#1036).

Checked by hand on a laptop with this folder (#1002): it loads, `tools.json`
lists `check_my_work` with the description, and a call answers at once with
"Say only \"Let me take a look.\" now, in those words. When the picture
comes: Tutor, don't solve: …".

Still to come (#941): a case that the tutor never guesses (a blurred still,
run against the model), and the lean over the page while it looks.
