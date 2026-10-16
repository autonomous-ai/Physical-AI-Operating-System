# Revised Provider worker progress and frozen shutdown plan

Revised **2026-10-16 04:32:55+0700**, from shell `date`. This is a separate scratch
revision of `/private/tmp/tests-823-provider-worker-next-plan.md`, SHA-256
`667e02178e78f3e41a24e197e75c0d2ca90c7059c6d1fb1dce0bcdfdc85b6ad1`. The raw plan is unchanged.
It incorporates the four contract corrections in
`/private/tmp/tests-823-provider-worker-next-review.md`, SHA-256
`34c7ad9a834b4680ea64cdcf2f43814005cdeaa1304d9625978e36edd2abf1a3`.

The separate, pure `Moment::between` addition is now written in the three allowed
preparation paths, with twelve new cases and exact reversal of the prior seven
root and thirteen Moment tests. It has not been compiled or executed. Its
handoff and hashes are in
`/private/tmp/tests-823-provider-moment-between-evidence/handoff.md`; root owns its
gates. That value-only preparation is the immediate bounded change. It supplies
the proposed controller's 1 ms intermediate projection; a deliberately limited
2 ms worker-only probe does not technically depend on that projection.

**After the Moment gate, prepare an explicit test-only worker clock/poll
endpoint without migrating an existing early-end scenario.** A worker receipt
means only an actual original bounded worker poll/notice boundary or actual
outer future result. It never means that Gemini's Actor, a connector attempt,
the scripted service, the runtime thread, or a sentinel child completed. An
endpoint owner, capture adapter and no-auto-advance bridge are named parts with
their own tests if included; they are not an implicit whole-runtime controller.
No worker implementation, Cargo command or new runtime test was performed for
this revision. The parent-reported Gemini gates remain imported evidence, not
independent execution here.

The original ten early-end coordinator/provider cases are the largest untreated
new family in the frozen inventory. The imported #889 observation is 10 PASS,
60.205 s summed across concurrent shards, not a serial baseline or a projected
saving. The separate direct name-note case was 1.116 s. Neither measurement was
repeated here; neither candidate is migrated by this plan. Do not touch
capture-hold, Supervisor/recovery, native-head, endpoint fixtures, or owner fixes
#866/#906/#890.

## Exact next-change boundary

| File under `lampOS` | Narrow proposed change |
|---|---|
| `crates/voice/src/provider_worker.rs` | Use the existing ScenarioClock monotonic reader under `cfg(test)` and the existing raw reader otherwise. Replace only the capture-age translation's existing `Instant::now()` with a named helper whose non-test body is that same one read. Add an explicit controlled test entrypoint and optional worker poll instrumentation. Keep existing constructors on the original tick source. |
| `crates/voice/src/provider_link.rs` | Conditional test monotonic import only. Preserve the before-monotonic / Tokio-now / after-monotonic sample sequence and every existing refusal. |
| `crates/voice/src/transport.rs` | Optional, explicitly installed `cfg(test)` channel observations after successful real send and fully validated real receive. No packet, normal constructor, receive-order, age, grace, late-packet, or I/O change. |
| New `crates/voice/src/provider_fixture.rs` | Worker endpoint, bounded receipt state, phase/permit types, pure shared capture-clock adapter, and an owned host control bridge for focused fixture tests. Do not expose a misleading whole-runtime `round_complete` API. |
| New `crates/voice/src/provider_fixture_tests.rs` | Focused clock, actual channel/poll, terminal and owned-bridge tests below. Tests that exercise only the worker must say so. They may hold setup before an Actor exists rather than claim to join an unobservable nested Actor. |
| `crates/voice/src/lib.rs` | Declare the new module only under `cfg(test)`. |
| `crates/voice/Cargo.toml` | Explicit dev-only Tokio `test-util` and Gemini `fixture-clock` activation. Keep ordinary dependency declarations/default factories unchanged; no production fixture option. |
| `docs/voice-test-fixtures.md` | Explain receipt scope, clock domains, shutdown phases and remaining migration blockers in English. |

Do not edit `coordinator.rs`, `coordinator/tests/early_end.rs`, existing test
bodies, current Gemini source, or root Scope behavior in this preparation. If
implementing the shared adapter would pull in a full controller, split it into
the following preparation instead; do not enlarge this PR to migrate a case.
The existing `serve_coordinated_test` remains a default wrapper. The opt-in
entrypoint requires a private, admitted endpoint created by the fixture owner.
An endpoint is bound to the root identity, its one current-thread runtime,
phase, and validated current admission; callers cannot freely construct it.
Taking an already-created Supervisor does not establish setup safety. The owner
must pause its runtime, confirm the inhibition guard, install the thread-bound
ObserverScope, initialize shared capture state and attach transport probes
before constructing or polling provider participants, and before the first Ready.
The entrypoint rejects an endpoint from another owner/runtime or without a valid
admission. It can still have this worker-facing shape:

```rust
// All of these are crate-private, test-only declarations.
pub(crate) async fn serve_coordinated_fixture<C: Connect>(
    channels: WorkerChannels,
    boot: BootId,
    provider: Supervisor<C>,
    fixture: WorkerEndpoint,
) -> Result<()>;

enum WorkerPhase { Run, ShutdownCatchUp, ShutdownDrain }
enum PollEnd {
    Complete,
    ControlDeferred { site: ControlDeferredAt },
    Stopped { cause: StopCause },
}
enum ControlDeferredAt { BeforeInput, BeforeHandoff }
enum StopCause { ReceivedStop, AuthorityRevoked }
// These describe actual branches, not a request to consume more work.
```

`Complete` means the existing bounded tick body reached its end, not empty
queues or global idle. `BeforeInput` is the original control-deferred continue
after synchronization/pruning; `BeforeHandoff` is the original priority-control
deferral returned from input handling. Neither means an input queue is empty.
Do not invent an `InputDeferred` outcome for `receive_inputs_inner` returning
`Empty` after eight iterations. Observe the actual iteration budget, retained
input and original backpressure reasons in bounded state without an extra read.

A successful poll receipt carries a strictly increasing poll sequence, admitted
Moment/round and phase, actual channel identities/counts/sequences, handled
control and resolved-handoff counters, retained input identity, pending
output/urgent counts, and original budget/backpressure observations. A separate
notice revision records actual notice handling, including its early continue.
An older tick receipt cannot prove completion of work announced after that
notice or after a newer producer generation. Stop versus Authority is recorded
at the actual cause site, never inferred from the requested phase.

A separate outer result record is emitted **after the serve future actually
returns**, preserving its real success/error. Panic and cancellation categories
are diagnostic records owned by the actual wrapper/join result; neither is a
successful poll or return. A Stop-handler receipt is not the outer-return record.
Do not move, erase or relabel the actual error/panic payload. No extra logical
clock sample is needed to stamp a receipt: use the already admitted Moment.

Keep the instrumentation synchronous and bounded. Recording progress must not
await, pump, advance time, perform another receive, or silently drop an overflow.
Use counters/current state plus a latched terminal result, rather than an
unbounded event log. A real missing receipt, counter gap, unsupported phase,
cancellation or overflow latches failure synchronously. It cannot increment a
successful completion count, reuse a stale receipt, perform another receive or
resume admission. A worker-only record cannot satisfy another role's target.

## Actual boundaries in the current source

`serve_coordinated_test` at `provider_worker.rs:869` enters the same
`serve_with_visual` body at 894. It sends Ready at 902, then samples LinkClock,
constructs intake and creates the original **2 ms Skip interval** at 910. Ready
therefore proves neither the first poll nor a ready Gemini connection.

The biased select begins at 1047. Preserve its tick-before-notice priority and
these boundaries:

* First drain the shared **16-control** budget at 1050. A real Stop calls
  `intake.stop()` at 2425; an authority revocation can also return `Stopped` at
  2461. Record which actual path occurred. Do not infer Stop from a phase flag.
* Keep `service_link` before input and before output credit. Keep synchronization
  and retired-output pruning before the control-deferred `continue` at 1380.
  Record that completed bounded poll as deferred; do not execute its skipped
  tail to obtain a nicer receipt.
* `receive_inputs_inner` at 2502 receives at most **eight** actual input packets,
  retains the source command and its fresh receipt time, and rereads priority
  control **after data receipt and before handoff**. Preserve the shared control
  budget, the deferred/Stop returns at 1385–1386, and the existing exact
  **100 ms** retained-input expiry. Reaching an eight-input limit is not proof
  that the data socket is empty.
* The output tail at 1415 tries at most **eight** sends, urgent first, and pops
  only after the real flow-controlled send succeeds. Emit the normal poll
  receipt after this tail. A `WouldBlock` or missing credit is not a sent packet.
* The notice arm at 1425 is distinct from a tick. Observe actual notice handling,
  including its existing early `continue`; queued output is not sent output.
  Errors remain errors, not successful completed polls.

The existing source has additional optional bus, tools, visual, memory and
host-timed renewal paths. The first controlled entrypoint supports the explicit
early-end configuration only. Validate/reject unsupported enabled configuration;
do not reset globals, replace their clocks, or silently disable behavior.

`transport.rs:82` assigns its sent sequence only after `try_send` succeeds.
`receive` at 100 has both original time samples, real decoding and order/age
validation, then sets `received` at 154. Install probes before any startup send
and observe only after those respective success points. Keep accepted transport
receipt distinct from successfully handled control and handoff. Each probe
retains channel identity and direction, boot, successful-send/accepted-receive
count, and actual sequence identity. A high sequence alone does not prove a
complete prefix. A gap can fail an explicitly announced fixture barrier without
changing production gap acceptance. Attach every required probe before Ready;
probe overflow/cancellation latches failure without another I/O operation,
receive, await or logical clock read.

`InputHandoff::submit` at 2883 first checks source capture-order, computes age
from `now_us - read_completed_at_us`, enforces the full **2,500 ms** strict bound,
then does one `Instant::now().checked_sub(age)` and retains the original
non-regressing translated-Instant clamp. Replace only that existing sample.
Do not freshen acquisition stamps, remove the clamp or call a successful
`InputSink` operation an Actor receipt: the handoff can intentionally resolve an
outage/refusal without Gemini accepting a command.

## Clock contract and two necessary follow-up preparations

There are three correlated domains plus independent host guards:

| Domain | Controlled value / unchanged purpose |
|---|---|
| IPC/authority monotonic microseconds | Active root Moment on the coordinator; admitted ObserverScope Moment on the runtime thread. Source acquisition timestamps remain original. |
| `std::Instant` capture age | Same originating epoch and exact elapsed Duration as that admitted Moment, including nanoseconds. Used by the one worker translation sample and Gemini's prepared shared capture source. |
| Tokio protocol time | One paused current-thread runtime, explicitly advanced by the same Duration. Existing 1/2/10/40 ms waits and setup/read/write/recovery deadlines remain timers in this domain. |
| Host time | Real startup, missing-progress, shutdown, channel/thread/child cleanup budgets. Never use frozen Tokio time as the sole watchdog. |

The Gemini adapter must store the **last admitted Moment/Instant in shared
state**, readable purely from any InputSender thread. A zero-sized adapter that
calls thread-local `scenario_clock::instant()` is wrong: an InputSender clone on
another thread would silently read host time. Only the runtime controller may
update the stored snapshot. Neither `CaptureClock::now` nor any Moment getter
may pump, sample another clock, check a queue or advance.

The one runtime-thread owner validates the next root Moment, exact ordered
round/phase transition, elapsed delta, and the runtime's current elapsed offset
before publishing any domain. Its paused-Tokio baseline is recorded separately:
compare elapsed offsets, not equality of independently sampled absolute epochs.
Keep the thread-bound ObserverScope in the outer runtime owner/block_on scope,
not inside a `tokio::spawn` future that must still be Send.

After validation, adopt the ObserverScope Moment, update the shared capture
snapshot, and immediately first-poll `tokio::time::advance(delta)` in the same
turn without an intervening yield. Pinned Tokio updates its clock before that
future's first yield. A partial-admission error closes/cancels the endpoint;
never reuse it or poll participants with mixed domains. Capture-source reads
from an external thread return the last atomically admitted capture snapshot;
they never consult that thread's Tokio or TLS clock. The pure adapter and
actual Actor/InputSender consumption are distinct test boundaries.

Preserve LinkClock's three sample positions, 1,000 us sample width, bounded
history and every existing comparison. Its current refusal is asymmetric:
monotonic advancement while Tokio remains fixed fails, but strictly later
Tokio with unchanged monotonic samples can pass. Submicrosecond projection can
also legitimately move Instant while integer microseconds remain equal. Do not
add a production refusal to satisfy a proposed test. The fixture owner must
independently reject incoherent admissions using its exact elapsed delta;
LinkClock checks alone do not establish that invariant.

**Intermediate Moment preparation, written but not yet gated:** the proposed
1 ms catch-up between T−2 and T requires an enclosed snapshot that the original
snapshot-only API could not produce. The separate three-file addition supplies:

```rust
impl Moment {
    pub(crate) fn between(&self, later: &Moment, elapsed_from_origin: Duration) -> io::Result<Self>;
}
```

Both endpoints must validate, share the same Arc origin, be ordered, and enclose
the absolute `elapsed_from_origin`, including either endpoint. It is not a
delta from the earlier endpoint. Construction uses the existing checked epoch
arithmetic and retains nanoseconds. Both endpoints descend from actual root
snapshots. It grants no arbitrary future advance, pump or fault injection.
Observer adoption still rejects regression, different roots and invalidation.
The separate `scenario_clock.rs` + new boundary tests + fixture-doc addition
reverses exactly to the prior preparation and original source. Root pump/2 ms/
frozen-Shutdown code and all old tests remain exact. Await its fresh gate;
do not replace the service's original 1 ms rest with 2 ms steps.

**Actor/service receipt blocker:** the completed Gemini capture-clock change
does not expose runtime progress or task-lifetime observations. Do not extend
its clock callback for that purpose. A later, separately owned progress seam
must distinguish command enqueue/handling, incoming service frame
parse/handling, actual flushed client/server writes, due maintenance, pending
async write/deadline, buffered output/drain, and actual task return/cancellation.
The existing Actor terminal state precedes final drain (`session.rs:1055`), and
Connection Drop aborts its owned task at 608 without exposing its join. No
Supervisor change is needed or proposed; an opt-in Actor/task registry at the
existing spawn boundary can observe actual lifetime while normal ownership
stays with Connection. Its exact API and completeness require the Gemini owner's
review. Worker-only receipts cannot fill this gap.

An actual `Poll::Pending` observation around the original select future may
describe that participant's wait after its maintenance, but not a write that is
mid-handler or another task's work. New producer work must invalidate the
relevant earlier receipt by generation/prefix. A known protocol deadline can
permit advancement toward that deadline; an unexplained pending operation or an
unconsumed announced prefix cannot. Preserve existing yields for fairness; no
number of yields, empty socket checks, send successes or `advance().await`
returns constitutes a progress barrier.

## Runtime ownership, auto-advance and cancellation

Pinned Tokio 1.53.1 explicitly auto-advances a paused runtime when idle. A live
`spawn_blocking` task inhibits that behavior (`time/clock.rs:121–159`,
`runtime/blocking/schedule.rs:25,47`). Use one owned blocking host mailbox/condition
wait for the controlled runtime's entire named lifetime. Confirm it is installed
before constructing or polling provider participants. The guard's owner must
outlive all named work for which cleanup is claimed. A worker harness may hold
setup before an Actor exists, but cannot claim to join an unobservable Actor.
Do not use private Tokio APIs or a busy runnable task.

Use one bounded Step slot, a separate latched cancellation request and an explicit
release flag. Cancellation must wake the bridge even if the Step slot is full;
it must also wake the async controller without requiring a timer. A host deadline
or root-controller Drop requests cancellation; it must **not terminate the
blocking guard first**, because that would let surviving Tokio tasks auto-advance.
Keep inhibition while cancelling and awaiting all owned async work, then release
and join the blocking guard, finish/drop the runtime, and join its OS thread.
Do not use runtime shutdown/background detachment to stand in for a known join.

Preserve task error/panic rules: abort both fixture tasks when requested, await
both before propagating either panic, accept cancellation only when requested,
and retain genuine worker errors. The old fixture does this at
`early_end.rs:550,650–661`; leave its body unchanged. The controlled constructor
cannot copy its **5 ms Tokio sleep** stop monitor at 648: a frozen sleep is not
woken by setting the AtomicBool. Use the real cancellation notification to enter
the same abort/join policy. Later nested Actor task observations are required
before claiming complete provider-runtime cleanup.

Every wait uses the caller's absolute remaining host deadline, not a fresh
allowance for each receipt. There is a genuine limit: Rust cannot forcibly join
an arbitrary uncooperative OS thread safely. Constrain this fixture to known
cooperative tasks and a wakeable bounded bridge; a missed cleanup deadline is a
failure, not a joined result or permission to leave a detached task. Process
isolation would be a separate design if an unbounded blocking participant is
required. No such participant is authorized here.

Root invalidation requires cancellation. Existing `ObserverScope` correctly
retains its last value after root Drop/panic, rejects later adoption even of an
equal snapshot, and restores host fallback only on its own Drop. Preserve that
contract. Arrange for the root pump/controller owner to close/wake the bridge
on Drop (a Weak reference to an Rc-owned Scope avoids a Scope/pump ownership
cycle). Cancel/join participants before dropping the observer. Do not reinterpret
an invalid snapshot as host time, invent one more tick, or claim real Stop was
consumed during emergency cancellation.

## Preserve T then T+2, including the shutdown catch-up

The coordinator calls `step(Run)` before its loop body (`coordinator.rs:2789`).
Scope pumps at **T**, then advances exactly **2 ms**. The coordinator next runs
and sends input/control at **T+2**. Preserve that shared ordering.

1. Bootstrap at the held root epoch. Observe the worker's one original immediate
   interval tick; a first parent pump at the same epoch must not invent a second.
2. Each pump snapshots real successful parent-send targets and already consumed
   prior worker output. Release peer work with the original source times.
3. Bring the provider only up to the current root Moment, with at most 1 ms
   intermediate steps or an explicitly known earlier deadline. Honor the
   original 1 ms service rest, 2 ms worker, 10 ms Actor/capture and 40 ms packet
   cadences. At T−1, do not let an unaligned worker tick read IPC stamped T.
4. Require role-specific processing receipts or an explicit original bounded
   deferral. Return from the pump; **do not wait for the coordinator to consume
   output emitted in this same pump**. It cannot run until the pump returns.
   Validate its actual consumption at the next pump.

The fixture owner enforces one-way `Run → ShutdownCatchUp → ShutdownDrain →
terminal/cancelled` transitions. Latch one actual root shutdown Moment and the
caller's one inherited absolute host deadline when shutdown begins. Neither
can be replaced or extended by a later request. Reject Run after shutdown,
Drain before catch-up, duplicate/out-of-order rounds, target drift and a permit
without its owner. Run requires an actual original interval tick; same-Moment
requests may observe state but cannot manufacture another tick. Only one permit
can be outstanding. Host cadence readings are guard instrumentation, not added
logical sample sites.

After a Run pump at T, the root may end the conversation at **T+2 while the
provider remains at T**. The existing code requests scripted-service abort
before `rig.shutdown` (`early_end.rs:740`); shutdown stamps and sends real Stop
envelopes at the root's current T+2 (`coordinator.rs:483–510`). Therefore:

* First catch the provider up to the **already admitted root shutdown Moment
  T+2**, honoring any still-due original ticks. Service cancellation has already
  been requested. This does not advance the root. Holding the provider at T
  would reject the real Stop as future-dated.
* Once caught up, freeze all logical clocks at the latched target Moment. A next
  Tokio tick in the future cannot read a remaining Stop. The explicit fixture
  tick adapter may then admit at most one **host-paced 2 ms ShutdownDrain
  permit**, with Skip behavior under the caller's remaining inherited **300 ms**
  cleanup deadline. Anchor its next host eligibility to the actual preceding
  worker poll, including an original tick; never queue a burst or renew the
  deadline. The permit authorizes cleanup work, not a fabricated Tokio due time.
* A drain permit executes the original entire bounded tick/control path,
  including the shared 16-control budget and actual Stop handler. It does not
  consume control itself, bypass a backlog, forge a tick due time, or force a
  result. Label it cleanup, not Run progress. Keep the original outer biased
  selection; an already-due original tick has precedence over a drain permit.
* Worker Stop receipt, worker future return, Actor lifetime end, runtime thread
  join and sentinel child exit remain separate. Run the existing actual child
  kill/reap/exit checks regardless of fixture errors. On root invalidation, skip
  snapshot catch-up and cancel; stale Moment adoption is forbidden.

This host-paced drain is an explicit fixture scheduling choice, not a feature of
a pure clock. It needs focused review and tests. If that choice is rejected,
frozen shutdown plus tick-only Stop intake remain incompatible; keep the old
scenario on host time rather than secretly advancing a clock or treating abort
as Stop receipt.

**Deadline-origin integration item:** the existing 3 s startup deadline starts
after runtime/Rig construction (`early_end.rs:689`); the 12 s conversation
deadline starts at the conversation call (`730–734`). `Scope::enter` starts its
own host watchdog immediately. A later controlled constructor must preserve both
original deadline origins and must not accidentally impose an earlier Scope
deadline on the conversation, nor reset/extend either old budget. Resolving the
umbrella Scope guard versus these phase guards is separate from this worker-only
preparation. The 300 ms cleanup deadline remains exactly the coordinator's
existing shared host deadline.

## Focused boundary-case matrix

These are proposed new tests, not executed results. Use actual channel/task
receipts and explicit barriers; do not assert a machine must schedule within a
tiny elapsed interval or retry failures.

M = the separate Moment preparation. W = a required first-preparation proof if
that worker/endpoint/bridge API is included. I = later Actor/service/controller
or scenario integration, not a worker-preparation completion claim.

| # | Boundary | Required proof and stage |
| --- | --- | --- |
| 1 | Default entrypoints | W: original real ticker/source, all old bodies/assertions; feature unification leaves ordinary Gemini constructors System. |
| 2 | Pure coherent domains | W: repeated reads do no work; owner checks exact elapsed deltas independently of asymmetric LinkClock refusal and submicrosecond truncation. No production rule change. |
| 3 | Shared capture source | W: a real external thread reads the stored admitted source and held invalidation value. I: actual InputSender/Actor consumption; no such claim while setup is held before Actor creation. |
| 4 | Moment projection | M: absolute inclusive same-origin offsets, exact nanoseconds, bounds/identity/overflow validation, normal/panic/TLS invalidation, observer refusal and no pumping. Twelve new source cases await execution. |
| 5 | No auto-advance | W if the bridge is included: after confirmed guard installation, hold a genuinely future timer, complete an independent host handshake with frozen Tokio, then explicitly admit its advance. Release/join only after named cleanup. |
| 6 | Initial/repeated epoch | W: one actual immediate tick; same-Moment Run cannot invent a second. |
| 7 | Phase ordering | W: local announced-prefix accounting only. I: coordinator output at T consumed at T+2/next pump, never a same-pump consumption deadlock. |
| 8 | Control slice | W: real 17-command backlog gives an original first-slice deferred receipt and later actual Stop. Do not execute skipped tails or extra priority receives. |
| 9 | Data/control race | W: real retained input, priority reread and unchanged 100 ms expiry with causal barriers/original intake seams, not scheduling guesses. |
| 10 | Output credit/capacity | W: real successful send/receive probes; zero credit/failed send changes no sent count; control progresses. Queueing a notice is not a send. |
| 11 | Handled/downstream | W: handoff resolution cannot satisfy a downstream target. I: hold/release the real Actor/service consumer and require its own receipt. |
| 12 | Due service work | I: full original 1 ms rests, 100/150 ms delays, 20/40 ms recovery, 40 ms packets, real parsed/flushed messages. |
| 13 | Blocked async work | W: own bridge/endpoint pending waits. I: Actor/service write versus idle-select work; no arbitrary yield count establishes progress. |
| 14 | Shutdown lag | W: real stamped Stop at T+2 requires valid provider catch-up without advancing root; due original ticks retain priority. I: actual coordinator/service ordering. |
| 15 | Frozen Stop | W if drain is added: actual handler followed by separate actual worker return under frozen clocks; no invented tick or Stop. |
| 16 | Frozen backlog/deadline | W if drain is added: one immutable target/deadline, one outstanding permit, 2 ms host Skip pacing anchored to actual polls, no renewed allowance/burst; cancellation wakes and owned bridge joins. |
| 17 | Root Drop/panic | W: cancel full/idle bridge, reject stale/equal admission, preserve frozen reads until owned cleanup. I: nested Actor/service lifetime. No fake Stop or host fallback. |
| 18 | Task failure | W: actual worker/wrapper/bridge failure/panic/cancel outcome and owned joins; failures do not increment success. I: original worker-plus-service join policy and nested lifetime. |
| 19 | Actor terminal/drain | I: terminal before buffered drain is not return; real Actor lifetime observation belongs to a later owner seam. |
| 20 | Real child boundary | I: original actual sentinel exit/kill/reap checks; logical receipts/runtime joins never replace them. |
| 21 | Deadline origins | I: startup and conversation guards start at their original lifecycle points; one actual cleanup deadline, no renewed or prematurely started budget. |

A whole matrix result cannot be claimed from W tests. No global `round_complete`
is introduced. The first worker slice may be smaller than the table if it omits
an optional bridge/drain capability; its stated API and tests must omit that
capability together rather than silently assuming it.

## Preservation and subsequent qualification

Before implementation, snapshot current source, not an older design's bytes:
the worker/coordinator files have changed since the first audit. Never reverse
another owner's changes. Require an exact allowlisted reversal for the new
imports/helper/endpoint/probe additions and any shared-body extraction. Restore
the precise original sample locations, budgets, select order, returns, errors,
capture clamp, and tick expression in that reversal. A source-level default-path
proof is not a generated-code-equivalence claim.

Preserve every existing Voice test body/attribute/name and all unowned Rust
files by whole-file/function hashes as appropriate. Keep all current assertions,
including upstream additions since the frozen inventory. Later root-owned gates
must check compiled identities/ignored flags, strict lint, default and explicit
feature behavior, and affected dependents. Memory's dev dependencies also include
Gemini and Voice, so the feature-unification qualification is broader than only
Voice/Surround/Evaluator. No Cargo or feature-graph command was run for this plan.

Only after bounded Moment projection, Actor/service lifetime/progress,
auto-advance inhibition, shutdown and deadline-origin decisions are proved
should one explicit early-end constructor migrate. Keep the link-drop case's
**six-second logical run, 200/600 ms endpoint behavior, 100/150 ms drop/answer
delays, twelve answer packets, 2/10/40 ms cadences, source timestamps, actual IPC,
and all assertions**. The existing host constructor and its guard test stay.
Paired local/native evidence and whole-original-set qualification remain later
work; this design claims no speedup or completion of the original 123 cases.

## Provenance

The unchanged raw plan's initial source snapshot was **2026-10-16 03:55:44
+0700**; its final recheck was **2026-10-16 04:06:37 +0700**. The hashes below
retain that historical provenance, not a fresh post-Moment source claim. The
only implementation performed for this revision task is the separate pure
Moment addition, whose fresh hashes are in its handoff. Full original values
and lengths are in
[`source-hashes.json`](tests-823-provider-worker-next-evidence/source-hashes.json);
the final read-only recheck and seven additional frozen reference hashes are in
[`source-recheck.json`](tests-823-provider-worker-next-evidence/source-recheck.json).
Key files:

| Source | SHA-256 |
|---|---|
| `scenario_clock.rs` | `4bb0811dbc62ad2345755eca06daa8f91a4e64a8dc1bfab66f763afe5cd7a5f6` |
| `scenario_moment_tests.rs` | `c372eac927457059471d7a6bc3b56c8855f530aab1b7b81e11305fe2dddfe210` |
| `provider_worker.rs` | `d51e9519eae25d9638978dfe60d70f81e433038cc2bbe10cdc593cf0af432f6d` |
| `provider_link.rs` | `b44639b8cddeacd8432f0870dd69d4ff29028b4855ef711cd64a6002584b499b` |
| `transport.rs` | `49308fd4a3e19ba1179a0410785e85912ed018757d79872497f40517529eae73` |
| `coordinator.rs` | `c901c8ef6601ed5ba26f3c7dac89765d0bc0829f1ac97d6a69cc6cf20701f4f8` |
| `coordinator/tests/early_end.rs` | `128632acb07813f234b72163a2c248ba9522bb29c58b0d66dbbac336acda4edf` |
| Frozen Gemini `session.rs` | `27f0d1b329d5407e2b85941a60fd747d12eed3fc7aa07accbf0ceda373ae6073` |

Prior inputs read: `/private/tmp/tests-823-provider-clock-design.md`,
`/private/tmp/tests-823-gemini-capture-clock-plan.md`,
`/private/tmp/tests-823-voice-remaining-audit.md` and its exact selected/native
evidence. This plan updates the older design with the completed immutable Moment
and shared capture-clock APIs; it does not assume the still-missing progress
seams were part of those changes.
