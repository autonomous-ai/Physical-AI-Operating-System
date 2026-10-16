# Testing a skill on the lamp's own recordings

A skill passes when, replayed against what the lamp actually sensed, it does
what it should and nothing else. Never claim a test you did not run.

## Recordings

The lamp's bus can be recorded and played back (`lamp-bus`, on the lamp or
any computer with the bus):

```sh
# On the lamp: record the topics the skill listens to, for as long as the case takes.
lamp-bus record tests/arrives-after-a-break humans/faces/tracked --seconds 900
# Elsewhere, or later: play it back, here 60 times as fast.
lamp-bus replay tests/arrives-after-a-break/humans_faces_tracked.jsonl --speed 60
```

A recording is one `.jsonl` file per topic (`/` becomes `_`), the topic's
declaration first. Keep recordings short: just the case, nothing private
(faces are boxes and ids, never pictures; `voice/heard` is what someone
said, so leave it out unless the skill needs it, and then ask the owner).

## The test file

Each case in `tests/` is a folder with its recordings and `expect.toml`:

```
tests/
  acts-after-50-minutes/
    humans_faces_tracked.jsonl
    expect.toml
  quiet-when-just-done/
    humans_faces_tracked.jsonl
    expect.toml
```

```toml
# expect.toml (proposed shape; the runtime's test runner, #938, decides)
[[expect]]
at = "50m"            # about when, in recording time
call = "body.mood"    # the service the skill must call
args = { mood = "stretch" }

[none]                # or: nothing at all in this case
```

## The cases every skill needs

- **It acts**: the recording in which it must act, and what it calls.
- **It does not act**: at least one of nobody there, too soon after the last
  time (the cooldown), a person talking to the lamp, the lamp paused.

Run them with the runtime's test command on the folder (`lamp-skills test
<folder>`, #938). Until that exists, say plainly that the skill is untested
and which cases it would need; do not write a test runner of your own.

## Recording a case that does not exist yet

Ask the owner first, say what will be recorded and for how long. Prefer
recordings the lamp already has; ask the person beside the lamp only for
what needs a person (sitting down, a hand).
