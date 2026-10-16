# The skill folder

```
<name>/
  SKILL.md      the standard (Agent Skills): name, description, how the lamp talks about it
  skill.toml    what it needs, its triggers, whether it asks first, its steps, its tools
  tests/        recordings of the bus and what the skill should do in each
  moves/        body clips (data, played within the body's limits)
  lights/       ring patterns
  sounds/       short sounds
  run.py        optional, only for logic the data cannot say
```

The schema is the runtime's (`lamp-skills`, #938) and its document is the
truth; this is the shape from `lampOS/docs/skills.md`. Fields marked
*proposed* are this creator's suggestion until the runtime takes or renames
them: if the runtime refuses one, follow the runtime.

## `SKILL.md`

```markdown
---
name: stretch-break
description: When someone has been at the desk a long time, the lamp stretches as an invitation to take a break.
---

# Stretch break

What the lamp does and why, in two or three sentences, as the owner would
say it. How the lamp talks about it if asked ("I stretch when you've been
sitting a long time; tell me to stop if you'd rather I didn't").
```

The description is always in the lamp's instructions; keep it to one line.

## `skill.toml`

```toml
needs = ["eyes", "body"]          # parts it uses: eyes, ears, air, voice, body, light, schedule, tasks

[[trigger]]
topic = "humans/faces/tracked"
when = "appears"                  # a face where there was none
absent_for = "5m"                 # only after this long with nobody
cooldown = "20m"                  # at most once in this long

[ask]                             # anything the lamp starts itself asks the Policy first
policy = "hello"

[[do]]
body = { look_at = "face" }
light = "warm_glow"
say = "They are back at their desk. Greet them in one short line, by name if you know it."
```

### Triggers

| Field | Meaning |
|---|---|
| `topic` | The topic it listens to (only those in [senses-and-actions.md](senses-and-actions.md)) |
| `when` | `appears` (something where there was nothing), `leaves`, `present` (*proposed*: there, continuously) |
| `absent_for` | Only after this long without it |
| `for` | *Proposed*: only once it has lasted this long (`present` for 50 minutes; a value above a threshold for 10 minutes) |
| `field`, `above`, `below` | *Proposed*: a number in the message and a threshold (`field = "co2_ppm"`, `above = 1000`) |
| `cooldown` | At most once in this long. Always set one |

Durations are strings: `"30s"`, `"10m"`, `"2h"`.

### Asking first

Anything the lamp starts by itself asks the Policy (Jev) first: `[ask]` names
the kind of thing (`hello`, `news`, `nudge`). The Policy can say no (someone
asked it not to speak in the mornings, a conversation is under way). A skill
that only moves or lights quietly may skip `say` but still asks.

### Steps (`[[do]]`)

| Field | Meaning |
|---|---|
| `body` | `{ look_at = "face" }`, `{ mood = "<one of the body's six gestures>" }`, or `{ move = "moves/<file>" }` once move files land |
| `light` | A named change on top of the person's light, for a while: `{ colour = "warm", level = 2, for = "3s" }` (*proposed* shape) |
| `say` | A note for the model, in plain words, that tells it what happened and how to answer: never a script, never a number to compare |
| `until` | *Proposed*: hold the step until the trigger's condition ends (a droop while the air is stuffy) |

## `tests/`

See [testing.md](testing.md).
