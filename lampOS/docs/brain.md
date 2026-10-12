# Lamp brain

The brain decides whether Lamp speaks or acts, in which voice route, and what it
may do without being asked. This is the design; nothing here is built yet.

- **Owner.** Agent 2, by the owner's decision of 12 October 2026. Main and
  Agent 1 own the files the brain plugs into (§9).
- **Scope change, same day.** Agents, tasks, memory and initiative are now in
  scope for V2. The earlier exclusions in [architecture](architecture.md) and
  `AGENTS.md` are superseded for this work.
- **Owner decisions:**
  - initiative is **rare**;
  - **Gemini's own voice everywhere**, with no ElevenLabs, for cost and one
    consistent voice.
- **Baseline.** Every design choice answers a measured V1 problem in the
  [V1 brain baseline](v1-brain-baseline.md). References to `§n` of that
  document are written "V1 §n".

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
   - In V2, conversation is answered by the voice model itself. The slow
     agent gets only work that needs it.
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

## 2. Shape

```text
capture ─► core: reflexes (privacy, admission, endpoint, Stop, permits)
               │ typed observations
               ▼
        lamp-brain  (deterministic, no I/O)
          route · attention · arbiter · agenda · budgets
               ▲ proposals                      ▲ results
               │                                │
   provider worker (one per service cycle)      agent worker ⇄ Hermes   (phase 2)
     Live:    Gemini Live, native audio, tools    tasks · memory · skills
     Economy: STT → text mind → Gemini TTS
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
- **The agent worker (phase 2).**
  - It is a supervised process that owns the connection to Hermes.
  - Hermes results come back as agenda items or tool results. Hermes never
    writes to an actuator, as [architecture](architecture.md) already
    requires.
  - Lamp converses without Hermes; only tasks and memory need it.

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

- **Recommendation: a fast text model** (`gemini-3.8-flash` through the
  Autonomous relay):
  - the same persona text and memory snapshot as Live;
  - the last few exchanges carried over as text.
- **Alternative: Hermes, for V1 parity.** Rejected for conversation: V1
  measured Hermes at 9–13 s for "hello" and 6–22 s for delegated turns
  (V1 §10). Hermes still handles tasks in either route (phase 2).
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
| Quiet hours | local wall clock inside the window (default 22:00–08:00) | no, except a task result the person asked to hear | replies as usual |
| Away (phase 2) | no presence evidence for 15 min | no; items wait or expire | — |
| Busy (phase 2) | another conversation or a call, from admission evidence | no | replies only to clearly addressed speech |

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
| Asked-for result | "Your report is ready." | not counted against initiative; held in quiet hours unless the person asked for it now | 2 |
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
| `remember` | `fact` (≤200 characters) | rejects text that prescribes tools, endpoints or routing (V1 #421) | agent worker → Hermes memory | 2 |
| `forget` | `fact` | — | agent worker | 2 |
| `start_task` | `request` | Hermes available; duplicate check | agent worker → Hermes / Harness | 2 |
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

- One short English persona text, owned by the brain, is used by Live, the
  economy text mind and Hermes.
- It keeps the [architecture](architecture.md) restraint rules (stillness
  valid, no fillers, no invented sight).
- It replaces V1's "always act physically", and it contains no routing rules
  (§1).

**Memory (phase 2).**

- **The store.** Hermes's `MEMORY.md` and `USER.md` are the single store.
- **Reads.** At session start the agent worker supplies a snapshot of at
  most 1,500 characters.
- **Writes.** They happen only through `remember` and `forget`.
- **Not memory.** The brain never puts tools, endpoints or routing into
  memory. The Live conversation context stays temporary working memory.
- **Without Hermes.** Lamp has no memory and does not claim to remember
  ([architecture](architecture.md)).

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

## 9. Ownership and seams

**Mine (Agent 2):**

- `crates/brain`, a new crate;
- the economy provider worker and its STT, text-mind and TTS clients (new
  crates);
- the agent worker (phase 2);
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

**Phase 2, first update:** the agent worker (Hermes tasks), the agenda,
reminders, the memory snapshot, presence from the camera, and the
unprompted-output owner.

**Phase 3:** gestures through the choreographer, once motion qualifies.

## 11. Open questions

1. **The limit.** What exactly does the Gemini limit cover: only Live, or
   every model on the key? What resets it, and what does the relay send?
   V1 recorded no code (V1 §8). The economy route only helps if its services
   are not refused too.
2. **The economy text mind:** a fast text model (recommended) or Hermes
   (§3.2).
3. **The defaults:** the wording of the economy sentence, the budget values,
   and quiet hours.
4. **Hermes on the A733.** Memory and CPU are unmeasured, and so is whether
   it runs on the device or off it, for phase 2.
5. **Speech-synthesis latency.** First-audio latency is undocumented. It is
   measured in step 2 of phase 1.
