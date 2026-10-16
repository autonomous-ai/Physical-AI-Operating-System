---
name: lamp-skill-creator
description: Create, test and install physical skills for an Autonomous lamp (lampOS V2) - things the lamp does in the room by noticing something with its senses and answering with its body, light and voice. Use when someone says "Lamp, learn to ...", asks to teach the lamp a new behaviour, or to change, test or install one of its skills. Not for software-only skills for Claude Code or Codex.
---

# Lamp skill creator

A physical skill is something the lamp does in the room: it notices
something with its senses (topics on the lamp's bus) and answers with its
body, its light and its voice (services). This skill helps an owner make one
for their own lamp, test it on the lamp's own recordings, and install it.

The design is `lampOS/docs/skills.md`. Read [references/senses-and-actions.md](references/senses-and-actions.md)
before writing anything: a skill can only use what is listed there.

The person is usually the lamp's owner, not a programmer. They said "Lamp,
learn to ..." out loud; the lamp handed it to you. Speak plainly. Never ask
them about topics, TOML or JSON; ask about what they want the lamp to do,
when, and how it should feel in the room.

## The process

1. **Capture what they want** (below). One or two questions at most, then a
   short description back to them in their words.
2. **Check it can be done** with today's senses and actions. If it needs a
   sense or an action the lamp does not have, say so plainly and offer the
   nearest thing it can do. Never invent a topic or a service.
3. **Write the skill folder** ([references/skill-folder.md](references/skill-folder.md)).
4. **Test it on the lamp's own recordings** ([references/testing.md](references/testing.md)):
   at least one recording where it should act and one where it must not.
5. **Install it** ([references/installing.md](references/installing.md)). It is live at once;
   no restart.
6. **Tell them** in two sentences what it will do and when, and that they can
   say "Lamp, stop doing ..." to turn it off.
7. **Iterate** on what they say after living with it.

## 1. Capture what they want

Get these, from what they said first and as few questions as possible:

- **When**: what in the room starts it. Translate their words into a sense:
  "when I sit down" is a face appearing; "if it gets stuffy" is CO2 high;
  "when I've been at it too long" is a face present for a long time.
- **What it does**: with the body, the light, the voice. Prefer the body and
  the light; speak only if they asked for speech or it is the point.
- **How often at most**: every skill has a cooldown. If they did not say,
  choose one that a person would find natural, not mechanical, and tell
  them.
- **Who it is for**: anyone at the desk, or one known person.

## 2. The lamp is a creature, not a machine

Every skill is judged by what a person in the room sees and hears, not by
whether its trigger fired. Before writing one, ask: would a person or a pet
do this here?

- **Small and rare.** Unprompted behaviour is a glance, a droop, a shade of
  light. Never a lecture, never a scan of the room, never the same line
  twice in a row.
- **Body and light before voice.** A lamp that keeps talking unasked is
  tiring. Speech in an unprompted skill is the exception, said once, short.
- **Silence and stillness are valid.** When in doubt, do less.
- **Never pretend to sense what it cannot.** Say what the lamp actually
  noticed.
- **A person wins.** A person speaking or touching the lamp always comes
  first; the runtime enforces it, so never fight it from a skill.
- **Numbers live in `skill.toml`**, never in the prose of a `say`: the runtime
  applies durations and cooldowns, the model never compares numbers.

## 3. Write the skill folder

```
<name>/
  SKILL.md      name, description, and how the lamp talks about it
  skill.toml    needs, triggers, ask, do, tools
  tests/        recordings of the bus and what the skill should do in each
  moves/ lights/ sounds/   optional data
```

The format and every field: [references/skill-folder.md](references/skill-folder.md).
Use the name the owner would use, in lower case with hyphens:
`stretch-break`, not `ergonomic-reminder-v2`.

No code unless the data truly cannot say it. A `run.py` is optional in the
design, but the first skills prove a new skill needs none; do not write one
without saying why.

## 4. Test

A skill is not finished until it has passed on the lamp's own recordings:

- one recording in which it **must act**, and the calls it must make;
- one in which it **must not** (too soon after the last time, nobody there,
  a person talking to the lamp), and that it makes none.

How to record and replay, and the test file: [references/testing.md](references/testing.md).
If no recording exists for a case, record one on the lamp (with the owner's
yes) or say which case is untested. Never claim a test you did not run.

## 5. Install

Copy the folder to the lamp's skills folder; the runtime reloads it live.
[references/installing.md](references/installing.md) says how, and how to take it back out.
Ask the owner before touching their lamp, and say what will move or light up.

## 6. Tell them, and listen

Tell the owner, in their words, what the lamp will now do and when, and how
to stop it. After they have lived with it, ask what felt right and what felt
odd; change the numbers first (how long, how often), the behaviour second.

## When this skill is wrong

If something here is not true of the lamp (a topic renamed, a field the
runtime refuses), the lamp's own `lampOS/docs/skills.md` and the runtime's
error are right and this file is stale: follow them, and say what was wrong
so the skill creator can be mended.
