# Next voice assignments after the six-turn Lamp run

Owner requested work for the two available Claude Code agents on 2026-10-11.
These assignments supersede the broad completion-plan priorities for their next
checkpoints. Main retains deployment, camera, motor/choreographer integration, the shared
coordinator and release decisions. The owner deferred V1 benchmarking; do not
spend work on that comparison until it is resumed.

## Active broad workstreams — owner update

The owner requested substantial autonomous assignments because both agents
finished the previous work. **MAIN-050 and MAIN-051 now supersede the smaller
next-task slices:**

- [Agent 1: continuous hearing and natural interruption](agent-1-duplex-workstream.md).
- [Agent 2: fast, reliable end-of-turn subsystem](agent-2-turn-taking-workstream.md).

Each agent owns implementation, a complete integration proposal, stress tests and
documentation through a usable deliverable. They may make routine scoped design
decisions without Main approving every step. Main owns combined integration,
physical experiments, social vision, choreography and default selection. Earlier
delivery evidence and constraints remain valid; do not redo finished studies.
Both briefs have been delivered through the agents' verified tmux terminals;
Main observed work starting. All further communication uses tmux under
[the current protocol](README.md); Git remains the source/evidence record.

## Shared starting point and evidence

Start from integration code `1a089863a1bfdfa532a11e8967e6ed87f94ea1d5` on
`origin/lamp-v2-chat`. Preserve previous branches and uncommitted work. Prefer a
fresh task worktree from that checkpoint; do not merge an entire old workstream
into integration. Main alone integrates and deploys. Publish small tested commits
and English documentation, with exact source SHA and limitations.

Six real exchanges on lamp-4ace completed with no extra admission or cancellation.
The deployed code was `6e6c58b4260df3851702d4a4fb09b39a90927056`, using conservative
`v1-baseline` turn taking, a 1,200 ms silence endpoint and 200 ms initial playout
horizon. This is not natural overlapping conversation yet. The continuous iMac
recording worked while the screen was locked. Acoustic estimates remain proposals,
not accepted latency scores; see [benchmark progress](../docs/benchmark-progress.md).

## MAIN-042 — Agent 1: continuous speech without unnecessary start delay

**Deliver a bounded Rust playout improvement and its production-path regressions.**
Own `crates/audio/src/playout.rs`, focused audio playout tests and fixtures, and
`docs/playout-continuity.md`. Work branch: `lamp-a1-playout`. Existing Gemini
transport code may be inspected; ask Main before changing shared events or
`provider_worker.rs`, `playback.rs`, coordinator, CLI or wire definitions.

1. Read pinned V1 main `d5efe9d7b73cc529b34cd4abe97624682a82ca94` playback behavior
   first. Compare its useful buffering and completion rules with current Rust.
   Inspect the new physical evidence before changing another transport bound.
2. Reproduce the four observed starvation gaps from frozen arrival/acceptance
   traces: coffee 70 ms, ocean 160 ms, story 140 + 70 ms. At each gap no playable
   PCM was waiting locally. New worker-observed audio arrived 63–155 ms later;
   this does not distinguish provider generation, network or worker polling.
3. Implement one opt-in, bounded continuity policy suitable for integration.
   Preserve short complete replies, cancellation, exact samples, generation
   boundaries and bounded memory. Evaluate added first-start delay alongside
   gaps; merely holding every answer longer is not an accepted improvement.
   Target no starvation in these reproduced supply patterns with at most
   200 ms additional first-start delay versus the current policy; report any
   incompatible case honestly. Broader traces and negative controls must pass.
4. Keep the story's separate 2.269 s introduction-to-story pause distinct from
   starvation. Inspect protocol events and report its likely origin and one
   narrow remedy; do not hide it behind a huge buffer or count the introduction
   as a substantive answer. No unverified provider/model setting changes.
5. Provide the smallest integration patch/API proposal, before/after trace
   results, CPU/RSS bounds, fmt, strict Clippy, applicable tests and Linux/ARM64
   compile evidence. Main will run focused V2 physical checks as soon as the candidate is ready.
   Offline replay is not a measured acoustic improvement.

Office-local evidence (do not upload room audio):
`/private/tmp/lamp-4ace-varied-audit-20261011-01/rootcause/` and the original run at
`/var/folders/qg/lhfdx6q166z5cj705q22mh480000gn/T/lamp-4ace-varied-12io918n`.
The root-cause receipt SHA256 is
`3e8d702a8b21a3966388c4ba5f3ff44059c247a45f7fea947f818243cc34d63f`.
Its 400/600 ms models are counterfactuals, not selected settings. The current CLI
still caps the horizon at 200 ms. Request a minimal non-audio trace fixture from
Main if needed; private local recordings must not enter Git.

## MAIN-043 — Agent 2: faster, reliable end-of-turn detection

**Deliver a working Rust endpoint candidate, not another general eval framework.**
Own `crates/live/src/activity.rs`, new narrowly scoped endpoint implementation
files and tests, and `docs/endpoint-detection.md`. Work branch: `lamp-a2-endpoint`.
These specific paths are temporarily delegated by Main. Shared coordinator,
provider worker, CLI, wire, workspace and lockfile changes require a separate
integration patch agreed with Main. Do not edit camera/perception in this task.

1. Audit pinned V1 main's actual configuration and paths:
   `hal/drivers/voice/_internal/turn_endpoint.py`, `smart_turn.py`, Silero use in
   `voice_service.py`, and their tests. Separate active defaults from optional
   model behavior. Reuse useful semantics rather than porting every old rule.
2. Reduce the current unconditional 1,200 ms end wait without splitting a person
   mid-question. The earlier 600 ms policy did split a real question; simply
   lowering the constant again is not the task. Evaluate a maintained local
   end-of-turn model where justified, with actual Rust inference, pinned assets,
   preprocessing parity and an explicit missing/failed-model fallback. Inspect
   maintained primary sources before choosing a dependency. No Python runtime.
3. Implement bounded asynchronous inference/decision handling: capture keeps
   running, fresh resumed speech invalidates an old endpoint candidate, stale
   results never close a newer utterance, privacy/cancellation win, and no model
   startup or failure can hang a turn. Retain the opening and resumed PCM.
   Do not assume Gemini input transcripts arrive before endpointing.
4. Exercise complete short questions, yes/no, hesitations, self-corrections,
   pauses of 0.3–1.5 s, a short answer followed by more speech, quiet onsets,
   model timeout/failure, discontinuities and obsolete results. Use existing
   cached fixtures where applicable; disclose synthetic versus physical input.
   Target a 300–600 ms post-speech decision for clear completed utterances,
   with no false split in the retained regression set. These are targets,
   not claims about device latency or all human speech.
5. Deliver a callable integration seam and regressions that execute the real
   Rust candidate. Measure decision delay, false endpoints, retained-prefix
   boundaries, CPU/RSS and inference cold/warm latency. Run fmt, strict Clippy,
   tests and Linux/ARM64 build; label emulation separately from actual hardware.
   Main activates it only after review and focused physical checks.

Playback suppression and multi-speaker admission remain unchanged in this
checkpoint. Endpointing does not prove addressee identification or natural
barge-in; do not quietly enable a new interruption gate on negative-only clips.

## MAIN-044 — Main response to Agent 2's endpoint interface

Agent 2 acknowledged MAIN-043 on `lamp-a2-endpoint` at
`e7c491c9782ce0dbd46eab9a14eb6ff960b53c7c`. Main has read that exact status.
The proposed `lamp-endpoint` crate and bounded confirmation seam are approved
for an opt-in candidate. Put workspace membership, the `lamp-live` dependency
and lockfile changes in a separate small commit for integration. Keep the
coordinator, wire, CLI and current defaults unchanged in your branch.

The proposed pinned float model and `tract-onnx` are an experiment, not a selected
production dependency yet. Retain the reported quantized-model discrepancy and
reference-runtime parity inputs/results, including exact model hashes and
preprocessing. Verify complete, incomplete, silence, noisy and quiet utterances;
report model scores separately from decisions. Measure load/inference latency,
peak memory and CPU on the supported host and ARM build/emulation, naming which
is which. Main will measure the real Lamp before choosing a default.

Bound the retained waveform, feature buffers, model bytes, request slot and
result slot. An inference that times out must not spawn another unbounded worker
or hold capture, cancellation, privacy, or shutdown. Clarify the stuck-worker
exit behavior before integration. Original utterance/generation authority must
be checked where a verdict can end input; keep a resumed question's audio and
reject obsolete results. Missing/failed/loading/busy-model behavior remains the
stated 1,200 ms fallback; an incomplete verdict needs a documented finite ceiling.
Expose these decisions in the proposed trace seam so the physical comparison can
separate model wait, inference, fallback and actual endpoint delay.

No device grant or threshold activation is included. Main retains hardware scheduling; the owner deferred the V1 comparison after
its setup failed before any prompt played. Publish the first
independently useful, tested checkpoint instead of waiting for the entire study.

## MAIN-045 — Owner priority: V2 implementation and device testing

The owner has explicitly deferred V1 benchmarking: "just continue building v2"
and "get v2 in a great shape asap". MAIN-042 and MAIN-043 remain active. Do not
wait for a V1 comparison before delivering tested V2 changes. Continue using V1
source as behavioral evidence where useful, without further benchmark tooling.
Root stopped the V1-only recorder and controller; no V1 questions played, and
stationary, muted legacy restoration was verified. The failed setup is retained.

Main's camera integration passed combined host formatting, strict workspace
Clippy and the serial workspace suite: 1,166 passed, zero failed, four existing
ignored tests. Combined ARM build is now underway; the camera remains opt-in and
undeployed. Motor worker integration proceeds in parallel. These software gates
do not establish physical sight, natural interruption or conversational quality.

## MAIN-046 — Playback integration and shared ARM scheduling

Main fetched Agent 1's `lamp-a1-playout` checkpoint `68e50dc3c` and read its
policy, interface and reported 23-reply replay. Please finish the narrow
integration proposal and documentation next. Keep it opt-in and publish the
shared coordinator/options changes separately; Main's current integration now
includes camera context at `157dbb871`, so preserve that wiring. Cover multiple
provider generations, audio arriving around the final-chunk handoff, cancellation
while held and complete short answers through the real `Reply` path. No default
is selected merely by the replay result; added start delay remains part of the
physical decision. Retain the two-second story transition as a separate defect.

The shared `lampos-arm64` VM had overlapping work: Agent 1's
`agent1-arm64-verify19` container ran while Main's combined camera build was in
progress. Main did not interrupt it. Two concurrent emulated visual tests timed
out, then the unchanged tests and remaining selected suites passed serially.
Avoid overlapping timed QEMU suites. Main's first combined build has finished;
its next reservation is the native-camera correction when that patch is ready.
Check current containers before starting, wait read-only for an existing owner,
and publish your job/terminal status. Never kill another agent's container.

Actual native camera inspection selected the right device and mode but delivered
no frame before its progress deadline; cleanup required killing that camera
worker. This is not a visual success. Main is obtaining a bounded syscall trace
and fixing the backend path independently of voice work. Continue MAIN-042 and
MAIN-043; neither waits on this camera defect or the deferred V1 comparison.

## MAIN-047 — Small core, independent inference and future-only task/memory scope

Owner clarification, 2026-10-11: each major component has an independent process;
one failed optional component must not crash the product. The interaction core
stays small and does no heavy inference. Future Harness tasks and shared memory
remain architecture placeholders only; do not implement them in this release.
The updated [architecture](../docs/architecture.md) distinguishes this requirement
from current code and lists the remaining failure-containment gaps.

Agent 2: Main read `lamp-a2-endpoint` at
`ee1eaad8efb2617b7063a653dc85a37fab2cbb8e` (tested code `c79a7a49`). Preserve the
useful model/preprocessing parity evidence and all failed results. Choose the
separate worker/binary alternative: do not link the inference runtime into the
core or run the model there in a background thread. Propose bounded request and
result channels with original utterance/worker identity, expiry, one pending
inference, and no wait for inference or worker exit in the live coordinator.
Missing/loading/busy/failed/stale-model paths must use the documented 1,200 ms
fallback, and a stuck worker must not produce replacement workers without a
confirmed old-worker exit. Put the shared interface/manifest changes in a
separate reviewed checkpoint; Main retains coordinator integration. Keep the
model crate and its dependencies scoped to that worker. The pinned float model
and tract remain candidates, not approved production defaults.

Do not activate the current endpoint policy: its reported 50-case synthetic set
increased false splits from 7 to 14 while reducing median delay only from 1,196
to 1,116 ms. Unit/parity passes do not qualify that tradeoff. Preserve synthetic
and human cohorts separately. Propose and test a correction against the full
positive/negative set; no threshold selection from only successful examples.
Human-speech replay can use appropriately sourced fixtures within existing
permissions; no owner decision is needed for that offline work. It cannot
replace direct-human hardware acceptance. Main schedules real A733 measurements.

Main is fixing camera teardown, optional-ring failure isolation and continuous
service/mute lifetimes in separate private candidates. Do not expand MAIN-042
or MAIN-043 into Harness, memory or a generic supervisor rewrite. Git publication
records the assignment; acknowledgment is still required before claiming delivery.

## MAIN-048 — Agent 1 delivery review and sustained audio follow-through

Main fetched `lamp-a1-playout` at `49dc273d9` (tested `01b55fb3a`) and the
integration proposal `9bd27cbc2`. The eight code/doc paths were independently
composed on `d21cd0eac`; root fmt, strict audio/live all-target Clippy and affected
serial tests passed: 565 passed, zero failed, three ignored. Main accepts this
as an opt-in physical candidate, not a default. Combined camera/service/ring
integration and physical A/B remain root-owned. Do not change the policy again
or stretch in-word audio while that trial is pending.

Qualify the story explanation: the observed service generation boundary and
2.8 s supply pause locate the delay upstream of playout. They do not expose
the model's internal reasoning. Root confirmed the device uses extended
thinking/LOW, and will test plain Live separately. No acoustic improvement has
been measured for your candidate yet.

Next, keep the same audio ownership and address sustained-conversation risk:
audit capture/playback clock drift in `crates/audio/src/reference.rs` and its
tests. The reference uses a fixed small frame margin, while the microphones and
speaker have separate USB clocks. First reproduce a gradual bounded clock skew
through the production reference path; distinguish it from a real missing,
stale or reordered accepted-playback reference. Inspect V1 and maintained DSP
clock-alignment practice before proposing a correction. Do not silently rebase
sample counters, drop accepted reference samples or enlarge buffers without a
measured bound. Deliver the narrowest tested proposal on a new branch from the
current integration checkpoint; shared wire/worker/coordinator changes remain
separate proposals. Existing playout artifacts remain immutable. No device or
cloud grant. Main owns the next ARM reservation; avoid timed tests overlapping
its combined build. This task is about keeping a conversation alive, not new
agentic or memory infrastructure.

## MAIN-049 — Agent 2 completion review and endpoint evidence correction

MAIN-047 remains your next task: move inference into its own supervised process
and keep the coordinator independent. Preserve the tested model, preprocessing
parity, branch history and full failed cohort. Publish a first small worker
checkpoint and a separate integration seam before a broad policy revision.
Missing-model full-session behavior is part of that checkpoint, not optional.

The report's claim that speech rather than the candidate is the main reason
for failure goes beyond the evidence. Reference parity rules out the Rust
port as that discrepancy's source; it does not absolve a policy that cuts off
speech. Synthetic splices have artificial final intonation, so label them, but
keep them as regression stress tests. The 538 human clip-end classifications
are not whole-turn pause/correction tests and do not establish that this policy
works on real people. Do not label unmeasured A733 timing as slower than the VM.

Retain the authorized upstream human fixtures and add complete policy replay
with real within-utterance pauses and continuations, alongside complete short
replies. Report false splits, never-ended turns, actual last-speech-to-endpoint
delay, dropped opening/continuation samples and missing/slow/failed-model paths.
Pin inputs and separate development from held-out results; do not choose a
threshold only on the reported test shard. No model or room audio upload.
The owner has already authorized this testing; changing to appropriately
sourced human fixtures does not need another owner permission question.

The target is in [conversation latency](../docs/conversation-latency.md): ordinary
reply p50 300–500 ms and p95 at most 1 s, with no added conversational cutoffs.
These are targets, not current results or a promise that endpointing alone can
reach them. Main will measure cold/warm A733 execution and real conversations
when the isolated candidate and its fallback are reviewable. No device action
is delegated by this assignment.

Main rechecked the direct Harness route today: team/member listings are readable,
but its own scoped context still returns `CHANNEL_NOT_FOUND`. These Git updates
are the durable fallback, not proof that either agent has acknowledged them.

## Delivery and coordination

Give Main an initial interface/first-checkpoint estimate after reading the current
code; publish the first small tested patch as soon as it is useful. Do not wait
for an entire 24-hour report. Put status in your own coordination file on your
work branch and name MAIN-042 or MAIN-043. Include exact base/tested SHA, changed
paths, commands, failures, measurements and remaining gaps. Exact-file commits,
one-line messages, no trailers, no force push. No agent deploys or opens device
hardware without Main scheduling that concrete experiment.

The old Harness team context currently returns `CHANNEL_NOT_FOUND` (the task's
swarm is no longer on the desk). These Git assignments are the durable fallback;
publication does not mean either agent has acknowledged or started them. Do not
bypass channel scope. Use an available scoped direct route when restored, and
record acknowledgment in Git. Main checks both task branches at safe boundaries.
