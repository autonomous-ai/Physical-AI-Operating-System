# Stuffy air's tests

**How they were made.** Not on a lamp: each case's `.jsonl` files were written
by `lamp-bus record` on a private bus while its messages were published with
`lamp-bus pub ... --at-us` at the case's times (as Welcome back's were, #938),
faces at least every 2 s where a condition reads them (the runtime's face
lifespan). Re-recording a case on a lamp, with its owner's yes, replaces the
synthetic one.

**Run** with `lamp-skills test lampOS/skills/<name>` (or `cargo test -p
lamp-skills`, which runs every skill's cases).

| Case | What it holds | Result (lamp-skills, #950 at `a0b5f1a47`) |
|---|---|---|
| `acts-when-stuffy-ten-minutes` | CO2 1,300 ppm for 25 minutes (every 30 s), a face every second | Pass: dims once, at 10 minutes |
| `quiet-on-a-short-rise` | CO2 1,300 for 4 minutes, then 900 to minute 15, a face every second | Pass: nothing |
| `quiet-when-nobody-there` | CO2 1,300 for 25 minutes, no face | Pass: nothing |
| `once-per-stuffy-spell` | CO2 1,300 for 50 minutes, a face every second | Pass: once, at 10 minutes |
| `quiet-when-paused` | Not recorded: waits for the Policy's gate (#939) | Not run |

`quiet-when-paused` waits for the Policy's gate (#939), the one gate for
the pause: `lamp-skills test` skips the ask, so that case will fail until
the gate is in the replay. A `[[while]]` on `lamp/pause` would do it now, but
would keep the skill silent on a lamp where nothing publishes the pause.
