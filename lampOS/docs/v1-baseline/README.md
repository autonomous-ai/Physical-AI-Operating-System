# V1 baseline working notes

These are the detailed notes behind [the V1 brain baseline](../v1-brain-baseline.md).
They describe V1 `main` at `170a1b85e7e6197097019e4175efdcbe416e3c2c`
(12 October 2026). Every path in them is relative to that tree's root.

| File | Area |
| --- | --- |
| [a-voice-frontend.md](a-voice-frontend.md) | HAL capture, wake word and follow-up window, end of turn, guards, dispatch, speaker gate, announcer |
| [b-realtime.md](b-realtime.md) | Realtime orchestrator: providers, Gemini Live, tools and routing, handoff to the main agent, quota fallback, realtime memory |
| [c-os-server.md](c-os-server.md) | os-server: event intake and gates, local intent and Jev, reply streaming, `[HW:…]` markers, ambient loop, schedules, health watch, busy queue |
| [d-runtimes-mind.md](d-runtimes-mind.md) | Hermes and the other runtimes, `SOUL.md`, skills, memory stores, tools, prompt-only decisions |
| [e-body-perception.md](e-body-perception.md) | HAL perception, presence, gaze and tracking, buttons and touch, LED and servo ownership, safety, dead code |
| [f-shipped-baseline.md](f-shipped-baseline.md) | What a Lamp ships, product intent, measured numbers, known failures, cost and limits, marketing claims |

**How far they were checked.**

- Each note was written by a separate code-reading pass and carries a
  file:line reference for each claim.
- Only the claims used in the baseline summary were re-read in the code for
  it. Treat any other claim as a pointer to check, not as verified.
- Items the notes mark "unverified" are unverified.
- Statements about devices, the backend or fleet configuration are limited
  to what this repository shows.
