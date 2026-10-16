# Stretch break demo's tests

The demo is `../../stretch-break` with `for = "60s"` and `cooldown = "5m"`,
so that a person sees it without sitting fifty minutes (the owner's session,
2026-10-16 15:00). Its cases replay the real skill's recordings (see
`../../stretch-break/tests/README.md` for how they were made) with what the
demo should do in them:

| Case | Recording | Expected |
|---|---|---|
| `acts-after-a-minute` | the real `acts-after-50-minutes` | looks at the face and stretches once, at about a minute |
| `once-while-they-stay` | the real `quiet-until-they-leave` (3 hours, never leaving) | once at about a minute, never again |
| `across-the-room` | the real `across-the-room` (a face 0.04 of the picture tall, as lamp-4ace saw at 15:16) | nothing |
| `waits-for-the-talk-to-end` | the demo's sitting, `voice/state` talking from 30 s to 2 min 45 s (made by hand from the real recording's shape) | nothing at the minute; one stretch at the first face after the talk, 3 minutes (`wait`, #1021) |
| `someone-at-the-desk` | lamp-4ace, 2026-10-16 16:43:00 to 16:44:30, **made from the journal, not recorded** (the body's face lines as boxes, the voice's turn events as `voice/state`; no words): the owner at the desk, faces 0.15 to 0.24 tall | one look and stretch at 64 s, the first face past the minute (0.15, just over `near = 0.14`). On the lamp it fired at 16:43:53, the sitting having begun at 16:42:41, before the window |
| `quiet-when-paused` | not recorded, as in the real one: waits for the Policy's gate (#939) | not run |

The real `quiet-when-short-sitting` (40 minutes) and `quiet-while-talking`
(talking from minute 48) are about the fifty minutes, and say nothing about a
one-minute demo, so they are not here.
