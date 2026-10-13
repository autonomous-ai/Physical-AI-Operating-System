# Agent engines for lampOS

12 October 2026. Design research only. The owner wants an AI-native robotic OS
with a replaceable agentic engine; Hermes, Codex, Claude Code and Pi are
candidates. The engine may run on another computer. No engine selection,
installation, runtime implementation or device deployment follows from this
note.

The [ROS study](ros2-design-study.md) records the observation/action architecture,
the agreed Deepgram-to-Gemini-TTS fallback direction and proposed behavior
terminology. The [V1 audit](../v1-brain-audit.md) remains the source for the
corrected V1 capability baseline.

The [final architecture](../v2-architecture.md) now supplies the recommended
canonical memory, conversation journal, question/answer and delivery ownership
decisions; the [build plan](../v2-build-plan.md) supplies qualification gates.
The [decision-model study](decision-models-study.md) covers the separate Jev-like
plug-in boundary and corrects an earlier omission: V1 already uses Jev for
several bounded decisions, independently of its agentic engine.

## Recommendation

Use Hermes as the first V2 backend to preserve V1's agentic behavior, then
qualify a second adapter to prove the boundary is independent of Hermes. Pi is
a strong candidate when direct control of the agent loop is the priority;
Codex is another useful candidate because this repository already integrates
its App Server. This is a migration recommendation, not a measured ranking of
reasoning quality, latency or reliability.

The engine manages a model/tool loop and its working session. The model is a
separate choice, within the providers each engine supports. Neither a runtime's
name nor its coding-oriented defaults establish whether it can reason about
robot observations. All four expose useful integration mechanisms.

Recommend remote hosting initially, keeping Lamp's local controls independent
of the engine process and network. Exact hosting is still a product decision.

## Current upstream capabilities

Checked current primary documentation/source on 12 October 2026. These are
upstream capabilities, not proof that the pinned V1 adapter exposes them or
that any particular release has passed Lamp qualification. Pin versions before
implementation.

| Engine | Relevant capabilities | Fit and integration work |
| --- | --- | --- |
| Hermes | Programmatic embedding, gateway protocols, HTTP/SSE runs with status, steering and stop; assistant memory and skills. | First choice for V1 continuity. Translate its session conventions and qualify the existing V1 patches against upstream. |
| Pi | TypeScript SDK and subprocess RPC; replaceable tools, context/resources, models and persistent branching sessions. | Strong option for a deliberately customized brain. More companion policy and memory integration belongs to our application. |
| Codex | App Server with persistent threads, streamed turns, steering, interruption, MCP and host tools. | Credible general task engine. Use its structured protocol and verify features against the pinned binary. |
| Claude Agent SDK | Claude Code's loop exposed through Python/TypeScript, custom/MCP tools, streaming input/output, interrupt and resumable sessions. | Credible general task engine with Claude model integration. Our current bridge needs more of the upstream control surface. |

Sources: [Hermes integration](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/developer-guide/programmatic-integration.md),
[Hermes memory](https://hermes-agent.nousresearch.com/docs/user-guide/features/memory),
[Pi SDK](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/sdk.md),
[Pi RPC](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc.md),
[Codex App Server](https://developers.openai.com/codex/app-server),
[Claude agent loop](https://code.claude.com/docs/en/agent-sdk/agent-loop),
[Claude streaming input](https://code.claude.com/docs/en/agent-sdk/streaming-vs-single-mode),
[Claude custom tools](https://code.claude.com/docs/en/agent-sdk/custom-tools).

Important distinctions:

- Hermes steering is cooperative. Its run API can acknowledge queued guidance
  before the agent consumes it, and reports undelivered guidance as
  `pending_steer`. The upstream gateway does not promise durable prompt
  admission across owner restart. Its curated `USER.md`/`MEMORY.md` and
  conversation search are useful starting points, not a complete multi-person
  identity model.
- Pi currently lives at `earendil-works/pi`; older `badlogic/pi-mono` links
  redirect there. Current SDK steering waits for the assistant turn's tool
  calls to finish; `agent_settled` is the final boundary after automatic
  continuation. Its separate `pi-durable` package supports persisted work and
  restart recovery but explicitly labels its API experimental. External tool
  effects still require idempotency.
  [Pi durable](https://github.com/earendil-works/pi/blob/main/packages/durable/README.md)
- Codex supports `thread/inject_items` to add model-visible history without
  starting a turn. `turn/steer` targets an expected active turn; interruption
  ends through a terminal event. Dynamic host tools and TCP WebSocket transport
  are currently experimental; stdio is a documented transport. These details
  require version checks, not assumptions that every installed Codex has them.
- Claude sessions persist conversation and tool history; they do not restore
  external state. The SDK exposes more than our current bridge. Its deployment
  also needs explicit process, filesystem and settings isolation.
  [Claude sessions](https://code.claude.com/docs/en/agent-sdk/sessions),
  [Claude hosting](https://code.claude.com/docs/en/agent-sdk/hosting)

No engine's interrupt acknowledgment proves a physical action stopped. That
claim requires a receipt from lampOS's action executor.

## Boundary owned by lampOS

This is a proposed contract, not a finalized API. It must expose useful engine
differences without turning the whole interface into a text prompt.

| Operation | Contract |
| --- | --- |
| Discover capabilities | Versioned support for context updates, tools, steering, interruption, media, session recovery and health. Unsupported operations fail explicitly. |
| Update context | Supply or reference selected observations without starting a model turn. Retain source, age, confidence and context version. An adapter can buffer context when no native operation exists. |
| Start task | Stable OS task/request IDs, objective, context version, tool grants, deadline and budget. Distinguish accepted, rejected and uncertain submission from completion. |
| Stream events | Correlated progress, tool requests/results, explicit speakable output and terminal task outcomes. Intermediate agent text is not automatically speech for the room. |
| Ask/answer | Correlated pending questions and separately typed permission requests; task/question/focus versions, authorized respondent and idempotent answer delivery. Preserve unfinished questions across conversation turns. |
| Steer task | Target the current owner/version; distinguish queued guidance from consumed guidance and handle guidance left pending when a turn ends. |
| Cancel | Separate stopping speech, cancelling an action, interrupting a model turn and cancelling a task. Inhibit local physical authority without waiting for cloud reasoning. |
| Resume/reconcile | Inspect known work after reconnect; restore available session state; settle unknown outcomes before retrying side effects. A broken stream does not mean the task stopped. |

The OS owns the observation/context service, public task identities and records,
physical action ownership, capability checks and execution outcomes. Recommend
an engine-independent boundary for canonical personal memory and corrections;
the final architecture recommends a private local durable store and revisioned
memory tools. Adapters redirect or reconcile native personal-memory writes;
engine working notes cannot silently become the only remembered copy. A separate
bounded OS conversation journal/persona projection preserves cross-route
referents and silent external history. Engine transcripts, compaction and private
working state may remain engine-specific. Do not duplicate every piece of
internal session state into the OS.

Expose typed robot capabilities such as `look_at`, `set_light` and `speak`.
Engines request actions through this boundary; the final writer stays local.
MCP can expose these tools, but it does not define observation transport,
attention, resource arbitration or task durability for the OS.

Permitted sensor streams can update context continuously while meaningful
changes, user requests or task needs trigger reasoning. A subscription does not
authorize a model call on every frame, a permanent memory write or an outward
reaction. Reflexes and bounded local behaviors work without an engine.

Initially, changing the selected engine applies to new tasks. Existing work
stays with its owner until completion or an explicit handoff. Portable task
records do not imply that arbitrary live engine checkpoints can be migrated
during a tool call.

## Lessons from the existing adapters

Source audit at current repository HEAD
`b8c23cc9916de3f074c34b8689134437d2ebb4b6`, separately from the older V1 baseline
in the companion audit. Paths below are repository-relative at that revision.

| Existing evidence | Lesson for the Rust contract |
| --- | --- |
| `system/domain/agent.go:35`, `:94`, `:152`, `:223` | `AgentGateway` combines agent execution, device setup, channels, skills and voice plumbing. Reuse protocol knowledge, not this entire interface. |
| `runtimes/hermes/runs_transport.go:57` | Native run support is discovered, including a V1-specific enriched-events capability. Do not infer it from the engine name. |
| `runtimes/hermes/run_expirer.go:42`; `runs_transport.go:298` | Preserve owner-scoped cancellation and recovery after uncertain event-stream loss. |
| `runtimes/codex/gatewayd/gatewayd.go:24`; `appserver.go:257` | Preserve bounded admission and correlated steering acknowledgments. Writing a request is not proof of task completion. |
| `runtimes/claudecode/gatewayd/server.go:124`; `runtimes/claudecode/translator.go:124` | The current bridge lacks typed steer/cancel and drops partial stream events. These are adapter gaps, not proof of upstream inability. |
| `runtimes/codex/gatewayd/appserver.go:189`; `runtimes/claudecode/gatewayd/child.go:131` | Existing permission-bypass settings must not replace the new OS action authority boundary. |

## How to choose with evidence

Use the same allowed tools, context and representative Lamp scenarios. Where
model choice differs, report it separately from engine overhead. Measure first
use and repeated use, time to useful speech/action, task success, unwanted
responses, cost and recovery behavior. Do not inherit V1's measured slow turns
as proof that all future Hermes deployments must have the same latency.

Required scenarios include passive observations producing no model turns;
follow-up speech during a task; sensor evidence becoming stale; steering at turn
completion; local stop while a remote tool runs; reconnect after uncertain
submission; engine restart; and Gemini Live quota exhaustion. Switching engines
must retain OS-owned task/action history and authorized personal context.

This research used source inspection and current official documentation. No
engine was installed, no comparative runtime benchmark was run and no device
was accessed.
