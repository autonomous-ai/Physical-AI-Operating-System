# Physical skills

A physical skill is something the lamp can do in the room: it notices
something with its senses and answers with its body, its light and its
voice. A skill made for Claude Code or Codex acts in software; a lamp skill
acts in the physical world. This document is the design, settled with the
owner on 2026-10-16 between 06:45 and 07:40.

The owner's direction, in order:
- Build the most capable skill first; safety and security for skills from
  other people come later, before anything is published.
- Owners build skills for their own lamp first, by talking to it. Publishing
  to the world comes much later.
- Prove the framework with a first set of skills that work well. Many
  behaviours built into V2 should become skills: "greeting users when they
  are back at their desk as a physical skill rather than an ad hoc
  implementation".
- Use the existing standards (Agent Skills' `SKILL.md`, MCP for tools)
  rather than inventing a format.

Decided the same morning: developers may publish skills from launch, listed
only after our review, and skills are free at launch. Neither happens before
the first skills below work.

## What V1 did

V1 had 30 skills in `autonomous-os/skills/`, each a `SKILL.md` with an
optional `skill.json` naming the capabilities it needs (`presence`,
`motion`, `light`, `expression`, `audio`). Read on 2026-10-16:

| V1 | Verdict |
|---|---|
| `SKILL.md`, the same format as OpenClaw and Claude | Keep |
| `skill.json` capabilities: one skill runs on any body that has them | Keep |
| The model reached the hardware by writing `[HW:/led/off]` into its reply, or `curl` to the hardware server; regexes repaired the markdown the model mangled | Drop: skills call services and read topics |
| Every sensing event, schedule and skill update went into the one agent conversation as text (`[sensing:presence.enter] …`); prose in the lamp's character decided which skill answered | Drop: a skill's trigger is a subscription to a topic, decided in code |
| Thresholds and cooldowns written in prose for the model to compare (wellbeing's 45-minute hydration, a music cooldown left at its development value) | Drop: numbers live in the skill's data and the runtime applies them |
| The OS owns polling, thresholds and cooldowns; context is computed before the model sees it (the environment skill, `[wellbeing_context]`) | Keep: V1's cleanest contract |
| The ambient loop and the agent both wrote the LEDs; a lock was pushed through monitor events | Fix: requests carry who and how important, and the part that owns the light decides |
| `skill-creator`, with runs with and without the skill and a grader | Keep, taught the lamp's topics and services |
| `make push-skill`: live on the next conversation, no reboot | Keep |
| A store (catalogue, browse, one-tap install, publish by pull request) | Later |
| No physical skill had a test | Fix: every skill carries tests on the lamp's own recordings |

## What V2 has

Senses are **topics** on the bus ([bus.md](bus.md), [msgs.md](msgs.md));
actions are **services**, one JSON line in and one out on a socket.

Topics a skill can read today: `humans/faces/tracked` (faces, a passing id,
whether turned to the lamp), `voice/heard`, `voice/said`, `voice/state`,
`room/state`, `name/heard`, `controls/touch`, `controls/gesture`,
`ears/level`, `ears/sudden`, `air/state`, `schedule/due`, `tasks/status`,
`body/joint_states`, `body/hand`, `light/state`, `policy/decisions`,
`lamp/pause`, `mood/state`.

Services: the actions part (light and volume, and the schedule tools;
`/run/lampos-actions/call.sock`), Schedule, Tasks (work handed to Claude
Code or Codex), Memory, the eyes (`permit.sock` for the camera, `who.sock`
for faces it knows), the body (`mood.sock`: six gestures and a mood;
`gaze.sock`).

What is missing for skills:
- **No way to ask the voice to say something.** Each unprompted line (the
  hello, reminders, task news) is wired inside the voice by hand.
- **No arbitration.** The ring shows the newest light asked for, whoever
  asked; the body merges its inputs by fixed rules. bus.md's table says so
  ("Several requesters, one actuator: No").
- **`camera/image` is declared and published by nobody.**
- **The body plays only its six built-in gestures**; a skill cannot bring a
  move of its own.
- Expression's catalogue is data, but its triggers are a fixed list in code.

## The design

### A skill is a folder

```
welcome-back/
  SKILL.md        the standard: name, description, and how the lamp talks about it
  skill.toml      what it needs, its triggers, its steps, its tools
  moves/          body clips (data, played within the body's limits)
  lights/         ring patterns
  sounds/         short sounds
  tests/          recordings of the lamp's bus and what the skill should do in each
  run.py          optional: code for logic the data cannot say, given the bus
```

`skill.toml` for the first skill:

```toml
needs = ["eyes", "voice", "body", "light"]

[[trigger]]
topic = "humans/faces/tracked"
when = "appears"            # a face where there was none
absent_for = "5m"           # after at least five minutes with nobody
cooldown = "20m"

[ask]                       # anything the lamp starts itself asks the Policy first
policy = "hello"

[[do]]
body = { look_at = "face" }
light = "warm_glow"
say = "They are back at their desk. Greet them in one short line, by name if you know it."
```

The numbers are in the data, never in prose. The runtime applies them.

### The schema, as `lamp-skills` reads it

The seventh builder wrote this on 2026-10-16 (#938) from the code in
`crates/skills`. It includes the fields the lamp skill creator proposed (Ten,
#947, `skill-creator/references/skill-folder.md`) that the runtime took.
Where the design above says nothing, the runtime does the smallest thing, and
each such choice is marked *(chosen)*. A skill that breaks any rule here is
not loaded and the program says why in its journal. The other skills still
load.

**The folder.** The folder's name is the skill's name: 1 to 64 lowercase
letters, digits and hyphens. A folder whose name begins with a dot is passed
over.

**`SKILL.md`** begins with Agent Skills' front matter between two `---`
lines. It has two required fields: `name:`, which is the folder's name, and
`description:`. The rest is for the model and for people; `lamp-skills` does
not read it.

**`skill.toml`.** A key not listed here refuses the skill *(chosen: a typo
must not leave a step silently undone)*.

| Key | Required | What it is |
|---|---|---|
| `needs` | no | The parts it uses (`eyes`, `ears`, `air`, `voice`, `body`, `light`, ...). Kept, not checked yet |
| `[trigger]` or `[[trigger]]` | yes | When the skill runs: one table, or several, each checked on its own |
| `trigger.topic` | yes | The topic it listens to |
| `trigger.field` with one of `equals`, `above`, `below` | yes, except on `humans/faces/tracked` | What a message must show for the trigger to hold. A number is compared as a number, anything else exactly. On `humans/faces/tracked` without them, it holds while a face is in view |
| `trigger.when` | no | `present` (the default): it holds, and has held for `for`. `appears`: it holds where it did not just before, once each time. `leaves` and `changes` are refused for now |
| `trigger.for` | no | It has held this long |
| `trigger.gap` | no | A dropout shorter than this does not break `for` |
| `trigger.absent_for` | no | `appears` only: at least this long before, it did not hold. At the program's start nothing has been away: someone there at the first message is not greeted |
| `trigger.rearm` | no | `leaves` or `ends`, which mean the same: once each time it holds, and again only after it stopped. Without it, `present` runs at every message once `for` has passed, as often as `cooldown` lets it |
| `trigger.cooldown` | no | Not again within this long of the last run. A run the Policy refused does not count |
| `[[while]]` | no | Conditions that must also hold, all of them, on the newest value of each topic while that value lasts: a `topic` with `field` and one of `equals`, `above` or `below`, or `humans/faces/tracked` with `present = true` |
| `[ask] policy` | yes | The Policy's question asked before anything is done *(chosen: required, since every trigger is something the lamp starts itself)* |
| `[[do]]` | at least one | The steps, in order. Within one table: body, then light, then say |
| `do.body.look_at` | no | Only `"face"`. The largest face in view goes to the body's `gaze.sock` as the eyes send one (`{"face":{"x","y","size","turned":0}}`) |
| `do.body.gesture` | no | One of the body's six gestures (`nod`, `headshake`, `greeting`, `laugh`, `shy`, `stretching`), sent to `mood.sock` as `{"gesture":NAME}` |
| `do.light` | no | A file in `lights/`, written as its name, or a table in place. A file `lights/NAME.toml` holds the same keys. `{ colour, level, for }` asks the actions part for `light_colour`. *Not carried out yet:* `level` (1 to 5) and `for`. `{ dim = N, until = "ends" }` asks for `light_dimmer` N times. *Not carried out yet:* `until` |
| `do.say` | no | A note for the voice to say in its own words. A stub until the voice's say service (#939) |

A duration is a whole number and one of `ms`, `s`, `m` or `h` (`"5m"`).

The ask is the only gate. The runtime adds no conditions of its own, such as
the pause or someone speaking: per the design above, the Policy is the one
gate for everything the lamp starts itself (#939). Until that gate exists, a
skill that must keep quiet in those cases says so with a `[[while]]`.

**`moves/`, `sounds/`** may exist and are not read. The body plays only its
six gestures, nothing plays a sound yet, and `body.move` is refused. A
**`tools`** key in `skill.toml` is refused until `lamp-skills` writes tools
beside the actions part's `tools.json`. A **`run.py`** refuses the skill,
because nothing runs it yet.

**`tests/`** has one folder for each case. A case holds the `.jsonl` files
`lamp-bus record` wrote and an `expect.toml`. The expect file lists, as
`[[expect]]`, every call the case may make, in order, or says `none = true`:

    [[expect]]
    at = "600s"                # optional: when, from the case's first message
    within = "1s"              # optional; without it, exactly
    call = "body.gaze"         # body.gaze, body.gesture, light or say
    args = { look_at = "face" }

A call matches when its name is the same and each argument named in `args`
is the same. Arguments it leaves out are not compared. The arguments the
runtime gives are:

- `body.gaze`: `look_at`, `x`, `y`, `size`
- `body.gesture`: `gesture`
- `light`: `colour`, `level`, `for`, or `dim`, `until`
- `say`: `note`

A case is replayed in the program, not on a bus. Every message of every file
goes in the order of its `at_us` (the publisher's clock), with the ask
skipped. Any call not listed fails the case. A case with no recording yet is
counted and not run.

`lamp-skills test SKILL_DIR` loads a skill and runs its cases.
`cargo test -p lamp-skills` loads every folder in `lampOS/skills/` and runs
its recorded cases, so CI checks every skill. To make a case, record the
topics while the thing happens, or publish it by hand with
`lamp-bus pub ... --at-us`. Welcome back's two cases were made that way:

    lamp-bus record skills/welcome-back/tests/CASE humans/faces/tracked --seconds 40 --root R &
    lamp-bus pub humans/faces/tracked latest '{"faces":[]}' --type lamp_msgs/Faces --at-us 0 --root R

### `lamp-skills`, a new part

Its own process, its own owner, like every part
([build as components](architecture.md)):
- loads every skill folder and listens to the topics its triggers name;
- applies the conditions, timers and cooldowns;
- before anything unprompted, asks the Policy "may the lamp do this now?",
  so Jev is the one gate for everything the lamp starts itself;
- runs the steps by calling the services;
- publishes `skills/events`: triggered, asked, answered, done, with why.

As built on 2026-10-16 (#938, `crates/skills`, not yet run on a lamp):

    lamp-skills [--dir DIR] [--bus-root DIR] [--gaze-socket PATH] [--mood-socket PATH] [--actions-socket PATH]
    lamp-skills test SKILL_DIR...

- It loads every folder in `--dir` (`/var/lib/lampOS/skills`), looks at the
  folder every two seconds, and loads them all again when a file other
  than a test changed. Cooldowns are kept across a reload.
- It subscribes once to each topic that a trigger or a `[[while]]` names,
  and to `humans/faces/tracked` for a skill that looks at a face, and keeps
  each topic's newest value. A look uses only a list of faces whose lifespan
  still lasts; with none, the other steps run without it. Time comes from its own monotonic
  clock, read when a message arrives.
- **The ask.** The Policy's gate is not built yet (#939). With
  `LAMP_SKILLS_ASK=skip` it answers yes without asking; without it every
  run is answered no and nothing is done. `answered` says which.
- **The steps.** `look_at` and `gesture` are datagrams to the body's
  `gaze.sock` and `mood.sock` (default `/run/lampos-body/`), never waited
  for. `light` is a `light_colour` call (or `light_dimmer` calls) to the
  actions part's `call.sock` (default `/run/lampos-actions/call.sock`). Each
  call is waited for up to two seconds. `say` is a stub: nothing is
  spoken.
- `done` says how each step went: `look at the face: told the body`,
  `light warm: done`, `say: a stub, nothing spoken (...)`, or why not.

Off unless installed: `deploy/units/lamp-skills.service` is a template no
lamp's folder carries. Nothing in the voice changed: the voice's own hello
(`speak_up.rs`, Arrival) still greets where it is switched on, and two
hellos would come on a lamp that ran both.

What is not done, and why:

- **No say service.** Until the voice has one (#939), Welcome back looks and
  warms the light and says nothing.
- **The look is one sight.** The body's gaze takes a sight only while the
  lamp listens or speaks, three in a row, so one datagram does not turn the
  head by itself; with the eyes' own stream it turns as the hello is
  spoken. A move the body plays on request is Third's (move files).
- **The light stays warm.** `light_colour` is the same call a person's "make
  it warm" makes; nothing gives the light back after the hello. A request
  that lapses is Third's arbitration.
- **Arrival's other rules are not in the skill**: a face nearer than 0.14 of
  the picture's height, there for three seconds, and nothing said for half
  a minute. The doc's example has none of them; Welcome back follows the
  example. `for = "3s"` exists for the second; the others need a condition
  that does not exist yet.

### Four additions to V2

1. **A say service in the voice**: `say {skill, note or text, priority,
   expires}`. The voice speaks it as a written turn through the gates it
   already has (pause, a person speaking, a conversation under way). The
   hello, reminders and task news move onto it.
2. **Arbitration at the parts that own the body, the light and the
   speaker**: every request carries its source, a priority and a lease that
   lapses unless renewed (Expression's leases already work this way). The
   order: a person (their voice, their hand) over safety, over the
   conversation, over skills, over idle life.
3. **The eyes publish `camera/image`** (a file and when it was taken), so a
   skill can look.
4. **The body plays move files** from a skill, within its own speed and
   force limits.

### Tools for the model

A skill may declare tools: a service call or a function in `run.py`.
`lamp-skills` writes them beside the actions part's `tools.json`, which the
voice reads at every session open, so a new skill reaches Gemini without a
change to the voice. A skill's description is always in the lamp's
instructions; its body is loaded when it is used.

### Reflexes stay in the body and Expression

A tap on the head, a startle at a bang, giving way to a hand must answer in
under 150 ms. They stay built into the parts that own the body and the
light. A skill may add a clip for a reflex as data. Behaviours that unfold
over seconds are skills.

### Building one

"Lamp, learn to …": the lamp hands the request to Claude Code with the lamp
skill creator (V1's `skill-creator`, taught the topics, the services,
`skill.toml` and the tests). Claude Code writes the skill, tests it on the
lamp's own recordings (`lamp-bus record` and `replay`), installs it, and the
lamp shows the owner, who keeps it or asks for a change. Owners can also
build in Claude Code or Codex on their own computer; a change reloads live
on the lamp. A "My skills" page in the lamp's app (on or off, what each did
and when) comes once there are more than a handful. The store comes after
launch.

## V1's moves and emotions: what we take

The owner, 08:00: "evaluate carefully to pick the good stuff from V1, not
everything." All 29 of V1's recorded moves (`hal/recordings/*.csv`) were
converted the way the first six were (offsets from the standing pose,
scaled into the body's reach, slowed under its speed caps), built into an
evaluation body on lamp-4ace, played one at a time between 08:02 and 08:21,
and photographed from the iMac five times each, about 2.4 s apart (the
sheets are in `~/lamp-camera/review/v1moves-a/sheets/` on the office
iMac). Stills that far apart miss a quick move; the size and length below
are from the converted data, and the owner sees the short list live.

Criteria: does a person read it at once; would a living thing do it; is it
visible at the size the body allows; is it short enough to be a gesture
(three seconds or so) rather than a performance.

| V1 move (V1's meaning) | Length, size | Seen | Verdict |
|---|---|---|---|
| nod, headshake, greeting, laugh, shy, stretching, idle | 3 to 10 s | In V2 since 14 Oct | Keep (already ported) |
| acknowledge ("got it", micro-nod) | 8 s as recorded | Small | Keep, cut to its first nod (about 1.5 s) |
| curious (tilts, looks around) | 10 s | A clear lean to one side | Keep the tilt only (about 3 s); the looking around goes |
| happy_wiggle (good news) | 13 s | A clear sway, the most readable move of all | Keep, cut to about 4 s |
| sad (droops slowly) | 10 s | A clear droop, head low for seconds | Keep as **caring**, for empathy only, about 5 s |
| shock (jolts back) | 9 s | A clear recoil | Keep as **surprised**, smaller; a bang still gets the built-in single glance |
| confused (head tilt side to side) | 20 s at this size | Tilted and lowered | Keep, cut to about 3 s: the lamp's "Sorry?" (#930) |
| sleepy (droop with head catches) | 7 s | A small droop | Keep for Bedtime |
| excited (vertical bounce) | 17 s | Barely visible at this size | Try cut to 3 s for Task news "done"; drop if it still reads as nothing |
| listening (lean forward, head cock) | 4 s | Small | Drop: V2's listening pose already does it |
| thinking_deep | The recording does not move | — | Drop: V2 looks away while thinking |
| goodbye (wave) | 3.5 s, ends folded | — | Drop: it ends in another pose; the lamp folds by itself when left |
| fear | 13 s, big head roll | Little | Drop: fear is not something a desk companion should show |
| playful | 12 s | Little; no clear meaning | Drop for now; games later |
| scanning (looks around) | 18 s | A clear turn | Keep for when a person asks ("look around for my keys"); never on its own (CLAUDE.md rule 8) |
| music_groove, music_hiphop | 18 s | A big dip and rise, clearly a dance | Keep groove for Dance; hiphop is nearly the same recording, drop |
| music_hype | 21 s | Fast (blurred stills) | Keep for Dance, energetic songs |
| music_chill, music_waltz, music_jazz | 10 to 12 s | A sway, small | Keep chill and waltz for Dance; jazz to be judged live |
| music_classical, music_rock | 11, 23 s | Little | Drop unless the live look says otherwise |

V1's emotion table (`skills/emotion/SKILL.md`) paired each move with a
light. Kept as named expressions, with the light toned to the lamp's own
eye rather than V1's colours: **happy** (happy_wiggle, a warm sparkle),
**curious** (the tilt, the eye's arc toward the person), **caring** (sad's
droop, a slow warm breath), **surprised** (shock, one quick brightening),
**sleepy** (sleepy, dimming), **acknowledge** (the short nod, a brief
brighter eye), **confused** (the tilt, a soft pulse). V1's rainbow,
coloured blinks per emotion and the "speaking wave rainbow" are dropped:
they read as a gadget, not a creature.

## V2's own behaviours: skill or reflex

The owner, 08:00: skills are the fundamental part of how the lamp works;
what was built one-off becomes skills. Each behaviour V2 has today, and
where it goes:

| Behaviour today | Where it lives now | Becomes |
|---|---|---|
| Hello when someone comes back to the desk | The voice (`speak_up.rs`), asking the Policy (#883) | Skill: Welcome back |
| A reminder or timer coming due | The voice's reminder consumer | Skill: Reminders (setting one stays a voice tool) |
| News of a task handed to Claude Code or Codex, and progress news | The voice (`task_news.rs`), the Policy's progress act | Skill: Task news |
| Sleep at night, waking in the morning | Expression's catalogue (`alive`) | Skill: Bedtime and Morning, later |
| Turning to a voice, looking away to think, back before its first word, nods while speaking | Voice, body, Expression | Built in: the conversation, under a second |
| A tap on the head, a hand held on it, three seconds to sleep | Controls, Expression, voice (#711, #849) | Built in: a reflex, under 150 ms |
| One startled glance at a bang | Expression (`Sudden`) | Built in: a reflex |
| Giving way to a hand, rising when spoken to, folding when left alone, idle shifts | Body | Built in |
| Eye contact (the head follows a face) | Eyes and body | Built in |
| Taking a look when asked what it sees | A voice tool (#836) | Stays a tool; skills may call it |
| Light, volume, timers, memory, faces, hand-overs | Voice tools and services | Stay tools and services; skills call them |
| The privacy switch and the self-test | Voice, eyes, self-test | System, never a skill |

Built-in gestures (the nod on "yes") use the same move and expression
library as skills.

## The first skills

| Skill | Trigger | What it does | Replaces |
|---|---|---|---|
| Welcome back | A face arrives after 5 minutes with nobody | Policy; turns to them, the light warms, a short hello, by name if known | The hello (#883, `speak_up.rs`) |
| Reminders | `schedule/due` | Looks at the person, the ring pulses, says it | The reminder consumer in the voice |
| Task news | `tasks/status`: done or failed | Done: a small glad move and says so; failed: slumps, the ring goes red, says so | Task news in the voice (`task_news.rs`) |
| Stretch break | Someone at the desk for 50 minutes | Stretches itself as an invitation, never a lecture (creature.md) | New |
| Stuffy air | CO2 high for 10 minutes | Droops a little and dims a shade; mentions it only when spoken to (creature.md) | New |

The first three prove that built-in behaviour moves onto the framework; the
last two that a new skill needs no new code.

## Who builds what

| Piece | Builder |
|---|---|
| `lamp-skills`, the skill format and its tests; Welcome back | Seventh |
| The say service; the Policy as the gate; Reminders and Task news as skills | Second |
| Arbitration at the body and the light; move files | Third |
| The lamp skill creator, and Stretch break and Stuffy air written with it | Ten |
| Each piece on lamp-4ace as it lands | First |

Target: Welcome back running as a skill on lamp-4ace on 2026-10-17; all
five by 2026-10-19.

## Later

Permissions per sensor and actuator for skills from other people, signing,
review and the store: after the first skills work, before anything is
published.
