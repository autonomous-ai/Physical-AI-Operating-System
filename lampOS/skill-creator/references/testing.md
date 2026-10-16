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
[[expect]]
at = "50m"                 # optional: when, from the case's first message
within = "1m"              # optional; without it, exactly at
call = "body.gaze"         # body.gaze, body.gesture, light or say
args = { look_at = "face" }

[[expect]]
call = "body.gesture"
args = { gesture = "stretching" }
```

The `[[expect]]` entries are **all** the calls the case may make, in order:
any other call fails it, so "exactly once" needs nothing more. Arguments left
out of `args` are not compared. The runtime gives: `body.gaze` `look_at`,
`x`, `y`, `size`; `body.gesture` `gesture`; `light` `colour`, `level`, `for`,
or `dim`, `until`; `say` `note`. A case where the skill must do nothing has
no `[[expect]]`, only:

```toml
none = true
```

## The cases every skill needs

- **It acts**: the recording in which it must act, and what it calls.
- **It does not act**: at least one of nobody there, too soon after the last
  time (the cooldown, or before it re-arms), a person talking to the lamp
  (if the skill has a `[[while]]` on `voice/state`). The pause is the Policy's
  gate (#939), which a test skips: a pause case waits for that gate.

Run them with `lamp-skills test <folder>`: each case is replayed in the
program (not on a bus), every message in order of its time, with the
Policy's ask skipped. A case with no recording is counted and not run. Say in
`tests/README.md` which cases exist, how each was made, and which passed. Do
not write a test runner of your own. Before trusting a pass, change the
skill so it should fail (a shorter `for`) and see the case fail.

## Making a case without waiting for the room

A case can be made by publishing its messages with their times into a
private bus while recording it:

```sh
lamp-bus --root /tmp/case-bus record tests/acts-after-50-minutes humans/faces/tracked voice/state &
lamp-bus --root /tmp/case-bus pub humans/faces/tracked latest '{"faces":[]}' --type lamp_msgs/Faces --at-us 0
# ... one pub for each message, --at-us in microseconds from the case's start
kill %1
```

Publish faces at least every 2 s wherever a condition reads them (a face
lasts 2 s), as the lamp's eyes do. Say in `tests/README.md` that the case was
made this way, not recorded on a lamp.

## Recording a case on the lamp

Ask the owner first, say what will be recorded and for how long. Prefer
recordings the lamp already has; ask the person beside the lamp only for
what needs a person (sitting down, a hand).
