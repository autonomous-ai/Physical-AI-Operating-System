# Lamp brain

The brain decides whether Lamp speaks or acts, in which voice route, and what it
may do without being asked. This is a design under discussion with the owner;
**nothing is built**. On 12 October the owner asked to settle the design before
any code.

- **Owner of the work.** Agent 2 from 12 October 2026, handed over the same
  day; see [the handoff](brain-handoff.md). Main and Agent 1 own the files the
  brain plugs into (§9).
- **Scope change, 12 October.** Agents, tasks, memory and initiative are now in
  scope for V2. The earlier exclusions in [architecture](architecture.md) and
  `AGENTS.md` are superseded for this work.
- **Owner decisions, 12 October:**
  - initiative is **rare**;
  - **Gemini's own voice everywhere**, with no ElevenLabs, for cost and one
    consistent voice.
- **Recommended, not yet decided.**
  - No agent runtime on the Lamp (§1.1). The owner questioned Hermes as part
    of the redesign but has not confirmed.
  - Everything listed in §11.
- **Evidence.**
  - Every design choice answers a measured V1 problem in the
    [V1 brain baseline](v1-brain-baseline.md). References to `§n` of that
    document are written "V1 §n".
  - Protocol and integration facts are in [brain research](brain-research/README.md).

## 0. How the brain is organized

Gemini is the part that understands and talks: the cortex. "The brain" is
the whole loop around it, from senses to actions, and almost all of that loop
is code that can be tested and measured. In V1 the loop was two models
(Gemini Live and Hermes) following long prompts (V1 §1).

| | V1 | V2 |
| --- | --- | --- |
| Understands and talks | Gemini Live, or Hermes after a handoff | Gemini Live (a fast text model at the usage limit, §3.2) |
| Decides when to listen, speak, act or nudge | mostly prompt text, in both models | code: attention, executive, budgets |
| Memory | Hermes files plus a separate Gemini log | one store owned by lampOS (§7) |
| Long tasks | Hermes on the Lamp | Harness on the paired computer |

### 0.1 Senses

The body is a 5-servo arm, a WS2812 LED ring head, a camera, a microphone, a
speaker, a button, touch pads and a touch strip (V1 `robots/lamp/ROBOT.md` and
its JSON files at `170a1b85e`). V1 had most of these senses; V2 has few of
them today.

| Sense | Hardware | What it tells the brain | lampOS V2 today | V1 |
| --- | --- | --- | --- | --- |
| Hearing | microphone and voice board, echo cancellation | someone is speaking; the words; when they stop; loud or unusual sounds; its own voice, to ignore | speech detection, end of turn, echo cancellation | + speaker ID, tone of voice (cloud), sound events |
| Sight | camera, 1280×720 | someone is present; who; whether they look at Lamp; an object held up; activity; room brightness | frames sent to Gemini with an admitted question; no local perception | face recognition and gaze on device; activity and emotion in the cloud (opt-in) |
| Touch | button (tap, hold); 4 head pads (TTP223); 12-pad strip (MPR121) | tap = stop, hold = sleep, pat, swipe | privacy switch only | all gestures |
| Body feel | servo position, load and temperature | pushed or moved by hand; overload; heat; own pose | motor readback; motion inhibited | contact detection, overload stop |
| Inner state | software | Gemini reachable or over its limit; network; privacy; worker health | provider link state, privacy | partial |
| Time | clock | time of day; time since the last conversation; quiet hours | monotonic only | yes |
| Air (Pro models) | CO2 and particulate sensors | stuffy room | types only (`crates/environment`) | Pro only |
| Extended | paired computer, phone | in a meeting; a task finished; a message | none | Buddy and Harness events |
| Memory | storage | who the person is; what they asked Lamp to keep | none | Hermes files |

### 0.2 Actions

| Action | Through | Examples | lampOS V2 today |
| --- | --- | --- | --- |
| Speak | speaker, Gemini's voice | answer, remind, stop mid-sentence, volume | speak and stop; no volume control |
| Look | head and arm | turn toward a voice, look at the person or at a held object | inhibited until motion qualifies |
| Body language | head and arm | small nod, tilt, lean in, settle, sleep pose | inhibited |
| Light as a lamp | LED ring | on, off, brightness, warmth, aim at the work area | none |
| Light as expression | the same LED ring | listening, thinking, a soft acknowledgment | fixed ring policy |
| Attend | microphone, camera | listen, ignore, take a closer look | listening |
| Remember | storage | remember, forget | none |
| Act in the world | paired computer, phone | a Harness task, a reminder, music, a message | none |

**Lamp's first job is to be a good lamp.** Light and head position are its
most useful actions, and they share the LEDs and motors of its expressions.
The brain must arbitrate between them. For example, it never shows a
"thinking" color while the person reads under the light.

### 0.3 Layers

```text
 senses ─► REFLEXES ───────────────────────────────────────────► actions
           privacy cuts ears and eyes · a tap stops speech     (instant, code)
           a push relaxes the arm · do not hear your own voice
   │
   ▼
 PERCEPTION     raw senses → facts with confidence and age     (local, ~10–500 ms)
   "speech started" "looking at me" "a person is present" "patted"
   │
   ▼
 ATTENTION      asleep · resting · attentive · in conversation  (code)
   decides what reaches the thinking layer (§4)
   │
   ▼
 THINKING       Gemini Live: understand, decide, talk          (cloud, ~0.5–3 s)
   proposes actions ▼
 EXECUTIVE      rules, budgets, quiet hours, reminders, vetoes (code, §5, §6)
   │
   ▼
 MOTOR PLANNING one intention → voice + head + light, in time  (code)
   smooth, safe, interruptible
   │
   ▼
 actions        speaker · light · head and arm · computer tasks

 MEMORY beside the loop: the Gemini session is short-term, the store long-term (§7)
 SLOW WORK handed off like using a computer: Harness on the paired machine
```

**Where each layer lives:**

- **Reflexes** are in the lampOS core today: privacy, admission, Stop,
  permits.
- **Motor planning** extends the choreographer. Ownership stays in
  `lamp-interaction`.
- **Executive and attention** are the `lamp-brain` crate (§2).
- **Perception** grows as separate supervised workers. The core does no
  inference (`architecture.md`).

## 1. Principles

1. **Models propose; code decides.** V1 left addressing, speaking,
   thresholds, cooldowns and body language to prompt text, and executed
   model-written hardware markers after a capability check only (V1 §1, §4).
   - In V2, every action reaches an actuator only after the brain accepts a
     typed proposal.
   - Acceptance happens inside the existing ownership, permit and safety
     checks, never instead of them.
2. **Conversation stays on the fast path.**
   - V1 sent doubtful turns to Hermes, and those turns took 6–22 s (V1 §10).
   - In V2, conversation is answered by the voice model itself. There is no
     on-device agent runtime (§1.1). Work that needs a computer goes to
     Harness on the person's paired computer.
3. **One voice.**
   - V1 discarded Gemini's audio and paid ElevenLabs to say the same text
     (V1 §8).
   - In V2, Gemini Live speaks, and every other path uses the same Google
     voice.
4. **Restraint.**
   - Silence and stillness are valid ([architecture](architecture.md)).
   - V1's persona demanded physical expression on every response (V1 §6, §11).
   - Unprompted speech is rare, budgeted in code and logged with its reason.
5. **Honest degraded modes.**
   - V1 degraded silently at the Gemini limit; V2 today exits the service
     there (§6).
   - Every degraded mode either keeps Lamp useful or says once, plainly, what
     is wrong.
6. **Memory has one owner and cannot steer routing.** V1 had four stores, and
   memory poisoning hijacked routing (V1 §7, #421).
7. **Measured or not done.**
   - V1 never measured its acknowledgment target, false wakes or false replies
     (V1 §10).
   - Each brain feature ships with its metric and a voice-eval scenario.
8. **No added latency.** Brain calls in the core are bounded, allocation-free
   decisions inside the existing 2 ms loop. Their budget is the host p99
   ≤100 µs that [input admission](input-admission.md) already meets.

### 1.1 No on-device agent runtime

V2 does not run Hermes, OpenClaw or any other agent runtime on the Lamp.
V1's Hermes had five jobs, and each now has a better home:

| Hermes job in V1 | Home in V2 | Why it is better |
| --- | --- | --- |
| Conversation at the Gemini limit | the economy route's fast text model (§3.2) | about 1–2 s instead of 6–22 s; the same persona as Live |
| Device control through skills (`led-control`, `scene`, `servo-control`, `audio`, `voice`, `camera`) | typed tools the brain validates (§6) | checked in code, not marker text with no body validation (V1 §4) |
| Reactions and nudges (`sensing`, `mood`, `wellbeing`, `habit`, `music-suggestion`, `user-emotion-detection`) | the agenda with budgets in code (§5) | rare by construction; thresholds are tested constants, not prompt text |
| Memory (`MEMORY.md`, `USER.md`) | a small lampOS memory store (§7) | one owner; no routing or endpoints in memory (V1 #421) |
| Real work (`harness-use`, `computer-use`, `connectors`, `claude-buddy`) | Harness on the paired computer (§6) | agents, connectors and permission prompts already live there |

Keeping an on-device agent would also cost the following:

- **lampOS rules.** Hermes is a Python service. lampOS forbids Python
  services on the Lamp (`AGENTS.md`).
- **Security.** V1 runs it as root with command approvals switched off
  (V1 `runtimes/hermes/presync.sh:74-76`).
- **Resources.** It is a large resident process on the A733, beside a
  voice loop with little headroom.
- **Behavior.** Its own persona and memory drift from Live's, and its
  behavior is decided by prompt.

**What V2 gives up for now:**

- the Telegram or app text channel;
- user-authored skills (`skill-creator`);
- direct MCP connectors on the Lamp;
- music playback (`music`);
- face and speaker enrollment and guard mode as agent skills.

Connectors and user skills are available through Harness. Music,
enrollment and a text channel return as typed device features if the owner
wants them.

## 2. Shape

```text
capture ─► core: reflexes (privacy, admission, endpoint, Stop, permits)
               │ typed observations
               ▼
        lamp-brain  (deterministic, no I/O)
          route · attention · arbiter · agenda · budgets
               ▲ proposals                      ▲ results
               │                                │
   provider worker (one per service cycle)      memory store (lampOS)    (phase 2)
     Live:    Gemini Live, native audio, tools  task link ⇄ Harness on the
     Economy: STT → text mind → Gemini TTS        paired computer         (phase 2)
               │ audio
               ▼
        core: speaker / ring / motor writers (unchanged owners and guards)
```

- **`lamp-brain`.** A new crate with no I/O, no threads and no clock reads;
  time is passed in. It is like `lamp-interaction`, but holds policy, not
  authority. The core calls it at the existing seams (§9) and records each
  decision as a trace row.
- **Provider workers.** Each one keeps today's `ProviderInput` /
  `ProviderOutput` contract (`crates/live/src/provider_worker.rs:126-200`).
  The economy route is a third `ProviderSource`
  (`crates/live/src/coordinator.rs:610-628`), so admission, endpointing,
  playback, ownership and AEC reference are unchanged for it.
- **The task link (phase 2).**
  - It is a supervised Rust process that owns the connection to Harness on
    the paired computer. It reuses Harness's pairing and task contract, and
    does not invent its own (the parent repository's `docs/harness.md`
    describes the V1 client).
  - Results come back as agenda items or tool results. Harness never writes
    to an actuator, as [architecture](architecture.md) already requires.
  - Lamp converses without Harness; only computer work needs it.
- **The memory store (phase 2).** It is a small file owned by lampOS (§7).

## 3. Voice routes

| Route | Listening | Thinking | Speaking | When |
| --- | --- | --- | --- | --- |
| **Live** (default) | Gemini Live audio input, manual activity (as today) | `gemini-3.8-live`, no extended thinking | Gemini Live native audio, configured voice (default Kore) | Gemini available |
| **Economy** | lampOS capture and endpoint; STT through the Autonomous relay | text mind (§3.2) | `gemini-3.8-flash-lite-tts`, **the same voice name** | Gemini Live refused for quota or usage limit |
| **Unavailable** | — | — | one prerendered notice in the same voice | both routes refused, or no network |

### 3.1 Choosing the route

- **Owner of the choice.** The brain chooses the route at a **service-cycle
  boundary**, when no turn is active. The core never swaps a provider during
  a turn.
- **Today.** A terminal provider link ends `serve` with an error
  (`coordinator.rs:2829-2847`), and the pilot unit does not restart
  (`deploy/4ace-attended/lamp-live-pilot@.service:17`). At the limit, Lamp
  simply stops.
- **New behavior.** That exit becomes a cycle end with reason
  `route_change`. `service::supervise` then starts the next cycle with the
  route the brain selects.
- **Live → Economy** on any of:
  - `QuotaExceeded` (`crates/gemini/src/session.rs:1946-1966`,
    `wire.rs:250-256`);
  - relay close code 4029 (`crates/gemini/src/lib.rs:424-428`);
  - a terminal link whose fault is quota or rate limit.
- **Economy → Live.**
  - The brain probes Live only while attention is idle (§4), with a backoff:
    15 min, then 30, then hourly.
  - The probe is a connect-only session with no audio.
  - It switches back at the next idle cycle boundary.
  - Lamp does not announce the return.
- **Economy → Unavailable** when the economy services refuse too. One
  prerendered notice plays, and the brain backs off as above.
- **Network or credential failures** keep today's recovery and failure
  notices ([provider recovery](provider-recovery-integration.md)). They are
  not route changes.
- **The notice.**
  - After a switch to Economy, the first economy reply begins with one short
    sentence, for example "I'm on a slower connection for a bit."
  - It is said once per switch, inside the person's own turn. It is never an
    unprompted announcement into the room, and never repeated at each
    reconnect ([failure notices](failure-notices.md)).

### 3.2 The economy text mind

- **A fast text model** (`gemini-3.8-flash` through the Autonomous relay):
  - the same persona text and memory snapshot as Live;
  - the last few exchanges carried over as text.
- **Not Hermes.** V1 measured Hermes at 9–13 s for "hello" and 6–22 s for
  delegated turns (V1 §10), and V2 runs no agent on the Lamp (§1.1).
- **Target.** End of speech to first audio ≤2.0 s p50 and ≤3.0 s p90 on the
  host, then measured on the Lamp. These are candidate targets, unmeasured.
- **Pipeline:**
  1. **Capture and end of turn.** Capture and the local endpoint are lampOS's
     own (silence, or the opt-in fast endpoint, `docs/endpoint-fast.md` on
     `lamp-a2-endpoint-fast`).
  2. **Speech to text.** Each turn streams to the Autonomous relay at
     `{base}/v1/ws/audio/transcriptions` with header
     `Authorization: Token <key>`. Settings:
     - `nova-3-general`, linear16 at 16 kHz;
     - interim results on;
     - `keyterm` for "Lamp" and the wake words.

     That is V1's protocol (V1 `hal/drivers/voice/stt/autonomous.py`).
  3. **Text mind.** It runs at the end of turn and streams sentences.
  4. **Speech synthesis.** Sentences go to
     `{base}/v1/google-search/v1beta/models/{model}:streamGenerateContent?alt=sse`
     with header `x-goog-api-key` and `prebuiltVoiceConfig.voiceName`.
     - It returns 24 kHz PCM, sent as `ProviderOutput::Audio`.
     - The first chunk goes out at about 60 characters, growing to 520, as V1
       does (V1 `hal/drivers/voice/tts/service.py:1500-1545`).
     - The 3.8 TTS model is not usable inside Live, so the two clients stay
       separate.

### 3.3 One voice

- **What is documented.** Google states that Live native audio supports the
  TTS voices. It does not state that a voice name sounds the same in both.
- **What we check.** A listening comparison of Kore through Live and through
  `gemini-3.8-flash-lite-tts` comes before we call the routes one voice.
- **The notices.**
  - Failure notices (`crates/audio/src/notice.rs:41-48`) and the
    economy-switch sentence are rendered once, at provisioning, with the TTS
    route in the configured voice.
  - They are stored as validated assets: never synthesized during a failure
    ([failure notices](failure-notices.md)).

### 3.4 Cost

Google's prices for the 3.8 Live models are $3 per million tokens of input
audio and $12 per million of output audio, at 25 tokens per second. The whole
session context is billed again on every turn.

- **Instruction size.**
  - The instruction stays small, at most about 2,000 tokens with tools and the
    memory snapshot.
  - V1's realtime prompt was about 42 KB, before Hermes's persona and
    memory were added (V1 §3).
- **Context.** Compression is configured
  (`contextWindowCompression.slidingWindow`), and the session is recycled when
  idle.
- **Unchanged.** Input audio is sent only for admitted speech, as today.
- **Recorded.** Each cycle records the provider's usage metadata, so cost per
  active hour is measured, not estimated.
- **Economy cost.** Text plus TTS runs $0.009–0.0135 per spoken minute through
  2026. It has no audio context to re-bill.

## 4. Attention

The brain keeps one attention state from typed observations. Each state lists
what it allows.

| State | Evidence | Unprompted speech | Tools and replies |
| --- | --- | --- | --- |
| Muted | privacy closed or unavailable (`crates/live/src/privacy.rs`) | no | no output; privacy is already authoritative |
| Engaged | a turn active, or the last exchange less than 20 s ago | no; replies only | yes |
| Idle | none of the above | only within budget (§5) | — |
| Quiet hours | local wall clock inside the window (proposed default 22:00–07:00, V1's night-dimming window) | no, except a task result the person asked to hear | replies as usual |
| Away (phase 2) | no presence evidence for 15 min | no; items wait or expire | — |
| Busy (phase 2) | another conversation or a call, from admission evidence | no | replies only to clearly addressed speech |

### 4.1 Attention gates what Gemini hears

Without a gate, every sentence in the room is streamed to Gemini. That is
billed as input audio, sent to the cloud, and left to Gemini to judge. The
recommended order puts cheap, local evidence first:

1. **A wake word or Lamp's name** anywhere in a sentence opens a conversation.
   So does looking at Lamp, once camera presence exists.
2. **Follow-ups** inside the conversation need neither.
3. **`not_for_me`** stays as Gemini's second check inside a conversation
   (§6). Gemini calls it instead of answering when the speech was not
   addressed to Lamp. Code then guarantees nothing is spoken, and counts it.

The options and their costs:

| Approach | Reliability | Experience | Cost and privacy |
| --- | --- | --- | --- |
| Wake word, as on new V1 Lamps | high, decided on the device | worse: said every time | only addressed speech leaves the device |
| Wake word plus a conversation window (V1) | high | better | good |
| Local evidence: gaze, the name anywhere, an ongoing conversation | medium, measurable | close to "just talk" | good |
| No gate; Gemini decides with `not_for_me` | lowest, a model judgement | best when it works | the whole room goes to the cloud |

The owner has not chosen; this is open question 1 in §11. The owner's
earlier experience was that tap-to-talk is reliable but a poor experience,
and just-talk is the best experience but unreliable.

**Inputs and gaps:**

- **Quiet hours need a wall clock.** The core uses monotonic time only, so
  the core passes local time and the zone offset as an observation.
- **Presence and busy need perception.** They start in phase 2, together with
  camera presence; until then Lamp is never treated as Away or Busy.
- **No room scans.** The state is never inferred from a scan, and missing
  evidence stays missing ([architecture](architecture.md)).

## 5. Initiative: the agenda and its budgets

**The agenda.** It holds at most 16 items. Each item records:

- its kind;
- who asked for it;
- when it was created and when it expires;
- its priority;
- its text, or a reference to a task result.

Expired items are dropped and logged. The person can ask "anything for me?",
which makes the next item speak as a reply rather than as initiative.

| Kind | Example | Default budget | Phase |
| --- | --- | --- | --- |
| Asked-for result | "Your report is ready." (a Harness task) | not counted against initiative; held in quiet hours unless the person asked for it now | 2 |
| Reminder | "It's 3 pm: call Anna." | person-set, delivered on time unless Muted; quiet hours honored unless set inside them | 2 |
| Return greeting | "Welcome back." after ≥2 h away | at most 1 per day | 2 |
| Degraded-mode notice | the economy sentence | once per switch, inside a turn (§3.1) | 1 |

**Initiative budget (rare).** These are code constants, changeable only
through configuration, never through prompt text:

- at most 1 unprompted utterance per 30 min;
- at most 4 per day;
- none while Engaged, Muted, Busy, Away or in quiet hours.

Every release or refusal is a trace row with its reason and the budget state.

**Delivery.**

- **Live:** the item becomes a short text turn the model voices
  (`clientContent`). Google documents a transcript side effect for one
  variant, which must be tested first.
- **Economy:** speech synthesis directly.

Phase 2 needs the core to support unprompted output: today every output needs
an admitted `TurnOwner` (`crates/interaction/src/controller.rs:372-378`).

## 6. Tools and the arbiter

**Declaration.**

- Tools are declared once, at Live session setup
  (`setup.tools[].functionDeclarations`, explicit `behavior: BLOCKING`).
- The brain owns the declarations and the validation. The provider worker
  carries `toolCall` / `toolResponse` / `toolCallCancellation` without
  interpreting them.

| Tool | Arguments | Validation | Executor | Phase |
| --- | --- | --- | --- | --- |
| `not_for_me` | — | always accepted; the turn ends without reply | core: cancel the reply, record `addressee_rejected` | 1 |
| `set_light` | `level` 0–100, or `on` true/false | range; quiet-hours brightness cap; privacy | ring/light owner, through its permit | 1 |
| `set_volume` | `level` 0–100, or `step` ±1 | range; never fully mute speech without saying so | speaker volume (no volume control exists yet) | 1 |
| `remember` | `fact` (≤200 characters) | rejects text that prescribes tools, endpoints or routing (V1 #421) | memory store | 2 |
| `forget` | `fact` | — | memory store | 2 |
| `start_task` | `request` | Harness paired and connected; duplicate check | task link → Harness | 2 |
| `set_reminder` | `when`, `what` | parses to a wall-clock time in the future | brain agenda | 2 |
| `gesture` | name from a fixed set | motion qualified; per-gesture rate limit | motor owner | 3 |

**Execution rules:**

- **Act first, then report.** The `toolResponse` reports what actually
  happened, or the refusal reason, so the model never claims an action
  before it is done (V1 #564).
- **Cancellation.** A `toolCallCancellation` after an interruption undoes a
  reversible action, such as restoring the previous light level, and is
  recorded.
- **Superseded turns.** A proposal from a superseded turn is dropped.
- **Limits.** Each tool kind has a rate limit, for example `set_light` at
  most 4 per 10 s.
- **The economy route** has no tool calls in phase 1. Its text mind answers
  in speech; light and volume commands there wait for phase 2. A small set of
  exact local commands, as in V1's fast path, may be added if the pilot needs
  them.

**Prerequisites.**

- Today a Gemini tool call is a fatal session error
  (`crates/gemini/src/wire.rs:269-271`, `lib.rs:431-449`).
- The tools need Agent 1 to add the wire messages (`tools` in setup,
  `toolCall`, `toolResponse`, `toolCallCancellation`, `clientContent`,
  `usageMetadata`).
- They also need Main to add the coordinator path from `ProviderOutput` to
  the brain and to the executors.

## 7. Persona, memory and context

**The persona.**

- One short English persona text, owned by the brain, is used by both Live
  and the economy text mind.
- It keeps the [architecture](architecture.md) restraint rules (stillness
  valid, no fillers, no invented sight).
- It replaces V1's "always act physically", and it contains no routing rules
  (§1).

**Memory (phase 2).**

- **The store.** One small lampOS file on the Lamp. Each fact records its
  text, when it was saved, and the utterance that asked for it.
  - The store is bounded, for example 200 facts.
  - It is private to the service user, like the provider configuration.
- **Reads.** At session start, a snapshot of at most 1,500 characters goes
  into the Live instruction and into the economy text mind.
- **Writes.**
  - The person writes through `remember` and `forget`.
  - Automatic extraction after a conversation is a later option. It would
    run on the cheap text model outside the live loop, be measured, and be
    off until it is.
- **Not memory.** The brain never puts tools, endpoints or routing into
  memory. The Live conversation context stays temporary working memory.
- **What Lamp claims.** Lamp never claims to remember what the store does not
  hold ([architecture](architecture.md)).
- **V1 memory.** It is not migrated automatically. V1's `USER.md` and
  `MEMORY.md` from the backed-up Lamp can be reviewed and imported once,
  deliberately.

**Session resumption.** V1's proxy does not forward Gemini session resumption
(V1 `hal/config.py:754-757`), and the V2 provider recovery assumes it.
Agent 1 needs to confirm what the current relay supports.

## 8. Metrics and acceptance

Each metric has a voice-eval scenario (`crates/voice-eval`) and a trace
source. Host results are candidates. Lamp results come from Main's scheduled
device runs.

| Metric | Definition | Phase-1 target |
| --- | --- | --- |
| Route switch | from the quota signal to the first economy answer audible | next turn answered; notice exactly once |
| Economy latency | end of speech to first audio, p50 / p90 | ≤2.0 / ≤3.0 s host, then device |
| Tool truth | the tool result matches the executed state; the light changes within 300 ms of the call | 100% on the scripted service |
| `not_for_me` | false replies on the overheard cohort; missed address on the addressed cohort | measured, then a target set |
| Unprompted speech | count per hour and per day; in quiet hours; during Engaged | ≤ budget; 0; 0 |
| Cost | provider usage per active hour, by route | recorded per cycle |
| No regression | time to first audio and interruption on the existing Live scenarios | unchanged within noise |

### 8.1 Compared with V1

The senses and actions are almost the same as V1's: it is the same body.
What changes is who decides, in what order, and whether each decision is
checked and measured.

| | V1 (`main`) | V2 design | V1 evidence |
| --- | --- | --- | --- |
| Who decides | four layers in sequence; most judgement is prompt text | Gemini handles meaning only; attention, budgets, quiet hours and action limits are code | rules forgotten around turn 10 (#449); memory poisoning (#421) |
| Paths to an answer | Gemini answers or hands off to Hermes | one path | 6–22 s handed-off turns; stale replies 12–28 s after a stop |
| Voice | Gemini audio discarded, ElevenLabs re-speaks | Gemini's own voice | 4.0 s median to first sentence |
| Interrupting Lamp | microphone discarded while speaking; only a tap stops it | listening during playback (already in V2) | `voice_service.py:1119-1138` |
| Is it for Lamp | wake word on or off by first-boot date, then the model | one attention state from local evidence first (§4.1) | two Lamps on one build differ |
| Actions | model-written markers fired after a capability check; eight light writers | typed proposals, one writer per actuator, newer turns cancel older gestures | no body validation or rate limit |
| Senses | silent failures | each perception carries age and confidence | sound stops with the camera; guard never fires on motion |
| Initiative | 45–120 s timer moves; prompt-set nudges | rare, budgeted, logged | — |
| At the limit | silent fallback | explicit route and one honest sentence | — |
| Safety | agent as root, approvals off | no agent on the Lamp | `presync.sh:74-76` |
| Proof | almost nothing measured | a metric per feature | acknowledgment target: 0 eligible samples |

**Not better yet.**

- Today V2 does less than V1: no tools, local perception, touch gestures,
  memory or motion.
- V2 exits at the Gemini limit.
- Every row above is an architectural property, not a measured result.

**The bar.** Run the same scenarios on both, with V1's numbers as the target:

- time to first word (V1: 4.0 s median);
- handed-off turns (V1: 6–22 s);
- interruption by voice (V1: none);
- false replies to overheard speech, and missed addressed speech (V1: never
  measured);
- stale replies after a stop (V1: 12–28 s);
- cost per active hour.

## 9. Ownership and seams

**The brain workstream (Agent 2 until the handoff):**

- `crates/brain`, a new crate;
- the economy provider worker and its STT, text-mind and TTS clients (new
  crates);
- the memory store and the Harness task link (phase 2; the link is
  coordinated with the Harness team);
- persona text and tool declarations;
- this document;
- the brain voice-eval scenarios.

**Proposed to Agent 1** (`crates/gemini`, as separate commits):

- tool and `clientContent` wire messages;
- usage metadata;
- the relay's resumption support.

**Proposed to Main** (shared files, small commits):

1. **Observe:** typed calls at the seams in `coordinator.rs`:
   - per-tick snapshot, before the ring update at about line 2849;
   - link changes at about lines 974-978;
   - candidates and endpoints at about lines 2589 and 2695;
   - turn completion at about lines 2813 and 3595.
2. **Route:** replace the terminal exit (lines 2829-2847) with a cycle end,
   and choose `ProviderSource` per cycle in `coordinator/service.rs`.
3. **Tools:** route `ProviderOutput` tool calls to the brain, and its
   accepted actions to the light and volume executors.
4. **Phase 2:** an unprompted-output owner kind in `lamp-interaction`.
5. **Volume:** a speaker volume actuator, decided together with Agent 1
   (`crates/audio`).

## 10. Phases

**These phases predate the senses-and-actions discussion (§0).** Re-cut them
once the owner answers §11, items 1, 3, 5 and 6.

**Phase 1, before launch.** It closes the gap where Lamp stops at the Gemini
limit, and adds the arbiter.

1. The `lamp-brain` crate: route, attention (Muted, Engaged, Idle, quiet
   hours), budgets and tool validation, with unit tests. It has no
   dependencies.
2. The economy provider worker. Its STT, text-mind and TTS clients are tested
   against scripted servers, then through the real conversation loop, as the
   fast endpoint was (`lamp-a2-endpoint-fast`).
3. The route seam, the prerendered notices and the voice comparison (§3.3).
4. The `not_for_me`, `set_light` and `set_volume` tools, once the wire and
   coordinator seams land.

**Phase 2, first update:**

- the memory store and its snapshot;
- the agenda and reminders;
- the Harness task link;
- presence from the camera;
- the unprompted-output owner.

**Phase 3:** gestures through the choreographer, once motion qualifies.

## 11. Open questions for the owner

Decisions the owner has not made yet, with the current recommendation:

1. **How Lamp knows it is addressed** (§4.1). Wake word, wake word plus a
   conversation window, local evidence, or no gate.
   - Recommended: the layered approach, with `not_for_me` as the second
     check.
2. **No on-device agent** (§1.1). Recommended. The owner questioned Hermes
   but did not confirm dropping it.
3. **Senses at launch** (§0.1). Is local camera perception in (presence, who,
   looking at Lamp)? It drives attention, greetings and "light where I work".
   Its A733 cost is unmeasured.
4. **Touch.** Bring V1's vocabulary over (tap = stop, hold = sleep, pat)?
5. **Motion at launch.** Look and body language depend on Main's motor
   qualification. Launch requirement or update?
6. **How smart the light is.** Voice commands only, or reacting to the person
   (dim on leaving, follow the work area)?
7. **"Alive" versus "rare."** Do silent reactions, such as glancing toward a
   voice or a light pulse on return, count against the unprompted-speech
   budget? Recommended: no, but they get their own limits.
8. **Memory.** Only "remember this", or also automatic extraction from
   conversation? Recommended: explicit first; automatic only once measured
   (V1 was poisoned, #421).
9. **The usage limit**, for the backend team (§3.1):
   - Which limits exist per device and service?
   - What does the relay send at each?
   - When do they reset?
   - Can a device read what is left?
   - Do they differ by plan?

   The economy route only helps if its services are not refused too.
10. **Defaults.** Proposed:
    - switch sentence "I'm on a slower connection for a bit.";
    - at most 1 unprompted utterance per 30 min and 4 per day;
    - quiet hours 22:00–07:00.
11. **Features dropped with Hermes** (§1.1). Should a text channel, music or
    enrollment return as typed device features, and when?
12. **The Harness task contract.** It is checked with the Harness team before
    phase 2.
13. **Speech-synthesis latency.** Undocumented. Measure it before promising
    the economy target.
