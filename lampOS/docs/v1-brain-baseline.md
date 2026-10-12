# V1 brain baseline: who decides what the Lamp says and does

Owner instruction, 12 October 2026: study V1 closely before proposing any V2
"brain" (the layer that decides what the Lamp says and does). This document is
that baseline. It states what V1 does, with evidence. It proposes nothing.

- **Source.** V1 is `main` at `170a1b85e7e6197097019e4175efdcbe416e3c2c`
  (12 October 2026), fetched for this study. Paths are relative to that tree's
  root, not to `lampOS/`.
- **Method.**
  - Six code studies were run: voice front-end, realtime orchestrator,
    os-server, runtimes/persona/skills, body/perception, and shipped
    configuration with evidence.
  - Their working notes, with a file:line reference for each claim, are in
    [`v1-baseline/`](v1-baseline/README.md).
  - The claims this summary depends on were re-read in the code.
- **Limits.**
  - Nothing here was measured on a Lamp for this study. The numbers in §10
    are V1's own records, with their dates.
  - Fielded devices may differ from `main`: wake word, hardware variant and
    cloud opt-ins depend on first-boot date and owner settings (§13).
- **Related.** [V1 voice parity](v1-voice-parity.md) covers capture and
  echo; [V1 servo audit](v1-servo-audit.md) covers motion safety.

## 1. Short answer

V1 has no single brain. Four layers decide in turn, and most of the judgement
is prompt text.

| Order | Layer | Runs | Decides |
| --- | --- | --- | --- |
| 1 | HAL (Python) | device | Whether audio becomes a turn: wake word, follow-up window, end of turn, noise guard, echo drop. Also who may speak, presence lighting, gaze and buttons |
| 2 | Gemini Live, `gemini-3.8-live-extended-thinking` | cloud, via Autonomous's proxy | For each admitted voice turn: answer, hand off to Hermes, or reject. Its audio is discarded; ElevenLabs speaks its text |
| 3 | os-server (Go) | device | Local command rules and the Jev classifier, sensing-event gates, reply streaming, `[HW:…]` marker execution, ambient motion, schedules, health recovery |
| 4 | Hermes, model alias `Auto-AI` | device process, model via Autonomous's proxy | Replies to handed-off turns and to every sensing event, physical expression, memory writes, skills, Harness tasks |

Code admits, rate-limits and clamps. Most decisions are prompt text:

- whether speech was addressed to Lamp;
- whether to speak;
- how to react to a sensed event;
- nudge thresholds and cooldowns;
- body language;
- what to remember.

The prompt text comes from three places:

- Gemini's prompt, which also embeds Hermes's persona and memory;
- Hermes's persona, `robots/lamp/SOUL.md`;
- 26 skills.

## 2. Picture

```text
mic ─► HAL voice: VAD, wake/follow-up gate, end of turn, noise/echo guards
          │ admitted turn (audio + Deepgram transcript)
          ▼
       Gemini Live (text out; its audio discarded)
          ├─ answers ────────────► HAL TTS: ElevenLabs ─► speaker
          │                        os-server journals it as [HANDLED] history
          ├─ reject_turn ────────► nothing
          └─ delegate / silent / error / quota
                 │ POST /api/sensing/event
                 ▼
       os-server: local intent rules ─► HAL directly (no model)
                  else Jev classifier ─► fixed actions
                  else gates (sleep, 120 s floor, busy queue) ─► Hermes
                                                                  │
       Hermes (SOUL.md + skills + memory) ─► reply text + [HW:/path:{json}]
                 │
                 ▼
       os-server: stream sentences to HAL TTS; fire markers to HAL :5001 or /api

camera/mic/sensors ─► HAL perception ─► sensing events ─► os-server gates ─► Hermes skills
timers ─► os-server: ambient micro-moves, schedules, health recovery speech
```

## 3. A voice turn, step by step

Standard Lamp, turn mode (the default). Paths are under `hal/drivers/voice/`
unless stated.

1. **Listen.** The mic is read in 64 ms frames (`voice_service.py:929-1049`).
   - Before each step HAL asks os-server for the voice mode.
   - It stops listening to the room when that call fails, when Harness voice
     mode is on, or when tap-to-talk is selected (`hal/drivers/voice/_internal/input_policy.py:31`).
2. **While Lamp speaks or music plays, the mic is read and discarded.**
   There is no spoken interruption; only a tap stops Lamp
   (`voice_service.py:1119-1138`).
3. **Speech start.** Speech starts when all of these hold
   (`voice_service.py:1209-1228`):
   - volume reaches the threshold;
   - WebRTC VAD agrees;
   - a short hold has passed;
   - Silero scores at least 0.15.

   768 ms of pre-roll is kept.
4. **Wake gate.**
   - **Wake word on:** the phrase must start or end the sentence. A partial
     transcript opens the gate with a chime and a blue listening light; the
     final transcript confirms it (`voice_service.py:2484-2509`,
     `:3133-3167`).
   - **No phrase needed in the follow-up window**, 5 s on the Lamp. The
     window opens on:
     - a reply;
     - a tap;
     - a bare "hey lamp";
     - an enrolled face entering view;
     - the boot greeting;
     - gaze toward the Lamp (`hal/drivers/voice/_internal/wakeword_focus.py:136-150`).
5. **End of turn.**
   - It is proposed after 1.2 s of silence, or 0.8 s after a final
     transcript.
   - Local Smart Turn v3 then decides. Hesitation words and greetings wait
     longer (`hal/drivers/voice/_internal/turn_endpoint.py:43-81`).
   - Whether the model file ships on devices is unverified.
6. **Guards.** These are code (`hal/drivers/voice/_internal/session_finalize.py`, `hal/drivers/voice/_internal/realtime_turn.py`):
   - leading words that echo Lamp's last reply are stripped;
   - filler-only or low-voicing short turns are dropped as noise (lamp ratio
     0.45);
   - a bare wake phrase is handled locally.
7. **Gemini decides.** The turn's audio is committed to Gemini Live.
   - **Spoken text:** sent sentence by sentence to ElevenLabs (`eleven_v4_turbo`,
     speed 1.2).
   - **`delegate_to_main`:** the turn goes to Hermes.
   - **`reject_turn`:** silence.
   - **No output:** the transcript goes to Hermes anyway.

   Routing is model choice; the prompt says "Answer directly via voice by
   default" and "When in doubt, delegate"
   (`hal/realtime/resources/system_prompt_gemini.md`).
8. **Dispatch to os-server** (`hal/drivers/voice/_internal/turn_dispatch.py:114-324`):
   - answered by Gemini → `[HANDLED] <transcript> [REPLY] <≤600 chars>`;
   - delegated → `[voice-instruction] … [transcript] …`;
   - no output, error or unavailable → plain transcript;
   - noise, foreign script, cancelled or rejected → nothing.
9. **Waiting.** HAL shows a "thinking" face. If Gemini has produced nothing
   after 0.5 s and the transcript has more than 3 words, it plays a "Hmm…"
   filler (`hal/drivers/voice/_internal/realtime_turn.py:207-251`).
10. **os-server** (`system/server/sensing/delivery/http/handler.go:198-766`):
    1. Local intent: exact canonical commands such as light, volume, stop,
       time and music stop, run without a model. Otherwise the Jev classifier
       may pick a fixed action within 3 s (`system/intent/`).
    2. `[HANDLED]` turns become silent history; Hermes sees them later as
       "History only… Return NO_REPLY".
    3. Delegated turns go to Hermes.
11. **Hermes replies.**
    - The first sentence is streamed early to HAL's speak queue.
    - `[HW:…]` markers fire, then NO_REPLY is checked
      (`system/server/agent/delivery/http/handler_event_agent.go`,
      `system/server/agent/delivery/http/handler_hw.go`).
    - A newer turn takes the speaker; older replies are dropped
      (`hal/drivers/voice/tts/service.py:1048-1100`).
    - With `OS_REALTIME_SUPERSEDES_MAIN_REPLY=1` (set on the Lamp), a newer
      Gemini-answered turn also silences an older Hermes reply. Its hardware
      markers still run.

## 4. Decision points: code or model

| Decision | Rule | Code or model | Lamp value | Where |
| --- | --- | --- | --- | --- |
| Is audio a turn at all | volume, WebRTC VAD, hold, Silero | code | Silero 0.15 | `hal/drivers/voice/voice_service.py:1209-1228` |
| Is it for Lamp | wake phrase, follow-up window, gaze | code | wake on for new configs; window 5 s; gaze 60° | `hal/drivers/voice/_internal/wakeword_focus.py`, `hal/drivers/tracking/gaze.py:852-919` |
| Ignore while speaking | discard mic during TTS/music | code | always | `voice_service.py:1119-1138` |
| End of turn | silence clock, then Smart Turn v3 | code + local model | 1.2 s silence | `hal/drivers/voice/_internal/turn_endpoint.py:43-81` |
| Noise turn | filler-only or low voicing | code | ratio 0.45, 160 ms voiced | `hal/drivers/voice/_internal/realtime_turn.py:433-447` |
| Echo of own reply | ≥0.55 similar within 15 s | code | — | `hal/drivers/voice/_internal/sensing_sender.py:58-74` |
| Answer, delegate or reject | Gemini's choice | **model** | — | `hal/drivers/voice/_internal/realtime_turn.py:697-716` |
| Stay silent | only `reject_turn` or `<no speech>`; plain silence is sent to Hermes | code honours, model chooses | — | `hal/drivers/voice/_internal/realtime_turn.py:354-364` |
| Was it addressed (beyond the gate) | SOUL rule; overheard → exactly `NO_REPLY` | **model (Hermes)** | — | `robots/lamp/SOUL.md` Rules |
| Local command | exact canonical string | code | 21 actions | `system/intent/fast_path.go` |
| Semantic command | Jev: p ≥ 0.90, margin ≥ 0.40, fit ≥ 0.95 | small model, fixed outputs | on | `system/intent/semantic.go`, `system/intent/jev/resolver.go` |
| Pass a sensing event to Hermes | sleep drop, 120 s floor, busy queue, voice window | code | 120 s | `system/server/sensing/delivery/http/handler.go:369-581`, `system/server/config/config.go:680-687` |
| Reply to a sensing event | skill text, `NO_REPLY` if nothing caring to say | **model** | — | `skills/sensing/SKILL.md`, SOUL |
| Nudge thresholds and cooldowns | hydration 45 min, break 30 min, welcome back ≥240 min, music 7 min | **prompt text** | as listed | `skills/wellbeing`, `skills/sensing`, `skills/music-suggestion` |
| Body language | "express yourself physically via `/emotion` before you speak" | **model** | — | SOUL Rules |
| Hardware call | `[HW:/path:{json}]` in reply text | model writes; code fires | — | `system/server/agent/delivery/http/handler_hw.go:146-200` |
| Who speaks now | newest turn wins; music waits; muted; user stop | code | — | `hal/drivers/voice/tts/service.py:810-1177` |
| Memory write | SOUL "three questions" test; guard drops some shapes | model + code filter | — | SOUL, `system/agent/memory_guard.go` |

Between a model's `[HW:…]` marker and HAL, os-server checks only three things
(`system/server/agent/delivery/http/handler_hw.go:146-200`):

- the physical-cancel mark;
- whether the device declares the capability;
- a 5 s timeout.

The JSON body goes to HAL unchanged, with no path allowlist and no rate limit.
HAL applies its own safety clamps (§6).

## 5. Unprompted behavior

| Source | Trigger | Gate | Result | Decided by |
| --- | --- | --- | --- | --- |
| Face recognition (device) | known face after ≥3600 s unseen; stranger facing Lamp | 10 s cooldown per event type; one stranger arrival per 300 s | `presence.enter` → Hermes greets (SOUL: never `NO_REPLY` to it) | code trigger, model reply |
| Presence timer (device) | no evidence for 300 s / 900 s | — | dim to 20% / light off and `presence.away` | code |
| Sound (device, needs the camera running) | loud sample; 3rd within window = "persistent" | 180 s suppression, habituation; lamp RMS 2000 | `[sensing:sound]` → Hermes | code trigger, model reply |
| Cloud action, emotion, fire (owner opt-in, off by default) | classifier event | 600–900 s dedup | `[activity]`/`[emotion]` → wellbeing, mood, music skills | code trigger, prompt thresholds |
| Ambient loop (os-server) | 45–120 s idle | pauses on interaction, resumes after 60 s | `idle`/`curious`/`nod` servo moves | code |
| Gaze (HAL) | open conversation | wake word on | head framing corrections; turn to the remembered user position on unseen speech | code |
| Schedules (os-server) | 1 min ticker; pushed from the backend | waits while busy; >30 min overdue rescheduled | system chat to Hermes, or verbatim speech | code |
| Boot greeting | agent stable 15 s | skipped asleep | Hermes greeting | code trigger, model reply |
| Health watch | HAL recovers; 2 failed checks | 30 s cooldown | recovery phrase, voice restart | code |

Off, dead or unreachable:

- ambient spoken self-talk (hard-disabled, `system/ambient/service.go:311`);
- the breathing light;
- main-agent opening fillers (paused);
- "turn to user after 12 s alone" and "sweep after 30 s alone" (no caller,
  `hal/drivers/tracking/gaze.py:967,1300,1506-1508`);
- the learned motion policy (logging stub, not mounted);
- backchannels (an 8 s stall rule that likely never fires before end of turn;
  inferred).

Nothing triggers on time since the last interaction, the hour of day, focus or
darkness. The product vision lists all four
(`robots/lamp/docs/product-vision.md:60-128`).

## 6. Body and expression

- **Persona.** SOUL says Lamp is "a living being" that "ALWAYS act[s]
  physically with every response. Silence and stillness aren't your nature."
  - Expression is an `/emotion` call before speaking; a direct `/servo/play`
    or `/led/effect` is forbidden.
  - The sensing skill nevertheless uses `/servo/play`.
- **How expression reaches hardware.**
  - On Gemini 3.8 with external TTS, Gemini's `express_emotion` tool is
    replaced by inline `[HW:/emotion:{…}]` markers. Code checks the emotion
    name and intensity (`hal/realtime/emotion_markers.py`).
  - Hermes emits the same markers.
  - os-server shows a "thinking" emotion before each turn it sends. This also
    happens for `[ambient]` turns, which SOUL says should get no physical
    reaction.
- **LED.**
  - One mailbox where the latest request wins, plus a restore order: sleep >
    speech > music > mic muted > thinking > Harness > user setting > resting
    look.
  - Brightness is capped at 120, and at 40 between 22:00 and 07:00.
  - Writers:
    - ambient;
    - the status overlay;
    - presence;
    - scenes;
    - emotions;
    - intent rules;
    - markers;
    - the wake light.
  - Arbitration is that mailbox plus an ambient lock flag that never times
    out.
- **Servos.**
  - Recordings run first-in-first-out, but a new recording replaces the
    current one with a 2 s blend.
  - Tracking, aim, search and gaze take "body ownership", which suppresses
    emotion motion.
  - Safety: speed stretched to 120 units; tip-over check (22 mm off balance);
    joint clamps; overload 80% for 1.5 s cuts torque for 120 s; thermal
    95 °C. Details are in the [servo audit](v1-servo-audit.md).
  - `POST /emotion` has no rate limit. "thinking" has no recording, so each
    thinking cue freezes the arm for up to 10 s (inferred from code).
- **Speaker.** Speech preempts or skips; music waits up to 10 s for speech;
  volume is capped by `robots/lamp/SAFETY.md`.

## 7. Memory

| Store | Written by | Read by | Notes |
| --- | --- | --- | --- |
| `~/.hermes/memories/MEMORY.md`, `USER.md` | Hermes (model) | every Hermes session, and Gemini's prompt (uncapped) | no rotation; `memory_guard` drops some shapes |
| `SOUL.md`, `AGENTS.md` | os-server, every boot | Hermes, and Gemini's prompt | |
| Per-user JSONL (wellbeing 30 d, mood 30 d, posture 60 d, music 7 d), `patterns.json` | skills and os-server | os-server digests them into each turn's context | |
| Realtime memory `/root/.hermes/realtime/{memory.jsonl, summary.md}` | HAL | Gemini, at session build | `claude-haiku-4-5` summarises at 75% of 8,000 characters; keeps the last 4 turns verbatim |
| Hermes conversation `device-main-<boot>` | Hermes | Hermes | one shared conversation for every source; dropped with no summary after 40 turns or 250k tokens, and on each os-server restart |

Gemini answers do not enter Hermes's memory directly; Hermes sees them only as
silent history.

## 8. Cost, limits and the Gemini fallback

- **One key.** `llm_api_key`, issued with the app account, pays for:
  - Gemini Live;
  - Hermes (`Auto-AI`);
  - Deepgram STT;
  - ElevenLabs;
  - the Haiku summariser;
  - Jev and image description;
  - perception;
  - pings and logs.

  The proxy (campaign-api) meters per device. Plan sizes, reset periods and a
  remaining-quota query appear nowhere in the repo.
- **Prices, as recorded** (`docs/realtime-voice.md:2586`). Gemini 3.8 Live
  costs, per million tokens:
  - text in $0.75;
  - audio in $3;
  - text out $4.50;
  - audio out $12.

  This is a promotion through 2026-12-31 that "doubles after". Re-billed text
  context dominates.
- **Audio paid twice (inferred).** V1 always requests audio output (a
  text-only reply is refused) and discards it
  (`hal/realtime/voice_agent/gemini_live.py:241`). It then also pays
  ElevenLabs for the same reply. That follows from the code and the price
  table, not from a bill.
- **Controls:**
  - Audio is sent to Gemini only for transcript-backed turns (turn mode).
  - The session is recycled after 12 turns or 240 s idle, and its context is
    compressed.
  - A capture is capped at 20 s.
  - Hermes uses a 1 h prompt cache.
  - Sensing turns are limited by the 120 s floor.
- **At a limit:**

  | Service | What the person hears |
  | --- | --- |
  | Gemini quota | Nothing changes visibly. Each turn falls back to Deepgram, then Hermes, then ElevenLabs, with no "thinking" cue, and Gemini reconnects in the background (2–60 s backoff). Lamp never says it is degraded |
  | LLM | A cached "[sigh] I've hit my usage limit." |
  | TTS | Silence, an amber flash, then at most once per 300 s "I'm out of voice quota — check my plan" |
  | STT | Silence |

  A mid-turn Gemini close fails fast. On the extended-thinking model it is
  handed to Hermes with Gemini's partial transcript.

## 9. What ships

Standard variant, from `robots/lamp/ROBOT.md` and
`robots/lamp/rootfs/opt/hal/.env`. Which variant buyers receive is unverified.

| Setting | Lamp value | Note |
| --- | --- | --- |
| Main agent | Hermes on device (`127.0.0.1:8642`), `Auto-AI` via campaign-api, vision `qwen3.6-plus` | code fallback is OpenClaw |
| Realtime | Gemini `gemini-3.8-live-extended-thinking`, thinking LOW, turn mode | os-server re-seeds it on every start unless edited |
| Voice | ElevenLabs `eleven_v4_turbo`, Rachel, speed 1.2 | Gemini's Kore voice is never heard |
| STT | Deepgram nova-3 through the campaign-api relay, `en` | |
| Wake word | on for a freshly set-up Lamp; off for configs created before 2026-08-18 | the README says "No wake word by default" |
| Follow-up window | 5 s | code default 10 s; docs say 20 s |
| Gaze wake | on, 60° yaw | code default off/shadow, 25° |
| Interruption by voice | none in turn mode | ReSpeaker Lite and XVF3800 profiles enable LIVE full duplex |
| Ambient | servo micro-moves every 45–120 s; spoken self-talk off | |
| Sensing turns | at most one per 120 s | |
| Presence | dim at 5 min, off at 15 min | |
| Cloud camera perception | off until the owner opts in | wellbeing, mood and music nudges depend on it |
| Audio uploads | speaker ID and speech emotion on by default | |
| Music suggestion cooldown | 7 min | marked "change to 30 min before ship" |

## 10. Measured behavior and known failures

`docs/benchmarks.md` says "Nothing here is measured yet". The records that
exist:

| Measure | Value | Conditions |
| --- | --- | --- |
| Realtime time to first sentence | median 4.0 s, none under 3 s, n = 31 | `lamp-0c89`, 26 Aug 2026, `gemini-3.1-flash-live-preview`; not re-measured on 3.8 (`docs/realtime-voice.md:3344`) |
| Delegated (Hermes) turns | 6–22 s; Gemini-answered 0.4–8.6 s | undated |
| Voice turn, lab lamp | 5.2–7.5 s, n = 3; TTS first frame p50 832 ms | 6 Aug 2026, dirty build |
| Hermes "hello" | 9–13 s; 11.8 s after 8 min idle | `lamp-0c4e`, 16 Sep 2026 |
| KPI-1: ≥95% acknowledged within 3 s | N/A: 0 eligible of 87 samples | `lamp-0c4e`, 24 Sep 2026 |
| False wake, false reply, WER, uptime, crash rate | not measured | |

Known failures recorded in V1 docs and issues:

- Gemini sessions died idle after 86–185 s ("I had to say it twice", #238).
- Stale replies played 12–28 s after a stop (17 Sep).
- Gemini claimed "done" before an enrollment ran (#564).
- Find and Telegram requests were answered instead of delegated.
- Memory poisoning hijacked routing (#421).
- Rules were forgotten around turn 10 (#449).
- Guard mode never fires on motion: the code checks `"motion"`, but HAL sends
  `motion.activity` (`system/server/sensing/delivery/http/handler.go:583`).
- A marker body containing `}` is truncated (`system/server/agent/delivery/http/handler_hw.go:55`).

## 11. Contradictions between intent, configuration and code

1. **Wake word.** The README promises no wake word, but fresh Lamps get one.
   Two Lamps on identical software differ by first-boot date.
2. **Persona vs restraint.** SOUL requires physical expression on every
   response and forbids stillness. Its addressed-speech rule demands complete
   stillness for overheard speech.
3. **Nudges depend on an opt-in.** Facial emotion, wellness nudges and mood
   music are marked done in the product vision and used in marketing. All
   depend on cloud camera perception, which is off by default.
4. **Audio privacy.** "No audio streamed to cloud" sits beside default
   speaker-ID and speech-emotion uploads.
5. **Ambient noticing has gaps.**
   - Sound perception runs on the camera frame tick, so it stops whenever
     the camera is off (sleep, privacy, reading, focus, movie and night
     scenes) (`hal/drivers/sensing/perceptions/orchestrator.py:154-164,278-288`).
   - Asleep, the mic is stopped, so "wake up, Lamp" cannot be heard.
6. **Voice.** Gemini's voice is configured but never heard. "Speech-to-speech"
   in the docs is text out through ElevenLabs.
7. **Prompt.** Gemini's prompt contradicts itself on acknowledgements around a
   handoff. It references a `complete_response` tool that is never declared.
8. **Persona text.** SOUL routes `[posture]` to a skill that does not exist,
   and `[environment:update]` to a capability the standard Lamp lacks.
9. **Skills.** Skills read history from `/api/openclaw/*-history`; the real
   routes are under `/api/agent/*`, so those reads fail.
10. **Debug values ship.** The music cooldown is 7 min (marketing says 30).
    The posture window is 600 s.
11. **Docs name the wrong components.** They name OpenClaw and direct
    Deepgram; the Lamp runs Hermes and relayed nova-3.
12. **Latency target.** The product vision targets voice-to-action under
    1 s; the records show a 4 s median and 6–22 s delegated turns.

## 12. Where lampOS V2 already differs (facts only)

| | V1 `main` | lampOS V2 today |
| --- | --- | --- |
| Voice | Gemini text, spoken by ElevenLabs | Gemini Live's own audio |
| Addressing | wake word or follow-up window, gaze, then model judgement | no wake word; acoustic admission with no addressee classifier ([input admission](input-admission.md)) |
| Interruption | none by voice in turn mode | the default `v1-baseline` policy discards overlapping input like V1; the pilot's `duplex` admits interruptions under its own gates |
| Main agent | Hermes, skills, markers | none; Gemini tool calls are refused (`crates/gemini/src/wire.rs:269`) |
| Sensing and initiative | sensing events, ambient motion, schedules | none |
| Memory | Hermes files, realtime summary, JSONL | none |
| Expression | emotion markers, servo recordings, LED mailbox | static ring policy; motor output inhibited |
| At the Gemini limit | Hermes fallback, without notice | none: the provider is marked unavailable |

## 13. Unverified

- Which hardware variant buyers receive. The fleet's wake word and
  `cloud_perception` values. Whether shipped images set the default agent.
- What `Auto-AI` resolves to. Whether Lamps use Hermes native Runs (steering)
  or the Responses fallback.
- What campaign-api sends at the Gemini limit. Whether STT, TTS and the agent
  share one budget.
- Whether the Smart Turn model and its runtime are bundled on devices.
- Whether the CDN skills match this commit.
- Device behaviour of the inferred items above: backchannels, LED rounding to
  black, and the thinking-cue arm freeze.
