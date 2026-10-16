# What a skill can sense and do

Senses are topics on the lamp's bus; actions are services. Only what is
listed here exists. The source of truth is the lamp's own `lampOS/docs/msgs.md`
(each message's fields) and `lampOS/docs/bus.md` (who publishes what); if
they disagree with this file, they are right.

## Senses (topics)

| Topic | What it says | Good for |
|---|---|---|
| `humans/faces/tracked` | The faces in view: a passing id, where, whether turned to the lamp. Said a few times a second while the camera is open | Someone arrives, is there, leaves; looks at the lamp |
| `voice/heard`, `voice/said` | What a person said, what the lamp said (text) | Reacting to words; rarely needed: the voice already answers |
| `voice/state` | Resting, listening, thinking, speaking | Keeping out of a conversation |
| `room/state` | Unknown, quiet, busy | Doing nothing in a busy room |
| `name/heard` | The lamp's name was heard, how sure | |
| `controls/gesture`, `controls/state` | A touch gesture on the head or base; button holds | Answering a touch |
| `ears/level`, `ears/sudden` | The room's loudness every 100 ms; a sudden sound | Quiet hours, a bang |
| `air/state` | Every 5 s: `co2_ppm`, `pm2_5` (and `pm1_0`, `pm4_0`, `pm10`), `humidity`, `temperature` (inside the base, warmed by the lamp: not the room's), and `feel`, the lamp's own judgement from the CO2: `fresh` (under 800 ppm), `fine` (to 1,200), `stuffy` (to 2,000), `very_stuffy`, `unknown`. Any of the numbers can be missing | Stuffy air: trigger on `feel`, the lamp's own word, rather than a number of your own |
| `schedule/due` | A reminder or timer is due | Reminders |
| `tasks/status` | How work handed to Claude Code or Codex is going | Task news |
| `body/joint_states`, `body/hand` | Where each joint is; a hand pushing the lamp | Not moving against a hand |
| `light/state` | The light the person asked for | |
| `mood/state` | The lamp's mood | |
| `lamp/pause` | The lamp is paused: do nothing | Every skill honours it (the runtime does) |
| `policy/decisions` | What the Policy decided about speaking first | |

## Actions (services)

| Service | What it does |
|---|---|
| **say** (the voice) | The lamp says something, as a written turn through the voice's own gates (a person speaking, a conversation under way, the pause). Give it a note for the model ("They are back. Greet them in one short line."), not a script. Being built (#939); until then the runtime stubs it |
| **light** (the actions part) | The lamp's light: on/off, one of ten colours, a level 1 to 5. A skill's light is a short change on top of the person's own light, never a replacement of it |
| **body: gaze** | Look at a face, or a direction |
| **body: gesture** | One of the body's six gestures: `nod`, `headshake`, `greeting`, `laugh`, `shy`, `stretching`. There is no droop, lean or bow yet: those come as move files |
| **body: moves** | A move file from the skill's `moves/`, played within the body's limits (being built, #940) |
| **schedule** | Set, list, cancel reminders and timers |
| **tasks** | Hand work to Claude Code or Codex |
| **memory** | What the lamp remembers |
| **eyes: who** | Whose face is at the lamp, if known |

The camera's picture (`camera/image`) is not published yet: a skill cannot
look at the picture itself, only at the faces the eyes report.

## Names in tests

A test names each call by its service: `say`, `light`, `body.gaze`,
`body.gesture`, `body.move`, `schedule`, `tasks`, `memory`. The runtime's
own test runner (#938) is the truth if it names them otherwise.

## The light's colours

`yellow`, `red`, `green`, `blue`, `cyan`, `purple`, `orange`, `pink`,
`white`, `warm`; levels 1 (dimmest) to 5.

## Never

- Never write to `light/command` (that is the person's light) or to a device.
- Never call a service the lamp does not have; never invent a topic.
- Never put a number for the model to compare in a `say` note.
