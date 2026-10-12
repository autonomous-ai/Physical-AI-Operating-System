# V1 Lamp — HAL voice front-end (study notes)

Source: `v1-main` checkout of `origin/main` at `170a1b85e` (V1 as deployed). All paths are relative to it.
Scope: `hal/drivers/voice/**` plus what it calls in `hal/runtime.py`, `hal/app_state.py`, `hal/config.py`,
`hal/routes/voice.py`, and the boundary with `hal/realtime/**` and os-server. Facts only. "Unverified" marks
anything not provable from code in this checkout.

Abbreviations: `VS` = `hal/drivers/voice/voice_service.py`, `VC` = `hal/drivers/voice/_internal/config.py`,
`HC` = `hal/config.py`, `RT` = `hal/drivers/voice/_internal/realtime_turn.py`,
`TD` = `hal/drivers/voice/_internal/turn_dispatch.py`, `TTS` = `hal/drivers/voice/tts/service.py`,
`AS` = `hal/app_state.py`, `RV` = `hal/routes/voice.py`, `LENV` = `robots/lamp/rootfs/opt/hal/.env`.

---

## 0. What the standard lamp actually runs (effective configuration)

How the env reaches HAL: systemd `EnvironmentFile=/opt/hal/.env` (`scripts/imager/build-orangepi.sh:594`), plus
`load_dotenv` (`hal/server.py:10`, `hal/main.py:22`). `/opt/hal/.env` is `LENV`; a hardware profile named in
`/etc/autonomous/hardware-profile` merges `robots/lamp/overrides/<name>/rootfs/opt/hal/.env` over it
(`scripts/provision/apply-overrides.py:18-41`). No marker / `standard` = base `LENV` only (`docs/bootstrap-ota.md:395-398`).
Which profile fielded units carry is **unverified**.

| Setting | Code default | Standard lamp (LENV) | Source |
|---|---|---|---|
| Input mode | `automatic` (config.json `voice_input_mode`) | automatic (no override) | `HC:578-581` |
| Wake word gate | `False` unless config.json `wakeword` is literally `true` | `true` **only** on a config.json os-server creates fresh (ROBOT.md `voice.wakeword: true`); a pre-existing config without the key stays `false` in memory and on disk | `HC:583`, `robots/lamp/ROBOT.md:15-20`, `system/server/server.go:297-309`, `system/server/config/config.go:446-451` |
| Realtime brain | enabled, provider `gemini` | enabled (os-server re-seeds `realtime` block, `enabled:true, provider:gemini, model gemini-3.8-live-extended-thinking`, unless pinned) | `HC:566-576`, `system/server/config/realtime.go:73-117`, `system/server/config/config.go:433-444` |
| LIVE (full-duplex) mode | off | `HAL_LIVE_MODE=false` (LENV:130); `pro-respeaker-lite` and `pro-xvf3800` set `true` | `HC:701-705`, `robots/lamp/overrides/pro-respeaker-lite/rootfs/opt/hal/.env:21`, `.../pro-xvf3800/.../.env:13` |
| Realtime turn detection | `off` (manual activity/commit) | `off` (LENV:57) | `HC:721-725` |
| Realtime native audio | false | false (LENV:52) | `HC:728-730` |
| TTS provider | HAL falls back to `openai` if config.json lacks `tts_provider` | `elevenlabs` (seeded into config.json from ROBOT.md when empty; setup may overwrite) | `hal/runtime.py:546`, `robots/lamp/ROBOT.md:13`, `system/server/server.go:259-294`, `system/device/setup.go:242-244` |
| ElevenLabs transport | HTTP chunked | HTTP (`HAL_TTS_ELEVENLABS_WS=false`, LENV:68) | `hal/drivers/voice/tts/backend.py:71-78` |
| ElevenLabs model | `eleven_v4_turbo`, `pcm_24000`, local tempo | same | `hal/drivers/voice/tts/elevenlabs.py:29,178,183,193` |
| TTS speed | 1.2 (`HAL_TTS_SPEED`), config.json `tts_speed` wins | 1.2 | `HC:47,156-167`, LENV:13 |
| STT provider | Deepgram direct if config.json `deepgram_api_key`, else AutonomousSTT relay | **unverified** which; AutonomousSTT model `nova-3-general` when `stt_language` set | `hal/runtime.py:579-592`, `system/device/config_update.go:785-791` |
| Software AEC (WebRTC AEC3) | on | on, delay 205 ms, tail 2.0 s, NS on, ref 500 ms (LENV:62-66) | `VC:64-74` |
| Gaze wake | off, shadow on, yaw 25° | on, shadow off, yaw 60° (LENV:92-99) | `HC:824-832` |
| Follow-up window | 10 s (`HAL_WAKEWORD_FOLLOWUP_TIMEOUT_S`), config.json `wake_followup_s` (3–30) wins | 5 s (LENV:81) | `HC:586-602` |
| Opening-fillers-only | false | true (LENV:83; `pro` also true) | `HC:606-608` |
| Presence wake for strangers | false | false (LENV:86) | `HC:111-114` |

Wake phrases (when nothing owner-edited): prefixes `hello, hey, hi, alo, okay, ok, wake up` × {`autonomous`, device type
(`lamp`, from `DEVICE_TYPE=lamp`, LENV:44)} (`VC:100-105`) merged with the same prefixes × agent name (`AS:1492-1498`,
agent name read from the runtime workspace, else device type `AS:1473-1489`). config.json `wake_phrases` replaces the whole
list (`VS:418-427`, `HC:188-196`).

`HAL_ADDRESSED_GATE`: **does not exist** in this checkout (no match anywhere in `v1-main`). It was introduced by commit
`6ff85902a` ("voice: admission gate, early commit, grace cut, Google voice, non-verbal acks", 2026-10-08), which is on
`origin/lamp-release` and is **not** an ancestor of `170a1b85e`. In V1 the "addressed" decision is the wake-word gate +
follow-up focus (`is_addressed`, `hal/drivers/voice/_internal/wakeword_focus.py:136-150`) plus the realtime model's
`reject_turn` tool (model-dependent).

"Presence-warm STT": no such mechanism in V1. It arrives in commit `56b29e8f1` ("voice: presence-warm STT, conversation
window, name anywhere, session keepalive, grace cut, experience map"), also only on `origin/lamp-release` and not an ancestor
of `170a1b85e` (same for the follow-ups `471a48fb5` and `f3dd5a911`). The only STT pre-connect is `HAL_STT_KEEPALIVE` (default false, lamp false,
`VC:77-81`, LENV:8). Presence affects voice only by granting wake focus on `presence.enter` of an enrolled face
(`hal/drivers/sensing/sensing_service.py:272-273,312-326`).

---

## 1. Audio capture → "a turn"

### 1.1 Construction and lifecycle
- `runtime.py` builds `TTSService` (`hal/runtime.py:547-571`) and `VoiceService` (`hal/runtime.py:572-611`) at boot from
  config.json; STT boost terms `name:3` for agent name, device type, `autonomous`, owner wake-phrase words (`AS:1501-1524`).
  If `_mic_muted`, the pipeline is built but not started (`hal/runtime.py:606-608`).
- `/voice/start` can rebuild the pipeline (`RV:61-164`). Note it builds `AutonomousSTT` **without** `stt_model`/`stt_language`
  (`RV:132-135`), unlike boot (`hal/runtime.py:581-592`); so after a `/voice/start` rebuild the relay uses its defaults
  `nova-3-general`/`en` (`hal/drivers/voice/stt/autonomous.py:16-17,289`).
- Start: refuses if not `available` (sounddevice + numpy + STT key) (`VS:491-493,554-561`); creates `SmartTurnDetector` when
  `TURN_END_ENABLED and not LIVE_MODE` (`VS:566-567`); starts realtime orchestrator if enabled (`VS:568-569`); spawns the
  `voice` thread running `_loop` (`VS:570-571`).
- `start_voice_service` refuses while privacy mic lock or enrollment owns the mic (`AS:503-515`).
- Stop: aborts active mic, clears wake focus, cancels harness capture, stops realtime (summarize LLM calls) with 3 s join,
  joins voice thread 5 s (`VS:630-694`).

### 1.2 The mic loop (`_loop`, `VS:929-1049`)
- Pins thread to fast cores (`VS:932`); sleeps 0.5 s on start unless tap-to-talk (`VS:934-936`).
- Backend: `arecord` when `HAL_AUDIO_INPUT_ALSA` is set (lamp: `plug:device_micro2`, LENV:3) at 16 kHz (`VS:939-943`), else
  sounddevice (`VS:944-952`). Frame = 64 ms (`VC:16`, `VS:954`) = 1024 samples.
- Every iteration it does an HTTP `GET 127.0.0.1:5000/api/harness/voice-mode` (timeout 0.5 s) on the mic thread
  (`VS:966`, `hal/drivers/voice/_internal/harness_voice.py:11-32`). On any failure it returns
  `{"enabled": False, "generation": -1, "unavailable": True}` (fail closed, `harness_voice.py:30-32`).
- `requires_manual_capture(mode)` = Harness voice mode ON **or os-server unreachable** or device tap-to-talk
  (`hal/drivers/voice/_internal/input_policy.py:31-32`, `harness_voice.py:35-36`). When true and no tap capture is pending,
  ambient VAD does not run at all (`VS:972-978`). Consequence: if os-server's voice-mode endpoint fails, the lamp stops
  listening to ambient speech.
- Before opening the mic: `_wait_for_tts` (polls 0.2 s until TTS idle, then an RMS reverb gate up to
  `ECHO_GATE_MAX_WAIT_S`=1.5 s in 50 ms windows until RMS < `ECHO_RMS_FLOOR`, `VS:871-927`); waits while music plays
  (`VS:980-984`); waits while a LIVE session runs (`VS:989-994`).
- Opens the mic wrapped in software AEC (`VS:1019`, `VS:696-714`, `hal/drivers/voice/aec.py:352-359`), then `_vad_loop`.

### 1.3 VAD entry (`_vad_loop`, `VS:1088-1327`)
- Re-reads voice mode every 0.25 s; leaves if manual capture is required (`VS:1111-1118`).
- **TTS or music playing → mic drained and discarded** (warm mic: arecord stays open, frames read and thrown away;
  `VS:1119-1138`; re-check after each blocking read `VS:1167-1171`). `TTS.speaking` is true from speech admission
  (`TTS:899-900`), i.e. during synthesis TTFB as well as playback. When playback ends: echo-skip up to
  `WARM_MIC_ECHO_SKIP_MAX_S` (0.1 s) until RMS < `ECHO_RMS_FLOOR` (lamp 300), then lookback cleared and Silero reset
  (`VS:1140-1161`).
- Backchannel cue in the room → VAD muted until it decays (`VS:1176-1191`, `hal/drivers/voice/backchannel.py:60-67`).
- Lookback ring of `PRE_ROLL_FRAMES` (default 8, lamp 12 ⇒ 768 ms) (`VS:1095`, `VC:24`, LENV:6).
- Entry per frame: `RMS >= HAL_VAD_THRESHOLD` (default 3500, lamp 500) **and** WebRTC VAD says any 30 ms chunk is speech
  (WebRTC default off; lamp on, aggressiveness 2) (`VS:1334-1339`, `VC:19,39-41`, `hal/drivers/voice/_internal/vad_filters.py:81-107`).
  Consecutive frames must keep qualifying; one failing frame resets (`VS:1320-1322`).
- Hold-off: buffered speech (excluding the first frame) ≥ `HAL_SPEECH_HOLDOFF` (default 0.2 s, lamp 0.05 s) (`VS:1218-1222`).
  With 64 ms frames the lamp needs 2 consecutive qualifying frames (128 ms).
- Silero confirmation on the held frames: peak prob ≥ `HAL_SILERO_THRESHOLD` (default 0.3 and disabled; lamp enabled, 0.15)
  (`VS:1223-1228`, `vad_filters.py:136-182`, `VC:28-30`). Fails open on error (`vad_filters.py:139,180-182`).
- On confirmation: pre-roll = lookback + held frames, resampled to 16 kHz (`VS:1229-1265`); wake focus latched
  (`VS:1245-1247`); gaze check `gaze.on_speech_start()` may grant focus and show an LED-only "pending listening" cue
  (`VS:1248-1261`, `AS:1108-1128`); voice mode read again (HTTP) (`VS:1269`); route = LIVE session if LIVE mode
  (`VS:1270-1284`) else `_stream_session` (`VS:1285-1296`).
- After the session: lookback cleared, Silero reset, `SESSION_COOLDOWN_S`=0.3 s sleep (skipped after a user-cancel)
  (`VS:1297-1311`, `VC:25`).

### 1.4 Capture session (`_stream_session` / `_impl`, `VS:2294-3563`)
- Reserves "user input capture" in TTS (blocks optional speech, see §4) for non-LIVE automatic turns (`VS:2300-2306`,
  `TTS:855-866`).
- Per-turn policy frozen in `InputPolicy.for_turn` (`input_policy.py:52-60`): `realtime_allowed = not explicit and not
  live_mode and not harness/unavailable`.
- Opens a new STT session on a thread (`VS:2674-2694`); while connecting, mic frames are buffered (`VS:2696-2713`).
  If TTS starts during connect it waits ≤2 s and stops buffering (`VS:2706-2708`). Pre-roll + connect buffer are sent first
  (`VS:2731-2785`); a pre-connected keepalive session closed at speech start is reconnected once and replayed (`VS:2746-2764`).
- Posts `{"type":"voice_listening"}` to os-server (timeout 0.3 s) (`VS:2799-2806`); os-server only stores
  `voiceActiveUntil = now+10 s` (`system/server/sensing/delivery/http/handler.go:233-239`).
- Prewarms the realtime session if wake word is on (`VS:2431-2432`).
- Per frame (`VS:2809-2966`):
  - closes on TTS start (`endpoint_method="tts_started"`, `VS:2826-2830`) or music start (`VS:2831-2835`);
  - duration ceiling: `TURN_END_MAX_DURATION_S` (180 s) once words exist, else `MAX_SESSION_DURATION_S` (default 30, lamp 20)
    (`VS:2837-2852`, `VC:49,56`, LENV:51);
  - sends audio to STT; on a normal 1000 close before any transcript, one reconnect-and-replay (`VS:2858-2893`);
  - streams the same audio to the realtime model only once the realtime turn is started (§2.2) (`VS:2899-2915`);
  - silence clock: a frame with RMS ≥ threshold refreshes `last_speech_time` only if Silero (3-frame windows, same threshold)
    confirms speech; loud non-speech counts as silence (`VS:2917-2945`, `VC:35-36`).
- **End of turn** (`VS:2946-2966`):
  1. `turn_should_close`: silence > `HAL_SILENCE_TIMEOUT` (default 2.5 s, lamp 1.2 s), or ≥ `ENDPOINT_SILENCE_S` (0.8 s) since
     an STT final that arrived after the last speech (`vad_filters.py:244-257`, `VC:20,111`).
  2. Then `TurnEndpoint.should_close` (when `HAL_TURN_END_ENABLED`, default true, non-LIVE, non-manual) on the last 125 frames
     (8 s) of PCM (`VS:2947-2956`, `hal/drivers/voice/_internal/turn_endpoint.py:43-81`):
     - no alphanumeric text → close now (`silence_clock`);
     - silence ≥ `TURN_END_MAX_PAUSE_S` (6 s) → close (`turn_pause_limit`);
     - trailing hesitation word (EN/VI list: uh, um, and, or, but, because, "let me think", và, nhưng…) → keep waiting
       (`turn_endpoint.py:10-17,68-69`);
     - short greeting (≤3 words starting hello/hi/hey/chào) and silence < 2.5 s → wait (`turn_endpoint.py:20-25,70-71`);
     - Smart Turn v3 result (local ONNX via pipecat, CPU 1 thread, max 8 s PCM) → close if "complete", else wait up to 6 s
       (`turn_endpoint.py:72-74`, `hal/drivers/voice/_internal/smart_turn.py:8-17,47-58,76-112`);
     - result pending → wait 0.5 s from candidate, then fallback silence ≥ `TURN_END_FALLBACK_S` (2.5 s) (`turn_endpoint.py:78-81`).
     - Smart Turn unavailable (missing pipecat extra/model) → silence fallback (`smart_turn.py:103-112`). Imager installs
       `--extra pipecat` (`scripts/imager/build-orangepi.sh:1253`); presence of the bundled model on devices is **unverified**.
- Finalize (`hal/drivers/voice/_internal/session_finalize.py:71-105`): transcript = finals + last partial; leading words that
  repeat the device's last TTS text are stripped (single short word ignored) (`session_finalize.py:21-68`); punctuation-only
  → empty; speaker-ID buffer trimmed to last speech + 200 ms; SER keeps full buffer.
- Max-duration with words → whole request **discarded** (`VS:3109-3113`).

### 1.5 Wake word (when `WAKEWORD_ENABLED`)
- Match rule: a wake phrase equals a sentence or starts/ends a sentence (split on `.!?`) (`hal/drivers/voice/_internal/speaker_decorate.py:240-258`).
- Partial match → gate "detected": wake chime (if config.json `wake_chime` not false, `HC:183-185`) and listening LED
  (`VS:2484-2499`). Chime preempts audio on the speaker (`TTS:2167-2179`).
- Final match → "confirmed" (`VS:2501-2509`, `VS:2533-2534`).
- At session end, if detected but not confirmed: confirm on assembled text, or one-letter slip in the name
  (`speaker_decorate.py:260-281`), or prefix + any word in the name slot (`speaker_decorate.py:283-302`); else the turn is
  dropped (`VS:3133-3167`). Loose rules can only confirm a gate an exact partial opened.
- Bare wake phrase ("hey lamp" only): no agent/realtime turn; open realtime activity discarded; chime (if not already) +
  follow-up window + listening LED for the window (`VS:3360-3378,3426-3427,3472-3473,3524-3525`, `VS:444-474`).

### 1.6 Follow-up focus (`hal/drivers/voice/_internal/wakeword_focus.py`)
- `is_active()` true while any tracked authorized turn is not cancelled, or a refresh deadline is in the future (`:109-117`).
- `begin(iid)` holds an authorized turn for up to 300 s while processing (`:29-44`); `activity()` binds os-server main runs
  (`:46-69`, `RV:492-498`); after local processing, main run and pending speech for that owner finish, the idle window
  (lamp 5 s) starts (`:92-107`).
- `refresh()` (button, bare wake, gaze, presence, os-server boot greeting) sets a deadline, never shortens one
  (`:119-133`; callers `VS:397-410`, `hal/drivers/button_actions.py:259-266,320`, `hal/drivers/tracking/gaze.py:899-921`,
  `hal/drivers/sensing/sensing_service.py:312-326`, `RV:477-483`, `system/server/config_watch.go:338`).
- Gaze grants `min(GAZE_WAKE_FOCUS_S=10, window)` (`HC:871`, `wakeword_focus.py:124`); needs ≥2 samples and ≥60 % facing in
  the last 1.5 s, cooldown 5 s (`gaze.py:858-900`, `HC:832-869`). When there is no usable face evidence at speech start it
  requests a "remembered-pose reacquire" (the body turns), released at speech end (`gaze.py:887-896,930-958`).
- Presence: only enrolled faces unless `HAL_PRESENCE_WAKE_STRANGERS` (`sensing_service.py:315-317`); `presence.enter` is
  rate-limited by `EVENT_COOLDOWN_S` 60 s (`sensing_service.py:263-268`, `HC:83`).
- Focus is latched at session start; a session that started inside the window is a follow-up even if it ends after
  (`VS:2377-2384`).

### 1.7 Barge-in / interruption
- **Turn-based mode (standard lamp): no voice barge-in.** While TTS speaks or music plays, mic frames are read and discarded
  (`VS:1119-1138`); a TTS start during a capture ends the capture (`VS:2826-2830`). A spoken wake phrase cannot interrupt
  because no STT session exists while speaking.
- Physical single tap is the barge-in: cancels os-server speech, stops TTS and music, unmutes, grants focus, ack chime
  (`hal/drivers/button_actions.py:271-332`); the spoken "Listening" cue is disabled (`button_actions.py:215-233`).
- `/tts/stop` releases a running realtime receive loop (`cancel_automatic_reply`) and stops TTS (`RV:467-474`, `VS:2275-2285`);
  os-server's intent rule `stop_talking` calls it (`system/intent/rules_audio.go:107`).
- LIVE mode only: provider server-interrupt cancels the live reply; local energy ducks playback (`VS:1451-1470,1721-1763,2167-2168`).

### 1.8 Echo handling (all layers)
1. Mic drained during TTS/music (`VS:1119-1138`).
2. Post-playback echo skip ≤0.1 s to RMS < 300 (`VS:1140-1161`); `_wait_for_tts` RMS gate ≤1.5 s when the loop reopens the mic (`VS:871-927`).
3. Software AEC3 on the mic stream; bypassed when nothing played for `AEC_TAIL_S` (2 s) (`aec.py:241-275`).
4. Backchannel self-audio window mutes VAD (`backchannel.py:60-67,183`, `VS:1176-1191`).
5. Transcript echo-prefix stripping (`session_finalize.py:21-68`).
6. Dispatch echo filter: transcript similarity ≥0.55 to last TTS text within 15 s → not sent (`hal/drivers/voice/_internal/sensing_sender.py:58-74,88-89`, `VC:47-48`). Skipped for `voice_agent_handled` (`TD:235-241`).
7. TTS starting mid-capture closes the capture (`VS:2826-2830`).

### 1.9 Tap-to-talk vs just-talk
- Default is automatic ("just talk"). `voice_input_mode: tap_to_talk` is opt-in in config.json (`HC:578-581`); it forces
  `WAKEWORD_ENABLED=False`, `LIVE_MODE=False`, turn detection off (`HC:583,701,722-723`, `hal/drivers/voice/_internal/input_mode.py:11-26`).
  Mode changes apply without HAL restart via `/voice/input-mode` (`RV:191-201`, `input_mode.py:44-90`); a wakeword change
  restarts HAL (`system/device/wakeword.go:48-56`).
- Tap path: `physical_short_tap` (`hal/drivers/device_tap_actions.py:17-72`): first tap supersedes pending replies
  (timestamp cutoff), stops TTS/music, unmutes, starts capture; second tap finishes. Capture: 10 ms reads, ready tone after the
  first frame, buffered during STT connect ≤10 s, max `MAX_SESSION_DURATION_S` (`hal/drivers/voice/_internal/device_capture.py:39-85`,
  `hal/drivers/voice/_internal/device_voice_pipeline.py:113-235`). Realtime streams in parallel and commits at finish
  (`device_voice_pipeline.py:255-301`, `hal/drivers/voice/_internal/device_realtime.py:170-175,267-271`). Dispatch is always
  `voice_command` (`device_voice_pipeline.py:475-480`). Empty STT with confirmed speech → "I didn't catch that. Could you say
  it again?" (`hal/drivers/voice/_internal/device_retry_feedback.py:14-53`, `hal/i18n.py:48-49`).
- Harness voice mode ON: ambient VAD off; capture only via explicit harness capture; dispatch "harness_only"
  (`VS:595-602`, `TD:129-153`).

### 1.10 Sleep / mute / privacy
- Sleepy emotion preset has `mic: off, speaker: off` (`hal/presets.py:146`); entering sleepy stops the voice service and mutes
  the speaker (`hal/routes/emotion.py:151-156`, `AS:517-545,608-624`). While asleep the lamp cannot hear "wake up …";
  wake is by tap (`button_actions.py:159-172`, `device_tap_actions.py:37-42`) or remote API. Physical sleep gesture speaks the
  sleep phrase and waits `HAL_SLEEP_ANNOUNCEMENT_DELAY_S` (2 s) first (`button_actions.py:420-445`, `HC:49`).
- Wake restores only mutes sleep itself set (`AS:627-647`).
- Mic mute route: stop voice service in background; unmute refused (409) while the HW switch is off (`RV:501-532`).
- Scenes reading/focus/movie keep the mic on but mute the speaker (`hal/presets.py:167-174`): turns are still captured and
  dispatched, replies are suppressed (`RV:309-311,434-436`).

---

## 2. Turn path (non-LIVE) — STT, dispatch, reply

### 2.1 STT providers
- Selection: `deepgram_api_key` present → `DeepgramSTT` else `AutonomousSTT` (`hal/runtime.py:579-592`).
- `DeepgramSTT` (`hal/drivers/voice/stt/deepgram.py`): default model `flux-general-en` (Listen v2) with only model/encoding/
  rate params — no language, no keywords for Flux (`deepgram.py:13,40-50,110-114`); runtime constructs it without model/
  language (`hal/runtime.py:580`) ⇒ English-only Flux; listener ready timeout 5 s, close join 5 s (`deepgram.py:199-202,232-235`).
  EndOfTurn events are treated as finals (`deepgram.py:152-160`).
- `AutonomousSTT` (`hal/drivers/voice/stt/autonomous.py`): Deepgram wrapper on campaign-api WS `…/ws/audio/transcriptions`,
  default `nova-3-general`, language default `en`, nova params: smart_format, interim results, endpointing 1500 ms,
  vad_events, `keyterm` boosts (`autonomous.py:16-21,63-95,98-100,279-309`). Open timeout 10 s; `close()` sends
  `CloseStream` and joins the receiver up to **15 s** waiting for the final transcript (`autonomous.py:131-136,250-273`).
  KeepAlive is a no-op for Flux (`autonomous.py:232-248`).

### 2.2 Realtime at the boundary (turn path, realtime enabled — the lamp default)
- Audio, not text, goes to the realtime model; the STT transcript is used for gating, noise guard and main dispatch.
- When the realtime turn may start (`VS:2586-2668`):
  - wake word ON: only after capture is complete **and** wake confirmed, or immediately if a follow-up window was active at
    session start / is active now (`VS:2595-2604`);
  - wake word OFF: as soon as `prepare_turn` finishes (off-thread) (`VS:2640-2668`).
  - On start: `bind_audio_turn()`, `send_text("[TURN CONTEXT] Time | Location | Reply language | Current user | pending main
    question")` (`RT:273-325`), flush buffered audio (`VS:2614-2626,2649-2655`).
- Overlap drain ("early realtime"): if wake OFF or follow-up active, and the endpoint was smart_turn/fallback/pause/silence,
  the realtime turn runs while STT CloseStream drains in a worker (`VS:2990-3094`). A **wake-phrase opener without an open
  window is not eligible** (`early_authorized`, `VS:3001`): it closes STT synchronously (≤15 s join, `VS:3096`), then
  prepares realtime (waits ≤`PREWARM_JOIN_TIMEOUT_S`=4 s, `VS:2581-2584`, `hal/realtime/orchestrator.py:73`), then commits.
- `run_realtime_turn` (`RT:514-1073`): commit audio, stream output on the voice thread; text sentences go to HAL TTS with
  `realtime_reply=True` (first sentence via `speak`, rest `speak_queue`; `RT:778-852`); `FIRST_CHUNK_MAX_CHARS`=0 ⇒ whole
  first sentence (`HC:1118-1120`, `RT:107-111`). ElevenLabs path holds a sentence until trailing reaction tags arrive
  (`RT:37-79,823-825`). Outputs: `DelegateSignal` (`RT:700-708`), `RejectSignal` (`RT:709-714`), `LookReplaySignal`
  (re-commit, `RT:697-699,860-873`), text, native audio (unused on lamp). Gemini 1011 retries only for models needing the
  idle workaround (`RT:633-638`). Exceptions → route `realtime_error`, treated as delegated fallback (`RT:996-1013`).
- Result routes (`RT:165-174`): handled (spoke), delegated, ai_rejected, no_output, error, unavailable, noise_dropped,
  foreign_dropped, not_started, cancelled.
- Visual: `thinking` cue at commit (`RT:680-683,192-204`), cleared at first speech or on dead turns (`RT:253-270`).

### 2.3 Dispatch to os-server (`TD:114-324`, `sensing_sender.py:76-177`)
- Whether to dispatch at all (`VS:3467-3478`): tap/harness capture always; else `should_dispatch_to_main(wakeword_enabled,
  authorized)` = true when wake word off, else wake confirmed or follow-up focus (`RT:367-374`); bare wake never.
  Not-addressed turns are excluded from metrics and only submit SER (`VS:3477-3478,3522-3537`).
- Speaker ID ("Speaker - Name: text" / "Unknown Speaker: [voice:hash] text (audio saved at …)") from a prepass thread
  (`VS:3218-3312,3479-3484`, `speaker_decorate.py:429-528`); cached 90 s (300 s in follow-up) (`VC:89-92`).
- Event type: `voice_command` when wake confirmed; `voice_followup` in a follow-up; else classify (`voice_command` if the
  text has a wake phrase, else `voice`) (`VS:3514-3517`, `speaker_decorate.py:331-338`, `input_policy.py:72-79`).
- Route (`TD:177-313`):
  - noise_dropped / foreign_dropped / cancelled / explicit `reject_turn` → **nothing sent** (`RT:359-364`, `TD:314-322`);
  - handled → `voice_agent_handled` with `[skills: input-branching]\n[HANDLED] <msg>\n[REPLY] <reply≤600 chars>`
    (main agent notified; "stays silent" per log) (`TD:225-248`, `HC:1110`);
  - delegated → `[voice-instruction] <delegate msg>\n[transcript] <msg>` (+ `[realtime-handoff]` note if realtime already
    spoke, + `[realtime-context]` JSON, + vision hint/image) with the voice event type (`TD:249-299`);
  - anything else with a transcript (no_output, error, unavailable, not_started) → **plain STT fallback** (`TD:300-313`).
- POST `/api/sensing/event` with `type, message, voice_turn_type, harness_voice, interaction_id, current_user,
  suppress_auto_fillers, images` (`sensing_sender.py:99-123`); timeout 30 s for user voice (5 s otherwise, 90 s with image),
  3 tries on 503 with 2 s backoff, no retry on connection error for voice (`sensing_sender.py:134-177`). This POST runs on the
  mic thread (`VS:3504-3521`).
- Also: a non-handled transcript is saved as realtime main handoff (`VS:3499-3500`); SER submitted (`TD:324`).
- Per a HAL comment, os-server treats plain `voice` as overheard ambient input (`input_policy.py:72-76`) — os-server side
  **unverified** here.

### 2.4 How the reply is spoken
- Realtime replies: spoken by HAL TTS inside `run_realtime_turn` (above).
- Main-agent replies: os-server streams the first sentence early then the remainder at run end through
  `/voice/speak-queue` with `turn_id`, `turn_seq`, `realtime_feedback:true` (`system/server/agent/delivery/http/handler_event_agent.go:648-680`,
  `system/lib/hal/client.go:258-268`). HAL: highest `turn_seq` wins; a newer turn preempts (stop + take the speaker), older seq
  dropped as superseded (`TTS:1048-1100`); same-turn segments queue and pre-synthesize behind the current speech
  (`TTS:1134-1177,1179-1210`) and are drained gaplessly on the same stream (`TTS:1219-1300,1876-1880`).
- Each main reply that finishes playing is fed to realtime as `[TTS HISTORY] …` (≤300 chars), or `not spoken` /
  `interrupted` variants (`VS:290-308,366-386`, `HC:1112`); if it ends in a question, the next actionable answer is owed to
  main for ≤300 s (`hal/drivers/voice/_internal/main_followup.py:35-47`, `HC:1106`).
- Synthesis: text split into growing sentence chunks (60 chars ×2 per chunk, ≤520, ≤12) (`TTS:1500-1545`); head chunk fetched
  before opening the stream, tail produced in parallel (`TTS:1761-1881`); 3 retries per chunk, no retry on 404/503/rate-limit
  (`TTS:1603-1649`); empty playback flashes a backend-error LED (`TTS:1925-1931`); rate-limit notice at most every 300 s
  (`TTS:34-36,1938-1960`). Exact-text cache-first hits play without API (`TTS:944-952,1102-1110`).
- Interrupting TTS: `stop()` sets the stop event, clears the pending queue (non-LIVE) (`TTS:650-666`); `speak()` on a busy
  interruptible speech stops it, else returns busy (`TTS:954-966`).

---

## 3. Every decision point (speak / stay quiet / ignore / hand off)

| # | Decision | Rule | Code or model | Default (lamp) | Where |
|---|---|---|---|---|---|
| 1 | Is the mic listening at all | running, not muted, not asleep, HW switch on, no enrollment, privacy unlocked | code | on | `AS:503-515`, `RV:501-532`, `hal/routes/emotion.py:151-156` |
| 2 | Ambient VAD suspended | Harness voice mode ON, os-server voice-mode unreachable, or tap-to-talk | code | os-server dependent | `input_policy.py:31-32`, `harness_voice.py:30-36`, `VS:972-978,1115-1118` |
| 3 | Ignore audio during own speech/music | `tts.speaking or music.playing` → drain | code | always | `VS:1119-1138,1167-1171` |
| 4 | VAD entry | RMS ≥ thr AND WebRTC AND hold-off AND Silero | code (Silero = small model) | 3500/off/0.2 s/off (lamp 500/aggr 2/0.05 s/0.15) | `VS:1209-1228,1334-1339` |
| 5 | Gaze opens focus | ≥2 samples, ≥60 % facing within 1.5 s, yaw ≤ limit, cooldown 5 s | code (face detector) | off (lamp on, 60°) | `gaze.py:839-922`, `VS:1248-1261` |
| 6 | Presence opens focus | `presence.enter` with enrolled face (or strangers flag) | code | on when wakeword on | `sensing_service.py:272-273,312-326` |
| 7 | Wake gate open/confirm | phrase at sentence start/end; partial opens, final confirms; slip/prefix-slot confirm | code | wakeword false (fresh lamp config true) | `VS:2484-2509,3133-3167`, `speaker_decorate.py:240-302` |
| 8 | "Addressed" (LED cue, backchannel) | wakeword off, or wake heard, or focus | code | — | `wakeword_focus.py:136-150`, `VS:2458-2482` |
| 9 | End of turn | silence clock → hesitation/greeting rules → Smart Turn → fallback/limits | code + local ONNX classifier | 2.5 s / 0.8 s / 6 s / 2.5 s (lamp silence 1.2 s) | `vad_filters.py:244-257`, `turn_endpoint.py:43-81` |
| 10 | Drop overlong request | duration ≥180 s with words (else 30/20 s) → discard | code | 180 / 30 (lamp 20) | `VS:2837-2852,3109-3113` |
| 11 | Cut capture when lamp starts talking | TTS start mid-capture ends it (turn still dispatched) | code | always | `VS:2826-2830` |
| 12 | Noise turn | non-actionable filler-only text; or ≤3 words / empty and Silero voiced density < ratio (and voiced ms < min); or empty transcript with REQUIRE_TRANSCRIPT | code | ratio 0.55, min voiced 0, require transcript true (lamp ratio 0.45, min voiced 160 ms) | `RT:377-382,400-411,433-447`, `noise_guard.py:4-15`, `VS:801-839,3196-3216`, `HC:661-694` |
| 13 | Bare wake phrase | whole turn is only wake phrase(s) → chime + window, no agent | code | — | `VS:3360-3378` |
| 14 | Realtime may hear the turn | wake confirmed after capture, or follow-up focus, or wakeword off | code | — | `VS:2595-2613` |
| 15 | Harness follow-up pending | os-server says active → delegate to main, no realtime reply | code (os-server state) | — | `RT:450-456,594-596` |
| 16 | Answer / delegate / reject / silent | Gemini Live chooses: speak, `delegate_to_main`, `reject_turn`, `<no speech>` | **model** (prompt-guided) | gemini-3.8-live-extended-thinking (seeded) | `RT:697-716,887-897`, `hal/realtime/orchestrator.py:81,183,211`, `hal/realtime/resources/routing_prompt_gemini.md:2-10` |
| 17 | Honor a reject | only explicit `reject_turn` and `HAL_REALTIME_AI_REJECT_FILTER` | code over model output | true | `RT:354-364`, `HC:678-680` |
| 18 | Foreign-script reply | CJK/Vietnamese characters on a device not set to that language → never spoken, not dispatched | code | true | `RT:414-430,787-801,907-917`, `HC:696-698` |
| 19 | CoT leak | regex filter strips planning text before speaking | code | always | `hal/drivers/voice/_internal/cot_leak_filter.py`, `RT:725-726,804,829` |
| 20 | Answer owed to main (#564) | after a heard main question ≤300 s, realtime told to delegate; a self-answer/reject is forwarded to main | code + model | 300 s | `RT:316-324,607,1046-1061`, `main_followup.py` |
| 21 | Dispatch to os-server | addressed and not dropped | code | — | `VS:3467-3498`, `RT:367-374` |
| 22 | What is dispatched | handled→`voice_agent_handled`; delegated→`[voice-instruction]`; else STT fallback | code | — | `TD:177-313` |
| 23 | Echo-drop at dispatch | similarity ≥0.55 to last TTS within 15 s | code | 0.55 / 15 s | `sensing_sender.py:58-74,88-89` |
| 24 | Dead-air filler ("Hmm...") | realtime turn admitted, transcript >3 words, not opening-fillers-only follow-up, no output after delay | code | 1.5 s (lamp 0.5 s) | `RT:207-251,619-627`, `VS:3297-3306`, os-server `deadair_filler.go:247-280` |
| 25 | Backchannel ("Uhm/Ok/Yeah…") | addressed automatic turn, not suppressed, partial unchanged 8 s, ≥5 s since last, TTS idle | code | stall 8 s, interval 5 s | `backchannel.py:27-35,69-134`, `VS:2527-2529` |
| 26 | Speech admission (speaker gate) | see §4 | code | — | `TTS:810-1177`, `device_input_gate.py` |
| 27 | Harness announcement | gate open (idle 1.5 s, not listening…), progress spoken with p=0.15 | code + RNG | — | `hal/drivers/harness/announcer.py:102-122`, `update_queue.py:108-137` |
| 28 | Tap capture empty | confirmed speech + empty STT + no realtime outcome → retry phrase | code | — | `device_retry_feedback.py:14-53` |
| 29 | LIVE mode (profiles only) | music → skip; noise pre-roll → skip; wake closed → turn path; idle 15 s / 3 unprompted replies / 600 s / end_conversation +3 s → hang up | code + provider VAD + model | off (lite/xvf3800 on) | `VS:1341-1376,2082-2128,1709-1720`, `VC:126-154` |

---

## 4. Queued speech, announcer, speaker gate

### 4.1 TTS admission ("who may speak when")
- Speaker muted (`app_state._speaker_muted` or privacy) → refused (`TTS:810-818,935-938,1028-1031`; routes return
  `suppressed`, `RV:309-311,434-436`).
- Owner suppressed (turn stopped by the user / superseded by a tap cutoff) → refused and reported as unspoken
  (`TTS:820-838,939-942`, `hal/drivers/voice/tts/turn_supersession.py:11-26`).
- Music streaming → `/voice/speak` and `/voice/speak-queue` 409 (`RV:312-316,438-441`).
- User capture active (automatic session reservation): optional speech (interruptible, not reply, not realtime) dropped
  (`TTS:907-912`); passive-sensing speech raises `PassiveSpeechSuppressed` (`TTS:888-890,914-917`). Main replies and realtime
  replies are not optional, so they play — and their start ends the capture (`VS:2826-2830`).
- Device tap capture active: `DeviceInputGate` defers non-optional speech FIFO (≤32 items/64 KB) until capture ends; optional
  speech rejected (`hal/drivers/voice/tts/device_input_gate.py:19-75,153-178`).
- Busy speaker: `speak()` interrupts only interruptible speech, else busy (`TTS:954-966`); `speak_queue()` queues; newer turn
  preempts (`TTS:1071-1095`).
- Wake chime preempts current audio (keeps main queue) (`TTS:2167-2179`).

### 4.2 Fillers and backchannels
- Backchannel: plays a random filler (EN `Uhm,Ok,Hmm,Yeah,Uh huh,Right,Sure,Mm,Ah,Oh`, `hal/i18n.py:387-393`) at 0.5 volume
  directly on the TTS stream, bypassing the TTS lock and `speaking` flag, tied to the current STT session
  (`backchannel.py:136-204`). Suppressed in follow-ups when opening-fillers-only (lamp true) (`VS:2387-2392,2527-2528`).
  Inference (not device-verified): with lamp end-of-turn ≤6 s, an 8 s unchanged partial inside one capture is rarely reached.
- Realtime dead-air filler: HAL POSTs `/api/sensing/filler` (`RT:228-241`, `VC:11`); os-server speaks a cached, interruptible
  "Hmm..."/"Mm..." (`system/lib/i18n/fillers.go:19-26`, `system/server/sensing/delivery/http/deadair_filler.go:268-280`).
  os-server's main-agent opening filler is paused in code (`deadair_filler.go:225-231`).
- Spoken "Listening" cue on tap: disabled (`button_actions.py:215-233`).

### 4.3 Harness announcer (`hal/drivers/harness/announcer.py`, `update_queue.py`)
- `/voice/harness/update` queues an update (`RV:401-415`).
- Gate: TTS available and idle, ≥1.5 s since last speech, no music, voice not listening / not harness capturing / LIVE speaker
  not busy, ≥1.5 s since last transcript, no realtime turn in flight (`announcer.py:102-122`, `HC:1152`).
- Snapshot: results/questions (questions first) always; progress-only only if renderable, ≥15 s after request, ≥60 s since
  last progress for the run, and with probability 0.15; progress >30 s old and results >600 s old dropped
  (`update_queue.py:90-137`, `HC:1142-1150`).
- Rendering: through the realtime model as a device-initiated `<harness_update>` envelope with a result chime
  (`hal/drivers/voice/_internal/realtime_announce.py:76-185`); preempted → requeued; no speech → fallback summarizer (≤12 s)
  or sanitized text spoken with `harness_result` chime, waiting ≤10 s for a busy speaker (`announcer.py:161-206`).

---

## 5. Configuration knobs in this area

Environment (`VC` unless noted): `HAL_VAD_THRESHOLD` 3500 (lamp 500), `HAL_SILENCE_TIMEOUT` 2.5 (1.2), `HAL_SPEECH_HOLDOFF`
0.2 (0.05), `HAL_PRE_ROLL_FRAMES` 8 (12), `HAL_SESSION_COOLDOWN_S` 0.3, `HAL_SILERO_ENABLED` false (true),
`HAL_SILERO_THRESHOLD` 0.3 (0.15; lite 0.10), `HAL_SILERO_CHUNK_SIZE` 512, `HAL_SILENCE_VAD_ENABLED` true,
`HAL_SILENCE_VAD_WINDOW_FRAMES` 3, `HAL_WEBRTCVAD_ENABLED` false (true), `HAL_WEBRTCVAD_AGGRESSIVENESS` 2,
`HAL_WEBRTCVAD_FRAME_MS` 30, `HAL_ECHO_RMS_FLOOR` 200 (300), `HAL_ECHO_GATE_MAX_WAIT_S` 1.5, `HAL_ECHO_GATE_WINDOW_S` 0.05,
`HAL_ECHO_SIMILARITY_THRESHOLD` 0.55, `HAL_ECHO_RELEVANCE_WINDOW_S` 15, `HAL_MAX_SESSION_DURATION_S` 30 (20),
`HAL_TURN_END_ENABLED` true, `HAL_TURN_END_FALLBACK_S` 2.5, `HAL_TURN_END_MAX_PAUSE_S` 6, `HAL_TURN_END_MAX_DURATION_S` 180,
`HAL_WARM_MIC` true, `HAL_WARM_MIC_ECHO_SKIP_MAX_S` 0.1, `HAL_AEC_ENABLED` true, `HAL_AEC_DELAY_MS` 205, `HAL_AEC_NS` true,
`HAL_AEC_TAIL_S` 2.0, `HAL_AEC_REF_MS` 500, `HAL_STT_KEEPALIVE` false, `HAL_STT_KEEPALIVE_PING_S` 3,
`HAL_SPEAKER_PREPASS_JOIN_S` 2.0, `HAL_SPEAKER_PREPASS_COMMIT_JOIN_S` 0.2, `HAL_SPEAKER_ID_CACHE_S` 90,
`HAL_SPEAKER_ID_CACHE_FOLLOWUP_S` 300, `HAL_ENROLL_NUDGE_COOLDOWN_S` 1800, `HAL_ENDPOINT_SILENCE_S` 0.8,
`HAL_LIVE_DUCK_GAIN` 0.12, `HAL_LIVE_UPLINK_DURING_PLAYBACK` mute (cancelled; lite/xvf `always`), `HAL_LIVE_PLAYBACK_TAIL_S`
0.35, `HAL_LIVE_IDLE_HANGUP_S` 15, `HAL_LIVE_IDLE_REQUIRES_TRANSCRIPT` true, `HAL_LIVE_MAX_UNPROMPTED_REPLIES` 3,
`HAL_LIVE_HANGUP_GRACE_S` 3, `HAL_LIVE_MAX_S` 600.

Environment (`HC`): `HAL_TTS_SPEED` 1.2, `HAL_TTS_ELEVENLABS_WS` false, `HAL_SLEEP_ANNOUNCEMENT_DELAY_S` 2.0,
`HAL_WAKEWORD_FOLLOWUP_TIMEOUT_S` 10 (5), `HAL_VOICE_OPENING_FILLERS_ONLY` false (true; lite/xvf false), `HAL_LIVE_MODE`
false, `HAL_LIVE_VAD_*` (low/""/300/0; lite/xvf high/high/100/1000), `HAL_REALTIME_TURN_DETECTION` off,
`HAL_REALTIME_NATIVE_AUDIO` false, `HAL_REALTIME_FILLER_DELAY_S` 1.5 (0.5), `HAL_REALTIME_REQUIRE_TRANSCRIPT` true,
`HAL_REALTIME_REQUIRE_SPEECH_ON_EMPTY_STT` true, `HAL_REALTIME_NOISE_SPEECH_RATIO` 0.55 (0.45),
`HAL_VOICE_NOISE_MIN_VOICED_MS` 0 (160), `HAL_REALTIME_NOISE_GUARD_MAX_WORDS` 3, `HAL_REALTIME_NONACTIONABLE_FILLERS`
(ok, okay, yeah, uh, um, hmm, huh, oh, ah, right, one sec, hold on, hang on, …), `HAL_REALTIME_MIN_COMMIT_DURATION_S` 0.8,
`HAL_REALTIME_AI_REJECT_FILTER` true, `HAL_REALTIME_FOREIGN_SCRIPT_GUARD` true, `HAL_REALTIME_FIRST_CHUNK_MAX_CHARS` 0,
`HAL_REALTIME_TTS_HISTORY_MAX_CHARS` 300, `HAL_REALTIME_REPLY_SYNC_MAX_CHARS` 600, `HAL_REALTIME_MAIN_FOLLOWUP_S` 300,
`HAL_REALTIME_ZOMBIE_RECONNECT_AFTER` 3, `HAL_GAZE_WAKE` false (true), `HAL_GAZE_SHADOW` true (false), `HAL_GAZE_MAX_YAW_DEG`
25 (60), `HAL_GAZE_WAKE_FOCUS_S` 10, `HAL_GAZE_COOLDOWN_S` 5, `HAL_PRESENCE_WAKE_STRANGERS` false,
`HAL_SPEAKER_RECOGNITION_ENABLED` true, `HAL_SPEECH_EMOTION_ENABLED` true, `SPEAKER_MATCH_COS` 0.5, `HAL_HARNESS_*` (§4.3),
`HAL_AUDIO_INPUT_ALSA` / `HAL_AUDIO_OUTPUT_ALSA` (lamp `plug:device_micro2` / `plug:device_speaker`).
Backchannel (`backchannel.py:27-36`): `HAL_BACKCHANNEL_FILLERS`, `HAL_BACKCHANNEL_STALL_S` 8, `HAL_BACKCHANNEL_INTERVAL_S` 5
(lamp 5), `HAL_BACKCHANNEL_VOLUME` 0.5, `HAL_BACKCHANNEL_ECHO_TAIL_S` 0.4. TTS (`TTS:34-40`):
`HAL_TTS_RATE_LIMIT_NOTICE_INTERVAL_S` 300, `HAL_TTS_CACHE_DIR`.

config.json keys read by this area: `wakeword`, `wake_phrases`, `wake_chime`, `wake_followup_s`, `wake_light`,
`voice_input_mode`, `realtime{enabled,provider,native_audio,gemini{model,voice,thinking_level,google_search,vision},…}`,
`agent_runtime`, `tts_provider`, `tts_voice`, `tts_speed`, `tts_instructions`, `tts_api_key`, `tts_base_url`,
`deepgram_api_key`, `stt_api_key`, `stt_base_url`, `stt_model`, `stt_language`, `llm_api_key`, `llm_base_url`, `device_type`
(`HC:153-208,526-608`, `hal/runtime.py:535-592`, `hal/wake_light.py:45-47`).

Env presets `hal/env-presets/01-03` are tuning files to copy by hand (`hal/env-presets/README.md`); not applied automatically.

---

## 6. Timing (values and where)

| Item | Value | Where |
|---|---|---|
| Frame | 64 ms | `VC:16` |
| Entry hold-off | 0.2 s (lamp 0.05 s ⇒ 2 frames) | `VC:21`, `VS:1218-1222` |
| Pre-roll | 8 frames/512 ms (lamp 12/768 ms) | `VC:24` |
| Voice-mode poll (HTTP, mic thread) | each loop + every 0.25 s; 0.5 s timeout | `VS:966,1111-1113`, `harness_voice.py:14` |
| `voice_listening` POST | 0.3 s timeout | `VS:2800-2806` |
| Silence clock | 2.5 s (lamp 1.2 s); 0.8 s after a final | `VC:20,111` |
| Smart Turn | candidate wait 0.5 s; pause cap 6 s; fallback 2.5 s; PCM ≤8 s | `turn_endpoint.py:31-81`, `smart_turn.py:8` |
| Capture ceilings | 30 s (lamp 20 s) no words; 180 s with words | `VC:49,56` |
| Session cooldown | 0.3 s | `VC:25`, `VS:1307-1308` |
| Post-TTS echo skip | ≤0.1 s; reopen RMS gate ≤1.5 s | `VC:45,60` |
| STT connect / close | Autonomous open 10 s, recv join 15 s; Deepgram ready 5 s, join 5 s | `autonomous.py:131-136,264`; `deepgram.py:199,233` |
| Tap capture connect buffer | ≤10 s | `VS:2702`, `device_capture.py:39-79` |
| Realtime prepare join | 4 s; LIVE availability wait 5 s | `orchestrator.py:73`, `VS:1373,2250` |
| Realtime receive gap / silence ceilings (boundary) | 8 s / 20 s / progress 15 s | `HC:610-624` |
| Dead-air filler | 1.5 s (lamp 0.5 s); POST timeout 2 s | `HC:1115`, `RT:220-233` |
| Speaker-ID join | 0.2 s when realtime started; else 2.0 s | `VC:83-84`, `VS:3308-3312` |
| Harness follow-up GET | 0.15 s timeout | `RT:453` |
| Dispatch POST | 30 s user voice; 5 s other; 90 s image; 503 retry 2 s ×3 | `sensing_sender.py:136-168` |
| Listening LED safety reset | 8 s after session | `VS:3551-3560` |
| Gaze pending LED cue | 3 s | `AS:257` |
| Follow-up window | 10 s (lamp 5 s), owner 3–30 s; processing lease 300 s | `HC:586-602`, `wakeword_focus.py:41` |
| Thinking-LED stuck reset | 25 s | `HC:288-290` |
| TTS | stop→lock 2 s; write stall 8 s; output latency 0.12 s; slice 40 ms; queued first frame 15 s; stall 30 s; head get 2 s | `TTS:62-70,964,1114,1246,1292,1835` |
| Backchannel | stall 8 s, min interval 5 s, echo tail 0.4 s | `backchannel.py:30-36` |
| Announcer | grace 1.5 s, poll 0.25 s, busy wait 10 s, summarizer 12 s | `announcer.py:14-15`, `HC:1152-1156` |
| Sleep | announcement delay 2 s; speaker drain grace 2 s, max 15 s | `HC:49`, `AS:245-246` |
| LIVE | idle hangup 15 s, unprompted 3, grace 3 s, max 600 s, playback tail 0.35 s | `VC:135-154` |

No explicit end-to-end latency budget exists in this code; only the per-stage timeouts above.

Serial blocking on the voice thread in the turn path (from code order): end-of-turn → (wake opener) synchronous STT close
(≤15 s) → finalize → realtime prepare (≤4 s) → commit → stream model output until the model finishes (TTS plays on its own
threads) → speaker-ID join → synchronous dispatch POST (≤30 s) → 0.3 s cooldown → VAD resumes (and drains while TTS still
plays) (`VS:3095-3108,3296,3435-3447,3479-3521,1297-1311`). The mic is not read during these steps.

---

## 7. Docs vs code disagreements found

1. `docs/realtime-voice.md:1082` says the HAL default follow-up window "remains 20 s"; code default is 10 s (`HC:587`).
2. `docs/simulator.md:301` says the lamp image `.env` sets 20 s; it sets 5 s (LENV:81).
3. `docs/bootstrap-ota.md` (Optional hardware overrides) says `pro-respeaker-lite` has "HAL AEC enabled … Live off"; its
   `.env` sets `HAL_AEC_ENABLED=false` and `HAL_LIVE_MODE=true`
   (`robots/lamp/overrides/pro-respeaker-lite/rootfs/opt/hal/.env:13,21`).
4. `robots/lamp/docs/physical-controls.md:173` gives a tap recording timeout "default 30 seconds"; the lamp env sets
   `HAL_MAX_SESSION_DURATION_S=20`, which is that limit (`device_voice_pipeline.py:104-105`).
5. `RV:61-62` docstring says "always-on Deepgram STT"; STT is per-turn and usually the AutonomousSTT relay.
6. `HC:827` comment calls gaze shadow "ON by default"; lamp turns it off (LENV:93) — consistent, noted for clarity.

## 8. Dead, disabled or surprising (code facts)
- Spoken "Listening" tap cue: commented out (`button_actions.py:215-233`).
- os-server opening filler: hard-paused (`deadair_filler.go:225-231`).
- `HAL_ADDRESSED_GATE`: absent in V1 (on `lamp-release` only).
- No presence-warm STT; STT keepalive off.
- Deepgram direct path ignores `stt_language`/keywords for Flux (`deepgram.py:40-50`, `hal/runtime.py:580`).
- `/voice/start` rebuild drops `stt_model`/`stt_language` (`RV:132-135`).
- Wakeword effective value depends on config.json age: pre-existing configs without the key stay off even on a lamp
  (`config.go:446-451`, `server.go:297-309`).
- os-server unreachable ⇒ lamp stops ambient listening (fail closed, `harness_voice.py:30-32`).
- No voice barge-in in turn mode; music playback also blocks all voice input.
- A wake-phrase opener cannot use the early-commit overlap; it waits for STT close.
- A main-agent reply that starts while the user is still talking ends the user's capture (`VS:2826-2830`).
