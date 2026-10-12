# V1 baseline — the realtime voice orchestrator (`hal/realtime/**`)

Checkout: `v1-main` at `170a1b85e` (origin/main, "V1 as deployed"). All paths are relative to it.
Facts only. Every claim has a `file:line`. "Unverified" marks what this repo cannot prove
(chiefly the campaign-api backend, which is not in this repo, and which hardware profile ships).

Scope: `hal/realtime/**` (orchestrator, Gemini Live agent, context/memory, prompts, tools).
Touched only at the boundary: `hal/drivers/voice/_internal/realtime_turn.py`,
`turn_dispatch.py`, `voice_service.py`, `hal/drivers/harness/announcer.py`, and the
os-server Go code that receives the handoff.

---

## 0. One-paragraph picture

Every captured user utterance goes two ways at once: HAL's own streaming STT (Deepgram)
and, in parallel, the raw audio to a cloud "realtime" model session (default: Gemini Live,
model `gemini-3.8-live-extended-thinking` on a fleet lamp). The realtime model either
(a) answers by itself — its *text* transcript is spoken by HAL's TTS (ElevenLabs on the
lamp), and the exchange is later copied to the main agent as silent history; (b) calls
`delegate_to_main`, and HAL forwards `[voice-instruction] <model's words>\n[transcript] <STT>`
to os-server → the main agent (Hermes by default on lamp), whose reply os-server pushes back
to HAL TTS; (c) calls `reject_turn`, and the turn goes nowhere; or (d) produces nothing /
fails, and HAL forwards the raw STT text to the main agent. The routing choice is made by
the model, guided by a ~40 KB prompt plus tool descriptions; code enforces only a handful of
deterministic guards (noise, foreign script, find→delegate, empty-delegate, silence→fallback,
unconfirmed-answer→fallback).

---

## 1. Providers, selection, models, voices, modes, streaming

### 1.1 Selection

- Provider knob: `REALTIME_PROVIDER = _rt_str("HAL_REALTIME_PROVIDER", _RT.get("provider"), "gemini")`
  — env > `config.json` `realtime.provider` > `"gemini"` (`hal/config.py:576`; precedence helper
  `hal/config.py:556-563`; `config.json` read once at import, `hal/config.py:536-553`).
- Master switch: `REALTIME_ENABLED` = `HAL_REALTIME_ENABLED` > `realtime.enabled` > `True`
  (`hal/config.py:566-575`).
- `none|off|disabled|""` → orchestrator does not start (`hal/realtime/startup.py:14-17`).
- Built in `RealtimeOrchestrator._make_agent()`: `gemini` → `GeminiLiveAgent`, `openai` →
  `OpenAIRealtimeAgent`, `gptlive` → `GPTLiveAgent`, `pipecat_v1` → `PipecatV1Agent`; anything
  else → `None` (`hal/realtime/orchestrator.py:534-572`).
- os-server seeds `config.json` `realtime` = `{enabled:true, provider:"gemini", gemini:{model:
  "gemini-3.8-live-extended-thinking", voice:"Kore", thinking_level:"LOW"}, openai:{...},
  gptlive:{...}, pipecat_v1:{...}}` (`system/server/config/realtime.go:70-117`) and **re-writes it
  on every os-server start while the block is not `pinned`** (`system/server/config/config.go:433-445`).
  Any operator edit via the realtime-set API pins it (`system/device/realtime.go:40-47`).
- Rationale recorded in code for the extended-thinking default: "plain gemini-3.8-live delegates or
  stays silent every turn" (`system/server/config/realtime.go:71-72`).

**So a fleet lamp runs: provider `gemini`, model `gemini-3.8-live-extended-thinking`, voice `Kore`,
thinking `LOW`.** HAL's own code default is the plain `gemini-3.8-live` (`hal/config.py:743-745`),
used only if config.json has no model.

### 1.2 "Gemini non-Live / extended thinking" is not a separate provider

There is no fifth provider. `gemini-3.8-live-extended-thinking` is a model name passed to the same
`GeminiLiveAgent`. The code branches on the substring `"extended-thinking"`:
- thinking config: plain `3.8-live` gets no `thinking_config` (it "rejects thinkingLevel outright");
  extended-thinking gets `thinking_level` with MINIMAL clamped to LOW
  (`hal/realtime/voice_agent/gemini_live.py:222-238`);
- all declared tools become `Behavior.NON_BLOCKING` ("Extended-thinking accepts ONLY NON_BLOCKING
  tools") (`gemini_live.py:269-283`);
- the turn needs a confirmed outcome (`_requires_outcome`), a 6 s routing grace after the first
  terminal, and a progress watchdog (`gemini_live.py:767-773`);
- a receive error falls back to the main agent (`fallback_to_main = "extended-thinking" in model`,
  `gemini_live.py:1828-1830`).
In `docs/realtime-voice.md` "non-Live" means `HAL_LIVE_MODE=false` (turn-based), e.g. "Gemini non-Live
extended-thinking" (`docs/realtime-voice.md:1865`, `:503-507`).

### 1.3 Models and voices per provider (code defaults; config.json/Go seed in brackets)

| Provider | Model default | Voice default | Key / base URL |
|---|---|---|---|
| gemini | `gemini-3.8-live` (`hal/config.py:745`) [seed `gemini-3.8-live-extended-thinking`, `realtime.go:73`] | `Kore` (`hal/config.py:746`) | key: `GEMINI_API_KEY` > `GOOGLE_API_KEY` > `realtime.api_key` > `llm_api_key` (`hal/config.py:732-737`); URL: `HAL_GEMINI_LIVE_BASE_URL` > `realtime.base_url` > `<llm_base_url>/ws/gemini` (`hal/config.py:738-742`) |
| openai | `gpt-realtime-2` (`hal/config.py:925`) | `alloy` (`:926`) | `OPENAI_API_KEY` > `realtime.api_key` > `llm_api_key`; `<llm_base_url>/ws/openai` (`:915-924`) |
| gptlive | `gpt-live-1` (`:953`), backend `gpt-5.6-luna` (`:965`) | `marin` (`:954`) | key `:941-946`; URL falls through to the OpenAI realtime URL (`:948-952`) |
| pipecat_v1 | `qwen/qwen3.6-35b-a3b` (`:993`) at `https://campaign-api.autonomous.ai/api/v1/ai/v1/qwen/v1` (`:988-992`) | none (text out; HAL TTS speaks) | key `:981-986` |

Gemini voice enum: 30 prebuilt voices (`hal/realtime/enums/gemini.py:13-44`); os-server validates only
5 (`Puck|Charon|Kore|Fenrir|Aoede`, `system/server/config/realtime.go:267,288`).

Gemini's own voice is irrelevant on the lamp: see 1.5.

(Details of the three non-default providers are in §10, from a delegated read.)

### 1.4 Turn-based vs LIVE mode

- `LIVE_MODE = VOICE_INPUT_MODE == "automatic" and HAL_LIVE_MODE` (default `false`) (`hal/config.py:701-705`).
- Turn detection: `HAL_REALTIME_TURN_DETECTION` default `off`; forced `off` in tap-to-talk; forced
  `server_vad` when LIVE_MODE and it was off (`hal/config.py:720-725`). Gemini maps this to
  `vad_enabled` (`hal/realtime/config.py:178-180`).
- **Turn mode (LIVE off — standard and pro lamp profiles):** Gemini server VAD is disabled
  (`automatic_activity_detection.disabled=True`, `gemini_live.py:181-183`). HAL brackets each user
  turn: the first audio frame sends `activityStart` (`gemini_live.py:444-452`), `commit_audio()` sends
  `activityEnd` (`gemini_live.py:687-719`). HAL's own capture/VAD/wake gate/endpointing decides the
  turn; audio frames are streamed to Gemini **while the user is still talking** once the realtime turn
  is opened (`hal/drivers/voice/voice_service.py:2765-2782`, `:2900-2914`); in wake-word mode frames
  are buffered and flushed only after the STT *final* confirms the wake phrase
  (`voice_service.py:2592-2629`). A per-turn `[TURN CONTEXT]` text is sent before the audio
  (`voice_service.py:2616`, `:2651`; built at `hal/drivers/voice/_internal/realtime_turn.py:273-325`).
- **LIVE mode (pro-respeaker-lite and pro-xvf3800 profiles set `HAL_LIVE_MODE=true`):** mic streams
  continuously into one session; Gemini server VAD owns end-of-turn and barge-in (sensitivities from
  `HAL_LIVE_VAD_*`, `gemini_live.py:176-211`). `[TURN CONTEXT]` is sent once at session entry
  (`voice_service.py:2249-2252`). `end_conversation` is registered only in LIVE
  (`orchestrator.py:410-412`). A delegate hangs up the live session so the main reply does not play
  into the open uplink (`voice_service.py:1670-1708`).
- **Why per-turn on standard lamps (from code comments/config):** the noise guard must see an STT
  transcript before anything is committed ("Cost guard: Gemini replies only when STT produced text",
  `hal/env-presets/01-balanced-cost-safe.env:28-29`; `REALTIME_REQUIRE_TRANSCRIPT` "Never commit an
  empty-STT turn (Gemini invents a reply)", `hal/config.py:673-676`); a dropped turn's already-streamed
  audio must be discarded by rebuilding the session (`orchestrator.py:725-734`); and LIVE is an
  exclusive whole-process choice because turn detection is fixed at connect
  (`docs/realtime-voice.md:2742-2751`). Echo: LIVE uses provider VAD "LOW start sensitivity + prefix
  padding keep our own echo from opening a turn" (`gemini_live.py:179`).

### 1.5 Audio out: Gemini voice is discarded on the lamp

- Gemini is always asked for AUDIO ("TEXT-only is refused (WS 1007), so with our own TTS the audio is
  discarded") plus `output_audio_transcription` "The only way to get the reply as text"
  (`gemini_live.py:240-260`; `hal/realtime/config.py:186`).
- HAL plays the model's own audio only if `HAL_REALTIME_NATIVE_AUDIO=true` **or** the TTS provider is
  `gemini` (`native_voice()`, `hal/drivers/voice/tts/gemini.py:108-116`; used at
  `realtime_turn.py:544`). The lamp sets `HAL_REALTIME_NATIVE_AUDIO=false`
  (`robots/lamp/rootfs/opt/hal/.env:52`) and `voice.tts_provider: elevenlabs` (`robots/lamp/ROBOT.md:13`).
- Therefore on the lamp **both the realtime model's answers and the main agent's replies are spoken by
  the same ElevenLabs TTS voice**. Realtime text is split into sentences and sent to `tts.speak` /
  `tts.speak_queue` (`realtime_turn.py:778-852`; first-clause splitting off by default,
  `REALTIME_FIRST_CHUNK_MAX_CHARS=0`, `hal/config.py:1117-1120`). The Gemini prompt even tells the
  model to use "native ElevenLabs v4 square-bracket tags" (`hal/realtime/resources/system_prompt_gemini.md:7`).
- Gemini output sample rate is 24 kHz; input 16 kHz (`gemini_live.py:149-156`; `hal/config.py:747`).

### 1.6 Session lifecycle (Gemini)

- Startup: one background worker builds instructions, makes the agent, connects; failures retried by
  the orchestrator with backoff 2 s → ×2 → 60 s cap (`hal/realtime/startup.py:28-78`;
  `orchestrator.py:67-69`, `:944-986`). Memory catch-up summarization runs in a background thread
  after connect (`startup.py:67-68`).
- Instructions (the whole system prompt + memory) are built **once per session connect**
  (`startup.py:43`; `orchestrator.py:623`). Memory written mid-session reaches the model only at the
  next rebuild, except text injected live (`[TURN CONTEXT]`, `[TTS HISTORY]`).
- Rebuild triggers (each makes a brand-new connection with freshly built instructions):
  - parked session resumed on the next capture (`orchestrator.py:794-810`); prewarm on speech start
    (`orchestrator.py:762-766`);
  - `requires_fresh_session` (unresolved tool call, or a valid `delegate_to_main` was seen —
    `gemini_live.py:1689-1694`) → rebuild before streaming (`orchestrator.py:811-824`);
  - emotion-marker mode or voice changed (`orchestrator.py:825-838`);
  - Gemini idle ≥ `HAL_GEMINI_PRE_TURN_RECYCLE_S` (60 s) before a turn (`orchestrator.py:839-855`);
  - post-turn: zombie (3 consecutive silent turns), idle-reset (turn after ≥240 s idle), turn cap (12
    turns) (`orchestrator.py:1397-1440`); all suppressed while a LIVE session owns the mic
    (`orchestrator.py:1412-1413`);
  - noise-dropped / bare-wake / max-duration turn whose audio already streamed
    (`orchestrator.py:725-734`; callers `realtime_turn.py:1031`, `voice_service.py:3112`, `:3376`).
- Idle park: watchdog polls every 5 s; Gemini parks after 45 s idle, GPT-Live after 30 s, OpenAI and
  Pipecat never (`orchestrator.py:869-910`; `hal/config.py:645-647`, `:978`). Parked still reports
  `available` (`orchestrator.py:491-504`).
- Session resumption off by default: "the campaign-api proxy does not forward resumption (zombie
  sessions)" (`hal/config.py:754-757`).
- Context compression for 3.8: trigger 32768 tokens, target 24576 (`hal/config.py:749-752`;
  `gemini_live.py:301-308`).
- Agent-level reconnect after a drop: the recv loop self-heals with exponential backoff 2 s → 60 s
  ("so a persistent failure (e.g. usage-limit 4029) doesn't hammer the endpoint")
  (`gemini_live.py:134-138`, `:1755-1817`, `:1956-1966`).

---

## 2. Tools and instructions

### 2.1 Which tools are registered (`orchestrator.py:407-426`)

| Tool | Registered when | Lamp default (extended-thinking, ElevenLabs, LIVE off) |
|---|---|---|
| `delegate_to_main` | always (`:407`) | yes |
| `reject_turn` | `HAL_REALTIME_AI_REJECT_FILTER` (default true, `hal/config.py:678-680`) (`:408-409`) | yes |
| `end_conversation` | `LIVE_MODE` (`:410-412`) | no (yes on LIVE profiles) |
| `express_emotion` | device has `expression` capability (`:413-414`; `hal/runtime.py:604`) **and not** emotion-marker mode (`:545-546`) | **no** — replaced by inline markers (see 2.6) |
| `look` | camera present AND `HAL_GEMINI_VISION` (default true) AND provider gemini (`:415-422`; `hal/config.py:767-773`) | yes (lamp declares `vision`, `robots/lamp/ROBOT.md:23`) |
| `web_search` | `HAL_PIPECAT_WEB_SEARCH` AND provider `pipecat_v1` (`:361-366`, `:423-425`) | no |
| Google Search (built-in, not a function) | `HAL_GEMINI_GOOGLE_SEARCH` default true (`hal/config.py:759-765`; `gemini_live.py:290-291`) | yes |
| `complete_response` | **never declared** ("do not advertise it anew", `gemini_live.py:284-287`) | no — but prompts still mention it (dead text) |

On extended-thinking every declared function is `NON_BLOCKING` (`gemini_live.py:269-283`).

Note: the tool is named `end_conversation`, not `end_call` (`orchestrator.py:211`).

### 2.2 Per tool: what it does, what code runs, enforcement

**`delegate_to_main(message)`** — description `orchestrator.py:81-165` (a ~4 KB policy text: device /
desktop actions, music, scheduling, memory, skills, real-time facts, wellbeing symptoms, room air,
digital work/Harness, sleep/wake, finding objects, research/documents, channels/sending, task
follow-ups, enrollment; "forward ONLY the current user's faithfully understood words in their spoken
language"; "remain silent for background speech"). Parameter `message` required (`:167-181`).
Code (`orchestrator.py:1298-1361`): parse `message`; if empty → reply `{"error": "message must not be
empty"}` and keep reading (`:1311-1323`); else ACK `{"result":"delegated"}` (withheld —
`trigger_response=False` — for a non-resumable, non-LIVE Gemini session that already requires a fresh
session, `:1329-1351`), `end_turn()`, yield `DelegateSignal(message, transcript=user_transcript,
handoff_context)` and **break immediately** (`:1352-1361`). Gemini side marks the session for
replacement before publishing the call (`gemini_live.py:1689-1694`) and attaches handoff context
(search queries/sources + "Already spoken to user", ≤6000 chars; `gemini_live.py:829-833`).
Enforcement: **model choice**. Code only rejects empty messages.

**`reject_turn()`** — description `orchestrator.py:183-209`: drop the turn when (a) not addressed to
the device, (b) addressed but a bare acknowledgment/filler/stray word, (c) another language; "never
merely because you are uncertain"; voice must be blank.
Code (`orchestrator.py:1249-1297`): in turn mode, if output already began → reply `{"error":
"reject_turn must be the only turn outcome"}`, end turn, and the rejection is **ignored**
(`:1254-1268`); otherwise ACK `{"result":"turn dropped"}` (withheld in the same quarantine case as
delegate, `:1271-1293`) and yield `RejectSignal`. In LIVE, Gemini sets a `_reject_followup_barrier`
that swallows ACK-triggered speech and auto-answers further tool calls with `"ignored: rejected turn"`
until new user input (`gemini_live.py:1162-1209`, `:1683-1685`).
Downstream: `RejectSignal` stops TTS if already speaking (`realtime_turn.py:709-714`) →
`ROUTE_AI_REJECTED` (`:918-930`) → `should_drop_downstream_turn()` true when the filter flag is on
(`realtime_turn.py:354-364`) → nothing reaches the main agent (`turn_dispatch.py:177-181`, `:314-322`).
Enforcement: **model choice**; code only forbids reject-after-speech in turn mode.

**`end_conversation()`** (LIVE only) — description `orchestrator.py:211-226`. Code: ACK `{"result":
"ending after farewell"}` with `trigger_response=True`, yield `EndCallSignal`, keep streaming the
farewell (`orchestrator.py:1231-1248`); the live pump hangs up after `HAL_LIVE_HANGUP_GRACE_S` = 3 s
(`voice_service.py:1709-1720`; `hal/drivers/voice/_internal/config.py:149`). Model choice.

**`express_emotion(emotion, intensity)`** — description `orchestrator.py:238-249`; enum of 12 emotions
(`:231-237`). Code: fire HAL's in-process emotion handler on a daemon thread
(`orchestrator.py:1600-1669`); ACK is **always** `trigger_response=True` ("# patch: not to block the
session", `:1632-1641`) although the comments above it (`:1630-1631`) say a spoken turn should not be
acked. Model choice. Not registered on the lamp default (markers instead).

**`look()`** — description `orchestrator.py:284-301` (one frame from wherever the head points; must
re-call for every present-tense visual question; never for finding things). Code
(`orchestrator.py:1196-1215`, `:1705-1877`):
1. If the model's input transcript matches a deterministic find regex (`find|locate|search|look
   for|look around|where is|do/can you see my|tìm|ở đâu|đâu rồi`, `hal/realtime/find_intent.py:7-23`),
   the look is converted into a `DelegateSignal` with the transcript — **code-enforced**
   (`orchestrator.py:1671-1703`).
2. Camera turned off by the user → error result, no capture (`:1718-1730`).
3. Cost guard: one image per turn and none within `HAL_GEMINI_VISION_MIN_INTERVAL_S` (10 s) of the last
   send → "using the current view; answer now" (`:1731-1761`; `hal/config.py:906-908`).
4. Aim the head (yaw) toward a person, bounded by `HAL_LOOK_AIM_DEADLINE_S` (8 s), optionally speak a
   "look_capturing" line, then capture with a 0.3–0.5 s settle scaled by last move
   (`:1763-1804`, `:272-281`; `hal/config.py:778-784`, `:898-904`).
5. Downscale to 768 px wide (`:1913-1921`; `hal/config.py:774-776`).
6. On extended-thinking (once `interactionStatus` was seen, `supports_look_continuation=True`,
   `gemini_live.py:1291-1293`) the JPEG is returned inside the tool response and the same interaction
   continues (`orchestrator.py:1823-1844`; `gemini_live.py:529-557`); otherwise ACK + separate video
   frame + `LookReplaySignal` → HAL re-commits the turn audio (`orchestrator.py:1206-1214`,
   `realtime_turn.py:860-873`).
7. Tell the model `found_user:false` if aiming did not find the user (`orchestrator.py:1821-1833`).
8. Persist the frame so a later delegate/fallback carries it as `[vision-image]` + base64 if ≤45 s old
   (`orchestrator.py:1855-1859`, `:1927-1956`; `turn_dispatch.py:32-64`; `hal/config.py:909-913`).
Watchdog extended to `HAL_REALTIME_LOOK_RECV_TIMEOUT_S` = 20 s (`orchestrator.py:1752`, `:1854`;
`hal/config.py:629-631`).

**`web_search(query)`** (pipecat_v1 only) — description `orchestrator.py:324-358`; handler blocks the
output loop calling `grounded_search()` against the campaign-api Google-Search relay (`gemini-3.7-flash`,
10 s timeout), errors hint "call delegate_to_main" (`orchestrator.py:1546-1598`; `hal/config.py:1025-1040`).

**Google Search grounding** (Gemini built-in): no HAL code decides it; the model searches server-side.
HAL only observes `grounding_metadata`, treats queries/chunks as verified progress (extends the wait
to 15 s from commit), and collects up to 12 query/source lines for the handoff context
(`gemini_live.py:1228-1276`).

**`complete_response`** (legacy, never declared): if an old resumed session calls it, HAL ACKs
`{"result":"recorded"}` and treats it as advisory confirmation (`gemini_live.py:1657-1682`).

### 2.3 Where the system instruction lives and how it is assembled

`ContextManagerBase.build_instructions()` (`hal/realtime/context_manager/base.py:355-407`) concatenates:
1. **prompt** — provider file: `system_prompt_gemini.md` / `_openai.md` / `_gptlive.md` /
   `_pipecat.md`, fallback `system_prompt.md`; `{language}` replaced with the name for config
   `stt_language` (`base.py:168-189`, `:450-466`; language default "English", `orchestrator.py:486`).
2. **identity** — `# DEVICE IDENTITY` + a reading rule (name comes from IDENTITY.md / SOUL.md) + the
   runtime's persona files (`base.py:367-378`). Persona source:
   - Hermes (lamp default gateway, `robots/lamp/ROBOT.md:9-10`): `/root/.hermes/SOUL.md` +
     `/root/.hermes/memories/USER.md` (`hal/realtime/context_manager/hermes.py:30-46`), **no size cap**;
   - OpenClaw-layout runtimes: `SOUL.md`, `IDENTITY.md`, `USER.md` capped at 12000 chars
     (`hal/realtime/context_manager/openclaw.py:32-55`; `hal/config.py:1108`).
   Gateway: `HAL_AGENT_GATEWAY` > config `agent_runtime` > `openclaw` (`hal/config.py:527-534`).
3. **skills** — `# SKILLS CATALOG`: table of name + first sentence (≤150 chars) of every
   `skills/*/SKILL.md` frontmatter description (`hermes.py:62-94`; `openclaw.py:127-159`).
4. **device_mem** — `# DEVICE MEMORY`: Hermes `memories/MEMORY.md` (uncapped, `hermes.py:48-60`);
   OpenClaw-layout: `device_summary.md` + root `MEMORY.md` + newer `memory/*.md`, ≤8000 chars
   (`openclaw.py:57-125`).
5. **realtime_mem** — `# REALTIME MEMORY`: `[Previous summary]` + verbatim turns (§5).
6. **routing** (Gemini only) — `routing_prompt_gemini.md` restated after memory "so Gemini makes the
   function call instead of a promise-only answer" (`base.py:389-392`).
7. **overheard** — only if ROBOT.md `answer_overheard_speech: true` (`base.py:394-396`, `:156-162`);
   lamp does not set it (`hal/board/device.py:176-178`; only `robots/intern-v2/ROBOT.md:24` does).
Then, for Gemini 3.8 with external TTS + expression, the `express_emotion` prompt bullet is swapped
for `emotion_markers.md` (`hal/realtime/emotion_markers.py:9-15`; `orchestrator.py:540-548`).
Size is logged per section as "billed EVERY turn" (`base.py:398-406`). The doc estimates the prompt at
"~7-10k token" (`docs/realtime-voice.md:2592-2594`); the Gemini prompt file alone is ~42 KB on disk
(116 long lines) — measured by this read, not by the code.

Per-turn injections (not part of instructions): `[TURN CONTEXT] Time | Location (from timezone) |
Reply language | Current user | Main agent is waiting for this answer to its question "...": call
delegate_to_main ...` (`realtime_turn.py:273-325`); `[TURN CONTEXT UPDATE]` speaker correction
(`:328-334`); `[TTS HISTORY]`, `[TTS HISTORY, not spoken]`, `[TTS HISTORY, interrupted; ...]` lines with
main-agent speech (≤300 chars) (`voice_service.py:365-385`). On Gemini 3.8 these go as
`clientContent` with `turn_complete=False` (`gemini_live.py:464-476`); they are dropped only on
2.5 native-audio models (`orchestrator.py:1958-1975`).

### 2.4 Key quotes from `system_prompt_gemini.md` (lamp default prompt)

- Addressed speech first: "Before answering, delegating, or using any action tool, establish that the
  speaker is talking to this device ... For overheard speech, produce no voice or text and no emotion,
  movement, look, or delegation; use only `reject_turn` ..." (`system_prompt_gemini.md:4`).
- Language lock: "Speak EXCLUSIVELY in {language}" and other-language speech is "dropped via
  `reject_turn`" (`:6`, `:31`).
- Brevity: "default to one or two short sentences" (`:12`).
- "Default to silence unless clearly addressed in intelligible speech ... When in doubt, stay silent"
  (`:19`); reject vs silence: "a silent end falls through to the main agent" (`:20`).
- Direct-answer bias: "**Answer directly via voice by default** — `delegate_to_main(message)` adds heavy
  latency ... Any request to *do* or *change* something ... delegate it immediately" (`:39`).
- Route before speech: "your first response MUST be the actual `delegate_to_main` function call ...
  Do not speak an acknowledgment ... Emit only the handoff function call, with no spoken audio or text
  before or after it" (`:43`).
- Direct-answer list: identity, time/date/location from `[TURN CONTEXT]`, chit-chat/knowledge, this
  conversation, feelings, public live lookups via Google Search, local lookups using `Location`
  (`:57-64`).
- Delegate list: memory recall of earlier sessions, hardware, movement, finding things, timers/
  reminders/music, state writes, private data, research/documents, channels/sending, skills (`:66-76`).
- "**When in doubt, delegate.** You are a fast voice front-end; the main system is the authoritative
  brain" (`:86`).
- Routing restated after memory (`routing_prompt_gemini.md:2-12`).

Internal inconsistency in the prompt: `:43` and `routing_prompt_gemini.md:4` forbid any speech before/
after a delegate, but `:55` allows "one neutral acknowledgment such as 'I can help with that.'" and
`:66` heads the list "DELEGATE (optional brief acknowledgment, mandatory tool call)". Many lines
(`:21`, `:22`, `:41-44`, `:89`, `:101`) reference `complete_response`, which is never declared.

### 2.5 Routing: enforced in code vs prompt-only

| Rule | Where | Code or model |
|---|---|---|
| Handle vs delegate vs reject | prompt + tool descriptions | **model** |
| Find/locate request that reached `look` → delegate | `find_intent.py:7-23`; `orchestrator.py:1671-1703` | **code** (only after the model picked `look`) |
| Empty delegate message refused | `orchestrator.py:1311-1323` | code |
| Reject after speech already began (turn mode) ignored | `orchestrator.py:1254-1268` | code |
| Silent/empty/timeout turn → main agent with STT text | `realtime_turn.py:976-995`; `turn_dispatch.py:300-313` | code |
| Extended-thinking spoken answer without confirmed outcome → main | `gemini_live.py:882-940`, `:966-1019`; `base.py:333-345`; `orchestrator.py:1174-1184` | code + Haiku classifier (`response_outcome.py:11-77`) |
| Only `<no speech>` markers → treated as rejection | `orchestrator.py:1379-1388`; `realtime_turn.py:889-897` | code |
| Non-actionable filler / short noise transcript never committed | `realtime_turn.py:400-447`; `hal/config.py:681-694` | code |
| Reply in a foreign script never spoken, turn dropped | `realtime_turn.py:419-430`, `:787-801`, `:907-917` | code |
| Harness follow-up active → delegate, skip realtime | `realtime_turn.py:450-456`, `:594-596` | code |
| Answer owed to a pending main-agent question → forwarded live even if realtime answered/rejected | `realtime_turn.py:1045-1061`; `main_followup.py:35-61` | code (plus prompt hint in `[TURN CONTEXT]`) |
| Announcement output that is a delegate/reject is ignored | `realtime_announce.py:139-141` | code |

---

## 3. Handoff to the main agent

### 3.1 When it happens (`turn_dispatch.py:177-313`)

| `rt.route` | Message sent to os-server `/api/sensing` |
|---|---|
| `realtime_handled` (realtime spoke) | `[skills: input-branching]\n[HANDLED] <STT final>\n[REPLY] <realtime transcript ≤600 chars>` as event `voice_agent_handled` (`turn_dispatch.py:225-248`; cap `hal/config.py:1110`) |
| `delegated` (tool call, local fallback, or error) | `[voice-instruction] <delegate message>\n[transcript] <STT final>`; empty delegate message → STT text alone (`turn_dispatch.py:249-255`). Appends `[realtime-handoff] ...` when realtime already spoke (`:256-270`), `[realtime-context]` JSON-quoted handoff context ≤6000 chars (`:271-279`), `[vision-image]` hint + image (`:280-281`), `[snapshot: ...]` (`:282-283`) |
| `realtime_no_output` / `realtime_unavailable` / `realtime_not_started` | raw STT final text (+ vision hint) (`turn_dispatch.py:300-313`) |
| `ai_rejected` / `noise_dropped` / `foreign_dropped` / `realtime_cancelled` | nothing (`turn_dispatch.py:177-181`, `:314-322`; `realtime_turn.py:359-364`) |

Before dispatch, HAL writes the user's words into realtime memory as handed off
(`voice_service.py:3499-3500`; `orchestrator.py:1981-1986`).

In LIVE mode the delegate path calls `dispatch_turn` with the provider's own input transcription (no
local STT), then hangs up (`voice_service.py:1670-1708`).

### 3.2 What os-server does with each

- `voice_agent_handled`: first calls `CancelSpeechForNewerTurn` (supersede, §3.4), then journals it in
  the external-history store and returns — it does **not** start a live main-agent turn
  (`system/server/sensing/delivery/http/handler.go:355-366`; `realtime_history.go:20-43`;
  `system/server/external_history.go:14-20`). A 2 s loop later sends it to the main agent, when idle,
  as "History only: ... Do not execute, continue or delegate the request again ... You may only note
  mood or other memory. Return NO_REPLY." with the run marked silent
  (`system/externalhistory/sync.go:22-31`, `:104-115`; `realtime.go:9-26`).
- `[voice-instruction]` turns are normal voice turns tagged `voice_handoff`
  (`system/lib/sensingmsg/sensingmsg.go:28-39`); Hermes strips the marker and the `[transcript]` line
  (`runtimes/hermes/steering.go:118-125`).

### 3.3 What the lamp says meanwhile

- On commit the face goes to the `thinking` emotion/LED (`realtime_turn.py:192-205`, `:680-683`).
- Realtime dead-air filler: if no realtime output within `HAL_REALTIME_FILLER_DELAY_S` (code 1.5 s,
  **lamp 0.5 s**, `hal/config.py:1115`; `robots/lamp/rootfs/opt/hal/.env:58`) HAL POSTs
  `http://127.0.0.1:5000/api/sensing/filler` (`realtime_turn.py:207-250`;
  `hal/drivers/voice/_internal/config.py:11`) and os-server speaks one of `"Hmm..."`/`"Mm..."`
  (`system/lib/i18n/fillers.go:19-26`; `deadair_filler.go:270`). Armed only when the transcript has
  more than 3 words (`realtime_turn.py:385-387`, `:377-382`) and auto-fillers are not suppressed
  (`realtime_turn.py:621-627`; lamp standard/pro set `HAL_VOICE_OPENING_FILLERS_ONLY=true`,
  `.env:83`, `overrides/pro/rootfs/opt/hal/.env:2`). Cancelled on the first spoken sentence or on a
  delegate (`realtime_turn.py:700-708`).
- After a delegate, os-server's own fillers apply: it "skips the short opening acknowledgment" for
  delegated turns and may say one continuation "Hmm..." 3.5 s after the first tool start
  (`docs/realtime-voice.md:72-80`; `deadair_filler.go:38-50`) — boundary, not re-verified line by line.
- Per the prompt, realtime says nothing before/after delegating (`system_prompt_gemini.md:43`) —
  model-dependent; if it does speak, dispatch adds `[realtime-handoff]` (`turn_dispatch.py:263-270`).

### 3.4 How the main reply is spoken; supersede

- Main agent replies return to os-server, which sends text to HAL TTS (`deliverTTS`,
  `system/server/agent/delivery/http/handler_tts.go:189-260`). On the lamp that is ElevenLabs — never
  the realtime model's voice.
- HAL feeds each finished main-agent speech back into the realtime session as `[TTS HISTORY]` and into
  `memory.jsonl` as `[Main agent reply]` (`voice_service.py:290-306`, `:365-385`;
  `orchestrator.py:1988-1991`). Cancelled main speech is still fed (`handler_tts.go:207-216`).
- `OS_REALTIME_SUPERSEDES_MAIN_REPLY` (os-server env, default off, `handler_tts.go:161-166`):
  when on and HAL reports `voice_agent_handled`, os-server stamps a watermark; any in-flight main-agent
  turn created before it loses the speaker (its TTS and fillers are dropped; its hardware markers still
  run) (`handler_tts.go:168-187`, `:99-110`; wiring `system/server/server.go:176`;
  `handler.go:357-362`). **Lamp sets it to 1** (`robots/lamp/rootfs/opt/hal/.env:101-112`; os-server
  loads that file, `system/cmd/os-server/main.go:54`).
- Main-agent "usage limit" banner text is replaced by a cached short notice ("[sigh] I've hit my usage
  limit.") at most every 30 s (`handler_tts.go:20-35`, `:220-235`; `system/lib/i18n/phrases.go:150`).

---

## 4. Failure, cost, quota

### 4.1 Fail-fast

- Gemini recv loop: any `ConnectionClosed` (including 1000 idle, `go_away`, 4029), `APIError`, or
  other exception → `_fail_fast_turn()` → if a turn is awaiting output, push
  `TurnDoneEvent(fallback_to_main = "extended-thinking" in model)` so `receive()` stops now instead of
  waiting 8 s; then mark disconnected (`gemini_live.py:1956-2008`, `:1819-1838`).
- `receive()` converts `fallback_to_main` into `MainAgentFallbackOutput` (`base.py:333-345`); the
  orchestrator turns that into `DelegateSignal(message = Gemini's input transcript)`
  (`orchestrator.py:1174-1184`).
- Plain (non-extended) model: no fallback flag → empty turn → `ROUTE_NO_OUTPUT` → raw STT to main
  (`realtime_turn.py:976-995`; `turn_dispatch.py:300-313`).
- Send failures: reconnect; a bound commit whose session changed → `AudioTurnSessionChanged` → one
  synchronous rebuild + full audio replay; if that fails the exception becomes `ROUTE_ERROR`,
  `delegated=True` with no message → STT text to main (`realtime_turn.py:481-511`, `:996-1013`;
  `gemini_live.py:1932-1954`).
- Silent watchdog: `HAL_REALTIME_RECV_QUEUE_TIMEOUT_S` = 8 s gap (`hal/config.py:609-612`;
  `base.py:241-325`), extended while the server keeps talking up to `HAL_REALTIME_TURN_MAX_SILENCE_S`
  = 20 s (`base.py:276-293`) except on extended-thinking turn mode, which uses verified progress up
  to 15 s from commit (`hal/config.py:617-620`; `gemini_live.py:818-827`).
- 1011 replay (`HAL_GEMINI_TURN_RETRIES`=2) only for 2.5 native-audio models
  (`realtime_turn.py:633-638`) — inactive on the 3.8 default.

### 4.2 Backend proxy / BFF

- All realtime, summarizer, outcome-check, STT and search traffic goes to `llm_base_url` (os-server
  config, set at provisioning; empty code default `system/server/config/config.go:318-320`) with the
  device's `llm_api_key`: Gemini Live at `<llm_base_url>/ws/gemini` (`hal/config.py:738-742`),
  summarizer/outcome via Anthropic SDK at `llm_base_url` minus `/v1` (`hal/config.py:1126-1131`),
  Pipecat Qwen relay and Google-Search relay at `campaign-api.autonomous.ai`
  (`hal/config.py:988-992`, `:1032-1035`). The cloud reachability probe is
  `campaign-api.autonomous.ai:443` (`system/network/reachability.go:29`).
- Usage metering, per-device limits and what the BFF sends on a Gemini limit are **not in this repo —
  unverified**. HAL code names `4029` only in comments for Gemini (`gemini_live.py:136`, `:1962`);
  GPT-Live has explicit close-code handling `4001/4002/4029` (`hal/realtime/voice_agent/gpt_live.py:79-83`),
  and the doc describes the GPT-Live BFF contract ("`4029` device over its usage limit",
  `docs/realtime-voice.md:2082-2108`).
- HAL logs token usage and an estimated cost per Gemini usage event to `hal.realtime.usage`
  (rates `gemini_live.py:60-87`, log `:613-660`). Nothing in HAL enforces a budget.

### 4.3 Walkthrough: the user hits the Gemini usage limit (lamp default, LIVE off)

Assumption (unverified): the BFF closes the Gemini WebSocket (e.g. code 4029) and refuses new
connections until the limit resets.

**Turn N (limit hit while Gemini is answering):**
1. HAL STT (Deepgram direct if `deepgram_api_key`, else `AutonomousSTT` via campaign-api,
   `hal/runtime.py:575-592`) runs in parallel as always.
2. Gemini socket closes → `_fail_fast_turn("ws close 4029")` → `TurnDoneEvent(fallback_to_main=True)`
   (extended-thinking) (`gemini_live.py:1978-1989`, `:1819-1838`).
3. Orchestrator yields `DelegateSignal(message=<Gemini's partial input transcript>)`
   (`orchestrator.py:1174-1184`). If the user already heard part of a realtime answer, that speech
   stays heard.
4. Dispatch: `[voice-instruction] <Gemini transcript>\n[transcript] <STT final>`, or just the STT text if
   Gemini had no transcript (`turn_dispatch.py:249-255`) → os-server → main agent (Hermes) → reply
   spoken by ElevenLabs via os-server (`handler_tts.go:189-260`).
   (If the limit hits during upload instead: commit raises, one rebuild is attempted, fails →
   `ROUTE_ERROR` → STT text to main, `realtime_turn.py:481-511`, `:996-1013`.)

**Turn N+1 … (limit still active):**
1. `orchestrator.available` is false because the agent's `_connected` is clear
   (`orchestrator.py:491-504`; `base.py:78-81`). The agent's recv thread keeps retrying the connect
   with backoff 2 → 4 → … → 60 s (`gemini_live.py:1755-1817`, `:1961-1966`).
2. Capture: `prepare_turn()` may attempt a synchronous rebuild (e.g. ≥60 s idle → pre-turn recycle),
   which fails and is logged (`orchestrator.py:839-855`, `:655-668`). `start_realtime_turn` binds
   nothing because realtime is unavailable (`voice_service.py:2647-2650`).
3. `run_realtime_turn` → `ROUTE_UNAVAILABLE` ("Enabled but agent not available — falling back to OS
   server", `realtime_turn.py:1037-1041`). No `thinking` cue and no realtime dead-air filler (those
   live inside the realtime branch, `realtime_turn.py:608-628`).
4. Dispatch: raw STT final text → main agent (`turn_dispatch.py:300-313`); the user turn is still
   written to realtime `memory.jsonl` (`voice_service.py:3499-3500`).
5. So each turn is served by: **STT = HAL's Deepgram/AutonomousSTT; brain = main agent (Hermes);
   voice = ElevenLabs via os-server TTS**, with os-server's main-agent fillers. Wake-word gating still
   applies (it runs on STT). Expect higher latency: every chit-chat turn now costs a full main-agent turn.
6. `[TTS HISTORY]` injections are silently dropped (agent not available, `base.py:185-189`), but the main
   replies still land in `memory.jsonl` (`orchestrator.py:1988-1991`).
7. Nothing tells the user the realtime layer is down; no HAL code speaks a Gemini-quota notice. The only
   spoken quota notices are the os-server main-agent banner replacement (`handler_tts.go:220-235`) and
   the TTS quota phrase "I'm out of voice quota — check my plan." for TTS 429/quota errors
   (`hal/i18n.py:152-157`; `hal/drivers/voice/tts/backend.py:19-20`). Whether the main agent, STT and TTS
   share the same per-device limit as Gemini: **unverified** (BFF not in repo).

**Recovery:** when a background reconnect succeeds, `_connected` is set and the next turn uses Gemini
again (`gemini_live.py:1804-1809`). Edge path: if the failing turn had a delegate (session marked
`requires_fresh_session`), the next `prepare_turn()` rebuild uses `discard_old_on_failure=True` and a
failure sets `_agent=None` (`orchestrator.py:811-824`, `:655-668`); from then on only later
`prepare_turn()` rebuild attempts (pre-turn recycle at ≥60 s session idle) can restore a session —
there is no background retry for `_agent=None` (traced from code; not observed on a device).
If the limit is already hit at boot, the startup retry loop (2 s → 60 s) owns reconnects
(`startup.py:58-65`; `orchestrator.py:959-986`).

### 4.4 Cost controls in code

Idle park (45 s Gemini / 30 s GPT-Live), pre-turn recycle (60 s), post-turn idle reset (240 s),
turn cap (12), zombie (3), vision interval (10 s) and one image per turn, 768 px frames, noise guard
before commit, context compression (32k/24k), summary caps, ACK withheld after a handoff on an
abandoned session (`orchestrator.py:1329-1351`). Values in §8.

---

## 5. Realtime memory

- Files (per active runtime workspace, never shared across runtimes, `hal/config.py:1076-1078`):
  - `memory.jsonl` — working memory; one JSON line `{ts, user, agent}` per turn
    (`base.py:409-426`). Default path `<workspace>/realtime/memory.jsonl` — on Hermes
    `/root/.hermes/realtime/memory.jsonl` (`hal/config.py:1043`, `:1059-1061`, `:1078`).
  - `memory_raw.jsonl` — append-only archive, trimmed to the last 500 when over 1000 lines
    (`base.py:219-221`, `:553-569`; `hal/config.py:1079-1080`).
  - `summary.md` — realtime summary (`base.py:212`).
  - `device_summary.md` — summary of OpenClaw-layout `memory/*.md`; Hermes does not summarize device
    memory (`hermes.py:96-98`; `openclaw.py:161-232`).
- What is written: realtime-handled turns (`realtime_turn.py:970-975`; `voice_service.py:3431`);
  handed-off user turns with agent text "[This request was handed to the main agent; its spoken reply
  follows.]" (`orchestrator.py:1981-1986`); main-agent spoken reply fragments as user
  "[Main agent reply]" (`orchestrator.py:1988-1991`); Harness announcements as "[Harness update]"
  (`announcer.py:168`); LIVE delegates (`voice_service.py:1682`).
- Summarizer: `RealtimeSummarizer` (Anthropic Messages API, streaming), model
  `claude-haiku-4-5-20251001`, key `llm_api_key`, base `llm_base_url` minus `/v1`, max_tokens 4096,
  thinking disabled for memory, 120 s client timeout, 2 retries with 1.5 s doubling backoff, input
  capped to the last 100k chars (`hal/realtime/summarizer.py:15-165`; `orchestrator.py:369-376`;
  `hal/config.py:1126-1139`). Prompt `resources/summarize_prompt.md:1-35` (third person, oldest→
  newest, `## Current activity` first while a quiz/debate/game is running, `## Open requests` last).
- When it runs: in the background when verbatim turns exceed 75 % of the 8000-char budget
  (`base.py:532-581`; `hal/config.py:1091-1093`); at startup (catch-up, background, `startup.py:67-68`;
  `orchestrator.py:993-999`); at `stop()` unless a mode transition (`orchestrator.py:1001-1024`). It
  keeps the newest 4 turns (≤4000 chars) verbatim and removes only the lines it read
  (`base.py:279-345`; `hal/config.py:1097-1099`).
- Budgets: verbatim turns ≤8000 chars newest-first (`base.py:514-530`); `summary.md` ≤5000 chars,
  shrunk by dropping oldest bullets but keeping `## Current activity` / `## Open requests`
  (`base.py:29-57`, `:323-330`); open requests and current activity expire after 3600 s
  (`base.py:75-153`, `:473-491`; `hal/config.py:1101`); device memory ≤8000 (OpenClaw layout only);
  identity ≤12000 (OpenClaw layout only).
- Injection: as `# REALTIME MEMORY` inside the system instruction, **only when a session is built**
  (`base.py:386-387`). Within a session, continuity comes from Gemini's own context.
- Relation to main memory:
  - Realtime **reads** the main agent's persona/memory files read-only at each session build
    (SOUL.md, USER.md, MEMORY.md, skills) (`hermes.py:30-94`).
  - Realtime never writes main-agent memory files. Realtime-handled exchanges reach the main agent
    only as silent "History only ... You may only note mood or other memory. Return NO_REPLY" messages
    (`system/externalhistory/sync.go:22-31`).
  - Main-agent replies reach realtime as `[TTS HISTORY]` (live) and `[Main agent reply]` lines in
    `memory.jsonl` (durable).
  - The prompt tells the model: recall of earlier sessions is delegated, "even when a paraphrase of it
    appears here" (`system_prompt_gemini.md:83`); questions about the current conversation are answered
    from realtime memory and never delegated (`:61`; `routing_prompt_gemini.md:6`).

---

## 6. Announcements, `look`, Google Search

### 6.1 Device-initiated turns (Harness updates)

- Only Harness updates use realtime announcements in this code (`hal/drivers/harness/announcer.py`;
  `realtime_announce.py:1`).
- Gate: TTS idle and ≥1.5 s since last speech/transcript, no music, not listening/capturing/live
  speaking, no realtime turn in flight (`announcer.py:98-120`; `hal/config.py:1152`).
- Realtime used only when enabled, not in LIVE, provider `supports_announce`, Harness voice mode not
  bypassing realtime (`announcer.py:122-133`). Gemini supports announce only with LIVE off and a
  non-native-audio model (`gemini_live.py:170-174`).
- Envelope `<harness_update><instructions>…</instructions><content>…</content></harness_update>`,
  content ≤4000 chars, tags neutralized (`realtime_announce.py:57-89`; templates
  `resources/harness_announce_result.md`, `harness_announce_progress.md`).
- Sent as `clientContent` with `turn_complete=True` (`gemini_live.py:477-491`), streamed through the
  same `stream_output()` with a 20 s receive watchdog (`orchestrator.py:1492-1526`, `:78-79`). A user
  capture preempts it and the rest is drained ≤3 s (`orchestrator.py:775-788`, `:1528-1544`); the
  snapshot is requeued (`announcer.py:174-177`). A delegate/reject in reply is ignored
  (`realtime_announce.py:139-141`). Spoken text saved as "[Harness update]" (`announcer.py:166-168`).
- Fallback (no realtime, LIVE on, or nothing spoken): Haiku summarizer with 12 s bound, else sanitized
  text, spoken by TTS; progress-only updates are not spoken by fallback (`announcer.py:178-212`).
- Progress cadence knobs: speak probability 0.15, ≥60 s gap per run, quiet first 15 s, max age 30 s;
  results/questions expire after 600 s (`hal/config.py:1141-1150`).

### 6.2 `look` — see §2.2. Gemini only, lamp has it.

### 6.3 Google Search grounding — built-in Gemini tool (§2.2). Prompt routes public live facts to it
(`system_prompt_gemini.md:63-64`); grounding metadata extends the wait and feeds handoff context
(`gemini_live.py:1228-1276`). Not available to OpenAI/GPT-Live; Pipecat uses the client `web_search`.

---

## 7. Decision points

| Decision | Rule | Code/model | Default | Where |
|---|---|---|---|---|
| Realtime on, which provider | config | code | on, gemini | `hal/config.py:566-576`; `realtime.go:94-117` |
| Commit this capture to the model? | not filler-only; not short (≤3 words) Silero-non-speech; non-empty STT (REQUIRE_TRANSCRIPT) | code | on | `realtime_turn.py:400-447`, `:602-603` |
| Open realtime in wake-word mode | only after STT final confirms wake phrase, or follow-up window | code | lamp wakeword=true | `voice_service.py:2592-2629`; `ROBOT.md:20` |
| Skip realtime: Harness follow-up | OS reports active Harness follow-up | code | — | `realtime_turn.py:594-596` |
| Speak / delegate / reject / stay silent | prompt + tool descriptions | **model** | direct answer by default; "when in doubt, delegate"/"stay silent" | `system_prompt_gemini.md:19-20,39,86`; `orchestrator.py:81-209` |
| Silent model turn | → main agent with STT text | code | — | `realtime_turn.py:976-995`; `turn_dispatch.py:300-313` |
| Extended-thinking answer accepted? | IDLE with speech & no pending work; else Haiku COMPLETE/CLARIFICATION; else fallback to main | code+LLM | grace 6 s, outcome ≤10 s, progress ≤15 s | `gemini_live.py:882-940`, `:1597-1631`; `response_outcome.py:11-77` |
| `<no speech>`-only output | treated as rejection | code | — | `orchestrator.py:1379-1388`; `realtime_turn.py:889-897` |
| Reject after speech began | ignored in turn mode | code | — | `orchestrator.py:1254-1268` |
| Rejected turn reaches main? | no, if filter on | code | on | `realtime_turn.py:354-364` |
| Foreign-script reply | suppressed, turn dropped | code | on | `realtime_turn.py:419-430`; `hal/config.py:695-698` |
| Answer owed to main question | forwarded live even if handled/rejected | code | 300 s window | `realtime_turn.py:1045-1061`; `hal/config.py:1106` |
| Find request via `look` | redirected to delegate | code (regex) | — | `orchestrator.py:1671-1703` |
| Look: capture? | camera not off; once per turn; ≥10 s since last image | code | — | `orchestrator.py:1718-1761` |
| Express emotion | model picks marker/tool; code validates enum and 0–1 intensity, dedups per turn | model + code | lamp: inline markers | `emotion_markers.py:58-79`; `orchestrator.py:574-582` |
| Thinking face during wait | on commit; cleared on first speech | code | — | `realtime_turn.py:192-205`, `:253-270` |
| Dead-air filler | after 0.5 s (lamp) without output, >3 words, fillers not suppressed | code | 1.5 s code | `realtime_turn.py:207-250`, `:621-627` |
| End the call | model (`end_conversation`), hang up after 3 s | model | LIVE only | `orchestrator.py:1231-1248`; `voice_service.py:1709-1720` |
| LIVE hang-up | 15 s idle; 600 s max; >3 unprompted replies | code | — | `voice_service.py:2086-2114`; `_internal/config.py:139-154` |
| Older main reply silenced by newer realtime answer | supersede watermark | code (os-server) | off in code, **on for lamp** | `handler_tts.go:161-187`; lamp `.env:112` |
| Announce Harness update | gate + provider support; model phrases it | code + model | — | `announcer.py:98-177` |
| Session rebuild/park/recycle | timers/counters | code | §8 | `orchestrator.py:775-1440` |

---

## 8. Timing values

| Value | Default | Where |
|---|---|---|
| Receive gap watchdog | 8.0 s | `hal/config.py:609-612`; `base.py:241-243` |
| Max silent turn while server active | 20.0 s | `hal/config.py:621-624`; `base.py:276-293` |
| Extended-thinking NON_BLOCKING routing grace | 6.0 s | `hal/config.py:613-616`; `gemini_live.py:767-773` |
| Extended-thinking progress deadline (from commit) | 15.0 s | `hal/config.py:617-620`; `gemini_live.py:818-827` |
| Spoken-outcome classifier deadline | 10 s | `hal/config.py:1124`; `gemini_live.py:842-843` |
| IN_PROGRESS interaction stall | max(20, 8) = 20 s, renewed by output | `gemini_live.py:1323-1333` |
| Continuation buffer cap | 2,000,000 bytes / 16,000 chars | `gemini_live.py:1379-1382` |
| Look watchdog | 20.0 s | `hal/config.py:629-631` |
| Look image interval | 10.0 s | `hal/config.py:906-908` |
| Look frame handoff max age | 45.0 s | `hal/config.py:911-913` |
| Look aim deadline | 8 s | `hal/config.py:782-784` |
| Capture settle | 0.3–0.5 s | `orchestrator.py:273-281` |
| Camera wake for capture | 2.0 s | `orchestrator.py:1903` |
| Announcement receive watchdog | 20.0 s | `orchestrator.py:78-79` |
| Announcement drain | 3.0 s | `orchestrator.py:76-77` |
| Prewarm join | 4.0 s | `orchestrator.py:72-73` |
| Turn-in-flight max | 120 s | `orchestrator.py:74-75` |
| Idle park poll | 5.0 s | `orchestrator.py:70-71` |
| Gemini idle park | 45 s | `hal/config.py:645-647` |
| GPT-Live idle park | 30 s | `hal/config.py:978` |
| Gemini pre-turn recycle | 60 s | `hal/config.py:641-643` |
| Post-turn idle reset | 240 s | `hal/config.py:637-639` |
| Turn cap | 12 turns | `hal/config.py:653-655` |
| Zombie reconnect | 3 silent turns | `hal/config.py:633-635` |
| Initial connect retry | 2 s → 60 s | `orchestrator.py:68-69` |
| Agent reconnect backoff | 2 s → 60 s | `gemini_live.py:134-138` |
| Gemini send timeout / one receive turn / join | 10 s / 300 s / 5 s | `hal/realtime/config.py:189-192` |
| Commit waits for previous turn done | 10 s | `gemini_live.py:1921-1922` |
| Connect/disconnect submit default | 30 s | `gemini_live.py:1840-1846` |
| Realtime dead-air filler | 1.5 s (lamp 0.5 s) | `hal/config.py:1115`; lamp `.env:58` |
| Main follow-up window | 300 s | `hal/config.py:1106` |
| Summarizer client timeout / retries / backoff | 120 s / 2 / 1.5 s ×2 | `summarizer.py:65`; `hal/config.py:1133-1139` |
| Open-request / current-activity TTL | 3600 s | `hal/config.py:1101` |
| Harness announce grace / summarizer bound | 1.5 s / 12 s | `hal/config.py:1152`, `:1156` |
| LIVE hang-up grace / idle / max | 3 s / 15 s / 600 s | `_internal/config.py:139-154` |
| External-history flush tick | 2 s | `system/externalhistory/sync.go:104-115` |

---

## 9. Configuration: code defaults vs what the lamp sets

### 9.1 `config.json` (os-server seed, re-seeded while unpinned)
`realtime = {enabled:true, provider:"gemini", gemini:{model:"gemini-3.8-live-extended-thinking",
voice:"Kore", thinking_level:"LOW"}, openai:{gpt-realtime-2, alloy, minimal}, gptlive:{gpt-live-1, marin},
pipecat_v1:{qwen/qwen3.6-35b-a3b}}` (`system/server/config/realtime.go:94-117`; `config.go:433-445`).
`google_search` and `vision` are not seeded → HAL defaults true.
Other inputs: `llm_api_key`, `llm_base_url`, `stt_language`, `stt_model`, `agent_runtime`,
`voice_input_mode`, `wakeword`, `wake_followup_s` (`hal/config.py:526-602`).

### 9.2 `robots/lamp/ROBOT.md`
gateway default `hermes` (`:9-10`); `voice.tts_provider: elevenlabs` (`:13`); `wakeword: true` (`:20`,
adopted only on a fresh config); capabilities `vision`, `expression` (`:23`, `:33`); no
`answer_overheard_speech`.

### 9.3 `robots/lamp/rootfs/opt/hal/.env` (all lamp builds)
| Key | Lamp | Code default |
|---|---|---|
| `HAL_REALTIME_NATIVE_AUDIO` | false (`:52`) | false |
| `HAL_REALTIME_REQUIRE_TRANSCRIPT` | true (`:53`) | true |
| `HAL_REALTIME_REQUIRE_SPEECH_ON_EMPTY_STT` | true (`:54`) | true |
| `HAL_REALTIME_NOISE_SPEECH_RATIO` | **0.45** (`:55`) | 0.55 |
| `HAL_VOICE_NOISE_MIN_VOICED_MS` | **160** (`:56`) | 0 |
| `HAL_REALTIME_TURN_DETECTION` | off (`:57`) | off |
| `HAL_REALTIME_FILLER_DELAY_S` | **0.5** (`:58`) | 1.5 |
| `HAL_REALTIME_ZOMBIE_RECONNECT_AFTER` | 3 (`:69`) | 3 |
| `HAL_WAKEWORD_FOLLOWUP_TIMEOUT_S` | **5** (`:81`) | 10 |
| `HAL_VOICE_OPENING_FILLERS_ONLY` | **true** (`:83`) | false |
| `OS_REALTIME_SUPERSEDES_MAIN_REPLY` | **1** (`:112`, os-server) | off |
| `HAL_LIVE_MODE` | false (`:130`) | false |
| `HAL_LIVE_UPLINK_DURING_PLAYBACK` | cancelled (`:131`) | mute |
| `HAL_LIVE_IDLE_HANGUP_S` | 15 (`:132`) | 15 |
| `HAL_LIVE_MAX_UNPROMPTED_REPLIES` | 3 (`:133`) | 3 |
No `HAL_GEMINI_*` or `HAL_GPTLIVE_*` keys are set for the lamp.

### 9.4 Hardware-profile overrides (merged key by key, `scripts/provision/apply-overrides.py:28-40`)
- `pro`: only `HAL_VOICE_OPENING_FILLERS_ONLY=true` (`overrides/pro/rootfs/opt/hal/.env:2`).
- `pro-respeaker-lite` and `pro-xvf3800`: **`HAL_LIVE_MODE=true`**, `HAL_GEMINI_USE_LANGUAGE_CODES=true`,
  LIVE VAD start/end `high`, prefix 100 ms, silence 1000 ms, duck 0.4, uplink during playback `always`,
  fillers-only false, software AEC off (`overrides/pro-respeaker-lite/rootfs/opt/hal/.env:13-31`;
  `overrides/pro-xvf3800/rootfs/opt/hal/.env:11-23`).
- No profile file (or `standard`) keeps the base package (`apply-overrides.py:18-25`).
**Which profile the shipping lamp uses: unverified** (selected at image build, `scripts/imager/build.sh:1838`).

### 9.5 `hal/env-presets/`
Three manual VAD presets ("Copy one preset to the device HAL `.env`", `hal/env-presets/README.md:3`);
not applied automatically. Their realtime keys: REQUIRE_TRANSCRIPT true, REQUIRE_SPEECH_ON_EMPTY_STT
true, NOISE_SPEECH_RATIO 0.55, TURN_DETECTION off (`01-balanced-cost-safe.env:28-32`).

### 9.6 Effective lamp realtime stack (standard/pro, LIVE off)
Gemini Live `gemini-3.8-live-extended-thinking`, thinking LOW, Google Search on, `look` on, manual
activity brackets, tools `delegate_to_main` + `reject_turn` + `look` (all NON_BLOCKING), emotion via
inline `[HW:/emotion:{…}]` markers, Gemini audio discarded, text spoken by ElevenLabs, Hermes context,
supersede on, wake word on.

---

## 10. Non-default providers (OpenAI Realtime, GPT-Live, Pipecat v1)

(From a delegated read of `openai_realtime.py`, `gpt_live.py`, `pipecat_*.py`; key lines spot-checked:
`gpt_live.py:79-84`, `:956-970`, `pipecat_v1.py:109-113`, prompt lines below.) Paths:
OR = `hal/realtime/voice_agent/openai_realtime.py`, GL = `.../gpt_live.py`, PV = `.../pipecat_v1.py`,
PP = `.../pipecat_pipeline.py`, PS = `.../pipecat_stt.py`, WS = `hal/realtime/web_search.py`,
RC = `hal/realtime/config.py`.

**Common.** None of the three ever emits `TurnDoneEvent(fallback_to_main=True)`; only Gemini does
(`gemini_live.py:982`, `:1010`, `:1830`). Their failure signal is a bare `TurnDoneEvent()` (no
`execution_completed`), so a failed turn becomes `ROUTE_NO_OUTPUT` → STT text to main. Reconnect
backoff 2 s → ×2 → 60 s (OR:730-765, GL:822-845, PV:237-260). Each has its own prompt file
(`context_manager/base.py:168-174`).

### 10.1 OpenAI Realtime (`openai`)
- `gpt-realtime-2`, voice `alloy`, 24 kHz both ways, reasoning `minimal`, transcription
  `gpt-4o-mini-transcribe`, noise reduction `far_field` (`hal/config.py:925-936`); truncation fixed at
  retention ratio 0.5 (RC:63-64).
- Audio appended continuously (OR:251-253). Turn mode: commit → `input_audio_buffer.commit` +
  `response.create` after waiting ≤10 s for the previous response (OR:290-316; RC:76). LIVE: server_vad
  (threshold 0.7 for `low`, 0.3 for `high`), client commit skipped (OR:146-173, 294-297).
- Turn ends on `response.done` (completed and not interrupted ⇒ `execution_completed`) (OR:664-694).
  Barge-in truncates the server item to what was heard (OR:402-472, 539-554).
- Tools native (`tool_choice: auto`, OR:201-203); results as `function_call_output`, `response.create`
  only when `trigger_response` (OR:278-288). Audio + transcript out; model audio played only if native
  audio is on (OR:591-641).
- Errors: non-benign `error` event → `OpenAIRealtimeError` → fail-fast + drop (OR:856-867); no
  quota-specific code; no idle parking (`orchestrator.py:903-910`); no announce support (base default,
  `base.py:191-194`).
- Delegate notes: `_turn_done` is cleared only in `_safe_response_create` (OR:314), so with server VAD
  fail-fast is a no-op and the 8 s watchdog ends the turn; the `ImageInput` branch is unreachable
  (`look` is Gemini-only); `express_emotion`'s always-true ACK triggers an extra `response.create`
  here.

### 10.2 GPT-Live (`gptlive`, OpenAI `/v1/live`)
- `gpt-live-1`, voice `marin` (13 voices), 16 kHz both ways (`hal/config.py:953-956`); URL falls back to
  the OpenAI realtime URL, SDK appends `/live/sessions` (`hal/config.py:947-952`); `x-request-id:
  hal-<12 hex>` per connect (GL:259-269).
- The wire has no turn/VAD/tool events: the adapter synthesizes them (GL:1-6). Turn mode "commit" =
  600 ms of appended silence (GL:386-402); new user turn after a 1500 ms transcript gap
  (GL:431-459); reply ends after 800 ms of output silence, or 400 ms if the user overlapped
  (interrupted) (GL:461-557); output below −50 dBFS dropped (`hal/config.py:967-972`).
- No tools at the Live layer; only `delegate_to_main` is honoured (GL:53, 160-168). Delegation
  `client|responses|auto`; `auto` = `responses` when web search is on (default true) → **effective
  default `responses`** (RC:99-106; `hal/config.py:958-963`): a backend model `gpt-5.6-luna` with hosted
  `web_search` and a `delegate_to_main` function (GL:194-229). Client mode turns
  `session.delegation.created` into `delegate_to_main` after the transcript settles 0.25 s (≤500 ms)
  (GL:595-648). Context injected via `session.thinking.append`, ≤1600 chars (GL:332-343).
- Close codes 4001/4002/**4029 ("device over its GPT-Live usage limit")** push backoff straight to the
  60 s cap — the docstring says "stop retrying" but it keeps retrying every ~60 s (GL:79-84, 956-970).
  Any non-HAL `error` → fail-fast + drop (GL:762-771, 934-939). Idle park 30 s; disconnect sends
  `session.close` and waits ≤5 s for final usage (GL:278-298; RC:115). Billing logged at $0.05/min
  (GL:50-51, 773-788). No announce support.

### 10.3 Pipecat v1 (`pipecat_v1`, on-device pipeline, text out)
- Qwen `qwen/qwen3.6-35b-a3b` via the campaign-api chat-completions relay, temperature 0.7, 300 max
  tokens, thinking disabled (`hal/config.py:987-999`; PP:321-336). STT = VoiceService's provider, else
  `AutonomousSTT` (PV:124-142). Needs the `pipecat` extra (PV:173-178).
- Pipeline STT → user aggregator → LLM → sink (PP:347-349). Turn mode: commit sends `STTFinalizeFrame`;
  turn stops on the finalized transcript (PV:284-290, 319-336). LIVE: Silero (0.85/0.2 s/0.2 s/0.7) +
  Smart Turn v3 (stop 3.0 s) or 0.8 s timeout; ≥2 words to barge in while busy (PP:270-295;
  `hal/config.py:1006-1021`).
- All orchestrator tools bridged via futures; `delegate_to_main`/`reject_turn` never re-run the LLM
  (PV:50, 379-431); the action rule is re-injected before each utterance and appended as a "FINAL RULE"
  (PP:70-79, 208-216, 317-320). Client `web_search` via the Google-Search relay
  (`gemini-3.7-flash`, 10 s; answers ≤1200 chars, ≤5 sources) (WS:18-62, 110-145).
- Output text only → HAL TTS (PV:552-564). Tool result timeout 15 s, Pipecat function timeout 20 s
  (PV:399-410; PP:335); start timeout 60 s (RC:150-151). No quota code, no idle park. Announce
  supported only outside LIVE (PV:109-113).

### 10.4 Prompt routing differences (non-Gemini)
- `system_prompt.md` (fallback only): "Execute the tool call OR emit spoken audio ... If you call
  `delegate_to_main`, your spoken audio output must be completely blank" (`:32`); live feeds like
  weather delegate (`:56`).
- `system_prompt_openai.md`: same routing (`:52-83`), "**You cannot perform actions** ... MUST delegate
  with blank voice output" (`:73`).
- `system_prompt_pipecat.md`: input is an STT transcript, output text for TTS (`:3-7`); `web_search` is a
  direct answer (`:77`) yet "Live External Feeds ... real-time local weather" still delegates (`:88`) —
  internal contradiction.
- `system_prompt_gptlive.md`: "For other handoffs, say at most a two-word acknowledgment" (`:43`);
  backend prompt `system_prompt_gptlive_backend.md` uses `web_search` for public facts (`:7`).
- All five say "Do not call `complete_response`" (default `:30`, openai `:54`, pipecat `:60`, gptlive
  `:26`, backend `:4`), while the Gemini prompt tells the model to call it "if available".

---

## 11. Surprising, dead, disabled or contradictory

1. **Model default split**: HAL code default `gemini-3.8-live` (`hal/config.py:745`) vs os-server seed
   `gemini-3.8-live-extended-thinking` re-written on every start while unpinned
   (`realtime.go:73`; `config.go:433-445`). The Go struct comment still says `"gemini-…-live-preview"`
   (`realtime.go:27`).
2. **Gemini's voice is never heard on the lamp**: audio is generated and discarded; ElevenLabs speaks
   the transcript (`gemini_live.py:240-260`; lamp `.env:52`; `ROBOT.md:13`). "Speech-to-speech" in the
   doc intro (`docs/realtime-voice.md:33-37`) is not how the lamp runs.
3. **`complete_response` is dead but pervasive**: never declared (`gemini_live.py:284-287`), yet
   referenced in the delegate description (`orchestrator.py:125`), the Gemini prompt (`:21,22,41-44,89,101`)
   and the routing prompt (`:2,6,8,10,12`); only a legacy handler remains (`gemini_live.py:1657-1682`).
4. **`express_emotion` tool is not used on the lamp default**; inline markers replace it
   (`orchestrator.py:574-582`). The tool's ACK is always `trigger_response=True` ("patch"),
   contradicting its own comment (`orchestrator.py:1630-1641`) and the doc, which says a spoken turn is
   not acked on Gemini (`docs/realtime-voice.md:1449-1460`).
5. **Prompt contradicts itself on acknowledgments before delegation**
   (`system_prompt_gemini.md:43` vs `:55`, `:66`); doc says "The Gemini prompt allows one immediate,
   brief acknowledgement before a delegate" (`docs/realtime-voice.md:571-572`).
6. **Silence is not "stay quiet"**: a model that stays silent falls through to the main agent with the
   STT text (`realtime_turn.py:976-995`; `turn_dispatch.py:300-313`); only `reject_turn` or a
   `<no speech>` marker actually keeps the lamp quiet.
7. **Doc table says Gemini skips `send_text`** (`docs/realtime-voice.md:2623`); code drops it only for
   2.5 native-audio models and sends it as `clientContent` on 3.8 (`orchestrator.py:1958-1975`;
   `gemini_live.py:464-476`).
8. **Doc says LIVE has no `[TURN CONTEXT]`** (`docs/realtime-voice.md:2955`); code sends it once at
   live-session entry (`voice_service.py:2249-2252`).
9. **Doc names prompt sections that do not exist in the Gemini prompt** ("Direct Home Run",
   `docs/realtime-voice.md:1570`, `:2403`; "Expression Exception", `:1535` — that heading exists only in
   `system_prompt.md:33`, `_openai.md:57`, `_pipecat.md:63`).
10. **Native-audio-only machinery is inactive on 3.8**: 1011 replay retries (`realtime_turn.py:633-638`),
    WS keepalive pings (`gemini_live.py:102-109`), text-drop guard (`orchestrator.py:1960-1970`).
11. **`send_function_result()` has no callers** outside tests (`orchestrator.py:1993-1996`).
12. **`HAL_REALTIME_MIN_COMMIT_DURATION_S`** is consulted only when REQUIRE_TRANSCRIPT is false
    (`realtime_turn.py:443-447`) — inactive on the lamp.
13. **Hermes persona and MEMORY.md are uncapped** in the realtime prompt (`hermes.py:30-60`), while
    OpenClaw-layout runtimes are capped at 12000/8000 chars (`openclaw.py:47-55`, `:80-90`).
14. **Realtime-handled turns never run the main agent live**; they become silent history ~2 s later
    when the main gateway is idle (`handler.go:363-366`; `sync.go:104-115`). The doc's route table says
    "The main agent gets `voice_agent_handled` and stays silent" (`docs/realtime-voice.md:407`) — true
    only in the history-sync sense.
15. **No user-facing signal when Gemini is down or over limit**; the lamp silently degrades to
    STT → main agent → TTS (§4.3).
16. **Gemini 4029** appears only in comments; actual BFF behavior is unverified.
17. **Gateway default mismatch**: HAL code default `openclaw` (`hal/config.py:527-531`) vs lamp ROBOT.md
    `hermes` (`ROBOT.md:9-10`) — HAL follows `agent_runtime` in config.json.
18. **Prompt size**: the Gemini prompt file is ~42 KB of rules (`system_prompt_gemini.md`), plus a 4 KB
    delegate tool description, re-sent at every session build; the doc's "~7-10k token" estimate
    (`docs/realtime-voice.md:2592-2594`) was not re-measured here.
19. **GPT-Live 4029**: docstring says it "stop[s] retrying codes that won't change", code only jumps the
    backoff to 60 s and keeps retrying (`gpt_live.py:956-962`).
20. **OpenAI Realtime fail-fast is a no-op under server VAD** (`_turn_done` cleared only in
    `_safe_response_create`, `openai_realtime.py:314`); those turns end on the 8 s watchdog.
21. **Pipecat prompt contradiction**: `web_search` answers public facts directly
    (`system_prompt_pipecat.md:77`), yet "real-time local weather" is listed under delegate
    (`:88`).
22. **Prompts disagree across providers on `complete_response`**: non-Gemini prompts say never call it;
    the Gemini prompt says call it "if available"; it is declared for none.

