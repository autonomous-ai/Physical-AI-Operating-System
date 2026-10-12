# F. V1 Lamp shipped baseline (product level)

Scope: facts only about V1 as deployed. Source checkout: `v1-main` = `origin/main` at
`170a1b85e` ("- deploy", 2026-10-12). All paths are relative to that checkout. "Unverified"
marks anything that the repository alone cannot settle (for example what the cloud OTA
floor actually rolled to the fleet, or what the campaign-api proxy resolves a model alias to).

Repo version files at this commit: os-server `0.1.223` (`system/VERSION_OS_SERVER`), HAL
`0.1.284` (`hal/VERSION_HAL`), web `0.1.90` (`system/VERSION_WEB`), bootstrap `0.0.32`
(`system/VERSION_BOOTSTRAP`), lamp device profile `0.0.109` (`robots/lamp/VERSION`).
Unverified: these are the last *uploaded* versions; the fleet only auto-updates to the cloud
`min_version` floor (`docs/bootstrap-ota.md:203-224`), which is not in the repo. Bootstrap
polls every 5 min (`docs/bootstrap-ota.md:93`, `:327`; `docs/hosted.md:20`).

---

## 1. What actually ships on a Lamp

### 1.1 How the effective configuration is assembled

| Layer | Source | Notes |
|---|---|---|
| Image | `scripts/imager/Makefile` | `DEVICE_TYPE` required (`:33-38`); `DEFAULT_AGENT` optional, bakes `/root/config/f_r_default_agent` (`:40-49`); `VARIANT` optional hardware assembly, empty = standard (`:51-54`). OrangePi is the default target (`:16`). The image does not bake a `config.json` (no match in `scripts/imager/build-orangepi.sh` for `config.json`/`wakeword`/`realtime`). |
| Device profile | `robots/lamp/ROBOT.md`, `SAFETY.md`, `SOUL.md`, `presets.json`, `rootfs/**` | `rootfs/opt/hal/.env` lands as `/opt/hal/.env` (`scripts/imager/README.md:131-133`). |
| Hardware variant | `robots/lamp/overrides/<variant>/` selected by `/etc/autonomous/hardware-profile` | Missing/empty/`standard` = base package (`docs/bootstrap-ota.md:395-398`). Renderer merges the variant `.env` over the shared `.env` and applies `profile.json` volume/capabilities (`docs/bootstrap-ota.md:400-410`). Which variant buyers' lamps carry is a build input; **not recorded in the repo (unverified)**. |
| os-server first boot | `system/server/config/config.go`, `system/server/server.go` | No config file -> `Default()` saved (`config.go:394-403`); realtime block re-seeded from `DefaultRealtimeConfig()` on every start unless `pinned` (`config.go:433-445`); `wakeword` seeded from ROBOT.md only for a fresh config (`server.go:297-309`), pre-existing configs pinned to `false` in memory (`config.go:446-451`); TTS provider/voice seeded from ROBOT.md while unset (`server.go:260-295`). |
| Setup (app or browser) | `POST /api/device/setup` (`docs/setup-flow.md:50-74`, `system/device/setup.go:303-352`) | Writes `llm_api_key`, `llm_base_url`, `llm_model`, `device_id`, optional STT/TTS keys/URLs, `stt_language` (-> `stt_model = nova-3-general`, `system/device/config_update.go:784-791`), MQTT topics. The app "hands over the keys and pairing" (`README.md:64`); "the key comes with the app account" (`docs/hosted.md:14-17`). |
| Runtime selection | `system/device/runtime.go:48-91`, `system/agent/factory.go:32-110` | `config.agent_runtime` > `/root/config/f_r_default_agent` > ROBOT.md `gateway.default` > OpenClaw (`robots/lamp/docs/agent-runtime.md:6-13`). |

### 1.2 Effective lamp settings (Standard variant unless noted)

| Setting | Effective lamp value | Where it comes from | Code default / contradictions |
|---|---|---|---|
| Main agent runtime | **Hermes** | `robots/lamp/ROBOT.md:8-10` (`gateway.default: hermes`), seeded into empty `agent_runtime` (`system/device/runtime.go:62-76`) | Code fallback is OpenClaw (`system/agent/factory.go:96-108`). An image built with `DEFAULT_AGENT` overrides it (unverified which, if any, was used for shipped images). ROBOT.md also says `protocol: websocket` but Hermes uses SSE; os-server only logs a warning (`factory.go:21-44`). `docs/hosted.md:14` and `robots/lamp/docs/product-vision.md:13,93` still call OpenClaw the default/brain. |
| Main agent model | Hermes `config.yaml` `.model.provider = custom:autonomous`, `.model.default = "Auto-AI"` (campaign-api alias), `api_mode: anthropic_messages`, base `https://campaign-api.autonomous.ai/api/v1/ai` | `runtimes/hermes/presync.sh:42-46`, `docs/agentic/hermes.md:650-665` | What "Auto-AI" resolves to is chosen by the proxy ("whatever model it picks", `docs/agentic/hermes.md:654-657`) - **unverified**. `llm_model` (os-server default `claude-opus-4-6`, `config.go:319`) is ignored by Hermes while on campaign-api. Prompt caching on with 1 h TTL, context_length pinned 256000 (`presync.sh:85-95`, `hermes.md:666-687`). Vision sub-model `qwen/qwen3.6-plus` (`presync.sh:56`, `hermes.md:688-690`). Approvals off (`hermes.md:696-702`). |
| Realtime enabled | **true** | `DefaultRealtimeConfig()` (`system/server/config/realtime.go:94-117`) | HAL default also true (`hal/config.py:566-575`). |
| Realtime provider / model | **Gemini Live, `gemini-3.8-live-extended-thinking`, thinking `LOW`** | `realtime.go:70-76,94-103` seeded into `config.json`; HAL resolves env > config.json > default (`hal/config.py:556-563,745-748`) | HAL's own fallback is plain `gemini-3.8-live` (`hal/config.py:743-745`); `README.md:147` says the default is `gemini-3.8-live`. os-server comment: plain 3.8-live "delegates or stays silent every turn" (`realtime.go:71-72`). `docs/overview.md:20` states the seed correctly. |
| Realtime endpoint / key | `{llm_base_url}/ws/gemini`, key = `llm_api_key` | `hal/config.py:732-742` | Session resumption off because "the campaign-api proxy does not forward resumption" (`hal/config.py:754-757`). |
| Realtime voice | `Kore` configured, **but not heard**: `HAL_REALTIME_NATIVE_AUDIO=false`, so realtime text is spoken by HAL TTS | `robots/lamp/rootfs/opt/hal/.env:52`; `hal/config.py:727-730` | Exception: Gemini TTS + Gemini Live plays native audio regardless (`docs/os-server.md:658-665`); not the lamp default. |
| Realtime mode | **Turn-based** (`HAL_LIVE_MODE=false`, `HAL_REALTIME_TURN_DETECTION=off`): HAL VAD + STT endpoint bracket each turn | `.env:52-58,130` | Variants `pro-respeaker-lite` and `pro-xvf3800` set `HAL_LIVE_MODE=true` (full duplex, forces `server_vad`, `hal/config.py:701-725`) (`overrides/pro-respeaker-lite/rootfs/opt/hal/.env:21`, `overrides/pro-xvf3800/rootfs/opt/hal/.env:13`). `docs/bootstrap-ota.md:434-436` says pro-respeaker-lite has "HAL AEC enabled ... Live off", contradicting its `.env` (`HAL_AEC_ENABLED=false`, `HAL_LIVE_MODE=true`). |
| Realtime guards | require transcript; Silero re-check on empty STT; noise ratio 0.45; min voiced 160 ms; zombie reconnect after 3 silent turns; filler after 0.5 s; opening-window fillers only | `.env:53-58,69,83` | Code defaults: noise ratio 0.55, min voiced 0, filler 1.5 s, opening-only false (`hal/config.py:605-608,664-672,1115`). |
| Realtime session cost knobs | idle reset 240 s, max 12 turns/session, Gemini idle park 45 s, pre-turn recycle 60 s, context compression at 32768 -> 24576 tokens | `hal/config.py:636-655,749-752` (code defaults; lamp `.env` does not override) | |
| Realtime in-session tools | Google Search grounding **on**, `look` vision tool **on** (one frame, min 10 s apart) | `hal/config.py:758-773,905-908` | |
| Realtime memory summarizer | on, `claude-haiku-4-5-20251001` via `llm_base_url` | `hal/config.py:1126-1139` | |
| Realtime supersedes main reply | **on** (`OS_REALTIME_SUPERSEDES_MAIN_REPLY=1`) | `.env:101-112` | Code default off; enabled 28/08/2026 after a lamp-0c89 check (`.env:108-111`). |
| TTS provider / voice | **ElevenLabs**, voice by STT language (English default `Rachel`), model `eleven_v4_turbo`, HTTP (not WS), speed 1.2 | ROBOT.md `voice.tts_provider: elevenlabs` (`ROBOT.md:11-12`) seeded by `server.go:260-295`; voices `system/domain/device.go:1358-1391`; model `hal/drivers/voice/tts/elevenlabs.py:29`; `.env:13,68` | HAL fallback when unset is OpenAI (`hal/runtime.py:546`). Key/URL fall back to `llm_api_key`/`llm_base_url` (`hal/runtime.py:541-546`), path `/elevenlabs` (`elevenlabs.py:32`). SOUL directs ElevenLabs v4 audio tags (`robots/lamp/SOUL.md:66`). |
| STT provider | **AutonomousSTT** = Deepgram `nova-3-general` relayed by campaign-api, endpointing 1500 ms, language default `en` | `hal/runtime.py:579-592`; `hal/drivers/voice/stt/autonomous.py:16-21,280-309` | Deepgram direct only if `deepgram_api_key` set (`runtime.py:579-580`). `product-vision.md:123,857,869` and `docs/overview.md:293-298` still describe Deepgram/OpenClaw. |
| Capture tuning | silence timeout 1.2 s, max capture 20 s, pre-roll 12 frames, Silero 0.15, WebRTC VAD aggressiveness 2, warm mic on, software AEC on (205 ms delay) | `.env:6-24,46-66` | Code max capture 30 s (`.env:47-51`). |
| Voice input mode | `automatic` (hands-free) | `system/server/config/voice_input_mode.go:10-16`; `hal/config.py:577-581` | `tap_to_talk` is opt-in. |
| Wake word (speech attention gate) | **On for a freshly set-up lamp**; **off for any lamp whose `config.json` existed before the ROBOT.md key** | `ROBOT.md:13-19`; `server.go:297-309`; `config.go:446-451`; HAL `hal/config.py:582-583` | `robots/lamp/README.md:59` tells buyers "No wake word by default — it answers when you speak", contradicting `ROBOT.md:19`. |
| Wake phrases | built-in `hello|hey|hi|alo|okay|ok|wake up` x `autonomous` / device type / agent name; owner-editable, max 30 | `docs/web-ui.md:313`; `system/server/config/wake_phrases.go:11-21` | `product-vision.md:869` lists "Hey Lamp", "này Lamp", "Lamp ơi". |
| Follow-up window after a wake | **5 s** (lamp image default) | `.env:81`; `hal/config.py:584-602` | os-server and HAL code default 10 s (`wake_phrases.go:18`, `hal/config.py:586-588`); owner setting `wake_followup_s` 3-30 s wins. |
| Addressed gate openers | wake phrase, single click, gaze wake, enrolled person entering view, boot greeting | `robots/lamp/docs/physical-controls.md:183`; `hal/drivers/voice/_internal/wakeword_focus.py:136-150` | With wakeword off every utterance counts as addressed (`wakeword_focus.py:148-149`); the main agent then gets `[ambient]` speech and SOUL asks it to judge addressee (`SOUL.md:48,59`). Stranger-only enter does not open the gate (`.env:84-86`, `HAL_PRESENCE_WAKE_STRANGERS=false`). |
| Gaze wake | **on, not shadow**, yaw cone 60 deg (code 25 deg), 60 % facing vote over 1.5 s | `.env:88-99`; `hal/config.py:823-871` | Code default off + shadow (`hal/config.py:824-830`). Set by hand on lamp-0c89 before being written down (`.env:88-91`). Inert unless wakeword is on (`hal/config.py:823`). |
| Local intent / Jev intent / Jev Harness | local rules **on**; Jev semantic fallback **on** (`{llm_base_url}/jev/decisions`); Jev Harness on | `config.go:628-634`; `system/server/config/jev_intent.go:24-45`; `config.go:244-246` | `config.go:240` comment says Jev "default off"; code and `README.md:53,143` say on. |
| Ambient "lifelike" loop | LED resting restore + servo micro-movement every 45-120 s (`idle`/`curious`/`nod`) when not paused; pauses on interaction, resumes after 60 s quiet | `ROBOT.md:31-34`; `system/ambient/service.go:23,82-91,287-306` | **TTS self-talk (mumble) is disabled in code** (`service.go:308-314`, `mumblePaused = true`) although `ROBOT.md:32-33` and `architecture-decision.md:90,120` describe it. Breathing loop disabled (`service.go:83,205-207`). |
| Proactive sensing turn floor | at most one ambient-sensing agent turn per **120 s** across ambient types | `config.go:680-687`; `robots/lamp/docs/sensing-behavior.md:49-52` | voice/presence enter/leave/fire never floored. |
| Presence timers | dim to 20 % after **5 min** without motion/activity; light off + `presence.away` (agent announces sleep) after **15 min** | `hal/config.py:371-373` (no lamp override); `sensing-behavior.md:158-172` | `product-vision.md:100,106` says leave after 15 min and idle animations after 30+ min; UC-11 "dim after 5 min, off after 15 min" (`product-vision.md:574`) matches. |
| Face / presence enter | friend re-greet after 1 h forget window, stranger 30 min; strangers greeted only once they face the lamp | `hal/config.py:96-114`; `sensing-behavior.md:130` | |
| Cloud camera perceptions (motion/action, facial emotion, pose, fire hazard) | **all off** until the owner opts in under Settings > Privacy | `hal/config.py:240-251,279,307,329`; `config.go:735-750`; `sensing-behavior.md:210-235` | Consequence: `motion.activity`, facial emotion, posture nudges and fire hazard are not produced by default (`sensing-behavior.md:229-230`). |
| Voice perceptions uploading audio | speaker recognition **on**, speech emotion (SER) **on**, both POST audio to the DL backend at `llm_base_url` | `hal/config.py:210-237,375-382,498-515`; `hal/runtime.py:601-602` | `product-vision.md:132` says "no video/audio streamed to cloud for ambient processing". |
| Perception service (DL backend) | base = `llm_base_url`, encrypted payloads (`HAL_DL_ENCRYPTION=true`) | `hal/config.py:210-237`; `.env:41` | |
| Camera | on, 1280x720 capture / 960x540 stream, auto exposure, gain 64; auto-off on `night` scene or `sleepy` | `.env:30-38,70-79`; `robots/lamp/docs/camera-lifecycle.md:30-40` | Pose motion (local) off (`.env:25`). |
| Music | HAL `MusicService` (yt-dlp -> ffmpeg -> aplay) | `hal/runtime.py:619-630`; `docs/overview.md:276-282` | Proactive music suggestion cooldown is **7 min** in the shipped skill, marked "(production: change to 30 min before ship)" (`skills/music-suggestion/SKILL.md:65`); marketing brief says 30 min (`robots/lamp/docs/mood-marketing-copy.md:57`). |
| Wellbeing | thresholds hydration 45 min, break 30 min (20 with yawning) | `skills/wellbeing/SKILL.md:121-128` | Triggered by `[activity]` = `motion.activity`, which needs the off-by-default cloud `motion` opt-in (`skills/wellbeing/SKILL.md:3`; `sensing-behavior.md:229-230`). |
| Schedules | none on device by default; backend-authoritative `schedule.sync` over MQTT; bad/blank timezone -> UTC | `docs/mqtt.md:402-410`; `system/schedule/sync.go:9-30` | |
| Language | STT `en` unless setup sends `stt_language`; replies follow the owner's current turn language | `autonomous.py:17,289`; `SOUL.md:55` | Setup offers Japanese etc. (`docs/setup-flow.md:78`). |
| Volume / light bounds | startup volume 75 (Standard), max 100; brightness ceiling 120, quiet hours 22:00-07:00 ceiling 40; servo 120 deg/s | `ROBOT.md:51`; `SAFETY.md:11-32` | pro-respeaker-lite 35/50, pro-xvf3800 77/77 (`overrides/*/profile.json`); `docs/bootstrap-ota.md:424` says 35/35 for the Lite. |
| LEDs | 32-pixel ring, low-glare palette (channels 0-3) | `robots/lamp/presets.json:3-37` | `product-vision.md:39,172,392` and `architecture-decision.md:64` say 64 LEDs and a GC9A01 eye display; `ROBOT.md:25-29` says lamp has **no screen**. |
| Telemetry off-device | analytics sink **off** (empty URL) | `.env:114-127` | Local log always written. |
| Guard mode | off | `config.go:673-678` | |
| Admin password | 4-char hardware suffix if none set | `docs/setup-flow.md:84` | |

### 1.3 Hardware variants (only differences from Standard)

| Variant | Audio path | Voice behavior differences | Volume start/max |
|---|---|---|---|
| `standard` (no marker) | Jieli voice mic `device_micro2`, C-Media sensing mic, base speaker | as table above | 75 / 100 (`ROBOT.md:51`, `SAFETY.md:17`) |
| `pro` | same as Standard + SEN63C air sensor (`environment` capability) | opening fillers only (`overrides/pro/rootfs/opt/hal/.env:1-2`) | base |
| `pro-respeaker-lite` | ReSpeaker Lite on-chip AEC, software AEC off, Silero 0.10 | **Live (full-duplex) mode on**, Gemini language codes on, live VAD high/high, 1000 ms silence, duck gain 0.4, uplink always (`overrides/pro-respeaker-lite/rootfs/opt/hal/.env:12-31`) | 35 / 50 (`overrides/pro-respeaker-lite/profile.json:2-3`) |
| `pro-xvf3800` | XVF3800 array AEC, software AEC off | Live mode on, same live knobs (`overrides/pro-xvf3800/rootfs/opt/hal/.env:11-23`) | 77 / 77 |

Which variant ships to buyers: not stated in the repo (unverified).

---

## 2. Intended behavior (design documents)

### 2.1 Product vision (`robots/lamp/docs/product-vision.md`, last touched 2026-09-28)
- Pitch: "the best desk lamp ..., the most expressive companion robot, the smartest AI assistant (powered by OpenClaw), and an open-source platform" (`:13`).
- Four pillars: "It Understands Me" (`:21-33`), "It Feels Alive" - the LLM decides servo/LED/audio per reaction (`:35-48`), "It's Actually Useful" (`:50-58`), "It Acts on Its Own" (`:60-133`).
- Split: "Lamp = senses (cheap, always-on). OpenClaw = brain (smart, called when needed)" (`:93`); events are "lightweight - no LLM tokens burned" until significant (`:126`).
- **Who decides table** (`:97-108`):

| Trigger | Lamp action | Who decides |
|---|---|---|
| Arrive (person enters frame) | turn on, warm greeting, preferred light | OpenClaw (personality) |
| Leave (no presence 15 min) | dim -> sleep -> off | Lamp (rule-based) |
| Darkness | raise brightness | Lamp (auto) + OpenClaw (scene) |
| Focus (silence + typing) | hold, suppress interruptions | Lamp detect -> OpenClaw confirm |
| Stress (sighs, tense tone) | warm light, offer break | OpenClaw (empathy, memory) |
| Joy (laughter) | bounce, warm flash | OpenClaw (emotion skill) |
| Late night | reduce blue, reminder | OpenClaw (memory) |
| Idle 30+ min | breathing LED, blink | Lamp (built-in, no AI) |
| Wake up (schedule) | sunrise, chime | Lamp (schedule) + OpenClaw (greeting) |
| Video call | optimize face light | Lamp detect -> OpenClaw adjust |

- Event list claims all "Done" incl. `motion.activity`/`emotion.detected` via perception-service (`:114-124`), which today are off by default (section 1.2).
- Privacy promise: "All sensing runs on-device - no video/audio streamed to cloud for ambient processing" (`:132`).
- NFRs: voice-to-action < 1 s (`:837`; also UC-01 `:375`), TTS < 500 ms after LLM (`:643`), English + Vietnamese (`:840`).
- Status matrix: all P0/P1 done; P2 circadian, auto-tracking, gesture, video call, screen awareness not done; UC-M1/M2/M3 done (`:802-810`).
- Shipped-implementation note: hardware routes are HAL `:5001`; the agent emits inline `[HW:/path:{...}]` markers that os-server parses and forwards (`:297`).

### 2.2 Architecture decision (`robots/lamp/docs/architecture-decision.md`, dated 2026-03-24)
- Two layers: Layer 1 system works without the brain; Layer 2 brain reads SKILL.md and calls hardware (`:15-22`).
- Layer 1 includes an "Autonomous sensing" loop and "Ambient life" with "TTS self-talk", pausing on interaction and resuming after 60 s (`:89-90`).
- Rule-based actions: auto-dim on leave, brightness on darkness, idle animations; AI-driven: greetings, mood, empathy, schedule-aware suggestions (`:108-109`).
- Emotion skill as "key differentiator": one `/emotion` call -> coordinated servo + LED + audio with randomized presets (`:335-356`).
- Doc conflict: `:333` says skills call HAL directly at `:5001`; `product-vision.md:297` and `README.md:135,143` say skills emit `[HW:...]` markers that os-server strips and posts.
- Open question left open: preset vs fully generative body language (`:388`).

### 2.3 Agent runtime (`robots/lamp/docs/agent-runtime.md:1-13`)
- Lamp defaults to Hermes; precedence saved `agent_runtime` > `f_r_default_agent` > ROBOT.md; changing ROBOT.md does not switch configured devices.

### 2.4 HW call strategy (`robots/lamp/docs/hw-call-strategy.md`, Vietnamese)
- Current: inline markers - 1 inference round, hardware fires late (after turn end), depends on LLM format, LLM "sometimes strays" to exec/curl (`:3-13`); exec tool path - N+1 rounds, fires immediately (`:15-24`).
- Proposed `/hw/batch` endpoint (`:26-95`); it was written for OpenClaw on `claude-haiku-4-5` (`:81`). No `/hw/batch` exists in `hal/`, `system/` or `skills/` at this commit (grep: no matches) - proposal not implemented.

### 2.5 Platform description (`README.md`, `docs/overview.md`)
- "One voice, three ways to act": a conversation stays with realtime; a music request goes to the main runtime's skills; a computer task goes to Harness and returns a short spoken result (`README.md:113-117`).
- os-server `intent` = local rules then Jev fallback (on by default), deferring contextual follow-ups to the main agent; `server` strips `[HW:...]` markers and posts them to HAL before words are spoken (`README.md:143`).
- Realtime "answers directly or delegates tasks to the main runtime" (`README.md:147`; `docs/overview.md:20`).
- Lamp quick start promise: "Say something, it turns to look at you, the ring lights up, and it answers" (`README.md:65`).
- `docs/overview.md:291-299,319-328` still shows the older pipeline (RMS VAD -> Deepgram -> local intent -> OpenClaw; cooldowns motion/sound 60 s, presence 10 s, light 30 s).

### 2.6 The V1 decision chain as documented (what actually decides)
1. HAL wake/attention gate (section 1.2 openers). Ungated speech is either dropped (wakeword on) or sent as `[ambient]` (wakeword off), where SOUL asks the main agent to judge addressee (`SOUL.md:48,59`).
2. Realtime model (Gemini Live) receives every gated voice turn first, streamed in parallel with STT; it **handles**, calls `delegate_to_main`, or calls `reject_turn` (`docs/realtime-voice.md:64-85`). Silence/timeout/transport failure fall back to the main agent (`:82-85`). Every turn logs one route (`realtime_handled`, `delegated`, `ai_rejected`, `realtime_no_output`, `realtime_error`, `realtime_unavailable`, `noise_dropped`, `realtime_not_started`, `realtime_cancelled`) (`:399-415`).
3. Delegation classes are prompt rules in tool descriptions/provider prompts, "not enforced in code" (`docs/realtime-voice.md:86-148`, esp. `:110-111`). Google Search grounding answers single public facts in-session (`:1561-1589`); `look` answers pure visual questions (`:1590-1614`).
4. "The model receives every voice turn before os-server does"; local chitchat is off while realtime is enabled; command rules stay on for what reaches os-server (`docs/os-server.md:1119`). Delegated/fallback voice turns and text chat try local rules (~50 ms) then Jev (BFF Decisions `typesafe/jev-1.13`, timeout 3000 ms) then the main runtime (`docs/os-server.md:1066-1068,1117,1121-1135`).
5. Main runtime (Hermes, model alias Auto-AI) runs skills and emits `[HW:...]` markers; os-server dispatches them to HAL and speaks text via ElevenLabs (`README.md:135,143`).
6. If realtime spoke, the main agent receives a silent `[HANDLED]` sync and should return NO_REPLY; with `OS_REALTIME_SUPERSEDES_MAIN_REPLY=1` an older in-flight main reply loses the speaker (agent-2 report: `docs/realtime-voice.md:773-812`; `.env:101-112`).
7. After a main-agent question, the user's next answer within 300 s is routed to main (`hal/config.py:1102-1106`).
8. Proactive behavior: HAL sensing events -> os-server SensingHandler (one ambient turn per 120 s floor) -> main agent skills (`sensing-behavior.md:49-52`). Rule-based presence dim/off in HAL (`sensing-behavior.md:168-172`). Ambient micro-movements in os-server (`system/ambient/service.go:287-306`).

---

## 3. Measured and known behavior

Overall state of measurement: `docs/benchmarks.md:3` - "**Nothing here is measured yet.**" `make latency` (`scripts/bench/latency.py`) exists but "nobody has run it on a robot" (`docs/not-built-yet.md:19`). Not covered by any tool: time to first spoken word at the speaker, power, RSS/CPU, uptime (`docs/benchmarks.md:36-46`). No production KPI values exist; no false-wake rate, false-reply rate or word error rate is documented anywhere searched.

### 3.1 Latency

| Metric | Number | Date / device / conditions | Source |
|---|---|---|---|
| Realtime time to first sentence | none < 3.0 s, **median 4.0 s**, n=31 | 26/08/2026, lamp-0c89, `gemini-3.1-flash-live-preview` via campaign-api | `docs/realtime-voice.md:3342-3346` (reason the lamp filler delay is 0.5 s). Not re-measured on the shipped `gemini-3.8-live-extended-thinking` (none found). |
| Turn path commit -> first spoken sentence | 3-6 s (vs 97 ms last-audio -> first-audio in Live mode) | 2026-09-07, intern-v2-6286, Gemini 3.1 Flash Live | `docs/realtime-voice.md:2737-2739` |
| Realtime reply clears thinking cue | 0.4-8.6 s | undated | `docs/realtime-voice.md:3389-3391` |
| Delegated turn (event forwarded -> assistant turn done) | **6-22 s** | undated | `docs/realtime-voice.md:3390-3391` |
| Voice agent turnaround to Agent DONE | 5.2-7.5 s, n=3 | 2026-08-06 "Lab Lamp", dirty build OS v0.1.3, HAL 0.0.868, provider/runtime not stated | `robots/lamp/docs/lamp-proof-benchmarks.html:144,151` |
| TTS chunk start -> first audio frame | p50 832 ms, p95 929 ms, n=6 | same lab run | `lamp-proof-benchmarks.html:150` |
| LED dispatch / camera snapshot | p50 7.4 / 484.7 ms, p95 17.5 / 1052.4 ms, n=30 | same lab run | `lamp-proof-benchmarks.html:146-147` |
| Post-capture session handshake | 2-3 s; first ack 4-5 s after user stopped when filler armed at commit | 2026-09-18, lamp-dbda | `docs/realtime-voice.md:3328-3331` (agent report) |
| Post-idle connect | ~2 s per post-idle turn -> prewarm with 4 s join cap | 22/09/2026, lamp-4ace | `docs/realtime-voice.md:1822-1831` (agent report) |
| STT endpoint tail (streaming Nova-3) | median 524 ms, max 611 ms, 0/11 empty (Nova-2: 1/11 empty, max 11.7 s) | 2026-10-09, 11 Vietnamese captures, device not named | `docs/realtime-voice.md:232-241` (agent report) |
| Camera "look" through the agent | 3-5 s good day, 10-20 s slow; up to 80 s silent | lamp, undated code comment | `system/vision/describe.go:36-40` (agent report) |
| Reactive sensing turns (OpenClaw era) | baseline 30-85 s; after fixes ~10-14 s for `motion.activity` | 2026-05-07, Pi ".85"/".38" | `robots/lamp/docs/debug/turn-speed-phases.md:3,22-25,71,94` |
| Hermes "hello" with/without prompt caching | 18.3 s -> 12.6 s (no cache) vs 13.8 s -> 9.2 s steady (cache ~85 %); 3.6 s warm vs 11.8 s after an 8 min gap | 2026-09-16, lamp-0c4e, campaign-api | `docs/agentic/hermes.md:671-677` |
| Hermes conversation blob growth | turn ~7 s became ~50 s with a 28 MB / ~6M-token chain (before rotation) | undated | `docs/agentic/hermes.md:116-120` |
| Stale reply after stop | stale replies started 12-28 s after a stop (Harness result path and wait filler) | 2026-09-17 | `docs/voice-metrics.md:316-320` |

Targets for comparison: product NFR voice-to-action < 1 s (`product-vision.md:837`); KPI-1 >= 95 % acknowledged within 3 s of end of speech (provisional) (`docs/voice-metrics.md:10-17`).

### 3.2 Quality / KPI rates
- KPI definitions and targets: KPI-1 ack <= 3 s >= 95 %, KPI-2 outdated reply played < 5 %, KPI-3 task turns finish >= 85 % (KPI-1/2 provisional) (`docs/voice-metrics.md:10-17`).
- lamp-0c4e cohort 2026-09-24 14:15-15:00: 87 interactions, 44 missing endpoints, 13 interrupted, 30 noise/rejected/no-transcript, **0 eligible -> KPI-1 N/A** (`docs/voice-metrics.md:110-113`). In that log, 3 `provider_delegate` vs 77 `provider_transcript` creations (`voice-metrics.md:110-127`, agent report).
- KPI-3 only synthetic: 2/2 on lamp-0c89, 2026-09-11, "instrumentation check, not production evidence" (`docs/voice-metrics.md:853-867,888-898`).
- Self-interruption detector (removed): 27/08/2026, 20 replies into a silent room -> lamp cut itself off 4 times; best AUC 0.72; any threshold with 0 % self-interruption missed 90-100 % of real interruptions (`docs/realtime-voice.md:1386-1396`). No voice barge-in today; interruption by tap or provider VAD (agent report: `:1353-1419`).
- Echo/self-answer: intern-v2-6286 2026-09-07 Live, 4 unprompted replies in 35 s with nobody present (`docs/realtime-voice.md:3076-3079`); lamp-ee17 2026-09-18 Pipecat, answered its own reply for 4 rounds (`:2478-2482`).
- Backchannel words became turns: "Ok"->"Okay.", "Oh"->"no", 19/08/2026 (`docs/realtime-voice.md:1057-1058`).
- Noise guard trial lamp-52e6 2026-10-08: old guard accepted 7/12 noise and 15/16 speech; new settings 0/12 noise, 16/16 speech (small same-corpus test) (`robots/lamp/docs/sensing-tuning.md:477-481`).
- Gemini Live idle death: 2026-08-21 `gemini-3.1-flash-live-preview` closed after 86-185 s of nothing sent; 107/113 closes on zero-turn sessions; user experience "I had to say it twice" (#238) (`docs/testing-gemini-idle-recycle.md:4-15`). `realtime-voice.md:1810` says 86-198 s (doc disagreement).
- Face ID: 990 frames 2026-09-04, 97.8 % recognised at 0.45 (`sensing-tuning.md:256-266`); upload-path recall 92.0 % @0.30, 86.4 % @0.40 (`:248-250`). No voice speaker-ID accuracy is documented (`robots/lamp/docs/speaker-enrollment.md`, thresholds only).
- Delegation correctness: anecdotes only, no aggregate rate. Examples: "find my pen" question forms answered by Gemini instead of delegated (2026-09-15 lamp-ac82, `realtime-voice.md:102-108`); "send to Telegram" not delegated (2026-09-21 lamp-0c89, `:136-139`); realtime claimed "All done! I've got you remembered" before enrollment (#564, 2026-10-02, `:90-91`); face request answered by realtime (2026-10-05 green-lamp, `:93-95`); plain `gemini-3.8-live` "delegates or stays silent every turn" (`system/server/config/realtime.go:71-72`).
- Addressed speech: Lamp does not set `answer_overheard_speech`; rejecting overheard speech is prompt policy (`docs/realtime-voice.md:150-165`).
- Gaze: lamp-0c89 04/09/2026 gaze confirmed the user 3.6 s late so the turn ran unaddressed (`realtime-voice.md:767-772`, agent report); 2026-09-30 green-lamp a side-on co-worker passed as facing (`vision-tracking.md:670-674`); #545 user missed through 18 looks; #567 lamp-4ace 18-look sweep (`hal/drivers/tracking/gaze.py:1337-1338`).

### 3.3 Reliability
- Busy-flag wedge (OpenClaw era): sensing silent up to 5 min until `busyTTL`; seen 2026-05-06 `stuck_for_s=348` (`debug/busy-stuck.md:3,14,69`).
- Stuck asleep ~19 min until HAL restart, 2026-04-22; later fixed so button tap and friend `presence.enter` wake; a stranger still does not (`debug/sleep-stuck.md:38-76`).
- OpenClaw self-replay: one sensing event -> two agent runs; doubles token cost for passive events; upstream openclaw#50956 (`debug/openclaw-selfreplay.md:7-41`).
- runId misattribution: wrong text spoken; FIFO fix in code; Telegram-interleave variant open (`debug/runid-race.md:17-28,229,250-253`).
- Zombie sessions: reconnect after 3 consecutive silent turns (~24 s) (`hal/config.py:632-635`; `realtime-voice.md:2939-2940`).
- intern-v2-6286 2026-09-07: noise floor held a Live session open, device deaf until restart (`realtime-voice.md:3121-3126`).
- Summarizer: proxy spent the whole 4096-token budget on reasoning and returned no text, 2026-09-28 (`hal/realtime/orchestrator.py:372-374`); long sessions "forget the rules" around turn 10 (#449, `hal/config.py:1086-1093`).
- Memory poisoning: one self-written `USER.md` line hijacked routing on lamp-dbda for most of a day (#421, `docs/os-server.md:1416-1421`).
- HAL suite not fully green: `test_gemini_generation_complete.py:52` fails on baseline (`docs/voice-metrics.md:877-880`).
- No uptime / crash-rate statistics exist (`docs/benchmarks.md:45-46`).

### 3.4 Known failure modes still present in code (from debug audits, spot-checked)
- Guard alert compares sensing type to `"motion"` while HAL emits `motion.activity`, so guard never fires on motion (`debug/proactive-skills-audit.md:16-27`; still `system/server/sensing/delivery/http/handler.go:583`).
- Guard tag lost when queued events drain (`proactive-skills-audit.md:29-33`).
- A `}` inside an `[HW:...]` marker body truncates it; the same suggestion repeats ~2 min later (`proactive-skills-audit.md:77-87`; regex `system/server/agent/delivery/http/handler_hw.go:55`).
- No skill checks busy/sleep/presence; wellbeing can nudge an empty room (`proactive-skills-audit.md:117-121`).
- Agent compliance failures noted in the April sensing pipeline: skipped logging, wrong payloads, invented user name, spoke its own plan aloud; "the agent layer is the fragile part" (`debug/sensing-pipeline.md:201-228,267-276`).
- Music-suggestion cooldown ships at the 7 min test value (`skills/music-suggestion/SKILL.md:65`).

### 3.5 Not built (from `docs/not-built-yet.md`)
- Self-serve gateway key (`:6`); measured hardware numbers (`:19`); hosted-model table for voice/face/mood (`:20`); plain browser setup page (`:22`); OTA signature verification skipped until `signing_public_key` provisioned, rollback only os-server/bootstrap (`:23`). Product-vision P2 items not done: circadian, auto-tracking, gesture, video call, screen awareness (`product-vision.md:808`).

---

## 4. Cost and limits (what the device can see of the Autonomous backend)

### 4.1 One device key, one gateway
- A single `llm_api_key` (docs call it the device's "lobster" key, `docs/realtime-voice.md:2094`, `docs/os-server.md:428`) at `llm_base_url` (campaign-api / bff-campaign-service) covers: realtime Gemini, main agent, STT, TTS, perception (DL backend), Jev, vision-describe, summarizer, `/ping`, `/alert` and GELF logs. It is a required setup field (`docs/setup-flow.md:57,74`); "the key comes with the app account" (`docs/hosted.md:14-17`). Who issues it and on what terms is outside the repo (unverified). A "self-serve gateway key" is not built (`docs/not-built-yet.md:6`).
- Offline: local intents (~50 ms), recorded moves and the safety gate keep working; "conversation, voice, face and mood stop until the network is back" (`docs/hosted.md:60-65`).

### 4.2 Paths, models, defaults (Standard lamp)

| Path | Endpoint | Model | Default | Evidence |
|---|---|---|---|---|
| Realtime voice | `{base}/ws/gemini` | `gemini-3.8-live-extended-thinking`, thinking LOW | on | `hal/config.py:732-748`; `realtime.go:73-75` |
| Google Search grounding, `look` tool | inside Gemini session | - | on | `hal/config.py:758-776` |
| Main agent (Hermes) | `custom:autonomous`, Anthropic-messages mode, `https://campaign-api.autonomous.ai/api/v1/ai` | alias `Auto-AI` (proxy-chosen; "smart-agent-router", `docs/realtime-voice.md:1714-1715`) | on | `runtimes/hermes/presync.sh:39-46,79-114` |
| Main-agent vision / OS describe | same proxy, `X-Auto-Source: vision` | `qwen/qwen3.6-plus` (or catalog default image model) | on demand | `presync.sh:51-60`; `system/vision/describe.go:23,48,231-279` |
| STT | `{base}/v1/ws/audio/transcriptions` | Deepgram `nova-3-general` | on | `hal/drivers/voice/stt/autonomous.py:16,98-100,292-308` |
| TTS | `{base}/elevenlabs/text-to-speech/{voice}/stream` | `eleven_v4_turbo` | on | `hal/drivers/voice/tts/elevenlabs.py:29-32,135-141,178-191` |
| Realtime memory summarizer, spoken-outcome check | `{base}/v1/messages` | `claude-haiku-4-5-20251001` | on | `hal/config.py:1126-1139`; `hal/realtime/response_outcome.py:52-60` |
| Speaker embedding, speech emotion (SER) | `{base}/hal/api/dl/...` (RSA/AES encrypted) | perception service | **on** | `hal/config.py:229-237,375-382,498-520` |
| Face emotion, action/motion, pose, fire hazard (OWLv2) | `{base}/hal/api/dl/...`, `{base}/detect/owlv2` | perception service | **off (opt-in)** | `hal/config.py:240-251,279,307-318,329` |
| Jev intent / Jev Harness | `{base}/jev/decisions` | `typesafe/jev-1.13`, 3 s / 1.5 s budget | on | `system/intent/jev/client.go:20,138-142`; `jev_intent.go:24-39`; `config/jev_harness.go:7-22` |
| Music | YouTube via yt-dlp + ffmpeg | - | on demand | `hal/drivers/voice/music_service.py:185,619-623` |
| Status ping | `{backend or base}/ping` every **15 s** | - | on | `system/beclient/client.go:24-25,154-200` |
| Ops alerts | `{backend or base}/alert` -> bff-campaign-service -> Telegram | - | on | `system/lib/alert/alert.go:41-75`; `docs/os-server.md:424-437` |
| GELF logs | `{autonomous base}/logs/gelf`, all os-server and HAL INFO+ logs, prefers the AutonomousDefaults key even after BYO | - | on | `system/server/config/gelf_relay.go:10-31`; `hal/drivers/gelf_handler.py:43,65-83` |
| Analytics (voice KPIs off-device) | `AUTONOMOUS_ANALYTICS_URL` | - | **off** | `robots/lamp/rootfs/opt/hal/.env:114-127` |
| OTA metadata | baked `OTA_METADATA_URL`, every 5 min | - | on | `docs/bootstrap-ota.md:93,327` |

The `/ping` body reports status, MAC id, versions, runtime, TTS provider/voice, STT language, `wakeword_enabled`, `voice_input_mode` and installed skills (`system/beclient/client.go:171-200`), so the backend can see each lamp's wake-gate state (whether anyone reads it: unverified).

### 4.3 Who pays, metering, and what the device can see
- Metering exists in the BFF: GPT-Live sessions metered as `ceil(seconds x 17)` tokens per row in `lobster_usage`, with "the device's rate limit" checked at connect and after every usage row (`docs/realtime-voice.md:2093-2106`); HAL names close code `4029` "device over its GPT-Live usage limit" (`hal/realtime/voice_agent/gpt_live.py:79-84`). GPT-Live is not the lamp default.
- Plan/quota wording in device code: TTS notice "I'm out of voice quota - check my plan." (`hal/i18n.py:152-158`); LLM "Plan-limit notice" (`system/lib/i18n/phrases.go:18-19`); fire hazard "plan quota exhausted" (agent report: `fire_hazard.py:28`).
- No plan names, quota sizes, prices per device or reset periods appear anywhere in the repo. The device has no endpoint for its remaining quota; `/ping` returns only status, device_id and MQTT (`system/beclient/client.go:212-221`).
- Device-side accounting: Monitor Analytics computes `TokensBilled = in + cacheWrite + cacheRead + out`, "Cached reads are billed at full price (no 0.1x discount) to match backend billing" (`system/server/agent/delivery/http/handler_api_analytics.go:174-175`). Realtime usage with estimated USD goes to local logs only (`docs/realtime-voice.md:2503-2516`; estimate "is not evidence of the provider's actual bill" `:2542-2545`).
- Provider list prices recorded in docs (per 1M tokens text-in/audio-in/text-out/audio-out): `gemini-3.8-live`/`-extended-thinking` $0.75/$3.00/$4.50/$12.00, "promo through 2026-12-31, doubles after" (`docs/realtime-voice.md:2586`); `gpt-realtime-2` $4/$32/$24/$64 (`:2588`); GPT-Live $0.05/min (`:2241-2243`). "`in_text` dominates (the ~7-10k token system prompt plus accumulated session context is re-billed every turn)"; Google Search billed per grounded request (`:2592-2596`). No Hermes main-agent, STT, TTS or perception unit costs are in the repo.
- BYO: Settings can replace key/URL/model for llm, tts, stt, realtime; the first change snapshots `AutonomousDefaults`, restorable per section (`config.go:141-143,714-720`; `system/device/config_update.go:244-306,793-818`). `llm_config_mode=runtime` keeps OS credentials for voice and backend; subscription logins exist for some runtimes (`system/runtimeauth/flow.go:24-35`). "No account? ... voice, face and mood still need the gateway" (`docs/hosted.md:15-17`).

### 4.4 What happens at the limit (user experience)
| Limit | Detection | What the user gets | Evidence |
|---|---|---|---|
| Main-agent LLM plan limit | reply text matches "reached the usage limit" / "upgrade for higher limits" / "your access will reset" / "upgrade your plan" | cached phrase "[sigh] I've hit my usage limit." (VI/JA/ZH variants), repeats suppressed for 30 s | `system/server/agent/delivery/http/handler_tts.go:20-36,219-236`; `phrases.go:148-154` |
| TTS quota/rate limit | ElevenLabs 429 or 401/402 with "quota"; OpenAI/Gemini 429 | reply goes silent, strip flashes amber 3x, then cached "I'm out of voice quota - check my plan." at most once per 300 s; silent if the WAV is not cached | `hal/drivers/voice/tts/service.py:31-35,1637-1640,1925-1960`; `elevenlabs.py:220-226`; `hal/i18n.py:152-158` |
| Realtime (Gemini) quota / resource exhausted / go_away | `_fail_fast_turn` | turn falls back to the main agent (Hermes path, slower); no realtime filler armed; no spoken notice | `docs/realtime-voice.md:1843-1856,1020-1023` |
| STT proxy quota/error | logged warning | no notice (silence) | agent report: `hal/drivers/voice/stt/autonomous.py:181-209` |
| Perception 429 | pause 300 s | log only | agent report: `emotion.py:300-313`, `fire_hazard.py:143-146` |
| Jev non-200 | defer | turn goes to main agent | agent report: `system/intent/jev/client.go:161` |

Where the "usage limit" banner text originates (proxy vs model) is unverified; there is no Go handling of HTTP 429/402 from the LLM (agent report).

### 4.5 Cost controls in code and their stated reasons
- Realtime: recycle after 12 turns "to cap re-billed context", after 240 s idle; Gemini idle park 45 s; pre-turn recycle 60 s; compression 32768 -> 24576 tokens (`hal/config.py:636-655,749-752`). Per-turn floor caps: device memory 8000 chars, memory 8000, summary 5000, identity 12000 (`hal/config.py:1080-1112`).
- No reply without a transcript (cost guard) (`.env:53-54`); STT not kept connected while idle (`.env:8`); max capture 20 s "caps ... the STT bill" (`.env:47-51`).
- Noise turns skip commit so their audio is never billed (`docs/realtime-voice.md:3183-3190`).
- Hermes: prompt caching with 1 h TTL, context_length pinned, conversation rotation at 40 turns or 250k tokens (`presync.sh:84-95`; `runtimes/hermes/rotation.go:9-13`).
- Sensing: one ambient agent turn per 120 s, checked before the vision-describe call (`config.go:680-687`; agent report: `system/server/sensing/delivery/http/handler.go:381-397`).
- Fixed phrases replay from the WAV cache (`system/ambient/service.go:325-326`; `handler_tts.go:230-235`).


---

## 5. The marketing promise (copy found in the repo)

The repo has no copy of the autonomous.ai/lamp landing page. Buyer-facing text that is in the repo:

**`robots/lamp/README.md`** (links the product page `:6`; "Shipping - $499" `:88`)
- "always-on AI desk robot ... *present*: it sees your workspace, tracks faces and motion, remembers your work, and speaks up when something is relevant. The articulated arm physically turns to look at you" (`:14-16`).
- "No wake word by default - it answers when you speak, turns to look at you, and the ring lights up" (`:59-60`) - contradicts the wakeword seed for fresh configs (`ROBOT.md:19`, added 2026-08-18 in `2cd9d5a64`).
- Privacy: "On-device face recognition ... Data encrypted in transit, never stored or used for training. Bring your own AI provider" (`:42-43`).
- "Capabilities: ... audio, vision, motion, light, display, sensing, presence" (`:37`) - lamp declares no display (`ROBOT.md:25-29`).
- Skills "Live on the next conversation, no reboot" (`:76-79`); swap brain via a debug-only Runtime tab (`:81-84`).

**Top-level `README.md`**
- "Say something, it turns to look at you, the ring lights up, and it answers" (`:65`); "Install a skill from the Skill Store - one tap, live on the next conversation" (`:66`); "Type what you want it to do in the app and it writes the skill" (`:67`).
- "Try saying" local-command examples with expected behavior (`:33-45`).
- Lamp + Reachy Mini "will hold a conversation until you stop them" (`:85`).

**`docs/autonomous-os-product-page.html`** (prototype product page, last touched 2026-09-27)
- "It notices. Then it cares. Lamp looks toward the room, listens for what matters, and responds with light, movement, and voice" (`:261-262`).
- "A companion that learns your rhythm ... recognize your return, turn toward you, remember the threads you share" ("Hey, been a while") (`:272-276`).
- "It feels ... look up, wiggle, pause to think, and rest - while its light carries the emotion" (`:284-286`); emotion demos Happy/Excited/Thinking (green pulse)/Curious/Sleepy (`:415-437`) - lamp preset for `thinking` is purple `[2,0,3]` (`robots/lamp/presets.json:19`).
- "Essential responses stay close to the device. When a moment calls for more, the agent can bring skills, memory, and broader intelligence" (`:309-311`); "A quick request can stay immediate" (`:321`).
- "Open. Local-first. Bounded." (`:356-358`).

**`robots/lamp/docs/product-vision.md`** (internal, but contains the original promise list): "remembers context across days and weeks", "proactively initiates ('You seem quiet today, everything okay?')" (`:476-477`); stress -> auto-dim + lean closer (`:27`); voice-to-action < 1 s (`:375,837`).

**Marketing copy briefs** (Vietnamese, for editing what the lamp says, not buyer claims): `robots/lamp/docs/mood-marketing-copy.md` (mood -> one-line music suggestion; claims a 30 min re-suggest cooldown `:57`, shipped skill uses 7 min) and `robots/lamp/docs/wellbeing-marketing-copy.md` (hydration 45 min / break 30 min nudges triggered "each time the camera detects activity" `:17-26`, which needs the off-by-default cloud motion opt-in). Both point at stale paths `lamp/resources/openclaw-skills/...` (`mood-marketing-copy.md:8-10`, `wellbeing-marketing-copy.md:6-7`).

**Not in repo (unverified here):** the user's own notes record a staging landing page (staging.autonomousdev.xyz/lamp/learn-more) that, as of 2026-10-08, promised six agent runtimes, Blender gestures, Home Assistant, co-host chat/votes, persona presets, Pro air-quality alerts, local models. This study did not fetch or check it.

---

## 6. Contradictions between intent, configuration and code

| # | Intent / doc says | Configuration / code does | Evidence |
|---|---|---|---|
| 1 | Brain is OpenClaw (product vision, hosted.md) | Lamp defaults to Hermes | `product-vision.md:13,93`; `docs/hosted.md:14`; vs `ROBOT.md:9`, `agent-runtime.md:3` |
| 2 | ROBOT.md `gateway.protocol: websocket` | Hermes transport is SSE; os-server only warns | `ROBOT.md:10`; `system/agent/factory.go:21-44` |
| 3 | README: realtime default model `gemini-3.8-live` | os-server seeds `gemini-3.8-live-extended-thinking`; HAL fallback is plain 3.8-live, which os-server says "delegates or stays silent every turn" | `README.md:147`; `realtime.go:71-76`; `hal/config.py:743-745` |
| 4 | Lamp README: "No wake word by default" | Fresh configs seed `wakeword: true`; older configs stay off; so two lamps on identical software can behave differently depending on first-boot version | `robots/lamp/README.md:59`; `ROBOT.md:13-19`; `server.go:297-309`; `config.go:446-451` |
| 5 | Wake follow-up window: code default 10 s; `realtime-voice.md:652` says "the lamp image ... sets 20" | Lamp image sets 5 s | `wake_phrases.go:18`; `hal/config.py:586`; `.env:81`; `docs/realtime-voice.md:652` |
| 6 | Ambient life includes TTS self-talk | Mumble loop hard-disabled (`mumblePaused = true`) | `ROBOT.md:32-33`; `architecture-decision.md:90`; `system/ambient/service.go:308-314` |
| 7 | Main-agent fillers at 3.5 s ("Hmm...") | Opening and continuation main-agent filler playback "temporarily paused" | `docs/realtime-voice.md:70-81` vs `:1011-1018` |
| 8 | "All sensing runs on-device - no video/audio streamed to cloud" | Speaker-ID and speech-emotion audio upload to the DL backend by default; camera uploads only after opt-in | `product-vision.md:132`; `hal/config.py:210-237,375-377,498-500` |
| 9 | UC-M1/M2/M3 (facial emotion, wellness reminders, mood music) "DONE" and marketing briefs describe camera-triggered nudges | The camera perceptions feeding them are off by default; wellbeing triggers on `motion.activity` which is not produced without opt-in | `product-vision.md:691-766,809`; `sensing-behavior.md:210-235`; `skills/wellbeing/SKILL.md:3` |
| 10 | Music suggestion re-suggest cooldown 30 min | Shipped skill uses 7 min "(production: change to 30 min before ship)" | `mood-marketing-copy.md:57`; `skills/music-suggestion/SKILL.md:65` |
| 11 | Product vision: 64 LEDs, GC9A01 eye display, Raspberry Pi 4 | 32-LED ring, no screen, OrangePi default image target | `product-vision.md:39,170-176`; `presets.json:4`; `ROBOT.md:25-29`; `scripts/imager/Makefile:16` |
| 12 | Product vision: voice via Deepgram STT; overview pipeline Deepgram -> OpenClaw | AutonomousSTT (Deepgram nova-3 relayed by campaign-api); realtime model sees every voice turn first | `product-vision.md:123,857`; `docs/overview.md:291-299`; `hal/runtime.py:579-592`; `docs/os-server.md:1119` |
| 13 | Skills call HAL directly with curl | Skills emit `[HW:...]` markers parsed by os-server | `architecture-decision.md:133-137,333` vs `README.md:135,143`, `product-vision.md:297` |
| 14 | `config.go` comment: Jev "default off" | Jev enabled by default | `config.go:240` vs `jev_intent.go:24-27`, `docs/os-server.md:1123` |
| 15 | bootstrap-ota: pro-respeaker-lite "HAL AEC enabled ... Live off", 35/35 volume | Its `.env` disables HAL AEC and enables Live; profile 35/50 | `docs/bootstrap-ota.md:424,434-436` vs `overrides/pro-respeaker-lite/rootfs/opt/hal/.env:13,21`, `profile.json:2-3` |
| 16 | Proactive event cooldowns: motion/sound 60 s, presence 10 s, light 30 s | Cross-type ambient floor 120 s; light check interval 300 s in code | `docs/overview.md:328`; `config.go:680-687`; `hal/config.py:88` |
| 17 | Architecture: voice-to-action < 1 s; TTS < 500 ms after LLM | Measured: realtime first sentence median 4.0 s (older model); delegated turns 6-22 s; TTS first frame p50 832 ms | `product-vision.md:643,837`; section 3.1 |
| 18 | `docs/hosted.md:19` BYO endpoint "not built yet" (#198) | `docs/not-built-yet.md:6` says it "already works" | agent report; both lines read |
| 19 | Product page "thinking" = green pulse | Lamp `thinking` preset is purple | `docs/autonomous-os-product-page.html:425-427`; `presets.json:19` |
| 20 | hw-call-strategy proposes `/hw/batch` | Not implemented | `robots/lamp/docs/hw-call-strategy.md:26-95`; no match in `hal/`, `system/`, `skills/` |
| 21 | Vision-describe budget 20 s + 15 s | Code uses 45 s + 35 s (30 s for a camera look) | `docs/realtime-voice.md:1726` vs `system/vision/describe.go:34,41-43` (agent report) |
| 22 | NFR "Uptime 24/7", "OTA seamless, rollback on failure" | No uptime tracking; OTA rollback covers only os-server/bootstrap; signature verification skipped until a key is provisioned | `product-vision.md:841,847`; `docs/benchmarks.md:45-46`; `docs/not-built-yet.md:23` |

---

## Method note

Sections 1, 2, 5 and 6 were read directly. Sections 3 and 4 merge three sub-searches (debug/benchmarks/issues; realtime/voice docs; backend cost paths); their load-bearing citations were re-read and confirmed (`lamp-proof-benchmarks.html:140-151`, `voice-metrics.md:10-17,108-113`, `testing-gemini-idle-recycle.md:1-16`, `realtime-voice.md:64-140,158-165,399-416,773-790,1011-1023,1385-1396,1843-1856,2086-2110,2503-2597,3340-3347,3386-3392,2735-2740`, `sensing/delivery/http/handler.go:583`, `beclient/client.go:25,171-221`, `gelf_relay.go:8-32`, `rotation.go:9-14`, `handler_api_analytics.go:172-176`, `hal/i18n.py:150-158`, `gpt_live.py:79-85`). Rows marked "agent report" were not individually re-read.
