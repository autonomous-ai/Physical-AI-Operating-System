# lampOS V2: recommended architecture

**Latest discussion:** the [HTML architecture draft](v2-brain-design.html)
(13 October 2026, pending owner approval) updates the executive/decision-model
role and the topics-and-services interface vocabulary. Use that draft for the
team walkthrough; this earlier recommendation remains supporting research.

12 October 2026. Architecture recommendation after the V1, current V2, ROS,
decision-model and real-time-systems review. This describes the intended system,
not capabilities already implemented or qualified on a Lamp. No implementation
or deployment is part of this review.

**Keep the existing Rust V2 foundations and build the complete companion around
them in small vertical slices.** Preserve V1's useful behavior and agentic
capabilities, while replacing its inconsistent observation, action and recovery
boundaries. Keep V1 available until the replacement passes explicit parity and
device gates. The next engineering slice is provider failure recovery, not a
new generic robotics framework.

The [build plan](v2-build-plan.md) defines those slices and cutover gates. The
[review index](v2-review-index.md) pins source revisions and records verification.
The [migration inventory](brain-research/v1-migration-inventory.md) accounts for
the existing product, including setup, integrations and updates.

## 1. Architecture in one picture

This is a logical architecture. Boxes are responsibilities, not necessarily
individual processes. Channels are ROS-inspired lampOS interfaces, not ROS 2
topics running on the device.

```text
                           THE PHYSICAL WORLD
                                   |
        +--------------------------+--------------------------+
        | Mic    Camera    Touch/buttons    Environment       |
        |          Body position/load/temperature             |
        +--------------------------+--------------------------+
                                   |
                    Local privacy + device drivers
                                   |
                  Typed raw streams / device state
                                   |
              +--------------------+---------------------+
              |                                          |
              v                                          v
     Perception services                        Immediate local reflexes
     speech, presence, identity,                stop, overload, touch
     objects, sound, environment                 bounded; no cloud wait
              |                                          |
              v                                          |
   +------------------------------------------------------------------+
   |             ROS-LIKE OBSERVATION CHANNEL GRAPH                    |
   | typed pub/sub; source + time + uncertainty; bounded delivery       |
   | raw media has dedicated paths; subscriptions need permission       |
   +------------------------------------------------------------------+
        |                |                |                 |
        v                v                v                 v
   Attention       Current situation   Behaviors       Task context
   + addressee     + attributed facts  + initiative    + agent bridge
        |                |                |                 |
        |                +-------+--------+-----------------+
        |                        |                          |
        v                        v                          v
   Conversation       DecisionProvider interface     AgentEngine interface
   + voice routing    Jev / Jev-like / local / none  Hermes / Pi / Codex /
        |             bounded semantic judgments    Claude / other
        |                        |                   (remote by default)
        |                        v                          |
        |             judgments return to caller           |
        |                                                   |
        +--------------- action/task proposals -------------+
                                   |
                                   v
                 Behavior + task coordination / expression
                  attention, priorities, task continuity
                                   |
                                   v
                  LOCAL ACTION ADMISSION + CONTROL CORE <--- reflexes
                  authority, ownership, expiry, interlocks
                                   |
                 +-----------------+-----------------+
                 |                 |                 |
                 v                 v                 v
             Speaker           Light/ring       Motor controller
             one writer        one writer       one bus writer
                 |                 |                 |
                 +---------- actual outcomes --------+
                                   |
                     channels + original requester

   Across the system:
   - Personal memory and identity, with scoped read/write policies
   - Durable tasks, schedules and delivery records
   - Lifecycle supervision, provider health and resource budgets
   - Setup, configuration, updates, security and diagnostics/replay
```

Decision models and agent engines are separate plug-in interfaces. A behavior
may use rules, a decision model, an engine, or a combination. Neither Jev nor
Hermes is a mandatory stage between every sensor event and every action.
Conversation may delegate a task to the engine; the engine may request a bounded
decision. Both use selected context and receive actual action outcomes.

Many services can observe and reason concurrently. There is still one coherent
Lamp interacting with the room. Receiving an observation gives a component no
authority to speak, move, change a light, store a personal memory or invoke a
paid model.

## 2. Decisions and reasons

| Decision | Reason and practical consequence |
| --- | --- |
| Rust application OS on the supported ARM Linux image | Preserve working ALSA, camera, USB/serial and deployment foundations. Rust is not a hard-real-time guarantee. No new kernel or mandatory MCU rewrite. |
| Small deterministic local control core | Privacy, revocation, leases and output ownership must work when models, storage or networking stall. Core logic stays independently testable. |
| Separate lifecycle supervisor | Opening devices, launching, killing and reaping workers must not run inside the interaction loop. No replacement hardware owner until the previous one has exited. |
| Typed observation graph with different delivery profiles | Several applications can use selected evidence independently, without putting all media through one broker or invoking a model for every sample. |
| Topics, services, actions and tasks remain distinct | A temperature observation, a configuration read, a cancellable movement and a research task have different lifetimes and failure semantics. |
| Explicit behavior layer | Companion responses need attention, coordination and restraint; a transport bus alone provides none of these. Start with state machines, not a general behavior-tree framework. |
| Optional decision providers beside interchangeable engines | Preserve V1's Jev experience without making a cloud classifier the scheduler or physical authority. Provider choice must not determine task and memory ownership. |
| Remote Hermes first; qualify a second engine | Lowest capability-migration risk from V1. This is a continuity choice, not a benchmark result. Keep a local-host adapter possible, but ship no on-Lamp agent runtime by default. |
| Gemini Live primary; Deepgram + answer model/engine + Gemini TTS fallback | Matches the owner's voice direction. Route health and shared quota dependencies remain explicit. Endpoint changes do not guarantee independent quota. |
| OS-owned task records and canonical personal memory | A spoken turn or engine session can end without losing the user's task, preferences or correction. Engine working transcripts remain engine-specific. |
| Incremental standalone replacement | Reuse proven Rust workers and port V1 behavior by contract. No permanent dependence on the V1 Go server/Python HAL and no big-bang retirement of V1. |

The product advantage must be demonstrated: fewer unwanted responses, sustained
availability, natural interruptions, useful agency, coherent expression and
recoverable work. A language change or a larger diagram is not evidence of a
tenfold improvement.

Alternatives considered:

| Alternative | Assessment |
| --- | --- |
| Keep evolving V1 exclusively | Viable for short-term fixes and the safest deployed reference. It does not advance the chosen standalone Rust architecture. Preserve V1 until replacement evidence warrants retirement; do not claim its language makes reliable behavior impossible. |
| Big-bang V2 rewrite | Concentrates hardware, product, protocol and data-migration risk. Reject in favor of independently useful slices with rollback. |
| Adopt ROS 2 as the runtime | Strong existing graph/tooling, but companion attention, memory, conversation and provider failure still need product design. The owner chose to bring its useful contracts into lampOS. No native ROS or bridge dependency now. |
| Route every observation/action through Jev or an engine | Adds cloud latency and availability dependencies to local control, and discards useful deterministic behavior. Keep models optional and accountable to explicit callers. |
| One process for everything, or one process for every node | The first weakens fault isolation; the second adds unnecessary IPC and memory. Group by timing, hardware ownership and failure domain. |
| One global reliable bus for all traffic | Bulk media and stalled consumers can delay control; reliable delivery still does not prove freshness or completion. Use separate bounded planes and explicit per-stream semantics. |

## 3. What already exists

V1 already has sensors feeding agentic reasoning, local reactions and body
guards, remote Hermes support, memory, skills, realtime delegation, schedules,
Jev integration and substantial operational infrastructure. Its Jev paths
include intent fallback, skill preloading, Harness selection and experimental
Buddy suggestions. These exist in the pinned V1 baseline, not just a later
branch. Preserve their eligibility checks and useful fallbacks; see the
[decision-model study](brain-research/decision-models-study.md).

Current Rust V2 at `e1eae8de5ad65c1ec6c6450948afbcb510982240` supplies:

- Typed interaction ownership, privacy generations, bounded leases and fresh
  observation metadata in `lamp-interaction`.
- Separate control/media IPC, native audio/DSP, speaker accounting and provider
  workers; bounded camera, ring and motor interfaces.
- Environmental snapshot semantics, replay/evaluation tooling and exact worker
  lifecycle evidence.
- New combined-pose clearance and measured-stop logic. Preserve this work. Its
  clearance coordinates are calibrated dimensionless values, not degrees;
  host/emulation tests have not qualified physical motion.

It does not yet supply the complete graph, general action/task interfaces,
agent tools, canonical memory, all V1 sensors/behaviors, provisioning or product
recovery described here. In particular, terminal provider completion still
stops the current service, and the current Gemini wire parser rejects tool-call
messages. Both are real implementation work, not configuration switches.

Source pins, exact gaps and verification limits are in the
[review index](v2-review-index.md) and
[real-time study](brain-research/realtime-systems-study.md).

## 4. Three execution planes

```text
                    APPLICATION PLANE (ordinary scheduling)
     conversation / context / behaviors / decisions / tasks / memory
          network and disk I/O; bounded async jobs; replaceable services
                         | proposals / results
                         v
                    CONTROL PLANE (local, bounded)
     input/turn ownership / privacy / action grants / output arbitration
          independent urgent queues; no model, network or DB waits
                         | grants / revoke / receipts
                         v
                    MEDIA AND DEVICE PLANE
     capture -> DSP -> admitted voice route      camera -> allowed consumer
     accepted speaker render -> echo reference  motor bus -> body feedback
          dedicated workers; explicit buffers; local expiry enforcement

     Lifecycle supervisor configures/starts/stops all three planes.
     Observation graph exports selected state; it is not their master clock.
```

The control core owns facts that determine immediate permission. The application
layer owns semantic policy and longer work. Hardware writers independently
enforce the last valid grant and its expiry, so a stalled core does not leave
unbounded authority. The supervisor can restart services but cannot fabricate
readiness, sensor freshness or proof of physical rest.

Lifecycle work itself must progress per worker: nonblocking exit checks,
bounded pending recoveries and initialization inside the worker or a bounded
helper. A camera stuck in teardown cannot hold up speaker or motor supervision.
Across supervisor crashes, the outer service manager verifies teardown of the
previous complete worker generation/cgroup before permitting new device writers.
The new supervisor cannot assume ownership of the old parent's child handles.
Unresolved process/device ownership quarantines that device; lease expiry,
timeout or killing the parent is not proof its descendants released hardware.

Proposed initial process grouping:

| Process role | Contents | Failure boundary |
| --- | --- | --- |
| Supervisor | Configuration validation, launch registry, capabilities, restart/backoff and device ownership | Root service manager restarts it conservatively; workers expire authority on lost owner. |
| Interaction/control | Pure ownership state machine plus bounded IPC adapters, immediate input/output arbitration | Fresh boot identity on restart; old outputs cannot be rebound. No blocking child cleanup here. |
| Device/media workers | Existing audio, capture/DSP, speaker, ring, camera, motor; new physical-control/environment workers | Device-specific fault isolates its capability. Shared physical buses keep one writer. |
| Observation/context service | Semantic topic fan-out, current snapshots, consumer registration and projections | Subscribers detect missing/stale state; critical local paths continue independently. |
| Companion service | Attention policy, conversation route manager, small behaviors and expression planning | No hardware ownership; restart drops obsolete proposed outputs. |
| Task/integration service | Durable task controller, schedules, engine adapters, Harness/Buddy/MQTT connectors | Reconcile external work after restart; never guess that a disconnected task stopped. |
| Storage service | Task journal, memory, identity references and deletion manifest | Local controls continue on failure; new durable work is rejected explicitly. |
| Inference/provider workers | Network clients, perception and decision adapters | Separate large native inference or untrusted provider parsing from control. |
| Management/diagnostics | Setup/API/UI, updates, bounded traces and authorized replay | Never block interaction on rendering, logging or upload. |

These are starting isolation boundaries, not one process per topic or behavior.
Lightweight trusted services can share an application process. CPU-heavy/native
inference and independent failure domains earn their own process. No dynamic
third-party library loads into the control core. A reviewed executable manifest
declares subscriptions, actions, credentials, resource limits and restart policy.
Same-user Unix sockets do not sandbox hostile installed code; only trusted
packages are supported initially. Stronger OS isolation is a separate gate for
untrusted extensions.

## 5. Channels, services and execution contracts

ROS topics, QoS, lifecycle, actions, controller resource claims and introspection
are the main patterns to borrow. In ROS, publisher rates are application choices;
deadline detection is not scheduling proof, and cancellation admission is not
action completion. See the [ROS study](brain-research/ros2-design-study.md) and
[ROS action design](https://design.ros2.org/articles/actions.html).

### Observation channels

Each envelope carries schema/version, source and boot identity, sequence,
acquisition time, receive time, validity/quality, privacy lineage and causal
references. Spatial data also names its frame, units and calibration version.
Remote observations name their clock domain and timing uncertainty. Never
compare two hosts' monotonic clocks or replace acquisition time with arrival.

The graph maintains per-consumer bounds. A slow habit observer cannot back up
capture or starve attention. A newer uncertain/absent measurement replaces an
older good value; missing data never silently means zero or still present.

| Stream family | Initial delivery policy | Typical consumers |
| --- | --- | --- |
| Audio and accepted echo reference | Existing ordered, bounded dedicated blocks; explicit gaps; no general bus relay | DSP, admitted voice route, explicitly allowed sound perception |
| Camera frames | Dedicated permitted consumer paths; latest useful frame; bounded in-flight ownership | Presence, vision tasks, provider when granted |
| Presence, pose, environment, capability state | Latest state with age/expiry; snapshot plus revisions on reconnect | Attention, behavior, context and agent bridge |
| Touch/button transitions | Ordered bounded events; overflow resets gesture recognition and reports loss | Local controls, touch behavior |
| Transcript/semantic events | Correlated events with turn, subject and provenance; bounded retention | Conversation, context, admitted task routing |
| Action/task outcomes | Stable IDs, acknowledgment/reconciliation; task outcomes durable | Original requester, task journal, diagnostics |
| Privacy/stop/grants | Dedicated priority control, locally enforced; observer copies are informational | Control core and final writers |
| Diagnostics | Best effort, bounded and explicitly lossy | Flow Monitor, metrics, optional replay |

No universal sensor frequency is introduced. Board profiles declare acquisition
rates and maximum age per measurement; each consumer declares its acceptable
age and cadence. Initial audio rates remain those of the existing qualified
component configuration, rather than a graph-wide timer. A high-rate publisher
does not force inference at that rate.

For initial semantic pub/sub, extend the existing bounded Unix datagram
transport with a small local registry and fan-out service. Control and bulk
endpoints remain separate. Metadata messages, including envelopes, fit its
existing 8192-byte limit. Larger context/media use scoped resource references
and bounded application-service transfers, never ad hoc datagram fragmentation.
Application request/reply and resource reads use a versioned JSON API over a
private Unix stream socket; cap message/body sizes, concurrent requests and
deadlines. This slower path is outside control and audio timing loops.

Choose explicit registration from a deployment manifest before network-wide
discovery. Keep transport behind contracts. Reconsider
[iceoryx2](https://github.com/eclipse-iceoryx/iceoryx2) when measured camera
fan-out copying warrants shared memory; its upstream throughput is not a Lamp
result. Consider [Zenoh](https://github.com/eclipse-zenoh/zenoh) for a future
distributed graph, not as a prerequisite for a paired agent connection. Neither
is selected to replace working control IPC in this plan.

### Services, actions and tasks

| Interface | Examples | Required meaning |
| --- | --- | --- |
| Service | Discover capabilities, read current context, inspect a task, change a setting | Bounded request/reply. Reads have versions; mutations have request IDs and explicit persistence semantics. |
| Action | Speak, set light, look toward a target, play media | Admission, owner, required resources, deadline, progress if useful, cancel request and actual terminal outcome. |
| Task | Research, find an object, run a workflow, remind at a future time | Durable objective across turns/actions; permissions, budgets, progress, recovery and user-visible result. |

An action request includes `action_id`, `request_id`, originating task/turn,
requested capability, typed parameters, evidence references, resource set,
deadline and cancellation scope. The executive validates it and issues a
short-lived grant. The final writer checks current boot/owner/privacy, expiry,
resource ownership and device-specific limits again.

Admission and execution are different records. Outcomes distinguish rejection,
started, completed, superseded, expired, cancelled-with-evidence, failed and
unknown. A successful HTTP request, queued packet or model assertion cannot
become `completed`. Each capability defines its proof: speaker playback,
observed motor state or an external provider receipt. Do not promise exactly-once
external effects; persist intent, use idempotency where available and reconcile
ambiguous outcomes before a retry.

On revocation, no ordinary action can acquire new authority. Motor stop may
need the existing narrow, provisioned measured-hold authority to finish safely;
it is distinct from continued interaction motion. `Inhibited` is not physical
rest, and servo-reported rest is not force or optical proof.

## 6. Perception, situation and attention

Perception transforms permitted raw streams into reusable observations. Retain
V1's useful VAD, presence/face, sound, posture/activity and environmental work
where the capability exists on the selected board. Separate availability from
deployment defaults. Sound sensing must not depend on receiving camera frames.
Heavy inference runs at an explicit rate with coalescing, not inside a capture
callback. Cloud perception is optional and subject to disclosure policy.

The situation service maintains current attributed evidence: who may be here,
what was heard, what the body is doing, task state and recent interaction.
Presence, identity, speaker, addressee and authorization are separate fields.
Contradictory face and voice identity stays unresolved; a remembered name cannot
override current uncertainty. Temporal joins have a bounded skew and produce
partial/unknown results when a source disappears. Spatial joins require the
right calibrated frame; stale target coordinates cannot authorize a movement.

Attention has two boundaries:

1. **Local admission before upload.** Physical controls, an explicit wake cue
   and a qualified conversational-focus policy decide whether room audio may
   enter a cloud conversation route. Preserve bounded retained opening audio
   and the existing candidate-before-interruption design. Tentative speech
   does not itself cancel the current answer.
2. **Semantic acceptance after admission.** Transcript/context and, optionally,
   a decision model or Gemini `not_for_me` check can reject an admitted turn.
   This catches mistakes but cannot undo audio already uploaded.

Initial launch recommendation: explicit activation is always available; natural
follow-ups are admitted only inside a bounded, qualified focus session. Keep
ambient ambiguous speech silent. Broader hands-free addressee detection must
pass visitor, phone-call, television and multi-person cases before being a
default. An exact focus timeout is a versioned Lamp policy to tune with those
tests, not an architectural constant inferred from a demo.

Context can update while Lamp stays quiet. A model call, permanent memory write
and outward response each need their own trigger and budget. Attention must
not become a global lock that prevents useful passive observation.

## 7. Conversation, tasks and fallback voice

```text
        Local privacy + attention admit an utterance
                            |
                     Voice route manager
                     /                  \
              Live ready              Live unavailable
                 |                           |
          Gemini Live audio            Deepgram STT
                 |                           |
           answer / tools        answer model or AgentEngine
                 |                           |
                 |                       response text
                 |                           |
                 |                       Gemini TTS
                 +-------------+-------------+
                               |
                 same owned speaker + echo reference
                               |
                    playback/cancellation receipts

      Task requests from either route -> durable task controller
      Room-eligible results -> attention/expression admission -> voice route
```

Keep acquisition, capture readiness, conversation-route readiness and audible
playback separate. The listening cue means the configured path can accept and
retain input now. A connected socket or a restarted process is insufficient.
Changing routes preserves the local speaker, actual render reference, turn
ownership and privacy checks; cloud workers never own the audio device.

The route manager classifies transient network/session errors, rate limits,
exhausted quota, bad credentials and unsupported protocols separately. Use
bounded jittered backoff, a total retry budget and provider health state. Do not
restart an exhausted provider in a tight loop. Gemini TTS may share credential,
billing or quota dependencies with Live; failover is useful only when the full
fallback chain is actually available. If neither route is available, local
controls, useful lighting, authorized sensors and existing task records remain
alive, and the status truthfully reports voice unavailability.

On a mid-answer failure, retire the old route-attempt/output generation before
admitting a replacement. Every PCM chunk, transcript and new tool/action proposal
retains that generation; the final admission boundary rejects retired work even
if it shares the current turn ID. Drain only already accepted speaker obligations
under still-valid authority, with a bounded local deadline. A dead provider may
never send its terminal marker: classify unresolved output honestly and stop it
locally rather than wait forever. Record truncated output.

Keep receipts for tasks/actions already admitted under their own IDs; retiring
a voice route must not erase their outcome. Do not replay an utterance or repeat
tool calls whose effects are uncertain. Reuse retained input only while its
original privacy/admission and retention bounds still permit it. Session
reconnect/resumption is a provider capability, not an assumption of the OS.

Implement Gemini tool request, response and cancellation IDs as an adapter to
the same action/task system. It can answer a normal question directly, request
an eligible Lamp action, or delegate longer work. Agent progress and internal
reasoning are not speech. Each task durably retains initiating principal, source
conversation/channel, reply destination, disclosure scope and delivery policy.
Only room-eligible results pass to attention/expression; a private messaging
result does not become room speech merely because Lamp is idle. Keep the
destination stable through retries, scheduled execution and engine changes.
The full result remains available on its authorized surface.

One conversational turn owns speech at a time. Multiple bounded digital tasks
can continue in the background. New speech supersedes prior speech/associated
expression, not an unrelated research job. Separate commands stop speaking,
cancel a physical action, interrupt an engine turn or cancel a whole task.

## 8. Behaviors and decision providers

Use the following terms consistently:

| Term | Responsibility | Example |
| --- | --- | --- |
| Perception | Produce evidence | Person track, speech candidate, ambient light |
| Reflex | Immediate bounded local behavior | Touch stops speech; overload inhibits motion |
| Behavior | Coordinate observations/context/goals into action proposals | Briefly orient to an addressed speaker, then settle |
| Decision model | Answer a bounded semantic question | Which eligible skill or response applies, including none |
| Agent engine | Run an open-ended model/tool loop | Research, inspect a scene, operate an authorized integration |

Behaviors declare the topics they need, preconditions, resources, priority,
cooldown, timeout and cancellation behavior. Small deterministic state machines
are sufficient initially. They may ask `DecisionProvider` to resolve ambiguity
or create an engine task; asynchronous operations keep their IDs across ticks.
No repeated model submission from a polling loop.

V1 already uses Jev. The redesigned interface preserves useful code-first
candidate filtering, none/abstention, confidence/fit checks, current-capability
revalidation and fallback. It centralizes provider/version/data budgets and
observability without forcing every caller through Jev. Its upstream public
API takes text/structured state; it is not raw audio/video perception or an
agent loop. A returned distribution is not a calibrated correctness guarantee.
See the [API and V1 study](brain-research/decision-models-study.md).

Pin a decision specification, model version, candidate set and relevant context
revision. Bound total admission-to-result time, input size, concurrency, retries
and spend. Discard a late result after changed attention, privacy, eligibility
or task ownership. Provider failure/uncertainty means the caller's explicit
fallback, usually silence, a local rule or the original engine route.

First integration: advisory skill/context hints alongside existing preparation,
without delaying the first engine turn. Qualify social/addressee decisions with
labeled Lamp recordings before making them authoritative even within the
semantic layer. Physical safety and exact numerical control never depend on a
probability threshold from a model.

Initiative stays rare. A presence event updates awareness; it does not imply a
greeting. Touch response is immediate and small. A voice-orientation behavior
that finds nobody makes a limited glance and gives up. Exhaustive scanning is
reserved for an explicit task. Light, gaze and speech share one expression plan
with actual playback/action feedback, rather than three independent reactions.

## 9. Body authority and expression

Separate persistent useful state from transient expression. A reading-light
setting survives a spoken-turn change; a listening overlay or conversational
gesture does not. Keep one physical writer per light device/bus and one motor
bus writer. A lamp with separate illumination and decorative LEDs still needs
an explicit resource map rather than an assumed shared brightness control.

Initial precedence is physical safety/privacy, direct user control, current
conversation/task action, then optional autonomous expression. Priorities are
not sufficient on their own: resource conflicts, permitted interruption and
resumption semantics are part of each capability. A multi-resource expression
either acquires its declared resources together or uses a declared partial
fallback. It never holds one indefinitely while waiting for another.

Use semantic body capabilities, such as `look_toward(target_ref)`, named qualified
gestures, `set_light` and `speak`. Calibrated transforms and local controllers
produce device goals. Keep existing normalized servo coordinates explicitly
typed; do not relabel them as physical angles. Agent tools cannot issue raw
serial/EEPROM writes or bypass local qualification by invoking a shell.

Cancellation first inhibits ordinary output, then records what actually stopped.
Physical privacy immediately closes relevant capture/upload grants and purges
queued derived work according to lineage. A camera toggle does not silently
become an unrelated whole-device mute. Narrow local protection may continue
without camera/microphone evidence. An unknown motor condition disables new
motion; the system must not describe a software timeout as guaranteed rest.

The board's physical privacy switch can deliberately bind several resources:
the audited V1 Lamp profile disables microphone and camera and mutes the speaker
together. Preserve that declared behavior. It is distinct from a camera-only
setting and from independently tracked microphone/camera privacy lineages.

## 10. Agent engines, task continuity and memory

The OS contract covers capability discovery, selected context updates without
starting a model turn, task submission, correlated event streams, tool requests,
questions/answers, permission requests, steering, interruption, health and
reconciliation. Engines expose unsupported operations explicitly. The
[engine study](brain-research/agent-engines.md) details current adapters and
upstream limits.

An `awaiting_input` task records question/request ID, owning task/engine,
question version, expected answer schema, authorized respondent/channel, focus
revision and expiry. Deliver an answer idempotently to that question and confirm
acceptance before resuming. Clarification and permission approval are different
request types; answering a factual question cannot grant an action. If several
questions could own “yes,” resolve the ambiguity rather than guessing. New
conversation must not lose an unfinished task's pending question.

Host Hermes remotely first to preserve V1 continuity and reserve Lamp resources
for interaction. Existing remote tools that assume the Lamp's filesystem need
explicit replacements. Device capabilities go through the local action gateway;
digital tools run on their authorized host. Reuse established Harness pairing
and command contracts for Harness integration, and preserve Buddy's independent
identity/transport. They are integration services, not alternate unguarded body
routes. Do not invent a second Harness protocol.

Changing engines applies to new tasks first. Existing tasks finish on their
owner or undergo an explicit, reconciled handoff. An OS task record can be
portable while an engine checkpoint, transcript compaction or in-flight tool
operation is not. The initial second adapter must pass the same behavioral
contract; Pi or the existing Codex protocol are candidates, not selected winners.

Five kinds of state have different owners:

| State | Owner and lifetime |
| --- | --- |
| Current situation | Bounded in-memory observations, per-field freshness and provenance; rebuild after restart. |
| Conversation continuity | OS-owned bounded journal/summary with conversation/turn/source IDs, accepted content, actual delivery status and projection cursors; private retention policy. |
| Canonical personal memory | OS-owned attributed facts/preferences/corrections, purpose and retention metadata; private durable store. |
| Task/action history | Durable task intent, external-effect and delivery receipts; transient physical authority never survives reboot. |
| Engine working state | Engine-specific sessions, summaries and tool history; link by task/session ID, do not make it the only canonical user memory. |

The conversation journal preserves referents such as “the second option” across
Live/fallback or engine changes; durable facts alone cannot do that. Project
handled exchanges as attributed history, not new instructions or tool execution.
Share one versioned Lamp persona, language/voice preference and applicable user
context across routes. Retain V1's external-history semantics while keeping
private channel scopes and per-consumer projection cursors explicit.

Recommend a local SQLite store behind one asynchronous writer for task records,
memory metadata and delivery outboxes. Store sensitive credentials/biometric
assets in separately scoped private storage, referenced by opaque IDs. Use
transactions and durable acknowledgment before reporting a task saved; keep
checkpoint/fsync work outside timing paths. WAL still has one writer and relies
on storage guarantees, so it is neither a hard-real-time queue nor proof against
faulty media. [SQLite WAL](https://www.sqlite.org/wal.html),
[atomic commit](https://www.sqlite.org/atomiccommit.html)

Personal memory entries retain source, subject, confidence/status, consent scope,
creation/revision and deletion policy. A user's explicit correction supersedes
an old fact and invalidates cached projections. Proposed inference does not
silently become an established identity or permission. Preserve useful V1
conversation continuity and habit evidence, but do not treat all room sensing
as permanent memory. Raw media recording stays off by default and opt-in
diagnostic captures are bounded. Enrollment, export and forgetting must address
derived indexes, paired hosts and engine caches as well as the primary store.

Adapters route canonical personal-memory reads/writes through OS tools with
revision and commit receipts. Disable, redirect or explicitly reconcile competing
native personal-memory writes: an unmodified remote Hermes `MEMORY.md` write
cannot be the only effect behind “I remembered that.” Engine-private working
notes remain possible, but cannot override an OS correction or forgotten fact.
Qualify remember/correct/forget across both voice routes and an engine switch.

Retain V1's backend-confirmed schedule CRUD contract initially: the local store
mirrors confirmed definitions, and offline edits remain visibly pending until
confirmed. Name one execution owner for each schedule revision; preserve stable
occurrence IDs across reconnect so local and remote executors cannot both fire.
Any future local-only scheduler is a separate explicit contract change, not an
accidental consequence of choosing SQLite.

Schedules have timezone/DST policy and separate due, admitted, executed and
delivered states. A notification accepted into a speech
queue is not proof the person heard it. A cancelled or partly played result
remains visible without being blindly spoken again after every reconnect.

## 11. Supervision, security and operations

Services follow configured -> starting -> ready -> degraded/unavailable ->
stopping -> stopped, with explicit fault and bounded retry state. Readiness is
capability-specific. Privacy permission is separate from health. A provider
failure can remove conversation while leaving reading light, physical controls
and task inspection available. Lifecycle state alone does not prove worker exit.

Preserve V1 provisioning, Wi-Fi recovery, configuration, credentials, device
identity, UI, OTA and diagnostic capabilities through Rust replacements. Updates
are versioned bundles with compatible worker/schema sets, integrity/authenticity
checks and a recoverable last-known-good installation. Configuration is
validated and atomically persisted before reporting a saved version; changes
declare whether they apply immediately, at a turn boundary or after restart.
Provider credentials never appear in model context or diagnostic payloads.

Every persisted state class belongs in a manifest: configuration, secrets,
pairings, identity, memory, tasks, schedules, engine caches, calibration,
diagnostics and update metadata. Reset/export/rollback enumerate that manifest.
Keep unit calibration separate from user identity so a reset cannot silently
replace a mechanical qualification with a generic default. Keep the private
4ace backup outside repository history and this research workflow.

Authentication identifies a remote requester; it does not grant every sensor,
memory or actuator. Per-component capabilities constrain both data disclosure
and actions. Revalidate at the final boundary. Model/provider text, documents
and observation content remain data, not authority to change policy. Installed
agent sandboxes must not expose a second direct path to Lamp device nodes.

PREEMPT_RT, RTOS priority/resource disciplines and watchdogs can help meet local
timing requirements, but only an end-to-end measurement can qualify this board,
driver stack and mechanism. Start with bounded work and isolated workers on the
supported Linux image. Add RT kernel scheduling changes if measured tail latency
requires them; use an MCU only if an actual physical deadline cannot be met.
See the [real-time systems study](brain-research/realtime-systems-study.md).

## 12. Tooling and acceptance

Provide one diagnostic view of the graph: publishers/subscribers, rates, ages,
sequence gaps, dropped/coalesced work, capability readiness, active owners,
decision acceptance/abstention and action/task outcomes. Preserve Flow Monitor's
usefulness while keeping diagnostic transport independent of sensor channels.
Every user-visible action must be explainable from a trigger through its grant
to its result. Use IDs rather than raw personal content in routine telemetry.

Replay authorized observations, context revisions, policy/model versions and
faults through the same attention/behavior/control code with simulated outputs.
Model adapters can be stubbed or replayed for deterministic control checks;
live model quality and physical performance need separate trials.

Retain the current proposed device targets, with exact boundaries:

| Target | Qualification boundary |
| --- | --- |
| Local control acceptance p99 <=5 ms | Event receipt to control acceptance under concurrent load; not physical stop. |
| Ring feedback p95 <=50 ms | Valid input/playback transition to visible change; no false listening cue. |
| Interruption p95 <=200 ms | Admitted interruption to acoustic silence; separately measure speech-onset admission, lost opening words and false interruption. |
| Useful answer p50 <=2 s, p95 <=4 s | End of user speech to first substantive audible word; filler and cues do not count. Report Live and fallback separately. |

These are targets from the V2 design, not outcomes established by this review.
Motion has device-specific clearance and stop qualification; no universal
mechanical bound is inferred from its software deadlines. All-day availability,
false addressing, memory correctness, task recovery and natural behavior are
additional gates in the [build plan](v2-build-plan.md).

## 13. Decisions intentionally left to qualification

The architecture does not depend on guessing an exact camera frame rate, engine
benchmark winner, focus timeout, model-confidence threshold, mechanical stop
time or choice of RT kernel. Each has a named build-plan experiment and a safe
failure/default path. Changing them must not rewrite the core contracts.

Recommended complete Lamp scope includes microphone, physical controls, camera
presence/qualified identity, body feedback, useful lighting, restrained safe
motion, conversation, agent tasks and memory. Installed environmental sensors
are profile-driven. Optional cloud affect/posture models and broader autonomous
initiative can follow after core parity; any product deferral is recorded, not
silently described as V1 parity. A stationary voice pilot is useful, but it is
not the completed robotic OS.

No on-Lamp agent engine is required for the default deployment. An optional
future local adapter must meet the same resource, permission and lifecycle
contract; it cannot change who owns the body.
