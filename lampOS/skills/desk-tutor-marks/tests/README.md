# Desk tutor's marks' tests

Started by its tool (`show_verdict`) alone, which the model calls after Desk
tutor's look and before it answers. A case says the call with
`[[call_tool]]` in `expect.toml`; neither needs a recording of the room.

| Case | What it holds |
|---|---|
| `a-step-right-is-nodded-at` | `verdict = "right"`: the `acknowledge` nod and the tutoring note, at once |
| `a-step-wrong-gets-a-tilt` | `verdict = "wrong"`: the `curious` tilt and the tutoring note, at once |

Not shown on a lamp: whether the model calls it before its answer and with
the right verdict, how long that call delays its first words, and how the
nod and the tilt look over the reading pose. That is the lamp check.
