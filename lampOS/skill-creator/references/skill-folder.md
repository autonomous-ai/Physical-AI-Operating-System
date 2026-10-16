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

Refused for now, so never write them: `run.py`, a `tools` key (the key is
`tool`, below), `body = { move = ... }` (move files, #940), `when = "leaves"`
or `"changes"`. `moves/` and `sounds/` may exist but are not read.

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
| `field` with `equals`, `above` or `below` | What a message must show: `field = "co2_ppm"`, `above = 1200`; `field = "state"`, `equals = "resting"`. A number is compared as a number, anything else exactly. **Required on every topic but `humans/faces/tracked`**. Only a field at the top of the message: one inside another (`schedule/due`'s `oldest.due_unix_ms`) cannot be named |
| `when` | `present` (the default): it holds, and has held `for`. `appears`: it holds where it did not just before |
| `for` | It has held this long |
| `gap` | A dropout shorter than this does not break `for` (a face lost as the person looks away) |
| `absent_for` | With `appears`: it did not hold for at least this long before |
| `rearm` | `leaves` or `ends` (the same): once each time it holds, again only after it stopped. This is "don't nag": once per sitting, once per stuffy spell. A chance held back by a condition (unless it has a `wait`), the cooldown or the Policy is spent |
| `cooldown` | Not again within this long. Always set one |
| `wait` | With `rearm` or `appears`: if a `[[while]]` holds it back (someone talking), it waits this long while the trigger still holds and acts once the conditions hold, rather than giving up its turn. A creature waits for a pause: `wait = "10m"` |

Durations: a whole number and `ms`, `s`, `m` or `h` (`"10m"`).

### Conditions (`[[while]]`)

Each is a `topic` with `field` and `equals`/`above`/`below`, or
`topic = "humans/faces/tracked"` with `present = true` ("someone is at the
desk"). All must hold, on each topic's newest value while that value lasts
(a face lasts 2 s). The runtime adds no conditions of its own: the pause and
a person speaking are the Policy's gate (#939).

### Asking first (`[ask]`)

Required with a trigger. `policy` names the kind of thing: `hello` (greeting
someone), `news` (telling them something), `nudge` (an unprompted suggestion,
spoken or not). A skill started by its `tool` has none: a person asked.

### Started when a person asks (`tool`)

When the owner wants the lamp to do something **when they ask for it**
("check my work", "do a little dance"), the skill has a `tool` in place of a
trigger: the model calls it when what the person says matches the skill's
description.

```toml
needs = ["eyes", "voice", "light"]
tool = "check_my_work"              # a lowercase letter, then lowercase letters, digits and _, at most 48

[[do]]
light = { colour = "white", for = "20s" }
look = true                         # the voice's full look at what is in front of the lamp
say = "Tutor: a hint or a question, never the answer."
```

- The `SKILL.md` description is what the model reads to decide when to call
  it: say **when to call**, in the person's words, at most 1,024 bytes.
- No trigger, no `[ask]`. Its `[[while]]` conditions still hold it back.
- **A tool takes no arguments.** "Focus for 25 minutes" cannot pass the 25:
  a number the person says is for the voice's own tools (a timer is the
  voice's `set_timer`), not a skill's.
- One `[[do]]` list: a skill cannot do one thing when asked and another when
  something later happens. That is two skills, and the second needs a
  trigger that can tell its moment apart (see `schedule/due` in
  [senses-and-actions.md](senses-and-actions.md)).
- `say` in a tool's run is not spoken by the say service: it goes back to
  the model with the call's answer, as what to do next. `look = true` (only
  in a skill with a tool and no trigger) has the model say "Let me take a look." and the
  voice hand it the picture.
- While its last run is still under way (a light's `for`), a call again
  gets the look and the say again and holds the run's poses again, but
  nothing more is made: the light already on stays and is given back at its
  `for`. Only a skill whose steps are all body and light refuses it then
  ("I'm still doing that.").

### Steps (`[[do]]`)

In order. Within one table: body, then light, then say.

| Key | Meaning |
|---|---|
| `body = { look_at = "face" }` | Turn to the largest face in view |
| `body = { gesture = "..." }` | One of `nod`, `headshake`, `greeting`, `laugh`, `shy`, `stretching` |
| `body = { mood = "reading", for = "20s" }` | A pose held for `for` (more than 0, at most `2m`): `reading` is the only one, the head down over the desk and the arm leaning forward (#1011). The run lasts while it is held. Both keys or neither |
| `light = { colour = "...", level = 1..5, for = "4s" }` | The colour, then stepped to `level` from the level shown; `for` holds it in a run with no say. Given back when the run ends |
| `light = { dim = 1, until = "ends" }` | Dimmer by N steps, held until the run ends (the say ends, or its longest `for`), then given back |
| `light = "NAME"` | The same keys from `lights/NAME.toml` |
| `say = "..."` | A note for the voice to say in its own words through its say service (#954), never a script or a number to compare. One a skill. The other steps are made when the voice begins the say, so the look and the light come with the words, and none if it never begins. In a tool's run, the note goes to the model instead |
| `look = true` | Only in a skill with a `tool` and no trigger: the voice's full look (above) |

**The light is always given back.** Before a run the light shown is read
from `light/state`; at the end of the run (the say ends, or the run's
longest `for`) it is set back. If nothing said the light in the last 3 s,
the light step is left out: a skill never leaves someone's light changed. A
light lasts at most 10 minutes, so "a calm light while I focus for 25
minutes" cannot be a skill's. Two skills, or a person, changing the light
meanwhile are not arbitrated yet (#940).

## `tests/`

See [testing.md](testing.md).
