---
name: desk-tutor-marks
description: Call only after check_my_work, once you have seen the page, just before you answer: verdict right when every line you could read is right, wrong when you are about to point to a line that goes wrong. The lamp nods or tilts its head. Never when you could not read the page, and never for anything else.
---

# Desk tutor's marks

Desk tutor's fourth step, from the owner (#941): "a small nod where a step
is right, a curious tilt where it's wrong". Desk tutor (`desk-tutor`) leans
over the page, lights it and takes one look; only the model knows, once the
picture comes, whether the work is right. So the verdict is a second tool,
`show_verdict`, which the model calls with `right` or `wrong` just before it
answers, and the lamp shows it with the body while it speaks:

- **right:** `acknowledge`, V1's short nod (about 2 s).
- **wrong:** `curious`, V1's tilt (3 s).

It is not called when the page could not be read: the lamp says what it
cannot read, and a nod or a tilt would claim a verdict it does not have.

The head stays over the page afterwards, as Desk tutor holds it for 2
minutes (#1098), so a follow-up is still over the page; it comes back up
when the page's light is given back.

## If asked about it

"When I check your work, I nod if it's right and tilt my head if something
looks off, then tell you where."
