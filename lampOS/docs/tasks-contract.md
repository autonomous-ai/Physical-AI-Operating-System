# Tasks and the Harness contract

The first client uses the provisioned Harness CLI and its existing account,
machine links and policy checks. Tasks keeps durable local evidence; Harness
owns launch, transport, credentials and remote agent behavior. This review
supports [task 11](work-split.md#11-one-task-out-to-an-agent-and-back-through-harness)
and the [chosen route](brain-build.md#step-5-in-more-detail-tasks-through-harness-on-the-lamp).
It does not turn an observed answer into a completed task.

## Source and version

Reviewed on 2026-10-14 against OpenHarness commit
`748269145cb76d1a164141f6bc55758e013334b0`, then refreshed to
`0e6b16d78fd80697dea252376f19d7acf49d240f` (08:33:52 UTC that day).
The [five-commit comparison](https://github.com/autonomous-ai/openharness/compare/748269145cb76d1a164141f6bc55758e013334b0...0e6b16d78fd80697dea252376f19d7acf49d240f)
does not change the reviewed CLI envelopes, caller policy, creation receipts
or completion correlation. The downloaded creation command and agent-client
command/model files were byte-identical across these pins. Links below use
the refreshed revision. This is source evidence, not deployed-version proof.

Brain-build records Harness 0.3.94 installed and signed in on lamp-4ace on
2026-10-13, without a task sent then. That historical report does not establish
that the installed release has this contract. Harness injects its release
version; its CLI package's `0.0.1` is not a compatibility check.

## Caller and permission boundary

A real Harness pane takes precedence over outside-agent identification.
Outside callers are resolved from Claude session variables, then a Codex
thread, then generic `AI_AGENT` and `HARNESS_AGENT_SESSION`. A generic
`AI_AGENT=lampos` and stable session produce `agent:lampos:<session>`.
Harness trims/truncates the generic name and session; the wrapper must validate
its own ASCII values, keep a nonempty session of at most 80 characters across
restarts, and remove inherited pane/Claude/Codex identity markers. These are
collaboration identifiers, not credentials. Omitting them can make the CLI
treat its caller as the person. [Caller model](https://github.com/autonomous-ai/openharness/blob/0e6b16d78fd80697dea252376f19d7acf49d240f/cli/src/agentClient/model.ts#L94).

Account or local Harness machine changes require explicit reconciliation of
retained tasks. Tasks must not read private Harness authentication or ledger
files, switch identities to gain authority, or manufacture a pane. Existing
CLI policy checks account permission, caller lineage, ownership and rate
limits. A directly callable daemon RPC does not reproduce those checks.

Request explicit `--mode ask`. Claude selects its default/manual permission
mode. Codex and OpenCode add no ask-specific flags: their existing engine
configuration remains authoritative. Ask therefore does not prove a newly
restricted sandbox. Cursor has an ask mode but lacks the first-prompt task
contract. An unsupported mode must fail without falling back to auto, full,
or terminal. Normal ask AgentFrames can contain `bypassPermission:null`;
requiring literal false would reject a valid source-produced frame.
[Launch modes](https://github.com/autonomous-ai/openharness/blob/0e6b16d78fd80697dea252376f19d7acf49d240f/cli/src/lib/engineLaunch.ts#L58),
[AgentFrame projection](https://github.com/autonomous-ai/openharness/blob/0e6b16d78fd80697dea252376f19d7acf49d240f/cli/src/lib/agentFrame.ts#L270).

Remote first prompts have a 2,000 UTF-16-unit limit including Harness's caller
header. For `lampos`, the inspected header costs 92 units plus the local
machine display-name length. That name has no source-enforced length bound,
so a small task limit alone cannot guarantee admission. Overlong remote
prompts are refused; local launches can use a task file. Never silently
truncate the confirmed task. Task text in CLI arguments can be visible to
the same local user and must not contain credentials.

## Creation variants and identity

`new --json` produces these source-backed variants, not a versioned JSON
Schema. Require bounded values, the expected variant and matching identities;
successful process exit alone is insufficient. The CLI can emit an empty
agent object when its daemon response is malformed.

| Variant | Required distinction |
|---|---|
| `{ok:true,machineId,agent,note?}` | Created agent; retain `agent.id` and the outer full machine ID. No creation ID is promised. |
| `{ok:true,pending:true,machineId,creationId,name,note}` | Installation/launch may continue after CLI exit; no agent ID yet. |
| `{ok:true,existing:true,name,machineId,state,agentId?,note}` | `starting` may lack agentId; `running` supplies it. Same-name reuse is not a payload-bound receipt. |
| `{ok:false,error}` | Caught failure, normally exit 1 or usage exit 2. Prose does not prove whether dispatch occurred. |
| No JSON, stderr only | Early parse/usage or daemon failure remains possible with `--json`. |

All three success variants exit zero. A new agent ID, engine conversation
`sessionId`, creation ID, caller session and lamp task ID are separate domains.
[Creation implementation](https://github.com/autonomous-ai/openharness/blob/0e6b16d78fd80697dea252376f19d7acf49d240f/cli/src/lib/newCommand.ts#L405).

Account machine IDs are 32 lowercase hexadecimal characters; computer IDs
are different. Persist an explicit full account machine ID, project, allowed
engine and exact confirmed task. [Owner machine validation](https://github.com/autonomous-ai/openharness/blob/0e6b16d78fd80697dea252376f19d7acf49d240f/backend/src/utils/slug.ts).

**Exact target selection is not guaranteed by the current CLI.** Its resolver
tries an exact ID, then unconditionally tries case-insensitive label/prefix
matches. `machines --json` is NDJSON and can preflight the ID, but `new`
fetches machines again. Deletion/rename between those steps can activate the
fallback. Checking the returned ID detects a mismatch after possible launch;
it cannot prevent that launch. This time-of-check/time-of-use gap requires an
owner-supported strict-ID path or explicitly qualified upstream behavior.
Local validation and fixtures do not close it.
[Resolver](https://github.com/autonomous-ai/openharness/blob/0e6b16d78fd80697dea252376f19d7acf49d240f/cli/src/lib/newCommand.ts#L199),
[machine listing](https://github.com/autonomous-ai/openharness/blob/0e6b16d78fd80697dea252376f19d7acf49d240f/cli/src/cli.ts#L2285).

## Observations and two recovery gaps

`ps --json` returns `{you,machine,harnesses:[...]}`: local agents plus tracked
pending creations, not every account agent. `show --json` returns an agent
ID, display fields, state, latest ask/answer, truncation flag and tool count;
name can be omitted. `wait --json` returns `{harnesses:[{name,id,state,detail?}]}`.
Its exit precedence is 124 for timeout/installing/starting, 3 for needs-input,
4 for stopped/gone/failed, otherwise zero. Caught command errors use 1 and
usage errors 2. A zero wait exit is not task success. Show/ps machine fields
are labels or prefixes; wait has none. Keep full launch identity separately.
[Observation commands](https://github.com/autonomous-ai/openharness/blob/0e6b16d78fd80697dea252376f19d7acf49d240f/cli/src/agentClient/commands.ts#L400).

First, each `new` makes its own creation ID and sends before writing its
caller-ledger receipt. Losing the initial reply can leave neither creation
ID nor agent ID, even though work started. There is no caller-supplied CLI
idempotency key. Persist intent and an attempting marker before spawning;
recover an interrupted attempt as uncertain, never as permission to launch
again. A unique name is only a reconciliation hint.
[Creation ordering](https://github.com/autonomous-ai/openharness/blob/0e6b16d78fd80697dea252376f19d7acf49d240f/cli/src/lib/newCommand.ts#L508).

Second, observation startup reconciles pending ledger entries before target
lookup. Once created, an entry moves to Started and no longer appears in
pending lookup; Started lookup does not match `child.creationId`. Thus a
fresh `show`/`wait` by a known creation ID can become unknown across that
transition. This is a control-flow inference, not a live failure measurement.
An already-running wait can follow the transition internally, but its final
agent-ID result omits the original creation ID. Preserve unresolved binding;
do not guess from a name, edit the ledger, or launch again.
[Reconciliation and lookup](https://github.com/autonomous-ai/openharness/blob/0e6b16d78fd80697dea252376f19d7acf49d240f/cli/src/agentClient/commands.ts#L180),
[pending ledger](https://github.com/autonomous-ai/openharness/blob/0e6b16d78fd80697dea252376f19d7acf49d240f/cli/src/agentClient/ledger.ts#L172).

Use finite observation deadlines, bounded stdout/stderr and one bounded
subprocess worker. Upstream wait defaults can be long or infinite; its own
timeout does not bound every stalled RPC. Killing the local process does not
cancel daemon/remote work. Observation commands can update Harness's pending
ledger and remote lineage, so “read” means no new task launch, not no side
effects whatsoever.

## Completion, questions and V1 protections

The CLI exposes no correlated result ID, input group, completed engine turn
or completion replay cursor. Latest text and successive idle observations
cannot establish that the lamp's request completed. The model explicitly
rejects synthesized transcript done events as proof. Keep launch, observed
output, completion and spoken-delivery evidence separate. The first adapter
must not announce automatic completion from these observations.
[Transcript semantics](https://github.com/autonomous-ai/openharness/blob/0e6b16d78fd80697dea252376f19d7acf49d240f/cli/src/agentClient/model.ts#L166).

`needs-input` supplies no stable dialog identity, question type or permission
options. It permits only a truthful needs-attention observation. Never
approve by voice or treat a close as approval. Outside-agent `send` lacks
the real-pane/same-tab authority required by Harness; terminal paste, raw
question RPCs and a new launch are not substitute follow-up routes.
[Message policy](https://github.com/autonomous-ai/openharness/blob/0e6b16d78fd80697dea252376f19d7acf49d240f/cli/src/messages/service.ts).

Keep V1's semantic protections from the [skill implementation](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/fd6705dd9e107817b33309d777cfa2d5c85e4062/skills/harness-use/scripts/harness.py),
[request service](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/fd6705dd9e107817b33309d777cfa2d5c85e4062/system/harness/service.go),
[result store](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/fd6705dd9e107817b33309d777cfa2d5c85e4062/system/harness/result_store.go) and
[permission handler](https://github.com/autonomous-ai/Physical-AI-Operating-System/blob/fd6705dd9e107817b33309d777cfa2d5c85e4062/system/server/harness_permission.go): durable
reservation before sending; uncertainty without resend; persist result
before replay acknowledgement; exact identity checks for every grouped
member; replay conflict detection; durable speech claims; and admission
distinct from completed playback. Relevant cases include
`test_uncertain_send_is_persisted_and_never_replayed`,
`TestResultStoreGroupSubsetReplayAndRestart`, `TestResultStoreAtomicMismatch`,
`TestResultStoreAdmissionIsNotPlayback` and
`TestHarnessPermissionNoticeOncePerOpenDialog`. These are source cases,
not newly executed V1 checks.

The older Device protocol has stronger correlation, but belongs to the
[route not taken](brain-build.md#the-other-route-not-taken-v1s-device-protocol).
Do not graft its pairing, encryption, result frames or cursors onto CLI
launches. Future qualified delivery retains the lamp's rules: explicit spoken
confirmation before sending, no progress narration, questions before results,
neither listening nor speaking and at least 1.5 seconds quiet, suppression
after ten minutes, and Voice's written turn rather than a second summarizer.
Only Voice can confirm playout; a crash can leave hearing uncertain.

## What has not been shown

No live CLI/daemon/account request, remote task, credential, permission dialog
or voice playback was exercised for this review. Source inspection establishes
neither installed compatibility nor strict target binding, reliable pending-ID
recovery, correlated completion, a permission sandbox, exactly-once hearing,
ARM64 interoperability or lamp latency. The runtime's host tests and limits
belong in [tasks.md](tasks.md); they cannot replace First's deployed Harness
and Voice qualification or an owner-supported resolution of these gaps.
