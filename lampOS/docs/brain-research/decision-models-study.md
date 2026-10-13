# Decision models beside agent engines

12 October 2026. Design research and proposed contracts; no implementation,
inference calls, installation, device access or private-data inspection.
The owner accepts independently replaceable agent engines and decision models,
including Jev and Jev-like providers. This does not select a production model.
Read with [agent engines](agent-engines.md), [ROS-derived contracts](ros2-design-study.md)
and the corrected [V1 audit](../v1-brain-audit.md). Older `brain.md` comparisons
must not erase V1's sensor-driven agency, remote Hermes, skills, local rules or
existing Jev integration. Section 7 corrects that last omission explicitly.

## 1. Recommendation and ownership

Make `DecisionProvider` a separate optional capability from `AgentEngine`.
An engine runs a model/tool loop for open-ended goals; a decision provider
answers a bounded semantic question over selected state. Neither owns the body.
Preserve V1's working Jev patterns behind a common provider boundary; this is
not a new V2 capability. Jev can complement Hermes, Pi, Codex or Claude without becoming their scheduler,
memory database or mandatory frontend. Engines may also request a bounded
decision through an OS tool, under the same budgets and data policy.

```text
observations -> OS situation state ------------------------+-> agent context
                     |                                    |
         deterministic eligibility + context selection     +-> local rules
                     |                                         |
              decision request -> provider adapter              |
                     |          [Jev / local / other]            |
              typed judgment or failure                         |
                     v                                          v
           OS acceptance / abstention -> behavior or agent task proposal
                                                |
                     action admission -> executor -> actual result receipt
```

lampOS owns observation provenance, attention, budgets, permissions, persona
policy, user associations, durable memory and task/action IDs. The engine owns
its working session; an adapter maps engine IDs to OS IDs. A decision request
is not a task, and a selected action is not an execution receipt. Provider
replacement does not rewrite memories, restart physical work or reset limits.

## 2. What Jev actually exposes

**Documented:** text or structured JSON state; no audio, images or video.
Independent typed questions can share one state. Choice selects an enumerated
option and returns its distribution; Score describes an ordered rubric;
Noul returns a yes-probability. This is no free-text reasoning or speech API.
[State](https://docs.typesafe.ai/concepts/state),
[primitives](https://docs.typesafe.ai/introduction)

**Documented:** `POST /v1/systemone` accepts `model`, `state`, `questions`;
results contain resolved `model`, named `answers` and token `usage`. Public
OpenAPI 0.2.0 lists that operation and `GET /v1/models`, with no session, task,
streaming or server-cancel operation. Its JSON schema is looser than some
prose limits, so an adapter must validate its supported subset itself.
[OpenAPI](https://api.typesafe.ai/openapi.json)

**Current vendor listing:** `jev-1.13.0`; moving aliases include `jev-latest`.
Pin the version and log the resolved ID. Listed limits are 64k total tokens and
32k for state plus the longest question. Price is $0.042 per million input
tokens, output free. Rate limits are explicitly changeable. English is the
strongest supported language; no per-customer fine-tuning is offered.
[Models](https://docs.typesafe.ai/models)

**Meaning of outputs:** Choice supports up to 255 candidates; include a genuine
none-of-these alternative when appropriate. Score supports descriptive ordered
levels, up to 10; its fractional value is their probability-weighted position,
not a measurement such as brightness, temperature or duration.
[Choice](https://docs.typesafe.ai/primitives/choice),
[Score](https://docs.typesafe.ai/primitives/score)

**Confidence is not an accuracy certificate.** Choice confidence is
`(p_max - 1/n) / (1 - 1/n)`; changing candidate count changes its interpretation.
Score confidence uses spread across ordered levels. Noul has no separate
confidence. Retain distributions and native statistic identity, not a universal
`certainty` field. Empirical calibration requires our labeled workload.
[Confidence](https://docs.typesafe.ai/confidence)

**Published limitations:** literal interpretation, numeric/date arithmetic,
indirection, irrelevant state, adversarial input and option-order sensitivity.
Typed output prevents an out-of-schema answer; it does not make the selected
answer true. Model-readable state remains untrusted data.
[Jev 1.13 limitations](https://docs.typesafe.ai/model-jaggedness/jev-1.13)

**Vendor evidence, not Lamp measurements:** the launch post claims 70–500 ms
responses and describes West Coast laptop evaluation. Workflow evals compare
with other models' averaged answers while assuming the workflow is correct;
they do not establish human-grounded robot reliability or tail latency.
[Launch post](https://typesafe.ai/blog/introducing-system-one-models-and-jev),
[evaluation method](https://evals.typesafe.ai/)

## 3. Choose the appropriate mechanism

| Mechanism | Use it for | Keep outside it |
| --- | --- | --- |
| Deterministic local rules | Stop/privacy, interlocks, ownership, exact arithmetic, timers, freshness, quotas, explicit controls | Ambiguous social meaning |
| Local specialized classifier | VAD, wake detection, presence or another narrow task with a qualified model | Treating perception confidence as permission or identity certainty |
| Pluggable decision model | Intent among eligible routes, context relevance, skill relevance, choice among permitted small behaviors | Raw perception, unconstrained planning, hard safety, numerical control |
| Generative model inside an engine | Conversation, novel goals, explanations, multi-step tool use and recovery | Direct actuator authority or silent expansion of permissions |

A simple local rule is a real baseline, not an inferior fallback by definition.
A hosted semantic gate cannot establish pre-upload privacy: transcription may
already have sent room audio elsewhere. Local capture/upload admission precedes
STT, Jev and the agent. Presence, speaker, addressee and authority stay distinct.

## 4. Proposed typed interface, version 1

The following is an interface sketch, not TypeSafe's wire format or new code.
Both local and remote implementations expose capabilities and asynchronous
evaluation. Unsupported output types or uncertainty semantics fail explicitly.

```text
DecisionProvider.describe() -> {
  adapter_version, provider_id, available_model_versions, deployment_class,
  question_kinds, uncertainty_kinds, input_limits, cancel_semantics
}
DecisionProvider.evaluate(DecisionRequest, cancellation) -> DecisionReply

DecisionRequest {
  schema: "lamp.decision.v1", request_id, attempt_id, trace_id,
  purpose_id, spec_version, policy_version, provider_profile_version,
  task_id?, turn_id?, subject_scope, locale,
  context: {snapshot_id, projection_version, facts[], source_refs[]},
  questions: [Choice | OrdinalScore | BooleanJudgment],
  candidate_set_hash, eligibility_version, data_policy_version,
  budget: {remaining_ms, max_input_bytes, max_attempts, max_cost_micros}
}
Fact {id, value, source, observed_at, received_at, expires_at,
      source_quality?, subject_ref?, provenance_class, disclosure_class}
Choice {question_id, instruction_version, candidates: [{id, description}]}
OrdinalScore {question_id, instruction_version, levels: [{id, description}]}
BooleanJudgment {question_id, instruction_version, yes_criterion, no_criterion}

DecisionReply {
  schema: "lamp.decision.v1", request_id, attempt_id, context_snapshot_id,
  candidate_set_hash, resolved_provider, resolved_model, adapter_version,
  outcome: Answered{answers[]} | Failed{code} | Cancelled | Expired,
  usage: {input_tokens?, output_tokens?, billed_cost_micros?, cost_known},
  timings: {queue_ms, request_ms, total_ms}, received_at
}
Answer {question_id, kind, value, distribution?, uncertainty_kind,
        native_confidence?, calibration_profile_id?}
OSOutcome = Accepted{value} | Abstained{reason} | Discarded{reason}
```

The OS deadline uses its monotonic clock; only remaining duration crosses hosts.
Wall timestamps explain observations, not cross-host deadline comparisons.
Question definitions and rubric text come from versioned specifications; state
cannot rewrite them. Native provider probabilities, calibrated estimates,
self-reported LLM scores and absent uncertainty are distinct capability values.
Never invent probabilities for a provider returning only a discrete answer.

Validate exact question IDs/types, known candidate/level IDs, finite values,
probability bounds and sums within a documented tolerance, response size and
resolved model version. Reject malformed results rather than silently renaming
choices, normalizing invalid outputs or fabricating a successful empty reply.
An ordinal score cannot become a physical magnitude by interpolation.

## 5. Eligibility, abstention and changing state

Before inference, the OS filters candidates by available hardware/services,
permissions, privacy mode, task scope, behavior preconditions and fresh evidence.
Do not ask the model to choose an unavailable action and hope it declines.
No eligible candidates means no request. One candidate is not proof it applies:
use a Boolean applicability judgment or retain `none`/`insufficient_evidence`.
Those semantic alternatives are distinct from provider timeout or local denial.

Only send the projection needed for that question: recent relevant observations,
attributed user text, applicable preferences and authoritative action receipts.
Keep absent, contradicted, stale and unavailable facts distinct. Identity doubts
survive projection; a remembered name is not evidence of the current speaker.
Facts containing user/third-party instructions remain labeled content.

Accept only while the request is active and its required facts, candidate set,
permission epoch and relevant state revisions still match. Unrelated observation
updates need not invalidate it. Revalidate action preconditions at execution.
Changed addressee, stopped task, privacy toggle or expired evidence discards a
late answer even if its confidence is high. Never replay it after reboot.

Use a policy per decision/version/model and consequence, with calibrated risk
and coverage targets. Uncertainty, a missing required fact, conflicting identity,
unsupported locale or missing calibration can each produce abstention. Do not
multiply several Noul answers as independent probabilities; shared evidence and
model errors can correlate. Dependent questions need explicit code sequencing.

## 6. Deadlines, load and fallback

Proposed initial qualification profiles, not measured performance promises:

| Decision | Deadline policy | Failure/abstention |
| --- | --- | --- |
| Physical stop/privacy/explicit controls | No model dependency | Local controller acts immediately |
| Optional context or skill hint | Prepare concurrently; use only if ready at the turn boundary; cancel after 500 ms | Original context/skill path; no late insertion |
| Optional small autonomous behavior | 500 ms admission-to-result; also bounded by fact expiry | Remain still/quiet; preserve useful state |
| Background relevance evaluation | 2 s end-to-end, one bounded retry only if budget remains | Keep authoritative records; defer optional ranking |

Start the pilot with at most two in-flight requests, one pending request per
purpose/subject, eight questions, 32 candidates/question and 32 KiB total input.
Coalesce replaceable observations; user task requests retain their own queue.
Cancel superseded work, cap daily/background spend and open a circuit breaker
on sustained failures. Do not spend the voice route's reserved budget on hints.

TypeSafe's JavaScript SDK timeout is per attempt, with no total retry budget;
its defaults retry twice and honor server delay instructions. An OS deadline
must cover queueing, retries and fallback. Client cancellation does not prove
server computation or billing stopped; the published API has no cancellation
receipt. Record unknown cost explicitly and suppress the late result locally.
[Request options](https://docs.typesafe.ai/sdk/javascript/api/interfaces/RequestOptions),
[retry policy](https://docs.typesafe.ai/sdk/javascript/api/interfaces/RetryPolicy)

Pin each fallback's calibration and data policy independently. Do not silently
replace Jev with a slower generative call on an interaction path. A caller may
escalate an unresolved user goal to the engine within its original authority;
an optional ambient judgment normally ends quietly. Provider failure never
terminates lampOS, disables local controls or consumes an agent task as success.

## 7. Existing V1: retain contracts, redesign ownership

The following source exists in pinned V1 `170a1b85e7e6197097019e4175efdcbe416e3c2c`;
the audited implementation paths are unchanged at current `b8c23cc9916de3f074c34b8689134437d2ebb4b6`.
This is code/default evidence, not proof of a particular device's configuration,
installed plugin revision, network availability or observed benefit.

| Existing path | Verified behavior and native policy | Preserve or redesign |
| --- | --- | --- |
| Device intents | Default on; canonical local intent first, capability-filtered catalog, then Jev, then normal agent fallback. One request, at most 3 s; busy calls skip, errors cool down 30 s. Choice probability ≥ .90, margin ≥ .40 and Noul fit ≥ .95; finite parameters also checked. `system/server/config/jev_intent.go:24`, `system/intent/semantic.go:18`, `system/intent/jev/resolver.go:43`, `system/intent/jev/client.go:265` | Retain local-first routing, offered-ID validation and code-owned execution. An attempted local execution is handled even if it fails, avoiding a second agent execution. These thresholds are domain policy, not universal confidence. |
| Hermes skill preload | Local managed plugin defaults on; remote runtime provisioning is skipped. Native-eligible catalog, one Choice plus fit questions; ≥ .70 / .20 / .60 probability/margin/fit, 3 s limit, 30 s failure cooldown, busy worker retained until it ends. Voice-tagged turns explicitly bypass it; full history is not sent. `runtimes/hermes/jev_plugin.go:38`, `runtimes/hermes/plugins/jev/router.py:19`, `:230`, `:341` | Retain advice-only semantics, native skill loading/permissions and voice bypass. An updated file may require a runtime restart; do not infer that remote Hermes received this plugin. |
| Other engine skill adapters | Common Go selector validates current catalog/content after inference and only supplies a current-turn skill hint. Codex, Claude Code and OpenCode build flags are false; PicoClaw's hook is also build-disabled and uses the Hermes router. `system/lib/jevskills/router.go:61`, `:195`, `runtimes/codex/gatewayd/jev.go:14`, `runtimes/claudecode/gatewayd/jev.go:13`, `runtimes/opencode/gatewayd/jev.go:14`, `runtimes/picoclaw/jev_hook.go:16` | Existing adapters are useful source, not evidence of enabled deployment. Retain native catalog exclusions, explicit-skill precedence, bounded inputs and rejection of late results. Go/PicoClaw skip context-dependent fragments and proactive/system events. |
| Harness target selection | Default on independently of local intent. Uses an already-fetched roster, ≤32 candidates, metadata ≤1,000 bytes each, 30 s snapshot lifetime; default 1.5 s inference budget capped at 3 s. Same strict intent thresholds; failure retains the proposed target. `system/server/config/jev_harness.go:7`, `system/server/harness_selection.go:41`, `:95` | Selection happens once before delivery reservation. The helper checks machine/server identity again, persists the selected target and idempotency key, and keeps that target during reconciliation. Preserve this invariant; add explicit snapshot/content revisions. `skills/harness-use/scripts/harness.py:519` |
| Buddy suggestion | Default on, advisory only. Fresh nontruncated UI snapshot; explicit enabled state, secure ancestors excluded, ≤32 press/focus candidates; .90 / .40 / .95 acceptance. Decision budget ≤3 s and remaining snapshot life; handler budget 8 s including observation. `system/buddy/jev/selector.go:75`, `:129`, `system/buddy/jev/client.go:212`, `system/server/buddy/delivery/http/handler_suggest.go:24` | Preserve typed references, snapshot expiry and absence of automatic UI execution. This is already a useful example of deterministic eligibility before semantic selection. |

**Transport is a separate compatibility profile.** V1 calls the authenticated
BFF `POST {llm_base_url}/jev/decisions` with model `typesafe/jev-1.13` and the
existing device key. Repository docs describe BFF forwarding to OpenRouter's
`/api/alpha/decisions`; that external implementation was not independently
audited. Direct TypeSafe uses `/v1/systemone` and different model/auth conventions.
Do not silently change endpoint, credentials or model naming during abstraction.
See `system/intent/jev/client.go:20`, `:65`, `docs/os-server.md:1321`.

**Concrete redesign needs:** Hermes caches ambient hints by event header plus
skill names, without payload, skill contents, model version or expiry in the key
(`runtimes/hermes/plugins/jev/router.py:129`). Replace this with a bounded,
purpose-specific semantic cache tied to context and catalog revisions. Current
parsers mostly discard resolved-model and usage fields; retain these for version
qualification, cost and tracing. Preserve per-purpose thresholds while sharing
transport/validation. Skill and Harness projections often lack conversation
history; supply only necessary attributed original-goal/correction context,
not indiscriminate transcripts. The proposed 500 ms hint profile is an experiment,
not an assertion that today's synchronous 3 s route has already been improved.

### Bounded behavior and skill bridge

| Situation | Semantic work, if useful | Deterministic ownership |
| --- | --- | --- |
| Someone arrives during another conversation | Choose `wait`, `small_acknowledgment`, `none` from eligible behaviors | Attention, quiet hours, motion hold, fresh presence and initiative budget |
| An admitted utterance concerns a light | Choose known intent/target or `unclear`; numbers parsed and checked separately | Authorization, brightness bounds and actual light result |
| User asks to find keys | Engine plans a bounded search; optional model ranks already permitted alternatives | Camera disclosure, motion envelope, cancellation and search deadline |
| A task needs context | Rank already authorized candidate facts or skills | Required facts always included; source/age preserved; memory unchanged |

A behavior is an executable bounded program with declared inputs, resources,
preconditions, maximum duration, cancellation and observable result. Jev may
select among such programs; it cannot create arbitrary shell commands or motion
trajectories. A model-selected behavior still competes for local resources.

V1 event guidance and user context (`system/lib/sensingmsg/sensingmsg.go:82`)
and expiring/coalescing ambient replay (`runtimes/hermes/events.go:138`) remain
useful. Separate preserving situation state from deciding to invoke a model.

Bridge reviewed V1 skills to versioned tool/action contracts and capability
manifests. Many currently embed loopback HTTP commands, for example
`skills/servo-control/SKILL.md:60`; remote engine portability needs an adapter,
not copying those addresses. Keep general task skills and agent reasoning;
do not reduce the whole skill catalog to fixed behaviors.

TypeSafe's skill cookbook ranks 182 Hermes skills then rechecks three, suggesting
at most one. Its vendor-run, synthetic single-turn test reports wrong loads
16.8% to 7.3% and needless loads 9.8% to 4.0% over 488 requests; some formerly
correct cases worsen. The agent retains its judgment. This supports testing
advisory selection, not assuming Lamp improvement or requiring two calls per turn.
[Cookbook and method](https://docs.typesafe.ai/cookbooks/skill_suggestion)

## 8. Deployment, qualification and remaining questions

The adapter runs outside the deterministic core; a hosted provider needs network
access, whereas a qualified local provider can run in a supervised worker.
Jev self-hosting, weights, regions and offline operation are not established by
the reviewed public contract. Do not promise them. TypeSafe says it does not
train on customer inputs; enterprise ZDR is separately offered. Its privacy
policy describes US hosting and nonzero default retention terms. Confirm the
actual account/deployment terms before household data is routed there.
[Data handling](https://docs.typesafe.ai/legal),
[privacy policy](https://typesafe.ai/legal/privacy-policy)

Disclosure rules run before serialization: send minimal authorized facts,
omit secrets/biometric templates, and respect deletion and purpose restrictions.
Default traces contain versions, IDs, reason codes and timing, not room text.
Private evaluation examples require a separate approved storage/retention path.

Qualify against local rules, no optional decision, Jev, and at least one other
provider using the same allowed evidence. TypeSafe publishes an LLM-backed
compatible adapter, useful as an evaluation reference; its normalization and
repair retries do not prove equivalent uncertainty or deadline semantics.
[Alternative-provider adapter](https://github.com/typesafe-ai/system-one-adapter-python)

Use held-out human-reviewed sequences, not model agreement as ground truth.
Measure false action/reply rates, missed useful actions, abstention/coverage,
calibration by task/locale, option-order sensitivity, context omission, cost,
and admission-to-useful-result p50/p95/p99. Include ASR errors, overlapping
speakers, stale identity, prompt injection, unrelated detail and unavailable
candidates; inject 429s, disconnects, quota exhaustion and delayed duplicate replies.
Split evaluation data by household/time; keep threshold tuning out of the test set.

First pilot: shadow context/skill advice, then opt into one reversible bounded
behavior only after measured benefit at fixed error limits. Replay with a second
provider and with none; physical controls, task identity and memory must survive
every switch. Open qualification gaps are real household accuracy, regional tail
latency, per-purpose calibration, privacy terms and benefit over simpler rules.
