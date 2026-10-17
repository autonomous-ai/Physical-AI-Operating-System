# Stuffy air on lamp-06c1, 17 Oct 2026: a hand-fed check

jarvis ran Stuffy air (#1066) on lamp-06c1 on the morning of 17 Oct, with
the air published by hand. lamp-06c1 has no working air sensor.

## The build

`lamp-skills` `e64fce06`, built by "lampOS on ARM64" run 38984214932 on
`d449eee60` (#1066's head). The body was `e998bb99`
(`deploy/lamp-06c1/LAMP.md`).

## The air

The air was published on `air/state` by hand, about every 28.6 s. These
times are from `air_state.jsonl`:

| At | `co2_ppm` |
|---|---|
| 07:55:19.1, the first reading | 1,400 |
| 08:06:17.6, the last at 1,400 | 1,400 |
| 08:06:46.3, the first under 1,200 | 750 |
| 08:13:55.7, the last reading | 750 |

## What happened (`skills_events.jsonl`)

- **The trigger:** at 08:05:20, "air/state has held for 601 s". The ask was
  skipped (`LAMP_SKILLS_ASK=skip`).
- **The body:** `sleepy` was told to the body. The body journal shows
  `08:05:20.501 gesture sleepy`, then `08:05:25.921 gesture over`.
  - That is 5.4 s into the 9.2 s clip (`docs/body.md`): it did not play
    whole.
  - At 08:05:25.9 the commanded shoulder, elbow and head pitch jump back by
    about 0.15, 0.19 and 0.15 rad within 160 ms.
  - Then the arm folds, the shoulder from 0.93 and the elbow from 0.85 rad,
    to 0.00 and 0.02 rad by 08:05:32.
  - `voice/state` was `listening`, then `thinking`, from 08:05:22.1 to
    08:05:25.0, inside the gesture.
  - The lamp check has to judge what was seen, a snap and then lying
    down, not the whole clip.
  `body_joint_states_0805-0806.jsonl` keeps the joints from 08:05:00 to
  08:06:00 (750 of the 16,124 messages recorded, chosen by
  `header.at_unix_ms`).
- **The dim:** held until the trigger ended, at 08:07:43.5. That is the
  first reading past the trigger's 1 min `gap` after the last 1,400. Then it was given back:
  "dim: done; held until its trigger ended; light given back to on warm at
  1: it already was". The lamp's light was already warm at level 1, the
  lowest, in all 1,289 messages of `light_state.jsonl`, so the dim could not
  be seen.

`voice_state.jsonl` is the voice's state through the run.

## Not shown

- A real air sensor: the air was fed by hand.
- The look, judged from stills: whether the cut `sleepy` (a snap, then
  lying down) reads as a drowsy lamp.
- The quiet hours at night.
