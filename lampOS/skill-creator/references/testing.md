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
declaration first. Record every topic the skill's trigger and conditions
name, and `voice/state` and `lamp/pause`. A case lasts as long as it needs
(a cooldown case may be two hours: replay plays it in seconds), but holds
nothing private: faces are boxes and ids, never pictures; leave
`voice/heard` (what someone said) out unless the skill needs it, and then
ask the owner.

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
at = "50m"                 # about when, in recording time, within `within`
within = "1m"
call = "body.gesture"      # the service, as named in senses-and-actions.md
args = { gesture = "stretching" }
```

The `[[expect]]` entries are **all** the calls the case may make, in order:
any other call fails it, so "exactly once" needs nothing more. A case where
the skill must do nothing has no `[[expect]]` at all, only:

```toml
none = true
```

## The cases every skill needs

- **It acts**: the recording in which it must act, and what it calls.
- **It does not act**: at least one of nobody there, too soon after the last
  time (the cooldown, or before it re-arms), a person talking to the lamp,
  the lamp paused. Talking and the pause are the runtime's own gates: their
  cases check the runtime keeps the skill out (record `voice/state` showing
  `listening` or `speaking` at the moment it would act, or `lamp/pause`), and
  say whether it acts once they end.

Run them with the runtime's test command on the folder (`lamp-skills test
<folder>`, #938; `lamp-skills --help` says whether it exists). Until it
does, say so in `tests/README.md`: which cases exist, which recording each
needs, and that none has run. Do not write a test runner of your own.

## Recording a case that does not exist yet

Ask the owner first, say what will be recorded and for how long. Prefer
recordings the lamp already has; ask the person beside the lamp only for
what needs a person (sitting down, a hand).
