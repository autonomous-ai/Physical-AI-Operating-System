# Provider clock and completed-poll preparation for the original early-end link drop

Read-only design at **2026-10-16 01:41:02 +0700** (from `date`), against the working source at
`/private/tmp/lampos-tests-fixture-progress`, previously verified head
`8a7521661cb06997a18eed32ead13d8d9ee81fd9`. Previously audited source hashes were
rechecked directly; no Git operation was used for this design. This report changes
no source and runs no build, test, device action or network request.

**Recommendation:** prepare a test-only provider time projection and actual loop
progress observations before migrating this one identity:
`coordinator::tests::early_end::the_link_dropping_in_between_fails_the_request_unheard_and_the_next_is_answered`.
A paused Tokio attribute or one extra `yield_now()` is insufficient. There are
three independently checked domains: coordinator/IPC monotonic timestamps,
Tokio protocol deadlines, and `std::Instant` capture age. The smallest general
solution needs an explicit Gemini capture-clock/progress seam as a separate
preparation change. No change to `gemini/src/recovery.rs` (Supervisor), native
head, capture-hold worker, endpoint fixtures, #866, workflows or settings is
proposed here.

## The current path, without inferred completion

| Source anchor | Actual path / boundary |
|---|---|
| `voice/src/coordinator/tests/early_end.rs:587` | `Setup::run_with_privacy_tolerance` rejects an active ScenarioClock before starting anything. Four `Worker::sleeping_fixture` children are real OS processes. The coordinator, capture/privacy/speaker peers and provider thread are distinct participants. |
| `early_end.rs:615` | The provider thread builds one unpaused current-thread Tokio runtime. It constructs the real `Supervisor`, spawns `serve_coordinated_test` and `serve_dials`, then retains the original task-abort/join handling. |
| `provider_worker.rs:860,884` | `serve_coordinated_test -> serve -> serve_with_visual`; the production body first sends Ready, samples LinkClock, constructs intake, then starts a **2 ms** interval with `MissedTickBehavior::Skip`. Ready alone is not a completed control tick. |
| `provider_worker.rs:1031` | A biased select prefers the due control tick over `next_notice`. Tick: drain control with one shared **16-command** budget; service link; feature work; synchronize/prune; at most **8 input** handoffs, each preceded by another control drain; at most **8 output** sends. Control or input deferral can `continue` before output sending. |
| `provider_worker.rs:1416,1516` | A separate notice select arm handles actual Supervisor output, updates state and queues urgent/data messages. Most queued output is sent by a later tick. Coordinated failure can set `upstream_ended` and keep the worker alive until Stop. Notice receipt is not output delivery or worker exit. |
| `provider_worker.rs:2022` | `service_link -> LinkClock::tick -> Supervisor::poll_link -> LinkClock::convert -> PendingLink::observe/try_send`; unchanged link polling must happen before input even when PCM output is full. |
| `provider_worker.rs:2385,2491,2776` | Intake retains at most one input, enforces its **100 ms** residence limit, rereads priority control after data receipt, checks authority and capture ordering, then calls `InputHandoff::submit`. Successful `submit` can also mean intentional retired-input discard; it is not automatically a Gemini enqueue. |
| `gemini/src/recovery.rs:903,965,975,1020` | `maintain`, `poll_link`, `next_output`, `output_step` preserve separate pending link and pending output. `next_output` intentionally waits while a link observation is pending. Failure may expose link state before already-buffered answer output is drained. This file need not be edited. |
| `gemini/src/testing.rs:138,195` | FakeConnector uses a real in-memory duplex WebSocket pair and `setup_and_spawn`, with each actual attempt published as a Dial. There is no external cloud/network connection. |
| `gemini/src/session.rs:629,1022,1068` | Real setup send/receive, spawned Actor, original **10 ms** actor timer. Each actor loop applies retirement/discard, releases held input, flushes events and checks deadlines before selecting command/socket/timer/capacity work. One server message per scheduling turn retains its existing yield. |
| `early_end.rs:166,187` | `serve_dials` has a 20 ms Tokio dial-receive timeout. `one_connection` uses `FakeService::rest(1 ms)`, records the actually parsed request, checks close/answer deadlines, and writes **960-sample/40 ms** audio packets every **40 ms**. The selected case closes the first connection **100 ms after its first End**, then answers only the second question after **150 ms**. |
| `early_end.rs:307,384,403` | Capture emits original source timestamps at **10 ms** intervals; privacy reports every **10 ms**; speaker records actual Channel receipt and sends actual playback acknowledgement. Their current 1 ms host sleeps are polling, not capture or playback evidence. |
| `early_end.rs:1224` | Six-second conversation; exact first/second wire lists, two connections, one answer, **12 × 4** played blocks, no early playback, first `provider_failed`, and zero given-up requests. Keep the full test body and assertions unchanged; select a new explicit fixture constructor/mode only. |

The four children are lifecycle sentinels, not the threads performing the fake
capture, service or playback. Their actual startup and kill/reap must remain real.
The current Ready/task-publication guard is **3 host seconds**; conversation
watchdog is **12 host seconds**. Coordinator shutdown retains its one shared
**300 ms host cleanup budget** and actual process observation. Logical Stop,
link failure, an Actor terminal state or a task receipt never substitutes for
actual child exit.

## Clock domains and the minimal explicit clock API

`provider_link.rs:222–357` samples `(host monotonic before, Tokio Instant,
host monotonic after)` in that order. Sampling allows at most 1,000 us uncertainty,
three attempts, and a bounded correlation history; conversion rejects a future
source or inadequate history. In particular, `retain` rejects **the same Tokio
Instant with an advancing host clock**. Freezing Tokio alone is correctly refused.
Do not widen these limits, bypass `retain`, invent a constant conversion offset,
or replace timestamp correlation with a readiness acknowledgement.

`provider_worker.rs:2877–2908` first checks source acquisition order and computes
age from IPC monotonic values, then subtracts that age from a fresh host
`std::Instant`; it keeps the existing clamp to the previous translated instant.
`gemini/src/session.rs:187,2399` checks age again at `InputSender::try_audio`;
`session.rs:1767` checks again when the Actor handles that command;
`session.rs:1686` checks retained audio against `MAX_INPUT_AGE + patience` before
writing. `MAX_INPUT_AGE` is **2,500 ms**, not the worker's separate 100 ms retained
input limit. Image-age checks are also system-Instant based. Session comments
explicitly distinguish capture age from Tokio protocol time.

For preparation, use test-only projection of an explicit coordinator moment,
not a process-global clock switch or a clone of the current Scope. Proposed
shapes (design signatures, not compiled code):

```rust
// voice/src/scenario_clock.rs, already cfg(test) at its module boundary.
#[derive(Clone, Copy)]
pub(crate) struct Moment {
    pub elapsed: Duration,
    pub monotonic_us: u64,
    pub instant: std::time::Instant,
}
impl Scope {
    pub(crate) fn moment(&self) -> io::Result<Moment>;
}
pub(crate) struct ObserverScope { /* thread-bound; no pump, no root advance */ }
impl ObserverScope {
    pub(crate) fn enter(initial: Moment) -> io::Result<Self>;
    pub(crate) fn observe(&mut self, next: Moment) -> io::Result<()>;
}

// New voice/src/provider_fixture.rs, cfg(test) only.
pub(crate) async fn serve_coordinated_fixture<C: Connect>(
    channels: WorkerChannels,
    boot: BootId,
    provider: Supervisor<C>,
    fixture: WorkerFixture,
) -> Result<()>;
```

`Scope` remains non-Send. ObserverScope is installed on the provider OS thread
around `runtime.block_on`, not inside a Send task. It only adopts moments from
the coordinator driver and rejects regressions, mismatched epochs and nested
scopes. Reads remain pure. Observe a new moment and advance paused Tokio in one
synchronous controller poll before any provider task is allowed to run; do not
publish an asynchronously changing root clock to an independently running worker.
Runtime and IPC clocks must agree whenever `ClockSample` is taken. `WorkerFixture`
carries Send-safe probes/control state, not the thread-bound ObserverScope. The
new controlled wrapper can call the existing serving core; production entrypoints
continue to install no fixture state.

Worker and LinkClock can retain their actual sampling expressions/order using
conditional imports: ordinary builds keep `lamp_ipc::monotonic_us`; test builds
use the scoped projection with raw-host fallback when no explicit fixture is
installed. Transport already uses the ScenarioClock import in tests. Add a
separately named `capture_instant_now()` to the worker's **one capture translation
site**; its normal branch is exactly `std::Instant::now()`. Do not casually replace
all `Instant` uses or make a clock read pump the runtime.

The worker's remaining std-time uses include 100 ms ears polling, 500 ms
known/presence polling, five-second air polling, one-second thread persistence,
renewal policy and execution timing (`provider_worker.rs:1057–1358,1880–1890`).
Those optional paths are outside this first fixture. The new constructor must
explicitly use `Options::reporting(Reporting::Coordinated)` (the existing
no-optional-tools fixture configuration at `provider_worker.rs:650`) and reject
unsupported active global features rather than quietly interpreting their host
clocks as logical time. Their host clocks stay unchanged; claiming a general worker
clock migration would require a separate audit/migration of those paths. Wall
clock persistence timestamps (`SystemTime`) also stay unchanged.

For Gemini, use one explicit fixture factory with a shared pure capture clock
and observations; leave default connector and session factories unchanged:

```rust
// gemini/src/testing.rs, behind a new default-off fixture-support feature.
pub trait CaptureClock: Send + Sync + 'static {
    fn now(&self) -> std::time::Instant;
}
pub struct FixtureHooks {
    pub capture_clock: Arc<dyn CaptureClock>,
    pub progress: SessionProgressSender,
}
pub fn connector_with_fixture(
    config: SessionConfig,
    hooks: FixtureHooks,
) -> (FakeConnector, mpsc::UnboundedReceiver<Dial>);
```

The fixture factory supplies the clock to the session's existing shared state;
InputSender and Actor use the same domain, including image/held-audio checks.
The normal path samples `std::Instant::now()` at the original sites, with exactly
the same comparison, age bound, write timeout, capture-order check and clamp.
Tokio still drives protocol deadlines, recovery, waits and original tickers;
there is no need to modify Supervisor/recovery for a second clock.

Enable Tokio `test-util` explicitly in **voice's dev-dependencies**. Gemini's
own dev-dependency does not guarantee it for an isolated voice build. A Gemini
fixture-support feature also needs explicit voice dev-dependency activation;
`cfg(test)` in a dependency is not enabled by the caller's tests. Cargo feature
unification means normal constructors must preserve host behavior even in a build
where fixture-support is enabled by another package. No runtime CLI/config flag.

A narrower alternative is to keep Gemini's host capture clock and forbid every
logical advance until all audio accepted during this step has actually been
written or discarded, with no queued/held audio left in the Actor. The original
link-drop scenario appears to have that shape, but it is not established by a
worker enqueue or an empty socket, and fails as a general solution for kept
successor audio. Do **not** choose it implicitly. A source clock preparation is
cleaner; owner review must authorize the Gemini session/testing scope before that
change starts.

## What a complete poll must report

A tick acknowledgement cannot mean “all queues empty.” The existing budgets and
deferral branches deliberately prevent that. Nor can “notice handled” mean “sent
to the coordinator.” Proposed concrete progress records:

```rust
pub(crate) struct WorkerStep {
    pub round: u64,
    pub up_to: Moment,
    pub control_sent_target: u64,
    pub input_sent_target: u64,
}
pub(crate) enum InputIdentity {
    Start { request: u64 },
    Audio { request: u64, sequence: u64 },
    End { request: u64 },
    Written { request: u64 },
    Reminder { request: u64 },
}
pub(crate) enum PollEnd {
    SliceComplete,
    ControlBudgetDeferred,
    InputControlDeferred,
    VisualControlDeferred,
    Stopped,
}
pub(crate) struct WorkerPollProgress {
    pub poll_seq: u64,
    pub at_us: u64,
    pub control_received: u64,
    pub control_resolved: u64,
    pub inputs_received: u64,
    pub handoffs_resolved: u64,
    pub retained_input: Option<InputIdentity>,
    pub notices_handled: u64,
    pub control_sent: u64,
    pub data_sent: u64,
    pub urgent_pending: usize,
    pub output_pending: usize,
    pub end: PollEnd,
    pub phase: PollPhase, // Run or ShutdownDrain, never interchangeable
}
```

`retained_input` identifies the actual still-owned command; it is not a fabricated
extra input or sequence. Counters are checked, cumulative and scoped to this
concrete worker/session, never inferred
from elapsed time or mutable queue lengths. Targets are frozen before a Step.
A target is satisfied only by real validated Channel receives and completed
handling. A shortfall may be carried to the next original tick **only with the
actual bounded-deferral/blocked reason recorded**, not a host scheduling guess.
Do not rerun another 2 ms **Run** tick at the same timestamp to make counters
match. The distinct host-paced Shutdown phase below is necessary because Scope
intentionally freezes protocol time during cleanup.

Insert completion observation after the original output-send loop, and also
immediately before every existing tick `continue` path (control deferral,
input-control deferral, visual-control deferral). Preserve the control-after-data
priority reread. Stop is distinct, errors are not successful completed polls,
and function-return/terminal observation comes from the wrapper after the real
serve future returns. Never swallow `?`, change the output cap, change which
messages are urgent, or insert a retry. Existing 16/8/8 budgets and 2 ms timer stay.

Successful Channel send counters advance only after `endpoint.try_send` succeeds;
accepted-receive counters advance only after framing, sequence, boot and freshness
validation returns a payload (`transport.rs:82–155`). Control resolved must
separately account for installed authority, deliberately skipped expired authority
and Stop; an accepted datagram is not necessarily installed authority. Handoff
resolved is similarly not synonymous with a successful Gemini enqueue. Observe
actual `InputSender::enqueue` success separately. Instrumentation must not add a
second receive or drain either channel itself.

Gemini progress needs session-tagged counters/state for **commands enqueued,
commands dequeued/completed, retained audio, client frames successfully flushed,
server frames actually processed, events enqueued, events consumed/discarded,
outbox pending, timer-boundary completion, and actual Actor return**. Count a
command after `command(...).await` resolves, not at dequeue alone; a write may
still be awaiting transport progress. Count service input only after real
WebSocket decode and `one_connection`'s matching state transition/Seen append.
Count service output flush separately from Actor processing. Track setup/close
frames explicitly; neither is a normal audio event. `Connection::next_event`
can discard retired events before returning one, so raw receive, discard and
returned-to-Supervisor are different counters.

No artificial protocol messages, queue sentinels, extra provider polls, changed
wire payloads or dummy activity are needed. Local bounded observation channels
can fail the fixture on overflow/disconnection; never block production queues or
silently drop a receipt. Exact per-session prefixes avoid confusing a reconnect
with the previous session's final drain.

The synchronous parent-facing controller can have this bounded API:

```rust
pub(crate) struct ProviderController { /* owns control peer and receipt reader */ }
impl ProviderController {
    pub(crate) fn step_until(
        &mut self, step: WorkerStep, host_deadline: std::time::Instant,
    ) -> io::Result<RoundProgress>;
    pub(crate) fn stop_until(
        &mut self, host_deadline: std::time::Instant,
    ) -> io::Result<RuntimeTaskResult>;
}
```

`RoundProgress` contains the actual worker polls, per-session Actor receipts and
scripted-service prefixes for that round, plus explicit pending obligations.
`RuntimeTaskResult` is the joined controlled task result, never an OS-child
status. Controller construction returns a separate runtime participant; the
participant owns the paused runtime, its ObserverScope and the blocking guard.
`stop_until` closes/wakes that participant and preserves the original task-result
propagation; timing out does not invent a successful join.

A complete tick is not sufficient to show that every runnable async participant
has settled at that instant. A truthful idle observation must come from **the
original select future actually returning Poll::Pending**, after its maintenance
and due work, with a phase identifying select-wait versus an in-progress async
write. A test-only `poll_fn` observer around that original future can report this
without waking/yielding the task or polling it an extra time. A pending write is
not an idle select. If work is published after such a receipt, its generation
invalidates that receipt and actual consumers must acknowledge the new prefix.
The existing actor `yield_now()` is preserved for fairness but proves nothing.
A future intentionally waiting for a protocol deadline must remain pending while
logical time advances toward that same deadline; record its phase, cause and
deadline instead of falsely reporting its command complete. Conversely, an
unexplained host I/O stall or an unconsumed announced prefix is not permission to
advance. The first link-drop controller should support only its known scripted
waits and bounded budget/capacity deferrals, failing explicitly on unsupported
states rather than becoming a general async scheduler.

## Driver order and manual Tokio progress

The existing coordinator calls `scenario_clock::step(Run)` before its loop work
(`coordinator.rs:2789`). Scope pumps peers at **T**, then advances by **2 ms**;
the coordinator next works at **T+2 ms**. Do not change this shared contract.
Consequences:

1. At a pump, snapshot successful parent control/data sends from the previous
   coordinator iteration and receipt counters for prior worker output.
2. Pump capture/privacy/speaker using their original source schedule and real
   Channels. Keep capture source timestamps; do not relabel a late block as new.
3. Drive the provider runtime up to the pump's current time, honoring original
   tickers and actual service work. It may process bounded prefixes, report
   backlog, and leave newly queued worker output for the next 2 ms tick.
4. Require processing receipts for work actually released this phase or an
   explicit original budget/backpressure deferral. The coordinator has not run
   yet, so **do not wait here for it to consume newly emitted worker output**.
5. Return from the pump. The existing Scope advances 2 ms; the coordinator runs
   and really receives/handles those output datagrams. Validate that prefix at
   the following pump. Deliberately deferred output is carried with its causal
   capacity/budget reason. Never claim a receive merely because send succeeded.

The provider observer must be able to adopt intermediate moments while catching
up to T. The service retains its original **1 ms rest timer**, and worker/Actor
retain **2/10 ms** tickers; step the provider runtime in at most **1 ms** increments
(or to an explicitly known earlier due deadline), without adding any worker tick.
This preserves the service's close/answer checks and 40 ms packet spacing instead
of repeatedly skipping a 1 ms timer in a 2 ms jump. Incoming parent IPC stamped T
must not be read by a worker tick at T-1: initialize the worker ticker at the
shared epoch and retain its 2 ms alignment. Freeze all participants at an admitted
moment while obtaining receipts. Startup alignment is part of the handshake,
not inferred from Ready. Bootstrap at the held epoch, observe the one real
immediate tick, and never manufacture a second tick when the first parent pump
uses that same epoch. A test must prove this order, including initial immediate
ticks and a reconnection created between later ticks.

**Paused Tokio is not manual by default.** Pinned Tokio 1.53.1 documents auto
advance when idle (`time/clock.rs:121–159`). It also explicitly documents that a
running `spawn_blocking` task inhibits auto advance; the current-thread blocking
scheduler increments/decrements that guard (`runtime/blocking/schedule.rs:25,47`).
Use one owned, host-bounded blocking control reader/condition wait for the life of
the fixture runtime. It waits without spinning and supplies Steps to the async
controller. A real stop/Drop channel wakes it; join it before dropping the runtime.
Do not call private Tokio APIs, run a busy yield task, let a guard finish between
rounds, or use a frozen Tokio timeout as the only host watchdog. `advance().await`
returns after advancing time; it does not itself prove worker/Actor/service work
completed. Receipts still decide when the next advance is legal.

Each host wait uses an absolute remaining budget, never a fresh timeout per
message. Preserve the original 3 s startup and 12 s conversation guards for this
fixture, starting each at its existing lifecycle boundary; shutdown uses the
caller's remaining shared host deadline. On a missing
receipt, stopped task, phase mismatch, counter regression or unsupported retained
work, fail explicitly, close/wake peers, then run the original abort/join and
actual child cleanup. Terminal records preserve pending/discarded work honestly.
A terminal Actor state is published before its drain (`session.rs:1022–1040`);
only its final wrapper can report task return. Runtime-thread join and OS child
exit remain separate observations.

**Explicit shutdown poll seam is required before migration.** There is a second
frozen-time problem beyond the outer stop loop: the worker reads `Control::Stop`
only in its 2 ms tick arm. After `Phase::Shutdown` freezes Scope, the next paused
Tokio tick cannot become due. A normal completed-poll hook alone cannot solve it.
Do not send an extra early Stop, replace Stop handling with an abort, advance just
one correlated clock, or pretend the worker consumed the real Stop.

The smallest proposed resolution is an explicit fixture-only poll permit:

```rust
pub(crate) enum PollPhase { Run, ShutdownDrain }
pub(crate) struct PollPermit {
    pub phase: PollPhase,
    pub at: Moment,
    pub host_due: Option<std::time::Instant>,
}
// Controlled tick source; normal source still delegates to the original ticker.
// Run permits require the original due 2 ms Tokio tick.
// Shutdown permits require a real, absolute 2 ms host polling deadline.
```

In Run, preserve the original interval, Skip behavior and every due time. In
ShutdownDrain, keep both protocol clocks at the last admitted moment and release
at most one bounded worker control tick per **2 ms host cadence**, with Skip
semantics and the coordinator's remaining shared cleanup deadline. The owned
blocking control reader can supply those permits without a paused Tokio timer.
Every permit still executes the original control-priority, budget and Stop code;
it does not directly read/consume a channel. Mark these receipts ShutdownDrain,
not additional conversation ticks or faster progress. Cancellation/Stop takes its
original path and the actual serve future must return before a terminal receipt.
No service packet/periodic conversation deadline is synthesized during cleanup.

This is an explicit fixture scheduling behavior, not a consequence of pure clock
injection. It changes neither normal runtime tick selection nor the already
threaded fixture. It needs owner review and a focused real-control/terminal test
before the migration can proceed. If retaining the original paused ticker even
in Shutdown is mandatory, the current frozen-Scope contract is incompatible;
leave the link-drop scenario on host time until a different reviewed cleanup
contract exists. The first preparation PR can remain useful without deciding or
migrating this shutdown mode.

The original provider-thread supervision loop checks its stop flag after a
**5 ms Tokio sleep** (`early_end.rs:648–649`). Copying that loop unchanged into a
frozen terminal phase would hang: setting an AtomicBool does not wake the sleep.
The controlled constructor must use its real stop/control notification to enter
the same original abort-and-join sequence without advancing logical time. Keep
the old threaded constructor and its 5 ms loop untouched. Preserve the current
order: abort future service sends before `rig.shutdown`, keep worker/peripheral
peers available through shared cleanup, then stop and join the runtime thread.

## Reviewable preparation split

1. **Voice clock preparation, no existing fixture migrated.** Files:
   `scenario_clock.rs`, new `provider_fixture.rs`/focused tests, conditional clock
   imports in `provider_worker.rs` and `provider_link.rs`, one named capture-now
   site, voice dev dependency. Keep ordinary constructors/host fallback and
   existing Scope pump semantics. Test pure reads, thread isolation, observer
   adoption, paired Tokio/monotonic samples and existing LinkClock refusals.
2. **Gemini fixture clock and progress preparation, no scenario migrated.** Files:
   `gemini/src/session.rs`, `gemini/src/testing.rs`, default-off feature metadata
   and new focused tests. No `recovery.rs` or existing assertion changes. This
   needs explicit ownership review. Preserve default setup/session factories;
   prove custom age behavior and every counter at actual processing/flush sites.
3. **Voice worker/channel progress preparation.** Files: conditional probes in
   `provider_worker.rs`, `transport.rs`, new fixture/controller tests. Preserve
   all tick select branches, budgets, cancellation/error returns and output
   ordering; validate full tick vs Deferred vs notice handling separately.
   Build the bounded runtime guard/control controller, including the explicit
   ShutdownDrain permit decision, without switching an old test. This can be one PR with step 1 only if the resulting diff stays small.
4. **One vertical migration.** New early-end fixture helper plus explicit
   constructor/mode selection for the original link-drop test. Existing shared
   threaded helper, other early-end tests and all original assertions remain.
   Reuse common scenario/service definitions; do not fork product behavior into
   a simulation. Keep the original 6 s logical run, 100/150 ms close/answer delays,
   12 packets, 2/10/40 ms cadences, IPC and real child cleanup.

Literal byte-identical original source is possible for existing test bodies and
unowned files, and should be checked. Inserting a capture seam/progress hooks into
the real worker/Actor necessarily changes those source files. Do not claim that
all production source bytes are untouched. The review obligation is a mechanical
mapping showing normal builds retain the same expressions, samples, comparisons,
branch order and waits, with default-off/cfg instrumentation absent; where a
shared helper is extracted, keep the original wrapper and demonstrate exact
reversal. This report does not prove generated-code equivalence.

## Regression and evidence plan

- **Clock coherence:** stationary reads cause no work; explicit paired advance
  accepts the normal LinkClock bracket; advancing only one domain still fails;
  future/regressed/over-uncertain samples still fail. No wider clock limits.
- **No auto advance:** with a real parked owned blocking guard and a timer due,
  let actual host synchronization complete and verify Tokio remains fixed;
  advance one admitted moment and observe timer work. Release/join guard on
  success, failure and unwind. Avoid elapsed-host speed assertions.
- **Intake priority:** send real valid control/data to the actual worker. Fill
  the 16-control slice, retain the actual data command, verify completion reports
  Deferred and no input handoff; process next original tick, preserving authority
  before handoff and the 100 ms expiration boundary. Keep existing tests unchanged.
- **Processing vs enqueue:** hold the Actor or service at a real local barrier;
  successful InputSender enqueue/service send/worker send must not satisfy the
  relevant consumed/processed target. Release the barrier and require exact
  receipt counts. Exercise a full/blocked output queue and an expired/discarded
  command, not only the happy path.
- **Capture age:** exercise one microsecond below and at the existing 2,500 ms
  boundary, retained-audio patience, reversed acquisition order, and held input
  across logical advance; both InputSender and Actor must use the supplied domain.
  Normal constructors continue to use host acquisition time. Do not weaken checks
  to accommodate synthetic stamps.
- **Loop boundaries:** a notice queues output but does not advance `data_sent`;
  output sends on the following original tick. Tick/notice/select-pending/Actor
  return are distinct. Test an async write blocked mid-handler and an Actor
  disconnected while buffered output remains. No pending receipt is forged.
- **Peer scheduler:** prove initial immediate ticks, a 10 ms capture boundary,
  40 ms answer packet spacing, close at the existing 100 ms deadline, and 20/40 ms
  backoff with actual setup completion. At T, newly emitted worker output may be
  consumed only in the following coordinator phase; no same-phase wait deadlock.
- **Terminal evidence:** freeze both clocks with the next Tokio tick in the
  future, send a real Stop, and prove a host-paced ShutdownDrain permit executes
  the existing intake and the real worker returns; no logical time advances.
  Missing control closes boundedly; Stop alone is not worker return; Actor terminal state is not final drain; thread join is not
  sentinel child exit. Preserve existing kill/reap and task-panic propagation.
- **Migration gate owned by root:** compare exact compiled selected identities
  and ignored flags; all original assertions and attempts retained; paired source
  and helper hashes; matched real before/after invocation, raw logs and actual
  exit statuses. Run local fmt/strict clippy/applicable/full package checks and
  native ARM qualification as root schedules them. No speedup or green claim is
  made by this read-only design.

The direct `a_name_note_told_with_a_request_ended_early_does_not_survive_it`
(`early_end.rs:1541`) remains a separate candidate: two iterations, ordinary Tokio
negative windows of 150/100/150/150 ms, no worker, LinkClock, child or coordinator
Scope. It can be reviewed independently for paused Tokio with the same two paths
and assertions. Do not include it in this link-drop preparation or change it now.

## Source hashes

SHA-256 of the exact files inspected. Previously audited entries match the prior
remaining-Voice audit; new reads add the Supervisor source and pinned Tokio guard
implementation without editing any of them.

- `lampOS/crates/voice/src/coordinator/tests/early_end.rs`: `128632acb07813f234b72163a2c248ba9522bb29c58b0d66dbbac336acda4edf`
- `lampOS/crates/voice/src/provider_worker.rs`: `9557efa06a257d0d193c838ced903bc5381a94208c96a9134ae95d58c145556c`
- `lampOS/crates/voice/src/provider_link.rs`: `b44639b8cddeacd8432f0870dd69d4ff29028b4855ef711cd64a6002584b499b`
- `lampOS/crates/voice/src/scenario_clock.rs`: `7eb0215c649311dd9303d3043c46d8c50af2d165ab677e6c1c37720a79fc7cef`
- `lampOS/crates/voice/src/transport.rs`: `49308fd4a3e19ba1179a0410785e85912ed018757d79872497f40517529eae73`
- `lampOS/crates/voice/src/provider_flow.rs`: `d32f128506d1e5ab7dadaf422cdde9e308442c4409d14c1db35f22c443f74720`
- `lampOS/crates/voice/src/coordinator.rs`: `2eb5bf39b0536df3f978bc1d3581d2220acd227fd57f0336594d1f62861956b2`
- `lampOS/crates/voice/src/process.rs`: `f9f1173bd1d33ea9f5ebc9ed3ed6247b802dbf299d017dabef90f93eca074d54`
- `lampOS/crates/gemini/src/session.rs`: `2856a8d1910fb2b1d46930cc7db17e695543ddc9524f55c8f36a9250ffc0438b`
- `lampOS/crates/gemini/src/recovery.rs`: `6342d3915fed01e2907ed5d5647cc99f0d2f267b7ccc33bb27ab35d040dbda58`
- `lampOS/crates/gemini/src/testing.rs`: `0a13daf42afc0024e8ed7473b1f323b4409ef0ef8ccd35bb8be90c9b8fa5cb6f`
- `lampOS/crates/gemini/src/lib.rs`: `b22f1ea07bcf47e8a9f86710658b1c5d8b879ab7b14c691ee51f7fb53645e76e`
- `lampOS/crates/voice/Cargo.toml`: `0764d2cb4d6c713bf08058420b9a6c9c84480951b4082e8ecce847b563dbcafc`
- `lampOS/crates/gemini/Cargo.toml`: `00336dabd32df739b068669cd5a861590192ae85fd7af824db90429593c86838`

Pinned local dependency sources (read-only; no network documentation consulted):

- `/Users/autonomous/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.53.1/src/time/clock.rs`: `a4769f71bbf20a0df3ad52f477f3118643ea0946148839f2010a6375336d5295`
- `/Users/autonomous/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.53.1/src/runtime/blocking/schedule.rs`: `a3402f8acb6faebd1e95660d8f5f607890ff89415ee7f8358761a3a7a87a2845`
- `/Users/autonomous/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.53.1/src/runtime/time/mod.rs`: `3123856cc9f93bb319c3c428f4e14f1b6bddad4344240150c616079004e41058`

Final source/design check at **2026-10-16 01:46:00 +0700** (from `date`).
