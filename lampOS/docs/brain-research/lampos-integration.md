# Where the brain plugs into lampOS

Code study of `lamp-v2-chat` at `10607a439` (12 October 2026). `C` stands for
`crates/live/src/coordinator.rs`.

**Verified by hand for the design:**

- the terminal exit at C:2829-2847 and the pilot unit's `Restart=no`;
- that a Gemini tool call is fatal (`crates/gemini/src/wire.rs:269-271`,
  `crates/gemini/src/lib.rs:424-449`);
- the `ProviderSource` enum at C:610-628;
- the `ProviderInput` and `ProviderOutput` enums
  (`crates/live/src/provider_worker.rs:126-200`).

Line numbers move as `lamp-v2-chat` changes; re-check before editing.

## 1. The coordinator loop

**Call chain.**
`run_serve` C:734 → `run_source` C:757 → `service::supervise`
(`coordinator/service.rs:6-51`, which waits for privacy and then runs one
runtime per cycle) → `run_inner` C:1745 (`Controller::new` C:1768, `spawn_rig`
C:1834) → `conversation` C:1986-2859.

**Each 2 ms iteration, in order (C:2072-2858):**

| Step | Where |
| --- | --- |
| Signal stop | C:2074-2082 |
| Privacy drain (≤16; in `serve` a mute ends the cycle) | C:2098-2139, C:2116-2131, C:2143-2154 |
| Microphone permission, camera, motor and head | C:2167-2197 |
| Provider priority control (≤16) | C:2198, `service_provider_control` C:962-994 |
| Submission gate | `apply_submission_gate` C:1268-1298 |
| Capture control; speaker receipts (≤32) | C:2213; C:2219, `service_speaker_control` C:1011-1201 |
| Provider data (≤2 packets or 2 ms) | C:2234-2424 |
| Finite-run deadline | C:2425-2446 |
| Capture (≤8 blocks): leases, `listening_ready`, duplex hearing, detector and admission, candidate decision, audio and End | C:2462-2724 |
| 20 ms authority republish; provider flush | C:2730; C:2749-2756 |
| Failure-notice selection; speaker dispatch | C:2757; C:2760 |
| Turn completion | C:2797-2824 |
| Terminal `serve` exit | C:2829-2848 |
| Ring; health; cue flush; sleep | C:2849; C:2850; C:2854; C:2855 |

**Turn lifecycle:**

- **Start:** `commit_admitted_input` C:3172-3254 (`admit` C:3193, Speech plan
  C:3194, queued Start C:3205). With no cloud, `accepted_without_cloud`
  C:1219-1241 cancels locally and creates no owner.
- **Answered:** provider audio C:2267-2314. The first accepted write calls
  `playback_started` C:1073-1084.
- **Finished:** completion C:2797-2823, with outcomes from `delivery_outcome`
  C:228-238 (`provider_failed`, `no_audio_answer`,
  `audio_written[_with_playback_gaps]_unscored`).
- **Cancelled:** `cancel_reply` C:3580-3606, with these reasons:
  - `user_interrupted` C:3185
  - `provider_interrupted` C:2342
  - `deadline_cancelled` C:2442
  - `service_stopped` C:2079
  - `privacy_muted` or `runtime_failed` C:1951-1960
  - `playback_discarded` C:3703

**There is no typed observation point.** Every observation is only
`record(trace, json!)` (C:940-942), and trace metadata cannot carry
authority (`docs/serve-runtime.md:50-53`). A brain needs typed inputs, not
trace rows.

**The gates:**

- **Speech:** `dispatch_speaker_chunk` C:3353-3419 is the single dispatch for
  provider audio and notices; the permit is issued at C:3388.
- **Notices:** chosen in `select_failure_notice`
  (`coordinator/notices.rs:231-265`).
- **Admission verdict:** `decide_prospective_input` C:3158-3168.
- **Ring:** `RingChoreographer::prepare` (`choreography.rs:57-125`), called at
  C:3325.
- **Motor:** `OptionalMotor::offer_segment` (`coordinator/motor.rs:904-952`).
  Ordinary conversation never calls it.
- **Authority fan-out:** `Rig::publish` C:289-318.

**The `Controller`** (`crates/interaction/src/controller.rs`) is the only
mutable authority (9-15):

- **Admission:** `admit` revokes the previous turn (156-184).
- **Plans and permits:**
  - Each needs the current `TurnOwner` (372-378), the microphone allowed,
    and live capture and admission leases (366-370).
  - Speech is refused while input is active (257-259).
  - Nonverbal permits die with a presentation change (253-256).
  - Expiry is clipped to the leases (260-263); ceilings are in
    `types.rs:5-7`.
  - Lease expiry revokes the turn (390-411).
- **What it doesn't do:** it has no social heuristic (156-158). The final
  writers check `BoundaryGuard` (`lib.rs:7-13`).

## 2. Observations available today

| Observation | Shape and source | Freshness |
| --- | --- | --- |
| Privacy | `WorkerEvent::Privacy{acquired_at_us, muted}` into `PrivacyGate` (C:2098-2142); states Unavailable, Muted, Stabilizing, Allowed (`privacy.rs:11-14`) | fresh within 50 ms; stable reopen 60 ms (`privacy.rs:5-6`) |
| Input candidates | `Candidate{trigger_read_at_us, first_read_at_us, decision_deadline_us, displaced_owner}` (`admission.rs:72-81`); `Evidence{through_sequence, produced_at_us, verdict}` (`admission.rs:117-124`); reject reasons (`admission.rs:100-108`) | evidence under 100 ms (`admission.rs:13`); decision budget 200 ms (`admission.rs:11`) |
| Turn-taking and duplex | per-block VAD probability and echo evidence (C:2536-2546); yield, commit and resume with `hearing_margin_db` (C:2934-3033); `playback_mask.available` and `yielded` (`turn_taking.rs:805`, `:272`), traced at C:2785-2792 | per block |
| Local endpoint | C:2695-2722 | after the silence window |
| Provider link | `ProviderLink{revision, session, observed_at_us, source_clock}`, states `Ready`, `Unavailable{reason}`, `Terminal{reason, fault, attempts}` (`provider_link.rs:28-58`), held in `CloudAvailability` (`coordinator/cloud.rs:16-66`); `Fault` includes `RateLimited`, `QuotaExceeded`, `PeerClosed{code}` (`provider_worker.rs:300-325`) | ordered by revision |
| Provider events | `TurnFailed{stage, reason}` C:2367; `Ended` C:2404; `Transcript` C:2322 (input transcripts are session-scoped and unreliable per turn); `Interrupted` C:2330; first audio C:2295 | — |
| Playback | first accepted write (C:1028-1095); `SpeechRetired` (C:1127); drained, gap, discard, cancelled tail, clock reset (C:1096-1188) | ALSA times, not acoustic |
| Camera | lifecycle and unavailability (`coordinator/camera.rs:286-330`); `VisualContext` receipt with frame sequence, dequeue times and outcome (C:979-988) | events older than 100 ms rejected (`camera.rs:238-245`) |
| Motor and ring health | `motor_degraded` (C:1920-1938); ring disable and degraded (C:3258-3280; `coordinator/optional_ring.rs:224`) | — |
| Readiness | `listening_ready` (C:2492-2514); `Snapshot::listening_ready` (`types.rs:304`) | — |
| Lifetime and signals | `supervise`; `service_provider_ended` (C:2829-2848); `Termination` (C:2074) | — |
| Presence or face | **none**. `lamp-environment` has freshness-tracked sensor types (`crates/environment/src/lib.rs:1-20`), but `lamp-live` does not use them | — |

## 3. Outputs and their gates

- **Speech:**
  - At most one 240-sample chunk is outstanding.
  - Each chunk needs a 100 ms `issue_output` permit from the reply's
    immutable plan (C:3388, C:252).
  - Provider audio also needs `Started` (C:3377).
- **Failure notices:**
  - They use the same path and owner (`docs/failure-notices.md:161-169`).
  - Only `turn_lost` is ever selected (`notices.rs:27-29`).
  - No production asset exists (`docs/failure-notices.md:146-147`).
  - `ProviderUnavailable` and `ServiceRefused` kinds exist but are unwired
    (`crates/audio/src/notice.rs:41-48`).
- **Ring:** the phase comes only from the `Snapshot` (`ring_wire.rs:16-25`). It
  needs a Light permit renewed every 20 ms (`choreography.rs:105-116`), and the
  worker rechecks the guard (`ring_worker.rs:309`).
- **Motor:**
  - Needs a Motion permit, a fresh placement `Observation` and qualification
    (`coordinator/motor.rs:904-952`).
  - Only the operator head path mints one (`head_qualification.rs:510-520`).
  - The worker opens with `automatic_motion:false` (C:1814).
- **Camera to Gemini:**
  - The provider worker decides, not the core: at most one attempt per open
    submitted input (`visual_context.rs:1-2`, `:251-270`;
    `provider_worker.rs:567-579`).
  - Frames must be under 250 ms old at dequeue (`visual_context.rs:17`).
  - Frames are revoked when the camera grant changes
    (`visual_context.rs:219-245`).
  - The core controls only camera permission (`camera.rs:315-330`).
- **Volume:** no control exists. The speaker never changes the mixer
  (`crates/audio/src/pcm.rs:1-3`).

## 4. At the Gemini usage limit today

1. **Classification.**
   - A server error with 429 or `RESOURCE_EXHAUSTED` becomes `QuotaExceeded`
     (`crates/gemini/src/wire.rs:250-256`).
   - A close reason mentioning quota or usage limit becomes `QuotaExceeded`
     (`session.rs:1946-1966`).
   - HTTP 429 at the handshake becomes `RateLimited` (`session.rs:1985-1993`).
   - Relay close codes 4001, 4002 and 4029 stay `PeerClosed`.
   - All map to `Recovery::Unavailable` (`lib.rs:424-430`).
2. **Supervisor.** It stops with `NotRecoverable` and no retry
   (`recovery.rs:1105-1106`, `:1225`), then publishes a `Terminal` link
   (`recovery.rs:806-811`).
3. **Worker.** It sends the link on priority control
   (`provider_worker.rs:550`). A request in flight becomes `TurnFailed`
   (`provider_worker.rs:648-659`), then `Ended` (`:661-668`).
4. **Coordinator.**
   - `ready_session` becomes None (`cloud.rs:41-47`).
   - Unfinished input is abandoned (C:1276-1295), and the reply is marked
     failed (C:1357-1368).
   - The `turn_lost` notice plays only if provisioned (C:2757). The turn
     finishes `provider_failed`.
5. **New speech.** It is cancelled locally as `input_not_submitted`, with no
   owner and no notice (C:1219-1241; `docs/failure-notices.md:191-193`).
6. **Exit.** Once settled, `serve` records `service_provider_ended` and exits
   with an error (C:2829-2847). The pilot unit has `Restart=no`
   (`deploy/4ace-attended/lamp-live-pilot@.service:17`). A quota refusal at
   startup also exits.

**The person hears nothing,** the ring blanks, and `serve` stays stopped.

## 5. Configuration

- **`ProviderConfig`** (`crates/live/src/config.rs:12-33`) reads `endpoint`,
  `model`, `voice`, `credential_file`, and optionally `thinking_level`,
  `language_code` and `authentication` (`api_key`, `bearer`, `ephemeral`).
- **Private files:** at most 16 KiB for the config and 4 KiB for the
  credential (`config.rs:42-55`, `:91-118`).
- **The instruction is fixed in source:** `CHAT_INSTRUCTION` (`config.rs:35`),
  or `CAMERA_CHAT_INSTRUCTION` (`config.rs:38`).
- **Setup:** audio output only, manual activity,
  `START_OF_ACTIVITY_INTERRUPTS`, both transcriptions, no tools
  (`crates/gemini/src/wire.rs:458-473`).
- **The pilot:** `gemini-3.8-live` with no thinking level
  (`docs/gemini-comparison-config.md:25-31`), voice Kore
  (`docs/live-runtime.md:614-616`).
- **The endpoint** is private to the device
  (`deploy/4ace-attended/manifest.template.json:47`). Pilots go through
  campaign-api (`docs/real-device-audio-pilot.md:20-22`).

## 6. Seams to reuse

- **Admission:** `Verdict::Reject` and `AcceptanceBasis::ClassifierDecision`
  (`admission.rs:85-124`), exercised at C:5834-5938.
- **Provider abstraction:** the `ProviderSource` enum (C:610-628) and the
  fixture provider share one input, output and link contract. An economy
  worker can implement it.
- **Notice kinds:** `ProviderUnavailable` and `ServiceRefused`
  (`crates/audio/src/notice.rs:41-48`).
- **Freshness:** `Observation` and `ObservationTracker`
  (`crates/interaction/src/observation.rs`).
- **Cue stream:** a bounded, non-authoritative event export
  (`docs/live-session-cues.md:22-38`).
- **Motion proposals:** `MovementOffer` and `Evidence`
  (`coordinator/motor.rs:269-281`).
- **Output kinds:** Speech, Light, Motion (`types.rs:136-140`).

## 7. Constraints a brain must keep (quoted)

- The core handles "small bounded state transitions" and "does not decode
  images, execute models, wait for disk writes … or carry out cloud/tool calls"
  (`docs/architecture.md:229-235`).
- "Bound queues, buffers, worker counts and retry work" (`AGENTS.md`).
- "Keep exactly one writer for each actuator bus" (`AGENTS.md`).
- "Gemini may propose intent; it cannot bypass safety or directly set unchecked
  joint targets" (`AGENTS.md`).
- "Stillness and silence are valid. No automatic room scans, stock spoken
  fillers, mandatory gestures, fabricated visual facts or forced eye contact"
  (`AGENTS.md`).
- "Camera and microphone privacy controls remain authoritative" (`AGENTS.md`).
- Harness or an agent "never writes directly to the speaker, lights or motors"
  (`docs/architecture.md`).
- Sensor data "does not trigger repetitive unsolicited commentary"
  (`docs/architecture.md:63`).
- Failure audio is never synthesized at failure time
  (`docs/failure-notices.md:10-15`). Repeating a notice at every reconnect is
  not acceptable (`docs/failure-notices.md:133-135`).
- Admission processing target: host p99 ≤100 µs inside the 2 ms tick
  (`docs/input-admission.md:84-88`).

## 8. Minimal seams and blockers

**To observe** (typed, allocation-free calls):

1. Per tick, before the ring update (C:2849): a snapshot of privacy, the
   cloud link, the reply, mask availability, and camera, ring and motor
   health.
2. Link changes (C:974-978), so the brain sees `Terminal` before `Ended`.
3. Transcripts (C:2322).
4. Candidates and endpoints (C:2589, C:2695).
5. Turn completion (C:2813, C:3595).

**To gate:**

1. The admission verdict for attention states (C:3158-3168).
2. Notice choice and debounce (C:2757).
3. A speech veto before `issue_output` (C:3353).
4. A ring-intent overlay into `prepare` (C:3325).
5. The terminal exit (C:2829-2848), replaced by a route decision.

**Blockers:**

1. Every output needs an admitted `TurnOwner` (`controller.rs:372-378`). Agenda
   speech, light or a look outside a turn needs a new owner kind in
   `lamp-interaction`.
2. A Gemini tool call is `UnsupportedMessage`, which is fatal. No
   `toolResponse` or `clientContent` sender exists either.
3. No STT or TTS worker exists.
4. There is no volume actuator, memory store, presence evidence or wall clock.
   The core uses `MonoTime` only, so quiet hours need a new input.
5. Visual-frame admission happens inside the provider worker, not the core.
