# V1 brain audit: capabilities to preserve and evidence for V2

Date: 12 October 2026. Status: design research; no implementation changes.

The owner asked for a full understanding of V1 before designing a substantially
better V2. This audit corrects an earlier discussion that underrepresented
Deepgram, sensor-driven agency, context sharing, and local control in V1.

The subsequent full review adds an important correction: **V1 already uses
Jev** for intent fallback, skill preloading, Harness target selection and
experimental Buddy suggestions. Those paths exist in the pinned baseline.
The [decision-model study](brain-research/decision-models-study.md) records their
exact eligibility, confidence and failure rules. The
[58-capability migration inventory](brain-research/v1-migration-inventory.md)
extends this audit to the full OS and later source changes. For the resulting
recommendation, read [V2 architecture](v2-architecture.md) and
[build plan](v2-build-plan.md); this audit preserves its historical source pins.

## Evidence boundary

- V1 source: `170a1b85e7e6197097019e4175efdcbe416e3c2c`.
- Previous research: `lamp-a2-brain` at `1e179f994`, particularly
  `lampOS/docs/v1-brain-baseline.md` and its six `v1-baseline/` reports.
- Method: four independent code-reading scopes covering voice; runtime and
  memory; perception; and body/reliability, followed by cross-checks of key
  source and test definitions. Previous notes were treated as pointers.
- Source references below are paths and line numbers at the pinned V1 commit,
  unless explicitly marked V2. Read them with `git show 170a1b85e:<path>`.
- This establishes code behavior and supported configuration, not fleet
  deployment, physical performance, or upstream service behavior. No tests or
  device experiments were run. No private backup, credentials, face data, or
  office recordings were accessed.
- This addendum is saved in the current worktree under `lampOS/docs/`; it does
  not modify the separate `lamp-a2-brain` worktree or its historical reports.

## 1. What V1 actually is

V1 is already a distributed intelligence system: a realtime voice frontend, an
agentic engine, perception, contextual memory, local command handling, body
control, and service supervision. Hermes is the runtime that manages model/tool
execution; the configured model makes semantic judgments. They are not the same
component.

```text
microphone -> local voice frontend -> Deepgram + Gemini Live
              |                         |
              |                 answer -> HAL TTS; reject -> drop
              |
              +-- admitted text / delegation / fallback ----------+
                                                                 |
camera / sound / touch / environment -> perception -> events ------+
                                                                 |
web / MQTT / schedules / external conversation history -----------+
                                                                 |
                                                                 v
                                                    os-server intake
                                                context, gates, queues
                                                     |         |
                                              local command    v
                                                     |   Hermes runtime
                                                     |   model + tools
                                                     |   skills + memory
                                                     |   managed runs
                                                     |      |       |
                                                     |      |       +->
                                                     |      |    Harness /
                                                     |      |    connectors
                                                     v      v
                                                    os-server + HAL
                                                 TTS, light, motion, music
                                                 safety and body ownership

Separate context paths link Gemini, Hermes, heard speech and external history.
Some reflexes and body policies act locally without an agent turn.
```

This is a logical map, not a claim that both voice routes receive every sample
at the same instant. Wake authorization, transcript guards, selected voice mode
and provider availability change the route. In the standard configuration,
ElevenLabs speaks both Gemini's answer text and Hermes's text; Gemini's generated
audio is discarded. Selecting Gemini TTS can instead enable native Gemini audio.

Sources: `hal/drivers/voice/voice_service.py:2586`, `:2731`;
`hal/drivers/voice/_internal/turn_dispatch.py:177`;
`hal/realtime/voice_agent/gemini_live.py:240`;
`hal/drivers/voice/tts/gemini.py:108`;
`system/lib/sensingmsg/sensingmsg.go:59`.

## 2. Capability inventory

Presence in this inventory is a parity question, not a decision that every
capability must launch in V2. Each requires an explicit keep/change/defer/drop
decision before it disappears from the design.

| Area | What V1 already contributes | What the V2 design must account for |
| --- | --- | --- |
| Speech frontend | Streaming Deepgram transcripts; pre-roll, VAD, echo/noise checks, wake/follow-up focus, language checks, local endpointing and optional semantic endpoint model | First words, ambiguous audio, turns with no transcript, interrupted speech, and usable fallback; Deepgram is more than a backup recognizer |
| Realtime conversation | Gemini can answer, reject or delegate; it has search and a guarded `look` tool, per-turn identity/time/language context, and context about pending agent questions | Fast conversation plus correct task handoff, active vision, grounded answers, and continuity of clarification questions |
| Sensor-driven agency | Selected presence, activity, sound, facial/speech-emotion, environment and touch events reach Hermes with relevant contextual digests | The agentic engine must understand the sensed world and may reason without an explicit spoken request |
| Agent execution | Capability-dependent skills, tools, model calls, native managed runs, steering, stop, submission uncertainty handling, and a Responses fallback | Multi-step goals, retries, task identity, user corrections, and truthful results; do not replace this with predefined device commands |
| Identity and personalization | Enrolled faces/voices, known/unknown people, familiar strangers, user metadata and per-person histories | Who a fact or task belongs to, uncertainty, enrollment quality, multiple people and stale identity |
| Memory and context | Persona/profile/durable memory, independent realtime history and summary, silent external-history replay, per-person activity/mood/habit records | Persistent knowledge, current conversation, task state and observational history have different lifetimes and owners |
| Initiative and time | Sensing-triggered reasoning; OS schedules; boot/recovery notices; local presence lighting and ambient movement | Rare initiative must preserve useful reminders, requested results and context-sensitive behavior |
| Body and lamp utility | User light settings, scenes, volume, music, expressions, gaze/tracking, active look/search, sleep and touch vocabulary | Lighting and body position are useful functions as well as expressions |
| Local authority | Privacy gates, stop gestures, named motion holds, stale LED-restore prevention, deterministic bounds, contact/overload handling | Cloud absence must not move these responsibilities out of the device |
| Cross-channel work | Web/MQTT chat, gateway messaging sessions, connectors, computer use, Buddy events and Harness tasks/results | Their context and delivery semantics; they are not all one conversation or one transport |
| Remote execution | A remote-Hermes adapter and, separately, delegation through Harness | Moving the main runtime and delegating a task are different operations; remote tools need explicit access to the physical device |
| Reliability and cost | Provider reconnect/recycle, alternate voice path, queue limits, ambient throttling, health checks, cached notices and process restart | Preserve useful recovery and budget controls; document what still works when one dependency fails |
| Observability | Flow events, interaction/run ownership, suppression reasons, voice and task metrics | Preserve traceability while measuring audible/physical outcomes and achieved user goals |

Additional source anchors: `hal/realtime/orchestrator.py:407`, `:1705`;
`runtimes/hermes/runs_transport.go:57`, `:120`, `:369`;
`runtimes/hermes/steering.go:30`, `:394`;
`system/agent/factory.go:57`; `runtimes/hermes/remote.go:8`;
`system/schedule/runner.go:108`, `:244`;
`hal/drivers/motors/hold.py:15`; `hal/privacy.py:24`;
`hal/app_state.py:981`; `system/healthwatch/service.go:20`, `:123`.

## 3. Corrections to the earlier discussion and research

1. **Hermes is more than a long-task worker.** It interprets selected sensing
   events as well as spoken/text requests. It supplies the agent execution
   machinery, skills, memory and tool use. Moving it off-device need not remove
   any of those responsibilities.
2. **An executive and a local nervous system already exist.** They are spread
   across HAL and os-server. Consolidation is an implementation hypothesis;
   the architectural labels themselves add no capability.
3. **Remote Hermes already exists in V1 code.** The existing remote setup is
   documented for Intern 2; this does not prove a remote Lamp deployment or
   that all Lamp-local tools work remotely. Many skills assume loopback APIs,
   local files and local credentials.
4. **V1 already shares context.** Gemini reads Hermes's SOUL, USER and MEMORY
   content, and agent speech/pending questions can inform voice turns.
   Gemini-handled exchanges enter a durable journal and are silently replayed
   to Hermes. The limitation is partial, asynchronous continuity, not absence
   of synchronization.
5. **One memory file would not solve continuity.** Durable personal facts,
   working conversation, active tasks, perception and event history are
   different kinds of state. A shared interface needs ownership, freshness,
   attribution, deletion and restart rules.
6. **Stopping speech is not cancelling work.** V1 deliberately retains old
   hardware/work when a newer conversation suppresses an old spoken reply.
   Physical cancel suppresses old speech and hardware markers and stops
   several local activities; it does not abort every underlying model task.
   Explicit task cancellation is a third operation. V2 must preserve these
   distinctions rather than cancelling all work on each new utterance.
7. **Sensors do not send every raw sample to Hermes.** Local reflexes act
   directly; perception emits selected semantic events; HAL, os-server and the
   Hermes adapter each filter, queue, expire or coalesce them. Selected ambient
   events have a default 120-second OS floor and a 60-second adapter expiry.
8. **A cloud-STT wake check is not a local-only privacy gate.** V1 sends audio
   to Deepgram before final wake authorization for Gemini. Any V2 promise that
   only addressed speech leaves the device needs a genuinely local gate or a
   clearly stated different privacy contract.
9. **Some old research overstates sharing or bounds.** The detailed mind note's
   8,000-character cap is not applied to Hermes MEMORY.md injection. That path
   reads the whole file; the realtime history budget is separate. The Gemini
   Hermes loader reads SOUL and USER, plus MEMORY separately, not AGENTS.
   `device-main` is shared by OS-submitted turns, not every gateway-owned
   messaging session. No Hermes heartbeat driver found in this repository is
   not proof that upstream Hermes lacks heartbeat/cron capabilities.
10. **Repository capability is not installed behavior.** Standard/Pro overlays,
    old/fresh configuration, cloud opt-ins and the selected voice mode matter.
    Addressed-gate/presence-warm-STT/name-anywhere changes on `lamp-release`
    must not be silently counted as part of pinned V1 main.

Sources for these corrections:
`hal/realtime/context_manager/hermes.py:31`, `:49`, `:70`;
`hal/realtime/context_manager/base.py:383`;
`hal/drivers/voice/_internal/realtime_turn.py:314`;
`system/externalhistory/store.go:69`, `:114`;
`system/externalhistory/sync.go:21`, `:53`;
`system/server/agent/delivery/http/handler_channel_turn.go:112`, `:155`;
`system/server/agent/delivery/http/handler_tts.go:102`, `:128`;
`system/server/agent/delivery/http/handler_tts_auto_cancel_test.go:24`, `:71`;
`hal/drivers/sensing/sensing_service.py:247`;
`runtimes/hermes/events.go:106`, `:138`;
`hal/drivers/voice/voice_service.py:2595`, `:2674`, `:2731`.

The branch distinction was checked with
`git log --oneline 170a1b85e..lamp-release -- hal/drivers/voice hal/realtime`;
relevant separate commits include `6ff85902a`, `56b29e8f1` and `471a48fb5`.

## 4. Concrete gaps that justify V2 design work

### A. Awareness and permission to speak are mixed together

Multiple gates can drop an observation before the agent can incorporate it.
This suppresses unnecessary model turns, but it can also remove useful context.
A candidate V2 improvement is to update a bounded situation state from fresh
observations independently of deciding whether to invoke a model or speak.
This does not mean sending all raw streams to a cloud service.

Evidence: `system/server/sensing/delivery/http/handler.go:383`;
`runtimes/hermes/events.go:138`.

### B. The two reasoning paths can receive conflicting identity

The sensing path chooses face identity before voice. Gemini's per-turn context
prefers a recognized speaker and uses face context as a fallback. The sensing
payload drops the identity source/age that the local resolver computed. These
can disagree without a shared rule for reconciling the evidence.

Candidate improvement: one attributed, freshness-aware account of people and
the current speaker, available to both reasoning paths. Presence, speaker,
addressee and authority must remain distinct facts.

Evidence: `hal/app_state.py:169`;
`hal/drivers/voice/_internal/realtime_turn.py:298`;
`hal/drivers/sensing/sensing_service.py:286`.

### C. Sensor failures can disable unrelated senses

SoundPerception uses a separate microphone but is scheduled from the camera
frame observer. Camera-off or missing frames therefore stop these sound checks.
Posture observations are also folded into motion events, so some enabled-sensor
combinations do not produce the expected autonomous event.

Candidate improvement: independent sensor lifecycles, bounded processing and
explicit stale/unavailable observations. Reuse the environment subsystem's
existing timestamp/source/freshness handling where suitable.

Evidence: `hal/drivers/sensing/perceptions/orchestrator.py:162`, `:279`;
`hal/drivers/sensing/perceptions/processors/motion.py:568`, `:702`;
`system/environment/detector.go:135`.

### D. Accepted commands can be mistaken for accomplished actions

The hardware-marker path counts any HTTP status below 400 as success.
`/servo/play` can return HTTP 200 with `status: ignored` when sleeping or held;
even acceptance returns before motion finishes. Marker results are not
uniformly returned to Hermes as tool results. Separate shell/curl tool calls
can expose their response, so this defect must not be generalized to every tool.

Candidate improvement: typed accepted/running/refused/completed/failed outcomes,
correlated to a request, with observed feedback available to the reasoning layer.
Apply action-specific cancellation and final local safety checks.

Evidence: `system/server/agent/delivery/http/handler_hw.go:146`, `:187`;
`hal/routes/servo.py:190`.

### E. Continuity across model sessions and process restart is incomplete

Hermes uses a fresh device conversation per os-server boot and rotates at
40 turns or 250,000 reported tokens. The adapter drops that in-session history
without creating a rotation summary. Durable memory and external-history files
survive, but the managed queue/ownership state is in memory. A journal reloaded
in `sending` becomes `uncertain` rather than being blindly replayed.

Candidate improvement: explicit durable task and context state, reconciliation
after reconnect, and truthful uncertainty. Preserve V1's refusal to blindly
repeat an ambiguous submission; do not promise exactly-once external effects.

Evidence: `runtimes/hermes/rotation.go:12`, `:16`, `:46`;
`runtimes/hermes/steering.go:30`;
`system/externalhistory/store.go:69`, `:114`.

### F. Voice behavior and service recovery need an outcome comparison

Standard V1 drains/discards microphone input while TTS/music plays; selected
Pro profiles take a different live path. V1 also provides transcript-based
fallback to the main agent and provider reconnect logic. The audited V2 branch
instead stops its serve loop after terminal provider output settles, and the
attended pilot unit has `Restart=no`. This is a capability gap in that snapshot,
not a reason to change a pilot's restart policy without reviewing its purpose.

Candidate improvement: natural interruption, correct admission, and a service
that remains useful through model failures. Native Gemini audio may avoid the
standard V1 synthesis round trip, but latency, voice consistency and cost need
measurement. Independent quota for fallback services is still unverified.

V1 evidence: `hal/drivers/voice/voice_service.py:1119`;
`hal/drivers/voice/_internal/turn_dispatch.py:177`;
`hal/realtime/orchestrator.py:944`.
V2 evidence at `1e179f994`:
`lampOS/crates/live/src/coordinator.rs:2829`;
`lampOS/deploy/4ace-attended/lamp-live-pilot@.service:17`.

## 5. What a defensible "10x better" comparison requires

Do not attach one multiplier to an architecture diagram. Pick user outcomes,
establish their V1 denominators, and compare V2 on the same device/setup/tasks.
Historical latency figures from different models, dates and configurations are
not a matched baseline. A code callback is not an acoustic or physical result.

| Outcome | Comparable evidence required |
| --- | --- |
| Useful agency | Multi-step tasks completed correctly, corrections incorporated, failures recovered, and unfinished work reported honestly |
| Natural conversation | End of the user's speech to first useful audible answer; first use, warm use, long sessions and slow services; do not substitute filler/admission time |
| Reliable attention | False replies per hour of calls/background conversation, missed addressed turns, lost first words and unnecessary interruptions |
| Coherent memory | Correct person attribution, recall across sessions/restarts, pending-question continuity and confirmed forgetting |
| Grounded body behavior | Actual light/motion state versus requested state, refused actions reported correctly, interruption/cancellation and preservation of user lighting |
| Resilient operation | Behavior during STT/Live/model/TTS refusal, remote-host outage, camera failure and reconnect; no duplicated external actions |
| Restrained initiative | Useful requested results delivered, nuisance interventions per day, and awareness retained while speech is suppressed |
| Efficiency | Cost per successfully served conversation/task and bounded device resource use under concurrent activity |

For a literal tenfold claim, choose a measurable rate such as unwanted replies
or lost tasks, with enough baseline events to estimate it. Preserve success
rates on other scenarios: becoming silent or dropping capabilities is not an
attention or reliability improvement. Numeric release targets remain open.

Minimum comparison scenarios: ordinary chat; an agent's clarification followed
by a short answer; a long task while chatting; stopping speech versus cancelling
the task; two people with conflicting face/voice evidence; return during a call;
camera disabled while sound remains available; light commands during a reading
scene; refused motion; Gemini quota while other services work; all cloud paths
unavailable; remote host disconnect/reconnect; conversation rotation/restart;
remember/recall/forget; and a delayed reminder or task result.

Existing metrics distinguish some of these boundaries, but an OS admission can
count as acknowledgment, the audio endpoint can be a stream write, and a task
execution event explicitly does not prove correctness. Reuse the instrumentation
and record actual user-visible outcomes separately.

Evidence: `hal/telemetry/voice_metrics.py:297`, `:377`;
`system/telemetry/voice_task.go:59`; `docs/benchmarks.md:3`.

## 6. Remaining unknowns before claiming complete product parity

- The installed V1 manifest, hardware profile, full effective settings and
  enabled services. The prior sanitized 4ace inventory establishes
  `wakeword=false`, Hermes selection/version and its Gemini model, not every
  feature's real-world behavior. The private backup must remain private.
- Which upstream Hermes features and native-Runs patches were active; which
  skills were downloaded from the CDN; what `Auto-AI` resolves to.
- Backend quota buckets, reset times, failure signals and whether fallback
  services remain available after a Live limit; current relay resumption.
- Actual sensor accuracy, false replies, physical safety behavior, acoustic
  latency and end-to-end task success on the target hardware.
- How remote execution obtains scoped device tools and synchronized context.
  Existing Lamp-local loopback paths cannot simply follow the engine to a Mac
  or server. Harness task delegation has an existing separate contract.
- Whether sensor and memory features should be preserved at launch or deferred.
  This is a product decision after the capability inventory, not an assumption
  made by omitting boxes from the new architecture.

The owner has confirmed that the agentic engine may run elsewhere. Its exact
host/runtime, memory placement, voice routing, launch senses and motion remain
design decisions. Earlier proposals for a single memory file or removing agency
are not accepted requirements.

## 7. ROS 2 and independent sensor subscribers

The owner chose to bring useful ROS patterns into lampOS: sensor channels that
multiple applications or reasoning services can subscribe to independently,
with the architecture organized around AI and the companion experience.
Specific contracts remain under discussion; a general subscription graph is
not an implemented V2 capability.

ROS 2 retains strongly typed topics with multiple publishers and subscribers.
Producers and consumers can be replaced or added without changing each other's
code. Applications choose their publication cadence; there is no single required
sensor frequency. Topics can carry periodic samples or event-driven updates.
QoS specifies queue history, reliability and freshness; compatible subscribers
can use different profiles. The sensor profile favors recent samples over
reliable delivery of every sample.

Primary references checked on 2026-10-12:

- [ROS 2 topics](https://github.com/ros2/ros2_documentation/blob/rolling/source/ROS-Framework/interfaces/About-Topics.rst)
- [ROS 2 QoS](https://github.com/ros2/ros2_documentation/blob/rolling/source/ROS-Framework/interfaces/topics/About-Quality-of-Service-Settings.rst)
- [ROS 2 actions](https://design.ros2.org/articles/actions.html)

### What the existing code supports

V1 at `170a1b85e` has genuine in-process observer fan-out. `DataObserver`
registers multiple callbacks, and the perception orchestrator connects
processors to frame and detected-face observations. Processors submit work to
a shared two-thread pool and skip new work while busy. These are explicitly
wired perception components, not a general subscription interface for separate
reasoning applications. Selected semantic sensing events then pass through
HTTP to os-server and its selected agent gateway. This does not mean every
V1 event path lacks publish/subscribe; diagnostic and messaging paths also
exist.

Evidence: `hal/drivers/sensing/perceptions/utils.py:38`;
`hal/drivers/sensing/perceptions/processors/base.py:16`;
`hal/drivers/sensing/perceptions/orchestrator.py:122`;
`hal/drivers/sensing/sensing_service.py:247`;
`system/server/sensing/delivery/http/handler.go:725`.

The available V2 Rust snapshot, `origin/lamp-v2-chat` at `10607a439`, separates
workers into processes and provides bounded, nonblocking Unix-datagram IPC.
Workers have explicit peers and control/data channels. The coordinator's
`Rig::publish` broadcasts authority to a fixed set of workers; it is not a
topic subscription facility. Direct bulk-media paths also exist, so the design
is not exclusively a hub through which all bytes pass. Process isolation and
message contracts are useful foundations, but arbitrary sensor subscriptions
for independent applications are not implemented by these components.

The camera transport already supports explicitly permitted consumer roles,
each with a bounded buffer and latest-frame pull requests. This is useful
multi-consumer infrastructure, even though it is not a general topic graph.

Evidence: `lampOS/crates/ipc/src/lib.rs:1`, `:125`, `:145`;
`lampOS/crates/live/src/coordinator.rs:274`;
`lampOS/crates/live/src/transport.rs:95`;
`lampOS/crates/camera/src/frames.rs:1`;
`lampOS/docs/architecture.md`.

### Chosen direction and contracts to define

Expose typed raw-sensor and derived-observation streams to authorized consumers.
Attention, local behavior, the agentic engine and optional diagnostic recording
could subscribe to different subsets. Give each consumer bounded queues and
explicit freshness requirements; a slow remote reasoner must not stall local
input handling. Raw audio/video need an appropriate transport, not mandatory
copying through a central broker. Publishing an observation must not imply
invoking an LLM or speaking to the user.

Keep actuator ownership coordinated locally. Consumers request actions; an
arbiter resolves conflicts and privacy/safety constraints, with one final writer
per actuator. ROS 2 actions provide a useful contract for goals, feedback,
results and cancellation, but do not themselves supply Lamp-specific arbitration
or prove that physical execution succeeded. lampOS implementation mechanisms,
remote access, topic schemas, rate budgets and launch consumers remain open.

The [ROS 2 design study](brain-research/ros2-design-study.md) extends this
comparison with production examples, timing and spatial contracts, action and
recovery semantics, companion requirements, and proposed contract validation.

## 8. V1 mapped onto the proposed companion architecture

This is a functional mapping of V1 main at the pinned revision, not a claim that
V1 already has the proposed common channel graph. Most capabilities already
exist. The proposed change is to make their observation, context, action and
recovery contracts consistent while preserving useful behavior.

Configuration matters: supported environment sensors, cloud perception,
selected voice modes and gateway capabilities are not enabled on every V1
device. Unmerged `lamp-release` features remain outside this map.

```text
PROPOSED ROLE                         V1 COMPONENTS TO MAP INTO IT

Sensors and privacy                   HAL drivers; local privacy gates
  Microphone                          Voice capture and separate sound sensing
  Camera                              Capture, face/presence inputs
  Touch/buttons                       Local gestures, wake/stop/sleep/privacy
  Environment                         Supported board/profile sensor workers
  Body feedback                       Servo readback, load/contact, temperature
             |
             v
Observation channels                  Partial equivalents:
                                      - in-process DataObserver callbacks
                                      - direct calls/shared HAL state
                                      - semantic sensing HTTP events
                                      - separate voice/event routes
             |
             +--> Perception          VAD and voice processing; Deepgram STT;
             |                        face/presence; activity, posture,
             |                        sound and emotion processing
             |                             |
             |                    derived observations
             |                             |
             +-----------------------------+
             |
             +--> Attention           Wake/follow-up focus, gaze checks,
             |                        speech/turn gates, Gemini decisions
             |                             |
             |                             v
             |    Conversation        Voice service + realtime orchestrator
             |                        + Gemini Live
             |
             +--> Current context     HAL app_state, identity/presence,
             |                        sensing digests, per-turn context
             |
             +--> Local behavior      Touch reactions, tracking, light states,
             |                        local stop/privacy/body safeguards
             |
             +--> Agentic engine      AgentGateway + Hermes adapter/runtime
                                      model, skills, memory, managed runs
                                             ^
                                             |
                  Additional inputs ---------+
                  Web/MQTT/messages, schedules, external/task events

Bounded decision assistance            Jev: intents, skill preloading,
                                      Harness selection, Buddy suggestions
                                      (separate from the engine/tool loop)

Conversation <--- delegation / context / task updates ---> Agentic engine

Physical action proposals
             |
             v
Action executive                      Distributed across os-server and HAL:
                                      local command/HW dispatch, TTS control,
                                      holds, LED priorities, device bounds
             |
             v
Actuator writers                      TTS/speaker, RGB, motor services
             |
             v
Execution feedback                    Local feedback and logs exist;
                                      a uniform result to the planner is a gap

Memory                                Hermes files; Gemini history/summary;
                                      external-history journal; person records
Supervision                           Provider recovery; healthwatch; systemd
Observability                         Flow Monitor/JSONL/SSE; voice/task metrics
```

The arrows place capabilities in the proposed architecture. Actual V1 routing
is the diagram in section 1: voice has its own gates and provider connections;
selected semantic observations travel through HAL and os-server to the chosen
agent gateway. Physical reflexes can act locally. Not all input passes through
one observer object, and not every sensor sample creates a Hermes turn.

### Capability mapping and changes to the connections

| Proposed component | Existing V1 behavior | What the channel architecture should change |
| --- | --- | --- |
| Sensor publishers | HAL captures audio, camera, physical controls and supported environmental values; motor/thermal feedback is mainly local. | Independent acquisition/lifecycles and typed observations. Preserve privacy and local protection. Remove unintended dependencies such as sound checks requiring camera frames. |
| Observation channels | Perception has local observer fan-out; voice, sensing HTTP, shared state and diagnostics use different paths. | A common subscription contract for authorized consumers, with explicit types, ordering, freshness and overflow. The diagnostic bus is not a substitute for sensor transport. |
| Perception | Local face recognition/presence and voice processing; Deepgram transcription; optional cloud activity/posture/emotion/fire perception; ambient sound/light processing. | Publish reusable derived observations with provenance and uncertainty. Keep each consumer's cadence separate from sensor cadence and model invocation. |
| Attention | Wake settings, follow-up focus, face-facing/gaze evidence, transcript guards, local speech gates and Gemini accept/reject/delegate judgments. | Make the inputs and admission policy explicit and testable. Preserve useful gates; do not invent a fully unified V1 attention service or count unmerged addressing work as shipped. |
| Current context | HAL state, face/voice identity, per-person evidence, environment snapshots and prompt digests. | Expose a fresh attributed view to conversation and agents. Reconcile conflicting face/voice evidence. Context should update independently of permission to speak. |
| Conversation | Voice service, Deepgram-assisted processing, realtime orchestrator, Gemini Live answer/reject/delegate, guarded look and search, agent fallback. | Preserve fast conversation and explicit task handoff; use the new observation/context boundaries. V1's Deepgram path is a real frontend capability, not merely an emergency fallback. |
| Agentic engine | Hermes with a configured model, skills/tools/memory, sensing-triggered reasoning and native managed runs when supported. Runtime adapters are swappable; remote Hermes is already supported in code. | Make it an independent consumer and action requester. Preserve agency and steering; remote placement requires scoped device tools and context, not just changing an endpoint. |
| Decision models | Jev already supports eligible intent selection, Hermes skill preload, Harness agent selection and advisory Buddy UI suggestions; other adapter support can be build-disabled. | Separate the replaceable decision-provider contract from the engine. Preserve caller-specific eligibility, native permissions, abstention and late-result rejection; source details are in the decision-model study. |
| Local behavior | Button/touch responses, gaze/tracking, LED states, ambient behavior and immediate body/privacy enforcement. | Keep immediate feedback independent of model latency. Give outward behavior coordinated ownership without routing every reflex through an LLM. |
| Action executive and expression | os-server command/HW handlers plus HAL TTS, app_state lighting arbitration, motor holds, scenes/animations, motion limits and contact protection. | Preserve these existing controls while defining common action IDs, resource ownership, expiry, cancellation scope and feedback to the requester. |
| Voice output | Standard configuration uses HAL synthesis/ElevenLabs for Gemini and Hermes text; Gemini TTS/native-audio paths also exist under configuration. | Preserve the output capability and cancellation behavior. The owner's V2 choice is Gemini's voice throughout; this is a deliberate provider/route change. |
| Task continuity | Native run identity, OS request ownership, steering/stop and careful handling of ambiguous submissions; some coordination state is in memory. | Keep task life separate from a spoken turn. Define durable ownership and reconciliation using the selected execution backend. Do not confuse muted speech with canceled work. |
| Memory | Hermes SOUL/USER/MEMORY inputs, separate Gemini history/summary, silent external-history replay and per-person/habit records. | Define attribution, freshness, correction/deletion and restart behavior across distinct kinds of state. V1 already shares context; neither one file nor one session is a complete replacement. |
| Supervision | Provider reconnect/fallback behavior, voice healthwatch and process restart units. | Explicit capability readiness and dependency-based recovery. Preserve working V1 recovery while closing the audited V2 quota-exit gap. |
| Observability and replay | Flow events, JSONL/SSE, voice/task metrics and subsystem logs. | Correlate observations, decisions and physical outcomes. Add reproducible multi-sensor replay; existing logs do not establish a complete replay system or actual acoustic/physical success. |

### Paths that the sensor diagram must also account for

- **Schedules and reminders:** V1 has an OS schedule store/runner. It can submit
  work to the agent or speak literal text. These are time-driven requests,
  distinct from sensor samples and from persistent personal memory.
- **Web, MQTT and messaging channels:** V1 has inputs beyond the microphone.
  Gateway-owned channel sessions are not automatically the device's current
  conversation. Source, person and output destination must survive routing.
- **Harness, Buddy and external history:** task delegation and external events
  cross separate boundaries. Relocating Hermes and delegating to another agent
  are different capabilities. Preserve their real contracts.
- **Enrollment, configuration and calibration:** identity banks, capabilities,
  privacy settings, physical bounds and model/provider configuration support
  the whole graph. They are configuration responsibilities, not an LLM's
  inference from an observation.

This is a preservation map, not a decision that every V1 feature must launch
in V2. Any keep/change/defer decision should identify the capability explicitly.

### Source anchors for the mapping

The detailed evidence is in sections 1-4. Principal paths at `170a1b85e`:

- Sensor/perception wiring: `hal/drivers/sensing/perceptions/orchestrator.py:122`;
  `hal/drivers/sensing/perceptions/utils.py:38`;
  `hal/drivers/sensing/sensing_service.py:247`.
- Voice and admission: `hal/drivers/voice/voice_service.py:2586`;
  `hal/drivers/voice/_internal/turn_dispatch.py:177`;
  `hal/drivers/tracking/gaze.py:839`;
  `hal/realtime/orchestrator.py:407`.
- Identity/context: `hal/app_state.py:169`;
  `hal/drivers/voice/_internal/realtime_turn.py:298`;
  `system/lib/sensingmsg/sensingmsg.go:59`;
  `system/environment/detector.go:135`.
- Agents and tasks: `system/agent/factory.go:57`;
  `runtimes/hermes/events.go:106`; `runtimes/hermes/steering.go:30`;
  `runtimes/hermes/runs_transport.go:57`.
- Local behavior and outputs: `hal/drivers/button_actions.py:271`;
  `hal/privacy.py:24`; `hal/drivers/motors/hold.py:15`;
  `hal/drivers/motors/animation_service.py:1146`;
  `hal/app_state.py:981`;
  `system/server/agent/delivery/http/handler_hw.go:146`;
  `system/server/agent/delivery/http/handler_tts.go:102`.
- Persistent context and other inputs: `hal/realtime/context_manager/hermes.py:31`;
  `system/externalhistory/store.go:69`; `system/externalhistory/sync.go:21`;
  `system/server/agent/delivery/http/handler_channel_turn.go:112`;
  `system/schedule/runner.go:108`.
- Recovery and measurement: `system/healthwatch/service.go:123`;
  `hal/telemetry/voice_metrics.py:297`; `system/telemetry/voice_task.go:59`.
