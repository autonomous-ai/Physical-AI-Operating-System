# V1 os-server: event and agent plumbing (study notes)

Checkout: `v1-main` at `170a1b85e` (origin/main, V1 as deployed). All paths are relative to it.
Scope: `system/server/**`, `system/agent`, `system/domain/agent.go`, `system/intent`, `system/ambient`,
`system/lib/sensingmsg`, `system/lib/speakergate`, `system/skillcontext`, `system/healthwatch`,
`system/statusled`, `system/monitor`, `system/schedule`, `system/environment`, `system/externalhistory`,
`system/lib/hal` (OS-side HAL client). Runtime adapters (`runtimes/**`) are cited only at the boundary
(busy flag, pending queue, run ids). Facts only; "unverified" marks what code here cannot prove.

---

## 0. Lamp baseline facts that shape everything below

| Fact | Evidence |
|---|---|
| Lamp default agent runtime is **Hermes** (not OpenClaw). Resolution order: `config.agent_runtime` > factory file `f_r_default_agent` > ROBOT.md `gateway.default` > `openclaw`. | `robots/lamp/ROBOT.md:9-11` (`gateway: default: hermes`); `system/device/runtime.go:52-60,85-94`; `system/agent/factory.go:46-121` |
| Realtime voice brain is on by default (provider gemini), so **local chitchat rules are disabled**. | `system/server/config/realtime.go:94-125`; `system/server/sensing/delivery/http/handler.go:185`; `system/server/config_watch.go:30` |
| Lamp declares `lifelike` → ambient loops run; `light`, `motion`, `audio`, `expression`, `media`, `companion` declared. | `robots/lamp/ROBOT.md:20-42` |
| `environment` capability is **commented out** for Lamp Standard; the three Pro profiles (`pro`, `pro-respeaker-lite`, `pro-xvf3800`) enable it by rewriting ROBOT.md at provisioning. os-server reads only ROBOT.md (no profile.json). | `robots/lamp/ROBOT.md:24`; `robots/lamp/overrides/*/profile.json`; `scripts/provision/apply-overrides.py:85-99`; `system/device/devicemd.go:48-62` |
| Wake-word gate default on for a fresh config (`voice.wakeword: true`). | `robots/lamp/ROBOT.md:19`; `system/server/server.go:297-309` |
| `OS_REALTIME_SUPERSEDES_MAIN_REPLY=1` on the lamp (os-server reads `/opt/hal/.env`). | `robots/lamp/rootfs/opt/hal/.env` (block "Read by os-server"); `system/server/agent/delivery/http/handler_tts.go:163-166` |
| Startup volume 75 (Standard), 35 (pro-respeaker-lite, max 50), 77 (pro-xvf3800). | `robots/lamp/ROBOT.md:46`; `robots/lamp/overrides/*/profile.json`; `system/server/config_watch.go:308-319` |
| No lamp-specific override of `local_intent`, `jev_intent`, `sensing_turn_floor_s`, `guard_mode` exists in repo; lamp uses code defaults. | grep over `scripts/` and `robots/` found none |

---

## 1. Event intake: every way something reaches the agent

### 1.0 The central funnel: `POST /api/sensing/event` → `SensingHandler.PostEvent`

Route: `system/server/server.go:442-444` (admin or direct-loopback auth). Handler:
`system/server/sensing/delivery/http/handler.go:198-766`. Producers that use this funnel:
HAL (voice, sensing, touch, fire), web monitor chat (`system/web/src/pages/monitor/ChatSection.tsx:1493`),
MQTT phone chat (`system/server/device/delivery/mqtt/chat_send_handler.go:83-139`), Buddy desktop-agent
notifications (`system/server/buddy/delivery/http/handler_ws.go:90-130`), environment worker
(`system/server/server.go:604-610`, `system/environment/service.go:102-134`).

Gate sequence, in code order (all `handler.go` unless stated):

1. Body limit 29 MiB; JSON + `validator` (type, message required); attachments ≤4, ≤10 MiB each, ≤20 MiB total (199-213; `attachment_limits.go:10-15`).
2. `environment.update`: 403 if `environment` capability undeclared; `dropped_disabled` if `environment.enabled=false` (217-224).
3. `voice_listening` → `voiceActiveUntil = now+10s`, return; `voice_listening_end` → `now+5s`, return (235-244). Never forwarded.
4. Telemetry task start for voice/chat (247-251).
5. `look.capture` → flow-only, never forwarded (262-268).
6. Monitor push `sensing_input` for every remaining event (279-283) — this is what pauses ambient (§4.1), and it happens **before** any drop.
7. Current-user bookkeeping: `mood.SetCurrentUser(req.CurrentUser)` (not for `speech_emotion.detected`); `presence.leave/away` clears it (287-291).
8. **Harness direct voice route** (293-295 → `system/server/harness_voice.go:133-196`): voice types with a `harness_voice` snapshot while Harness voice mode is on go to the paired Harness agent only; never reach local intent or the main runtime.
9. **Local intent** (299-349): only for `voice`/`voice_command`/`voice_followup`/`web_chat`/`mqtt_chat`, no images/files, `local_intent` enabled (default true, `config/config.go:628-634`), and `!deferContextualIntent` (`intent_context.go:20-44`). Matched → executes HAL actions, speaks reply via `hal.SpeakCachedForTurn` **only for voice types** (309-319), returns `handler:"local"`. Runs before sleep/readiness checks, so it works while the agent is down. Details in §2.
10. `voice_agent_handled` (realtime already answered): `onRealtimeHandled()` = `AgentHandler.CancelSpeechForNewerTurn` (`server.go:176`), then **persisted to the external-history journal and returned** (357-367; `realtime_history.go:20-43`). Because `SetRealtimeHistory` is always wired at Serve (`system/server/external_history.go:14-20`), the later `voice_agent_handled` branches in PostEvent (busy queue, `MarkSilentRun` at 627-629) are not reached in production.
11. **Sleep drop** (368-379): drops if passive AND not `voice` AND not chat AND not `presence.enter` AND not `fire_hazard.detected` AND `isSleeping()`. `isPassive = !(voice_command||voice_followup)`. `isSleeping` = `AgentHandler.IsSleeping` → HAL `GET /emotion/status` each call; falls back to `lastEmotion=="sleepy"` if HAL unreachable (`agent/delivery/http/handler.go:202-221`; wiring `wire_helpers.go:15-18`, `wire_gen.go:58-59`). So while asleep: motion, sound, light.level, presence.leave/away, motion.activity, emotion/speech_emotion, touch.head_pat, buddy.agent.*, environment.update are dropped; ambient `voice` is **not** dropped.
12. **Ambient turn floor** (383-397): types in `ambientFloorTypes` = `environment.update, motion.activity, emotion.detected, speech_emotion.detected, sound, presence.away, light.level` (1361-1369) are dropped if `now - lastAgentTurn < sensing_turn_floor_s` (default 120 s, `config.go:680-687`). Guard mode bypasses the floor except for `environment.update`. `lastAgentTurn` is stamped on every send **and every queue** from this handler, of any type (561, 753) — a user voice turn silences ambient types for 120 s.
13. **Voice wake**: `voice_command`/`voice_followup` while asleep on an `expression` body → async `hal.SetEmotion("greeting", 0.8)` (402-411). Not fired when a local intent already answered (step 9 returns earlier).
14. Chat images saved to `/tmp/web-chat-<ms>-<i>.jpg`, `[image: path]` appended; non-image files saved and `[file: path (name)]` appended (414-447).
15. **Describe-first gate**: if images and main model not vision-capable, each image described concurrently by the vision model; text replaces the image; images dropped (451-514).
16. **Busy / speaker fork** (518-581): see §5.3 for exact `shouldQueueEvent`. Applies only to passive inputs (everything except `voice_command`/`voice_followup`). Skipped for chat/voice/realtime-handled inputs when the runtime reports active-turn steering (522-524). `speakerBusy` = agent idle AND type waits for speaker AND HAL `GET /voice/status` `tts_speaking` (520-521; `lib/speakergate/speakergate.go:28-36`; `lib/hal/client.go:742-759`).
17. Guard flag: passive `presence.enter`/`motion`/`fire_hazard.detected` with `guard_mode` on (583).
18. **Not ready** (588-611): 503; `voice_command`/`voice_followup`/`presence.enter` speak cached "brain restarting" phrase, at most once per 60 s.
19. Run-id allocation (613-619), speech policy registration (`hal.RegisterTurnSpeechPolicy(runID, WaitsForSpeaker(type))`, 615), run flags (620-645), message build (650-676), voice follow-up/filler marks (680-694), send (725-737).

Voice `voice_command`/`voice_followup` never pass through the busy fork: they are sent to the runtime even mid-turn (524 requires `isPassive`). Runtime-side behaviour of that concurrent send is adapter territory (unverified here).

### 1.1 Message formatting: `sensingmsg.Build`

`system/lib/sensingmsg/sensingmsg.go:59-139`. Every result ends with a `[via:<source>]` line (`TurnSource` 22-42; `domain/via.go:21-32`), except slash commands.

| Type | Body |
|---|---|
| `voice_command`, `voice_followup` | `[user] <msg>` + enroll nudge (66) |
| `voice` (ambient STT) | `[user] [ambient] <msg>` + enroll nudge (67-69) |
| `web_chat`, `mqtt_chat` | `[user] <msg>`; slash commands verbatim (70-76) |
| any type with guard tag (not env) | `[sensing:<type>][guard-active][guard-instruction: …] <msg>` (78-80; handler.go:656-662) |
| `environment.update` | `[environment:update] <json>` + "Advisory sensor context… NO_REPLY when no useful, timely action is warranted." (85-86, 102-103) |
| `motion.activity` | `[activity] <msg>` with `[context: current_user=…]` inserted after first line + `[user_info: …]` + `[wellbeing_context: …]` (87-88, 119-129) |
| `emotion.detected` / `speech_emotion.detected` | `[emotion]` / `[speech_emotion]` + current_user + `[user_info]` + `[emotion_context]` (89-92, 130-133) |
| `presence.enter` | `[sensing:presence.enter]` + current_user + `[presence_context]` only if a *new* friend; or inline "A stranger joined X… speak to X" rule if an already-present friend (104-113) |
| `presence.leave` / `presence.away` | `[sensing:…]` + "[No crons to cancel. NO_REPLY unless worth saying.]" (114-115) |
| `touch.head_pat` | `[sensing:touch.head_pat]` + "[NO_REPLY unless worth saying — phrase already spoken locally.]" (116-118) |
| everything else (motion, sound, light.level, fire_hazard.detected, buddy.agent.*) | `[sensing:<type>] <msg>` (93-94) |

Non-user events also get `[context: current_language=<lang>]` (137; `lib/i18n/lang.go:41-46`). Enroll nudge: only for `Unknown Speaker:` with saved audio, once per 5 min (`domain/voice.go:9-33`).
After Build, os-server strips `[snapshot: …]`, `[pose_bucket: …]`, `[pose_worst: …]` (665-669). Voice and chat then get Harness routing context: `[harness-reply run_id=… channel=voice|web]` + connected/disconnected system-context + optional routing directives + up to 6000 runes of the last Harness result (`harness_context.go:11-29`; `handler.go:57-95`).

Skill-context blocks are built synchronously at message time with HAL HTTP calls: `/user/info` (600 ms timeout, `skillcontext/user.go:15-54`), `/audio/status` and `/audio/history` (800 ms each, `skillcontext/emotion.go:39-40,192-250`), plus local JSONL reads (`skillcontext/wellbeing.go:98-157`, `presence.go:25-48`).

### 1.2 Run flags set at intake (handler.go)

| Flag | When | Effect at reply time |
|---|---|---|
| `MarkGuardRun(runID, snapshot)` | guardActive (620-623) | reply broadcast to Telegram even on NO_REPLY (`handler_event_agent.go:789-800`) |
| `MarkSilentRun` | `voice_agent_handled` (627-629; unreachable, see 1.0 step 10); external-history sync (`externalhistory/sync.go:95`) | no first-sentence stream, no TTS |
| `MarkPoseBucketRun` | `motion.activity` with `[pose_bucket:]` (630-642); also logs a posture alert row | worst pose frames attached to a `/dm` reply (`handler_event_agent.go:931-952`) |
| `MarkWebChatRun` | `web_chat`/`mqtt_chat` (643-645; queued 538-540) | TTS suppressed; text to chat UI / MQTT |
| `MarkBroadcastRun` | legacy `[music-proactive]` cron text (`handler_event_agent.go:194-198`) | reply fanned out to channels |
| `RegisterTurnSpeechPolicy(passive)` | every send (615) and replay | HAL `passive_sensing` flag on speech (`lib/hal/client.go:259-271`) |
| `StartVoiceFollowup` | voice types (681) | HAL follow-up admission tracker (`lib/hal/followup.go:63-108`) |
| Filler marks | voice types (682-693) | fillers are paused (§3.6) |

### 1.3 Intake paths (one row each)

| # | Path | Source → entry | Gates | Formatting / flags |
|---|---|---|---|---|
| 1 | Wake-word voice (`voice_command`), focus-window follow-up (`voice_followup`) | HAL → sensing funnel | Harness route → local intent → (no sleep drop) → (no floor) → wake greeting if asleep → (no busy fork) → readiness | `[user] …`, voice filler marks, follow-up tracking |
| 2 | Ambient STT (`voice`) | HAL → funnel; also MQTT `chat.send` with `speak:true` is re-typed `voice` (`chat_send_handler.go:84-87`) | Harness route → local intent (with spoken reply) → not sleep-dropped → no floor → busy: **queued** | `[user] [ambient] …` |
| 3 | Realtime delegation `[voice-instruction] …` | HAL delivers as a voice type | same as 1/2; `realtimeAlreadySpoke` marks delegated filler (53-55, 684-685) | via `voice_handoff` |
| 4 | Realtime answered alone (`voice_agent_handled`) | HAL → funnel → journal | Speech supersede, then `externalhistory` journal; flushed every 2 s when runtime ready and idle (or busy + steering) | `[skills: input-branching][external-context]… Return NO_REPLY. [HANDLED]… [REPLY]…`, `MarkSilentRun` (`externalhistory/sync.go:22-32,54-115`) |
| 5 | Passive sensing (motion, sound, light.level, presence.*, motion.activity, emotion, speech_emotion, touch.head_pat, fire_hazard.detected) | HAL → funnel | sleep drop (except presence.enter, fire_hazard) → floor (ambient types) → busy queue/drop → guard → readiness | §1.1 table |
| 6 | Web monitor chat (`web_chat`) | web UI → funnel | local intent (no TTS) → busy **queued** with pre-allocated run id → readiness | `[user] …`, `MarkWebChatRun`; images saved/described |
| 7 | Phone chat (`mqtt_chat`) | MQTT `chat.send` → loopback POST (`chat_send_handler.go:23-139`) | same as 6 | same; local reply synthesised as `chat_response` final for MQTT stream |
| 8 | Telegram (OpenClaw) | gateway owns I/O; os-server observes `session.message` | not gated by os-server; device turns interleaved by Telegram are re-routed to DM | runID `tg-<msgID>`; TTS never; HW markers fired (`handler_event_session_message.go:17-278`) |
| 9 | Telegram/Slack/Discord (Hermes, lamp default) | Hermes gateway owns I/O; observer hook `POST /api/agent/channel-turn` | not gated | HW markers fired from `agent:end` response truncated to ~500 chars (`handler_channel_turn.go:99-203`) |
| 10 | Slack relay (Hermes) | MQTT slack event → Hermes `HandleInboundSlack` → `SendChatMessageWithRun` directly | bypasses sensing gates | `[via:slack]`; TTS suppressed (`runtimes/hermes/slack.go:81-125`; `handler_event_agent.go:906-909`) |
| 11 | Schedules | `schedule.Runner` every 1 min | defer if `IsBusy`; skip if connector missing; overdue >30 min re-anchored, not fired | agent kind: `SendSystemChatMessage(instructions + [via:schedule])`; speak kind: verbatim `gw.Speak` (`schedule/runner.go:110-136,246-273`) |
| 12 | Boot greeting | `handleSetUpCompleteChange(true)` startup goroutine | agent ready-stable 15 s within 120 s; setup finished; not sleeping (`expression` bodies) | `[system] You just woke up. Greet the user briefly…` + language/runtime/device/capability context + `[via:system]` (+ optional `[environment:initial]` snapshot on Pro); then `hal.GrantWakeFocus("boot_greeting")` (`config_watch.go:326-346`; `sysprompts.go:13-45`; `environment_greeting.go:13-59`) |
| 13 | Rename notice | MQTT `device.rename` | none | `[system] … renamed you to "X"…` + `[via:system]` (`device_rename_handler.go:14-60`) |
| 14 | Skill-update notice | runtime skill watcher | none (runtime) | `[system] The following skills have been updated… output exactly NO_REPLY…` (`system/skills/skill_update_prompt.go:10-17`) |
| 15 | Environment updates (Pro only) | `environment.Service` polls HAL every 10 s → funnel `environment.update` | capability/enabled → sleep drop → floor (even in guard) → busy queue → replay re-check | `[environment:update] {json}` + advisory/NO_REPLY guidance (§4.5) |
| 16 | Buddy desktop-agent status | paired Mac WS `agent_event` (completed/needs_input/error) → funnel `buddy.agent.<session>` | sleep drop → busy **queued** (prefix rule) | `[sensing:buddy.agent.X] [agent-management] A desktop agent changed status. Report briefly…` + untrusted JSON (`handler_ws.go:90-130`; `buddy/agent_events.go:24-56`) |
| 17 | Harness direct voice | funnel step 8 | Harness voice mode on; loopback-only | goes to paired Harness agent, not the main runtime; result spoken via HAL announcer (`harness_voice.go:133-209`; `agent/delivery/http/handler_harness.go:158-198`) |
| 18 | Harness results/progress for main-agent-delegated tasks | Harness transport events | 15-min reply window | final spoken via `hal.AnnounceHarnessUpdate`; progress shown, not spoken (`server/harness.go:352-498`) |
| 19 | OpenClaw cron / heartbeat | gateway-internal turns | not gated by os-server | cron: WS `cron started` + UUID lifecycle within 10 s → forced TTS; heartbeat: text containing `HEARTBEAT_OK` → never spoken (`handler_events.go:43-65`; `handler_event_agent.go:120-143,803-811,880-897`). Hermes cron/heartbeat handling is adapter territory (unverified). |
| 20 | Guard manual alert | `POST /api/guard/alert` | admin/loopback | `Broadcast` to channels, no agent turn (`handler.go:857-882`) |

Not an intake: `harness.agent.*` prefix in `shouldQueueEvent` (1374) has no producer anywhere in the repo (grep: only this line).

---

## 2. Local intent layer (no LLM)

Entry: `SensingHandler.matchVoiceIntent` → `intent.MatchWithFallback` (`sensing/delivery/http/intent.go:13-22`; `system/intent/semantic.go:18-42`).

### 2.1 Pre-gate: `deferContextualIntent` (`sensing/delivery/http/intent_context.go:20-44`)
Defers to the main agent (no local match attempted) when the normalised text is:
- empty (including malformed `[voice-instruction]`/`[transcript]` envelopes, or any text containing `[snapshot:`, `[vision-image]`, `[realtime-handoff]`, `[realtime-context]`; `intent/jev/catalog.go:107-136`);
- exactly one of `brighter, dimmer, darker, louder, quieter, warmer, cooler, energize, max brightness, stop, continue, yes, no, do it, try again` (32-34);
- contains a relative word (`brighter|dimmer|darker|louder|quieter|warmer|cooler`) without a physical target (`lamp|light|speaker|volume|servo|your camera|đèn|âm lượng|loa`) (35-37);
- a Harness task is pending or the 2-min follow-up window is open, unless the text names a physical target and no digital target (38-43).

### 2.2 Fast path: exact canonical strings (`intent/fast_path.go:10-81`)
Text is normalised (strip `[user]`/`[ambient]`, speaker decorations, trailing `.!?`, lowercase, `please`). If chitchat is enabled (it is not on the lamp, realtime on) → `matchChitchat` (≤5 words, no command words; replies + `/emotion` happy/idle, `intent/chitchat.go:30-83`). Then only these exact strings execute: `turn on the light`, `light on`, `turn off the light`, `light off`, `lights on/off`, `dim the light`, `dimmer`, `dim light`, `brighten the light`, `brighten light`, `volume up`, `louder`, `volume down`, `quieter`, `mute/unmute (the) speaker`, `stop (the) music`, `music off`, `stop playing`, `stop talking`, `ok stop`, `stop tracking|following|watching|track`, `what time (is it)`, `what's the time`, `goodnight`, `good night`, `brighter`, `energize`, `<scene> mode|light`, `<off-verb> (the) (<scene>) mode|scene`, `set the light|turn on the light|light on|set color|change color <color>`, `track|follow (the) <target>` (34-80).
Consequence: `dimmer`, `louder`, `quieter`, `brighter`, `energize` are in the canonical list but `deferContextualIntent` defers them first (2.1), so in production they never execute locally.

### 2.3 Rules executed (`intent/rules_*.go`, order `intent.go:47`)
| Rule | Capability | HAL calls | Spoken reply |
|---|---|---|---|
| `led_color` | light | `/led/effect/stop`, `/led/solid` RGB from 10 colours | "`<Color>` light on!" (`rules_led.go:57-74`) |
| `led_on` | light | `/led/solid [255,220,180]` + `/emotion happy 0.6` | "Light on!" (76-85) |
| `led_off` | light | `/led/off` (no emotion) | "Light off!" (86-95) |
| `brighten` | light | read `/led/color`, peak +25% (≥1), from off `[32,27,20]`; readback verified | "Brighter now." / failure text (97-202) |
| `dim` | light | read colour, halve channels, readback verified | "Dimmed." (104-152) |
| `scene_off` / `scene_<reading|focus|relax|movie|night|energize>` | light | `/scene/off` or `/scene {"scene":…}`; night adds `/emotion sleepy 0.4` | "Back to normal!", "Reading mode!" … "Goodnight!" (`rules_scene.go:52-107`) |
| `volume_up` / `volume_down` | audio | read volume; up +10% of ceiling; down halve; readback verified | "Volume up!/down!" (`rules_audio.go:15-70`) |
| `unmute_speaker` / `mute_speaker` | media | `/speaker/unmute` / `/speaker/mute` | "Speaker on!" / silent (72-90) |
| `music_stop` | media | `/audio/stop` | "Music stopped." (92-100) |
| `stop_talking` | audio | `/tts/stop` | silent (102-110) |
| `what_time` | none | none | "It's 3:04 PM." (`rules_misc.go:8-18`) |
| `servo_track_stop` / `servo_track` | motion | `/servo/track/stop` / `/servo/track {"target":[label]}` | "Stopped tracking." / "Tracking X." (`rules_tracking.go:10-40`) |
Light rules first check HAL sleep (`GET /emotion/status`) and refuse with an honest message if asleep or unknown (`intent.go:82-91`). Any failed HAL call replaces the reply with an honest failure (93-101). Dim/brighten and volume use `TryLock` mutexes: a concurrent adjustment returns "another adjustment is in progress" (`rules_led.go:113-119`, `rules_audio.go:13-18`).
The looser substring `rules` (`anyOf`) only ever see canonical strings or Jev-generated text: `intent.Match` (loose) has no production caller (grep: only `MatchWithFallback` is called, `sensing/delivery/http/intent.go:21`).

### 2.4 Jev semantic classifier (`intent/semantic.go`, `intent/jev/*`)
- Enabled by default; disabled only by `jev_intent.enabled=false` or `local_intent=false`. Endpoint `<llm_base_url>/jev/decisions`, bearer `llm_api_key`, model `typesafe/jev-1.13` (`config/jev_intent.go:24-46`; `jev/client.go:20`).
- Budget default 3000 ms, max 3000 ms (`jev/resolver.go:13-14`). Runs **synchronously inside the HTTP handler** before the event reaches the agent.
- Skips: disabled/missing config, input >2000 bytes, ASCII text with no catalogue keyword stem (`MayBeDeviceCommand`, `catalog.go:69-100`), no candidates, 30 s cooldown after any error/timeout, concurrent call already running (no queue) (`resolver.go:43-91,107-112`).
- 21 candidates (`catalog.go:35-64`) filtered to those whose capability is positively declared (`semantic.go:44-61`).
- Acceptance: chosen probability ≥0.90, margin over runner-up ≥0.40, independent fit ≥0.95; each parameter ≥0.90 and margin ≥0.40; probabilities must sum to 1±0.02 (`jev/client.go:262-306`).
- Accepted selection maps to code-owned text (`set the light <color>`, `track <label>`, or no parameters) and runs the same rule executor; raw speech never reaches HAL (`semantic.go:63-112`). Result tagged `Source:"jev"`.
- What the layer can do without the LLM: the 21 actions above plus chitchat (when realtime is off). Nothing else.

---

## 3. Reply out

### 3.1 Streaming path (`agent/delivery/http/handler_event_agent.go:615-689`, `handler_state.go`)
1. Every assistant delta: fillers suspended (`OnAssistantText`), delta buffered per raw run id (`handler_state.go:10-29`).
2. `canStreamSentenceTTS` false for `tg-` runs, web chat, silent, Slack, confirmed channel runs, or runs with a suppress reason (`handler_text.go:146-172`).
3. `tryFirstSentenceFlush` (`handler_state.go:44-100`): only once per run; defers on a partial `[HW:` or `](HW:` marker, any `<say>`, or `NO_REPLY`/`HEARTBEAT_OK` anywhere in the buffer; strips markers and `[image … removed]`; boundary = **rightmost** `.`/`?`/`!` followed by whitespace (decimal-safe) (`216-239`) — so "first sentence" is everything up to the last complete boundary present when checked; CoT-leak sentences dropped; silence narration defers.
4. Leading `[HW:]` markers (before any text) fire synchronously, 100 ms budget, before the TTS post; their count is recorded so lifecycle end skips them (663-676; `handler_hw.go:117-142,293-310`).
5. Filler cancelled; text sent via `deliverTTSQueue` → `SendToHALTTSQueueForTurn(text, runID, turnSeq)` → HAL `/voice/speak-queue` with `realtime_feedback`, `turn_id`, `turn_seq`, `passive_sensing` (`handler_tts.go:276-285`; `lib/hal/client.go:259-271`). Turn seq assigned on lifecycle start, monotonic for server lifetime (`handler_tts.go:44-60`; `handler_event_agent.go:96-98`).
6. Tool start with buffered text: the text is narration, demoted to the thinking row and removed from the reply, unless it holds an HW marker or the first sentence already streamed (`handler_event_agent.go:507-514`; `handler_state.go:248-267`).

### 3.2 Lifecycle end (`handler_event_agent.go:694-977`)
Order of decisions:
1. Cancel filler; consume suppress reason (`already_spoken` from built-in `tts` tool, 552-566), interleaved Telegram DM target, web-chat flag (`web_chat`), silent flag (`voice_agent_handled`) (720-735).
2. `flushAssistantText` → text + all `[HW:]` calls (`handler_state.go:271-284`). Skip calls already fired at stream time (745-749).
3. If text or calls or streamed: **fire remaining HW calls synchronously (100 ms budget, then async in order)** (750-753). This happens before the NO_REPLY check and regardless of web-chat/silent/channel status.
4. Control markers: `/broadcast` → broadcast run, `/speak` → force TTS, `/dm {"telegram_id"}` → DM target (762-780); else interleaved Telegram chat id (782-786).
5. Guard run → Telegram broadcast of text (or fixed "Motion or presence detected while guard mode is active." on NO_REPLY/empty) with snapshot (789-800).
6. Heartbeat detection (`HEARTBEAT_OK`) (803-811); `<say>…</say>` unwrap, empty `<say>` = NO_REPLY (`handler_text.go:83-98`); trailing `NO_REPLY`/`HEARTBEAT_OK` stripped (`handler_text.go:69-80`); remainder = text after streamed prefix; CoT-leak filter (`cot_leak_filter.go`) (813-840).
7. Harness owns the reply → return, resume Harness voice fillers (841-846).
8. `isAgentNoReply` (trimmed upper text == `NO` or starts with `NO_`) or `isMetaNonReply` (≤100 bytes, no `?`, matches phrases like "nothing to say", "no reply needed", "staying silent") → no TTS, `[no reply]` monitor row (847-862; `handler_text.go:35-66`). If sentence 1 already streamed, it cannot be unspoken (849-852).
9. Remainder empty → done (streamed-only or HW-only) (863-871).
10. Suppress reason → no TTS (872-875).
11. Channel classification: `tg-` prefix → channel; cron-fire → not channel; `/broadcast` or `/speak` → not channel; heartbeat → channel; confirmed channel (sender label) → channel; Slack origin → channel (876-909). Channel → no TTS; else `deliverTTSQueue(remainder)` (910-929).
12. `/dm` (priority) or broadcast with `len(text) > 10` → Telegram (`SendToUser[WithMedia]`) / all channels (931-961). Slack finalize (965-975).

`deliverTTSUnless` (`handler_tts.go:203-264`): drops speech for runs created at/before the click watermark or the realtime-supersede watermark (still feeds the text to realtime history); replaces LLM usage-limit banners with a cached "[sigh] I've hit my usage limit." at most once per 30 s; registers follow-up admission; HAL errors `ErrCaptureActive` (automatic capture in progress) and `ErrSpeakerMuted` are logged as cancelled/muted.

Built-in `tts` tool calls are intercepted and routed to HAL (`SendToHALTTS` → `/voice/speak` with realtime feedback, or speak-queue for passive turns), then the lifecycle-end flush is suppressed (`handler_event_agent.go:552-566`; `handler_tts.go:267-273`).

Errored turns: `tryRecoverIncompleteTurn` salvages buffered text (or chat.history trailing assistant text) and speaks it, firing only stream-buffer markers, never history markers (`handler_error_recovery.go:22-136`).

Chat `final` events never speak ("TTS comes from the lifecycle_end path", `handler_event_chat.go:164`); empty or slash finals with no lifecycle release the busy flag (89-144).

### 3.3 `[HW:…]` markers: parse and execute (`handler_hw.go`)
- Canonical form `[HW:/path:{json}]` or bodyless `[HW:/path]`; colon-separated paths (`/audio:play`) normalised to slashes. Body must be a flat `{…}` with no `}` inside (no nested objects) (`hwMarkerRe`, 55). Markdown-link form `[label](HW:/path:{…})` normalised to canonical, marker placed before label (60-81).
- Extraction strips markers from spoken text (89-113).
- Markers echoed inside a tool call's args (`echo '[HW:…]'`) are also fired, async (262-288; also `handler_event_session_tool.go`).
- Sources that fire markers: device turns (stream + end), Telegram/OpenClaw channel finals (`handler_event_session_message.go:212-228`), Hermes channel hooks (`handler_channel_turn.go:155-161`), recovered error turns.

### 3.4 OS-side special paths
| Path | Handling |
|---|---|
| `/speak` | control only: forces TTS even for channel-looking runs (`handler_event_agent.go:769-771,889-893`); `fireHWCall` returns true without HTTP (148-150) |
| `/broadcast` | control only: reply fanned out to all channels if `len(text)>10` (766-768, 953-961) |
| `/dm` | control only: `{"telegram_id"}` → Telegram DM with optional pose images (772-779, 931-952) |
| `/wellbeing/`, `/mood/`, `/music-suggestion/`, `/posture/`, `/buddy/` | POSTed to os-server `http://127.0.0.1:5000/api<path>` instead of HAL (166-173) → `PostWellbeingLog`, `PostMoodLog`, `PostMusicSuggestionLog/Status`, `PostPostureLog` (`sensing/delivery/http/handler.go:1011-1149,1310-1357`), Buddy `command/observe/suggest/exec` (`server.go:482-495`) |
| everything else | POST `http://127.0.0.1:5001<path>` (HAL) |

### 3.5 Validation between model marker and hardware call
Exactly these, in `fireHWCall` (`handler_hw.go:146-258`):
1. Regex shape only (flat body, no nesting); the body is **not** parsed or schema-validated by os-server; no path allowlist (any HAL path can be POSTed).
2. Click watermark: markers of runs created at/before the last physical cancel gesture are dropped (153-158; `handler_tts.go:108-112`). The realtime-supersede watermark does **not** drop markers.
3. Capability gate: `device.RouteCapability(path)` must be declared (160-164). Mapped prefixes only: `/emotion`→expression, `/scene`,`/led`→light, `/servo`→motion, `/policy`, `/display`, `/music`→media; everything else ungated, fail-open (`system/device/devicemd.go:91-109`). Undeclared capability list = fail-open (112-119).
4. 5 s HTTP timeout per call; failures logged; `/servo/track` 4xx/5xx speaks an apology via `hal.Speak` (194-199).
No rate limit, no clamping, no safety bounds in os-server. Brightness clamping lives in HAL's safety engine (`docs/safety.md`; HAL area, unverified here). The agent can also call HAL directly with shell tools (curl to 127.0.0.1:5001); os-server only observes those for the monitor (`handler_event_agent.go:498-550`).

### 3.6 Fillers and cues (`sensing/delivery/http/deadair_filler.go`)
- Opening filler: **disabled** by `const openingFillerPaused = true` (227-230).
- Continuation (dead-air) filler: **disabled** by `const continuationFillerPaused = true` in `fire` (722-727). Timers still arm (3.5 s delay, 2.5 s cooldown, cap 2 per turn, 39-51), but fire returns.
- Still live: HAL-requested realtime filler and named-pool filler via `POST /api/sensing/filler` (247-295); `SayInVoiceRun` cues used by `POST /api/vision/look` ("Taking a look." / "Got it — give me a sec.") (627-657; `system/server/vision.go:63-112`). `SayInVoiceRun` targets the first active voice run in a map (iteration order), not necessarily the turn that called look (605-619).
- Filler prewarm renders every pool phrase and local-intent reply into HAL's WAV cache after boot (159-221).

---

## 4. Autonomous loops (no user prompt)

### 4.1 Ambient (`system/ambient/service.go`)
- Starts after the boot greeting (`config_watch.go:351`); returns immediately unless `lifelike` declared (57-64). Sleeps 5 s, then resumes (73-74).
- Pause triggers (monitor bus): `sensing_input` (except `environment.update`), `chat_response`, `intent_match`, `tts`, `chat_send` → pause + clear ambient sleep flag; `hw_emotion` → pause, and if summary contains `"sleepy"` set ambient sleeping (155-192). Resume after 60 s of no such events, checked every 2 s, only if not sleeping (22-23, 193-201).
- `isPaused` also asks HAL `GET /emotion/status`; HAL unreachable counts as paused (124-136).
- `restingLEDLoop` (2 s tick): when unpaused and LED not locked, calls `hal.RestoreLED()` once per resume (259-285). LED lock set by `led_set` monitor events (agent `/led`/`/scene` markers or tool calls, agent `/emotion` tool calls, local intent LED changes) and by the web hardware proxy; cleared only by `led_off` events, `/scene/off` or `/led/off` via proxy, or `PUT /led/resting` (182-191; `system/server/proxy.go:36-60`). No time-based unlock.
- `breathingLoop`: commented out (83, 205-255).
- `microMovementLoop`: every 45–120 s (uniform random) plays one of `idle`, `curious`, `nod` via `hal.PlayServo` if not paused (287-306).
- `mumbleLoop` (self-talk every 5–15 min): **disabled** by `const mumblePaused = true` (309-314).
- The ROBOT.md comment still advertises "breathing LED, servo micro-movements, TTS self-talk" (`robots/lamp/ROBOT.md:34-37`); only micro-movements and resting-LED restore run.

### 4.2 Healthwatch (`system/healthwatch/service.go`)
- Starts after boot greeting (`config_watch.go:352`). Polls HAL `/health` every 5 s (21).
- HAL unreachable → status LED `hal_down` (67-74). On recovery, shows `hal_down` for 3 s then clears (81-89); if HAL was down and now voice and TTS are healthy → speaks a cached recovery phrase (e.g. "[sigh] Mm, back.") via `hal.SpeakCached` (114-120, 178-185). Not gated by sleep, agent busy, or speaker state in os-server.
- Hardware check: servo/LED/audio/voice per declared capability → status LED `hardware` set/cleared (91-112).
- Sensing false for ≥2 consecutive polls after voice was confirmed running → restart HAL voice pipeline, 30 s cooldown (20-24, 123-172, 189-215).

### 4.3 Status LED priorities (`system/statusled/service.go`)
Priority: connectivity 8 > error 7 > ota 6 > wifi_connecting 5 > booting 4 > hal_down 3 > agent_down 2 > hardware 1 (30-39). Highest active state applied via HAL `/led/status` overlay; when none active → `hal.RestoreLED()` (58-106). `FlashReady` after the agent is stable at boot (108-129; `config_watch.go:245-247`). Setters: booting (`server.go:254,586`), connectivity (network monitor every 5 s, `config_watch.go:182-185`; `network/service.go:29`), wifi_connecting (`device/setup.go`), hal_down/hardware (healthwatch), agent_down (each runtime adapter). No `Set(StateOTA)` or `Set(StateError)` call exists in `system/` or `runtimes/`.

### 4.4 Schedules (`system/schedule/runner.go`)
- Started with setup-complete monitor context (`config_watch.go:196`). First tick immediate, then every 1 min (14, 84-96).
- Per due schedule: overdue >30 min → re-anchor, not fired (18, 123-126); missing required connector → `skipped` (128-134, 158-177); `gw.IsBusy()` → defer to next tick, NextRunAt unchanged (129-132); else fire.
- `speak` kind → `gw.Speak(instructions)` verbatim (HAL `/voice/speak`), no agent turn; `agent` kind → `SendSystemChatMessage(instructions + [via:schedule])` → a `device-chat-*` run, spoken at lifecycle end like any device turn (246-273).
- Jitter up to 5 min per device for daily/weekly/monthly; interval floor 5 min (`schedule/spec.go:24,210-226`).
- Not paused by sleep, ambient state or speaker state in os-server.
- Schedules come only from backend `schedule.sync`; local UI edits are proposals the backend must confirm (`server.go:543-551`).

### 4.5 Environment (Pro bodies only)
- Worker runs from Serve, re-reads settings each tick (`server.go:604-610`; `environment/service.go:27-98`). Defaults: enabled, initial report on, evaluate every 10 s, sustain 60 s, cooldown 1800 s, retry 60 s, max sample age 10 s (`config/environment_settings.go:66-83`).
- Change events: per-metric delta ≥ max(delta, |baseline|×pct) sustained in one direction ≥60 s after warmup; comfort bands (PM2.5 >35, temp <19 or >27 °C, humidity <35 or >65 %, CO2 >1000 ppm) sustained 300 s with hysteresis; global cooldown 30 min after an accepted event; retry ≥60 s after any attempt (`environment/detector.go:79-114,163-256`; thresholds `environment_settings.go:71-81`).
- "Accepted" = runId returned or `handler:"queued"`; floor/sleep drops are not accepted and are retried after 60 s (`environment/service.go:102-133`).
- Initial snapshot: offered to the boot greeting first; if not consumed, sent once as an `environment.update` with `reason:"initial"` (`environment/startup.go:25-141`; `environment_greeting.go:13-27`).

### 4.6 Boot greeting
Once per os-server start with setup complete (and after a setup), after reconcile/onboarding, agent ready-stable 15 s, HAL ready, volume set; skipped if sleeping or sleep state unreadable on `expression` bodies (`config_watch.go:200-346`; `environment_greeting.go:46-59`). It is an LLM turn (`SendSystemChatMessage`), spoken at lifecycle end, then opens HAL's wake-focus window.

### 4.7 Session rotation (autonomous memory event)
After each lifecycle end with token usage: Hermes rotates the conversation after 40 turns or 250k tokens; OpenClaw after 150k tokens; 30 s cooldown (`handler_session_lifecycle.go:19,65-94`; `runtimes/hermes/rotation.go:12-13,47-49`; `runtimes/openclaw/service_chat.go:296-301`).

---

## 5. Arbitration

### 5.1 Who writes what (os-server side)
| Actuator | Writers in os-server | Conflict mechanism |
|---|---|---|
| LED | agent HW markers; agent direct tool calls (observed only); local intent; statusled overlays; ambient `RestoreLED`; boot/setup status; voice-wake `/emotion greeting`; web hardware proxy; FlashReady | ambient `ledLocked` flag driven by monitor events (`ambient/service.go:182-191`); statusled priority + HAL "transient overlay" + `RestoreLED` (`statusled/service.go`); no cross-writer lock in os-server. Final arbitration is HAL (unverified here). |
| Motion | ambient micro-movements; agent markers/tools; local intent track/stop; look hold claim/release (`vision.go:66-104`); voice-wake greeting emotion | ambient pauses on interaction events; HAL hold/sleep gates (HAL area, unverified) |
| Speaker | agent reply (speak-queue, turn-ordered); intercepted `tts` tool; local intent reply; realtime/pool fillers on HAL request; look cues; brain-restart notice; LLM-limit notice; healthwatch recovery; track-fail apology; schedule `speak`; Harness results/notices; external-history (silent) | HAL `turn_seq` (highest wins, `lib/hal/client.go:258-271`); click watermark and realtime-supersede watermark (`handler_tts.go:102-188`); HAL `passive_sensing` flag; `ErrCaptureActive` / `ErrSpeakerMuted`; speakergate deferral of passive replays; filler soft/hard cancel; LLM-limit 30 s debounce. Notices from healthwatch, track-fail, brain-restart, schedule-speak use plain `hal.Speak`/`SpeakCached` with no turn id or watermark. |

### 5.2 Busy flag
- Set true on `lifecycle.start` only for device-initiated run ids (`device-chat-*`, `device-sensing-*`); channel/cron/heartbeat UUID runs do not set busy (`handler_event_agent.go:235-245`; `handler_text.go:100-106`).
- Set false on lifecycle `end`/`error`, empty or slash `final` without lifecycle, channel `session.message` final, untracked assistant stop (`handler_event_agent.go:246-260`; `handler_event_chat.go:116,143`; `handler_event_session_message.go:188-194,235`). `POST /api/agent/busy` sets true (admin) (`handler_api_monitor.go:135-138`).
- `IsBusy` also true while a chat.send waits ≤30 s for its lifecycle start (OpenClaw `runtimes/openclaw/service_runs.go:177,195-205`) or while Hermes has in-flight streams (`runtimes/hermes/events.go:51-68`). Busy auto-clears after 5 min (OpenClaw, Hermes) or 45 min (claudecode, picoclaw) (`runtimes/*/events.go`).

### 5.3 `shouldQueueEvent` exactly (`sensing/delivery/http/handler.go:1373-1392`)
Called only when the input is passive and (agent busy and not steerable) or speaker busy:
- prefix `buddy.agent.` or `harness.agent.` → queue;
- `presence.enter`, `presence.leave`, `voice`, `voice_agent_handled`, `motion.activity`, `emotion.detected`, `speech_emotion.detected`, `environment.update`, `fire_hazard.detected`, `web_chat`, `mqtt_chat` → queue;
- `sound` → queue only if message contains `persistent`;
- anything else (`motion`, `light.level`, `presence.away`, `touch.head_pat`, …) → queue only inside the voice window (≤10 s after `voice_listening`, ≤5 s after `voice_listening_end`), else **drop** (`sensing_drop` reason `agent_busy`).
Queued chat/voice get a pre-allocated run id; speaker-busy queues start a speakergate waiter (553-560).

### 5.4 Queue replay (runtime adapters, boundary)
- Trigger: `SetBusy(false)` → drain; speaker-busy waiter (poll 500 ms, max 90 s, one waiter at a time; any exempt type releases the whole batch) (`lib/speakergate/speakergate.go:14-74`).
- OpenClaw drain (`runtimes/openclaw/service_events.go:79-244`): voice first; drop environment if replay no longer allowed (sleeping/capability/disabled, `sensingmsg/environment.go:17-26`); expire after 60 s for env, motion.activity, emotion, speech_emotion, presence.*; keep only the latest of each of those 7 types; send each remaining event as its own turn, re-built with `sensingmsg.Build` using the user snapshot from queue time and **no guard tag**.
- Hermes drain (lamp default, `runtimes/hermes/events.go:102-260`): same filters, then if any standalone event (voice, voice_command, voice_followup, voice_agent_handled, web_chat, mqtt_chat, environment.update, or with images) is present, sends only the first one and restores the rest; otherwise merges all ambient events into one turn under "[ambient signals batched while busy — respond once…]".
- `domain/agent.go:57` documents `QueuePendingEvent` as "last-write-wins per type"; implementations append and coalesce only at drain, only for 7 types.

### 5.5 Steering
If the runtime implements `ActiveTurnSteerer` and reports support, chat/voice/realtime-handled inputs bypass the busy fork and are sent into the running turn (`handler.go:522-524`; `domain/agent.go:281-284`). Hermes reports support only after its server advertises `/runs` control (`runtimes/hermes/steering.go:45-46`); whether the lamp's Hermes advertises it is unverified. Codex always true.

### 5.6 Cancel / supersede
- Physical click → `POST /api/agent/speech/cancel` (loopback): click watermark (speech + HW markers), all fillers cancelled, follow-ups cancelled, HAL `StopTTS` (`handler_api_monitor.go:106-131`; `handler_tts.go:130-146`). Turns keep running.
- `before_ms` variant (HAL, before admitting a new capture) → auto watermark (speech only), not gated by the env flag (`handler_api_monitor.go:114-123`; `handler_tts.go:150-159`).
- `voice_agent_handled` → auto watermark only if `OS_REALTIME_SUPERSEDES_MAIN_REPLY` set (lamp: set) (`handler_tts.go:170-188`).
- `MaxFillersPerTurn`, filler timers: see §3.6.

---

## 6. Decision points

| # | Decision | Rule | Code/model | Default / threshold | Location |
|---|---|---|---|---|---|
| 1 | Accept event | JSON + validator + attachment limits | code | 29 MiB body, 4×10 MiB, 20 MiB total | handler.go:199-213; attachment_limits.go:10-15 |
| 2 | Environment allowed | capability + `environment.enabled` | code | enabled true; capability Pro only | handler.go:217-224 |
| 3 | Voice window | `voice_listening` / `_end` | code | 10 s / 5 s | handler.go:235-244 |
| 4 | Harness-only voice | voice type + snapshot + mode on | code | RAM mode, off after os-server restart (`docs/os-server.md:93-95`) | harness_voice.go:133-153 |
| 5 | Defer contextual text | fragment list / relative word / Harness pending | code | — | intent_context.go:20-44 |
| 6 | Local fast path | exact canonical strings | code | `local_intent` true | fast_path.go:10-81; config.go:628-634 |
| 7 | Chitchat | ≤5 words, no command words | code | off when realtime on (lamp: off) | chitchat.go:30-83; handler.go:185 |
| 8 | Jev semantic | keyword prefilter, probability/margin/fit | model + code | on; 3 s; p≥0.90, margin≥0.40, fit≥0.95; 30 s error cooldown | jev/resolver.go:13-121; jev/client.go:262-306 |
| 9 | Light refuses when asleep | HAL `/emotion/status` | code | — | intent.go:82-91 |
| 10 | Realtime supersede | env flag | code | lamp: on | handler_tts.go:163-188 |
| 11 | Sleep drop | passive non-voice non-chat, not presence.enter/fire | code (HAL flag) | — | handler.go:368-379 |
| 12 | Ambient floor | 7 types, since last agent turn | code | 120 s; guard bypass except env | handler.go:383-397,1361-1369; config.go:680-687 |
| 13 | Voice wake greeting | voice command while asleep | code | `/emotion greeting 0.8` | handler.go:402-411 |
| 14 | Describe-first | images + text-only main model | code + vision model | concurrent per image | handler.go:451-514 |
| 15 | Speaker-busy wait | passive, agent idle, HAL speaking | code | poll 500 ms, max 90 s | handler.go:520-521; speakergate.go:14-74 |
| 16 | Queue vs drop | `shouldQueueEvent` | code | see §5.3 | handler.go:1373-1392 |
| 17 | Guard turn | guard on + presence.enter/motion/fire | code | guard off | handler.go:583; config.go:673-678 |
| 18 | Not-ready notice | voice cmd / presence.enter | code | once per 60 s | handler.go:588-611 |
| 19 | Opening / continuation filler | — | code | **paused** | deadair_filler.go:227-230,722-727 |
| 20 | Busy set | device-initiated lifecycle start | code | — | handler_event_agent.go:235-245 |
| 21 | Busy auto-clear | TTL | code | 5 min (OpenClaw/Hermes) | runtimes/*/events.go |
| 22 | Replay expiry / coalesce / merge | 7 types; Hermes merge | code | 60 s | runtimes/openclaw/service_events.go:116-176; runtimes/hermes/events.go:135-217 |
| 23 | First-sentence stream | eligibility + defer rules + rightmost boundary | code | once per run | handler_text.go:146-172; handler_state.go:44-100,216-239 |
| 24 | Narration demote | text before tool call | code | — | handler_state.go:248-267 |
| 25 | CoT leak drop | trigger/secondary regex + heuristics | code | — | cot_leak_filter.go |
| 26 | NO_REPLY | `NO` or `NO_*` prefix; empty `<say>` | code (model chooses) | — | handler_text.go:35-46,85-98 |
| 27 | Silence narration | ≤100 bytes, no `?`, phrase regex | code | 100 bytes | handler_text.go:49-66 |
| 28 | Speak or not | web chat / silent / channel / Slack / heartbeat / cron / `/speak` / `/broadcast` | code | — | handler_event_agent.go:872-929 |
| 29 | DM / broadcast | `/dm` > `/broadcast`, text >10 bytes | code (model marker) | 10 bytes | handler_event_agent.go:931-961 |
| 30 | HW marker drop | click watermark | code | — | handler_hw.go:153-158 |
| 31 | HW capability gate | RouteCapability declared | code | fail-open unmapped | handler_hw.go:160-164; devicemd.go:91-119 |
| 32 | HW target host | 5 prefixes → os-server | code | — | handler_hw.go:166-173 |
| 33 | Speech drop | click or auto watermark | code | — | handler_tts.go:102-106,203-218 |
| 34 | LLM-limit notice | banner patterns | code | 30 s debounce | handler_tts.go:21-36,219-236 |
| 35 | Ambient pause/resume | monitor events, HAL sleep | code | resume after 60 s | ambient/service.go:22,155-201 |
| 36 | Ambient resting LED | unpaused + not locked | code | 2 s tick | ambient/service.go:259-285 |
| 37 | Micro-movement | random interval | code | 45–120 s; idle/curious/nod | ambient/service.go:287-306 |
| 38 | Mumble | — | code | **paused** | ambient/service.go:309-314 |
| 39 | HAL recovery speech | HAL back, voice+TTS ok | code | 5 s poll | healthwatch/service.go:114-120 |
| 40 | Voice restart | sensing false ≥2 polls | code | 30 s cooldown | healthwatch/service.go:20-24,141-172 |
| 41 | Status LED | priority map | code | §4.3 | statusled/service.go:30-39 |
| 42 | Schedule fire | due, ≤30 min overdue, connectors, not busy | code | 1 min tick; 5 min jitter | schedule/runner.go:110-136 |
| 43 | Env change event | delta + sustain + cooldown + retry | code | 60 s / 1800 s / 60 s | environment/detector.go:163-256 |
| 44 | Boot greeting | ready-stable, not asleep | code | 15 s stability, 120 s wait | config_watch.go:239-346 |
| 45 | Session rotation | turns/tokens | code | Hermes 40 turns / 250k | handler_session_lifecycle.go:65-94 |
| 46 | Telegram interleave → DM | session.message during device turn within 30 s | code | 30 s | handler_event_session_message.go:57-101 |
| 47 | Cron fire → speak (OpenClaw) | cron started + UUID lifecycle within 10 s | code | 10 s | handler_event_agent.go:120-143; system/server/agent/delivery/http/handler.go:158-160 |
| 48 | Harness routing hint | regex on user text | code | — | sensing handler.go:57-85 |
| 49 | Ext. history flush | ready and idle (or steering) | code | 2 s tick | externalhistory/sync.go:54-115 |
| 50 | Enroll nudge | unknown speaker + saved audio | code | 5 min cooldown | domain/voice.go:9-33 |

---

## 7. Configuration keys and lamp values

| Key | Meaning | Code default | Lamp value |
|---|---|---|---|
| `agent_runtime` | brain backend | ROBOT.md `gateway.default`, else openclaw | hermes (`ROBOT.md:9-11`) |
| `local_intent` | local rules + Jev master switch | true (`config.go:628-634`) | default |
| `jev_intent.enabled` / `timeout_ms` | semantic classifier | true / 3000 (cap 3000) (`config/jev_intent.go:24-46`) | default |
| `realtime.enabled` / `provider` | realtime brain; disables chitchat | true / gemini (`config/realtime.go:94-125`) | default unless pinned |
| `sensing_turn_floor_s` | ambient floor | 120 (`config.go:680-687`) | default |
| `guard_mode`, `guard_instruction` | guard | false / "" (`config.go:673-678`) | default |
| `environment.*` | env interpretation | enabled, initial_report, 10/60/1800/60/10 s (`environment_settings.go:66-83`) | default; capability Standard no, Pro yes |
| `wakeword` | wake-word gate | false for existing configs, ROBOT.md for fresh (`config.go:446-451`; `server.go:297-309`) | true for fresh lamp configs |
| `tts_provider` / `tts_voice` | TTS | seeded from ROBOT.md | elevenlabs (`ROBOT.md:12`) |
| `startup_volume` (ROBOT.md) | boot volume | 100 if absent (`devicemd.go:24-25`) | 75 / 35 / 77 by profile |
| `stt_language` | reply language for CoT filter, fillers | "" | not in repo (per device) |
| env `OS_REALTIME_SUPERSEDES_MAIN_REPLY` | auto speech watermark | off | 1 (`rootfs/opt/hal/.env`) |
| constants (not config) | fillers paused, mumble paused, breathing disabled, MaxFillersPerTurn 2, FillerDelay 3.5 s, busyTTL 5 min, replay expiry 60 s, speakergate 90 s | code | same |

---

## 8. Surprising, dead, disabled or contradictory

1. **Paused by const**: opening filler, continuation filler, ambient mumble; breathing loop commented out (`deadair_filler.go:227,722`; `ambient/service.go:83,311`). ROBOT.md still advertises breathing and self-talk (`ROBOT.md:34-37`).
2. **Unreachable branches**: `voice_agent_handled` busy/queue and `MarkSilentRun` in PostEvent (journal path always wired); `harness.agent.*` queue prefix (no producer); `dimmer`/`louder`/`quieter`/`brighter`/`energize` canonical fast-path entries (deferred first); loose `intent.Match` (no production caller); `StateOTA`/`StateError` status LED (never set).
3. **Doc vs code** (code wins):
   - `docs/os-server.md:1070-1085` table: "turn off light" + idle emotion (code: no emotion), "happy"/"sad" emotion rules (none exist), volume up→100 / down→30 (code: +10% of ceiling / halve), "relax" (code requires "relax mode/light").
   - `docs/os-server.md:337` floor list omits `environment.update` and says guard bypasses; code floors `environment.update` even in guard mode.
   - `system/server/config/config.go:240-242` comment says Jev "default off"; code and `docs/os-server.md:1139` say on.
   - `system/domain/agent.go:57` "last-write-wins per type" vs append + drain-time coalescing of 7 types.
   - `docs/os-server.md:333-335` processing flow says "forward to OpenClaw"; lamp runs Hermes.
4. **Sleep does not stop** (in os-server): schedule runs, healthwatch recovery speech, and ambient `voice` turns (not sleep-dropped; can reach the agent while asleep). Only the boot greeting checks sleep. Environment updates are sleep-dropped and retried every 60 s. Whether HAL mutes these while asleep is HAL territory (unverified).
5. **HW markers fire even when the reply is silent**: NO_REPLY, web chat, channel, silent and suppressed runs still fire markers at lifecycle end; only the click watermark and capability gate stop them. No body validation, no path allowlist, no rate limit in os-server.
6. **First-sentence streaming uses the rightmost boundary** in the buffer at check time, not the first sentence.
7. **Floor is shared with user turns**: any agent turn from this handler (including a user voice turn or a queued event) resets the 120 s floor for ambient types.
8. **Jev adds up to 3 s synchronous latency** to non-canonical voice/chat requests that pass the keyword prefilter before the main agent sees them.
9. **Hermes drain** sends at most one standalone event per drain and merges ambient events; OpenClaw sends each event as its own turn. Lamp behaviour follows Hermes.
10. **Telegram via Hermes**: os-server only sees the first ~500 chars of the reply, so trailing markers can be clipped (`handler_channel_turn.go:155-156`).
11. **Ambient LED lock never times out**: after any agent/intent LED set, resting-LED restore stays off until an explicit off/scene-off/resting change.
12. **Ambient pauses on dropped events**: `sensing_input` is pushed before sleep/floor/busy drops, so even dropped sensing pauses micro-movements for 60 s.

Unverified (outside this area): HAL's arbitration of LED overlays, servo holds and speak-queue `turn_seq`/`passive_sensing`; whether HAL sends `voice` (ambient) turns when the wake-word gate is on; whether the lamp's Hermes advertises `/runs` steering; Hermes cron/heartbeat handling.
