# V2 architecture review

**Start with the latest [HTML architecture draft](v2-brain-design.html),
13 October 2026, pending owner review and approval.** Download the HTML and open
it in a browser for the diagrams and interactive walkthrough. It updates the
executive/decision-model role, the topics-and-services vocabulary, and the build
sequence. The earlier synthesis below remains research history where its
recommendations differ. The owner authorized committing and pushing these
design documents for remote review; that does not approve runtime implementation.

Status: design review complete, 12 October 2026. The deliverables are a recommended
architecture and a Rust implementation/migration plan, with source research and
qualification gates. This is design work: no runtime changes, installation,
deployment or commits. Completion here does not mean V2 is implemented or ready
to replace a physical V1 installation.

## Deliverables and completion criteria

The final review must recommend an architecture for the complete Lamp brain
and robotic OS, not merely the next voice fix. It must explain how to build it
incrementally while retaining useful V1 behavior and the existing V2 work.

| Requirement | Evidence needed before completing this review | State |
| --- | --- | --- |
| Review every element of the proposed architecture | Component responsibilities, placement, contracts, lifecycle, failure behavior and Rust ownership | Complete: architecture sections 1–13 and build ownership table |
| Study V1 strengths and weaknesses | Source-pinned inventory, corrections, migration decisions and behavioral acceptance criteria | Complete: corrected audit plus 58-capability inventory |
| Study ROS closely | Primary-source review of topics/services/actions, QoS, execution, lifecycle, timing, transforms, resource ownership, tooling and production lessons | Complete: ROS study and architecture synthesis |
| Study Jev closely | API/model limitations, confidence semantics, pluggable provider contract, appropriate use and evaluation plan | Complete: decision study, including existing V1 paths and separate BFF/direct-provider profiles |
| Study other real-time operating systems | Primary-source comparison and concrete scheduling, isolation, memory, watchdog and hardware implications for Lamp | Complete: Linux PREEMPT_RT, Zephyr, FreeRTOS, seL4, RTIC and Embassy study |
| Recommend the final V2 architecture | Coherent ASCII diagrams, decisions with rationale, explicit alternatives and unresolved hardware/provider qualifications | Complete: recommended architecture; measurements clearly remain future work |
| Plan the Rust build and migration | Dependency-ordered work packages, source-to-destination matrix, validation gates, setup/data migration and cutover/rollback | Complete: P0–P10, all inventory IDs mapped |
| Verify the recommendation | Cross-document consistency, source anchors, scenario walkthroughs and requirement-by-requirement completion audit | Complete for this design review: source spot-checks, independent reviews/closures, document validation and narrow existing-core tests below |

## Owner direction carried into the review

- Build an AI-native robotic OS, with Lamp as the first companion product.
- Learn useful ROS patterns and implement them within lampOS: sensor/observation
  channels, services and clear action contracts.
- Keep multiple independent consumers of selected observations; receiving
  evidence must not automatically cause speech, movement or a model turn.
- Support pluggable decision models such as Jev and pluggable agent engines
  such as Hermes, Pi, Codex and Claude. These are separate interfaces.
- Use Gemini's voice. At Live unavailability, the agreed direction is
  Deepgram STT -> answer model/agent -> Gemini TTS.
- Preserve agentic work, useful memory and restrained initiative. Agent hosting
  may be off-Lamp. Ordinary conversation and local controls must remain useful
  without the remote agent.
- Lamp-owned runtime code is Rust; native model/DSP libraries require explicit
  reviewed boundaries. No permanent dependency on V1 Python HAL or Go server.
- Design first. Make reasonable recommendations while the owner is away.
  Do not access the private 4ace backup, deploy, commit or push.

## Authoritative source snapshots

| Snapshot | Revision | Meaning |
| --- | --- | --- |
| V1 behavioral baseline | `170a1b85e7e6197097019e4175efdcbe416e3c2c` | Original main-branch baseline; excludes unmerged release-branch features. |
| Current V1 repository | `b8c23cc9916de3f074c34b8689134437d2ebb4b6` | Current worktree HEAD and local origin/main when review began; later fixes are recorded separately. |
| Earlier V2 source | `10607a43963ca1a1e7c1b0bfda024a9ecb0b99f9` | Basis of the earlier discussion and audits. |
| Current V2 integration source | `e1eae8de5ad65c1ec6c6450948afbcb510982240` | Local origin/lamp-v2-chat at review start; source retrieved and inspected separately from this worktree. |
| Earlier brain design/handoff | `1e179f9944c550390dbecd00057fe39d1a561ab3` | lamp-a2-brain; historical proposal, superseded where the current owner discussion changes it. |

The current checkout is `commit-push`, not the V2 branch. Design documents
are saved under `lampOS/docs/`; no branch was switched or merged.
Read pinned source with `git show <revision>:<path>`. Source-only temporary
exports aid inspection but are not deliverables or runtime installations.

Current V2 differs from the earlier source in 26 paths, concentrated on motor
clearance, measured stopping, worker contracts and their tests/docs. Other V2
source paths retain identical Git objects. Initial missing promisor objects
were retrieved successfully through the repository's existing remote; there is
no remaining source-access limitation for this snapshot.

The existence of code, a test or a verification report does not prove the whole
device is qualified. Reports must be read for their exact setup and limits.
No new acoustic, motion, network or on-device measurements are authorized here.

## Reading order

1. [Recommended architecture](v2-architecture.md): ASCII diagrams, responsibility
   boundaries, chosen defaults, alternatives and failure semantics.
2. [Build and migration plan](v2-build-plan.md): P0–P10, qualification matrix,
   Rust ownership and reversible cutover.
3. [V1 migration inventory](brain-research/v1-migration-inventory.md): all 58
   capability groups, source anchors, acceptance and private-state implications.
4. [V1 brain audit](v1-brain-audit.md): corrected behavioral baseline and mapping.
5. [ROS study](brain-research/ros2-design-study.md): transferable contracts and
   tooling rather than a native ROS adoption proposal.
6. [Decision models/Jev](brain-research/decision-models-study.md): API, uncertainty,
   existing V1 uses, versioned interface and empirical qualification.
7. [Real-time systems](brain-research/realtime-systems-study.md): scheduling,
   isolation, memory, lifecycle and physical limits.
8. [Agent engines](brain-research/agent-engines.md): remote Hermes continuity and
   the second-engine qualification required to prove independence.

## Recommended decisions

Continue V2 as an incremental standalone Rust replacement on the supported Linux
base. Keep the existing ownership/audio/IPC/camera/ring/environment/motor cores.
Separate lifecycle supervision, deterministic local control and ordinary
application services. Build ROS-like observation channels, bounded services and
explicit actions/tasks without routing every byte or decision through one bus.

DecisionProvider and AgentEngine are separate replaceable interfaces. Start
with remote Hermes for V1 continuity, qualify a second engine, and treat Jev as
optional caller-scoped assistance. V1 already contains Jev, sensor-driven agency,
memory and durable integration records; these are capabilities to preserve.

Keep the OS alive when Live fails. Add Deepgram -> answer model/agent -> Gemini
TTS through the same local speaker/control boundaries, with explicit shared-quota
failure behavior. The first implementation slice is provider-independent
lifetime; useful fallback is the next acceptance layer.

## Coverage cross-check

All 58 distinct inventory IDs were mapped in an independent review and checked
again after the plan corrections. Conditional/deferred scope is explicit.

| Inventory | Plan coverage |
| --- | --- |
| V01–V06 | P3; attention also P5; lifetime/interruption also P1 |
| S01–S05, S09–S10 | P5 |
| S06–S08, S12 | Explicit initial producer deferrals; selected future adapters require P5/P8 gates |
| S11 | P5/P6 |
| S13 | P2/P5 |
| B01–B04, B08 | P6; useful light B03 also P2 |
| B05–B07 | P2; complete reset P9 |
| B09 | P3 volume/mute, selected P6 music, explicit Bluetooth deferral |
| A01–A08 | P4; action/intents P2 and delivery/integrations P8; scoped history and skill migration explicit |
| A09–A10 | P2 deterministic intent, optional P7 decision assistance |
| I01–I06 | P8; selected perception also P5 and optional Jev also P7 |
| O01–O10 | P9, with lifecycle/provider recovery P1 and config/auth/profile foundations P0/P2 |
| D01–D04 | P0 fixtures, P2/P8/P9 operator support, P4 engine packaging, P10 qualification |

## Findings resolved during design review

Three independent source-grounded review scopes checked the draft architecture
and plan. Their fixes were incorporated and closure checked; these are design
resolutions, not claims of runtime fixes.

| Finding | Resulting contract or plan change |
| --- | --- |
| Supervisor restart could lose knowledge of old device owners | Outer worker-generation teardown barrier; per-worker nonblocking recovery; quarantine unresolved ownership. |
| Late Live PCM/tools could share a turn with fallback output | Separate route-attempt/output generation, bounded local drain and preserved already-admitted task receipts. |
| Pending questions could be mistaken for new conversation | Correlated awaiting-input/answer state, focus/expiry/respondent checks and separate permission requests. |
| An idle room could receive a private channel's task result | Durable origin/reply destination/disclosure scope; room eligibility precedes attention. |
| Facts/task receipts did not preserve conversational referents | Bounded OS conversation journal, actual delivery status, projection cursors and versioned persona. |
| Remote engine memory could diverge from canonical memory | OS memory revision/commit receipts; redirect or reconcile native personal-memory writes; correction/forget tests. |
| SQLite adoption could accidentally change schedule authority | Preserve backend-confirmed definition CRUD, pending offline edits and one execution owner per revision. |
| Camera-only controls could erase V1 physical privacy behavior | Preserve board profile's combined mic/camera/speaker switch policy separately. |
| Early fallback could upload before attention or execute unjournaled tools | Explicit activation before P5; restricted answer/read-only early engine before P4. |
| Optional Jev appeared on the required release path | P7 is a side branch; qualify an independent decision implementation before claiming portability. |
| Sensor history and skill/package migration were underspecified | Explicit producer deferrals, scoped attributed histories and engine-host skill/version/rollback deliverables. |
| Generic crash/latency tests missed live hangs and input delay | Frozen progress/privacy/core, stalled driver, electrical-edge, contention, maximum-gap and deadline-miss tests. |
| Rollback needed the same ownership and physical-state proof as upgrade | Bidirectional exit/quarantine plus qualified motor stop/hold evidence before handover. |

The scenario matrix covers normal/failure transitions for quota, privacy,
interruption, channel routing, identity, questions, task uncertainty, memory,
decision providers, resource conflicts, storage, updates and forgetting. No
scenario relies on a model asserting that an external effect completed.

## Verification performed

- Read source at the pinned revisions. The inventory checked 302 explicit and
  shorthand anchors across 164 source files, then the main review spot-checked
  decisive paths: provider-terminal exit, Gemini tool rejection, motor ownership,
  schedule authority/outcomes, physical privacy, external history and questions.
- Checked primary upstream documentation/source for the ROS, Jev, engine and
  real-time studies. Vendor measurements remain labeled as such; no engine or
  model was installed, queried or benchmarked for this review.
- Checked all nine documents for existing local links, balanced code fences,
  table shape, trailing whitespace and unique inventory IDs. Earlier studies
  now point to the synthesis and explicitly include V1's existing Jev work.
- Ran the existing current-V2 ownership/observation/environment/IPC host tests
  from a source-only export at `e1eae8de5ad65c1ec6c6450948afbcb510982240`:

  ```sh
  /Users/ab/.cargo/bin/cargo test --locked --offline -p lamp-interaction -p lamp-ipc -p lamp-environment
  ```

  Rust `1.96.0`, host `aarch64-apple-darwin`. The initial sandboxed run passed
  environment/interaction tests but failed two IPC probe tests because temporary
  Unix socket creation returned `Operation not permitted`. The same command was
  rerun with approved sandbox escalation: **66 tests passed** (12 environment,
  37 interaction, 16 IPC integration and one IPC doc test), final exit code 0.
  The final completion output was inspected. No runtime code was modified.

This was a narrow host verification of retained foundations, not the whole Rust
workspace, a Linux/ARM64 device test, an acoustic/motion measurement or an
implementation of the proposed architecture. Go/web code gates do not apply to
these documentation-only edits. Existing source's hardware reports retain their
own qualification boundaries.

## Remaining engineering and product decisions

The build plan owns all implementation work and remaining experiments: exact
board/profile inventory, physical clearance and stopping, privacy/input latency,
natural addressing, fallback quota/voice behavior, engine/model quality,
resource budgets, private-state migration and partner interoperability.

Default deployment has no agent runtime on the Lamp. Complete-product scope
recommends camera presence, selected safe motion, useful memory and agent tasks;
the first release's optional sense/motion selection remains a product decision.
Unknown hardware data is not replaced with guessed qualification. An optional
local engine or a narrower launch can use the same contracts, with an explicit
capability manifest and limits.

All deliverables are uncommitted in this worktree. No device, private 4ace backup,
real Harness session or external partner repository was changed. The next
implementation task is P0/P1 under its own scope, not a deployment or a blanket
rewrite of every component at once.
