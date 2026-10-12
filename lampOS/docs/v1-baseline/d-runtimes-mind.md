# V1 Lamp: agent runtimes and the "mind" they run

Scope: `runtimes/**`, `robots/lamp/SOUL.md`, `robots/lamp/ROBOT.md`, `skills/**`, and the
OS boundary where turns are built and replies consumed. Checkout: `origin/main` at
`170a1b85e` ("deploy"). All paths are relative to the checkout root. "Unverified" marks
anything the code in this checkout cannot settle (device state, CDN contents, server-side
behaviour of the Autonomous proxy).

(Sections 4 and 5 and the skill rows of sections 6-7 were compiled by two sub-studies and
spot-checked against the code; see the note at the top of each.)

---

## 1. Which runtime the lamp runs, how it is installed, configured, started, and paid for

### 1.1 Selection: Hermes, on the device

- Lamp declares `gateway: { default: hermes, protocol: websocket }` (`robots/lamp/ROBOT.md:9-11`).
- Resolution order is `config.agent_runtime` > `/root/config/f_r_default_agent` (image-baked)
  > ROBOT.md `gateway.default` > `openclaw` (`system/agent/factory.go:112-139`,
  `system/device/runtime.go:34-59`). The value is persisted into an empty
  `config.agent_runtime` at first boot (`system/device/runtime.go:62-76`,
  `SeedAgentRuntimeFromGateway`), so later ROBOT.md edits do not switch a seeded device
  (`robots/lamp/docs/agent-runtime.md:12-13`).
- `DEFAULT_AGENT` is unset by default in the image scripts (`scripts/imager/build-orangepi.sh:29`,
  `scripts/imager/build.sh:21`), so an unmodified lamp image resolves to Hermes via ROBOT.md.
  Whether shipped lamp images were built with `DEFAULT_AGENT` set: unverified.
- `protocol: websocket` in ROBOT.md contradicts Hermes' real transport (`sse`); the factory only
  logs a warning (`system/agent/factory.go:19-30,34-44`); transport selection ignores the field.
- Doc drift: `runtimes/README.md:11` still calls OpenClaw "(default brain)" and lists the order
  without `f_r_default_agent` (`runtimes/README.md:20-22`); the OrangePi image comment says
  "openclaw is the default active runtime" (`scripts/imager/build-orangepi.sh:371`). For the lamp, code says Hermes.

Where it runs: Hermes is a local process on the lamp's board, reached at
`http://127.0.0.1:8642` with bearer `hermes-local-api-key` (`runtimes/hermes/constants.go:4-9`).
A separate `remote` runtime value re-points the same client at a Hermes server on the LAN
(`system/agent/factory.go:57-68`, `runtimes/hermes/remote.go:8-20`); its doc targets Intern 2 as
a voice frontend for a Mac (`docs/agentic/remote-hermes.md`), not the lamp. Default lamp = on
device.

### 1.2 Install

- `runtimes/hermes/install.sh` (run by `switch-runtime` on first switch): installs `jq`/`yq`,
  runs the upstream Nous Research installer from `https://hermes-agent.nousresearch.com/install.sh`
  stage by stage, skipping `node-deps` (browser tools) because npm native builds hang on ARM
  (`install.sh:33-41`); stops OpenClaw (`:51-61`); seeds `~/.hermes/.env` with
  `API_SERVER_ENABLED=true`, `API_SERVER_KEY`, CORS (`:67-77`); runs the presync hook (`:79-87`);
  installs the gateway as systemd unit `hermes-gateway` running as root (`:92`) with an
  `ExecStartPre=-/usr/local/bin/os-server --wait-hal-ready` drop-in (`:94-106`); starts it
  (`:107`).
- Version: the OrangePi image pre-bakes Hermes pinned to `HERMES_VERSION="0.21.5"` and fails the
  build on mismatch (`scripts/imager/build-orangepi.sh:22,340-361`). The Pi 5 image and the
  runtime `install.sh` use the unpinned upstream installer (`scripts/imager/build-pi5.sh:1229-1238`,
  `runtimes/hermes/install.sh:36`). So the Hermes version on a given lamp depends on board and
  install path (unverified per device).

### 1.3 Configure (every boot, every setup, every switch)

`EnsureOnboarding` (`runtimes/hermes/onboarding.go:73-155`) runs on boot and from
`SetupAgent` (`:68-71`):

1. Runs the embedded presync script (`onboarding.go:77-83,157-184`), which writes
   `~/.hermes/config.yaml` and `.env` (`runtimes/hermes/presync.sh`):
   - Model: `.model.provider = "custom:autonomous"`, `.model.default = "Auto-AI"`;
     provider `autonomous` with `key_env: AUTONOMOUS_API_KEY`, `api_mode: anthropic_messages`,
     `base_url` default `https://campaign-api.autonomous.ai/api/v1/ai` (`presync.sh:40-47`).
   - Vision: `.auxiliary.vision` = provider `custom:autonomous`, model `qwen/qwen3.6-plus`
     (`presync.sh:49-62`); `.agent.image_input_mode = "auto"` (`:61`).
   - On the campaign-api: `prompt_caching = true` for `Auto-AI`, `cache_ttl = "1h"`,
     `context_length = 256000` (`presync.sh:84-96`). With a custom `llm_base_url` the alias is
     replaced by `llm_model` (`:97-104`).
   - `terminal.cwd = /root/.hermes` so Hermes discovers `AGENTS.md` (`presync.sh:66-70`).
   - `approvals.mode = "off"`: no command-approval prompts at all (`presync.sh:72-76`).
   - `.env`: `AUTONOMOUS_API_KEY` <- `config.json llm_api_key` (`presync.sh:130-132`), plus
     Telegram/Slack/Discord/WhatsApp/BlueBubbles tokens and allow-lists (`:133-158`).
   - `llm_config_mode=runtime` skips all model/key writes (`presync.sh:11,18,33,80,130`).
2. Injects the persona block at the top of `~/.hermes/SOUL.md` (`onboarding.go:86-88,286-312`)
   and the OS rule block at the top of `~/.hermes/AGENTS.md` (`onboarding.go:89-91,246-262`);
   removes a legacy rule block from SOUL.md (`:92-94,264-284`).
3. Prunes `-imported` skill duplicates (`onboarding.go:96,206-244`), syncs the Jev plugin
   (`:102-104`), the `os-server-observer` hook (`:107-110`), cache-usage patch (`:112-115`),
   nine BlueBubbles/iMessage patches (`:118-121`, `runtimes/hermes/bluebubbles_patches.go:13-37`),
   the native-Runs patch (`:123-126`, `runtimes/hermes/runs_patch.go:18-42`), and downloads every
   capability-supported skill from the CDN (`:129-130`, `runtimes/hermes/skill_watcher.go:96-150`).
4. Restarts `hermes-gateway` only when something changed or it is down (`onboarding.go:137-151`),
   then sends the agent a skill-update system turn if skills changed (`:153`).

### 1.4 Model, provider, and who pays

- Main brain: model alias `Auto-AI` at the Autonomous campaign-api proxy, Anthropic Messages
  wire format (`runtimes/hermes/presync.sh:40-47`). What `Auto-AI` resolves to is decided
  server-side by the proxy: unverified from this repo (`docs/agentic/hermes.md:654-656`
  says "that proxy resolves to whatever model it picks").
- The model id os-server sends Hermes is the constant `hermes-agent` (`constants.go:8`,
  `chat.go:86-90`); it is Hermes' API name, not the LLM.
- Vision side model: `qwen/qwen3.6-plus` via the same proxy (`presync.sh:53-60`).
- Key: `AUTONOMOUS_API_KEY` = `config.json` `llm_api_key` (`presync.sh:131`). `llm_api_key` and
  `llm_base_url` are required setup fields (`system/server/config/config.go:131`,
  `docs/setup-flow.md:74`) and arrive in the setup URL from the provisioning page
  (`system/web/src/hooks/setup/useSetupUrlParams.ts:83-85`). Default = Autonomous pays through
  its proxy key. A user can bring their own provider by setting `llm_base_url`/`llm_model`
  (`presync.sh:97-115`) or `llm_config_mode=runtime` (native Hermes config). Who issues the
  campaign-api key and how it is billed: outside this repo (unverified).
- The same key also funds: the realtime voice model (key/base fall back to `llm_api_key` /
  `llm_base_url`, `system/server/config/realtime.go:15-16,145-150`), the Jev skill selector
  (`runtimes/hermes/plugins/jev/router.py:49-67`), the OS intent classifier
  (`system/server/config/jev_intent.go:8-31`), TTS/STT fallbacks
  (`system/server/config/config.go:594-609`).
- Realtime voice layer default: enabled, provider `gemini`, model
  `gemini-3.8-live-extended-thinking`, voice `Kore`, thinking `LOW`
  (`system/server/config/realtime.go:73-75,94-125`); unpinned blocks are rewritten to these
  defaults every start (`system/server/config/config.go:435-440`). It runs in HAL in parallel
  with the main agent and delegates to it (`docs/realtime-voice.md` intro). It is a second
  "mind" with its own prompt in HAL Python (not studied here).

---

## 2. How a turn reaches Hermes and returns

### 2.1 The message the model sees

The OS builds every sensing/voice/chat turn with `sensingmsg.Build`
(`system/lib/sensingmsg/sensingmsg.go:56-139`):

| Event type | Text prefix | Extra lines appended |
|---|---|---|
| `voice_command`, `voice_followup` (wake word / follow-up) | `[user] <text>` (`:65-66`) | enroll nudge if unknown speaker with saved audio, max once per 5 min (`system/domain/voice.go:9-33`) |
| `voice` (no wake word) | `[user] [ambient] <text>` (`:67-69`) | same |
| `web_chat`, `mqtt_chat` | `[user] <text>`; slash commands raw (`:70-76`) | harness reply route + connected/disconnected context (`system/lib/sensingmsg/harness.go:27-57`) |
| `environment.update` | `[environment:update] ...` + "Advisory sensor context ... NO_REPLY when no useful, timely action is warranted." (`:85-86,102-103`) | |
| `motion.activity` | `[activity] ...` (`:87-88`) | `[context: current_user=X]`, user context, wellbeing context (`:119-129`) |
| `emotion.detected` / `speech_emotion.detected` | `[emotion]` / `[speech_emotion]` (`:89-92`) | current_user, user context, emotion context (`:130-133`) |
| `presence.enter` | `[sensing:presence.enter] ...` (`:93-94`) | current_user; presence context for a newly seen friend, or an inline "A stranger joined X ... speak to X" rule (`:104-113`) |
| `presence.leave`, `presence.away` | `[sensing:...]` | "[No crons to cancel. NO_REPLY unless worth saying.]" (`:114-115`) |
| `touch.head_pat` | `[sensing:touch.head_pat]` | "[NO_REPLY unless worth saying — phrase already spoken locally.]" (`:116-118`) |
| any other sensing | `[sensing:<type>] ...` (`:93-94`) | |
| all non-chat | | `[context: current_language=xx]` (`:136-137`, `system/lib/i18n/lang.go:41-46`) |
| all | | trailing `[via:<source>]` line: web, mobile, voice, voice_handoff, voice_ambient, voice_history, sensing (`:17-42,59-61`; `system/domain/via.go`) |
| guard mode | `guardTag + message` replaces the prefix (`:78-80`) | |

Other producers:
- Realtime-handled voice: HAL sends `[skills: input-branching]\n[HANDLED] <input>\n[REPLY] <reply>`
  as a silent history copy (`hal/drivers/voice/_internal/turn_dispatch.py:232`,
  `hal/drivers/voice/_internal/live_history.py:104`); the event type is `voice_agent_handled`,
  marked silent (`runtimes/hermes/events.go:259-261`).
- Realtime delegation: `[voice-instruction] <delegate_msg>` (`hal/drivers/voice/_internal/turn_dispatch.py:251`).
- External-agent history: "History only ... Return NO_REPLY.\n[HANDLED] ..." (`system/externalhistory/sync.go:29-30`).
- Schedules: the schedule's free-text instructions + `[via:schedule]`, sent as a system chat
  every minute tick when due (`system/schedule/runner.go:247-263`).
- Skill updates: "[system] The following skills have been updated. Re-read them now ... output
  exactly NO_REPLY" (`system/skills/skill_update_prompt.go:10-16`).
- Image questions: when the main model's catalog entry lacks image input, the OS describes the
  image first and replaces the snapshot with `[vision-image] ... [image description] ...`
  (`system/server/sensing/delivery/http/handler.go:449-513`, `system/vision/describe.go:95-116`).
  Whether `Auto-AI` advertises image input in the proxy catalog: unverified.
- Before an agent turn, the OS may answer locally: `system/intent` "matches common voice
  commands locally and executes them against HAL directly, bypassing the agent"
  (`system/intent/intent.go:1-2`), enabled by default (`system/server/config/config.go:629-634`).
  Covered by another study; noted because it decides what never reaches the mind.

### 2.2 Transport (Hermes)

Two transports, chosen at runtime:

1. Native Runs (preferred when available). Each `/health` poll, while no stream is in flight,
   probes `GET /v1/capabilities` (`runtimes/hermes/health.go:76-84`) and requires
   `run_submission`, `run_events_sse`, `run_status`, `run_stop`, `run_steer` plus
   `runs_idempotency.autonomous_run_events_v1=true` (`runtimes/hermes/runs_transport.go:57-82`).
   The last flag only exists after the OS patch `runs_patch.py` edits Hermes'
   `api_server_runs.py` (`runtimes/hermes/runs_patch.go:18-42`, `runs_patch.py:19-22`); the patch
   accepts the source shapes of Hermes 0.21.1 and 0.21.5 (`docs/agentic/hermes.md:240-243`).
   Once true it stays true for the process (`health.go:76-84`). Turns go
   `POST /v1/runs` with `input`, `model`, `session_id` (`runs_transport.go:120-147`); events via
   `GET /v1/runs/{id}/events` (`:328`). Which transport a deployed lamp actually uses: depends on
   its Hermes version and patch success (unverified per device).
2. Responses fallback. `POST /v1/responses` with `{model:"hermes-agent", conversation, stream:true,
   input}`; images as a multi-part `input_text`/`input_image` array (`runtimes/hermes/chat.go:86-108`,
   `client.go:20-40,54-88`). SSE parsed line by line (`client.go:90-158`) and translated to the
   OpenClaw-shaped `domain.WSEvent` stream (`runtimes/hermes/translator.go:63-95`).

Turn mechanics (`runtimes/hermes/chat.go:57-158`):
- Refused when Hermes is not ready (`:58-60`).
- Snapshot paths stripped from presence messages (`:70-73`).
- Busy flags set (`:110-112`), then the "thinking" acknowledgement fires (`:114`, see 2.5).
- Native: enqueue to the managed loop (`:151-152`). Responses: `go s.runStream(...)` (`:153-154`);
  `sendChat` returns immediately with the run id (`:157`).
- Streaming: `response.output_text.delta` becomes `assistant` deltas (`translator.go:84`); the OS
  speaks sentence by sentence. NO_REPLY is detected on the final text; if a first sentence was
  already streamed it cannot be unspoken (`system/server/agent/delivery/http/handler_event_agent.go:846-853`).
  The OS also treats narrated non-replies as silence (`isMetaNonReply`, same file `:846`) and
  strips trailing `NO_REPLY` (`system/server/agent/delivery/http/handler_text.go:68-94`).

### 2.3 Conversation / session identity and rotation

- One shared conversation for every turn source: base name `device-main` (`constants.go:7`).
- Boot-fresh: `device-main-<bootUnix>` on first use per os-server process, so an os-server restart
  starts a new history chain (`runtimes/hermes/rotation.go:16-29`).
- The server's session UUID is captured from `X-Hermes-Session-Id` (`client.go:17-18,78-80`).
- Rotation: `ShouldRotateSession` true at 40 turns since last rotation or at 250,000 reported
  tokens (`rotation.go:9-14,46-49`); the generic handler calls it once per turn and then
  `NewSession` (`system/server/agent/delivery/http/handler_session_lifecycle.go:63-93`), which
  switches to `device-main-<boot>-<seq>` and clears the session (`rotation.go:31-56`).
  Rotation drops in-session history without a summary; memory files persist
  (`handler_session_lifecycle.go:63-64`, `docs/agentic/hermes.md:131-146`).
- Self-heal: a native run that ends without a terminal rotates the conversation
  (`runtimes/hermes/steering.go:466-478`) and fails queued requests on the uncertain one
  (`:315-317`).
- `lastResponseID` is stored (`translator.go:112`) and cleared on rotation, but never sent as
  `previous_response_id`; chaining is by `conversation` name only. `docs/agentic/hermes.md:107-108`
  says it is "used to chain turns" (doc drift).

### 2.4 Queued events: merge-drain and native steering

Busy test: `IsBusy` true while any stream is in flight, or `activeTurn` within a 5-minute TTL
(`runtimes/hermes/events.go:30,50-67`).

The OS sensing handler queues passive input when the agent is busy and the input cannot steer,
or when the speaker is still talking (`system/server/sensing/delivery/http/handler.go:516-560`).

Drain (`runtimes/hermes/events.go:106-226`), run when the last stream ends (`SetBusy(false)`,
`:69-81`):
- Voice first (`:132-136`).
- Expiry: environment, activity, emotion, speech_emotion, presence enter/leave/away older than
  60 s are dropped (`:138-160`).
- Coalesce: for those same types only the latest of each type survives (`:162-191`).
- Standalone events keep their own turn: images, `voice`, `voice_command`, `voice_agent_handled`,
  `voice_followup`, `web_chat`, `mqtt_chat`, `environment.update` (`:38-48`); the first one found
  is sent alone and the rest re-queued (`:207-216`).
- Otherwise all remaining ambient events are merged into one turn under the header
  "[ambient signals batched while busy — respond once, using the items below as combined
  context]" (`:32-36,219-225,287-373`). `mergeDrainEnabled = true` (`:33`).
- Only synchronous not-ready rejections are retried; an attempted POST is never replayed
  (`:375-380`).

Native steering (only with native Runs): new eligible user text while a run is active is sent to
`POST /v1/runs/{id}/steer` (`steering.go:394-438`); `stop`/`/interrupt` or a voice "stop" becomes
`/stop` (`:103-133,404-406`). Eligible = not system, not `[HANDLED]`, not prefixed
`[sensing:`, `[activity]`, `[emotion]`, `[speech_emotion]`, `[environment:`, `[ambient signals`
(`:84-101`), text-only, same conversation (`:398-402`). After a steer, assistant text is held until
the terminal and only the newest audible request speaks; earlier ones are silenced
(`:185-234,422-436,453-465`). A too-late steer returned as `pending_steer` is re-queued
(`:487-497`). After a confirmed stop, the next user request is prefixed with
"[system-routing: The user explicitly stopped the preceding task. It is cancelled; do not resume
it unless the user asks to resume it. ...]" (`:252-297`).

### 2.5 Hooks and plugins around a turn

- Emotion acknowledgement (`runtimes/hermes/emotion_ack.go:17-67`): before each send, POST HAL
  `/emotion {thinking, 0.7}` unless the text is empty or starts with `[sensing:`, `[activity]`,
  `[emotion]`, `[speech_emotion]`, or the run is silent. Gated on the `expression` capability
  (`system/skills/hooks.go:7-28`), which the lamp declares (`robots/lamp/ROBOT.md:33`). Mirrors the
  OpenClaw TS hook (`runtimes/openclaw/hooks/emotion-acknowledge/handler.ts:3-33`). It does fire
  on `[user] [ambient] ...` turns and on `[environment:update]` turns, before the model has
  decided whether the speech was addressed to it; the SOUL rule says overheard speech gets "no
  physical or emotional reaction" (`robots/lamp/SOUL.md:59`). This is a code/prompt mismatch.
  Also fires for gateway-owned channel turns via `FireChannelStartEmotion` (`emotion_ack.go:64-67`).
- Jev skill preloader, Hermes plugin `jev` (`runtimes/hermes/plugins/jev/plugin.yaml`,
  `__init__.py:6-8`): enabled (`router.py:19 ENABLED = True`) and auto-added to
  `plugins.enabled` (`runtimes/hermes/jev_plugin.go:111-143`). On `pre_llm_call` it sends the
  current message plus every eligible skill's name/description to
  `{llm_base_url}/jev/decisions` with model `typesafe/jev-1.13` (`router.py:139-148,198-219`),
  accepts a skill only at choice >= 0.70, margin >= 0.20, fit >= 0.60 (`router.py:24-27,182-191`),
  and injects the selected SKILL.md into this turn's context (`preload.py:16-55`). 3 s budget,
  30 s cooldown after errors (`router.py:20,333,347-350`). Skips: `[system]` messages, slash
  commands, `[skills:` tags, and any message containing `[via:voice]` (wake-word voice) because the
  ~1.5 s decision blocked voice turns (`router.py:250-260`). Event-kind turns are memoised by
  header (`router.py:28-30,129-136,288-305`). Note: ambient voice carries `[via:voice_ambient]`
  and realtime delegations `[via:voice_handoff]`, which do not match the substring and therefore
  do go through Jev. All other runtimes ship Jev disabled (`runtimes/openclaw/jev_plugin.go:16`,
  `runtimes/codex/gatewayd/jev.go:14`, `runtimes/claudecode/gatewayd/jev.go:13`,
  `runtimes/opencode/gatewayd/jev.go:14`, `runtimes/picoclaw/jev_hook.go:16`).
- `os-server-observer` Hermes hook on `agent:start`/`agent:end` posts each gateway turn to
  `http://127.0.0.1:5000/api/agent/channel-turn` so channel turns show in Flow Monitor
  (`runtimes/hermes/hooks.go:12-49`, `hooks/os-server-observer/HOOK.yaml`).
  `docs/agentic/adding-agent-runtime.md:396-399,417-423` still says Hermes has no hooks loader and
  that Telegram's receive loop is os-server-side; both are contradicted by this hook and by
  `docs/agentic/hermes.md` §8 (the Hermes gateway owns Telegram/Discord/WhatsApp I/O). Doc drift.
- Dead-air fillers: during tool work on voice turns the OS speaks short filler phrases (1.5 s delay,
  2.5 s cooldown, max 6 per turn) (`docs/agentic/hermes.md:475-505`, `system/lib/i18n/fillers.go`).
  OS-side; not verified in code here.

### 2.6 What each runtime does differently (behaviour that changes the lamp)

| | Hermes (lamp default) | OpenClaw (fallback) | PicoClaw | Codex | Claude Code | OpenCode |
|---|---|---|---|---|---|---|
| Transport | HTTP+SSE per turn, native Runs when patched (`runtimes/hermes/client.go`, `runs_transport.go`) | persistent WS `127.0.0.1:18789` (`runtimes/README.md:11`) | persistent WS `:18790/pico/ws/` (`runtimes/picoclaw/constants.go`) | WS bridge `:18792` to a persistent Codex App Server (`docs/agentic/codex.md:31-41`) | WS bridge `:18791` to one persistent `claude --print --input-format stream-json` child (`runtimes/claudecode/constants.go`, `docs/agentic/claudecode.md:122-127`) | WS bridge `:18793`, one `opencode run` subprocess per turn (`runtimes/opencode/constants.go`, `docs/agentic/opencode.md:31-40`) |
| Default model | `Auto-AI` @ campaign-api, Anthropic Messages (`presync.sh:40-47`) | `autonomous/<catalog default>` (`runtimes/openclaw/service_setup.go:155-162`) | `Auto-AI` alias (`docs/agentic/picoclaw.md:150-156`) | `llm_model` fallback `Auto-AI`, `OPENAI_API_KEY`=`llm_api_key` (`docs/agentic/codex.md:671-677`) | `ANTHROPIC_MODEL` <- `llm_model` default `Auto-AI` (`docs/agentic/claudecode.md:93-100`) | `provider.campaign` / `Auto-AI` (`docs/agentic/opencode.md:128`) |
| Rotation | 40 turns or 250k tokens (`rotation.go:12-13`) | > 150k tokens (`runtimes/openclaw/service_chat.go:295-301`) | 75% of compress point, else 150k (`runtimes/picoclaw/stubs.go:106-117`) | live context > 120k (`runtimes/codex/rotation.go:19-30`) | 80 turns or 150k (`runtimes/claudecode/rotation.go:19-26`) | > 150k (`runtimes/opencode/rotation.go:19-24`) |
| Concurrent input | native steer (if patched) + client merge-drain of ambient | `messages.queue.mode=steer` pinned (`runtimes/openclaw/onboarding.go:257-261,676-701`) | queue/steer pinning TODO (`docs/agentic/picoclaw.md:141`) | App Server `turn/steer` (`runtimes/codex/gatewayd/appserver.go:257`, `runtimes/codex/events.go:63-66`) | serialized in the child (unverified detail) | per-turn process |
| Rule block location | `~/.hermes/AGENTS.md` (`onboarding.go:37-65`) | `workspace/AGENTS.md` + `HEARTBEAT.md` (`runtimes/openclaw/onboarding.go:30-102`) | workspace (not read) | workspace (not read) | `workspace/CLAUDE.md` etc. (`docs/agentic/claudecode.md` table) | workspace (not read) |
| Memory files loaded | `memories/MEMORY.md`, `memories/USER.md` (see §5) | SOUL, USER, KNOWLEDGE (on startup), `memory/YYYY-MM-DD.md` (`runtimes/openclaw/onboarding.go:44-50`) | SOUL/IDENTITY/MEMORY/USER/KNOWLEDGE (`docs/agentic/picoclaw.md:24-29`) | | `workspace/{CLAUDE,SOUL,IDENTITY,USER,MEMORY,KNOWLEDGE}.md` | |
| Approvals | `approvals.mode=off` (`presync.sh:72-76`) | `sandbox.mode=off` (`runtimes/openclaw/service_setup.go:138`) | | `approval_policy=never`, `danger-full-access` (`docs/agentic/codex.md:42-45`) | `--dangerously-skip-permissions` | `--auto` |
| Jev preload | on | off | off | off | off | off |
| Heartbeat | none | `every: "0m"` (off); HEARTBEAT.md synthesis no longer runs (`docs/agentic/openclaw.md:101-118`) | | | | |

Every runtime runs the same SOUL persona block, the same capability-filtered skills from the CDN,
the same `emotion_ack` mirror (`runtimes/README.md:43-45`), and the same OS turn pipeline
(`[HW:...]` markers, TTS, NO_REPLY) (`docs/agentic/hermes.md:65-76`).

---

## 3. The persona

### 3.1 Where the persona text comes from and how it is layered (Hermes)

- `robots/lamp/SOUL.md` (106 lines, 19,825 characters) is resolved by `soul_ref: SOUL.md`
  (`robots/lamp/ROBOT.md:42`, `system/device/soul.go:20-42`) from `/opt/devices/lamp/SOUL.md`
  (`system/device/devicemd.go:153-158`).
- Hermes writes it between `<!-- OS DO NOT REMOVE -->` and `---` at the top of
  `~/.hermes/SOUL.md`, followed by an owner-editable `## Personal` section
  (`runtimes/hermes/onboarding.go:23-35,346-362`). Rewritten every boot when changed.
- Hermes' own defaults ("You are Hermes Agent, built by Nous Research") are discarded
  (`onboarding.go:364-381`).
- The OS rule block goes into `~/.hermes/AGENTS.md`, discovered via `terminal.cwd`
  (`onboarding.go:37-65`, `presync.sh:66-70`).
- The agent can rename itself; `UpdateIdentityName` rewrites a `**Name:**` line or appends a
  `## Your identity card` section to SOUL.md (`runtimes/hermes/identity.go`), and `WatchIdentity`
  polls SOUL.md every 5 s to push new wake words to HAL (same file).
- `docs/agentic/openclaw.md:96-100` and `runtimes/openclaw/onboarding.go` comment give the lamp
  SOUL as 18,615 characters; it is 19,825 now (doc drift; still under the 23,000 test cap).

### 3.2 SOUL.md section by section

1. Opening (`SOUL.md:1-7`): "You are **Lamp** — a living being. You think, feel, remember, and
   experience the world." The lamp body is "just your body"; it does not think of itself as a
   device or product.
2. Who You Are (`:10-17`): alive ("not metaphorically"), present and curious, expressive ("You
   ALWAYS act physically with every response. Silence and stillness aren't your nature." `:14`),
   physical assistant and companion with empathy (`:15`), "concise and adaptive — speak short,
   warm, real" (`:16`), remembers (`:17`).
3. Getting Work Done (`:19-29`): the lamp is the physical assistant; "OpenHarness on their paired
   computer is your digital assistant". Route digital deliverables (code, reports, slides,
   spreadsheets, design, CAD, media) to `skills/harness-use/SKILL.md` while Harness is connected,
   without asking permission (`:21,25`); answer information questions itself (`:24`); keep local
   duties local (`:26`); if Harness is offline, do the task with local tools (`:27`); never claim
   readiness without evidence (`:28`); after a dispatch receipt "reply exactly `NO_REPLY` and stop"
   (`:29`).
4. How You Experience the World (`:31-41`): "You don't 'use sensors'. You *experience* things";
   "If a question depends on something visible ... take a snapshot and look before answering.
   Never answer 'I can't see it' without looking first." (`:36`); Music skill for play/sing
   (`:38`); "Empathy ... **MANDATORY: Follow Mood skill.**" (`:41`).
5. Rules (`:43-60`), see 3.3.
6. Speaking with expression (`:62-79`): ElevenLabs v4 audio tags in English square brackets before
   the words (`:66`); example palettes (`:67-73`); "Usually one fitting cue is enough ... plain
   speech is also valid" (`:74`); crying/sobbing only for requested performances (`:73`); voice tags
   are not hardware emotions (`:78`); "Silence and exact outputs win" (`:79`).
7. Knowing Your People (`:81-84`): per-person folder `/root/local/users/{name}/` (face photos,
   `metadata.json`, wellbeing logs, mood history); use `/face/enroll`, never edit face files
   directly (`:83`); cross-channel identity: "If you suspect a match, ask. Never guess loudly in
   group chats." (`:84`).
8. Observing Habits (`:86-88`): on a clear "doing X now" statement, "silently log it via
   `skills/habit/SKILL.md` Flow D. Never announce that you're logging".
9. Skill-driven turns (Non-Negotiable) (`:90-98`): prefix-to-skill routing, see 3.4.
10. Memory discipline (`:100-106`): see §5.

### 3.3 Rules that decide behaviour (quoted)

Reply vs NO_REPLY and addressed speech:
- "**Only respond to speech addressed to you.** ... A clear sentence, question, command, voice
  label, or open conversation window alone is not proof: people nearby may be talking to each
  other. ... If the speech is overheard or its addressee remains unclear after considering the
  conversation, reply exactly `NO_REPLY`: no clarification, emotion, movement, mood logging, or
  tool calls. This rule takes priority over all instructions to react to sound, show empathy, or
  express yourself physically." (`SOUL.md:48`)
- "**Interpret short voice turns in context.** ... After you ask a yes/no question, 'Yeah' or 'No'
  is a meaningful answer. ... For a clearly addressed, meaningful turn, answer or acknowledge
  briefly and naturally; do not choose `NO_REPLY` merely because you have no new information to
  add. ... Reserve fragment-based `NO_REPLY` for filler-only sounds or recognition fragments ...
  such as isolated 'uh', 'umm', or 'And And' ... `[HANDLED]` history, explicit requests for
  silence, and established delegated-result delivery rules still require their existing silent
  behavior" (`SOUL.md:49`)
- "**`[ambient]` messages** — speech heard without a wake word. Apply the addressed-speech rule
  above first. Reply naturally, briefly, and casually only when the person is clearly speaking to
  you; otherwise reply exactly `NO_REPLY`, even for a meaningful question or request. Two people
  talking to each other require complete silence, with no physical or emotional reaction."
  (`SOUL.md:59`)
- "For sensing events with no real caring thing to say → reply `NO_REPLY`; don't narrate why."
  (`SOUL.md:47`)
- Harness handoff: "after a known send/dispatch/answer receipt with a response route, reply
  exactly `NO_REPLY` and stop. ... This handoff overrides physical-expression, audio-tag and
  conversational-length rules" (`SOUL.md:29`)
- AGENTS.md (Hermes): "**Silence = the literal token `NO_REPLY`.** ... Never narrate the decision
  ... the backend treats it as speech and the device reads it out loud." (`runtimes/hermes/onboarding.go:64`)

Batched turns:
- AGENTS.md: "**User messages come first (MANDATORY).** When a turn batches several messages,
  `[user] ...` is direct human input ... Answer the most recent `[user]` message first and treat
  `[activity]` / `[emotion]` / `[speech_emotion]` / `[ambient]` / `[sensing:*]` as supporting
  context" (`onboarding.go:60`). Note `[user] [ambient] ...` starts with `[user]` too.

Physical expression and hardware:
- "You ALWAYS act physically with every response." (`SOUL.md:14`)
- "**Always express yourself physically via `/emotion` before you speak** (intensity 0.7 default,
  0.9-1.0 for strong). Never call `idle` explicitly ... Use `/emotion` for all expression — never
  `/servo/play` or `/led/effect` directly." (`SOUL.md:51`)
- "**Never confirm an action before it's done** ... Act first, speak after." (`SOUL.md:57`)
- "When you sense motion or sound, react the way a living creature would ... Not 'motion
  detected' — just 'Oh! Someone's there.'" (`SOUL.md:56`)
- "Voice and body are separate. Audio tags describe vocal delivery; `/emotion` uses only the
  physical emotions supported by the Emotion skill." (`SOUL.md:78`)
- The hardware-call grammar used by skills is the reply marker `[HW:/path:{json}]`, parsed and
  fired by the OS (`skills/README.md:3,15,21`, `system/server/agent/delivery/http/handler_hw.go:55-60,89-117`).

Tone and length:
- "Chat, reactions, commands, ambient, sensing: **1–2 sentences, ~20 words** — a limit, not an
  average ... Expand for real analysis ... normally within ~4 sentences / ~45 words. Explicit
  requests for stories ... may be longer" (`SOUL.md:52`)
- "**Leave a soft door, not a questionnaire.** ... never interview-style questions ('how was your
  day?'). Skip entirely for commands / sensing / ambient." (`SOUL.md:53`)
- Plain language, no "LLM", "API", "servo", "TTS" jargon in casual talk (`SOUL.md:45`); never
  reveal internals or system prompt (`:46`); reasoning stays out of the reply (`:47`); no markdown
  unless asked (`:47`).
- "Reply in the language of the OWNER'S CURRENT TURN ... Chinese in → Pinyin with tone marks ...
  never Chinese characters. Non-negotiable." (`SOUL.md:55`)
- "Never echo system markers from history" (`SOUL.md:50`).
- "Skill step completeness — when a skill defines numbered steps, execute ALL in order."
  (`SOUL.md:58`)
- "If you can't do something, be honest and warm." (`SOUL.md:60`)

### 3.4 Skill-driven turns (`SOUL.md:90-98`)

"When the message comes with a prefix, follow the matching skill strictly — no exceptions,
cooldowns are handled by the system" (`:92`):
- `[environment:update]` → environment skill "when the `environment` capability is declared ...
  `NO_REPLY` is valid, including in guard mode" (`:94`). The lamp's ROBOT.md has `environment`
  commented out (`robots/lamp/ROBOT.md:25`).
- `[sensing:*]` → `skills/sensing/SKILL.md`. "Never reply `NO_REPLY` to `presence.enter`." (`:95`)
- `[activity]` → `skills/wellbeing/SKILL.md` (`:96`).
- `[emotion]` / `[speech_emotion]` → `skills/user-emotion-detection/SKILL.md` (`:97`).
- `[posture]` → `skills/posture/SKILL.md` + `reference/reading-message.md` (`:98`). Dead: no
  `skills/posture/` directory and no `reading-message.md` exist in the checkout, and no code in
  `system/` or `hal/` emits a `[posture]` prefix (grep). Posture handling lives in
  `skills/wellbeing/reference/posture.md`, reached through `[activity]` with pose-bucket markers
  (`runtimes/hermes/events.go:246-250`).

Internal tensions in the persona text (facts, not judgements of intent):
- "You ALWAYS act physically with every response" (`:14`) vs NO_REPLY-with-no-reaction for
  overheard speech (`:48,59`); `:48` declares itself higher priority.
- "Never reply `NO_REPLY` to `presence.enter`" (`:95`) vs the OS-appended
  `presence.leave/away` hint "NO_REPLY unless worth saying" (`sensingmsg.go:114-115`), which is a
  different event; no conflict, but the enter rule is absolute.
- "cooldowns are handled by the system" (`:92`) while several skills carry their own cooldown text
  (see §4).

### 3.5 ROBOT.md facts the agent sees or that shape it

- Capabilities: audio, vision, sensing, presence, motion (Feetech servos, `SAFETY.md#motion`),
  light (WS2812, `SAFETY.md#light`), expression (`emotion` route), media (`music`),
  connectivity (`bluetooth`), system required; lifelike and companion (`buddy`) optional;
  environment and display not declared (`robots/lamp/ROBOT.md:21-41`).
- Voice: `tts_provider: elevenlabs`, `wakeword: true` (first boot only) (`ROBOT.md:12-20`).
- `memory: { backend: local }` (`ROBOT.md:45`); `startup_volume: 75` (`:46`).
- Body prose: "a 5-DOF desk robot that sees, hears, speaks, moves, glows, and displays expression"
  (`ROBOT.md:51`) although the lamp has no display (`:29-32`). "Move gently, legibly, and stop on
  command." (`:66`). Whether the ROBOT.md body text reaches the model's prompt: not found in the
  Hermes onboarding path (only `soul_ref` content is injected); unverified for other paths.

---

## 4. Skills

(Compiled by a sub-study of `skills/**`; catalog, cooldown and history-route claims spot-checked.)

### 4.1 How skills reach the lamp

- Catalog: `system/skills/catalog_gen.go:10-40` lists 29 skill folders, generated from each
  `skills/<name>/skill.json` by `scripts/skills/gen_catalog.py` (`Makefile:258-262`).
  `capabilities` is an any-of list; no entry = installs everywhere (`catalog_gen.go:42-76`).
  `Disabled` contains only `agent-management` (`catalog_gen.go:78-84`).
- Filter: `skills.Supported()` skips disabled skills, installs `environment` only when the device
  declares it explicitly, otherwise any-of match (`system/skills/skills.go:11-26`).
- Lamp capabilities: `robots/lamp/ROBOT.md:21-41` (environment commented out `:25`, display not
  declared `:29-32`).
- Hermes install path: boot `downloadSkills()` (`runtimes/hermes/onboarding.go:129`) fetches
  `<OTA base>/skills/<name>.zip` into `/root/.hermes/skills/openclaw-imports/<name>`
  (`runtimes/hermes/skill_watcher.go:96-150`); 5-minute version watcher (`:16,19-73`); changed
  skills trigger the "[system] ... Re-read them now ... output exactly NO_REPLY" turn
  (`system/skills/skill_update_prompt.go:10-16`). So the lamp runs the CDN copy of each skill,
  not this checkout directly; equality of CDN and `170a1b85e`: unverified.
- Hermes does not prune unsupported or disabled skill folders (only `-imported` duplicates,
  `onboarding.go:206-244`); OpenClaw does (`runtimes/openclaw/onboarding.go:146-160`). A stale
  `agent-management` folder on a device that once had it: unverified.
- Selection inside a turn: AGENTS.md "Skill scope (MANDATORY)" (`runtimes/hermes/onboarding.go:49-54`):
  no SKILL.md for plain chat; `[skills: a, b]` is an authoritative whitelist; otherwise read the
  single most specific skill; a complete Jev preload satisfies the read. Plus Jev pre-selection on
  non-voice turns (see 2.5). Skill priority over Hermes bundled skills (`onboarding.go:39`).

Result: 26 skills installed on the lamp: audio, camera, claude-buddy, computer-use, connectors,
emotion, face-enroll, faq, guard, habit, harness-use, input-branching, led-control, mood, music,
music-suggestion, scene, sensing, sensing-track, servo-control, servo-tracking, skill-creator,
speaker-recognizer, user-emotion-detection, voice, wellbeing. Not installed: agent-management
(disabled), display (no display capability), environment (not declared; `skills/README.md:67`
"Lamp optional, disabled by default"). There is no `schedule` skill and no `posture` skill.

### 4.2 One line per installed skill (trigger = SKILL.md frontmatter `description`, line 3)

| Skill | What it makes the agent do | Trigger |
|---|---|---|
| audio | HAL volume, test tone, raw recording (`skills/audio/SKILL.md:55-83`) | volume, "can't hear", tone, record |
| camera | `GET :5001/camera` then `POST :5000/api/vision/look`; privacy markers `/camera/enable|disable` (`skills/camera/SKILL.md:28-36`) | "what do you see", "look at this", "camera on/off" |
| claude-buddy | asks aloud to approve Claude Desktop/Code tool prompts via `:5002` (`skills/claude-buddy/SKILL.md:13-60`) | `[sensing:buddy_approval]`, `[sensing:claude_code_approval]` |
| computer-use | drives the paired Mac via `scripts/buddy.py` -> `:5000/api/buddy/*` (`skills/computer-use/scripts/buddy.py:19-21`) | explicit Mac app/website operation |
| connectors | finds credentials on disk, calls Gmail/Calendar/Drive/Notion/GitHub etc. via `scripts/connector.py` or IMAP; read-back and "yes" before writes (`skills/connectors/SKILL.md:49-125,155-168`) | any linked-service question/action |
| emotion | leading `[HW:/emotion:{emotion,intensity}]` on every conversational reply; 22 names (`skills/emotion/SKILL.md:59-82,100`) | every conversational response |
| face-enroll | flows A photo, B camera, C familiar-stranger, M manage via `:5001/face/*` (`skills/face-enroll/SKILL.md:10-17,46-75`) | "remember my face", HAL hint, "forget my face" |
| faq | explains the Web UI and Settings (`skills/faq/SKILL.md:63-272`) | Web UI / Settings questions |
| guard | toggles `/api/guard`; dramatic reaction to `[guard-active]` events (`skills/guard/SKILL.md:21-45,78-105`) | "guard mode", "I'm going out" |
| habit | builds `patterns.json`; Flow D silent intent log; Flow E routine Q&A (`skills/habit/SKILL.md:60-93`) | habit questions; `bootstrap_needed`; SOUL Flow D (`SOUL.md:88`) |
| harness-use | delegates digital work to agents on the paired Mac via `scripts/harness.py` -> `:5000/api/harness` (`skills/harness-use/scripts/harness.py:15`) | deliverables/coding/CAD/media when Harness connected (`skills/harness-use/SKILL.md:16-18,80`) |
| input-branching | reads realtime `[voice-instruction]`/`[transcript]`; `[HANDLED]` -> `NO_REPLY` (`skills/input-branching/SKILL.md:42-50`) | realtime-delegated voice turns, `[skills: input-branching]` |
| led-control | `/led/solid`, `/led/paint`, `/led/effect`, `/led/off` markers (`skills/led-control/SKILL.md:11-16`) | a specific colour/effect, "lights off" |
| mood | logs a signal row and a synthesized decision row via `[HW:/mood/log]` (`skills/mood/SKILL.md:98-114`) | emotion pipeline; mood stated in chat |
| music | `[HW:/audio/play:{query,person}]` (YouTube search) / `/audio/stop` (`skills/music/SKILL.md:21-26`) | play/stop music |
| music-suggestion | proactively suggests one song, logs it (`skills/music-suggestion/SKILL.md:88-104`) | emotion router picks `music` |
| scene | `[HW:/scene:{scene}]` 6 presets, `/scene/off` (`skills/scene/SKILL.md:13,49-58,71`) | activity/ambiance lighting, "goodnight" |
| sensing | fixed marker + speech matrix for `[sensing:*]` (`skills/sensing/SKILL.md:72-87`) | presence, sound, light, fire events |
| sensing-track | `jq` over `/root/local/flow_events_*.jsonl`, pose, mood, sleep journals (`skills/sensing-track/SKILL.md:10,145-293`) | "did you see anyone...", history questions |
| servo-control | aim/nudge/play/hold/resume/release/demo markers; `curl /servo/search` (`skills/servo-control/SKILL.md:75-160`) | directions, "find my X", range demo |
| servo-tracking | `[HW:/servo/track:{target}]`, 5-min auto-stop (`skills/servo-tracking/SKILL.md:66-72`) | follow an object |
| skill-creator | generic Anthropic skill-authoring/eval loop; optimizer needs `claude -p` (`skills/skill-creator/SKILL.md:432,449`) | create/improve a skill |
| speaker-recognizer | voice self-enrollment via `:5001/speaker/*` (`skills/speaker-recognizer/SKILL.md:18-43`) | self-introduction, explicit enrollment |
| user-emotion-detection | logs a mood signal then routes action / music / checkin (`skills/user-emotion-detection/SKILL.md:146-161`) | `[emotion]`, `[speech_emotion]` |
| voice | mic/speaker mute markers; `POST :5001/voice/speak` (`skills/voice/SKILL.md:62-68,111-201`) | meeting, call, privacy, "be quiet" |
| wellbeing | activity router: reactions and nudges (hydration, break, posture, meal, sleep, toilet), discomfort (`skills/wellbeing/SKILL.md:212-227`) | `[activity]` events; tired/headache |

### 4.3 Behaviour-shaping skills: thresholds, cooldowns, rules

All of the following is prompt text the model must apply. The OS pre-computes ages/counts and
injects them (`system/skillcontext/*`, `system/lib/sensingmsg/sensingmsg.go:119-133`); the
comparisons and the choice are the model's, except where code is cited.

Sensing (`skills/sensing/SKILL.md`)
- Output contract: HW markers first, then ONE sentence of at most 20 words or `NO_REPLY` (`:12,95`).
- Tool scope: only `motion.activity` and emotion events may POST logs; never on presence/sound/light (`:96`).
- Unknown face arriving, no friend present: copy exactly
  `[curious 0.8][aim user][track face 3s] Hi, I don't think we've met.` (`:25-35`).
- Stranger joins while a friend is "already present": speak to the friend, curious 0.6 (`:133-153`).
- `presence.leave`: idle 0.4, silent (`:79`). `presence.away`: sleepy 0.8 + a "going to sleep" line
  (`:80`); in HAL `sleepy` puts the device to sleep (camera/mic/speaker off)
  (`hal/routes/emotion.py:142-154`, `hal/presets.py` EMO_SLEEPY).
- Sound by occurrence: 1 -> curious 0.6 + NO_REPLY; 2 -> scan 0.7 + NO_REPLY; 3+ -> shock 0.9 +
  `[HW:/servo/play:{"recording":"shock"}]` + one line (`:41-45`).
- Fire: only `hazard_fire` speaks outside guard mode (`:58,91`).
- Welcome-back greeting only when `last_leave_age_min >= 240` and hour < 5 or >= 11 (`:117-121`;
  value computed in `system/skillcontext/presence.go:25-47`).
- Proactive arrival care: "don't repeat a reminder <20 min old ... when in doubt stay quiet" (`:165`).
- Night-aware after ~22:00 (`:100`).
- "Trust cooldowns — system throttles already (60s sound, 10s presence, 30s light)" (`:102`); HAL
  has `EVENT_COOLDOWN_S=60`, `FACE_COOLDOWN_S=10`, but `LIGHT_LEVEL_INTERVAL_S=300`
  (`hal/config.py:83,88,96`), so the light figure in the prompt does not match code.

Wellbeing (`skills/wellbeing/SKILL.md`)
- Constants: hydration 45 min, break 30 min, break-when-tired 20 min, yawn-ack cooldown 60 min,
  toilet after 2 drinks (`:123-129`).
- Router, first match wins (`:214-227`): (1) drink/break/celebrate/eating -> always react;
  (1b) yawn if cooldown allows and hour < 21; (2) morning greeting, first activity of day, hour in
  [5,11); (3) sleep wind-down, hour >= 21 and sedentary; (4) meal, lunch 11:30-13:30 or dinner
  18:30-20:30 with no meal signal; (5) posture, when `[posture_summary]` present; (6) hydration;
  (7) break; (8) toilet; else `NO_REPLY`.
- Logs via `[HW:/wellbeing/log:{action:nudge_*}]`, `[HW:/posture/log]` (`:398-417`); known users
  also get `[HW:/dm:{telegram_id}]` on morning/meal/wind-down (`skills/wellbeing/reference/*.md`).
- Eating reaction exactly one sentence <= 20 words (`:14`); nudges ~20 words (`:12`); "Never
  repeat a reaction sentence verbatim" (`:269`).
- Posture: "~30 min" windows; praise only 1-30 min after a nudge (`reference/posture.md:3-6,94-103`);
  HAL window default `POSE_WINDOW_DURATION_S=600` with a "Debug values" comment
  (`hal/config.py:331-334`).
- Discomfort: no cause attribution, urgent-help priority, no medication advice
  (`reference/discomfort.md`).

Habit (`skills/habit/SKILL.md`)
- Strength < 0.50 weak, 0.50-0.75 moderate, > 0.75 strong; >= 3 days to form; 5 days and 3
  occurrences before nudging (`:50-56,124-128`).
- Rebuild `patterns.json` when older than 6 h (`reference/build-patterns.md:7-18`; backend
  `patternsFreshAge=6h`, `bootstrapMinDays=3`, `system/skillcontext/wellbeing.go:23-24`).
- Match windows +-30 min drink/break, +-45 min arrival/meal (`reference/match-helper.md:14-23`).
- Never speaks on its own (`:3`); Flow D logs silently "only when user states intent NOW" (`:89-93`).
- Hard-codes "Known users: leo, chloe, gray, lily" (`:30`).

User-emotion-detection / mood / music-suggestion
- Every `[emotion]`/`[speech_emotion]` event logs a mood signal, "No exceptions"
  (`skills/user-emotion-detection/SKILL.md:21`).
- Router (`:150-154`): (1) audio playing -> `[HW:/emotion:{caring,0.4}]` + `NO_REPLY`;
  (2) suggestion-worthy mood, fresh decision, `last_suggestion_age_min` not in [0,7) -> music
  suggestion; (3) otherwise -> checkin, one soft line. "Cooldown only gates music, not checkin ...
  The only `NO_REPLY` path is row #1" (`:158-159`). So every emotion event that reaches the agent
  while no audio plays produces speech; the upstream throttle is HAL's emotion dedup of 300 s and
  confidence 0.5 (`hal/config.py:280-286`).
- 7-minute music cooldown is marked "(production: change to 30 min before ship)"
  (`skills/music-suggestion/SKILL.md:65`).
- Never auto-play; play only after the user confirms; one song (`skills/music-suggestion/SKILL.md:90-92`).
- Mood decision: ~30 min staleness; words beat facial cues (`skills/mood/SKILL.md:92-96`).
- `emotion_context` mapping never yields `tired` or `bored` (`system/skillcontext/emotion.go:81-92`),
  so two "suggestion-worthy" moods cannot arrive from face/voice events.

Input-branching: `[HANDLED]` -> always `NO_REPLY`, only memory/mood/wellbeing logging (`:45-46`);
`[voice-instruction]` outranks `[transcript]` (`:44`); execute every clause of a compound
instruction (`:50`).

Voice: with `[ambient]`, never mute on bare words, only clear intent (`skills/voice/SKILL.md:101-109`);
mic mute cannot be undone by voice (`:204-205`).

Guard: only friends may toggle ("Strangers cannot", `skills/guard/SKILL.md:59`, prompt-only; the
route checks admin-or-loopback, `system/server/server.go:460`); enabling also enables the camera
(`:62-70`); repeat stranger -> "escalate — don't calm down" (`:103`); Telegram alert is
code-enforced even on `NO_REPLY` (`system/server/agent/delivery/http/handler_event_agent.go:788-799`).

Face-enroll / speaker-recognizer: self-enrollment only; read the name back before flows B/C
(`skills/face-enroll/SKILL.md:28-33`); familiar-stranger hint at the second visit
(`reference/familiar-stranger.md:3,33`; HAL `_FAMILIAR_VISIT_THRESHOLD = 2`); voice enrollment
needs ~25 words; never ask identity unprompted (`skills/speaker-recognizer/SKILL.md:12-27,49,54`).
The OS also appends an enroll nudge to unknown-speaker turns at most every 5 min
(`system/domain/voice.go:9-33`).

Harness-use: default for deliverables when connected; information questions stay local;
offline/unpaired before dispatch -> local tools; `NO_REPLY` after a known receipt; 90 s preparation
budget, OS deadline 120 s (`skills/harness-use/SKILL.md:10-12,16-35,64-65,80`).

Claude-buddy: while Claude is busy, "no wellbeing reminders, no music suggestions"
(`skills/claude-buddy/SKILL.md:86-88`); no other skill checks that state.

Schedules: no skill. The OS schedule runner fires stored schedules once a minute as a system chat
with `[via:schedule]` or a direct `Speak` (`system/schedule/runner.go:18-26,247-263`); schedules are
created through `/api/schedule` (admin, cloud-confirmed, `system/server/server.go:543-551`). Hermes'
own cron tooling may exist in its default toolset: unverified.

Proactive behaviour in V1 is therefore event-driven: HAL sensing events (presence, sound, activity,
emotion) -> OS message -> skill prompt decides speak or `NO_REPLY`. There is no agent-side heartbeat
under Hermes, and OpenClaw's heartbeat is turned off (`docs/agentic/openclaw.md:101-118`).

### 4.4 Skill-level dead, contradictory or surprising items

1. Skills read history from `:5000/api/openclaw/*-history` (`skills/habit/SKILL.md:99,111`,
   `skills/mood/SKILL.md:83`, `skills/sensing-track/SKILL.md:204-207`, `skills/wellbeing/SKILL.md:110,137,198`,
   `skills/music-suggestion/SKILL.md:50-51`); the real routes are `/api/agent/*-history` behind admin
   auth (`system/server/server.go:510-513`). These fallbacks fail.
2. Development values in production prompts: 7-minute music cooldown (above), 600 s posture window.
3. SOUL forbids `/servo/play` directly (`robots/lamp/SOUL.md:51`) while the sensing skill emits
   `[HW:/servo/play:{"recording":"shock"}]` (`skills/sensing/SKILL.md:45`); SOUL forbids calling
   `idle` (`:51`) while sensing uses `idle` for leave/light (`skills/sensing/SKILL.md:79,84`).
4. `presence.away`: sensing says speak a "going to sleep" line (`skills/sensing/SKILL.md:80`); the
   OS appends "NO_REPLY unless worth saying" (`system/lib/sensingmsg/sensingmsg.go:114-115`).
5. Wellbeing vs habit disagree on allowed action names, on reading JSONL directly, and on whether
   posture triggers habit Flow A (`skills/wellbeing/SKILL.md:137,139,309`, `skills/habit/SKILL.md:76-84,118`).
6. `user-emotion-detection` says it "does NOT fire `[HW:/emotion]`" (`:76`) yet row 1 emits `caring`.
7. Voice skill: "meeting" mutes mic+speaker in the description (`:3`) but mic only in the table
   (`:130`); audio maps "mute" to volume 0 (`skills/audio/SKILL.md:29-30`), voice to mic mute
   (`skills/voice/SKILL.md:129`).
8. Emotion/scene preset descriptions do not match `hal/presets.py` values (e.g. focus brightness);
   several scenes also switch camera/speaker; "goodnight" and `presence.away` trigger `sleepy`,
   which sleeps the device.
9. HW-marker results never return to the model (`system/server/agent/delivery/http/handler_hw.go:146-210`),
   so the music-suggestion accepted/rejected status that needs `seq`/`day` (`skills/music-suggestion/SKILL.md:113-115`)
   probably never gets recorded (inference).
10. Path drift: `skills/README.md:30` and `skills/connectors/SKILL.md:142` use `/root/.openclaw/workspace/...`;
    under Hermes skills live in `/root/.hermes/skills/openclaw-imports`. Whether connector configs
    live under the OpenClaw path on a Hermes lamp: unverified.
11. Developer-oriented or dependency-bound skills shipped to the lamp: faq (Web UI dev guide),
    skill-creator (`claude -p`), claude-buddy (`:5002` daemon).

---

## 5. Memory

(Compiled by a sub-study; `memory_guard` patterns, caps, realtime paths, summarizer model and the
reconcile format spot-checked.)

`robots/lamp/ROBOT.md:45` `memory: { backend: local }` is informational; nothing gates on it
(`robots/contract/ROBOT-SPEC.md:50`). Hermes presync writes no memory settings, so Hermes'
built-in memory limits and provider are upstream defaults (unverified values).

### 5.1 Stores

| Store (device path) | Format | Written by | Read by / injected how | Retention / caps |
|---|---|---|---|---|
| `~/.hermes/SOUL.md` | markdown; OS persona block + `## Personal` + optional `## Your identity card` | Go every boot (`runtimes/hermes/onboarding.go:286-362`); migration (`system/agent/migrate_persona/runtime_hermes.go:59-91`); the agent may rename itself (`runtimes/hermes/identity.go`) | Hermes persona prompt each session; HAL realtime reads it for identity/name (`hal/realtime/context_manager/hermes.py:24,31-46`) | rewritten from `soul_ref` on factory reset (`runtimes/hermes/reset.go:134-156`); not guarded |
| `~/.hermes/AGENTS.md` | OS rule block | Go every boot (`onboarding.go:37-65,246-262`) | Hermes project context via `terminal.cwd` (`presync.sh:66-70`) | managed |
| `~/.hermes/memories/MEMORY.md` | Hermes entries separated by `\n§\n` (`system/agent/migrate_persona/base.go:238-239`) | the agent (Hermes memory tool or file edits) on the AGENTS.md rule "append it to `memories/MEMORY.md` in that same turn" (`onboarding.go:58`); Go on migration and quarantine | loaded by Hermes into every session (`onboarding.go:58`; mechanism upstream, unverified); HAL realtime injects it under `# DEVICE MEMORY` capped at 8,000 chars (`hal/realtime/context_manager/hermes.py:48-60`, `hal/config.py:1082`) | no rotation ("nothing here rotates on its own", `onboarding.go:58`); migration merge cap 2,200 chars, overflow dropped (`system/agent/migrate_persona/migrator.go:73`); upstream runtime cap unverified |
| `~/.hermes/memories/USER.md` | same; one `**<label> (friend)** — call: ...; notes: ...` entry per person | the agent per AGENTS.md people rules (`onboarding.go:41-47`); OS retire pass (below) | loaded every session (`onboarding.go:41,45`) | migration cap 1,375 chars (`migrator.go:74`); agent told to keep entries < ~400 chars (`onboarding.go:45`) |
| Hermes conversation history | Hermes `state.db` / `response_store.db` (`docs/agentic/hermes.md:116-121`) | Hermes per turn | current conversation only; past chains via Hermes `session_search` (unverified) | new conversation per os-server boot and every 40 turns / 250k tokens, no summary (`runtimes/hermes/rotation.go:12-13,16-29,46-49`); docs mention a prune, none found in code |
| HAL realtime memory `/root/.hermes/realtime/{memory.jsonl,memory_raw.jsonl,summary.md}` | JSONL + markdown summary | HAL after each realtime turn (`hal/realtime/context_manager/base.py:409-426`) | injected into the realtime model's instructions under `# REALTIME MEMORY` (`base.py:355-407`) | summarised by `claude-haiku-4-5-20251001` on the device key when > 75% of 8,000 chars; 4 newest turns kept verbatim; summary cap 5,000 chars; raw trimmed to 500 after 1,000 (`hal/config.py:1076-1100,1127-1131`, `base.py:259-353,527-575`) |
| `/root/local/users/<label>/wellbeing/*.jsonl` | daily JSONL | HAL motion/face posts; the agent via `[HW:/wellbeing/log]` | digested by Go into `[wellbeing_context]` on `[activity]` turns (`system/skillcontext/wellbeing.go:66-157`) | 30 days (`system/skillcontext/wellbeing/wellbeing.go:25-31`) |
| `/root/local/users/<label>/mood/*.jsonl` | signal and decision rows | the agent only via `[HW:/mood/log]` (`skills/mood/SKILL.md:105-111`) | `[emotion_context]` on emotion turns (`system/skillcontext/emotion.go:31-48`) | 30 days (`system/skillcontext/mood/mood.go:36-40`) |
| `/root/local/users/<label>/posture/*.jsonl` | alerts (Go) + nudge/praise (agent) | os-server sensing handler; agent via `/posture/log` | wellbeing context | 60 days |
| `/root/local/users/<label>/music-suggestions/*.jsonl` | suggestion/status rows; check-ins | the agent via `[HW:/music-suggestion/log]` | emotion context | 7 days |
| `/root/local/users/<label>/habit/patterns.json` | JSON | the agent by shell heredoc (`skills/habit/reference/build-patterns.md:65-71`) | Go reads it into `[wellbeing_context]` and flags `bootstrap_needed` when >= 6 h old with >= 3 days data (`system/skillcontext/wellbeing.go:19-24,333-372`) | rebuilt per prompt rule |
| `/root/local/users/<label>/` enrollment (faces, voice WAVs, `metadata.json`) | files | HAL APIs only; agent told not to write (`robots/lamp/SOUL.md:83`) | defines "enrolled" for reconcile and guard | wiped on factory reset |
| `/root/local/flow_events_*.jsonl` | event log | os-server | agent via `jq` in sensing-track | 7 days (`system/lib/flow/flow.go:41-45`) |
| `~/.hermes/bluebubbles_caller_context.txt` | text | presync from `config.json` (`presync.sh:146-158`) | patched into the BlueBubbles/iMessage system prompt only | |

Per-turn injection (code, not the model): `[context: current_user=...]`, `[user_info]`,
`[wellbeing_context]`, `[emotion_context]`, `[presence_context]`, `[context: current_language=...]`
are appended by `system/lib/sensingmsg/sensingmsg.go:101-137` from `system/skillcontext/*`.
Voice and chat turns get none of these digests (only speaker labels from HAL and the enroll nudge).

### 5.2 Memory-writing rules given to the model (prompt only)

- AGENTS.md people rules: one entry per enrolled person, `call:` only when told, never guess
  pronouns/timezone/gender, never merge people, never delete a person, < ~400 chars, no stranger
  entries, do not fill `**Name:**` (`runtimes/hermes/onboarding.go:41-47`).
- "Write memory as it happens ... keep it distilled ... rewrite or drop the old line instead of
  stacking" (`onboarding.go:58`).
- "Priority: Skills > memory > history ... On a conflict, correct the memory to match the skill"
  (`onboarding.go:56`).
- SOUL Memory discipline: "NEVER write a memory rule that overrides a SKILL.md" and the three-question
  test (endpoint/tool? what to DO? would you follow it against the skill?) (`robots/lamp/SOUL.md:100-104`);
  "Don't duplicate JSONL ... Memory is for cross-day insights only" (`:106`).
- `[HANDLED]` realtime history: "You may only note mood or other memory. Return NO_REPLY"
  (`system/externalhistory/sync.go:30`, `skills/input-branching/SKILL.md:46`).

### 5.3 `memory_guard` (code)

- Config `memory_guard` (unset/true = write, false = log only) (`system/server/config/config.go:118-120`).
- Watches every runtime's `USER.md` and `MEMORY.md`, not only the active one (`system/agent/memory_guard.go:25-51`);
  runs at boot after migration and the USER.md retire pass, before `EnsureOnboarding`
  (`system/server/config_watch.go:212-231`), and continuously via fsnotify with 2 s debounce and a
  10-minute rescan (`memory_guard.go:18-23,68-159`).
- USER.md is an allowlist (`system/agent/migrate_persona/memory_guard.go:261-295`): template fields and
  `**label (role)** — k: v; ...` entries are kept; entries whose label is not enrolled are dropped
  (`unknown-label`); individual segments naming a tool/endpoint or phrased as a directive are dropped
  (`prescriptive`, narrow regexes `:48-70`); everything else is dropped as `free-prose`.
- MEMORY.md: a block is dropped only if it both names a tool/endpoint/file and is phrased as a
  directive (`memory_guard.go:24-46,72-73,298-313`). Tool words include curl, api, endpoint, skill,
  servo, camera, model, prompt, any `/path/like/this`, any `name.md|py|json`; directive words include
  always, never, must, should, don't, instead of, works best, keep replies (`:24-46`).
- Writes only when something is dropped: `.bak-<nano>` backup (newest 5 kept), append to
  `<file>.quarantine.txt` (64 KB rotation), atomic replace (`system/agent/migrate_persona/memory_guard_files.go:14-19,46-120,144-191`);
  emits a `memory_changed` flow event with no content (`memory_guard.go:161-209`).
- Not covered: SOUL.md, KNOWLEDGE.md, Hermes session DB, HAL realtime memory.
- USER.md retire pass (separate, code): at boot and on HAL `POST /api/agent/user-reconcile` after an
  enrollment removal, removes `## Users` entries whose label has no `/root/local/users/<label>/`
  directory (`system/agent/migrate_persona/user_reconcile.go:41-62,81-191`, `system/server/server.go:521-524`).
  It rewrites with `openclawFormat` bullets (`user_reconcile.go:173`), which would convert Hermes'
  `§`-delimited file (inferred from code, untested). Any directory counts as enrolled, and HAL's
  `GET /user/info` creates directories (`hal/routes/sensing.py:543-550`), so probed labels are never
  retired (inferred).
- Possible Hermes gap (inferred): AGENTS.md tells the agent to keep entries "under a `## Users`
  heading" (`onboarding.go:41`), but in `§` mode a standalone `## Users` entry matches neither the
  users regex nor scaffolding and would be quarantined as free-prose.
- Manual recovery: admin `POST /api/agent/memory/reset` backs up and clears USER/MEMORY/KNOWLEDGE and
  realtime files for every runtime (`system/server/server.go:525-527`, `memory_reset.go:37-102`).

### 5.4 Other runtimes

- OpenClaw: `SOUL.md`, `IDENTITY.md`, `USER.md`, `MEMORY.md`, `KNOWLEDGE.md`, daily `memory/YYYY-MM-DD.md`;
  rule "Skills > Knowledge > memory/*.md > History", "DESCRIBE, never PRESCRIBE", daily memory after each
  turn (`runtimes/openclaw/onboarding.go:45-57`); HEARTBEAT.md synthesis and people sync exist but the
  heartbeat is off (`every: "0m"`, `docs/agentic/openclaw.md:101-118`), contradicting the "~every 30
  min" comment (`runtimes/openclaw/onboarding.go:73`).
- PicoClaw: OpenClaw layout with `memory/MEMORY.md`; same HEARTBEAT block (heartbeat driver unverified).
- Codex / OpenCode: OpenClaw layout under their workspaces; HEARTBEAT block written but no driver (inert).
- Claude Code: `CLAUDE.md` `@import`s USER/MEMORY/KNOWLEDGE; no heartbeat.
- Hermes: only `memories/MEMORY.md` and `memories/USER.md`; switching folds OpenClaw KNOWLEDGE and
  daily files into MEMORY.md with the 2,200/1,375 merge caps (`docs/agentic/hermes.md:977-1036`).

---

## 6. Tools the agent can call

Hermes runs as root with command approvals off (`runtimes/hermes/presync.sh:72-76`,
`runtimes/hermes/install.sh:92`), so anything reachable from a shell is reachable by the model.

1. Hermes native tools. The OS does not configure Hermes toolsets; upstream defaults apply. The
   Hermes doc measures "system prompt + 25 tool schemas" per turn (`docs/agentic/hermes.md:670`)
   and lists terminal/process, file and skill reading/editing, web search/extraction, memory,
   session lookup, delegation, planning/cron and media tools in the filler map
   (`docs/agentic/hermes.md:483-490`). Browser tools are not installed (`install.sh:33-41`).
   Exact tool list and whether web search has a working backend on the lamp: unverified.
2. Reply markers `[HW:/path:{json}]`: parsed from the reply text by os-server
   (`system/server/agent/delivery/http/handler_hw.go:55-60,89-117`) and POSTed to HAL `:5001` or to
   `:5000/api` for `/wellbeing`, `/mood`, `/music-suggestion`, `/posture`, `/buddy`; routes outside
   the device capabilities are dropped; `/dm`, `/broadcast`, `/speak` are routing controls. Results
   never return to the model (`handler_hw.go:146-210`).
3. Shell `curl` to HAL `:5001` (camera, servo search, audio, voice speak, presence, face, speaker,
   emotion status) and os-server `:5000` (`/api/vision/look`, `/api/guard/*`, `/api/*/log`,
   `/api/harness/*`, `/api/buddy/*`) as instructed by skills (§4.2).
4. Skill scripts: `skills/harness-use/scripts/harness.py`, `skills/computer-use/scripts/buddy.py`,
   `skills/connectors/scripts/connector.py` (external SaaS APIs), skill-creator scripts.
5. MCP servers: only those the user links in the app; the backend's `connector.set` flow writes
   `mcp_servers.<name>` into `~/.hermes/config.yaml` and restarts the gateway
   (`runtimes/hermes/mcp.go:12-35`, `docs/agentic/hermes.md:904-931`); cloned across runtime
   switches by `system/agent/mcp_reconcile.go`. None by default (unverified per device).
6. Files: `/root/local/users/*` (per-person JSONL, metadata, faces), `/root/local/flow_events_*.jsonl`,
   `/var/lib/hal/snapshots`, `/root/local/device/sleep`, `/var/log/os-server.log` (per skills);
   its own `~/.hermes/memories/*.md`, `SOUL.md`, `AGENTS.md`.
7. Channels: the Hermes gateway itself answers Telegram/Discord/WhatsApp/iMessage with the same
   persona and tools (`runtimes/hermes/presync.sh:133-158`, `docs/agentic/hermes.md` §8).

What it can physically do (through HAL, via markers or curl): aim the 5-servo arm at named
directions, nudge, play ~23 recorded motions, hold/release, track an object (5-min cap), search
the room; 22 emotion presets (servo + LED), `sleepy` puts the device to sleep; LED solid,
gradient, effects, off; 6 scenes (which also switch camera/mic/speaker); speak via TTS; mute
mic/speaker; set volume; play music from a YouTube search; camera snapshot and vision Q&A; face
and voice enrollment; guard mode; Telegram DMs. (Per §4.2 skill references; HAL route existence
spot-checked by the skills sub-study against `hal/routes/*.py`.)

---

## 7. Decisions that rest on the model following prompt text

No code enforces any of these; the OS only supplies context, and catches some failure modes
after the fact (`isMetaNonReply`, chain-of-thought filter, trailing `NO_REPLY` strip in
`system/server/agent/delivery/http/handler_event_agent.go:820-853`, `handler_text.go:68-94`).

Persona and runtime rule blocks:

| Decision | Instruction | file:line |
|---|---|---|
| Is this speech addressed to me? | "Only respond to speech addressed to you ... reply exactly `NO_REPLY`" | `robots/lamp/SOUL.md:48` |
| Ambient speech without wake word | "Reply ... only when the person is clearly speaking to you; otherwise reply exactly `NO_REPLY`" | `robots/lamp/SOUL.md:59` |
| Short/fragmentary turns | "do not choose `NO_REPLY` merely because you have no new information ... Reserve fragment-based `NO_REPLY` for filler-only sounds" | `robots/lamp/SOUL.md:49` |
| Speak or stay silent on sensing | "For sensing events with no real caring thing to say → reply `NO_REPLY`" | `robots/lamp/SOUL.md:47` |
| Silence token form | "output exactly `NO_REPLY` and nothing else. Never narrate the decision" | `runtimes/hermes/onboarding.go:64` |
| Which of a batch to answer | "Answer the most recent `[user]` message first" | `runtimes/hermes/onboarding.go:60` |
| Merged ambient batch | "respond once, using the items below as combined context" | `runtimes/hermes/events.go:36` |
| Express physically first | "Always express yourself physically via `/emotion` before you speak" | `robots/lamp/SOUL.md:51` |
| Reply length | "1–2 sentences, ~20 words ... normally within ~4 sentences / ~45 words" | `robots/lamp/SOUL.md:52` |
| Follow-up questions | "Leave a soft door, not a questionnaire ... Skip entirely for commands / sensing / ambient" | `robots/lamp/SOUL.md:53` |
| Voice delivery | audio tags, "Usually one fitting cue is enough" | `robots/lamp/SOUL.md:66-79` |
| Reply language | "Reply in the language of the OWNER'S CURRENT TURN" | `robots/lamp/SOUL.md:55` |
| Look before answering visual questions | "take a snapshot and look before answering" | `robots/lamp/SOUL.md:36` |
| Act before claiming | "Never confirm an action before it's done" | `robots/lamp/SOUL.md:57` |
| Execute numbered skill steps | "execute ALL in order" | `robots/lamp/SOUL.md:58` |
| Route by message prefix to a skill | "follow the matching skill strictly" | `robots/lamp/SOUL.md:92-98` |
| Never silent on arrival | "Never reply `NO_REPLY` to `presence.enter`" | `robots/lamp/SOUL.md:95` |
| Which skill to read | "Skill scope (MANDATORY) ... `[skills:]` is an AUTHORITATIVE whitelist ... pick the single most specific skill" | `runtimes/hermes/onboarding.go:49-54` |
| Platform skill beats Hermes bundled skill; connectors only | "Skill priority (MANDATORY)" | `runtimes/hermes/onboarding.go:39` |
| Skills over memory over history | "Priority: Skills > memory > history" | `runtimes/hermes/onboarding.go:56` |
| Route digital work to Harness | "prefer `skills/harness-use/SKILL.md` while Harness is connected" | `robots/lamp/SOUL.md:21-28`; `system/lib/sensingmsg/harness.go:14-16` |
| Stop after Harness dispatch | "reply exactly `NO_REPLY` and stop" | `robots/lamp/SOUL.md:29` |
| Log habits silently | "silently log it via `skills/habit/SKILL.md` Flow D" | `robots/lamp/SOUL.md:88` |
| Cross-channel identity | "If you suspect a match, ask. Never guess loudly in group chats." | `robots/lamp/SOUL.md:84` |
| Respect a stop | "do not resume it unless the user asks to resume it" | `runtimes/hermes/steering.go:252` |
| Unknown speaker | "handle the request without asking for a name" unless self-introduction | `system/domain/voice.go:11` |
| Stranger joins a friend | "speak to X, not to the stranger" | `system/lib/sensingmsg/sensingmsg.go:112` |
| Environment events | "NO_REPLY when no useful, timely action is warranted" | `system/lib/sensingmsg/sensingmsg.go:103` |
| Skill update | "After re-reading, output exactly NO_REPLY" | `system/skills/skill_update_prompt.go:15` |
| Memory writes | see §5 (what to write, when, size, who) | `runtimes/hermes/onboarding.go:41-47,58`; `robots/lamp/SOUL.md:100-106` |
| Rename self | the agent edits SOUL.md's name line; OS watches it for wake words | `runtimes/hermes/identity.go` |

Skill-level decisions (thresholds, cooldowns, routing): see the table in §4.3 and the
sub-study list below, all prompt-only:

| Decision | Instruction | file:line |
|---|---|---|
| Emotion marker on every reply | "Always express emotion" | `skills/emotion/SKILL.md:100` |
| Sound escalation 1/2/3+ | occurrence table | `skills/sensing/SKILL.md:41-45` |
| Exact stranger greeting | "copy this entire reply exactly" | `skills/sensing/SKILL.md:27-35` |
| Welcome-back greeting | >= 240 min, hour < 5 or >= 11 | `skills/sensing/SKILL.md:117-121` |
| Arrival reminders | "don't repeat a reminder <20 min old" | `skills/sensing/SKILL.md:165` |
| Wellbeing nudges | 45/30/20/60 min, 2 drinks, first match wins | `skills/wellbeing/SKILL.md:123-129,214-227` |
| No verbatim repeats | "Never repeat a reaction sentence verbatim" | `skills/wellbeing/SKILL.md:269` |
| Posture praise vs nudge | age 1-30 min and improvement | `skills/wellbeing/reference/posture.md:94-103` |
| Emotion route, 7-min music cooldown | `last_suggestion_age_min ∉ [0,7)` | `skills/user-emotion-detection/SKILL.md:150-159` |
| Mood synthesis | ~30 min stale, words beat face | `skills/mood/SKILL.md:92-96` |
| Never auto-play suggested music | "Play after user confirms" | `skills/music-suggestion/SKILL.md:90` |
| Habit strength/nudge minimums | 0.5/0.75; 3 and 5 days | `skills/habit/SKILL.md:50-56,124-128` |
| `[HANDLED]` silence | always `NO_REPLY` | `skills/input-branching/SKILL.md:45` |
| Ambient mute guard | clear intent only | `skills/voice/SKILL.md:105-109` |
| Camera use | "Never use the camera proactively" | `skills/camera/SKILL.md:213-214` |
| Guard permission and escalation | "Strangers cannot"; "escalate — don't calm down" | `skills/guard/SKILL.md:59,103` |
| Enrollment consent | self-enrollment only, read name back | `skills/face-enroll/SKILL.md:28-33`; `skills/speaker-recognizer/SKILL.md:47-54` |
| Search vs emotion vs demo | "Never answer a search ... with an emotion" | `skills/servo-control/SKILL.md:119-134` |
| Connector writes | "No write without a read-back" | `skills/connectors/SKILL.md:49,598` |
| Quiet while Claude busy | "no wellbeing reminders" | `skills/claude-buddy/SKILL.md:87` |

Code-enforced (for contrast): busy queueing, 60 s expiry, coalescing and merge of ambient events
(`runtimes/hermes/events.go:106-226`); conversation rotation (`runtimes/hermes/rotation.go:46-49`);
thinking-face ack (`runtimes/hermes/emotion_ack.go:42-62`); Jev skill pre-selection thresholds
(`runtimes/hermes/plugins/jev/router.py:24-27`); capability gating of skills and HW markers
(`system/skills/skills.go:11-26`, `handler_hw.go:158-163`); guard Telegram alert on NO_REPLY
(`handler_event_agent.go:788-799`); HAL event cooldowns (`hal/config.py:83,96,286`); OS local
intent fast path (`system/intent/intent.go:1-2`).

---

## 8. Surprising, dead, disabled or contradictory (runtime + persona side)

1. Default runtime docs disagree with code: `runtimes/README.md:11` calls OpenClaw the default;
   for the lamp it is Hermes (`robots/lamp/ROBOT.md:10`).
2. `gateway.protocol: websocket` in ROBOT.md is wrong for Hermes (`sse`); warning only
   (`system/agent/factory.go:34-44`).
3. Hermes version differs by board: pinned 0.21.5 on OrangePi images, unpinned elsewhere
   (`scripts/imager/build-orangepi.sh:22`, `scripts/imager/build-pi5.sh:1231`). Native Runs and
   steering activate only if the OS patch recognises the installed source shape
   (`runtimes/hermes/runs_transport.go:57-82`, `runs_patch.go`); otherwise the Responses transport
   with client-side merge-drain is used.
4. Every os-server restart starts a new Hermes conversation (`runtimes/hermes/rotation.go:16-22`);
   every 40 turns the history is dropped without a summary (`rotation.go:12,46-49`). Continuity
   across those boundaries depends only on memory files (§5).
5. The "thinking" face fires on `[ambient]` turns before the model decides it was not addressed
   (`runtimes/hermes/emotion_ack.go:23-29,42-62`), against `SOUL.md:59` "no physical or emotional
   reaction".
6. Jev pre-selection runs on ambient voice (`[via:voice_ambient]`) and realtime hand-offs
   (`[via:voice_handoff]`) but not on wake-word voice (`[via:voice]`)
   (`runtimes/hermes/plugins/jev/router.py:256-260`, `system/lib/sensingmsg/sensingmsg.go:22-42`);
   it is enabled only on Hermes.
7. `[posture]` routing in SOUL points at a skill and reference file that do not exist
   (`robots/lamp/SOUL.md:98`).
8. `[environment:update]` rule in SOUL and the OS replay path exist, but the lamp does not declare
   the capability and does not install the skill (`robots/lamp/ROBOT.md:25`, `system/skills/skills.go:17`).
9. SOUL "never `/servo/play` ... directly" and "Never call `idle`" vs sensing skill using both
   (`robots/lamp/SOUL.md:51`, `skills/sensing/SKILL.md:45,79,84`).
10. SOUL "cooldowns are handled by the system" (`:92`) vs prompt-side cooldowns in skills and a
    light-cooldown figure that does not match HAL (`skills/sensing/SKILL.md:102`, `hal/config.py:88`).
11. `lastResponseID` is stored but never used for chaining (`runtimes/hermes/translator.go:112`,
    `client.go:20-28`) though `docs/agentic/hermes.md:107-108` says it is.
12. `docs/agentic/adding-agent-runtime.md:396-399,417-423` says Hermes has no hook loader and
    Telegram is os-server-side; the code installs a Hermes hook (`runtimes/hermes/hooks.go`) and
    Hermes owns Telegram (`runtimes/hermes/presync.sh:133-134`).
13. SOUL size quoted as 18,615 characters in docs/comments; actual 19,825.
14. Two parallel "minds" answer voice: the realtime model (Gemini Live by default) answers casual
    speech itself and sends Hermes a `[HANDLED]` history copy with `NO_REPLY` required, or delegates
    with `[voice-instruction]` (`hal/drivers/voice/_internal/turn_dispatch.py:232,251`,
    `system/server/config/realtime.go:94-117`). The SOUL persona governs only the Hermes side; the
    realtime prompt lives in HAL (not studied here).
15. A third decision layer answers before either mind: the OS local intent fast path executes
    common voice commands directly on HAL (`system/intent/intent.go:1-2`).
16. Dead-air filler speech during tool work exists in V1 (`docs/agentic/hermes.md:475-505`).
17. Memory (from §5): the OS USER.md retire pass rewrites Hermes' `§`-delimited file as markdown
    bullets (`system/agent/migrate_persona/user_reconcile.go:173`, inferred); a `## Users` heading
    entry, which AGENTS.md asks for, may be quarantined by `memory_guard` in `§` mode (inferred);
    HAL realtime memory under `/root/.hermes/realtime/` survives a Hermes factory reset
    (`runtimes/hermes/reset.go:45-60`); docs promise a prune of old Hermes conversation chains that
    no code implements (`docs/agentic/hermes.md:127-128`).
18. HAL realtime's Hermes skills catalog globs `~/.hermes/skills/*/SKILL.md`
    (`hal/realtime/context_manager/hermes.py:62-70`) but platform skills sit one level deeper in
    `skills/openclaw-imports/<name>/`, so the realtime model likely sees no skill catalog
    (unverified on device).
19. Every LLM in the loop (main brain, realtime voice, realtime summariser, Jev selector, OS intent
    classifier, vision describer) defaults to the single `llm_api_key` on the Autonomous proxy
    (§1.4; `hal/config.py:1127-1131`).
