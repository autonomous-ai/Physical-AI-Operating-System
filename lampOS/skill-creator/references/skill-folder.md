# The skill folder

```
<name>/
  SKILL.md      the standard (Agent Skills): name, description, how the lamp talks about it
  skill.toml    what it needs, its trigger, its conditions, the Policy's question, its steps
  tests/        a folder for each case: recordings and expect.toml
  lights/       optional: named light steps (lights/NAME.toml)
```

The runtime (`lamp-skills`) reads this; its schema is in the lamp's
`lampOS/docs/skills.md`, "The schema, as `lamp-skills` reads it", and that is
the truth. **A key the runtime does not know refuses the whole skill**, and
the runtime says why in its journal: write only the keys below.

Refused for now, so never write them: `run.py`, a `tools` key,
`body = { move = ... }` (move files, #940), `when = "leaves"` or
`"changes"`. `moves/` and `sounds/` may exist but are not read.

## `SKILL.md`

```markdown
---
name: stretch-break
description: When someone has been at the desk a long time without getting up, the lamp stretches once, as an invitation to take a break; never a lecture.
---

# Stretch break

What the lamp does and why, in two or three sentences, as the owner would
say it. How the lamp talks about it if asked ("I stretch when you've been
sitting a long time; tell me to stop if you'd rather I didn't").
```

`name` is the folder's name: lowercase letters, digits and hyphens. The
description is always in the lamp's instructions; keep it to one line.

## `skill.toml`, a whole example

```toml
needs = ["eyes", "body"]            # the parts it uses: eyes, ears, air, voice, body, light, ...

[trigger]                           # what starts it
topic = "humans/faces/tracked"      # on faces, no field: it holds while a face is in view
when = "present"                    # present (the default) or appears
for = "50m"                         # it has held this long
gap = "2m"                          # a dropout shorter than this does not break it
rearm = "leaves"                    # once each time it holds; again only after it stopped
cooldown = "90m"                    # never again within this long

[[while]]                           # conditions that must also hold, all of them
topic = "voice/state"
field = "state"
equals = "resting"                  # not in the middle of a conversation

[ask]                               # required: the Policy's question before anything is done
policy = "nudge"                    # hello | news | nudge

[[do]]                              # steps, in order
body = { look_at = "face" }

[[do]]
body = { gesture = "stretching" }
```

### The trigger (`[trigger]`)

| Key | Meaning |
|---|---|
| `topic` | The topic it listens to (only those in [senses-and-actions.md](senses-and-actions.md)) |
| `field` with `equals`, `above` or `below` | What a message must show: `field = "co2_ppm"`, `above = 1200`; `field = "state"`, `equals = "resting"`. A number is compared as a number, anything else exactly. **Required on every topic but `humans/faces/tracked`** |
| `when` | `present` (the default): it holds, and has held `for`. `appears`: it holds where it did not just before |
| `for` | It has held this long |
| `gap` | A dropout shorter than this does not break `for` (a face lost as the person looks away) |
| `absent_for` | With `appears`: it did not hold for at least this long before |
| `rearm` | `leaves` or `ends` (the same): once each time it holds, again only after it stopped. This is "don't nag": once per sitting, once per stuffy spell. A chance held back by a condition, the cooldown or the Policy is spent |
| `cooldown` | Not again within this long. Always set one |

Durations: a whole number and `ms`, `s`, `m` or `h` (`"10m"`).

### Conditions (`[[while]]`)

Each is a `topic` with `field` and `equals`/`above`/`below`, or
`topic = "humans/faces/tracked"` with `present = true` ("someone is at the
desk"). All must hold, on each topic's newest value while that value lasts
(a face lasts 2 s). The runtime adds no conditions of its own: the pause and
a person speaking are the Policy's gate (#939).

### Asking first (`[ask]`)

Required. `policy` names the kind of thing: `hello` (greeting someone), `news`
(telling them something), `nudge` (an unprompted suggestion, spoken or not).

### Steps (`[[do]]`)

In order. Within one table: body, then light, then say.

| Key | Meaning |
|---|---|
| `body = { look_at = "face" }` | Turn to the largest face in view |
| `body = { gesture = "..." }` | One of `nod`, `headshake`, `greeting`, `laugh`, `shy`, `stretching` |
| `light = { colour = "...", level = 1..5, for = "4s" }` | Carried out: the colour. **Not carried out yet**: `level` and `for`, and nothing gives the person's light back afterwards |
| `light = { dim = 1, until = "ends" }` | Carried out: dimmer by N steps. **Not carried out yet**: `until`; the dim stays |
| `light = "NAME"` | The same keys from `lights/NAME.toml` |
| `say = "..."` | A note for the voice to say in its own words, never a script or a number to compare. A stub until the voice's say service (#939): nothing is spoken yet |

Until the light can be given back (the third's arbitration and leases, #940),
a light step changes the person's own light for good: leave light steps out,
or say plainly in `SKILL.md` that the skill must not be installed yet.

## `tests/`

See [testing.md](testing.md).
