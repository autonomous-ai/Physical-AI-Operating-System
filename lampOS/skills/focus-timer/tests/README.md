# Focus timer's tests

**How they were made.** Not on a lamp: each case's `.jsonl` files were written
by `lamp-bus record` on a private bus while its messages were published with
`lamp-bus pub ... --at-us` at the case's times: `schedule/due` every second
(its declared period), as `lamp_msgs/ScheduleDue`, and a face list every
second. Re-recording a case on a lamp, with its owner's yes, replaces the
synthetic one.

**Run** with `lamp-skills test lampOS/skills/focus-timer`.

| Case | What it holds | Result (lamp-skills test, 2026-10-16) |
|---|---|---|
| `acts-when-the-timer-ends` | Nothing due for 60 s, then a timer due for 30 s, then nothing; a face every second | Pass: a look and a nod, at 60 s |
| `once-per-timer` | A timer due from 60 s to 300 s (repeated every second), a face every second | Pass: one look and one nod, at 60 s |
| `quiet-when-nobody-there` | The same timer, no face | Pass: nothing |
| `quiet-when-schedule-is-shadow` | Pending 1 with `enabled = false` (Schedule in shadow mode), a face | Pass: nothing |

Checked that the cases can fail: with `when = "present"` in place of
`appears`, `once-per-timer` failed (a second nod at 180 s, after the
cooldown); without the face condition, `quiet-when-nobody-there` failed.

Not tested: a person talking to the lamp (the skill has no `[[while]]` on
`voice/state`: the voice is itself saying the timer is up as the nod comes);
the pause, which waits for the Policy's gate (#939).
