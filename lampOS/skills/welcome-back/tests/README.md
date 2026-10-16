# Welcome back's tests

**Run** with `lamp-skills test lampOS/skills/welcome-back` (or `cargo test -p
lamp-skills`, which runs every skill's cases).

| Case | How it was made | What it holds | Result (`lamp-skills`, `lamp-voice` `e933a95c4`) |
|---|---|---|---|
| `back-after-ten-minutes` | Synthetic, `lamp-bus pub --at-us` on a private bus (#938) | Nobody near for ten minutes (a face across the room at 0.10), someone sits down at 600 s; away and back within and after the cooldown | Pass: a hello at 603 s and 2,403 s, nothing else |
| `there-at-start-then-talking` | Synthetic, as above | Someone there from the start; back 8 s after the lamp spoke (no hello), then back after a quiet stretch | Pass: one hello, at 903 s |
| `back-at-the-desk-while-the-room-talks` | **Recorded on lamp-06c1**, 2026-10-16 17:50:00–17:58:00 (#995), `lamp-bus record humans/faces/tracked voice/state`, cut to the window | The person comes back to the desk at 17:56:39 (402 s in; face 0.21 to 0.31 of the picture's height) while the office talks near the lamp (`voice/state` busy every 10 to 60 s: requests at 17:56:27 and 17:56:32) | Pass: nothing. With the `[[while]]` taken out it fails with the hello at 402.4 s (gaze at the face, size 0.28; warm light; the greeting): the quiet half minute is the only thing that holds it |

The recorded case is what the skill does today in a busy office: the chance
of an arrival is spent when the room is not quiet at that moment, so nobody
is greeted. A `wait` on the trigger (#1089) would keep the chance until a lull;
if Welcome back takes one, this case's `expect.toml` becomes a hello at the
first quiet half minute after 402 s.

The recordings hold boxes and ids, no pictures, and no `voice/heard`.
