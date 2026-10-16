# The skill folder

```
<name>/
  SKILL.md      the standard (Agent Skills): name, description, how the lamp talks about it
  skill.toml    what it needs, its trigger, its conditions, whether it asks first, its steps
  tests/        recordings of the bus and what the skill should do in each
  moves/        body clips (data, played within the body's limits; #940)
  lights/       ring patterns
  sounds/       short sounds
  run.py        optional, only for logic the data cannot say
```

The schema is the runtime's (`lamp-skills`, #938) and its document is the
truth; this is the shape from the lamp's design (`lampOS/docs/skills.md`).
Fields marked *proposed* are this creator's suggestion until the runtime
takes or renames them. If the runtime refuses one (it says so on
`skills/events`), follow the runtime.

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
The folder's `tests/README.md` says which cases are tested and which are not.

## `skill.toml`, a whole example

```toml
needs = ["eyes", "body", "light"]   # parts it uses: eyes, ears, air, voice, body, light, schedule, tasks

[trigger]                           # one trigger: what starts it
topic = "humans/faces/tracked"
when = "present"                    # appears | leaves | present | changes
for = "50m"                         # present: only once it has lasted this long
gap = "2m"                          # proposed: a dropout shorter than this does not break it
rearm = "leaves"                    # proposed: once per occurrence; again only after it has ended
cooldown = "90m"                    # and never more often than this

[[while]]                           # proposed: conditions that must also hold, all of them
topic = "voice/state"
equals = "resting"                  # not while a conversation is under way

[ask]                               # anything the lamp starts itself asks the Policy first
policy = "nudge"                    # hello | news | nudge

[[do]]                              # steps, in order, each finished before the next
body = { look_at = "face" }

[[do]]
body = { gesture = "stretching" }

[[do]]
light = { colour = "warm", level = 2, for = "4s" }
```

### The trigger

One per skill. A skill that needs two things at once has one trigger and
the other as a `[[while]]`.

| Field | Meaning |
|---|---|
| `topic` | The topic it listens to (only those in [senses-and-actions.md](senses-and-actions.md)) |
| `when` | `appears` (something where there was nothing), `leaves`, `present` (there, continuously), `changes` (a field takes a value) |
| `field` | For `changes`, or with `above`/`below`/`equals`: the message's field, by its name in senses-and-actions.md (`co2_ppm`, `feel`) |
| `equals`, `above`, `below` | *Proposed*: the field's value (`equals = "stuffy"`), or a threshold for a number |
| `for` | Only once it has held this long |
| `absent_for` | For `appears`: only after this long without it |
| `gap` | *Proposed*: a dropout shorter than this does not break `present` or `for` (a face lost for a moment as the person looks away) |
| `rearm` | *Proposed*: `leaves` (or `ends`): once per occurrence, again only after it ended. This is "don't nag": once per sitting, once per stuffy spell |
| `cooldown` | The skill as a whole never acts more often than this. Always set one |

Durations are strings: `"30s"`, `"10m"`, `"2h"`.

### Conditions (`[[while]]`)

*Proposed.* Each is a topic, a field and `equals`/`above`/`below`, or a
topic and `present = true` ("someone is at the desk": `humans/faces/tracked`
with `present = true`). All must hold when the trigger fires. The pause
(`lamp/pause`) and a person speaking are the runtime's own conditions; do
not repeat them.

### Asking first

Anything the lamp starts by itself asks the Policy (Jev) first: `[ask]`
names the kind of thing: `hello` (greeting someone), `news` (telling them
something), `nudge` (an unprompted suggestion, spoken or not). The Policy can
say no (asked not to in the mornings, a conversation under way). A skill that
only moves or lights quietly still asks.

### Steps (`[[do]]`)

In order, each finished before the next. One kind of action a step.

| Field | Meaning |
|---|---|
| `body` | `{ look_at = "face" }`, `{ gesture = "<nod, headshake, greeting, laugh, shy or stretching>" }`, or `{ move = "moves/<file>" }` once move files land (#940) |
| `light` | `{ colour = "<one of the ten>", level = 1..5, for = "4s" }`: a change on top of the person's light, for a while, then theirs again; or *proposed* `{ dim = 1 }`: a shade dimmer than the person's own light, keeping its colour |
| `say` | A note for the model, in plain words, telling it what happened and how to answer: never a script, never a number to compare |
| `until` | *Proposed*, on a `body` or `light` step: hold it until the trigger's condition ends, then let go (a droop while the air is stuffy) |

## `tests/`

See [testing.md](testing.md).
