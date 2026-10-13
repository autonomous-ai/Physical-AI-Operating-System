# Building and migrating lampOS V2

**Latest discussion:** the [HTML architecture draft](v2-brain-design.html#build)
(13 October 2026, pending owner approval) updates the build sequence around the
Rust executive and pluggable decision layer. Use its sequence for the current
review; retain the detailed work packages below as supporting migration research.

12 October 2026. Proposed implementation and release plan for the
[recommended architecture](v2-architecture.md). This review authorizes no
runtime changes, deployment or private-data access. Work packages below are
future engineering scopes, each with a reviewable result and an exit gate.

**Build a standalone Rust replacement incrementally, keeping V1 as the working
product until the chosen V2 scope passes parity, recovery and device tests.**
Retain the useful V2 cores. Port V1 behavior and protocols selectively; replace
hidden coupling and ambiguous outcomes. Do not first rewrite every V1 file or
build a general middleware platform.

Read the [58-capability inventory](brain-research/v1-migration-inventory.md)
alongside this plan. Its IDs (`V`, `S`, `B`, `A`, `I`, `O`, `D`) make omissions
visible. Conditional/deferred capabilities remain recorded with their private
data intact. They do not count as completed parity.

## 1. Build order

```text
 P0  Contracts, capability scope and fixtures
  |
  +--> P1  Provider-independent lifetime / supervision --+
  |                                                    |
  +--> P2  Local controls + action/graph seams ----------+
             |                                         |
             +--> P5 Senses/context + attention         v
             |       |                          P3 Useful fallback voice
             |       v                                 |
             |    P6 Body behaviors + motion            v
             |                                  P4 Durable agency/memory
             |                                         |
             +--------------------------+--------------+
                                        |
                                        v
                                P8 Integrations + UX
                                        |
 P1/P2/P4 ------------------------------+--> P9 Operations / migration
                                                   |
 Selected mandatory packages ---------------------+--> P10 Qualification

 P4/P5 ---> P7 Optional decision models ---> qualify separately
            (does not block P8/P9 or core release)
```

This shows dependencies, not a demand to serialize independent file-scoped
work. P5 can progress beside P3/P4. Operational foundations start in P1/P2 and
must not be postponed until the last week. Optional Jev work must not block
reliable conversation or local controls. No calendar estimate is credible until
hardware/profile and per-package results are known.

The earliest useful deliverable is P1: Live exhaustion no longer kills the OS.
It can initially report voice unavailable while local capabilities continue.
P3 then provides the agreed useful fallback. These are separate acceptance
criteria; keeping a process alive alone is not successful fallback.

## 2. Preserve these foundations

Source snapshot: `e1eae8de5ad65c1ec6c6450948afbcb510982240`. Existing package names
below are real; new module boundaries later in this plan are proposals.

| Existing Rust area | Retain | Required extension or qualification |
| --- | --- | --- |
| `crates/interaction` | Pure ownership, privacy lineage, leases, observation provenance and stale-output rejection | Add distinct conversation/action/task/persistent-state ownership without weakening final-writer checks. It is not an addressee detector. |
| `crates/ipc` | Nonblocking bounded Unix datagrams and independent urgent/bulk endpoints | Versioned semantic envelopes, registration and QoS policies; do not exceed 8192-byte packets or pretend send acknowledgment means execution. |
| `crates/audio`, `crates/acoustic` | Native capture/playout, accepted render reference, AEC/VAD boundaries, retained input | Real room duplex, route changes, privacy, first-word retention and physical interruption qualification. |
| `crates/gemini`, `crates/live` | Useful wire/session/audio integration and explicit provider-output settlement | Recoverable provider lifetime, tool calls/responses/cancellation, scoped session context and fallback route manager. Extract blocking lifecycle operations from interaction dispatch. |
| `crates/camera` | Granted bounded frames, consumer roles, latest-frame behavior and original provenance | Independent perception clients, metadata/clock honesty, local-only person detection and complete retention cleanup. |
| `crates/environment` | Independent field freshness, uncertainty, units and fixed-size state | Actual selected-board Rust sensor drivers and source policy. Existing pure state tests do not establish hardware support. |
| `crates/ring` | One writer, ownership, revision/expiry and playback-driven cues | Useful lamp setting versus transient overlay, existing V1 lighting behavior and visual qualification. |
| `crates/motor` | One bus owner, qualified limits/profile checks, combined-pose clearance, narrow measured stop | Per-unit physical qualification, selected useful gestures and tracking; no generic safe clearance defaults or raw trajectory replay. |
| `crates/observer`, `crates/voice-eval` | Evidence collection, fixture replay and explicit measurement boundaries | Unified task/action/channel traces and safe sensor/model stubs. Diagnostic loss must remain visible. |

Keep reviewed DSP/inference/crypto/storage libraries and system facilities where
they fit. “Rust runtime” does not mean rewriting Linux, every native library,
remote Hermes internals or the existing React UI. No permanent V1 Go server or
Python HAL remains on the Lamp's required execution path at final cutover.

## 3. Work packages

### P0 — Contract baseline and parity ledger

**Outcome:** implementers share exact ownership and outcome vocabulary before
adding new services. Inventory coverage: all rows; `D01`, `D02`, `O02` directly.

- Pin V1 B/current H/V2 R revisions from the review index. Keep unmerged
  `lamp-release` behavior separate. Record later source changes explicitly.
- Declare the target hardware profile, minimum launch capability set and
  deferred capability list. The recommendation includes camera presence,
  useful safe motion, tasks and memory in the complete product; pilot scope
  may be smaller but must be named.
- Define observation, capability, action, task, decision and engine contracts.
  Include request/attempt IDs, clock domain, privacy/permission epochs,
  uncertainty, cancellation, origin/reply destination and question/answer IDs.
- Establish isolated simulator identities and test state directories. Translate
  useful V1 regressions into synthetic contract fixtures; do not copy private
  recordings, embeddings, keys or office-Mac backups into source control.
- Create one requirement ledger linking each inventory ID to implementation,
  migration rule, test evidence, target-hardware status and explicit deferral.

**Exit:** a sensor stub has two independent consumers, an action stub reports
admission separately from completion, and an engine stub survives a cancelled
utterance while its task continues. Existing control tests stay green. No
hardware action is involved.

### P1 — Provider-independent OS lifetime

**Outcome:** provider failure removes a capability without ending lampOS.
Coverage: `O01`, `O08`, `O09`, parts of `V04`, `V06`.

- Replace the terminal-service exit in the current coordinator with explicit
  route health/recovery after outstanding audio and receipt obligations settle.
  Preserve ordered terminal processing; do not discard accepted output metadata.
- Separate lifecycle supervision from the interaction loop. Worker start/stop
  must progress independently; a stuck camera cannot block speaker supervision.
  Use bounded per-worker recovery state and nonblocking exit checks.
- Enforce a worker-generation barrier across supervisor crashes. The outer
  service manager must account for all old descendants/device owners before
  admitting a replacement. Expired leases or a killed parent are insufficient.
- Classify session expiry, transient network failure, rate limits, quota,
  authentication and protocol incompatibility; bounded backoff belongs to the
  owning provider adapter. No nested uncontrolled retry loops.
- Provide truthful local status and diagnostics. The attended pilot service
  (`Restart=no`) is not silently converted into unattended production setup.

**Exit:** inject each terminal case, provider hang and supervisor/worker death.
Local stop/privacy and available light/control paths keep working; unavailable
voice never advertises readiness. No duplicate workers, stale output revival,
unbounded retries or unsafely repeated side effects. Reconnect returns through
fresh readiness. Also freeze useful worker progress while a heartbeat thread
continues, and freeze the control core while final writers still run: local
authority must expire. Physical measures remain a later authorized test.

### P2 — Local controls and common action/observation seams

**Outcome:** the Lamp remains directly useful and extensions share explicit
interfaces. Coverage: `B03`, `B05`–`B07`, `S13`, `A08`, `A09`, `O02`, `O04`,
`O05`, `D03`; foundation for all later packages.

- Port selected-board touch/button recognition and direct feedback, preserving
  hold/debounce/gesture cancellation. Keep privacy enforcement independent of
  model or graph health. An overflow cannot create a double-tap after the fact.
- Split useful illumination/volume settings from transient conversation cues.
  Define power/sleep/privacy state explicitly; current physical switch state
  overrides imported or cached permission. Preserve the selected board's
  declared combined switch policy (V1 Lamp disables mic/camera and mutes speaker),
  separately from a camera-only setting.
- Add versioned registration and bounded semantic pub/sub over current IPC.
  State topics support snapshot/resync; event topics expose gaps. Keep direct
  audio/camera/control paths. Deny unregistered or unauthorized subscriptions.
- Implement action admission/resources and typed receipts around existing
  writers. Stable action IDs, duplicate handling, partial-resource fallback and
  owner-scoped cancellation precede generalized agent tools.
- Add capability discovery and minimum authenticated management/config API.
  Config validation and durable version acknowledgment run outside control.
  Read-only diagnostic UI can adapt the existing web assets.

**Exit:** add a third observation consumer without changing a sensor driver;
stall/flood it and show control/audio progress and bounded memory. Test wrong
boot/privacy epochs, full queues, stale snapshots, ownership conflicts and
crash/restart. A spoken-turn change leaves the reading light setting intact.
Selected physical gestures require separate device latency/UX qualification.
Freeze the privacy producer and require loss of input/disclosure authority.
Stall driver I/O/teardown separately from crashing a process; verify quarantine
without duplicate writers and continued unrelated capabilities.

Before physical-control implementation, set per-profile electrical-edge ->
userspace-observation -> enforced revocation/feedback targets. The initial
profiling proposal is observation-to-relevant-software-revocation within 10 ms
on a healthy scheduled path, separate from polling/debounce, lease-expiry
containment and acoustic effects. Measure the electrical edge and accepted
capture/upload boundaries too; a short downstream metric cannot hide a slow
switch detector. If privacy depends on software, do not describe it as an
electrically independent cutoff.

### P3 — Useful voice under Live failure

**Outcome:** admitted conversation uses Gemini Live normally and the agreed
Deepgram -> answer model/engine -> Gemini TTS fallback when available.
Coverage: `V01`–`V06`, `B09` volume/mute, minimum `A01` adapter.

- Add a Rust Deepgram adapter using the existing V1 protocol knowledge, model
  options, pre-roll and reconnect behavior. Keep partial/final transcript IDs;
  a duplicate final cannot create a second turn or task.
- Add Gemini TTS through an explicit transport profile for the deployed relay
  or direct provider. V1's proxy/headers are not assumed interchangeable with
  Google's direct endpoint. Pin the configured voice/model and sample format.
- Connect the smallest useful remote Hermes turn adapter or direct answer-model
  adapter through the P0 interface. P4 supplies full durable task operation;
  P3 must advertise that distinction instead of calling every utterance a
  recoverable long-running task. Enforce answer-only/read-only capabilities
  until then, permitting only explicitly supported P2 local device actions.
  Native engine tools cannot perform unjournaled external work; attempted
  unsupported effects are denied before execution.
- Keep one local speaker/echo-reference path and route state. Switch only at a
  safe ownership boundary, retiring the old route-attempt/output generation.
  Late PCM, transcript and new tool proposals from that route are rejected;
  receipts for already admitted tasks still reconcile under their own IDs.
  Bound local drain of accepted speaker obligations without waiting for a dead
  provider's terminal marker. A mid-answer error records truncated output;
  no replay of uncertain tool effects or previously heard text.
- Implement the Gemini tool request/response/cancel wire messages and translate
  to P2 actions or P4 tasks when available. Unsupported tool capabilities are
  explicit; they must not crash parsing or masquerade as completed operations.
- Before P5 qualifies broader attention, P3 uses explicit physical activation
  and a bounded input window as its local pre-upload admission. Do not inherit
  V1's cloud-first wake decision implicitly. Qualify voice selection and prosody
  continuity; a shared voice name is not proof of identical acoustics.

**Exit:** force Live failure before speech, mid-input and mid-output. Test
healthy fallback, failed STT, failed engine, failed TTS and shared Gemini quota.
Inject old-route PCM/tools after replacement and a provider that never sends
its terminal marker. Verify acoustic output/cancellation and no false ready cue.
Reject an attempted external tool effect from the restricted early engine.
Where all cloud routes fail, local operation survives with honest status. Measure Live and
fallback separately on first and repeated turns; do not count filler as latency.

### P4 — Durable agency, conversation continuity and memory

**Outcome:** real computer/robot tasks continue across spoken turns and recover
without duplicate effects. Coverage: `A01`–`A08`, `D04`, minimum `I01`/`I02`.

- Build the OS task controller and local durable journal/outbox. Persist task
  origin, authenticated reply destination, actor, permissions, deadlines,
  idempotency key, engine/run mapping and uncertain effect state before durable
  acceptance. Reconcile disconnected work; never turn “unknown” into a retry.
- Qualify remote Hermes first. Audit its V1 patches against the pinned upstream
  engine, native tools/permissions, progress, steering and recovery. Do not
  assume the locally installed V1 Jev plugin exists on the remote engine.
- Migrate shipped and user-authored skills/assets with a compatibility report.
  Replace Lamp-loopback and local-file assumptions with scoped remote tools;
  keep native skill permissions and explicit user selection. Pin engine,
  adapter, patches and tool-schema versions in an engine-host package with
  capability negotiation and upgrade/rollback support.
- Implement correlated questions/answers and approval requests, including
  current task/question versions, cancellation and focus. A bare “yes” cannot
  authorize an unrelated queued question. The original channel response stays
  on its channel unless room speech was explicitly part of the task policy.
- Provide selected context updates without starting model turns, bounded
  OS-owned conversation journal/summaries and silent external-history updates.
  Persist projection cursors and actual delivery status; share versioned
  persona/language policy across routes. Late results retain attribution while
  a new user turn owns room speech.
- Introduce canonical memory/preferences/corrections through an OS API; bridge
  native engine memory deliberately. Canonical writes require OS revision/commit
  receipts; disable, redirect or explicitly reconcile competing native personal
  memory writes. Keep working transcripts/compaction engine-specific and
  preserve useful imported facts with their provenance.
- Preserve the scoped per-person history API for selected producers, including
  imported mood/wellbeing/posture/music records with original time, source and
  uncertainty. These records are distinct from curated personal memory and
  conversation text. Habit-pattern learning and unsolicited nudges remain
  deferred; importing history does not enable an unqualified producer.
- Port explicit reminders with stable occurrence IDs, timezone/DST policy and
  admission/execution/delivery states. Preserve backend-confirmed definition
  CRUD and one execution owner per revision; offline edits are pending until
  confirmed. A local-only schedule mode requires a separate explicit contract.
  Keep broad autonomous routines optional.
- Qualify a second engine using the same contract fixtures and workload before
  calling the architecture engine-independent. Choose Pi or the existing Codex
  protocol based on the measured integration experiment; adapter support must
  be published per operation, not inferred from its name.

**Exit:** submit/steer/question/cancel/reconnect/reboot through two engines and
a deterministic stub. Verify “the second option” across voice-route changes,
remember/correct/forget across engines, pending-question resumption and stable
private-channel destination. Test guidance racing completion, lost acknowledgment,
accepted-but-unconsumed/expired guidance and wrong-owner cancellation. Qualify
skill compatibility, unsupported engine versions and package rollback. A completed
external effect followed by a lost response is reconciled, not repeated. New
speech cancels old speech but not unrelated work. User cancellation stops the
target task locally and reports remote cancellation uncertainty truthfully.

### P5 — Independent senses, situation and qualified attention

**Outcome:** awareness is shared without forced model turns or outward behavior.
Coverage: `S01`–`S05`, `S09`–`S11`, `S13`, `V02`; selected `I06`.

- Port selected-board camera presence/local detection and configured
  environment drivers. Preserve current H's local-only person detection rule:
  a failed local model does not authorize a cloud fallback.
- Separate sound perception scheduling from camera activity. Resolve shared mic
  ownership explicitly rather than opening the same device from competing
  workers. Media access is scoped, revocable and has bounded retention.
- Build current situation projections: per-field provenance/age, uncertainty,
  partial joins, coordinate frame/calibration, conflicting identity and absent
  sources. Agent context sees meaningful transitions and snapshots, not a model
  invocation for each frame.
- Port enrollment/identity only behind private identity APIs. Test face/voice
  disagreement, visitors, leave/re-enter and unknown speaker. Validate imported
  model/preprocessing versions; incompatible embeddings stay quarantined.
- Qualify the local attention gate: explicit controls/wake first, then bounded
  follow-up focus. Preserve original input candidate deadlines and retained
  opening audio. Semantic rejection is secondary to upload admission.
- Typed look/read/search uses scoped current frames and returns no-target,
  unavailable or expired evidence honestly. Motion-dependent search waits for
  P6; a software image test cannot qualify active movement.

**Exit:** camera-off leaves sound/environment independent; absent sensors stay
unknown; visitor/call/TV/multi-person scenarios measure unwanted responses and
missed intended turns. No unadmitted cloud upload, stale identity assignment or
stale target movement in fixtures. Physical perception/attention accuracy needs
authorized, consented recordings on the selected device.

### P6 — Useful body behavior and coherent expression

**Outcome:** Lamp feels natural while staying physically useful and controlled.
Coverage: `B01`–`B04`, `B08`, selected `B09`, `S11` motion-dependent actions.

- Carry the latest motor clearance/measured-stop work forward intact. Qualify
  each supported unit/profile/placement before enabling motion. No factory-wide
  safe cell is inferred from one unit, URDF or V1 trajectory.
- Port named holds, overload/contact policy and selected V1 gestures/tracking
  through typed action requests. Keep ordinary motion authority separate from
  the narrow measured-stop sequence; do not silently add torque/EEPROM writes.
- Add behavior state machines and resource-aware expression. Touch response,
  speaker orientation, small presence acknowledgment and settling are first
  candidates. Stillness/silence is a valid outcome, and unsuccessful unsolicited
  search remains brief.
- Coordinate light/gaze with actual playback/action events. User reading-light
  settings survive transient cues. Music/duck/resume is an explicit action policy
  if offered; Bluetooth remains deferred unless selected for launch.

**Exit:** first simulated conflicts/cancellation/load faults; then authorized
physical clearance, stopping and naturalness trials. Inspect acoustic/visual/
motion recordings and comparable timing, not just goal coordinates. No behavior
is complete if it passes code tests but looks mechanical or repeatedly distracts
a person. Publish the exact qualified motion scope, not “all motion safe.”

### P7 — Pluggable decision models

**Outcome:** Jev and alternatives can help eligible behaviors/tasks without
becoming a hard dependency. Coverage: `A09`, `A10`, Jev portions of `I03`, `I04`.

- Implement `DecisionProvider` separately from `AgentEngine`. Version the
  decision specification, context projection, candidate set, purpose and model.
  Preserve native distributions and distinguish them from calibrated accuracy.
- Keep V1 BFF `/jev/decisions` and direct TypeSafe `/v1/systemone` as different
  compatibility profiles. Do not silently migrate endpoint/auth/model naming.
- Preserve per-purpose local eligibility, none/abstention, native skill
  permissions, voice bypass and current-state revalidation. Replace stale
  Hermes ambient-cache keys with bounded revision/expiry-aware entries.
- Start with advisory skill/context hints that run alongside preparation and
  do not delay the engine. Test against the unchanged rule/engine baseline.
  Use separate policies for intent, Harness targeting and Buddy suggestions;
  success in one does not validate the others.
- Enforce absolute request deadlines, concurrency/input/spend limits and local
  late-result suppression. SDK retries do not receive a second unrestricted
  budget. Provider cancellation may leave server computation/cost unknown.

**Exit:** fake providers prove swap/failure/expiry semantics; labeled Lamp cases
measure selective accuracy, abstention, regression, latency and cost. Permute
candidate order, change eligibility mid-flight and include irrelevant/adversarial
state. Qualify Jev plus one independent local or remote decision implementation
against the same permitted evidence and rules/no-decision baselines. Test absent
distributions and unsupported question types; do not invent probabilities.
An unavailable decision service does not take down conversation or local
behavior. No physical safety rule waits for a model. If no second implementation
is qualified, label the interface experimental rather than proven portable.

### P8 — Integrations and operator experience

**Outcome:** useful V1 external work is available through the new boundaries.
Coverage: `I01`–`I06`, `D03`, relevant `A02`, `A03`, `A05`.

- Preserve web/MQTT/message semantics, attachment/media authentication and
  reply routing. Gateway-owned remote replies stay silent in the room by
  default. Device commands use the same local action admission as voice.
- Port the OS side of Harness's current paired connection, capability discovery,
  tasks/questions/results and durable reservation/cursor handling. Verify the
  current owning team's contract/revision; changes require coordination rather
  than invented transport or credential fallbacks. Retain result-target identity
  through retries even when a new Jev suggestion prefers another agent.
- Port Buddy independently, preserving its pairing, stale UI snapshot checks,
  typed refs and advisory Jev behavior. Do not combine Buddy and Harness trust.
- Wrap selected MCP/connectors and current post-baseline FO auth/refresh flows.
  Remote data/tools keep their existing permission and host boundaries.
- Reuse useful setup/control/Flow Monitor UI with the Rust API. Expose capability
  and degraded status in plain language; ordinary use should not require
  configuring channels, queues or model confidence.

**Exit:** provider-specific fixtures plus interoperable tests against pinned
partner builds using isolated test state. Duplicate/reordered results, expiry,
reconnect, wrong peer, revocation, cancelled speech and missing capabilities
behave correctly. No test enters commands into the user's real Harness sessions.

### P9 — Complete OS operations and private-state migration

**Outcome:** V2 can be installed, recovered, updated and reset as a product.
Coverage: `O01`–`O10`, `D01`, `D03`, `D04`, all private-state inventory rows.

- Complete board manifests, boot readiness, first setup, Wi-Fi recovery,
  discovery/time, device identity and authenticated APIs. Preserve the current
  profile distinction between Standard and Pro sensors.
- Implement versioned atomic config and the durable storage service. Bound
  database/trace/disk use; disk-full or corrupt state cannot block local controls.
  Secrets and identity/biometric assets have separate scopes from general logs.
- Ship compatible signed/verified runtime bundles, explicit migrations, health
  checks and a last-known-good recovery path. Validate update compatibility for
  parent/worker contracts, calibration and task-store schemas. A failed update
  must not silently downgrade security or replay physical output.
- Implement the private migration manifest in the inventory. Begin with a
  redacted dry-run on synthetic fixtures; real data requires authorized access.
  Preserve source copies, IDs, consent, uncertainty, unresolved tasks and trust
  ownership. Do not import volatile actuator authority.
- Implement export/forget/reset across configuration, credentials, personal
  memory, engine caches, pairings, result stores and derived assets. Preserve
  unit qualification separately; factory reset must not install guessed motion
  settings. Offline paired hosts report pending deletion rather than false
  global completion.
- Make the support workflow reproducible: exact binary/source/model/profile
  versions, trace IDs, bounded support bundles and secret/content redaction.

**Exit:** interrupted setup, network changes, bad config, power-loss simulation,
disk pressure, failed update, migration re-run, downgrade incompatibility,
reset/forget and restore are exercised. Original data remains recoverable;
rollback cannot duplicate unfinished tasks or enable stale/unqualified motion.

### P10 — Comparative release qualification and cutover

**Outcome:** evidence supports replacing V1 for an explicitly declared scope.
Coverage: every L row and every selected C row in the inventory. D rows remain
visible with a migration/disposition record.

- Run the scenario matrix below with pinned builds, same board/profile,
  comparable provider/model/network conditions and stated sample sizes. Record
  first-use and warm behavior, failed trials, p50/p95/p99 where appropriate,
  acoustic/visual boundaries, errors, cost and resource use.
- Exercise CPU/memory/thermal/USB/serial contention and record maximum observed
  dispatch gaps, deadline-miss counts, queue high-water marks and page faults.
  Percentiles alone can hide rare long stalls; a finite observed maximum is
  not a proved worst-case bound.
- Finish an attended device pilot, then a minimum 24-hour soak including
  injected outages and planned cold restarts. Extend duration until the
  observed failure/recovery cases are understood; elapsed uptime alone passes
  no scenario.
- Enable only qualified capabilities. Do not mark the complete robotic OS done
  because a stationary voice pilot works. A narrower launch requires an explicit
  product scope decision and truthful UI/manifest.
- Perform an authorized per-unit cutover using the procedure in section 7.
  Track parity regressions and rollback triggers. Keep V1 artifacts and source
  data until the new installation and recovery path are validated.

**Exit:** all selected capability gates have evidence, critical failures are
resolved, deferred scope is explicit and both cutover and rollback are tested.
The architecture review itself does not satisfy this release gate.

## 4. Rust ownership and implementation boundaries

Use these as module responsibilities; create a separate crate only when its
dependency or testing boundary warrants one. Avoid a repository-wide rename
before the first useful slice.

| Owner/module group | Source to study | New or migrated responsibility |
| --- | --- | --- |
| Interaction + actions | Existing `interaction`, V1 holds/dispatch/safety/marker parsing | Keep pure ownership; add typed action admission/resource arbitration. Retire text markers as the authoritative hardware interface. |
| Supervisor/runtime | Existing `live` worker supervisors, V1 healthwatch/bootstrap/systemd | Per-worker lifecycle, generation containment, capability readiness and provider-independent root lifetime. |
| Graph + context | Existing observation/environment/camera cores, V1 observer/app_state/sensing paths | Authorized pub/sub, snapshots, context projections, provenance and bounded semantic event delivery. |
| Companion | V1 voice gates, physical controls, ambient and expression policies | Attention, behavior state machines, voice route policy and restrained initiative. Device writers remain separate. |
| Speech/provider adapters | Existing `gemini` plus V1 Deepgram/Gemini TTS/relay protocol code | Live/STT/TTS transport, tool wire mapping, per-route health, budget and cancellation semantics. |
| Agent/task service | V1 `AgentGateway`, `runtimes/*`, schedule/Harness journals | Neutral engine contract, durable goals, questions, steering, delivery and recovery. Keep engine-host packaging separate from device setup. |
| Decision adapters | V1 `system/intent/jev`, `jevskills`, Hermes plugin, Buddy selector | Optional versioned judgments; shared transport/validation plus caller-specific policy. |
| Private state | V1 config/person logs/memory/schedules/identity/trust | Atomic config, canonical memory, task journal, migration/export/deletion manifest and scoped resources. |
| Management/integrations | V1 server/API/web/MQTT/Harness/Buddy/FO clients | Rust API and client implementations preserving external contracts; UI assets can remain web code. |
| Device profiles/perception | V1 HAL/robot declarations, current Rust workers | Port selected drivers and bounded perception adapters; keep units, calibration and deployment scope explicit. |
| Diagnostics/evaluation | V1 Flow Monitor and current observer/voice-eval | Correlated traces, graph inspection, replay/fault injection and comparable physical evaluation. |

The migration inventory supplies precise source anchors for each capability.
Port protocols, useful rules and behavioral regression cases; do not blindly
translate thread layouts, undocumented shared state, unbounded queues or
configuration-derived permission bypasses into Rust.

## 5. Qualification matrix

Every row requires normal operation plus the stated fault. Host fixtures prove
software semantics; target-device trials prove the user-visible physical path.

| Scenario | Required result / evidence |
| --- | --- |
| Live quota exhausted | OS remains alive, route marked unavailable, fallback works when its entire dependency chain is healthy; no retry storm or stale playback. |
| Old voice route emits after replacement or never terminates | Retired generation cannot admit PCM/transcripts/tools; bounded local settlement, with already admitted task receipts still reconciled. |
| Both Gemini endpoints unavailable | Local controls/light/privacy still work; no false ready state, independent STT availability does not imply a complete voice route. |
| User interrupts and continues a long task | Old speech/expression stops within qualified limits; new turn preserves opening words; unrelated task survives. |
| A camera, model or subscriber stalls | Freshness becomes unknown/stale; independent audio/controls continue; bounded memory and explicit drops. |
| Supervisor dies with a device worker alive | No replacement writer until the previous generation is accounted for; device quarantined if teardown remains uncertain. |
| Core/privacy worker stops progressing while processes remain alive | Local grants expire and input/disclosure close according to their contracts; a separate healthy heartbeat does not mask the frozen useful path. |
| ALSA/serial operation or teardown stalls | Report elapsed overrun/unknown physical state; quarantine unreleased device ownership, without blocking unrelated worker recovery. |
| Stop during slow/failed motion | Ordinary output is inhibited; measured stop/refusal evidence remains truthful; never infer physical rest from process exit. |
| Two behaviors request the same body resource | Deterministic ownership/preemption; no jittering competition or deadlock; stable useful lighting survives conversation changes. |
| Visitor, TV or phone call | Count false admissions/uploads/responses and missed addressed turns separately. Ambiguous identity remains unresolved. |
| User says “yes” with multiple pending questions | Resolve only the current correlated question under valid focus/authority; clarify ambiguity rather than grant a different request. |
| Remote task submitted, connection lost | Reconcile stable task/effect ID; no duplicate effect and no false cancellation or completion. |
| Task from a messaging channel completes | Reply uses its durable destination; no automatic room speech or new target after reconnect. |
| Memory corrected, engine changed | Current authorized preference/correction reaches the new engine; stale native caches do not restore the old fact. |
| Live answer followed by “the second option” on fallback | Scoped conversation journal preserves referents and actual delivery status without replaying the earlier task or leaking private channel history. |
| Identity enrollment model changed | Incompatible embeddings are detected/quarantined; no silent identity merge or cloud re-enrollment. |
| Jev absent, late or inconsistent | Caller follows explicit fallback; late choice cannot execute; candidate/model/context provenance retained. |
| Harness/Buddy peer changes or lacks capability | Existing trust and negotiated capability contracts hold; no invented fallback or direct host/device bypass. |
| Reminder across timezone change/reboot | Defined occurrence semantics, no duplicate action and distinct execution versus delivery evidence. |
| Storage full, logging slow or update interrupted | Bounded degradation; local control remains responsive; durable acceptance and rollback stay truthful. |
| Privacy toggled during upload/queued result | Prevent new disallowed capture/disclosure and revoke derived queued work; disclose that already transmitted bytes cannot be retracted. |
| Physical switch edge under contention | Measure edge, observation, software revocation, accepted capture/upload, speaker queue and acoustic effects separately; apply the declared board's combined privacy policy. |
| Forget/reset with paired host offline | Local deletion and outstanding remote/cache work tracked separately; no false global completion or lost unit calibration. |

Measure the architecture's latency targets from their named boundaries:
control acceptance p99 <=5 ms, visible ring response p95 <=50 ms, admitted
interruption to acoustic silence p95 <=200 ms, and end-of-user-speech to useful
answer p50 <=2 s / p95 <=4 s. These are proposed acceptance targets inherited
from the V2 design, not measurements produced here. Measure fallback separately.
Report false interruption/admission, missed turns, missing first words, task
success and recovery cost alongside speed. Do not tune latency by dropping
valid input, hiding failed trials or playing an empty acknowledgment.

No numeric “10x” claim is accepted without a named metric, matched baseline and
repeatable workload. The initial product goal is to preserve useful V1 agency
and surpass its dependability and interaction quality, with measured evidence.

## 6. Initial scope and deliberate deferrals

| Recommended complete-product scope | Explicit later/conditional scope |
| --- | --- |
| Reliable Live and fallback voice, local privacy/touch, useful illumination, remote agent tasks, memory/corrections, explicit reminders, operator recovery/setup/update | Additional voice vendors or every legacy engine adapter |
| Camera presence and qualified identity, useful scoped vision, body feedback and selected natural safe motion | Unqualified gesture libraries, broad autonomous room scanning, generic robot locomotion |
| Installed board's environmental capabilities with explicit unavailable states | Advertising Pro hardware on Standard; every possible sensor driver |
| Harness/selected integrations required by actual users, typed device tools | Bluetooth, optional messaging/connectors unless included in declared launch scope |
| Small restrained local behavior and optional measured decision assistance | Emotion/wellness/guard claims, broad unsolicited self-talk, learned routines until outcome-qualified |

Explicit producer deferrals: facial-emotion (`S06`), speech-emotion (`S07`),
cloud activity/posture observations (`S08`) and guard/visual-hazard detection
(`S12`) are off in the initial default profile. This defers the underlying
producers as well as their nudges/claims. Preserve compatible private history
and settings inertly. If a launch profile selects any of them, implement its
bounded P5/P8 adapter and pass the inventory criterion before enabling it.

These are recommendations for a coherent product, not a claim the owner has
approved every sense or motion feature for the first release. Deferred V1 state
must remain importable or have an explicit disposition; deletion is not the
default migration strategy.

## 7. Cutover and rollback procedure to implement

1. Produce a redacted inventory of the authorized unit, effective config,
   calibration, credentials by reference, active work and paired identities.
   Verify backups and restore on synthetic data first. Never overwrite the only
   original or copy private assets into Git.
2. Install V2 in an inactive versioned location with a distinct test identity.
   Run offline fixture/shadow replay without physical outputs. Do not start V1
   and V2 against the same devices, MQTT identity or task endpoints concurrently.
3. Quiesce V1 task submission and reconcile external work/results. Preserve
   unknown effects as unknown. Stop V1 and prove all device owners have exited.
   A timed-out stop is not permission to open the motor bus again.
4. Import validated state transactionally, preserving stable IDs and scope.
   Incompatible biometric/model data is quarantined. No prior actuator grant,
   transient cue or pending raw movement is imported.
5. Start V2 with physical privacy respected and motion disabled. Verify core
   readiness, configuration, useful light and task/history inspection. Enable
   each qualified capability deliberately, then run the attended acceptance
   scenarios on the intended artifact/build.
6. If a rollback gate fails, stop V2 writers and reconcile new external work
   and prove the old worker generation/device ownership released before
   re-enabling V1. Quarantine unresolved ownership in either direction. Never
   blindly reverse-copy a newer task store into an
   older schema. The rollback plan must carry forward acknowledged user state
   and unresolved effects or explicitly block the unsafe downgrade.
7. Preserve the old image/config until recovery is demonstrated. Returning to
   V1 must not trigger its legacy startup motion or bypass the current unit's
   safety/privacy state; qualify a motion-disabled rollback profile beforehand.

For a motor handover in either direction, verify the qualified stop/hold outcome
as well as worker exit. A terminated process does not retract an already
accepted servo goal. If physical state remains unknown, neither runtime may
resume ordinary motion; follow the unit's authorized recovery procedure.

Deployments, SSH, physical experiments, external integration mutation and real
private-data migration are separate future authorized work. This review has
performed none of them.

## 8. Checks required during implementation

Run focused contract/fault tests per slice and all affected crate checks before
handoff. Exercise OS-specific code on Linux/ARM64; macOS host success cannot
validate ALSA, V4L2, GPIO, kernel scheduling or servos. Keep source/config/model
versions and failed trials in the evidence record.

If a migration slice changes Go code, follow this repository's sequential gates:
`go build ./...`, then `go vet ./...`, then `go test ./...`; affected concurrency
also needs focused race tests. Any `system/web/` change requires `npm run lint`
and `npm run build` in that directory. Behavior/API changes update their English
and Vietnamese product docs according to the repository rules. This review's
English design documents do not imply an implementation or completed code gates.

No runtime code changed during this architecture review. Its actual source,
documentation and narrow host-test verification are recorded in the
[review index](v2-review-index.md), separately from all the future gates above.
