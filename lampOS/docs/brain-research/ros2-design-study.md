# ROS 2 lessons for the Lamp brain

12 October 2026. Owner direction: build an AI-native robotic OS, with Lamp as
the first companion product. Implement useful ROS patterns within lampOS.
Detailed contracts remain under discussion; no runtime implementation is
authorized by this study.

The completed synthesis is the [V2 architecture](../v2-architecture.md) and
[build plan](../v2-build-plan.md). This study retains its earlier V2 comparison
pin; the [review index](../v2-review-index.md) and
[real-time study](realtime-systems-study.md) inspect the newer
`e1eae8de5ad65c1ec6c6450948afbcb510982240` snapshot. Its 26 changed paths concern
motor clearance/stopping and associated contracts/tests; unchanged graph/audio
comparisons remain applicable. No observation below claims full device parity.

The recommended direction is a typed observation graph with several independent
consumers, coupled to explicit action contracts and local actuator ownership.
Gemini conversation, agentic reasoning, attention and local behavior can consume
different evidence at different speeds. The agentic engine remains a first-class
consumer of the sensed world, including when it runs on another computer.

ROS is the engineering reference for sensor distribution, execution contracts,
timing, recovery and observability. lampOS owns their Rust implementation and
the companion-facing architecture: attention, conversation, agentic work,
context, memory and coordinated expression. Extend the current Rust controls
and output safeguards. Suitable libraries can still be reused; owning this
architecture does not require rebuilding every dependency.

The owner also confirmed sensor channels, explicit actuator/action APIs,
Deepgram transcription with Gemini speech synthesis for the fallback voice
route, and a swappable agentic engine. The recommended first engine is Hermes;
that selection and its host are still proposals. See the
[engine comparison and adapter design](agent-engines.md).

## Evidence and existing design

This study uses current primary ROS documentation, project source and the
authors' 2022 ROS 2 architecture paper. Rolling/main sources describe development
branches. The behavior borrowed from them must become explicit, tested lampOS
contracts rather than assumptions based on a middleware name.

Lamp source comparisons are pinned:

- V1: `170a1b85e7e6197097019e4175efdcbe416e3c2c`.
- V2 Rust: `10607a43963ca1a1e7c1b0bfda024a9ecb0b99f9`.
- Earlier brain/reuse documents: `lamp-a2-brain` at `1e179f994`.

The earlier `docs/reuse-decisions.md` already discusses ROS lifecycle, actions,
QoS and scheduling. This study extends that work to the brain's observation
graph, execution model and companion requirements. The
[V1 audit](../v1-brain-audit.md) contains the corrected behavioral baseline.

V1 has observer fan-out inside HAL, selected sensor events reaching an agentic
engine, local behavior and body arbitration. V2 has separate workers, bounded
IPC, observation provenance and final output authority. Its camera transport
supports explicitly permitted consumer roles with independent buffers.
Neither snapshot provides a general ROS-style subscription graph for arbitrary
reasoning applications. Extend these existing strengths.

## What real ROS applications teach us

The 2022 paper reports three relevant architectures:

- Ghost Robotics used actions for missions and lifecycle-managed components
  activated as needed.
- Mission Robotics kept its core on DDS and exposed ROS 2 for extensible sensor
  and customer integrations.
- Auterion combined PX4 with ROS 2 logging, visualization and simulation.

These cases support reuse at deliberate subsystem boundaries. Debugging and
integration tools can be as valuable as message transport. They are reported
deployment experience, not a controlled comparison with Lamp.
[Macenski et al., sections IV-A through IV-C](https://arxiv.org/html/2211.07752v1)

For Lamp, carry over explicit subsystem boundaries, replaceable consumers and
reproducible diagnostics. These are useful engineering practices within a
lampOS implementation.

## Practices to apply

### Independent observations and selective reasoning

ROS topics provide typed publish/subscribe interfaces with multiple producers
and consumers. Applications choose their publication cadence; there is no
universal sensor frequency. Consumers can select streams without the sensor
knowing which application uses them.
[Topics](https://github.com/ros2/ros2_documentation/blob/rolling/source/ROS-Framework/interfaces/About-Topics.rst)

Separate three Lamp decisions: acquire permitted evidence, update current
context, and invoke reasoning or produce behavior. A presence change can update
context while Lamp remains silent. A posture observer can consume selected
observations without changing the camera driver. Attention can combine speech
and presence while an agent handles a longer task.

V1's observation-to-Hermes gates sometimes discard evidence to suppress a
response. Preserve its restraint, but move response policy downstream of the
permitted state update. This does not require retaining every raw sample or
calling an LLM on every event. Privacy still governs acquisition and access.

### Delivery policy belongs to each stream

ROS QoS makes history, reliability, durability, deadlines, lifespan and
liveliness explicit. Compatible subscribers can request different profiles;
incompatible profiles can prevent communication. The sensor profile favors
recent samples. Reliable transport alone establishes neither fresh evidence
nor durable task history.
[QoS](https://github.com/ros2/ros2_documentation/blob/rolling/source/ROS-Framework/interfaces/topics/About-Quality-of-Service-Settings.rst)

Proposed Lamp contracts:

| Stream | Required behavior |
| --- | --- |
| Camera and pose | Bounded pending work; replace obsolete samples; retain original acquisition metadata and permitted consumers. |
| Microphone and echo reference | Ordered bounded blocks; explicit gaps and recovery, rather than arbitrary frame dropping. |
| Presence and environment state | Latest valid value with source, age and unavailable/uncertain states; recover a snapshot after reconnect. |
| Touch transitions | Ordered bounded events with age and identity; overflow cannot invent a gesture or replay it later. |
| Actions and results | Correlated IDs, admission/result and duplicate policy; ambiguous delivery cannot automatically repeat side effects. |
| Privacy, stop and authority | Independent priority handling and local enforcement, unaffected by bulk-media congestion. |
| Diagnostics and optional recording | Bounded resources and explicit lost-evidence reporting. |

A consumer's acceptable age and processing cadence are separate from the
producer's rate. A remote agent normally needs meaningful transitions and a
current snapshot, with richer data available through permitted requests.

### Preserve time, provenance and spatial meaning

ROS images specify acquisition time and optical coordinates. Timestamp
synchronizers support exact or approximate joins; requiring a match can hold
back output when a sensor is absent. The distributed clock model assumes
synchronization, while tf2 represents coordinate relationships over time.
[Image contract](https://github.com/ros2/common_interfaces/blob/rolling/sensor_msgs/msg/Image.msg),
[synchronization](https://github.com/ros2/message_filters/blob/rolling/doc/index.rst),
[clocks](https://design.ros2.org/articles/clock_and_time.html),
[tf2](https://github.com/ros2/ros2_documentation/blob/rolling/source/ROS-Framework/interfaces/About-Tf2/About-Tf2.rst)

For Lamp, join speech intervals with nearby gaze/person evidence using bounded
skew and wait policies. Missing camera evidence must not stall audio. Associate
moving-camera observations with measured pose at capture time; define optical
and body coordinates and calibration. A capture ID is not a coordinate frame.

Reuse V2's worker boot identity, sequence, acquisition time and explicit unknown
confidence. Preserve original sources when deriving evidence. A Mac's monotonic
clock cannot be compared directly with the Lamp's. Remote time synchronization
and uncertainty need their own contract. Shared context should expose conflicting
identity evidence instead of silently choosing whichever sensor replied last.
ROS does not supply that person/attention policy.

### Separate the communication graph from scheduling

ROS nodes are logical components, not necessarily individual processes.
Executor choice and callback groups affect scheduling. Current documentation
describes priority inversion and non-FIFO processing under backlog; its
EventsCBGExecutor also has an unbounded ready-event queue.
[Executors](https://github.com/ros2/ros2_documentation/blob/rolling/source/ROS-Framework/client-libraries/About-Executors/About-Executors.rst)

Keep acquisition, local stop and output timing independent of cloud inference,
disk writes and slow subscribers. Bound queues and in-flight work across every
adapter. Preserve useful failure boundaries without requiring a process for
every function. V1's shared perception pool and camera-triggered sound checks
show why this separation matters.

ROS composition and message loaning can reduce copying, but client/middleware
support and ownership semantics vary. Current loaning documentation identifies
subscription-loan limitations. Measure copying costs before replacing V2's
bounded camera delivery with shared memory.
[Composition](https://github.com/ros2/ros2_documentation/blob/rolling/source/ROS-Framework/nodes/Working-with-nodes/intra-process/Intra-Process-Communication.rst),
[loaning](https://github.com/ros2/ros2_documentation/blob/rolling/source/ROS-Framework/client-libraries/Working-with-Client-Libraries/Configure-ZeroCopy-loaned-messages.rst)

### Give work an action lifecycle

ROS actions distinguish acceptance, execution, feedback, cancellation in progress
and terminal results. The server decides how concurrent goals interact. Its
result cache is not a durable task ledger.
[Action design](https://design.ros2.org/articles/actions.html)

Lamp needs separate task, action and conversation-turn identities. New speech
may supersede an old answer while a research task continues; explicit task
cancellation has a different scope. Preserve this distinction already present
in V1, and return execution outcomes to the requesting planner.

Cancellation acknowledgment establishes canceling, not physical stop. Nav2's
action-node halt code handles cancellation/result timeouts before resetting
tree status. Keep richer Lamp evidence: requested, authority revoked, driver
inhibited, physical outcome observed or unknown.
[CancelGoal](https://github.com/ros2/rcl_interfaces/blob/rolling/action_msgs/srv/CancelGoal.srv),
[Nav2 halt](https://github.com/ros-navigation/navigation2/blob/main/nav2_behavior_tree/include/nav2_behavior_tree/bt_action_node.hpp)

Long-lived tasks should use the selected agent/Harness execution contract.
Do not add a duplicate task manager merely to imitate ROS actions. The local
executive owns Lamp's physical outputs; unrelated remote tools retain their own
execution and permission boundaries.

### Coordinate resources at the final writer

ros2_control separates shared state access from exclusive command-interface
claims. Controller switching has explicit policies and failure handling; it
does not promise mechanical safety or atomic rollback of every failed switch.
[Resource manager](https://control.ros.org/rolling/doc/api/classhardware__interface_1_1ResourceManager.html),
[switch contract](https://github.com/ros-controls/ros2_control/blob/master/controller_manager_msgs/srv/SwitchController.srv)

Several Lamp services may observe head position, but their action requests must
converge on coordinated ownership. Define which requests replace, preempt, wait
or conflict. Preserve V1's independent hold owners and V2's expiring permits,
generations and one bus writer. Topic publication cannot grant unchecked
actuator access.

Speech, gaze and light can express one interaction while their observations
come from different services. Output coordination does not require all
perception or reasoning to execute inside one core loop.

### Recover capabilities according to their dependencies

Managed lifecycle separates configuration and activation from process existence.
An external supervisor orchestrates transitions. Nav2's lifecycle manager relies
on launch respawn or another watchdog to recreate a dead process; it normally
tears down its managed group when a member fails.
[Lifecycle](https://design.ros2.org/articles/node_lifecycle.html),
[Nav2 lifecycle manager](https://docs.nav2.org/rolling/configuration_and_development/configuration_guide/core_servers/configuring_lifecycle_manager/)

Use explicit dependency groups for Lamp. Optional camera failure removes visual
capability; motor failure removes motion; healthy controls and voice remain
available where their dependencies permit. Preserve verified old-worker exit,
fresh identity and fresh authority before replacement.

The quota gap belongs here. V2 treats quota unavailability as terminal for the
provider recovery path, and the audited serve/pilot setup exits. The proposed
outer policy keeps local functions alive, reports affected requests truthfully
and tracks provider availability. Quota needs an appropriate reset/cooldown or
an independently available route; repeatedly restarting an exhausted service
does not restore capacity. ROS does not know the Gemini quota contract or
choose Lamp's fallback voice policy.

The owner has now selected this fallback direction:

```text
Default:   admitted audio -> Gemini Live -> native voice / action proposals
Fallback:  admitted audio -> Deepgram STT -> answer model / agent
                                        -> response text -> Gemini TTS
```

Deepgram supplies streaming transcription; a separate model or agent must
produce the answer. Gemini TTS converts that answer to speech and is a separate
API from Live. Current Google documentation supports streaming TTS.
[Deepgram streaming STT](https://developers.deepgram.com/docs/stt-streaming-feature-overview),
[Gemini speech generation](https://ai.google.dev/gemini-api/docs/speech-generation)

Keep the same attention policy, permitted context, task records and output
ownership across both routes. Do not replay a partly executed task merely
because the voice route changed. Verify that the chosen TTS and answer-model
routes remain available under the actual Live quota failure: Google limits are
project-scoped and model-dependent, so a separate endpoint does not by itself
establish independent capacity. Qualify voice continuity by listening as well
as matching the configured voice name.
[Gemini rate limits](https://ai.google.dev/gemini-api/docs/rate-limits)

Fallback unavailability must leave the local OS and healthy controls running.
Retain the proposed bounded recovery/backoff and truthful unavailable state;
neither route is guaranteed when connectivity or its dependencies fail.

### Use authored recovery where it fits

Nav2 behavior trees combine bounded retries, conditions, goal updates and
context-specific recovery. These patterns suit known procedures such as camera
acquisition or executing a body action.
[Nav2 recovery walkthrough](https://docs.nav2.org/rolling/getting_started/nav2_behavior_trees/detailed_behavior_tree_walkthrough/detailed_behavior_tree_walkthrough/)

Gemini and the agentic engine still interpret intent and plan open-ended work.
Start with explicit state machines for small procedures; add a tree engine when
composition warrants it. Any asynchronous model/tool leaf must retain its work
ID across ticks, avoid duplicate submissions and handle late results. Existing
provider retries must not gain an accidental second retry loop.

### Make difficult interactions reproducible

Rosbag2 supports recording/playback, bounded snapshot capture and lost-message
statistics. ROS tracing exposes execution timing; message traffic alone does
not explain every callback delay or missing decision.
[Rosbag2](https://github.com/ros2/rosbag2/blob/rolling/README.md),
[tracing](https://github.com/ros2/ros2_tracing/blob/rolling/doc/design_ros_2.md)

Record authorized observations alongside configuration/model versions, decision
inputs, grants, drops and action outcomes. Replay calls, visitors, quiet speech,
delayed observations and quota failures through deterministic attention and
executive code with simulated outputs. Preserve privacy and bounded retention;
recording need not be an always-on raw-media archive. Replays cannot make cloud
reasoning deterministic or establish physical/acoustic performance.

## Proposed brain architecture

This logical interface graph does not require a central broker or put every
raw stream on a network.

```text
Lamp sensors
     |
     v
Typed raw streams --> perception / fusion --> derived observation topics
     |                                            |
     |                         +------------------+-----------------+
     |                         |                  |                 |
     |                         v                  v                 v
     |                   Attention gate     Local behaviors    Agentic engine
     +-- permitted audio ----->|                  |             on Mac/server
                               v                  |                 |
                         Conversation             |                 |
                       Live / STT fallback        |                 |
                               |                  |                 |
                               +------ physical action requests ----+
                                                  |
                                                  v
                                       Local action executive
                                      ownership / limits / policy
                                                  |
                                                  v
                                       Speaker / light / motors
                                                  |
                          execution feedback -----+--> requesters

Current-context views and optional replay subscribe to selected observations.
Physical privacy and stop retain independent local priority enforcement.

Task request / progress / result:
Gemini and other task sources <--> task adapter <--> agentic engine
```

The remote boundary exports permitted topics/requests with explicit types,
time semantics, authentication and resource limits, and returns task progress
and results. Both reasoning paths can read consistent fresh context without
requiring one shared model session or deciding long-term memory storage now.
Raw audio/video access remains an explicit separate contract.
The audio path shown requires physical privacy permission and attention
admission before it reaches a cloud voice route.

Attention admits conversation and helps determine when unsolicited output is
appropriate. It should not globally prevent other consumers from updating
permitted context. Subscription alone likewise authorizes neither speech nor
movement. A shared context view is current evidence, not durable personal memory.

## lampOS implementation direction

The owner chose to bring the useful ROS patterns into lampOS. Native ROS
adoption and a ROS bridge are not the next design step. The runtime should
remain organized around the companion's needs, with a small stable Rust core
and independently replaceable perception, reasoning and behavior components.

Separate reusable robot contracts from Lamp-specific product choices. The OS
defines observations, capabilities, task/action lifecycles, permissions and
recovery. Lamp supplies its hardware profile, attention policy, companion
behaviors and coordinated expression. A different robot should not inherit
Lamp's servo layout or social behavior merely to use the same agent adapter.

Implement the necessary subscription, observation, action and lifecycle
contracts on top of the useful V2 foundations. Select maintained libraries for
individual mechanisms when they fit; do not build a general robotics framework
before the Lamp needs one. An agentic runtime may run elsewhere and interact
through the same explicit observation and action boundaries.

### Proposed terminology

Use **behavior** for a component that uses observations, current context and
goals to decide whether and how to act. Behaviors can be deterministic or
agent-assisted, and may maintain state over time. Silence and no action are
valid outcomes. This name does not require a behavior-tree implementation.

| Term | Meaning | Lamp example |
| --- | --- | --- |
| Perception | Produces observations from permitted input. | Speech detected; person present; head pose. |
| Behavior | Coordinates a response over time and requests actions. | Follow an addressed speaker, then settle when the interaction ends. |
| Reflex | An immediate, bounded local behavior with no cloud dependency. | Touch stops speech; overload inhibits motion. |
| Action | Executes a declared capability with admission and an outcome; longer work also has progress and cancellation. | Speak an utterance; look toward a target. |
| Task | A longer-lived objective that can span actions and conversations. | Find an object, or research a question and report back. |

The conceptual loop is observations + context + goals -> behavior -> action
request -> execution outcome -> new observations. Reflex priority and physical
limits remain local. Agent turn cancellation is not evidence that an actuator
stopped. Behaviors request resource ownership rather than writing hardware
directly.

ROS distinguishes observation topics, short request/reply services and actions
with feedback and cancellation. Keep those distinctions within lampOS rather
than calling every interaction a channel. Capability discovery and settings fit
services; movement or speech fit actions. Sensor rates, QoS and local control
loops remain explicit parts of the contract.
[ROS interface guide](https://github.com/ros2/ros2_documentation/blob/rolling/source/ROS-Framework/Interfaces-Topics-Services-Actions.rst),
[ROS action design](https://design.ros2.org/articles/actions.html)

### What makes these contracts useful to an AI companion

| Responsibility | lampOS design requirement |
| --- | --- |
| Awareness | Build a current context from observations with source, age, uncertainty and contradictions. Keep this distinct from durable personal memory. |
| Attention | Decide whether speech addresses Lamp and whether a response is appropriate, while other permitted observers can keep updating context. |
| Reasoning | Let Gemini, the agentic engine and specialized consumers use relevant subsets of evidence at their own pace. Bound model invocations and supply context without sending every sample to a model. |
| Ongoing work | Keep tasks alive across conversational turns; support clarification, progress, explicit cancellation and truthful completion. |
| Memory and continuity | Give reasoning access to relevant personal context, corrections and task history. The store, ownership and placement remain separate decisions. |
| Expression | Coordinate speech, gaze and light around one interaction, with rare initiative and valid silence/stillness. Multiple observers do not imply competing outward personalities. |
| User experience | Express setup, capabilities, privacy and failures in terms a person understands. Ordinary use should involve conversation and physical controls, without requiring topic or queue configuration. |

For example, a person returning to the desk can update presence and current
context, help attention interpret their next utterance, and inform an ongoing
task. It need not start a model turn, trigger a greeting or write a permanent
memory. Each of those is a separate decision with its own owner.

Subscriptions grant access to particular observations; action authority remains
separate. The remote agent connector cannot bypass local output ownership,
privacy or physical limits. Execution outcomes return to the requester so the
AI can distinguish what it intended from what the Lamp actually did.

## Proposed contract validation

This is future validation, not implementation authorization or a completed
benchmark. Begin with replayed/synthetic inputs and simulated outputs. Exercise
the new lampOS contracts against the current Rust behavior, retaining current
audio and final-writer paths as the baseline. Keep payloads, rates and policies
comparable, excluding cloud-model variance when measuring local runtime costs.

Demonstrate a second independent observer added without changing the sensor
driver, a remote agent stub requesting an action, and a correlated result.
Exercise:

1. Concurrent camera/audio traffic with a stalled observer and recorder.
2. Worker restart, delayed old-worker observations and clock discontinuity.
3. Remote reconnect with current-context recovery, without obsolete commands or
   duplicate work when the earlier outcome is uncertain.
4. New speech during a long task, separately followed by explicit task
   cancellation and physical stop.
5. Competing body requests, missing release evidence and late completion.
6. Optional sensor failure, provider quota and router/agent failure while
   healthy local controls remain available.
7. Unauthorized subscriptions/commands and privacy closure during delivery.

Measure observation age at use, queue occupancy/drops, control scheduling tails,
cancel-to-inhibition evidence, recovery time, cold readiness, RSS/CPU and install
size. Compare debugging effort using a reproduced failure. Run a bounded
long-duration fault soak after the basic cases pass.

Retain existing V2 gates: local control event to controller acceptance p99 at
most 5 ms; admitted interruption to acoustic silence p95 at most 200 ms. These
are targets, not results from this study. Simulated acknowledgment cannot satisfy
the acoustic gate. No obsolete action may regain authority after cancellation
or restart. Hardware qualification remains a separately authorized step.

## Product decisions still open

- Evidence that speech addresses Lamp.
- Launch senses, motion and initial independent consumers.
- First agentic runtime/host, exact task contract and durable memory ownership;
  engine swappability is agreed.
- Topic schemas, access policy, rates and freshness budgets.
- Concrete subscription/transport mechanisms, reusable libraries and resource
  budgets within lampOS.
- Fallback answer model, verified quota availability, switching/recovery policy
  and voice continuity; Deepgram STT and Gemini TTS are agreed directions.

ROS helps separate and verify these responsibilities. It does not settle social
attention, identity truth, memory semantics or natural physical behavior.

## Local source anchors

Paths are relative to the parent repository at the pinned revision. Inspect with
`git show <revision>:<path>`.

| Evidence | Revision and path |
| --- | --- |
| V1 observer fan-out and shared pool | V1 `hal/drivers/sensing/perceptions/utils.py:38`; `hal/drivers/sensing/perceptions/processors/base.py:16` |
| V1 camera-driven sound and gated sensing | V1 `hal/drivers/sensing/perceptions/orchestrator.py:122`; `hal/drivers/sensing/sensing_service.py:247`; `system/server/sensing/delivery/http/handler.go:351` |
| V1 speech supersession versus physical cancel | V1 `system/server/agent/delivery/http/handler_tts.go:102` |
| V1 independent motor holds | V1 `hal/drivers/motors/hold.py:15` |
| V2 provenance and freshness | V2 `lampOS/crates/interaction/src/observation.rs:39`, `:109` |
| V2 IPC and explicit workers | V2 `lampOS/crates/ipc/src/lib.rs:125`; `lampOS/crates/live/src/coordinator.rs:274` |
| V2 permitted camera consumers and bounded buffers | V2 `lampOS/crates/camera/src/frames.rs:1` |
| V2 output receipt distinctions | V2 `lampOS/crates/motor/src/runtime/wire.rs:189` |
| V2 inhibition and cleanup evidence | V2 `lampOS/crates/motor/src/runtime/supervisor.rs:441`, `:478` |
| V2 terminal provider recovery and service exit | V2 `lampOS/crates/gemini/src/recovery.rs:1223`; `lampOS/crates/live/src/coordinator.rs:2829`; `lampOS/deploy/4ace-attended/lamp-live-pilot@.service:17` |
| V2 targets and component failure boundaries | V2 `lampOS/docs/architecture.md`, failure containment and qualification sections |

Validation consists of source cross-checks and document checks. No middleware
was installed, no runtime code changed, and no device measurements were made.
