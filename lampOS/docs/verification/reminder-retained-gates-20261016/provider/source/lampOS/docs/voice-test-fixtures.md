# Voice fixtures on a loaded development host

Task [#739](https://github.com/autonomous-ai/lampOS/issues/739) concerns host
tests, not a change to the lamp's timing policy. `scenario_clock` and the
fixture helpers are compiled only under `cfg(test)`. There is no runtime
flag, environment setting or wider production deadline. Ordinary builds
continue to use the existing monotonic clock.

## Stepped scenarios

The six endpoint cases with an in-process classifier explicitly select
synchronous peers:

- `endpoint_session::complete_verdict_ends_early_but_resumed_speech_keeps_the_turn`
- `endpoint_session::verdicts_after_the_result_timeout_fall_back_on_every_turn`
- `endpoint_session::rapid_resumes_never_end_the_turn_early`
- `endpoint_session::a_complete_verdict_ends_at_one_second_and_its_answer_plays`
- `endpoint_session::a_worker_failure_falls_back_through_recovery_and_then_resumes`
- `endpoint_session::duplex_complete_verdict_ends_at_one_second_and_its_answer_plays`

The pilot fixture makes its mode explicit at every scenario. Its 32
single-cycle scenarios use stepped peers, including the original ring and
SIGTERM regressions and the deliberate privacy-expiry regression. Rapid
mute/reopen retains real-time peers because it exercises actual service
privacy waits across controller lifetimes. The stalled metadata writer
remains a separate real thread while its synthetic peers use stepped time.

The production coordinator, Unix channels, packet identities/sequences,
privacy gate, input admission and speaker `BoundaryGuard` remain in use.
Before each coordinator iteration, every emulated peer completes one
bounded step; scenario time then advances by 2 ms. A clock read never
advances time or runs a peer. Capture retains its 10 ms frame schedule.
The pilot privacy peer's requested 5 ms cadence becomes 6 ms on this
2 ms grid; permitted freshness remains strictly below 50 ms. The endpoint
peer's 10 ms privacy schedule is unchanged.

The scope belongs to one thread and cannot be sent or shared. Concurrent
scopes are independent; newly spawned threads retain the host clock.
Nested scopes and recursive pumps are rejected. A callback panic releases
its state and restores the host clock; an old guard cannot clear a later
scope. Session lifetime, peer schedules, envelope timestamps and age checks
share one clock. The 50 ms privacy, 60 ms reopening and 100 ms channel
boundaries remain unchanged.

Real `ProcessClassifier` workers retain real-time peers. Their finite ordering
fixture explicitly selects the existing per-instance test privacy allowance
of 100,000 us and a 12-second real conversation watchdog. This covers missing
model, hung worker, loading worker and duplex missing-model scenarios, plus
the existing ignored real-model provenance case. The six in-process stepped
scenarios and all three endpoint mute/cleanup scenarios retain the default
50,000 us gate. Every ordinary build retains that default too.

The allowance is capped at the existing 100,000 us IPC ceiling; it changes
neither envelope/source timestamps nor input admission. Worker startup,
result, hang, recovery and teardown deadlines remain real and unchanged.
The watchdog starts immediately before conversation, excluding startup and
cleanup. A host-clock guard rejects an active logical scope before creating
the real endpoint child. There is no new Ready wait: missing and still-loading
workers are intentional states in these tests. This is bounded scheduling
tolerance, not a qualification of arbitrary load or the lamp's 50 ms margin.

The stepped endpoint entry accepts an in-process classifier double explicitly,
including its scripted failure/recovery timer. Original functional assertions remain.
Pilot speaker-stall and revocation assertions now measure policy time;
the separately printed conversation/cleanup elapsed duration remains host
time. This does not qualify every other timed test as immune to scheduling.

The endpoint fixture's synthetic capture stream has one immutable source
origin and a sample block every 10 ms. Both peer modes retain each block's
scheduled acquisition timestamp when delivering a backlog. A delayed thread
therefore cannot manufacture a gap in these continuous synthetic samples.
Envelope delivery, the real classifier and unscoped policy clocks stay real;
old source audio is never refreshed to the delivery time.

Two controlled-clock regressions send through actual Unix channels: a 60 ms
delivery pause preserves the original contiguous sample stream, and fresh
envelopes carrying source audio aged 98/100/102 ms retain the unchanged
admission result (accept/reject/reject). Failed scenarios print bounded
after-send source-gap, delivery-gap and source-age diagnostics. These are
producer observations, not coordinator consumption ages or lamp latency.
The real-worker fixture also retains bounded privacy producer diagnostics;
they cannot distinguish coordinator consumption age from host scheduling.

The pilot fixture uses the same continuous source model in both modes,
with its origin set when the capture peer receives `StartCapture`. Each
bounded peer step sends at most 64 due blocks, retaining their original
10 ms acquisition schedule. Three deterministic checks cover a 60 ms
delivery pause, input admission at 99,999 us versus refusal at 100,000 us,
and rejection of an actual dropped block or source gap. Rapid-reopen
failures print each unsuccessful cycle's diagnostics before the service
assertion, including the largest capture send-attempt gap and oldest source
age at an attempt. These include failed send attempts; they do not measure
coordinator consumption. Its real privacy peer and 50 ms bound remain
unchanged.

## Explicit transport success observations

Test-only `Channel::install_probe()` attaches a bounded observer before any
send or receive attempt, including an attempt that fails or finds no packet.
Ordinary channel, worker-channel and reference-channel constructors remain
unobserved. No probe API or state exists in non-test builds.

A probe records only an actual successful kernel send or a fully decoded and
validated accepted receive, after the original sequence/receipt commit. It adds
no I/O, receive, clock sample, pump, advance or await. Original receive samples,
boot/order/age validation, startup grace, stall handling and gap acceptance stay
unchanged. Failed sends and refused/empty receives do not increment observations.
Probe faults do not change the transport operation's actual success or error.

Snapshots retain an allocation identity for that exact channel, including after
the channel and probe drop. Each direction records its configured boot, accepted
count, actual last sequence, consecutive prefix from sequence 1, and first gap.
An accepted gap stops that prefix permanently; a highest sequence or count alone
cannot prove complete consumption. State has fixed size and stores no event log.

Publication and snapshot reads are nonblocking. A snapshot makes one bounded
consistency attempt; `Busy` supplies no receipt and never implies completion.
Counter/publication overflow, invalid state and duplicate/late installation
latch failure without further successful evidence. Snapshots are historical
transport observations: they prove neither worker handling, Actor/service
progress, downstream completion nor child exit. A retained snapshot does not
validate the probe's later health. A later controller must check the live probe,
retain causal targets and use its own real watchdog; this preparation adds no
controller and migrates no scenario.

## Real-provider ordering scenarios

The EarlyEnd module runs the actual provider worker, Gemini supervisor and
a scripted WebSocket service on a real Tokio runtime. Its ten coordinator
scenarios run eleven `Setup` conversations (the option comparison runs
twice). Every conversation explicitly selects a per-instance privacy
freshness bound of 100,000 us, matching the existing worker IPC ceiling,
and a 12-second real conversation watchdog. This is bounded host scheduling
tolerance, not an arbitrary-load guarantee. The test-only constructor
accepts positive bounds at most 100,000 us; it and the override field do not
exist in normal builds. Every ordinary gate remains at 50,000 us, including
unrelated tests.

Capture/privacy/speaker producers start after the actual provider Ready
barrier. Slow provider setup cannot fill their queues with old samples.
The synthetic capture source retains its immutable 10 ms acquisition
schedule even when delivery is delayed. Original source timestamps, packet
order, 100 ms IPC/capture bounds, 60 ms stable reopening and the shared
300 ms shutdown remain intact. The 12-second watchdog begins immediately
before conversation; it excludes the existing three-second provider
readiness wait and subsequent cleanup. The fixture's sleeping child
processes still require forced termination, as before; their teardown is
not claimed as cooperative shutdown evidence.

The provider and scripted service share one current-thread Tokio runtime.
The fixture obtains the service's abort handle within the existing Ready
budget, then requests its cancellation before coordinator shutdown sends
provider Stop. Capture, privacy and provider peers retain their normal
cleanup order and budget. Both Tokio tasks are joined; unexpected task
panics propagate, and only cancellation requested by the fixture is accepted.
This prevents a closing provider connection from racing further scripted
answer sends, without swallowing a background failure.

The separate Tokio name-note test exercises the Gemini supervisor directly;
it does not construct `Setup`, capture peers or a privacy gate. It retains
its original continued/abandoned request flow. Both fixture entrypoints
reject an active scenario clock before starting child or provider resources,
so provider, envelope and coordinator clocks remain real and coherent.

Six privacy regressions cover exact observation, state and continuity
boundaries; per-instance/default isolation; immediate mute and 60 ms
reopening; invalid configuration; and future, stale and reordered
observations. Two EarlyEnd fixture regressions prove rejection of a scoped
clock and continuous source timestamps after delayed delivery, including
actual input admission at 99,999 us and refusal at exactly 100,000 us.
Three further checks cover requested cancellation, unexpected cancellation
and propagation of a background panic.
Every original scenario assertion remains, including the lone-name case's
one request with audio blocks 76 through 290, one early end, no abandoned
request and 48 speaker chunks. These tests do not measure the lamp's 50 ms
privacy response.

## Provider body observations

The explicit test-only `provider_worker::observation::coordinated` factory
binds an owned observation before the original provider body is first polled.
Ordinary constructors stay unattached. Both entrypoints use the same original
body, raw clocks, 2 ms Skip interval, control/input/output budgets and channels.
No existing real-provider scenario selects this observation yet.

The intake records an explicit Stop or authority revocation only after its
original handler has stopped intake. A generic error that clears intake is
neither cause. A sent or accepted datagram is not a handled command: the new
real-channel cases cover a queued Stop before polling, the original full
16-command control slice, actual permission/generation changes, and error
cleanup. Initial denied authority is not relabeled as revocation.

A separate wrapper records the original body future's actual successful or
failed return and preserves its Result object. Never-polled Drop and Drop after
entry remain separate from a return; neither identifies cancellation or panic.
The actual task owner must await its JoinHandle and keep its real result.
Component tests distinguish handler receipt from future return, join a
requested cancellation, and verify an unchanged panic payload. Held connection
attempts expose their own future-Drop receipt; that receipt is not a private
task join. New asynchronous waits share one immutable 10-second host deadline
per test, on an unpaused runtime. Failure cleanup requests cancellation without
claiming that request as a join.

Each worker identity retains its Arc allocation, and each snapshot reads one
bounded atomic state. No observation reads a clock, waits, performs I/O or pumps
work. Invalid publication latches an observation failure without changing the
real worker Result. There is no growing counter, event log or retry. An earlier
snapshot is historical evidence, not a lease on later health.

These receipts establish neither a completed bounded worker poll nor downstream
Actor/service progress. They do not establish Actor, runtime, thread or child
exit. Successful IPC, handled Stop, body return, task join and actual child reap
remain distinct boundaries. Clock admission, controlled scheduling and the
original early-end scenario migration remain separate work.

## Actual processes and cleanup

Child startup, exit observation and the shared shutdown watchdog keep real
host deadlines. Scoped protocol deadlines, including camera teardown, use
logical time. Stepped pilot children are
actual `cat` processes: a bounded echo proves that each has run and read its
input before scenario time starts. After closing input, bounded
`waitid(WNOWAIT)` observes successful exit of that exact PID. Nonzero exit,
signal death, timeout and an already reaped child are errors. The production
supervisor still owns the actual reap and exit-status checks.

Shutdown pumps receive the coordinator's existing shared 300 ms real
deadline (750 ms for diagnostic cleanup). They neither grant a fresh budget
per child nor advance scenario time or bypass the final receipt drain.
Fault injection cannot advance the shutdown clock. SIGTERM still signals
the actual test process. Real wall-clock watchdogs remain; logical scenario
durations and revocation intervals are not host or lamp latency measurements.
The shared real deadline still terminates a child that has not exited; these
stepped cases do not measure the camera's separate teardown deadline.

Clock tests cover host pauses, independent scopes, unscoped threads,
reentrancy, panic cleanup, callback-error recovery, watchdog expiry before and
after a peer step, frozen shutdown time and unchanged channel expiry.
A stepped coordinator regression advances exactly 50 ms after an accepted
privacy lease, then queues a fresh mute on the real channel. The existing
pre-drain freshness check must fail without a restartable physical-mute
token. Consumption also includes the ordinary 2 ms step and the lease's prior
age; this proves expiry and ordering, not exact consumption age. Actual
cleanup and reaping are still asserted.

The stalled-metadata scenario submits an event through the normal logging
path and waits for the actual writer's entered receipt before starting
children or scenario time. The writer stays blocked on its own real channel
through audio delivery, actual SIGTERM and coordinator cleanup. Separate
host instants measure signal send through verified child exit/reap, retaining
the original limit of less than one second. `finish_logging` still waits on
the real 500 ms bound; its test allows 250 ms of host scheduling margin and
requires the unacknowledged report. Logical time cannot satisfy either
measurement. Normal release is checked, and a later real writer-finish
receipt confirms it drained after release without changing the earlier
unacknowledged report.

A test-only release guard also unblocks that writer during assertion unwind.
A deterministic regression unwinds an entered real stalled writer with a
typed panic, preserves that panic and requires its eventual finish receipt.
It does not suppress writer failure or turn the stalled report into success.

## Motor setup

The motor fixture executes and reaps an inert preflight arm of its exact
frozen wrapper before starting the real supervisor protocol. It requires
the one-byte marker and successful exit; missing/wrong marker, nonzero exit
or timeout fail and reap the owned child. Preflight does not start a worker,
bind its sockets, open a device or establish motor readiness. The frozen
executable's identity and hash remain checked.

This separates a measured fresh-script execution cost on Fourth's Intel
Mac from Ready/authority tests. It does not explain every historical timeout
or qualify cold production startup. Original two-second fixture predicates
and production startup, progress, inhibit, permit and teardown bounds remain.
Failure output includes child phase, connection/Ready/reap state and the
observed parent poll gap.

The crash-preserves-voice fixture creates its active reply after config
freezing and unrelated child setup. While servicing the optional motor it
refreshes that same owner's existing 500 ms capture/admission lease, as live
capture would. It neither creates a replacement owner nor enlarges the
lease. Original audio-dispatch and cleanup assertions remain.

## Qualification and limits

Run from `lampOS/`:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --locked --offline -p lamp-voice -- --test-threads=1
cargo test --locked --offline -p lamp-voice-eval -- --test-threads=1
cargo test --locked --offline -p lamp-memory -- --test-threads=1
cargo build --workspace --locked --offline
```

The issue also requires ten full serial `lamp-voice` runs on the Mac beside
a Podman ARM64 build, and a green exact-head native ARM64 workspace workflow.
Source/build identity, raw results, load boundaries and failures belong in
the task's verification folder and Fourth's log. An isolated passing case
is not a passing full gate.

No lamp is accessed. A privacy peer's send-side gap is not the coordinator's
consumption age. The real 50 ms bound may still be tight on a loaded, hot
lamp; host fixtures cannot settle that question. First must measure physical
observation through coordinator consumption on the lamp with its actual
services and temperature, without widening the bound on synthetic evidence.

## Endpoint fixture cleanup after an expected failure

Task [#860](https://github.com/autonomous-ai/lampOS/issues/860) follows the two
endpoint-session `LEAK` warnings in full ARM64 run
[38737583302](https://github.com/autonomous-ai/lampOS/actions/runs/38737583302).
Both tests passed their assertions while a fixture could still hold inherited
stderr. That run remains the original evidence; a passing assertion does not
establish clean child teardown.

The unconfirmed-exit scenario retains its 1,500 ms worker grace and the
coordinator's 500 ms teardown wait. It first checks the original `Pending`,
unreaped record, non-restartable error and refusal to reopen service. Only
after those assertions does a fixture guard wait for the existing supervisor
to confirm actual reaping, with a separate five-second host cleanup bound.
This wait does not alter the recorded runtime result or grant the runtime a
longer teardown deadline.

The still-loading worker uses `exec sleep 6`, so the supervisor owns the
sleeper itself instead of a shell with a sleep descendant. It still sends no
Ready during the same four-second session, and the original 1,200 ms silence
fallback assertion remains. It uses the same final cleanup guard; stderr
remains inherited and production process supervision is unchanged.

Before handoff the guard owns the endpoint. Its destructor closes any
endpoint still in that slot before waiting. Once the endpoint moves into a
helper, that helper drops the classifier on return or unwind before the
caller's guard waits. A cleanup timeout or unreaped result fails a normal
test. During an existing panic it prints the cleanup failure and preserves
the original panic rather than aborting with a second one.

One real-child regression covers both a panic before handoff and a panic
inside the receiving helper. Each child must report Ready while its monitor
is still pending; after the typed panic is caught, a zero-duration monitor
wait must report Reaped, with one spawned and one killed worker. The zero
wait cannot finish cleanup for a guard that returned too early. This is
fixture lifetime evidence, not a new product shutdown guarantee.

The source change alone does not qualify the fix. Required evidence remains
the original assertion and identity comparison, this panic regression and
the two original cases without nextest `LEAK`, the crate/dependent gates and
a green exact-head native ARM64 run. No lamp or production-worker behavior
is measured by these fixtures.
