# V1 Lamp: HAL body and perception (Python) — baseline study

Scope: `hal/drivers/**` except `hal/drivers/voice/**`, plus `hal/safety/**`, `hal/policy/**`, non-voice `hal/routes/**`,
`hal/resting_led.py`, `hal/wake_light.py`, `hal/presets.py`, `hal/smooth_animation.py`, `hal/recordings/` (names), `hal/board/**`,
lamp config (`robots/lamp/**`), and the cloud perception docs.

Checkout: `origin/main` at `170a1b85e` ("deploy"). All paths are relative to that checkout. Facts only; no proposals.
"unverified" marks anything not confirmed in code. Where a claim touches the voice pipeline (`hal/drivers/voice/**`) or
`hal/realtime/**` (other agents' areas) it is noted only at the interface.

Contents
- Part A. Perception (sources, events, dedup, on-device vs cloud, camera lifecycle, environment)
- Part B. Tracking, gaze and search
- Part C. Actuators: LED strip, status LED, servos, safety, policy, presets, resting LED, wake light, scenes, music
- Part D. Physical controls, sleep, privacy, speaker/mic lifecycle, board profile (appears before Part C in this file)
- Part E. HAL HTTP surface (index)
- Part F. Decision points (consolidated)
- Part G. Configuration (HAL_* vars, lamp values)
- Part H. Dead, disabled, unreachable, stubbed
- Part I. Doc-vs-code contradictions and surprises

---

# Part A. Perception

## A.1 Wiring: what starts, and when

- Capabilities come from `robots/lamp/ROBOT.md` front matter. The lamp declares `audio`, `vision` (driver `opencv`), `sensing`,
  `presence`, `motion` (feetech), `light` (ws2812), `expression`, `lifelike` (optional), `media`, `connectivity`, `companion`
  (optional), `system` (`robots/lamp/ROBOT.md:21-42`). `environment` is commented out in the base file (`robots/lamp/ROBOT.md:25`);
  the three "Pro" hardware overlays turn it on via `profile.json` `"capabilities": {"environment": true}`
  (`robots/lamp/overrides/pro/profile.json`, `.../pro-xvf3800/profile.json`, `.../pro-respeaker-lite/profile.json`), applied at image
  build by `scripts/provision/apply-overrides.py:55-77` when `/etc/autonomous/hardware-profile` names a profile
  (`scripts/provision/apply-overrides.py:15-25`; empty or `standard` = no overlay).
- `display` is deliberately NOT declared (`robots/lamp/ROBOT.md:27-31`), so `/display` is not mounted and no DisplayService runs.
- Sensing classes are imported only if `sensing` is declared (`hal/runtime.py:166-177`); simulation swaps in
  `VirtualSensingService`, which has a presence machine but no perceptions (`hal/runtime.py:179-181`,
  `hal/drivers/sensing/virtual_service.py:8-32`).
- Sensing starts in a background thread after hardware threads join (`hal/runtime.py:676-716`):
  - gated by `HAL_SENSING_ENABLED` (default `true`, `hal/runtime.py:680-684`);
  - poll interval `HAL_SENSING_INTERVAL` (default `2.0` s, `hal/runtime.py:697`);
  - sensing mic = `AUDIO_SENSING_DEVICE` (`HAL_AUDIO_SENSING_DEVICE`; None skips SoundPerception) (`hal/runtime.py:696`, `hal/config.py:38-45`);
  - `enable_people_perception = "presence" in capabilities` (`hal/runtime.py:689,702`). People perception = face ID, facial
    emotion, motion, pose (`hal/drivers/sensing/sensing_service.py:93-108`).
  - `is_sleeping=lambda: state._sleeping` (`hal/runtime.py:701`).
- The orchestrator loop sleeps 3 s at start, then every tick (`poll_interval`, 2 s) reads one camera frame and calls
  `presence.tick()` (`hal/drivers/sensing/perceptions/orchestrator.py:267-291`).
- Each tick, a new frame is pushed to a `DataObserver`; every registered perception's `check()` is called
  (`hal/drivers/sensing/perceptions/utils.py:38-72`). `check()` is non-blocking: it skips if that perception is still busy, else
  submits to ONE class-wide `ThreadPoolExecutor(max_workers=2)` pinned to the "SLOW" CPU set
  (`hal/drivers/sensing/perceptions/processors/base.py:19-50`). All perceptions (on-device and cloud) share those 2 workers.
- Registered on the frame observer: face (if `enable_face`), the cloud set (motion, pose, fire hazard; added/removed live),
  light, and **sound** (`hal/drivers/sensing/perceptions/orchestrator.py:122-171, 195-238`). Emotion and per-face motion are
  registered on the `detected_faces` observer, which face ID sets at the end of its own check
  (`orchestrator.py:209-211`, `faceid/perception.py:548-553`).
- The frame is only pushed when a camera exists and `app_state._camera_disabled` is false (`orchestrator.py:282-288`).
  Consequence (fact): when the camera is off (manual, privacy switch, `sleepy`, scenes reading/focus/movie/night) or absent,
  no frame is pushed, so NO perception runs at all — including SoundPerception, which is registered on the frame observer
  (`orchestrator.py:154-164`). See I.1.

## A.2 Event transport to os-server

- All sensing events go through `SensingService._send_event` → `POST http://127.0.0.1:5000/api/sensing/event` with JSON
  `{"type", "message", "current_user"}` (`hal/drivers/sensing/sensing_service.py:247-310`, URL `hal/config.py:70`).
- Gates in `_send_event`, in order:
  1. While sleeping, every type except `presence.enter` is dropped (`sensing_service.py:255-257`).
  2. `presence.enter` resets the dedup state of every processor that has `reset_dedup` (motion, emotion, per-face motion, fire
     hazard) — each clears only if the visible user changed (`sensing_service.py:259-260`, `orchestrator.py:296-304`,
     `motion.py:732-753`, `emotion.py:742-766`).
  3. Per-type global cooldown, keyed by event TYPE (not by person): default `EVENT_COOLDOWN_S=60` s, or the caller's `cooldown`
     argument; skipped for `motion.activity`, `emotion.detected`, `fire_hazard.detected`, `sound` (they dedup themselves)
     (`sensing_service.py:262-270`, `hal/config.py:83`). The timestamp is recorded only when os-server answers 200
     (`sensing_service.py:303-308`), so a failed post is retried on the next qualifying tick.
  4. `presence.enter` grants the voice "wake-word focus" (lets the user speak without the wake phrase) only if the event's
     `new:` segment names a friend, or if `HAL_PRESENCE_WAKE_STRANGERS` is true (`sensing_service.py:272-273, 312-326`,
     `faceid/enter_message.py:46-52`, `hal/config.py:111-114`; lamp `.env` sets it `false`, `robots/lamp/rootfs/opt/hal/.env:86`).
  5. Images: each frame is saved as JPEG to `/tmp/hal-sensing-snapshots/sensing_<prefix>/` (max 50 files) and copied to
     `/var/lib/hal/snapshots/sensing_<prefix>/` (TTL 72 h, cap 50 MB); the persistent path is appended to the message as
     `[snapshot: <path>]` (`sensing_service.py:167-234, 275-282`, `hal/config.py:357-369`).
  6. `current_user` = `app_state.resolve_current_user()`: face wins; voice identity only fills an empty slot
     (`sensing_service.py:286-294`, `hal/app_state.py:127-180`; voice identity forgotten after `VOICE_USER_FORGET_S=300`,
     `hal/config.py:151`).
- Other perception-side posts to os-server (not via `_send_event`, so not sleep-gated or cooldown-gated by HAL):
  - `POST /api/wellbeing/log` `{"action","notes":"","user"}`: per-friend `enter`/`leave`, collapsed `unknown` enter/leave for
    strangers (`faceid/perception.py:441-455, 640-669, 682-704`), and one row per outbound activity label before each
    `motion.activity` (`motion.py:697-726`).
  - `POST /api/monitor/event` `sound_tracker` rows for every loud sample decision (`sound.py:104-115, 243-271`).
  - `POST /api/sensing/event` `speech_emotion.detected` directly from `SpeechEmotionService` (voice area) with 3 retries
    (`hal/drivers/voice/speech_emotion/service.py:647-700`).
  - `POST /api/agent/user-reconcile` after face remove/reset (`hal/routes/sensing.py:39-49, 403-433`).
- os-server-side handling (ambient floor 120 s, busy-agent drop/queue, sleep gate) is outside this area; the lamp doc describes it
  (`robots/lamp/docs/sensing-behavior.md:47-54, 852-857`) — unverified here.

## A.3 Event catalogue (HAL-side)

| Event `type` | Producer | Trigger | Message | HAL dedup / cooldown |
|---|---|---|---|---|
| `presence.enter` | `FacePerception` (on device) | a friend or stranger becomes "new" (never seen, or unseen longer than forget window: friend 3600 s, stranger 1800 s) | `Person detected — new: friend (a); already present: b (friend); faces in frame: N (labels)` + optional familiar-stranger hint + `[snapshot: …]` (`faceid/enter_message.py:21-43`, `faceid/perception.py:529-544`) | type cooldown `FACE_COOLDOWN_S=10` s (`faceid/perception.py:975-987`); stranger-only enters floored to one per `FACE_STRANGER_ENTER_FLOOR_S=300` s (dropped, not re-sent) (`faceid/perception.py:508-528`); strangers gated on gaze (A.4); allowed while asleep |
| `presence.leave` | `FacePerception` | a FRIEND unseen > `FACE_OWNER_FORGET_S` (3600 s); strangers never produce leave events | `Person no longer visible — friend (<id>)` | type cooldown 10 s (`faceid/perception.py:635-680`) |
| `presence.away` | `PresenseService` | IDLE → AWAY after `AWAY_TIMEOUT_S=900` s with no presence evidence | text instructs the agent to announce sleep ("You're going to sleep — lights off…") (`presence_service.py:185-196`) | cooldown = `AWAY_TIMEOUT_S` |
| `light.level` | `LightLevelPerception` (on device) | mean grey level changes by ≥ `LIGHT_CHANGE_THRESHOLD=100`/255 between checks every `LIGHT_LEVEL_INTERVAL_S=300` s | `Ambient light decreased/increased significantly (level: X/255, change: ±Y)` | cooldown 300 s (`light_level.py:35-66`) |
| `sound` | `SoundPerception` (on device) | sensing-mic RMS ≥ `SOUND_RMS_THRESHOLD` (code 8000; lamp 2000) | `Loud noise detected (level: N) — occurrence 1` or `… — persistent (occurrence 3)` | escalation/habituation (A.6) |
| `motion.activity` | `MotionPerception` (cloud) | flushed action labels while PRESENT | `Activity detected: <labels>.` + optional posture markers + 1 snapshot | flush 10 s, class cooldown 900 s, transition bypass 60 s, 300 s per-(user,labels) dedup (A.7) |
| `motion.activity` (per face) | `MotionPerFacePerception` (cloud) | off by default (`HAL_MOTION_PER_FACE_ENABLED=false`) | per face | per-face per-action dedup 300 s + shared class cooldown (`motion_per_face.py:27-75, 282-400`) |
| `emotion.detected` | `EmotionPerception` (cloud) | qualified facial emotion (A.8) | `Emotion detected: <Label>. (weak camera cue; confidence=…; bucket=…; treat as uncertain, <hedge>.)` + annotated frames | per-(user, polarity bucket) TTL `EMOTION_DEDUP_WINDOW_S` (code 300, lamp 600) (`emotion.py:632-737`) |
| `fire_hazard.detected` | `FireHazardPerception` (cloud OWLv2) | a hazard type seen continuously ≥ `FIRE_HAZARD_CONFIRM_S=10` s | `Fire hazard detected: <types>` + annotated frames | per-type TTL `FIRE_HAZARD_DEDUP_WINDOW_S=1800`; flush 10 s (`fire_hazard.py:276-360`) |
| `speech_emotion.detected` | `SpeechEmotionService` (voice area, cloud emotion2vec) | end of a mic session | `Speech emotion detected: …` | per-(user,bucket) TTL 300 s; sidecar `/tmp/hal-ser-state.json` (`speech_emotion/constants.py:40-41,70`, `service.py:66`) |

Not produced by HAL in this area: `environment.update` (os-server polls HAL and emits it; `hal/drivers/environment/service.py:1`
"no OS/agent dispatch", `robots/lamp/docs/environment-sensing.md:13-17`).

## A.4 Face detection, recognition, enrollment (all on device)

Pipeline (`faceid/pipeline.py:12-84`, `faceid/recognizer.py:153-177, 699-1081`):
- SCRFD 2.5G detector, input 640, conf 0.35, NMS 0.4 → MediaPipe FaceMesh landmark ONNX (ROI 1.4, roll-align; detections with
  landmark confidence < `HAL_FACE_LANDMARK_CONF_THRESHOLD=0.99` are dropped) → EdgeFace-S embedding (112×112).
  Models under `HAL_FACE_MODEL_PATH` (`/root/local/models`), downloaded from `https://storage.googleapis.com/autonomous-models`
  on first use (`faceid/model_store.py:11-42, 77-99`). ONNX sessions run 1 intra/1 inter thread (`recognizer.py:166-176`).
- Per-face gates before identity (`recognizer.py:770-870`): bbox height / frame height ≥ `FACE_HEIGHT_RATIO_THRESHOLD=0.10`;
  fraction of bbox off-frame ≤ `FACE_MAX_TRUNCATION=0.05`; Laplacian variance of aligned crop ≥ `FACE_MIN_SHARPNESS=100`.
- Identity decision (`recognizer.py:874-1000`), cosine on L2-normalised embeddings:
  - FRIEND if enrolled-upload similarity > `FACE_MATCH_THRESHOLD=0.40` or extended-bank similarity > `FACE_EXTENDED_THRESHOLD=0.45`;
    the name comes from the bank that authorised the match.
  - else known STRANGER if stranger-bank similarity > `FACE_STRANGER_THRESHOLD=0.45`.
  - else, if best owner similarity ≤ 0.2 (negative threshold), the face must persist `FACE_STRANGER_MIN_TICKS=2` consecutive
    ticks within `FACE_STRANGER_CORROBORATION_S=6` s before a new `stranger_N` is minted; until then UNSURE (`recognizer.py:649-668, 962-997`).
  - else UNSURE (owner similarity between 0.2 and the bar).
  - Stranger bank capped at 50 (oldest evicted), persisted under `HAL_STRANGERS_DIR` (`/root/local/strangers`) and re-read from
    disk on every frame with a face (`recognizer.py:719-720, 1083-1134`).
- Self-extension: a live view matched via the enrolled uploads with similarity > `FACE_EXTEND_MIN_ENROLL_SIM=0.45` is cropped and
  added to the user's `.extended/` bank if it is not near-duplicate (max sim ≤ 0.7) and is among the 10 most diverse views
  (`recognizer.py:271-381, 918-952, 1074-1079`; `max_extended_images=10` passed by `faceid/perception.py:87`).
- Enrollment: `POST /face/enroll` with base64 image + label (1–64 chars) + optional Telegram username/id; decodes, saves JPEG to
  `USERS_DIR/<label>/<ms>.jpg`, writes `metadata.json`, embeds EVERY face found in the image under that label; no check that
  exactly one face (or any face) is present (`hal/routes/sensing.py:259-283`, `faceid/perception.py:293-311`,
  `recognizer.py:193-234`). A watcher polls `USERS_DIR` every 2 s and reloads banks on any image mtime change
  (`faceid/perception.py:146-177`).
- Presence bookkeeping in `FacePerception._check_impl` (`faceid/perception.py:382-557`): a FRIEND/STRANGER is "new" if
  `last_seen` is None or older than its forget window; UNSURE faces never count. New friends → wellbeing `enter`; first new
  stranger of a session → wellbeing `unknown enter`. Any face present calls `presence.on_motion()` (`:457-458`).
  `last_seen` is persisted (throttled 30 s) to boot-scoped `/tmp/hal-presence-state.json`, so an in-boot HAL restart does not
  re-greet (`:45-56, 585-633`).
- Stranger greeting gate (`faceid/perception.py:493-506, 912-973`; `faceid/stranger_gaze.py:67-94`): a stranger that arrives
  without a new friend in the same frame is held "ungreeted" until they face the lamp on ≥ `FACE_STRANGER_GAZE_MIN_FACING=2` of the
  last `FACE_STRANGER_GAZE_TICKS=3` face-ID ticks younger than `FACE_STRANGER_GAZE_WINDOW_S=10` s. Facing uses gaze wake's yaw cone
  and `GAZE_MIN_FACE_PX=48` (see Part B). A stranger who never looks is never announced.
- Familiar stranger: when a stranger id's visit count reaches exactly 2, the raw frame is saved to
  `<STRANGERS_DIR>/snapshots/<id>_<ms>.jpg` and the enter message gets
  `(familiar stranger <id> — seen 2 times, ask user if they want to remember this face; image saved at <path>)`
  (`faceid/perception.py:58, 466-491, 535-540, 723-744`). Visit stats in `USERS_DIR/.stranger_stats.json`.
- `current_user()`: the friend with the newest session start among friends still within 3600 s; else `"unknown"` if any stranger
  within 1800 s; else `""` (`faceid/perception.py:768-813`).

## A.5 Presence state machine (HAL decides; no agent)

`hal/drivers/sensing/presence_service.py`:
- States PRESENT / IDLE / AWAY / DISABLED (`:15-19`). Starts PRESENT if people perception is on, else DISABLED (`:25-33`).
- Evidence that resets the clock: `on_motion()` from face (any face, incl. UNSURE), motion (any detection) and emotion (any reading)
  (`faceid/perception.py:457-458`, `motion.py:443-445`, `emotion.py:497-498`); `on_activity()` from voice turns
  (`voice`, `voice_command`, `voice_followup`, `voice_agent_handled`, in `hal/drivers/voice/_internal/sensing_sender.py:91-97`)
  and gestures (`hal/drivers/button_actions.py:274,360,390`, `hal/drivers/device_tap_actions.py:30`) via
  `app_state.note_user_activity` (`hal/app_state.py:762-770`); `on_wake()` on wake from sleep (`presence_service.py:63-74`,
  `hal/app_state.py:773-781`).
- `tick()` every sensing tick (`presence_service.py:104-127`): does nothing while sleeping or while os-server guard mode is on
  (GET `/api/guard`, cached `GUARD_CHECK_INTERVAL_S=10` s, `:90-102`).
  - PRESENT → IDLE after `IDLE_TIMEOUT_S=300` s: LED dimmed to `IDLE_BRIGHTNESS=0.20` × the user/resting base colour, via
    `rgb_service.dispatch` directly (`:118-121, 168-183`); skipped if the strip should stay dark.
  - IDLE → AWAY after `AWAY_TIMEOUT_S=900` s (measured from the last evidence): `rgb_service.clear()` (LED off) and `presence.away` event
    (`:123-127, 185-205`).
  - Any evidence in IDLE/AWAY → PRESENT and `_restore_user_led()` unless sleeping or the strip should stay dark (`:76-88, 151-166`).
- HTTP: `GET /presence`, `POST /presence/enable|disable` (`hal/routes/sensing.py:227-256`).
- Camera is NOT turned off on AWAY (design "skipped", `robots/lamp/docs/camera-lifecycle.md:126`; no such call in code).

## A.6 Sound (on device; escalation and habituation)

`hal/drivers/sensing/perceptions/processors/sound.py`:
- Device: `HAL_AUDIO_SENSING_DEVICE` resolved to a sounddevice index by name (lamp `plug:device_micro1` → `device_micro1`)
  (`:43-74`, `robots/lamp/rootfs/opt/hal/.env:4`). A dedicated capture thread records 0.5 s mono int16 at 44.1 kHz per request
  and computes RMS (`:165-196`, `SOUND_SAMPLE_DURATION_S=0.5`).
- Per tick (runs only when a camera frame is pushed — see A.1), the check is skipped while: TTS is speaking or spoke < 5 s ago;
  music is streaming; the servos are actively moving; or STT produced a transcript < 15 s ago (`:199-224`). Readings older than
  10 s are ignored (`:25, 234-235`).
- If RMS ≥ `SOUND_RMS_THRESHOLD` (code 8000; lamp `.env` 2000, comment: quiet room ~10, close thunder ~755,
  `robots/lamp/rootfs/opt/hal/.env:10-12`) → `_track()` (`:117-149`):
  - habituated: drop while gaps between loud samples are < 120 s; cleared after 120 s of quiet;
  - drop if within the 180 s suppress window, or < 15 s since the last passed sample;
  - window resets after 120 s since the last passed sample;
  - occurrence 1 → send `sound` "… — occurrence 1"; occurrence 2 → counted, not sent; occurrence ≥ 3 → send "… — persistent
    (occurrence 3)", then suppress 180 s and habituate (`:18-21, 241-274`).
- Every decision is mirrored to `POST /api/monitor/event` as `sound_tracker` (`:104-115`).

## A.7 Motion / activity recognition (cloud; opt-in)

`hal/drivers/sensing/perceptions/processors/motion.py`:
- Enabled only if os-server `config.json` `cloud_perception.motion == true` (env deliberately ignored)
  (`hal/config.py:240-251`); toggled live via `POST /sensing/cloud-perception` (`hal/routes/sensing.py:206-224`,
  `orchestrator.py:173-238`).
- WebSocket to `<llm_base_url>/ws/hal/api/dl/action-analysis/ws`, key exchange (RSA+AES), config message with the 30-label whitelist
  (`resources/white_list.txt`), threshold `MOTION_CONFIDENCE_THRESHOLD` (code 0.3, lamp 0.4), person detection on with
  min area 0.25 (`motion.py:120-187`, `hal/config.py:229-230, 257-272`). Heartbeat every 60 s; recv timeout 15 s drops the session
  and logs to `/root/local/dl_ws_stall.log` (`motion.py:193-280`).
- Every tick (unless presence is AWAY) the full frame is sent (`motion.py:417-426`). Detections call `presence.on_motion()`
  and are buffered with a local annotated snapshot (`:436-452`).
- Flush every `MOTION_FLUSH_S=10` s (`:507-521`): map labels with `ACTIVITY_GROUP` (`:44-78`): drink, break, celebrate → group
  name; sedentary (using computer, writing, texting, reading, drawing, playing controller), eat (eating *, dining, tasting food)
  and tired (yawning) → raw label (reading book/newspaper → "reading"); emotional (laughing, crying, singing) → dropped (`:526-541`).
- Gates in order (`:543-702`): presence must be PRESENT; sedentary opens the pose window (`pose.start_window()`); if the pose
  window is complete and the user is still sedentary and `bad_ratio ≥ POSE_BAD_RATIO=0.6`, posture markers are appended
  (`[computer_streak_min]`, `[posture_summary: {...}]`, `[pose_bucket]`, `[pose_worst]`) and dedup/cooldown are bypassed; then a
  same-coarse-class cooldown `MOTION_EVENT_COOLDOWN_S=900` s, bypassed on a class change once `MOTION_TRANSITION_MIN_GAP_S=60` s
  have passed; then a per-(current_user, label set) dedup of 300 s (hard-coded `_dedup_window_s=300.0`, `:334`). Dedup state is in
  boot-scoped `/tmp/hal-motion-state.json` (`:34, 347-391`).
- On send: wellbeing rows per label, then `motion.activity` with the LAST buffered raw frame attached (`:697-702`).

## A.8 Facial emotion (cloud; opt-in)

`hal/drivers/sensing/perceptions/processors/emotion.py`, `emotion_gating.py`:
- Enabled only by `cloud_perception.emotion` (`hal/config.py:279`).
- For every face in every face-ID result (including UNSURE faces), crop (`emotion_box` from the face mesh, else bbox) and
  `POST <llm_base_url>/hal/api/dl/emotion-recognize` with `raw: true`, timeout 10 s; HTTP 429 pauses recognition 300 s
  (`emotion.py:232-378, 455-558`).
- Device gate (`emotion_gating.py:9-51`): argmax must clear its per-label bar (happy 0.5, surprise 0.6, sad 0.8, anger 0.8,
  disgust 0.7, fear 0.5; override `HAL_EMOTION_LABEL_THRESHOLDS`), else Neutral stands in; result must clear
  `EMOTION_CONFIDENCE_THRESHOLD` (0.5).
- Occupancy (`emotion.py:220-229, 590-695`): readings older than 10 s dropped; "Happy" qualifies on one frame; other labels need a
  strict majority of that person's attempts (including empty ones) in the last 10 s and ≥ 2 readings. Neutral never fires.
- Skip if presence ≠ PRESENT or `current_user` is empty (`:617-630`).
- Dedup per (current_user, bucket) where bucket ∈ {positive: Happy, Surprise; negative: Sad, Fear, Anger, Disgust, Contempt}
  (`:49-57, 700-727`), TTL `EMOTION_DEDUP_WINDOW_S` (code 300, lamp `.env` 600), persisted to `/tmp/hal-emotion-state.json`.
- Each reading also calls `presence.on_motion()` (`:497-498`).

## A.9 Pose / ergonomics (cloud; opt-in; never emits its own event)

`hal/drivers/sensing/perceptions/processors/pose.py`:
- Enabled only by `cloud_perception.pose` (`hal/config.py:329`). WS `<llm_base_url>/ws/hal/api/dl/pose-estimation/ws`.
- Only while presence is PRESENT (else window reset) and at most one upload per `POSE_SAMPLE_INTERVAL_S` (code default 30 s)
  (`pose.py:632-697`). Each sample is appended to `/tmp/hal-sensing-snapshots/sensing_pose/samples_<day>.jsonl` with an annotated
  snapshot in the window bucket.
- A sample is "bad" if `risk_level ≥ 2` or any region sub-score ≥ `POSE_REGION_HIGH_SUBSCORE=4` (`pose.py:741-757`).
- Window: opened by the first sedentary flush, length `POSE_WINDOW_DURATION_S=600` s, needs ≥ 3 samples
  (`pose.py:699-810`). The result is only folded into `motion.activity` (A.7). Sign of flexion angles flipped by default
  (`POSE_FLIP_DLBACKEND_ANGLE_SIGN=true`, `hal/config.py:353-355`).

## A.10 Fire hazard (cloud; opt-in)

`hal/drivers/sensing/perceptions/processors/fire_hazard.py`: enabled only by `cloud_perception.fire_hazard`; full frame to
`<llm_base_url>/detect/owlv2` at most every `FIRE_HAZARD_CHECK_INTERVAL_S=5` s, no presence gate; objects ≥ 0.3 confidence classed
via `resources/{fire,smoke,flammable,safe}_classes.txt`; fire boxes overlapping flammable vs safe objects (> 0.2) become
HAZARD_FIRE / SAFE_FIRE / UNSURE_FIRE, smoke → SMOKE (`:83-245`); a type must persist 10 s; per-type TTL 1800 s (`:276-360`).
All confirmed types are sent, including SAFE_FIRE.

## A.11 Light level (on device)

Mean grey level of the camera frame every 300 s; event if |Δ| ≥ 100 (`light_level.py:35-66`). Uses the camera, so it does not run when
the camera is off.

## A.12 Speech emotion recognition (cloud; voice area — interface only)

- Service in `hal/drivers/voice/speech_emotion/` (voice agent's area). Initialised when the device has the `audio` capability and
  `HAL_SPEECH_EMOTION_ENABLED` (default `true`) (`hal/drivers/voice/_internal/speaker_decorate.py:132-147, 207-225`). It is NOT
  gated by the `cloud_perception` owner opt-in that gates the camera perceptions (`hal/config.py:240-248` covers only motion,
  emotion, pose, fire_hazard).
- Submitted at the end of a mic session with the speaker-ID user, or `"unknown"` (unknown speaker, failed recognize, no
  transcript, or a wake-gate-rejected turn) (`speaker_decorate.py:486-528, 551-576`, `voice_service.py:3522-3523`). `submit()`
  drops only empty users, clips < 3 s, or when every bucket for that user is still inside the dedup window
  (`speech_emotion/service.py:182-258`). Per-label thresholds happy 0.5, surprised 0.6, sad 0.7, angry/fearful/disgusted 0.6
  (`constants.py:29-37`). Flush 10 s; per-(user,bucket) TTL 300 s; posts directly to `/api/sensing/event` (not via
  `SensingService._send_event`, so HAL's sleep gate does not apply).

## A.13 Camera lifecycle (on device)

- Driver: ROBOT.md `vision.driver: opencv` → `LocalVideoCaptureDevice` (`hal/drivers/camera/factory.py:10-58`,
  `hal/runtime.py:123-151`). Device chosen by `HAL_CAMERA_NAME` (lamp `/dev/device-camera`, a udev symlink) else index
  (`hal/drivers/camera/video_capture_device.py:28-99`, `robots/lamp/rootfs/opt/hal/.env:35-38`).
- Capture loop (`video_capture_device.py:489-680`): with no registered consumer it reads one frame every 2.0 s (`idle_interval`);
  consumers (`/camera/stream`, `capture_still`, tracking) get full FPS (`:545-566, 681-690`). Frame-freeze (identical
  signature 10 s) and colour-corruption watchdogs reopen the device, escalating to a USB unbind/bind power cycle (at most one per
  10 min) (`:300-361, 585-630`). Digital zoom 1.0–5.0 applied in-loop for every consumer (`:655-665`, `hal/models.py:288-289`).
- State: `_camera_disabled` + `_camera_manual_override`, persisted (`hal/app_state.py:90-91, 650-690`).
  - Manual: `POST /camera/disable` sets override; `POST /camera/enable` is refused (403) when the override is set unless the request
    carries `X-OS-Owner-Action: 1` (set by os-server's proxy for a logged-in owner) or a voice transcript arrived < 60 s ago
    (`hal/routes/camera.py:73-140`).
  - Automatic: `_auto_camera_off/_on` skip under manual override or privacy mute; auto-off also skips while tracking
    (`hal/app_state.py:1429-1470`). Called by emotion presets with `"camera": "off"` (only `sleepy`) / `"on"` (most others),
    unless an active scene holds it off (`hal/routes/emotion.py:296-307`, `hal/presets.py:122-161`), and by scenes
    (reading, focus, movie, night → off; relax, energize → on; `scene off` → on) (`hal/routes/scene.py:124-128, 185-192`,
    `hal/presets.py:166-177`).
  - Privacy switch: forces off, restores previous state on release (`hal/privacy.py:33-66`), camera wrapped in `GuardedCamera`
    when the board's privacy button config has `disable_camera_on_mute` (`hal/runtime.py:405-406`, `hal/privacy.py:88-117`).
  - `GET /camera/snapshot` may start an auto-paused camera for one frame; 409 when manually or privacy disabled
    (`hal/routes/camera.py:143-223`, `hal/app_state.py:1420-1426`).

## A.14 Environment sensors (Pro only; on device; no events)

- `create_environment_group` builds one worker per component (SEN55, SCD41, SEN63C); a component is enabled only by its board JSON
  `enabled` flag (`hal/drivers/environment/group.py:13-28`, `registry.py:35-39`). Base lamp JSONs set all three `enabled: false`
  (`robots/lamp/sen55.json`, `scd41.json`, `sen63c.json`); Pro overlays set SEN63C `enabled: true` on I2C bus 0, poll 1 s,
  stale after 5 s (`robots/lamp/overrides/*/device/sen63c.json`).
- Workers poll and store samples; no events (`hal/drivers/environment/service.py:1, 61-96`). Read via `GET /environment/status`
  and `/environment/sample` (503 when stale) (`hal/routes/environment.py:10-22`). Overlapping metrics from two enabled components
  raise at boot (`group.py:39-48`).

## A.15 On device vs cloud summary

| Perception | Where | Owner opt-in | Emits |
|---|---|---|---|
| Face detect/align/embed, identity, stranger minting, enrollment | device (ONNX) | `presence` capability | presence.enter/leave, wellbeing rows |
| Presence state machine | device | `presence` capability | presence.away, LED dim/off |
| Stranger gaze vote | device | — | gates presence.enter |
| Light level | device (camera mean) | — | light.level |
| Sound level | device (sensing mic RMS) | — | sound |
| Environment (SEN63C) | device (I2C) | Pro overlay | none (polled) |
| Action recognition | cloud WS | `cloud_perception.motion` | motion.activity |
| Per-face action | cloud WS | motion + `HAL_MOTION_PER_FACE_ENABLED` | motion.activity |
| Facial emotion | cloud HTTP | `cloud_perception.emotion` | emotion.detected |
| Pose/RULA | cloud WS | `cloud_perception.pose` | folded into motion.activity |
| Fire hazard (OWLv2) | cloud HTTP | `cloud_perception.fire_hazard` | fire_hazard.detected |
| Speech emotion (emotion2vec) | cloud HTTP | none beyond `HAL_SPEECH_EMOTION_ENABLED=true` | speech_emotion.detected |

Cloud base URL is os-server `config.json` `llm_base_url` (else `DL_BACKEND_URL` env), key `llm_api_key`, sent as `X-API-Key`
(`hal/config.py:210-237`). With `HAL_DL_ENCRYPTION=true` (default; lamp `true`) every request is RSA+AES wrapped and is skipped,
never sent in plaintext, when no public key is available (`hal/drivers/sensing/crypto.py:160-200`, `hal/config.py:224`).

---

# Part B. Tracking, gaze and search

Summary: HAL never starts object tracking on its own (only `POST /servo/track`). The gaze watcher (wake-by-looking, framing
corrections, speech-triggered turn to the remembered user position, glance) runs only when wakeword is on. Both
absence-triggered behaviours (12 s repoint, 30 s sweep) are unreachable; the lamp only turns toward the remembered position and
glances after someone speaks.

## B.1 Gaze watcher (`hal/drivers/tracking/gaze.py`)

- Start: called from `hal/runtime.py:740-745`, inside the block that needs `LOOK_AIM_ENABLED` and a mounted camera
  (`hal/runtime.py:719`). `gaze.start()` returns unless `GAZE_WAKE_ENABLED` (`gaze.py:1520`) and `WAKEWORD_ENABLED`
  (`gaze.py:1522-1524`) — i.e. `voice_input_mode == automatic` and config.json `wakeword` is true (`hal/config.py:578-583`).
  Lamp `ROBOT.md:20` declares `wakeword: true`, adopted only for a freshly created config (`system/server/server.go:297-303`, per
  fork; older configs pinned false at `system/server/config/config.go:446-451`). Value on a given device: unverified.
- Code defaults `HAL_GAZE_WAKE=false`, `HAL_GAZE_SHADOW=true` (`hal/config.py:824-830`); lamp `true`/`false`
  (`robots/lamp/rootfs/opt/hal/.env:92-93`).
- Loop: 10 s start delay (`gaze.py:1464`), samples at 1/6 s (`gaze.py:1458`, `config.py:867`; comment says ~2 samples/s achieved,
  `config.py:856`); holds a camera consumer for its life, so the camera runs at full FPS while the watcher runs (`gaze.py:1476`).
- Sampling (`gaze.py:634-723`): skips when the camera is disabled, no camera/animation service, a servo write < 0.35 s ago
  (`aim.py:105`) unless idle is "breathing" or the body is owned, or the detector lock is busy. YuNet with landmarks on a frame
  downscaled to 640 px wide (`gaze.py:671-674`, `tracking/constants.py:7`); faces ≥ max(48 px, 0.15×frame height); nearest-centre
  wins (`detection.py:273-285`). Head yaw from eyes+nose, ≤ 90° (`gaze.py:190-213`); landmarks off-frame = "not measured"
  (`gaze.py:216-230`). With no face, local YOLO person: box touching the top and ≥ 15% of height = "head above frame";
  box ≥ 20% of area stamps "someone near" (`gaze.py:356-372, 607-616`). "Well framed" = face ≥ 48 px within 0.6 of centre
  (`gaze.py:710-713`).
- Wake vote (`_check_speech`, `gaze.py:839-922`), called by voice when VAD confirms speech (`voice_service.py:1252`) and at speech
  end (`:2994`, `:3176`): samples from the last 1.5 s (newest must be inside), measured samples only (`gaze.py:287-318`). Facing =
  yaw within a cone that widens toward the frame edge ×1.8 (`gaze.py:233-253`, `config.py:861-863`): code 25° centre/45° edge;
  lamp 60°/108°. Gate opens with ≥ 2 measured samples and ≥ 60% facing (`gaze.py:858-861`), 5 s cooldown (`gaze.py:866`); shadow
  mode only logs (`gaze.py:899-900`). A grant opens the wake follow-up window for `GAZE_WAKE_FOCUS_S=10` s, capped by the follow-up
  timeout (lamp `HAL_WAKEWORD_FOLLOWUP_TIMEOUT_S=5`, `.env:81`, unless config.json `wake_followup_s` overrides) (`gaze.py:902-919`,
  `voice_service.py:397-408`), and shows the listening cue (`voice_service.py:1255-1260`). No identity check: the face-ID name is
  only logged (`gaze.py:862-863`).
- Fewer than 2 measured samples at speech start → request a turn to the remembered user position (`gaze.py:888-897`); a user
  measured facing away never triggers it. At speech end it votes once more and hands the body back to idle (`gaze.py:941-959`).
- Framing moves (only while a conversation window is open; body must be free: no tracking, music, or servo hold):
  - Tilt (`_maybe_pitch`, `gaze.py:1011-1162`): median vertical offset over 6 s (≥ 8 samples spanning 4.8 s) > 0.15 of height,
    ≥ 4 s since last correction (`gaze.py:1037, 1062-1074`). Step = offset×45°, capped ±15°, split 0.2/0.6/0.2 over three pitch
    joints, lean-back limit 16 mm (`servo_follow.py:38-59`). Torso-only → climb 15°/step, max 4, then return to last face height
    (`gaze.py:973-1008, 1042-1089`). Move 1.0 s (stretched by safety policy) under body ownership (`gaze.py:1108-1117`); landing
    read-back ≤ 1.8 s, tolerance 2°, stalled joint rested 60 s and retried 2° short (`gaze.py:398-451`). Saves
    `/var/lib/hal/face_height.json` and an annotated snapshot.
  - Pan (`_maybe_yaw`, `gaze.py:726-824`): median horizontal offset over 12 s (8 samples spanning 9.6 s) > 0.10; 4 s cooldown
    (reuses tilt's, `gaze.py:731`); step offset×40°, cap ±12°, split 0.75 base / 0.25 wrist roll, over 1.0 s.
  - Gaze does not hand the body back to idle after a tilt/pan correction (`move_and_hold` drops the playing recording,
    `animation_service.py:829-841`); what restarts idle afterwards is unverified.
- `HAL_PRESENCE_WAKE_STRANGERS` (false code and lamp) is a separate path: face-ID `presence.enter` opens the wake window only for a
  newly visible friend (A.2). The stranger greeting gate reuses gaze's cone and size floor (`stranger_gaze.py:79,87`), so the lamp's
  60° also loosens stranger greetings.

## B.2 Object tracking (`hal/drivers/tracking/tracker_service.py`)

- Exists only when servo and camera are both mounted (`hal/runtime.py:756-762`). Only starter: `POST /servo/track`
  (`hal/routes/servo.py:583-615`), driven by agent skill markers (`skills/servo-tracking/SKILL.md:14`), the sensing skill on
  arrival (aim at user, then track `["face"]` for 3 s, `skills/sensing/SKILL.md:76-78`), and os-server intent rules
  (`system/intent/rules_tracking.go:32`).
- Stops: session limit min(request `max_duration_s`, `HAL_TRACKING_MAX_DURATION_S` code 10 / lamp 10) (`tracker_service.py:111-112,
  436-438`); confidence < 0.15 on 8 of last 15 frames (`:454-464`); box < 0.3% of frame (`:515-518`); box > 95% with no detector
  confirmation 3 s → retry then stop (`:600-606`); 20 s without detector confirmation (`:607-610`, never reached on lamp because of
  the 10 s cap); 30 consecutive misses → up to 4 retries then stop (`:370-377, 497-500`); `/servo/track/stop`, `/servo/stop`,
  `/servo/release`, a new start (`servo.py:376-379, 399-402, 618-625`; `tracker_service.py:129`); single click on button/MPR121
  (`button_actions.py:235-256, 278`; `device_tap_actions.py:31`); overheat, overload, contact stop, shutdown
  (`hal/runtime.py:906-907, 1073-1074, 1095-1096, 1115-1116`).
- Detection (`detection.py`): face → YuNet threshold 0.35 (`:220-246, 333-336`); COCO object → local YOLOv8n size 448, conf ≥ 0.15
  (0.35 for phone/mouse/remote), rejected if a higher-confidence other-class box overlaps (`:338-376`); anything else → remote
  YOLOWorld `{DL_BACKEND_URL}/detect/yoloworld`, 10 s timeout, encrypted when possible (`:433-547`). The "no remote fallback" flag
  and 2 s throttle apply only to COCO objects (`:378-387`); a face YuNet misses always goes remote, unthrottled (`:391-394`), which
  covers redetects every 1.5 s, the bearing sampler, look-aim and search fallbacks.
- Following: OpenCV TrackerVit (fallback CSRT, KCF, MIL) (`vit_tracker.py:18-40`); start freezes the camera and waits ≤ 1.5 s for a
  frame ≥ 0.3 s after the last servo move (`tracker_service.py:145-177`); vision loop 15 fps with smoothing, 0.2 s lead, deadband,
  small PID + velocity feed-forward capped 5°/step (`:537-664`, `tracking/constants.py:165-177`); fast profile 100°/s for offsets
  > 22% of width, else smooth 55°/s, both capped by the safety policy (`:567-580`). Holds instead of chasing when the box balloons,
  target centred and still, confidence < 0.25, or unconfirmed and < 0.4 (`:611-641`). Background redetect every 1.5 s, forced near
  the edge or on first miss; resets tracker only on clear disagreement, ≤ every 0.5 s (`:667-756`). Only `base_yaw` turns
  (`servo_follow.py:221`). Servo registers: velocity unlimited, acceleration 30 at start, 254 at end (`tracker_service.py:314-324,
  805-811`). Own mini-search: coast ≤ 6 frames, then wiggle `base_yaw` ±2°/frame, reversing every 8 frames (`:479-496`).
- Body sharing: claims a `TRACKING` hold (`hold.py:38-46`, `tracker_service.py:309`), sets the tracking flag, follower takes a
  body-owner count (`servo_follow.py:267-276`); "tracking active" = flag or any body owner (`animation_service.py:215-222`). While
  active: playing recording dropped (`animation_service.py:557-569`), `/servo/play` returns "ignored" (`servo.py:199-201`), emotion
  servo moves suppressed but LED still changes (`hal/routes/emotion.py:230-252`). Aim, search and gaze moves take a body-owner count
  for the move (`aim.py:281-305`). Stuck flag with no owner for 30 s is cleared with an ERROR (`tracking_wedge.py:6, 26-42`;
  `animation_service.py:406-413`). Camera freeze is reference-counted (`freeze_lease.py:18-46`). On end: registers restored, idle
  played unless someone holds the body (`tracker_service.py:792-832`).
- Supporting state: `user_bearing` = remembered direction + full pose (not sound direction); each sighting moves the estimate 25%
  (sightings < 30 s apart ignored); > 45° outliers only 5%, three in a row = lamp moved; confidence full at 8 sightings; dropped after
  3 misses within 24 h, a calibration change, or an old file version (v1/v2 dropped) (`user_bearing.py:18, 67-80, 98-129, 178-242,
  264-295`). `bearing_sampler`: every 300 s after 60 s, faces ≥ 15% of height near centre, never moves the lamp, skips while tracking
  or camera off (`bearing_sampler.py:79-86, 104-157, 173-183`). `user_check`: confirm needs face ≥ 15% of height; adopt a new bearing
  also needs centred and yaw ≤ 25° in ≥ 40% of ~1.25 s of samples (`user_check.py:45-108, 131-182`).

## B.3 Search

| Path | Trigger | Pattern | Status |
|---|---|---|---|
| `POST /servo/search` | agent request | 3 base positions (remembered, +90°, −90°) × 6 head looks = 18 (27 for a full people survey); blocks until done; base slowed; waits for arm to stop + 0.35 s before each frame; on find centres, straightens, saves frame, returns body to idle after 8 s; on miss returns to start | live (`servo.py:496-518`; `search.py:48-67, 96-110, 160-186, 426-625`) |
| Look-aim fallback | realtime `look` tool finds nobody and cannot use the remembered position | glance head left 45°, then right 45° | live (`aim.py:230-245, 808-827`); look-aim order person → hold if just seen → remembered position → glance → give up, 8 s deadline (`aim.py:759-896`) |
| Speech-triggered turn | < 2 measured gaze samples at speech start | move to remembered pose in 0.25 s (`aim.py:594-620`); declines if a face or near body seen in last 3 s, < 2 sightings, climb pending, or last turn missed < 60 s ago; after 6 s judges hit / climb (≤ 20 s) / near body (unscored) / miss | live (`gaze.py:1165-1280, 1308-1376`) |
| Gaze glance | a speech-triggered turn missed | same glance, checks the face is the user; cooldown 15 min (2 min with no bearing); skipped while held | live (`gaze.py:1283-1300, 1379-1433`) |
| Sweep after 30 s alone | — | — | unreachable: only caller passes `confirmed_miss=True` (`gaze.py:1300`); loop comment says neither sweep nor repoint is driven from the loop any more (`gaze.py:1506-1508`) |
| Turn after 12 s alone | — | — | unreachable: only caller forces it from speech (`gaze.py:967`) |

## B.4 Events from this area

- To os-server only spoken-filler requests: `POST /api/sensing/filler` (pool name, 1 s timeout, fire-and-forget) (`aim.py:308-323`,
  `hal/config.py:74`): `look_searching` once per aim; `look_found` / `look_lost` only after a search was announced;
  `look_still_searching` once halfway through `/servo/search` (`search.py:411-423`); `look_capturing` only when the aim moved
  (`hal/realtime/orchestrator.py:1792-1797`).
- Gaze, tracking and bearing sampler send no sensing events; gaze opens the voice window in-process. Snapshots on disk:
  `sensing_gaze/` (keep 40), `sensing_bearing/` (keep 30), search finds pool capped at 20 (`hal/app_state.py:315-316`).
- To the cloud: YOLOWorld detection requests.

## B.5 Notes from this area

- Lamp `HAL_GAZE_MAX_YAW_DEG=60` with edge scale 1.8: the cone reaches 90° once a face is ~62% from centre to edge, and measured yaw
  can never exceed 90° (`gaze.py:209-213, 233-237`), so almost any off-centre head counts as facing (arithmetic, not device-tested).
  Config comment says the cone reached 103°; with 60×1.8 it is 108° (`config.py:846-847`).
- A tilt correction or turn clears the wake-vote samples (`gaze.py:274-279, 1123, 1255`); someone speaking right after may look
  "blind" and trigger a turn (from code; not device-checked).
- Possible detector race: tracker redetect thread does not take the detector lock while sharing YuNet/YOLO models that `aim.py`
  says are unsafe concurrently (`aim.py:47-51`, `detection.py:86-90`, `tracker_service.py:408-415`) — unverified.
- `/servo/search` does not check sleep or servo holds (`servo.py:496-518`). `/servo/track/update` with no box likely 500s
  (`servo.py:651`, `hal/models.py:700`) — untested.
---

# Part D. Physical controls, sleep, privacy, speaker/mic lifecycle, board profile

## D.1 Input hardware the lamp declares (OrangePi sun60; Pi 4/5 entries have only the button)

- Primary GPIO button: chip 0 line 99, debounce 200 ms, `factory_reset: false` (`robots/lamp/gpio_button.json:13-22`).
- Reset button: chip 0 line 100, factory-reset only, hold 5 s (`robots/lamp/gpio_button.json:23-30`).
- Privacy slide switch: chip 1 line 9, settle 0.06 s, LOW = muted, watchdog 30 s, `disable_camera_on_mute: true`,
  `mute_speaker_on_mute: true` (`robots/lamp/privacy_button.json:3-11`).
- TTP223 head pad: chip 0, candidate lines `[37, 96, 97, 98]` with `detect: true` (`robots/lamp/ttp223.json`); how many are wired
  per unit is unverified.
- MPR121 touch strip: i2c-0, 0x5A, 12 electrodes, thresholds 6/4, debounce 10 ms, chip debounce 2, ≥ 3 pads per tap, swipe axis
  E11→E0, hold ≥ 2 s toggles voice input mode (`robots/lamp/mpr121.json`); absent chip only warns (`mpr121.py:570-589`,
  `hal/runtime.py:782-795`); presence on every unit unverified.
- `hal/board/boards.json:33-37` fallbacks (used only if the lamp JSON is missing) point the button at chip 1/line 9 (the privacy pin)
  and touch at lines [96, 100] (includes the reset line).
- Handlers start in `hal/runtime.py` (GPIO 764-779, MPR121 781-795, TTP223 797-804, privacy 806-813); sim/host boards skip them
  (`hal/runtime.py:1142-1181`).

## D.2 Gestures and what HAL does locally (no agent)

GPIO button (`hal/drivers/gpio_button.py`, `hal/drivers/button_actions.py`):
- Debounce applied to press and release edges separately (`gpio_button.py:151-158`). Holds act on release by measured duration
  (`gpio_button.py:194-211`; `button_actions.py:479-499`): > 2 s sleep, > 5 s shutdown, > 10 s factory reset (disabled on the lamp
  primary, so it falls to shutdown). Hold LED tiers painted at `Priority.HIGH`, 2 Hz blink (`button_actions.py:571-585`;
  `hal/presets.py:231-238`; lamp colours `robots/lamp/presets.json:56-62`). Reset button acts only on release after ≥ 5 s
  (`gpio_button.py:213-216`).
- Short taps, automatic mode: first tap runs `single_click_action` immediately; when the 0.4 s window closes, 3 taps reboot, other
  counts do nothing more (`gpio_button.py:231-266`). Tap-to-talk mode: every tap calls `physical_short_tap`
  (`gpio_button.py:218-229`). Spoken "Listening" cue is commented out (`button_actions.py:222-232`).
- `single_click_action` (`button_actions.py:271-333`): stops look-aim, search and tracking (even when privacy-muted, `:278`); if
  privacy-muted stop there; `POST /api/agent/speech/cancel` (no wait); 3 s guard suppressing `/audio/play`
  (`hal/app_state.py:289-305`) and stop music; if asleep wake by expressing `stretching`; unmute speaker; if mic muted unmute it, else
  stop TTS; grant wake-word focus; ~120 ms chime; presence `on_activity`.
- `physical_short_tap` (tap-to-talk, `device_tap_actions.py:17-72`): ignored if Harness state unavailable; asleep → wake + chime;
  TTS speaking → interrupt; capture active → finish it; else suppress older replies (`before_ms`), stop music, unmute, start capture.

TTP223 head pad (`hal/drivers/ttp223.py`): purely local. 0.2 s inter-contact, 1.2 s decision window, pet after 2 sessions, 1.5 s
cooldown (extends while touching), 0.5 s settle, swipe gaps 35–150 ms (`ttp223.py:23-59`). Every classification (tap, double tap,
swipe, pet) calls `head_pat_action` (`ttp223.py:413-429, 459-497`); first contact plays a soft pet chime (`:431-442`).
`head_pat_action` (`button_actions.py:358-374`) speaks a random phrase without interrupting (42 per language, `hal/i18n.py:162`),
skipped if TTS busy; reports `touch.head_pat` to os-server at most once per 60 s with a batched count (`button_actions.py:37-66`).
The handler has no `stop()` and is not stopped at shutdown (`hal/runtime.py:877-883`).

MPR121 strip (`hal/drivers/mpr121.py`), Harness OFF: tap → `physical_short_tap` (→ `single_click_action` in automatic mode);
triple-tap and hold-to-sleep/shutdown disabled (`:410-419, 730-735`); hold ≥ 2 s → `POST /api/device/voice-input-mode/toggle`, then
chime and spoken new mode, blocked while asleep, enrolling, privacy-locked, or Harness unknown/ON (`voice_mode_gesture.py:16-55`), cyan
arming light during hold; swipe +1 (right-to-left) → sleep, −1 → toggle Harness voice (`mpr121.py:688-698`); swipe needs ≥ 3 positions,
≥ 30 ms travel, ≤ 150 ms pad gaps, 120 ms release grace (`:182-186, 247-264`); action queue ≤ 2, hold/triple/swipe dropped if worker busy
or > 0.4 s stale (`:661, 750`). Harness state polled every 0.25 s; if unavailable every touch is ignored (`:640-641`;
`hal/drivers/harness/gestures.py:53-55`). Harness ON has its own mapping (tap capture/interrupt, 2 s hold disables Harness, swipes step
focus) (`harness/gestures.py:15-138`).

## D.3 Sleep

- Entry: GPIO hold 2–5 s; MPR121 swipe right-to-left; `POST /emotion {"emotion":"sleepy"}` (agent, web UI, or os-server).
- `sleep_action` (`button_actions.py:420-444`) is refused if `harness_blocks_sleep()`: Harness ON or os-server's Harness state
  unavailable within 0.5 s (`hal/routes/emotion.py:63-67`; `hal/drivers/voice/_internal/harness_voice.py:11-32`). It starts
  "I'm going to sleep now." on its own thread, waits a fixed `HAL_SLEEP_ANNOUNCEMENT_DELAY_S` (2.0 s, `hal/config.py:49`, lamp
  `.env:136`), then expresses `sleepy`.
- `sleepy` effects: sleep sidecar + journal; speaker muted and TTS stopped immediately, LED cleared, mic muted (`voice_service.stop`),
  music stopped; only the mutes sleep set are owned by sleep (`hal/app_state.py:517-545, 608-624`); camera auto-off unless tracking or
  manual override (`hal/routes/emotion.py:296-298`; `hal/app_state.py:1429-1450`); servos released after 1.0 s
  (`hal/routes/emotion.py:32, 199-222`); only `greeting`, `stretching`, `sleepy` accepted while asleep (`hal/routes/emotion.py:26-29,
  130-132`); sensing drops every event except `presence.enter` (`sensing_service.py:255-257`); presence clock paused
  (`presence_service.py:109-110`). Because the camera is off, no perception at all runs during sleep (A.1).
- Wake (`greeting`/`stretching`, button tap, privacy unmute) restores only the mutes sleep set (`hal/app_state.py:627-647`) and
  restarts the presence clock. Sleep survives a HAL restart (motion service starts with `skip_wake`; `sleepy` not re-expressed)
  (`hal/runtime.py:360, 841-852`). Sleep/wake are written to a local journal only, not pushed to os-server
  (`hal/app_state.py:472-500`).

## D.4 Privacy switch (`hal/drivers/privacy_button.py`, `hal/privacy.py`)

- Boot: everything locked until the switch is read; stays locked if the GPIO claim/read fails (`hal/privacy.py:78-85`,
  `hal/runtime.py:309`).
- Edges restart a 60 ms settle timer; unchanged level ignored; 30 s watchdog reconciles pin vs stored state
  (`privacy_button.py:134-214`).
- Mute: stop TTS and music, red LED, mute mic, set `_camera_disabled` and stop camera (wrapped in `GuardedCamera`, which hides frames),
  lock speaker, saving previous states (`hal/privacy.py:25-66, 88-117`).
- Unmute: `single_click_action("privacy-switch", unmute_output=False)` — which wakes a sleeping lamp — then restore previous camera
  and speaker states, chime, listening LED (`privacy_button.py:286-320`); at boot an unmuted switch only restores access
  (`:273-285`).
- While locked, `POST /voice/unmute` (`voice.py:518-522`), `/speaker/unmute` (`music.py:276-277`), `/audio/record` and record-enroll
  return 409. No event to os-server; state visible as `hw_mic_switch_muted` in `GET /voice/status`.

## D.5 Speaker lifecycle

- Output device: `HAL_AUDIO_OUTPUT_ALSA` alias matched against PortAudio names, 20 retries × 0.5 s (`hal/runtime.py:471-506`), else
  keyword search (`hal/app_state.py:1527-1625`). Standard lamp: C-Media via a −30..0 dB softvol (`robots/lamp/rootfs/etc/asound.conf:32-48`);
  hardware level pinned at step 9 (−28 dB), pro override step 6 (`lamp-cmedia-speaker.service`, per fork).
- Volume: `POST /audio/volume` 0–100, clamped to SAFETY `max_volume` (`hal/safety/policy.py:302-306`); via PulseAudio when a BT sink is
  active, else writes every playback-only mixer control; saved to `HAL_VOLUME_STATE_PATH` (`hal/routes/audio.py:135-174`). os-server
  restores the saved value at boot, else `startup_volume` (`system/.../config_watch.go:308-318`, per fork). Lamp: base startup 75
  (`robots/lamp/ROBOT.md:46`), max 100 (`robots/lamp/SAFETY.md:17`, softvol −30..0 dB); `pro` no volume fields (capabilities only,
  `overrides/pro/profile.json`); `pro-respeaker-lite` startup 35 / max 50 (`overrides/pro-respeaker-lite/profile.json`); `pro-xvf3800`
  77 / 77 (`overrides/pro-xvf3800/profile.json`).
- Mute flag `_speaker_muted`: `/speaker/mute`, sleep, privacy switch, record-enroll, scenes reading/focus/movie/night
  (`hal/presets.py:167-178`); TTS, chimes and `/audio/play` check it.
- Arbitration: TTS takes a single lock without waiting; interruptible speech is preempted, otherwise the new speech is skipped
  (`hal/drivers/voice/tts/service.py:2049-2058`, voice area). Chimes write straight to the output stream. Music waits up to 10 s for TTS
  before starting and file playback stops when TTS speaks (`music_service.py:360-374, 416-419`); a second music request while busy → 409.
  No software ducking outside live mode. `media_owner` does not arbitrate; it hands hardware over from a vendor daemon, used only on
  Reachy, empty on the lamp (`hal/runtime.py:153-164`; `hal/drivers/media_owner/factory.py:10-15`).
- Bluetooth: HFP if `HAL_BT_PREFER_HFP` (code 0, lamp 1) and supported, else A2DP; waits ≤ 8 s for the Pulse sink, then routes TTS (and the
  voice mic under HFP) to the headset; route changes serialised by `route_op_lock`; route restored at boot (`hal/routes/bluetooth.py:116-180`;
  `hal/drivers/audio_route.py:228-299`).

## D.6 Mic lifecycle

- Voice mic `plug:device_micro2` (Jieli USB on standard/pro; ReSpeaker Lite processed channel on Lite; XVF3800 on xvf)
  (`robots/lamp/rootfs/etc/asound.conf`, overrides' `asound.conf`). VoiceService opens it at boot unless muted (`hal/runtime.py:599-606`);
  starts refused while privacy-locked or enrolling (`hal/app_state.py:503-514`). Closed by `/voice/mute`, sleep, privacy switch,
  record-enroll (stop → `arecord` → restart, `hal/routes/speaker.py:349-446`), BT route swaps.
- Sensing mic `plug:device_micro1` (C-Media capture on standard/pro; onboard ES8389 on Lite/XVF). `SoundPerception` itself checks only
  TTS (+5 s), music, servo motion and recent transcripts, not mic mute, privacy or sleep (`sound.py:198-232`). In effect, on the lamp the
  privacy switch and `sleepy` both set `_camera_disabled`, which stops the sensing frame push that drives SoundPerception (A.1), so sound
  sensing stops under privacy and sleep; under plain `/voice/mute` (camera still on) it keeps running.
- `/audio/record` uses the keyword-chosen `audio_input_device`, not the ALSA alias; which device that picks on the lamp is unverified.

## D.7 Board and profile

- Board: `/proc/device-tree/model` containing "sun60iw2" → `orangepi_sun60`, also the default; `HAL_BOARD` overrides; boot fails if
  the board is not in ROBOT.md `boards` (`hal/board/board.py:108-153`).
- Device type: `DEVICE_TYPE` env (lamp `.env:44` = `lamp`) or config.json; no lamp fallback (`hal/server_support/boot_config.py:11-26`);
  `DEVICES_DIR=/opt/devices` (`.env:45`).
- Variant: `/etc/autonomous/hardware-profile` is read at install/OTA by `scripts/provision/apply-overrides.py`, not by HAL: missing or
  `standard` → base; otherwise merge `.env`, set volumes, uncomment capabilities, replace `device/*.json`, copy rootfs, set
  `HAL_VOLUME_STATE_PATH` (`apply-overrides.py:18-25, 80-136`).
- Per variant: all Pro variants enable `environment` + SEN63C; Lite: software AEC off, Silero 0.10, live mode on; XVF3800: software AEC
  off, live mode on; `pro`: `.env` adds only `HAL_VOICE_OPENING_FILLERS_ONLY=true` (same as base) plus the speaker step-6 service.
- Unknown keys in device JSON are logged and dropped (`hal/board/config_fields.py:9-21`).

## D.8 Events to os-server from controls

| Endpoint | When / payload | Timeout |
|---|---|---|
| `POST /api/sensing/event` | `{type: "touch.head_pat", message}`, ≤ 1 per 60 s, batched count (`button_actions.py:37-66`) | 0.5 s |
| `POST /api/agent/speech/cancel` | `{}` or `{before_ms}` on every floor-taking tap (`button_actions.py:35, 86`) | 1 s |
| `POST /api/system/factory-reset` | reset hold (`button_actions.py:551`) | 3 s |
| `POST /api/device/voice-input-mode/toggle` | MPR121 hold | 30 s |
| `POST /api/harness/voice-mode/gesture`, `/focus` | Harness gestures (`hal/drivers/harness/client.py:5, 25`) | 15 s |
| `GET /api/harness/voice-mode` | polled every 0.25 s by MPR121, and on sleep / tap-to-talk | 0.5 s |

os-server tags `touch.head_pat` "[NO_REPLY unless worth saying — phrase already spoken locally.]" (`sensingmsg.go:115-117`, per fork).

## D.9 Backchannel cues (voice-owned; interface only)

`hal/drivers/voice/backchannel.py:1-60`: while an STT session is open, if the partial transcript stays unchanged for
`HAL_BACKCHANNEL_STALL_S` (8.0 s) it plays a filler word (language-specific list, `HAL_BACKCHANNEL_FILLERS`), at most every
`HAL_BACKCHANNEL_INTERVAL_S` (5.0 s; lamp `.env:29` = 5) at volume 0.5. Whether and when it is instantiated on the lamp is the voice
agent's area (unverified here).
---

# Part C. Actuators: LED strip, status LED, servos, safety, policy, presets, scenes, music

## C.1 LED strip: hardware and driver

- Lamp declares `light: {routes: [led, scene], driver: ws2812}` (`robots/lamp/ROBOT.md:28`); no `display`, no `policy`, so `/display/*` and
  `/policy/*` are not mounted (`hal/runtime.py:955-966`; always-mounted: audio, emotion, scene, system, bluetooth, `hal/runtime.py:957`).
- `orangepi_sun60` drives the strip over SPI3.0 (`hal/board/boards.json` `orangepi_sun60.led`). `RGBService` picks memory strip (sim), raw
  spidev at 6.4 MHz GRB, or rpi_ws281x PWM (`hal/drivers/rgb/rgb_service.py:83-133, 158-177`). Ring = 32 pixels (`robots/lamp/presets.json:4`;
  platform default 64, `hal/board/presets_overlay.py:22`).
- Boot: strip cleared when `RGBService` is constructed (`rgb_service.py:183-190`); HAL's early LED app stops the systemd boot indicator
  before opening SPI (`hal/server.py:29-41`, `hal/server_support/boot_led.py:7-13`); boot indicator = white breathing at levels 0–3 over 3 s,
  sun60iw2 only (`robots/lamp/rootfs/usr/local/libexec/led-boot.py:20-39, 62-67`); shutdown fallback writes a black frame 5 s after HAL
  exits (`robots/lamp/rootfs/etc/systemd/system/led-shutdown.service`, `led-off.py:22-30`).
- Brightness gate: every pixel passes `clamp_color(policy)`, which scales the brightest channel down to the ceiling preserving hue
  (`rgb_service.py:236, 261`; `hal/safety/policy.py:270-285`). Lamp ceiling 120, 40 between 22:00–07:00 device time (`robots/lamp/SAFETY.md:12-14`;
  `policy.py:244-261`). Boot/shutdown scripts bypass HAL (levels 0–3 only).

## C.2 LED arbitration

- Driver mailbox: `ServiceBase` keeps ONE pending event; a new event replaces it if priority ≥ pending, lower priority is dropped while a
  higher one is pending (`hal/drivers/base.py:37-47, 78-95`). All callers use NORMAL except button hold warnings (`Priority.HIGH`,
  `button_actions.py:571-584`). Speaking-wave frames are `GuardedFrame`s checked for ownership and ≤ 100 ms age at admission and under the
  driver lock (`hal/drivers/rgb/presentation.py:12-35`; `rgb_service.py:200-220`). One tracked effect thread; `_stop_current_effect` signals
  and joins with 2 s timeout (`hal/app_state.py:326-334`).
- Writers. HTTP: `/led/solid|paint|off|effect|status|restore`, `PUT /led/resting`, `/led/resting/preview`, `/led/wake-light/preview`
  (`hal/routes/led.py`), `/scene`, `/scene/off` (`hal/routes/scene.py:90-101, 212-216`), `/emotion` (`hal/routes/emotion.py:276`),
  `/audio/play` (emotion LED, `hal/routes/music.py:236`) and `/audio/stop` (restore, `music.py:153-184`). Internal: emotion LED
  (`hal/app_state.py:1336-1417`), TTS speaking wave (`:1158-1266`), music wave (`:1269-1333`), mic-muted indicator (`:860-919`), backend-error
  3× amber flash (`:933-978`, triggered from `hal/drivers/voice/tts/service.py:1929`), thinking repaint (`:1021-1025`), listening pending cue
  at 0.35 with 3 s timeout (`:257, 1108-1143`), Harness indicator (`hal/drivers/harness/led.py:24-36`), presence dim/off
  (`presence_service.py:168-205`), privacy unmute listening cue (`privacy_button.py:245`), MPR121 arming colour (0,180,255)
  (`voice_mode_gesture.py:100-115`), button hold warnings, sleepy finalize clear (`hal/app_state.py:528-531`). Tracking writes no LED.
- Restore ladder `_restore_user_led` (`hal/app_state.py:981-1095`), first match wins: (0) TTS speaking and awake → ignore (`:986-988`);
  (1) sleeping → clear (`:997-1002`); (2) TTS speaking → skip; (3) music playing → skip (`:1004-1010`); (4) mic-muted indicator owns the
  strip (yields only to a scene) → red breathing (`:1016-1019, 826-832`); (5) thinking cue active → repaint thinking at 0.7 (`:1022-1025`);
  (6) Harness mode → Harness indicator (`:1027-1029`); (7) saved user state (solid, paint, effect, scene) → repaint (scene: LED only, no
  re-aim) (`:1043-1093`); (8) nothing saved → ambient resting look, or clear if resting is dark (`:1032-1041`).
- Sleep gate: `/led/solid|paint|effect|restore` dropped but return 200 (`led.py:41-46, 128, 177, 236, 335`); `/led/off`, `/led/effect/stop`
  not gated; emotion LED skipped unless `sleepy`; TTS wave, music wave, mic-muted skipped (`hal/app_state.py:862, 1164, 1275, 1355-1357`).
  `POST /scene` has no sleep check and paints while asleep (`scene.py:79-101`).
- TTS gate: `/led/effect` (except speaking_wave), `/led/effect/stop` and the emotion LED are skipped while TTS speaks (`led.py:245-247, 453-455`;
  `hal/app_state.py:1358-1365`). `/led/solid|paint|off` and `/scene` are not gated; they cancel pending restore, revoke speech presentation and
  stop the effect, overriding the speaking wave (`hal/app_state.py:337-349`).
- Mic-muted indicator: while it owns the strip, transient `/led/effect` and every `/led/effect/stop` are skipped (`led.py:250-255, 457-459`);
  any non-transient user LED command dismisses it (mic stays muted) (`hal/app_state.py:913-919`).
- Background emotion guard is `{idle}` only (`hal/app_state.py:36`): idle's LED is skipped when the user has saved LED state (`:1368-1375`).
  Transient writes do not save user state; transient effects overlay on the user base (`led.py:269`); emotion effects run on black (`:1381`).
  Harness ON swallows transient indefinite `breathing` (`led.py:257-259`).
- Presence dim/off bypass the ladder: they do not stop a running effect thread or check TTS/music/mic-muted (`presence_service.py:168-205`).
- Music + emotion: an emotion during music stops the music-wave thread (`:1382`) and its own restore is then skipped while music plays
  (`:1008-1010`), so the emotion effect stays until the song ends (derived from code).
- Restore timers carry a generation number; stale timers do not repaint (`hal/app_state.py:1146-1156`).

## C.3 Effects, default look, status LED, display

- Nine effects (`hal/presets.py:34-46`; `hal/drivers/rgb/effects.py:56-369`): breathing (sine, 100 steps of 0.03/speed s), breathing_fine
  (spatial dither colour/colour−1, stride 13), candle (per-pixel 0.8–1.0 at 30 Hz), rainbow, notification_flash (3 flashes, ends black),
  blink, pulse (wavefront travelling around the ring), speaking_wave (8-segment simulated VU meter), speaking_wave_rainbow.
  `/led/effect` validates speed 0.1–5, `duration_ms` 100–60000, brightness 0–1 (`hal/models.py:115-141`); default colour (255,180,100)
  (`led.py:267`).
- Resting look: lamp solid [1,1,1] (`robots/lamp/presets.json:3`); platform default black (`hal/presets.py:245`). No HAL-side repaint of the
  resting look at boot was found (lifespan only repaints mic-muted and restores a scene, `hal/runtime.py:825-839`); who paints [1,1,1] after
  boot is unverified (led-control.md says os-server's ambient loop calls `/led/restore`).
- Status LED: no separate LED; same strip. `POST /led/status {state}` resolves `STATUS_LED_PRESETS` (`hal/presets.py:206-227`) with the lamp
  overlay (channels ≤ 3, long cues speed 0.6, `robots/lamp/presets.json:39-54`); `setup` is a persistent solid saved with
  `source:"status:setup"`; all others transient effects (`led.py:314-329`); a non-transient `/led/off` ends `status:setup` and restores
  (`led.py:211-226`). The state machine (when each state shows) is Go-side `system/statusled` (`robots/lamp/docs/status-led.md:118-136`;
  unverified here). `mic_muted` is HAL-local (`hal/app_state.py:860-867`).
- Display: `display_service` (GC9A01, 2/15 fps, blink every 4 s, `display_service.py:18-21`) is not mounted on the lamp; `set_expression`
  calls are no-ops.

## C.4 Resting LED and wake light (HAL-owned looks)

- `hal/resting_led.py`: modes `default` (device preset snapshot), `off` (black), `custom` (RGB 0–255; black → `off`) (`:21-52, 88-93`); saved with
  fsync to `HAL_RESTING_LED_PATH` (`/var/lib/hal/resting_led.json`) (`hal/config.py:894-896`; `:67-85`); `init()` runs in `boot_config()`
  and rewrites `AMBIENT_RESTING_LED` in place (`hal/server_support/boot_config.py:63-66`; `:96-110`). `PUT /led/resting` clears saved user LED
  state and repaints unless asleep (`led.py:351-367`). Preview paints without saving and reverts after 10 s; not during sleep, TTS, music or
  mic-muted (`led.py:371-420`).
- `hal/wake_light.py`: config.json `wake_light {enabled, color, effect, speed}` read live; effects solid → none, breathing → breathing_fine,
  spin → pulse, blink → blink; speed clamped 0.25–3 (`:21-28, 50-70`). Unset fields fall back to the `listening` preset: lamp [0,0,3],
  breathing_fine, speed 0.3, starting at peak (`hal/presets.py:137-143`; `robots/lamp/presets.json:26`). `enabled()` forced true when the
  wake chime is off (`wake_light.py:45-47`; `hal/config.py:183-185`). Trigger: when the wake gate opens with wake focus active, voice expresses
  `listening` (`voice_service.py:464-466`); cleared after `wake_followup_timeout_s()+0.5` if nothing follows (`:468-474`; lamp 5.5 s); with
  wake word on and wake light off the listening emotion is skipped (`:2480-2481`). Preview 3 s (`led.py:424-445`).

## C.5 Emotion presets (`POST /emotion`)

- 22 presets (`hal/presets.py:121-162`); lamp overlay sets every colour to a 0–3 palette and `thinking.servo=null`, `stretching.servo="idle"`,
  `listening.speed=0.3`, `music_strong.brightness=0.012` (`robots/lamp/presets.json:8-37`).

| Emotion | Servo (lamp) | Colour (lamp) | Effect / speed |
|---|---|---|---|
| curious | curious | [0,3,3] | candle 0.3 |
| happy | happy_wiggle | [3,3,0] | candle 0.2 |
| sad | sad | [0,1,3] | breathing 0.4 |
| thinking | none (overlay) | [2,0,3] | pulse 0.3 |
| idle | idle | [1,2,3] | breathing 0.2 |
| excited | excited | [3,1,0] | candle 0.5 |
| shy | shy | [3,0,1] | breathing 0.3 |
| shock | shock | [2,2,2] | notification_flash 1.0 |
| listening | none | [0,0,3] | breathing_fine 0.3, from peak |
| laugh | laugh | [2,3,0] | candle 0.2 |
| confused | confused | [3,2,0] | candle 0.2 |
| sleepy | sleepy | [0,0,0] | solid; camera, mic, speaker off |
| greeting | greeting | [3,0,3] | breathing 0.3 |
| goodbye | goodbye | [3,0,2] | breathing 0.5 |
| caring | nod | [3,1,2] | breathing 0.4 |
| acknowledge | acknowledge | [0,3,0] | breathing 0.5 |
| stretching | idle (overlay) | [3,2,1] | breathing 0.6 |
| music_strong | music_rock | — | rainbow, brightness 0.012 |
| music_chill | music_rock | [0,3,2] | breathing 0.3 |
| scan | scanning | [0,2,3] | pulse 0.3 |
| nod | nod | [2,3,1] | breathing 0.5 |
| headshake | headshake | [3,0,0] | breathing 0.5 |

- Validation (`hal/routes/emotion.py:103-314`): name lower/trimmed; unknown → `curious` (ignored while asleep) (`:109-122`); intensity 0–1,
  default 0.7 (`hal/models.py:299`), scales only the LED colour with integer truncation (`hal/app_state.py:1378`; rainbow ignores it). With the
  0–3 palette at 0.7: 3→2, 2→1, 1→0 (e.g. idle [1,2,3] → [0,1,2]; sad [0,1,3] → [0,0,2]).
- Sleep: while asleep only greeting, stretching, sleepy accepted (`:26-29, 130-132`); `sleepy` refused if Harness on or unreadable
  (`:63-67, 135-137`); sleep persisted and journaled (`:142-149`); `sleepy` arms auto-release (park and torque off) after 1.0 s (`:32, 199-222`);
  wake restores sleep-owned mutes and runs scene off (`:224-227`).
- `thinking` arms a stuck-thinking timer `HAL_EMOTION_THINKING_RESET_S=25` s → idle + restore (`:167-193`).
- Servo gating (`:229-272`): blocked if tracking active, or a hold is active and (hold is explicit or the emotion is not a scene change:
  greeting, sleepy, stretching). After a sleep release, `svc.resume()` runs first (`:241-246`). Still emotions (no servo: listening, and on the
  lamp thinking) call `svc.halt()` and arm an idle resume after 10 s (`STILL_IDLE_RESUME_SECONDS`; music exempt; parked under a hold)
  (`:70-87, 256-272`). So on the lamp every `thinking` cue freezes the arm for up to 10 s.
- LED: painted only if tracking is active or the servo is not blocked (`:275-276`), so an explicit hold also suppresses the emotion LED.
  Restore: shock 2 s; others authored CSV duration + 0.5 s (3.5 s without recording); none for idle, sleepy, listening (`:278-294`). Duration
  from CSV timestamps, ignoring the 2 s ramp-in, speed stretch and 3 s end hold (`hal/app_state.py:715-730`), so the LED can return to resting
  while the arm is still moving.
- Camera per preset (see A.13). No dedup, rate limit or spam guard on `POST /emotion` in HAL.
- HAL-internal emitters (call sites outside this area): `listening` (`voice_service.py:466, 2482`), `thinking` with forced 0.7 LED and `idle`
  (`hal/drivers/voice/_internal/realtime_turn.py:198-202, 268`), `idle` when a realtime reply finishes (`hal/app_state.py:1196-1213`),
  `stretching`/`sleepy` from buttons (`button_actions.py:169, 442`), model-driven emotions from the realtime orchestrator
  (`hal/realtime/orchestrator.py:1652`).

## C.6 Servos: service and playback

- `feetech` → `AnimationService` (`hal/drivers/motors/factory.py:10-15`); fps 30, 2.0 s ramp into a recording (`HAL_SERVO_PLAY_RAMP_S`), 3.0 s end
  hold (`HAL_SERVO_HOLD_S`) (`hal/runtime.py:326-355`; `hal/config.py:8-13`). Event loop pops one FIFO event per tick and advances playback one
  frame (`animation_service.py:390-417`); overload monitor 20 Hz (`:61, 1139-1181`); `bus_lock` RLock serialises bus I/O (`:141`).
- Units: lerobot `RANGE_M100_100` (−100..100 over each joint's calibrated span), not degrees (`hal/follower/hal_follower.py:53`;
  `use_degrees=False` default, `hal/follower/config_hal_follower.py:60`).
- Configure: P gains {1:16, 2:16, 3:32, 4:16, 5:32}, I gains {3:10, 5:10}; base_yaw Goal_Speed 0; Torque_Limit from servo_overload.json; torque on
  unless overload lockout or waking asleep (`animation_service.py:264-311`).
- Recordings: 29 CSVs (`timestamp, base_yaw.pos, base_pitch.pos, elbow_pitch.pos, wrist_pitch.pos, wrist_roll.pos`, 0.05 s/row): acknowledge,
  confused, curious, excited, fear, goodbye, greeting, happy_wiggle, headshake, idle, laugh, listening, music_chill, music_classical, music_groove,
  music_hiphop, music_hype, music_jazz, music_rock, music_waltz, nod, playful, sad, scanning, shock, shy, sleepy, stretching, thinking_deep
  (`hal/recordings/`). Load = stability check → per-segment time stretch so speed ≤ min(`HAL_SERVO_MAX_DPS` 250, `motion.max_speed` 120) →
  resample to 1/30 s → cache (`animation_service.py:675-719`; `recording_timing.py:13-89`). Start blends from current pose over 2 s
  (`:466-496, 572-605`). End: music loops; sleepy holds final pose (`NO_IDLE_RECORDINGS`, `:74`); any recording under a hold holds; others hold 3 s
  then blend 2 s to idle; idle loops (`:606-652`). Missing recording only logs; `/servo/play` still returns "ok" (`hal/routes/servo.py:202-206`;
  `animation_service.py:477-479`). `/servo/upload` accepts .csv ≤ 2 MB, ≤ 20,000 rows, `timestamp` + known `<joint>.pos` columns, name ≤ 64
  chars, and can overwrite shipped recordings (`servo.py:52-187`; cache invalidated `animation_service.py:994-1000`).

## C.7 Safety bounds (`hal/safety/policy.py`, lamp `robots/lamp/SAFETY.md`)

| Bound | Lamp value | Effect | Where |
|---|---|---|---|
| schema | v1 | unknown major / missing / out-of-range fails boot | `policy.py:78-98, 217-236` |
| light.max_brightness | 120 | clamp, hue kept | `policy.py:251-285` |
| light.quiet_hours | 22:00–07:00 → 40 | lower ceiling | `policy.py:244-261` |
| audio.max_volume | 100 | volume clamp | `policy.py:302-306` |
| audio.quiet_hours | not set | would suppress `/audio/play` | `policy.py:288-294`; `music.py:203-206` |
| motion.max_speed | 120 | moves and recordings stretched in time; streaming followers capped | `policy.py:309-335`; `servo.py:303-327` |
| (pose check) | — | `/servo/move` 503 when no finite current pose | `servo.py:311-317` |
| motion.stop_always | true | informational | `policy.py:44` |
| motion.max_cog_offset_mm | 22 mm (urdf/lamp.urdf) | recording past 22 mm refused/skipped; warn at 85%; `/servo/move` and `/servo/nudge` cut short by bisection | `recording_stability.py:180-273`; `servo.py:318-326`; `animation_service.py:1553-1560` |
| motion.joint_limits | yaw ±105, base_pitch [−2,67], elbow [−11,77], wrist_roll ±60, wrist_pitch [−89,57] | every `send_action` goal clamped | `hal/follower/hal_follower.py:254-262` |
| thermal.max_temp_c | 95, resume 85 | polled every 10 s; on the rising edge only stops the tracker | `policy.py:200-214, 338-360`; `hal/runtime.py:1056-1083` |

- `move_to_raw` (startup, zero, park) writes Goal_Position registers directly and bypasses the joint clamp (`animation_service.py:875-931`).
- SAFETY.md labels bounds in deg and deg/s; all comparisons run on −100..100 normalised values (whether that approximates degrees: unverified).
- Thermal blocks nothing else: emotions, idle, moves and a new `/servo/track` still run when hot.

## C.8 Overload, contact stop, torque cap (`robots/lamp/servo_overload.json`, sun60 only; none on Pi/sim/host, `hal/runtime.py:1165-1167`)

- Torque cap 60% on base_yaw, base_pitch, elbow_pitch.
- Overload cut-off: ≥ 80% load for 1.5 s → halt, block goal writes, torque off on all servos (retried until confirmed), chime, stop tracker
  (`animation_service.py:1210-1250`; `hal/runtime.py:1086-1098`; `overload.py:66-101`); lockout 120 s then `resume()` to idle (`:1430-1446`);
  during lockout move/send/move-and-hold raise, raw moves skipped, recording frames dropped at the follower (`:1088-1096`;
  `hal_follower.py:231-232`).
- Contact stop: fixed floors base_yaw 55%, wrist_roll 75%, wrist_pitch 65% for 0.05 s; during a recording with a learned envelope the floor is
  learned + 12% load or + 3.5 units lag over ±3 frames (`contact_profile.py:23, 82-96`; `animation_service.py:1267-1306`); outside playback the
  `off_playback` floors apply except during and 0.5 s after a direct move. Action: halt, pin pose with torque on, stop tracker silently
  (`:1384-1412`); after 3 s return to idle (`:1414-1428`); still pushed 1 s into the pause → overload cut-off (`:1164-1172, 1198-1208`).
- `POST /servo/contact/learn` plays each recording `runs` (1–5, default 2) times, default set excludes `music_*`, ≤ 3 attempts with 90 s wait
  (`servo.py:410-427`; `animation_service.py:1322-1382`); saved to `HAL_CONTACT_PROFILE_PATH`.
- Reported on `/health` as `servo_overload` (`hal/runtime.py:1121-1124, 1389`); no push event.

## C.9 Startup, idle, hold, sleep pose, aim, nudge, move, demo

- Startup: move to `STARTUP_RAW` over 5 s, then idle (`animation_service.py:28-34, 46, 350-355, 419-437`). Restart while asleep: `skip_wake` leaves
  torque off, no move, no idle (`hal/runtime.py:360`; `animation_service.py:325, 350-352`).
- Zero: `ZERO_RAW` (= `STARTUP_RAW`) over 2 s (`:17-23, 1012-1026`). Release: park at `REST_RAW` over 2 s then torque off; route stops the tracker
  first (`:38-44, 1028-1052`; `servo.py:370-385`). Resume: 2 s ramp into idle (`:1054-1074`). `halt()` pins pose; next commanded motion clears
  it (`:725-747`).
- Sleep pose (derived, not device-verified): the 1.0 s auto-release parks at `REST_RAW`, so the 2 s-ramped `sleepy` recording likely never plays.
- Holds: four owners scene, tracking, explicit, look (`hal/drivers/motors/hold.py:15-22`); each releases only its own (`:38-63`); stale scene hold
  dropped on read (`:88-101`); look hold expires after 30 s (`servo.py:231`). A hold suppresses return to idle, `/servo/play` ("ignored"),
  `/servo/demo`, emotion servo movement. The music dance ignores a hold (`animation_service.py:614`).
- Aim: left/right change base_yaw only; center uses the full preset; others keep current yaw (`:1493-1498`); stops the event loop, moves (abortable
  only by halt), keeps `__aim_hold__` 5 s before idle (`:1502-1540`); default 0.5 s. Nudge: relative yaw ±180, pitch ±90 on base_yaw/base_pitch,
  limited by lean-back check (`:1542-1564`). Move: `move_and_hold` drops the current recording; arm stays until the next play (`:829-856`).
- Range demo `/servo/demo`: 14 legs, one joint at a time (`range_demo.py:51-90`); yaw targets ±135 (`tracking/constants.py:119`) clamped to ±105;
  base_yaw Goal_Speed 1600 during demo; 1.0 s legs, 0.3 s dwell; narration via sensing filler URL; refused while sleeping, suppressed, or running.

## C.10 Concurrency arbitration (servos)

- FIFO event queue, one event per tick; each `play` replaces the current recording with a 2 s blend; recordings are never queued
  (`animation_service.py:381-417, 466-496`).
- Body ownership refcount (`acquire_body`/`release_body`) plus tracking flag: while set, playback drops the current recording and writes
  nothing (`:216-222, 249-259, 557-570`); users: tracker, aim, demo, search, gaze moves (`aim.py:282-305`). Stuck flag cleared after 30 s
  (`tracking_wedge.py:6`; `:406-413`). Camera freeze lease skips servo writes without advancing frames (`freeze_lease.py:18-46`; `:541-543`).
  Halt flag stops loops/moves within one frame (`:545-555, 721-723`). Direct moves and aims preempt the current recording (`:829-845, 1502-1506`).
- Emotion vs tracking: servo suppressed, LED still updates. Emotion vs music dance: emotion replaces the dance, dance resumes after it
  (`:632-635`). Overload/contact block goal writes at the follower.
- Sleep gating: `/servo/play`, `/servo/demo` → "ignored"; `/servo/move|aim|nudge|track|resume` → 409; `/servo/hold|zero|release|stop|search`,
  `/servo/contact/learn` → not gated.

## C.11 Learned motion policy

`hal/policy/service.py:35-73` `LoggingPolicyService` records a dry run only ("no inference or motor command issued"); created only if ROBOT.md
declares `policy` (`hal/runtime.py:1003-1006`). The lamp does not, so `state.policy_service` is None and `/policy/*` is not mounted.
`/servo/stop` calls policy stop only if a service exists (`servo.py:391-396`).

## C.12 Scenes and music

- `POST /scene` (`hal/routes/scene.py:79-164`): one of 6 scenes; paints colour × brightness (lamp: energize 0.24, reading 0.19, focus 0.15, relax
  0.10, movie 0.04, night 0.012, `robots/lamp/presets.json:64-71`); saves scene as user LED state, persisted boot-scoped; aims in a thread
  (desk/wall/down/up) then claims a scene hold for reading and focus (`hal/presets.py:166-177`); camera/mic/speaker per preset (reading, focus,
  movie, night: camera off, speaker off; mic on for all); speaker-off stops music now and drains speech (2 s grace, 15 s cap)
  (`hal/app_state.py:245-246, 548-605`). No sleep check.
- `POST /scene/off` (`scene.py:167-219`): clears scene and hold; unmutes mic (unless privacy-locked) and speaker even if muted before the scene;
  restores the LED. Active scene re-activated after a HAL restart within the same boot (`scene.py:45-63`; `hal/runtime.py:825-832`).
- `POST /audio/play` (`hal/routes/music.py:193-237`): suppressed if speaker muted, within 3 s of a cancel click, or in audio quiet hours (none on
  the lamp); genre keywords choose a dance recording and emotion LED (jazz, classical, hiphop, rock, waltz, chill, hype; default groove/happy)
  (`music.py:41-76, 144-150`); `MUSIC_START` loops the dance and starts the music LED wave (`:225-228`); chill maps to the sleepy LED (black)
  (`:74, 236`); on completion back to idle and LED restore (`:153-190`). Music wave: rainbow at `music_strong` brightness if no saved user LED,
  else a wave in the user colour (`hal/app_state.py:1285-1313`). `_fire_music_backchannel` defined, never called (`music.py:104-127`).

## C.13 Events to os-server from actuators

None pushed for emotions, LED, overload, contact or thermal; visible on `/health` (`servo`, `led`, `servo_overload`, `thermal`)
(`hal/runtime.py:1364-1389`). Range-demo narration posts to the sensing filler URL (`aim.py:321`). `sleepy` makes a synchronous GET to os-server
`/api/harness/voice-mode`. Sleep journal is local JSONL.
---

# Part E. HAL HTTP surface (index)

Access: FastAPI on port 5001 (`HAL_HTTP_PORT`, `hal/config.py:14`); production binds 127.0.0.1 (`hal/config.py:15-18`) and a middleware admits
loopback/private-LAN client addresses (with no non-local X-Forwarded-For/X-Real-IP), a valid `Authorization: Bearer <device_auth_token>`, or a
same-origin Origin/Referer; else 403 (`hal/server_support/http_security.py:30-105`). Routers are mounted from ROBOT.md declarations plus an
always-mounted set (audio, emotion, scene, system, bluetooth) (`hal/runtime.py:955-1000, 1187-1200`). On the lamp `/display/*` and `/policy/*` are
not mounted; `/environment/*` only on Pro variants. Request bodies are Pydantic models in `hal/models.py`.

| Group | Routes (file:line of decorator) | Validation highlights | Detail |
|---|---|---|---|
| Sensing / presence | `GET /sensing` (sensing.py:80), `GET /sensing/pose-snapshot[/{ts}]` (:122, :155), `GET /sensing/pose-bucket/{id}[/img/{f}]` (:168, :183), `POST /sensing/cloud-perception` (:206), `GET /presence`, `POST /presence/enable|disable` (:227-256) | cloud-perception: 4 booleans, default false (`models.py:819-825`); applied live | A.2–A.9 |
| Face / user | `POST /face/enroll` (:259), `GET /face/status` (:286), `GET /face/owners` (:312), `GET /face/photo/{label}/{filename}` (:368), `GET /face/file/{label}/{path}` (:384), `POST /face/remove` (:403), `POST /face/photo/remove` (:418), `POST /face/reset` (:427), `POST /users/rename` (:436), `GET /face/stranger-stats` (:514), `GET /face/cooldowns` (:521), `GET /face/current-user` (:528), `POST /face/cooldowns/reset` (:535), `GET /user/info` (:553) | enroll: base64 decodable, non-empty, label 1–64 (`models.py:532-536`), no face-count check; rename: labels normalised, `unknown` reserved, 404/409; path routes reject traversal (400); `/user/info` creates the user dir as a side effect (`sensing.py:543-551`) | A.4 |
| Camera | `GET /camera`, `POST /camera/zoom` (1.0–5.0), `POST /camera/disable`, `POST /camera/enable` (owner check), `GET /camera/snapshot` (width/height 1–4096, quality 1–100), `GET /camera/stream` (camera.py:57-288) | enable 403 under manual override without owner; snapshot 409 when user/privacy disabled | A.13 |
| Environment | `GET /environment/status`, `GET /environment/sample` (environment.py:10-22) | 503 if service absent or stale | A.14 |
| LED | `GET /led`, `GET /led/color`, `POST /led/solid|paint|off|effect|status|restore`, `GET|PUT /led/resting`, `POST /led/resting/preview`, `POST /led/wake-light/preview`, `POST /led/effect/stop` (led.py:64-468) | effect speed 0.1–5, duration 100–60000 ms, brightness 0–1; resting RGB 0–255; status name known else 400; colours otherwise only brightness-clamped | C.1–C.4 |
| Emotion | `GET /emotion/status`, `GET /emotion/presets`, `POST /emotion` (emotion.py:40-314) | name (unknown → curious), intensity 0–1; no rate limit | C.5 |
| Scene | `GET /scene`, `POST /scene`, `POST /scene/off` (scene.py:73-219) | scene name else 400; no sleep check | C.12 |
| Servo | `GET /servo`, `POST /servo/upload|play|resume|hold|hold/claim|hold/release|move|zero|release|stop|contact/learn|aim|search|demo|bearing/reset|nudge|track|track/stop|track/update`, `GET /servo/position|status|aim|bearing|track` (servo.py:82-663) | move: known joints, finite, duration 0–10, 503 without pose; nudge yaw ±180 / pitch ±90; hold claim owner must be `look`; play: any string; contact/learn runs 1–5; track bbox 4 ints, `max_duration_s` > 0 | B.2–B.3, C.6–C.10 |
| Music / speaker output | `POST /audio/play` (query 1–500, person ≤ 64), `POST /audio/stop`, `POST /speaker/mute|unmute`, `GET /audio/status`, `GET /audio/history` (music.py:193-307) | 409 if busy; unmute 409 while privacy-locked | C.12, D.5 |
| Audio | `GET /audio`, `POST|GET /audio/volume` (0–100 then clamp), `POST /audio/play-tone` (20–20000 Hz, ≤ 5 s; 409 if muted), `POST /audio/record` (≤ 30 s; 409 if mic muted/locked) (audio.py:84-302) | | D.5–D.6 |
| Bluetooth | `GET /available`, `POST /scan/start`, `GET /scan/results`, `GET /devices`, `POST /pair`, `POST /forget`, `GET|POST /active`, `GET /status` (bluetooth.py:49-183) | no MAC format check; passed to `bluetoothctl` as argv | D.5 |
| System | `POST /system/reboot`, `POST /system/shutdown` (system.py:13-34) | none | D |
| Voice identity (speaker ID, voice-owned) | `/speaker/enroll|record-enroll|identity|reset|remove|recognize|list`, `/identity/current-user`, `/speaker/current-user/reset`, `/voice/strangers*` (speaker.py:205-690) | record-enroll 1–60 s | interface only |
| Voice / TTS (voice-owned) | `/voice/*`, `/tts/stop` (voice.py:61-641), `/voice/piper/*` (piper.py:182-260) | — | other agent |
| Not on lamp | `/display/*` (display.py:17-60), `/policy/run|stop`, `GET /policy` (policy.py:32-54) | — | C.3, C.11 |
| Health / meta | `GET /health`, `GET /device`, `GET /version`, simulator pages (`hal/runtime.py:1211-1396`) | — | — |

---

# Part F. Decision points (consolidated)

All are fixed code rules unless "model" is stated. Models used anywhere in this area: on-device SCRFD + MediaPipe FaceMesh + EdgeFace (face ID),
YuNet (gaze/tracking faces), YOLOv8n (tracking/gaze persons and COCO objects), OpenCV TrackerVit; cloud action recognition (Kinetics, WS),
facial emotion classifier, pose/RULA, OWLv2, YOLOWorld, emotion2vec (voice-owned). No learned policy decides behaviour (C.11).

| # | Decision | Rule | Code / model | Default (lamp) | file:line |
|---|---|---|---|---|---|
| 1 | Run people perception | device declares `presence` | code | on | `hal/runtime.py:689, 702` |
| 2 | Run cloud camera perception X | config.json `cloud_perception.X is True`; env ignored | code | all off | `hal/config.py:240-251, 279, 307, 329` |
| 3 | Push a frame to perceptions this tick | camera present and `_camera_disabled` false | code | 2 s tick | `orchestrator.py:279-291` |
| 4 | Run a perception check | skip if previous check still running; 2 shared workers | code | — | `processors/base.py:19-50` |
| 5 | Face usable | height ≥ 10% frame, ≤ 5% off-frame, sharpness ≥ 100, landmark conf ≥ 0.99 | code + models | as stated | `recognizer.py:770-870`; `model_store.py:30-32` |
| 6 | Face is a friend | upload sim > 0.40 or extended sim > 0.45 | EdgeFace embedding + threshold | 0.40 / 0.45 | `recognizer.py:880-952` |
| 7 | Face is a known stranger | stranger sim > 0.45 | embedding + threshold | 0.45 | `recognizer.py:953-961` |
| 8 | Mint a new stranger | owner sim ≤ 0.2 and seen 2 consecutive ticks within 6 s | code | 2 / 6 s | `recognizer.py:649-668, 962-997` |
| 9 | Add a live view to a user's bank | enroll sim > 0.45, max sim to bank ≤ 0.7, among 10 most diverse | code | on | `recognizer.py:271-381` |
| 10 | Person is "new" | unseen > 3600 s (friend) / 1800 s (stranger) | code | 3600 / 1800 | `faceid/perception.py:437-451` |
| 11 | Announce a stranger | new with a new friend, or faced the lamp on 2 of last 3 ticks within 10 s | YuNet-style yaw from mesh kps + code | 2/3, 10 s, cone 60° (lamp) | `faceid/perception.py:493-506, 912-973`; `stranger_gaze.py:67-94` |
| 12 | Floor a stranger-only enter | < 300 s since last stranger-only enter | code | 300 s | `faceid/perception.py:508-528` |
| 13 | Ask to remember a stranger | visit count == 2 | code | 2 | `faceid/perception.py:58, 723-744` |
| 14 | Send a leave | friend unseen > 3600 s | code | 3600 s | `faceid/perception.py:635-680` |
| 15 | current_user | newest-session friend within 3600 s; else "unknown" if stranger within 1800 s; else ""; face beats voice | code | — | `faceid/perception.py:768-813`; `hal/app_state.py:169-180` |
| 16 | Event type cooldown | per type; 60 s default; 10 s face; 300 s light; 900 s away; none for motion/emotion/fire/sound | code | — | `sensing_service.py:262-270` |
| 17 | Drop events while asleep | all except `presence.enter` | code | — | `sensing_service.py:255-257` |
| 18 | Presence enter grants wake focus | new friend, or `HAL_PRESENCE_WAKE_STRANGERS` | code | strangers off | `sensing_service.py:312-326` |
| 19 | Presence IDLE / AWAY | 300 s → dim ×0.2; 900 s → off + `presence.away`; paused while asleep or guard mode | code | 300 / 900 / 0.2 | `presence_service.py:104-127` |
| 20 | Sound reaches os-server | RMS ≥ threshold; not during TTS (+5 s), music, servo motion, < 15 s after a transcript; occurrence 1 sent, 2 counted, ≥ 3 sent then 180 s suppress + habituation until 120 s quiet | code | threshold 2000 (code 8000) | `sound.py:117-149, 199-274`; `.env:12` |
| 21 | Light event | |Δmean| ≥ 100 over 300 s | code | 100 / 300 s | `light_level.py:35-66` |
| 22 | Upload frame for action recognition | motion opt-in, presence not AWAY | code | off | `motion.py:417-426` |
| 23 | Send `motion.activity` | PRESENT; class cooldown 900 s (transition bypass after 60 s); 300 s same-(user,labels) dedup; posture bypass | code + cloud labels | conf 0.4 (lamp) | `motion.py:507-702` |
| 24 | Fold posture nudge | pose window complete (600 s, ≥ 3 samples), still sedentary, bad_ratio ≥ 0.6 | code + cloud RULA | — | `motion.py:583-636`; `pose.py:699-817` |
| 25 | Emotion qualifies | device per-label gate; Happy on 1 frame; others majority of 10 s attempts and ≥ 2; PRESENT; current_user set; (user,bucket) TTL | cloud classifier + code | TTL 600 s (lamp) | `emotion_gating.py:28-51`; `emotion.py:590-737` |
| 26 | Fire hazard alert | type persists 10 s; per-type TTL 1800 s | OWLv2 + overlap rule | off | `fire_hazard.py:170-360` |
| 27 | Camera auto off/on | emotion preset `camera` field, scenes; skipped under manual override, privacy, or (off only) tracking | code | — | `hal/app_state.py:1429-1470`; `emotion.py:296-307`; `scene.py:124-128` |
| 28 | Camera re-enable allowed | auto pause: anyone; manual off: owner header or voice transcript < 60 s | code | 60 s | `camera.py:104-140` |
| 29 | Gaze watcher on | `GAZE_WAKE` and wakeword and look-aim and camera | code | on if wakeword | `gaze.py:1520-1524` |
| 30 | Open wake gate by gaze | ≥ 2 measured samples, ≥ 60% facing in 1.5 s, 5 s cooldown, not shadow | YuNet + code | 60° cone (lamp) | `gaze.py:852-919` |
| 31 | Turn to remembered user on speech | < 2 measured samples at speech start + guards | code | on | `gaze.py:888-897, 1165-1280` |
| 32 | Tilt / pan framing correction | median offset > 0.15 / 0.10 over 6 s / 12 s, conversation open, body free | code | on | `gaze.py:726-824, 1011-1162` |
| 33 | Glance after a missed turn | confirmed miss, no hold, 15 min / 2 min cooldown | code | on | `gaze.py:1379-1433` |
| 34 | Tracker detector | face → YuNet; COCO → YOLOv8n; else remote YOLOWorld (faces also remote on YuNet miss) | code + models | — | `detection.py:320-394` |
| 35 | Tracking hold / chase / stop | confidence, size, detector age, 10 s cap | code + tracker score | 10 s | `tracker_service.py:431-664` |
| 36 | Unknown emotion name | → curious (ignored asleep) | code | — | `emotion.py:111-122` |
| 37 | Emotion while asleep | only greeting, stretching, sleepy | code | — | `emotion.py:26-29, 130` |
| 38 | Allow sleep | Harness off and os-server answers ≤ 0.5 s | code | fail closed | `emotion.py:63-67, 135` |
| 39 | Emotion moves servo | not tracking, not (hold and (explicit or not scene change)) | code | — | `emotion.py:229-272` |
| 40 | Still emotion | halt, idle resume after 10 s (lamp `thinking` included) | code | 10 s | `emotion.py:70-87, 256-272` |
| 41 | Emotion LED restore time | shock 2 s; CSV duration + 0.5 s; none for idle/sleepy/listening | code | 3.5 s fallback | `emotion.py:278-294` |
| 42 | Stuck thinking | revert to idle | code | 25 s | `emotion.py:167-193` |
| 43 | LED restore ladder | sleep > TTS > music > mic-muted > thinking > Harness > user state > resting | code | resting [1,1,1] | `hal/app_state.py:981-1095` |
| 44 | Recording speed and safety | stretch to ≤ min(250, 120); refuse if CoG > 22 mm; clamp joint goals | code | 120, 22 mm | `recording_timing.py:13-44`; `recording_stability.py:180-273`; `hal_follower.py:254-262` |
| 45 | Overload / contact / thermal | ≥ 80% 1.5 s → torque off 120 s; contact floor 0.05 s → 3 s halt; ≥ 95 °C stops tracker | code | servo_overload.json | `overload.py:66-101`; `animation_service.py:1146-1446`; `hal/runtime.py:1056-1098` |
| 46 | Button hold | > 2 s sleep, > 5 s shutdown, > 10 s reset (off on lamp) | code | — | `button_actions.py:479-499` |
| 47 | Button tap | first tap → `single_click_action`; 3 taps in 0.4 s → reboot | code | — | `gpio_button.py:231-266` |
| 48 | Head pad | any gesture → spoken pet phrase if TTS idle; report ≤ 1/60 s | code | — | `ttp223.py:413-497`; `button_actions.py:37-66, 358-374` |
| 49 | Touch strip | tap → floor-taking tap; hold ≥ 2 s → voice-mode toggle; swipe ±1 → sleep / Harness; all ignored if Harness state unknown | code | — | `mpr121.py:640-750` |
| 50 | Privacy switch | 60 ms settle; mute cuts mic, camera, speaker; unmute runs single-click (wakes) | code | — | `privacy_button.py:134-320`; `hal/privacy.py:25-66` |
| 51 | Volume | ≤ `max_volume` | code | 100 (Lite 50, XVF 77) | `policy.py:302-306` |
| 52 | Music style | keyword match → dance + LED | code | groove/happy | `music.py:41-76` |

---

# Part G. Configuration

Precedence: process env (systemd loads `/opt/hal/.env` = `robots/lamp/rootfs/opt/hal/.env`, merged with a hardware-profile overlay at install)
> code default in `hal/config.py`. Some values come from os-server `config.json` at `OS_CONFIG_PATH` (`/root/config/config.json`, `hal/config.py:153`):
`cloud_perception`, `llm_base_url`, `llm_api_key`, `device_auth_token`, `wakeword`, `voice_input_mode`, `wake_followup_s`, `wake_chime`,
`wake_light`, `tts_speed`, `device_type`. `hal/env-presets/*.env` contain only voice/VAD keys (none from this area).

## G.1 HAL_* in this area — code default vs lamp

| Variable | Code default (file:line) | Lamp base `.env` (line) | Overrides |
|---|---|---|---|
| `HAL_MODE` | production (config.py:16) | production (1) | — |
| `HAL_SENSING_ENABLED` | true (runtime.py:680) | — | — |
| `HAL_SENSING_INTERVAL` | 2.0 (runtime.py:697) | — | — |
| `HAL_AUDIO_SENSING_DEVICE` | None (config.py:39) | plug:device_micro1 (4) | same in all three |
| `HAL_AUDIO_INPUT_ALSA` / `_OUTPUT_ALSA` | None (config.py:34-35) | plug:device_micro2 / plug:device_speaker (3, 5) | same |
| `HAL_EVENT_COOLDOWN_S` | 60 (config.py:83) | — | — |
| `HAL_SOUND_RMS_THRESHOLD` | 8000 (config.py:85) | 2000 (12) | — |
| `HAL_SOUND_SAMPLE_DURATION_S` | 0.5 (config.py:86) | — | — |
| `HAL_LIGHT_LEVEL_INTERVAL_S` / `HAL_LIGHT_CHANGE_THRESHOLD` | 300 / 100 (config.py:88-89) | — | — |
| `HAL_USERS_DIR` / `HAL_STRANGERS_DIR` | /root/local/users / /root/local/strangers (config.py:91-92) | — | — |
| `HAL_FACE_COOLDOWN_S` | 10 (config.py:96) | — | — |
| `HAL_FACE_OWNER_FORGET_S` / `HAL_FACE_STRANGER_FORGET_S` | 3600 / 1800 (config.py:97-98) | — | — |
| `HAL_FACE_STRANGER_ENTER_FLOOR_S` | 300 (config.py:100) | — | — |
| `HAL_FACE_STRANGER_GAZE_TICKS` / `_MIN_FACING` / `_WINDOW_S` | 3 / 2 / 10 (config.py:102-109) | — | — |
| `HAL_PRESENCE_WAKE_STRANGERS` | false (config.py:111-114) | false (86) | — |
| `HAL_FACE_HEIGHT_RATIO_THRESHOLD` / `_MAX_TRUNCATION` / `_MIN_SHARPNESS` | 0.10 / 0.05 / 100 (config.py:116-122) | — | — |
| `HAL_FACE_STRANGER_MIN_TICKS` / `_CORROBORATION_S` | 2 / 6 (config.py:124-128) | — | — |
| `HAL_FACE_MATCH_THRESHOLD` / `_EXTENDED_THRESHOLD` / `_STRANGER_THRESHOLD` / `_EXTEND_MIN_ENROLL_SIM` | 0.40 / 0.45 / 0.45 / 0.45 (config.py:130-138) | — | — |
| `HAL_FACE_LANDMARK_CONF_THRESHOLD` | 0.99 (model_store.py:30-32) | — | — |
| `HAL_FACE_MODEL_PATH` (+ per-model paths, CDN base) | /root/local/models (model_store.py:11-36) | — | — |
| `HAL_FACEID_DEBUG_LOG_ENABLED` | false (config.py:140) | — | — |
| `HAL_VOICE_USER_FORGET_S` | 300 (config.py:151) | — | — |
| `HAL_DL_ENCRYPTION` | true (config.py:224) | true (41) | — |
| `HAL_DL_HEARTBEAT_INTERVAL_S` / `HAL_DL_WS_RECV_TIMEOUT_S` | 60 / 15 (config.py:218-220) | — | — |
| `HAL_MOTION_PER_FACE_ENABLED` | false (config.py:253) | — | — |
| `HAL_MOTION_CONFIDENCE_THRESHOLD` | 0.3 (config.py:257) | 0.4 (26) | — |
| `HAL_MOTION_FLUSH_S` / `_EVENT_COOLDOWN_S` / `_TRANSITION_MIN_GAP_S` | 10 / 900 / 60 (config.py:260-268) | — | — |
| `HAL_MOTION_PERSON_DETECTION_ENABLED` / `_MIN_AREA_RATIO` | true / 0.25 (config.py:269-272) | — | — |
| `HAL_EMOTION_CONFIDENCE_THRESHOLD` | 0.5 (config.py:280) | 0.5 (27) | — |
| `HAL_EMOTION_LABEL_THRESHOLDS` | "" → defaults (config.py:284) | — | — |
| `HAL_EMOTION_FLUSH_S` / `_DEDUP_WINDOW_S` | 10 / 300 (config.py:285-286) | — / 600 (28) | — |
| `HAL_EMOTION_THINKING_RESET_S` | 25 (config.py:288) | — | — |
| `HAL_FIRE_HAZARD_*` | interval 5, conf 0.3, overlap 0.2, confirm 10, dedup 1800, flush 10, detector owlv2, timeout 15 (config.py:309-319) | — | — |
| `HAL_POSE_MOTION_ENABLED` (dead) | true (config.py:321) | false (25) | — |
| `HAL_POSE_SAMPLE_INTERVAL_S` / `_WINDOW_DURATION_S` / `_WINDOW_MIN_SAMPLES` / `_REGION_HIGH_SUBSCORE` / `_BAD_RATIO` | 30 / 600 / 3 / 4 / 0.6 (config.py:330-340) | — | — |
| `HAL_IDLE_TIMEOUT_S` / `HAL_AWAY_TIMEOUT_S` / `HAL_IDLE_BRIGHTNESS` | 300 / 900 / 0.20 (config.py:371-373) | — | — |
| `HAL_GUARD_CHECK_INTERVAL_S` | 10 (config.py:81) | — | — |
| `HAL_SNAPSHOT_*` | tmp 50 files; persist 72 h, 50 MB (config.py:357-369) | — | — |
| `HAL_SPEECH_EMOTION_ENABLED` (voice-owned) | true (config.py:498) | — | — |
| `HAL_CAMERA_WIDTH` / `_HEIGHT` | 640 / 480 (config.py:22-23) | 1280 / 720 (30-31) | — |
| `HAL_CAMERA_STREAM_WIDTH` | 320 (camera.py:233) | 960 (32) | — |
| `HAL_CAMERA_STREAM_HEIGHT` (unread) | — | 540 (33) | — |
| `HAL_CAMERA_STREAM_FPS` / `_JPEG_QUALITY` | 10 / 65 (camera.py:232-234) | — | — |
| `HAL_CAMERA_INDEX` / `HAL_CAMERA_NAME` | 0 / None (config.py:19-21) | 0 / /dev/device-camera (34, 38) | — |
| `HAL_CAMERA_AUTO_EXPOSURE` / `_EXPOSURE` / `_GAIN` / `_BRIGHTNESS` | auto / 330 / 96 / unset (config.py:26-31) | auto / — / 64 (76, 79) | — |
| `HAL_TRACKING_MAX_DURATION_S` | 10 (config.py:65-67) | 10 (39) | — |
| `HAL_GAZE_WAKE` / `HAL_GAZE_SHADOW` / `HAL_GAZE_MAX_YAW_DEG` | false / true / 25 (config.py:824-832) | true / false / 60 (92, 93, 99) | — |
| other `HAL_GAZE_*`, `HAL_LOOK_AIM*`, `HAL_BEARING_*`, `HAL_TRACKING_*`, `HAL_YUNET_CONFIDENCE_THRESHOLD` | see Part B / fork table (config.py:56-67, 93-95, 778-871, 1158-1311) | — | — |
| `HAL_WAKEWORD_FOLLOWUP_TIMEOUT_S` | 10 (config.py:586-588) | 5 (81) | — |
| `HAL_SLEEP_ANNOUNCEMENT_DELAY_S` | 2.0 (config.py:49) | 2.0 (136) | — |
| `HAL_BT_PREFER_HFP` | 0 (config.py:37) | 1 (80) | — |
| `HAL_BACKCHANNEL_INTERVAL_S` (voice) | 5.0 (backchannel.py:31) | 5 (29) | — |
| Servo/LED: `HAL_SERVO_PORT` /dev/ttyACM0, `HAL_SERVO_FPS` 30, `HAL_SERVO_HOLD_S` 3.0, `HAL_SERVO_PLAY_RAMP_S` 2.0 (config.py:8-13), `HAL_SERVO_MAX_DPS` 250 (recording_timing.py:13), `HAL_CONTACT_PROFILE_PATH`, `HAL_RESTING_LED_PATH` (config.py:890-896), `HAL_STATE_DIR` /tmp (config.py:873), `HAL_SLEEP_LOG_DIR`/`_MAX_DAYS` (config.py:876-877) | as listed | — (lamp runs defaults) | — |
| Touch: `HAL_TOUCH_SWIPE` true, `_MIN_GAP_MS` 35, `_MAX_GAP_MS` 150, `HAL_TOUCH_PRESS_MIN_EMPTY_MS` 15, `HAL_TOUCH_DEBUG` off | per fork (ttp223.py:23-59) | — | — |
| `DEVICE_TYPE` / `DEVICES_DIR` | — | lamp / /opt/devices (44-45) | — |
| `HAL_LIVE_MODE`, `HAL_AEC_ENABLED`, Silero etc. (voice) | — | see `.env` | Lite/XVF: AEC off, live on; Lite Silero 0.10 |

## G.2 robots/lamp files

- `ROBOT.md`: capabilities (A.1), boards [sim, raspberry_pi_4, raspberry_pi_5, orangepi_sun60], gateway default hermes, `voice.wakeword: true`,
  `startup_volume: 75` (`robots/lamp/ROBOT.md:1-47`).
- `SAFETY.md` front matter: light 120 / quiet 22:00–07:00 → 40; audio max 100; motion max_speed 120, stop_always, max_cog_offset_mm 22,
  joint_limits; thermal 95/85 (`robots/lamp/SAFETY.md:8-50`; C.7).
- `presets.json`: resting [1,1,1], led_count 32, emotion palette 0–3, thinking servo null, stretching → idle, scene brightness table, status-LED
  overlay (`robots/lamp/presets.json`).
- `servo_overload.json` (sun60 only): torque cap 60%, overload 80%/1.5 s/120 s, contact floors (C.8).
- `gpio_button.json`, `privacy_button.json`, `ttp223.json`, `mpr121.json` (D.1). `sen55.json`, `scd41.json`, `sen63c.json` all disabled in base;
  Pro overlays enable SEN63C (A.14).
- `overrides/<profile>/profile.json`: pro = environment only; pro-respeaker-lite = 35/50 + environment; pro-xvf3800 = 77/77 + environment.

---

# Part H. Dead, disabled, unreachable, stubbed

Perception:
- Sound perception never runs while the camera is off or absent, because it is driven by the camera frame observer (`orchestrator.py:154-164,
  282-288`). Not dead, but effectively disabled under sleep, privacy mute, manual camera off and the reading/focus/movie/night scenes.
- All four cloud camera perceptions are off unless the owner opted in (`hal/config.py:240-251`); with them off, `motion.activity`,
  `emotion.detected`, posture nudges and `fire_hazard.detected` are never produced. Whether shipped lamps have them on: unverified.
- `HAL_POSE_MOTION_ENABLED`, `HAL_POSE_MODEL_PATH`, `HAL_POSE_MOTION_ANGLE_THRESHOLD` are read but no code uses them (`hal/config.py:321-327`; grep
  finds no consumer). Lamp `.env:25` sets the dead flag false.
- `HAL_CAMERA_STREAM_HEIGHT=540` in lamp `.env:33` is read by nothing.
- No production caller: `SensingService._capture_stable_frame` (`sensing_service.py:150-163`), `FacePerception.register_callback` /
  `unregister_callback` (so `_callbacks` is always empty, `faceid/perception.py:138-144, 555-557`), `FacePerception.has_friend_present`
  (`:751-766`), `PosePerception.is_window_bad` and `draw_latest_overlay` (`pose.py:812-827`), `SoundPerception.last_level` (`sound.py:96-99`),
  `RemoteMotionChecker.last_action` (never assigned, `motion.py:144, 294-296`), `MoveEnum` (`motion.py:101-106`).
- Emotional action labels (laughing, crying, singing) are dropped; the planned `motion.emotional` event does not exist (`motion.py:533-534`).
- `MotionPerFacePerception` is off by default; when enabled it runs in addition to (not instead of) `MotionPerception` (`orchestrator.py:200-208`).
- Camera "auto-on on sound spike" and "auto-off on AWAY" are not implemented (documented as skipped, `camera-lifecycle.md:126-128`).

Tracking/gaze (Part B): absence-triggered 12 s repoint and 30 s sweep unreachable (`gaze.py:967, 1300, 1506-1508`); `GAZE_REPOINT_AFTER_S` and
`GAZE_SWEEP_AFTER_S` therefore dead; the tracker's 20 s no-confirmation stop never fires under the 10 s cap; `play("tracking")` at tracker start
names a recording that does not exist (`tracker_service.py:151`; `animation_service.py:683-685`); look-aim never tilts (`aim.py:878`); old
user-bearing files dropped, not migrated (`user_bearing.py:106-114`). Gaze watcher entirely off when wakeword is off.

Actuators (Part C): learned motion policy is a logging stub and not mounted on the lamp (`hal/policy/service.py:35-73`; `hal/runtime.py:1003-1006`);
display not mounted; `hal/main.py`, `motors_service.py` and `hal/smooth_animation.py` are a legacy LiveKit "LeLamp" demo with missing imports, kept
only by the lint KEEP list (`hal/smooth_animation.py:18-20, 68-69`; `scripts/lint.py:13`; production runs `uvicorn hal.server:app`);
`SERVO_WAKE_UP` ("wake_up") has no CSV; `fear.csv`, `playful.csv` have no preset and `listening.csv`, `thinking_deep.csv` are unmapped on the lamp
(reachable only via `/servo/play`); `_fire_music_backchannel` never called (`music.py:104-127`); `reachy_sdk`, `stackchan`, `mock` motion drivers
unused on the lamp; `audio.quiet_hours` unset; `max_accel` reserved; thermal acts only on the rising edge; the `sleepy` recording likely never plays
(1.0 s auto-release parks first; derived).

Controls (Part D): `mic_toggle_action` no caller (`button_actions.py:388-417`); spoken "Listening" cue commented out (`button_actions.py:222-232`);
MPR121 triple-tap reboot and hold-to-sleep/shutdown disabled (`mpr121.py:410-419, 730-735`); TTP223 tap/double-tap/swipe all map to the pet
response; `load_button_config` no runtime caller; `media_owner` unused on the lamp; CM4 board entry marked UNVERIFIED in `boards.json`.

---

# Part I. Doc-vs-code contradictions and surprises

I.1 Sound perception depends on the camera. `SoundPerception.check` is registered on the camera frame observer
(`orchestrator.py:154-164`), and frames are pushed only when a camera exists and is not disabled (`orchestrator.py:282-288`). So loud-noise
detection stops whenever the camera is off. `robots/lamp/docs/camera-lifecycle.md:9-10, 19, 35-36, 47` says sound perception runs independently
of the camera. The `/voice/mute` docstring says "stop voice pipeline and sound perception" (`hal/routes/voice.py:503`); plain mic mute leaves the camera
on, so sound perception keeps running.

I.2 Sound sampling cadence: `sensing-behavior.md:66` says HAL fires on every sample "potentially several times per second"; the code takes one
0.5 s sample per 2 s tick (`sound.py:165-196, 226-233`).

I.3 `sensing-behavior.md:239-241` ("Only large motion is forwarded … Large motion → curious + scanning") and `:632` ("foreground motion triggers
motion.activity instead of motion") describe a pixel-motion pipeline that no longer exists; `:641` says "No images attached", but
`motion.activity` attaches the last frame (`motion.py:702`).

I.4 Pose: `sensing-behavior.md:247` says HAL streams every frame and samples every 60 s; code uploads at most one frame per
`POSE_SAMPLE_INTERVAL_S` (default 30 s, `config.py:332`; `pose.py:654-657`). `:255-258` says a sample is bad at `risk_level >= 3`; code uses
`>= 2` (`pose.py:755`).

I.5 Facial emotion: `sensing-behavior.md:739` says 7 emotions; code lists 8 including Contempt (`emotion.py:37-46`). Emotion is requested for
every face including UNSURE ones (`emotion.py:549-554`).

I.6 Speech emotion: `sensing-behavior.md:812` says unknown speakers are dropped; code submits them under `"unknown"`
(`speaker_decorate.py:519-528`; `speech_emotion/service.py:192-196`), consistent with `docs/speech-emotion.md:401-404`. `docs/speech-emotion.md:66`
says the wake-gate-rejected path is unreachable because `wakeword` defaults false; the lamp ROBOT.md seeds `wakeword: true` for new configs
(`robots/lamp/ROBOT.md:19-20`), so on such lamps that path is reachable. SER uploads voice to the cloud without the `cloud_perception` opt-in
that the camera perceptions require.

I.7 `camera-lifecycle.md:8` names InsightFace and "presence (pixel diff)"; code uses SCRFD + MediaPipe mesh + EdgeFace and presence from face /
motion / emotion evidence. `:28` "Face/pose ONNX inference every other tick" has no counterpart in code (checks skip only when busy). `:274`
describes a sound-spike auto-on timer that `:128` says was skipped and the code does not have.

I.8 Per-type cooldown is keyed by event type, not person (`sensing_service.py:266-270`): two different friends arriving within 10 s produce one
`presence.enter`; the second is dropped (its `last_seen` is already set, so it is not re-sent later). Same for two friends leaving.

I.9 Presence dim and default looks (derived, not device-verified): `int(1 × 0.20) = 0`, so IDLE "dim" of the lamp's resting [1,1,1] paints black
(`presence_service.py:178-181`); the TTS speaking wave on [1,1,1] and intensity 0.7 on single-unit channels also truncate to 0 (C.5; fork).

I.10 Sleep depends on the local os-server: `sleepy` (and button/swipe sleep) is refused when os-server's Harness state
(`GET 127.0.0.1:5000/api/harness/voice-mode`) is unreadable within 0.5 s (`emotion.py:63-67`), and MPR121 ignores all touches then.
`robots/lamp/SAFETY.md:91-92, 103` lists sleep among the local reflexes that stay alive on network/gateway loss; that holds while os-server itself
answers, not if os-server is down or slow. Sleep mutes after a fixed 2 s, possibly cutting its own announcement. The privacy switch's unmute
wakes a sleeping lamp.

I.11 On the lamp `thinking` has no servo recording, so every thinking cue halts the arm and arms a 10 s idle resume (C.5). No rate limit on
`POST /emotion`.

I.12 `robots/lamp/SAFETY.md:56-61` says a real `POST /servo/stop` is open work; it exists (`servo.py:388-407`). SAFETY.md and motion-playback.md
speak in degrees; the servo layer uses −100..100 normalised units (`hal_follower.py:53`). `ServoMoveRequest` description says nothing enforces
joint bounds, but `joint_limits` clamps them (`models.py:655-658` vs `hal_follower.py:254-262`). `move_to_raw` bypasses the clamp.

I.13 `POST /scene` paints the LED while asleep; presence dim/off do not stop a running effect; an emotion during music stays on the strip until
the song ends; the emotion LED restores on authored CSV time while the arm may still be moving; `/servo/play` returns "ok" for a missing
recording; `/servo/upload` can overwrite shipped recordings; `/servo/search` ignores sleep and holds (C, B).

I.14 Lamp `HAL_GAZE_MAX_YAW_DEG=60` with the 1.8 edge scale makes the cone reach 90° about 62% of the way to the frame edge, so almost any head
off-centre counts as "facing"; this cone is also used for the stranger greeting gate (B.5, A.4). Config comment says 103°; actual 108°.

I.15 `vision-tracking.md` contradictions (per fork): file format v2 vs code v3 (`:1066`); miss scoring on abort (`:1189-1191` vs
`aim.py:740-764`); lists the unreachable "alone too long" sweep as an entry point (`:553-556`); 3 roll-only looks vs 6 looks with tilt
(`:687-699`); remote fallback off for redetects only true for COCO objects (`:84`); idle "takes the arm back within a frame" after framing (`:814`)
unsupported; `:797` says `HAL_GAZE_WAKE` alone gates the watcher.

I.16 `emotion-led-mapping.md` and `led-control.md` (per fork): background guard covers idle only (not thinking); on the lamp thinking has no servo
too; greeting never plays `wake_up`; music_chill plays music_rock only; the emotion LED is always restored; `/led/restore` settles to [1,1,1] not
clear; effect table omits blink and both speaking waves; pulse travels around the ring, not centre-outward; paint indexes 0–63 vs 32 pixels;
"Driver: rpi_ws281x" only on Pi 4 (sun60 uses SPI).

I.17 `physical-controls.md` (per fork): `:662` says startup re-expresses sleepy (code deliberately does not, `runtime.py:841`); `:238`, `:773`
point to `hal/server.py` (wiring is in `runtime.py`); `:742` says 15 pet phrases (42). `mic-lifecycle.md` is stale (GPIO17, 500 ms debounce,
`_on_stop_button`, spoken "I'm listening", "no auto-mute"). `docs/bootstrap-ota.md:424, 435, 437` mismatch the Lite volumes (35/35 vs 35/50),
Lite AEC/live settings, and the pro rootfs override.

I.18 Surprises in wiring: `GET /user/info` creates a user directory as a side effect (`sensing.py:543-551`); face enrollment embeds every face in
the uploaded image under one label and accepts images with no face (`recognizer.py:193-234`); the stranger bank is re-read from disk on every frame
with a face (`recognizer.py:719-720`); the HAL perception pool is 2 threads shared by on-device face ID and all cloud uploads (whose WS
recv timeout is 15 s), so a slow cloud call can delay face ID (`base.py:19-24`; inference from code, not measured).
