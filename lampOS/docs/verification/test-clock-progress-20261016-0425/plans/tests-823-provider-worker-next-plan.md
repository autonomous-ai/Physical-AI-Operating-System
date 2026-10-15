# Provider worker progress and frozen shutdown: next #823 preparation

Read-only source plan at **2026-10-16 04:06:37 +0700**, from shell `date`.
Voice source is `/private/tmp/lampos-tests-provider-moment/lampOS`; Gemini's
completed capture-clock preparation is read separately from
`/private/tmp/lampos-tests-gemini-capture-clock/lampOS`. No source, Cargo cache,
Git, network, build, test, or device action was performed for this plan. The
parent reports that the 12 new Gemini cases and the default/feature suites pass;
this report does not independently claim that execution.

**Next separate change: prepare an explicit test-only worker clock/poll endpoint,
without migrating any existing early-end scenario.** Its receipt means that the
real provider worker completed one original bounded poll, or returned. It must
not mean that Gemini's Actor, a connector attempt, the scripted service, the
runtime thread, or a sentinel child completed. That deliberately limited seam is
reviewable before adding the later whole-runtime controller.

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
entrypoint should require its explicit endpoint, for example:

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
    ControlDeferred,
    InputDeferred,
    Stopped { cause: StopCause },
}
enum StopCause { ReceivedStop, AuthorityRevoked }
```

`Complete` means the existing bounded tick body reached its end, not empty
queues or global idle. A receipt carries a strictly increasing poll sequence,
the admitted Moment/round and phase, actual channel send/receive counters and
sequence identities, handled-control and resolved-handoff counters, retained
input identity, pending output/urgent counts, and explicit original
budget/backpressure deferrals. A separate terminal record is emitted by the
outer wrapper **after the serve future actually returns**, preserving its
success/error/panic result. A future dropped by cancellation is not a successful
return. No extra clock sample is needed to stamp a receipt: use the already
admitted Moment.

Keep the instrumentation synchronous and bounded. Recording progress must not
await, pump, advance time, perform another receive, or silently drop an overflow.
Use counters/current state plus a latched terminal result, rather than an
unbounded event log. A real missing receipt, counter gap, unsupported phase or
overflow is an explicit fixture failure, never an assumed completion.

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
receipt distinct from successfully handled control and handoff. Include actual
sequence identities/counts so a later sequence cannot masquerade as consumption
of a missing earlier packet. If the fixture requires a complete prefix and finds
a gap, fail its barrier without changing transport's production accept rules.

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

On the single runtime thread, validate/adopt the ObserverScope Moment, update
the shared capture snapshot, then immediately poll `tokio::time::advance(delta)`
in the same turn, with no intervening await. Pinned Tokio updates its clock
before that future's first yield. This avoids other runtime tasks observing
half-updated domains. Capture-source reads from an external thread need only
return the last atomically admitted capture snapshot; they must never consult
that thread's Tokio or TLS clock. Preserve LinkClock's three sample positions,
1,000 us sample width, bounded history and all mismatch/refusal comparisons.

**Intermediate Moment blocker:** the present immutable API only snapshots the
root's current elapsed time. It cannot produce the T−1 ms snapshot needed while
catching a provider up from T−2 to T. Prepare this separately, with a narrowly
checked operation such as:

```rust
impl Moment {
    pub(crate) fn between(&self, later: &Moment, elapsed: Duration) -> io::Result<Moment>;
}
```

Both endpoints must validate, share the same Arc origin, be ordered, and enclose
`elapsed`; construction uses the existing checked epoch arithmetic. Both
endpoints descend from actual root snapshots. It grants no arbitrary future
advance, pump or fault injection. Observer adoption still rejects regression,
different roots and invalidation. This can be a separate small
`scenario_clock.rs` + new boundary tests + fixture-doc change; keep all original
Scope pump/2 ms/frozen-shutdown code and old tests exact. Until it is reviewed,
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
wait for the controlled runtime's entire lifetime. Confirm it is installed before
spawning participant tasks. Do not use private Tokio APIs or a busy runnable task.

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

Shutdown needs three states: `Run`, `ShutdownCatchUp`, `ShutdownDrain`.
After a Run pump at T, the root may end the conversation at **T+2 while the
provider remains at T**. The existing code requests scripted-service abort
before `rig.shutdown` (`early_end.rs:740`); shutdown stamps and sends real Stop
envelopes at the root's current T+2 (`coordinator.rs:483–510`). Therefore:

* First catch the provider up to the **already admitted root shutdown Moment
  T+2**, honoring any still-due original ticks. Service cancellation has already
  been requested. This does not advance the root. Holding the provider at T
  would reject the real Stop as future-dated.
* Once caught up, freeze all logical clocks at that Moment. A next Tokio tick in
  the future cannot read a remaining Stop. The explicit fixture tick adapter
  may then admit at most one **host-paced 2 ms ShutdownDrain permit**, with Skip
  behavior and the caller's one remaining **300 ms** cleanup budget. Anchor the
  cadence to the actual preceding poll; never queue a burst of catch-up permits.
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

| Boundary | Required proof / expected failure |
|---|---|
| Default entrypoints | Existing constructors still choose the real ticker and host source; enabling Gemini's feature does not switch any default factory. All old bodies/assertions unchanged. |
| Pure coherent domains | Repeated reads do no work. Explicit paired admission advances monotonic/std/Tokio consistently. Advancing one domain alone still triggers the original LinkClock refusal. |
| Shared capture source | An InputSender clone on another real thread sees the stored admitted capture Instant, never its TLS host fallback; root invalidation retains that value until cancellation. |
| Moment projection, separate preparation | Same-root enclosed endpoints succeed at nanosecond boundaries; reversed endpoints, outside range, same-looking different origins, overflow and invalidated roots fail. Observer still cannot pump/advance. |
| No auto-advance | With a confirmed parked blocking guard and a due timer, complete a real host handshake and verify Tokio is stationary. An explicit admitted advance permits the timer; release/join the guard only after task cleanup. |
| Initial and repeated epoch | Exactly one actual immediate worker tick at bootstrap; a second same-Moment Run request cannot manufacture another. |
| Phase ordering | Output produced at T may remain for the coordinator at T+2. The pump does not wait on same-phase parent consumption; next pump checks the real consumed prefix. |
| Control slice | Real 17-command backlog with Stop beyond the first 16 yields an honest deferred receipt, then the original later tick consumes Stop. No extra priority receive. |
| Data/control race | Real input received before a newly visible Authority remains retained until the priority reread succeeds; exact 100 ms expiry still fails. |
| Output credit/capacity | At zero credit or a failed real send, sent counters do not advance. Control still progresses; queued notice/output is not a completed send. |
| Handled versus downstream | A resolved worker handoff alone never satisfies an Actor-handled or service-consumed target; hold the actual consumer and require its later receipt in the separate integration preparation. |
| Due service work | With later full progress seams: preserve original 1 ms rests, 100 ms first-drop, 150 ms answer delay, 20/40 ms recovery waits and 40 ms packets. Check actual parsed/flushed messages. |
| Blocked async work | A pending write is not an idle select. Advancement requires its known protocol deadline or real causal unblock, not yields or an empty socket. |
| Shutdown lag | Provider at T, root at T+2: real stamped Stop is accepted only after valid catch-up; root does not advance again. Preserve a still-due original tick before freeze. |
| Frozen Stop | With next original tick in the frozen future, a real host-paced drain permit runs the existing handler. Observe Stop receipt and later worker return separately; all logical clocks remain fixed. |
| Frozen backlog/deadline | Bounded cleanup polls do not reset the shared 300 ms deadline. Missing Stop/receipt fails, cancellation wakes a frozen runtime, and cleanup is still owned. |
| Root normal Drop and panic | Cancel wakes a full/idle bridge. Further observe, including an equal Moment, fails. Existing reads stay frozen. No fake Stop, host fallback or leaked blocking guard. |
| Task failure | Unexpected cancellation and panic retain the old propagation rules; join both fixture tasks before propagating either. A timeout is not a terminal receipt. |
| Actor terminal/drain, later owner seam | Terminal state before buffered drain is not task return; Connection abort must produce actual lifetime-end observation before a whole-runtime cleanup claim. |
| Real child boundary | Even all logical receipts and runtime/thread joins do not substitute for the existing actual sentinel-process exit/reap checks. |
| Deadline origins, migration only | Startup and conversation start at their original lifecycle points; an earlier-created Scope cannot shorten either budget. No per-message renewed watchdog. |

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

Initial source snapshot: **2026-10-16 03:55:44 +0700**. All 12 source/reference
hashes matched again at the date above. Full values and lengths are in
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
