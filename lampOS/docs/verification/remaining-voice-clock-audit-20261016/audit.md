# Remaining Voice clock work in the frozen #823 set

Source-only audit at **2026-10-16 01:15:19 +0700** (from `date`). The selected
set is exactly the original **63 identities with binary `lamp-voice`**. At
`8a7521661cb06997a18eed32ead13d8d9ee81fd9`, **37 use ScenarioClock, 24 retain
host time, and 2 are pure checks**. None uses paused Tokio test time. The
source classification is separate from the supplied native results added
below at **2026-10-16 01:17:28 +0700** (from `date`).

The highest-value next owned vertical slice is the original early-end
link-drop case, after a small default-preserving provider clock and completed
poll seam. Its current six-second scenario cannot safely be accelerated by
flipping the coordinator clock alone. The smallest isolated candidate is the
direct Supervisor name-note test: its negative windows already use Tokio and
it has no child, LiveRig lease or LinkClock. It still needs a focused gate.

The exact names, constructors, current/original function hashes, current file
hashes, attributes and remaining wait profiles are in
[selected-63.json](tests-823-voice-remaining-evidence/selected-63.json).
[Source hashes](tests-823-voice-remaining-evidence/source-hashes.json) cover 26
files and all three frozen inputs. Every audited source file matches both
HEAD and the known `6b08a70d5` base; this finite-fixture branch added no Voice
source changes. No source edits, builds, tests, Git mutations, network,
device actions or #866 changes were made. Native results are imported from
the root's completed #889 evidence; this audit ran no tests.

## Completed native #889 observations

Run **38780759392** contains all original 63 Voice identities exactly once:
**61 PASS and 2 LEAK**, with no missing or duplicate identities. The complete
run has 4951 unique outcomes, 4949 PASS and those same two LEAKs. CI reported
green; that does not mean leak-free.

| Frozen Voice group | Identities | Outcomes | Sum of reported case seconds |
|---|---:|---|---:|
| Pilot faults | 34 | 34 PASS | 10.143 |
| Endpoint session | 13 | 11 PASS, 2 LEAK | 23.743 |
| Early end | 11 | 11 PASS | 61.321 |
| Selected provider worker cases | 5 | 5 PASS | 42.292 |
| Total selected Voice set | 63 | 61 PASS, 2 LEAK | **137.499** |

These are sums of individual nextest durations across **four concurrently
running ARM64 shards**, not a standalone serial elapsed time. The complete
original 123 set sums to 619.655 s on that same boundary. Neither number
establishes the under-two-minute serial group target. Historical baseline
columns are retained for provenance; differing source/runs/load mean this
is not a matched before/after speedup claim.

The 37 source-clocked cases are all PASS and sum to 3.631 s (31 pilot:
1.547 s; six endpoint: 2.084 s). The two pure checks sum to 0.010 s. The
24 host-time cases sum to 133.858 s, including both LEAK durations. Source
clocked count and passing duration are separate facts, not a completion
claim for the whole task.

The known endpoint outcomes tracked under #872 remain explicit:

- `a_worker_still_loading_leaves_the_baseline_window`: **LEAK, 4.881 s**, shard 3/4.
- `unconfirmed_endpoint_exit_fails_mute_cleanup_and_never_reopens_service`: **LEAK, 0.715 s**, shard 1/4.

For prioritization, the assigned original early-end link-drop case was PASS
in 6.385 s, and the 20-second provider capacity case was PASS in 24.029 s.
Those are current observed durations, not projected savings. The source
recommendations below still require preserving their actual clock and I/O
boundaries.

[Native per-identity columns](tests-823-voice-remaining-evidence/native-889-selected-63.tsv)
and [group/route totals](tests-823-voice-remaining-evidence/native-889-totals.json)
are derived from the root's `889-full-results.json` and
`889-full-original-123.json`. Every selected row was checked against both;
input hashes are retained with the source hashes.

## Exact current routing

| Selected path | Identities | Current clock |
|---|---:|---|
| Pilot single-cycle scenarios | 31 | ScenarioClock, explicit `Fixture::Stepped` |
| Pilot cross-cycle mute/reopen | 1 | Threaded host time |
| Pilot pure boundary helpers | 2 | Explicit values; no timed scenario |
| Endpoint in-process classifier scenarios | 6 | ScenarioClock, `PeerMode::Stepped` |
| Endpoint finite real-process scenarios | 4 | Host time, explicit process/privacy route |
| Endpoint mute/exit scenarios | 3 | Host time and actual child exit |
| Early-end coordinator/provider scenarios | 10 | Host coordinator, three peer threads, separate unpaused Tokio runtime |
| Early-end direct Supervisor name-note case | 1 | Ordinary unpaused `#[tokio::test]` |
| Five selected provider-worker cases | 5 | Ordinary unpaused Tokio tests plus host `Instant`/monotonic time |

The 63 identities still include 55 that start actual OS children. This count
does not imply those children perform the mocked audio/provider work. The
other eight are the two pure cases, five in-process provider-worker cases and
one direct Supervisor case.

The six stepped endpoint cases are:

- `a_complete_verdict_ends_at_one_second_and_its_answer_plays`
- `a_worker_failure_falls_back_through_recovery_and_then_resumes`
- `complete_verdict_ends_early_but_resumed_speech_keeps_the_turn`
- `duplex_complete_verdict_ends_at_one_second_and_its_answer_plays`
- `rapid_resumes_never_end_the_turn_early`
- `verdicts_after_the_result_timeout_fall_back_on_every_turn`

Both `missing_model_session_ends_on_the_unchanged_silence_fallback` and its
`duplex_` counterpart remain **real-process host cases**. They launch the
scripted endpoint that writes a missing-model error and exits 2. They are not
classifier doubles (`endpoint_session.rs:982,1656`). This corrects the
preliminary 39-logical/22-host count; the verified final count is 37/24.

## What the merged clock already covers

`scenario_clock.rs:1,124,161` defines one test-thread-local scope. Reads do no
work. Every normal step runs the registered peers and then advances exactly
2 ms; a real watchdog bounds the operation. Other threads keep their actual
clock. Shutdown steps preserve logical time and receive the coordinator's
existing real shared deadline. `coordinator.rs:92,2789,4776` uses this domain
only for scoped tests; the normal path retains the original 2 ms sleep.

Pilot `run_in` enters the scope only after actual child startup barriers,
then pumps privacy and the other peers (`pilot_faults.rs:1354,1414,1470`).
`Peer::close_input` observes the owned child's real successful exit with
`waitid(WNOWAIT)` before another logical step; the supervisor still performs
the real reap (`pilot_faults.rs:540`, `scenario_clock.rs:248`). Capture,
speaker pacing, camera schedule, leases, policy deadlines and event-triggered
faults therefore already run in scenario time for those 31 identities.

The six endpoint doubles use the same scope and synchronously pump all four
peers before advancing (`endpoint_session.rs:774,819,827`). Their sample-count
policies and classifier delays/recovery remain intact. They still have one
mandatory real wait: four sentinel children execute `sleep 60`, cannot obey
the mocked Stop, and consume the shared **300 ms** cleanup grace before real
kill/reap (`process.rs:80`, `endpoint_session.rs:911`, `coordinator.rs:686`).
The 60 seconds is keepalive, not the normal test duration. This wait also
remains in each host finite endpoint and early-end run.

The stepped metadata/SIGTERM case intentionally blocks a real writer before
running. It still consumes the real **500 ms `FINISH_WAIT`** while the writer
cannot finish, then releases and confirms it. Its 2 s entry receipt limit is
a guard; real signal delivery and child reap stay observable
(`pilot_faults.rs:2465,2533`, `service_trace.rs:24,175`). Do not describe all
37 clocked cases as free of wall-clock waits.

## Mandatory waits still in host time

### Early-end

Ten frozen coordinator identities call `Setup::run_with_privacy_tolerance`,
directly or through `resumed_at_three_tenths`. The helper rejects an active
ScenarioClock **before spawning anything** (`early_end.rs:546,587`). It runs
finite 4/5/6-second sessions; the early-vs-normal comparison runs two
5-second sessions. Capture and privacy follow 10 ms schedules, the service
uses its original answer delay and 40 ms packet cadence, and 200/600 ms
endpoint rules remain sample-count policies. The 350 ms playout setting is
buffered PCM, not an additional unconditional timer.

The provider and fake service run on their own OS thread and current-thread
Tokio runtime; capture/privacy/speaker each use another thread
(`early_end.rs:440,615`). The 3 s Ready/task-publication timeout and 12 s
conversation timeout are failure guards. The 20 ms dial timeout, 5 ms stop
check, 1 ms service/peer polls are polling cadence, not extra seconds to add
to the finite lifetime. Providers are Ready before sample producers start;
scripted sends are cancelled before ordinary Stop, followed by real thread
joins and panic propagation (`early_end.rs:689–754`).

For the assigned link-drop identity (`early_end.rs:1224`), retain the
six-second session, first-link close **100 ms after End**, second answer
**150 ms after End**, all original block lists, two connections, 48 played
speaker chunks, no premature playback and the failed/unheard first request.
The source now also has a second link-drop test after a continued request;
it is outside the frozen 63 and must not be silently omitted from broader
qualification after changing their shared helper.

The remaining early-end identity,
`a_name_note_told_with_a_request_ended_early_does_not_survive_it`
(`early_end.rs:1541`), bypasses the coordinator and worker loop. It has two
branches, each with 150/100/150/150 ms silence drains. Those configured
windows total 1100 ms; they are unpaused Tokio waits today, with a 3 s guard.
Its service is in-memory Tokio duplex I/O (`gemini/src/testing.rs:1,37`),
and capture stamps use host `std::time::Instant`.

### Five provider-worker identities

| Identity suffix | Required elapsed behavior today |
|---|---|
| `twenty_second_answer_through_a_five_second_reply_buffer_follows_capacity` | 20 s modeled 24 kHz audio plus the full 4 s hold after 5 s; 5 s buffer; original 3 s network stall, 200 ms non-burst gaps and five-message burst; overlapping intervals must not be summed |
| `kept_start::with_no_first_answer_the_patience_ends_in_the_strict_timeout_and_no_end` | Twenty 10 ms input blocks; 60 ms quiet drains; 2500 ms kept-start patience then strict 2000 ms barrier; final 20 ms negative receive |
| `resumed_question_survives_a_silent_service_and_the_assumption_is_reported` | 2000 ms AssumeAfterQuiet barrier for each of Legacy and Typed, retaining the >=1900 ms assertion |
| `review_2026_10_14::review_a_name_heard_after_a_requests_sound_ended_does_not_let_out_its_answer` | Wait to end +1000000 us +4000000 us age, real bus publication, then 200 ms pump |
| `review_2026_10_14::review_sound_a_name_heard_as_a_request_began_lets_out_the_answer_the_rule_would_hold` | Wait to begin +200000 us +4000000 us age, real bus publication, then 200 ms pump |

`LiveRig` spawns an in-process worker task over local IPC, not an OS process
(`provider_worker.rs:5195`). Its `pump`, silence detection, modeled speaker
and elapsed assertions use host `Instant`; authority, capture/admission
leases and transport stamps use host monotonic time. Worker/Gemini timers
are Tokio timers. The 3/5 s observation guards and 1/2/5 ms polls are not
nominal durations (`provider_worker.rs:5336–5413,7860–8093,8740,9058`).

For the two name tests, keep the real bus, global `BUS_ROOT` isolation and
serialization. `NameIn` checks supplied monotonic time every 20000 us and
drains at most eight messages (`bus_out.rs:216,256`). Publication success
proves delivery, not that the worker consumed the message; a future clock
migration needs a consumption/poll boundary before judging the answer.

### Real endpoint and cross-cycle pilot cases

The four finite real-process endpoints retain host session durations:
missing-model (baseline and duplex) 4 s each, loading 4 s, hung-worker 6 s.
The loading command's `sleep 6` is deliberately longer than its session;
cleanup can end that child earlier. The hung case exercises 700 ms hang and
100 ms restart backoff inside the run (`endpoint_session.rs:1040,1413`).

The three mute/exit identities end from a privacy event, not a finite
session sleep. Endpoint `sleep 60` ignores closed input, so normal 300 ms
grace is consumed before actual kill/reap. The unconfirmed-exit test sets
1500 ms grace and requires the coordinator's `max(remaining,500 ms)` wait
to return Pending while exit is unconfirmed. The two-cycle case checks that
the old PID is absent before the next cycle (`endpoint_session.rs:1327,1373,
1597`, `coordinator.rs:740`). Do not replace these processes with doubles.

`ProcessClassifier` has three helper threads, `Instant`/Condvar startup,
hang, backoff and drain clocks, and sole ownership of child kill/wait. Its
default 10 s startup, 2 s hang, two restarts/1 s backoff and 300 ms grace are
configured bounds; only exercised paths spend them
(`turn-end-wire/src/client.rs:97,194,608,748,796`). A coordinator-only clock
would bypass their meaning. Any migration needs a separate multi-thread
clock/progress design with real exit and drain barriers.

`rapid_mute_and_reopen_never_overlaps_workers_or_carries_speech_across`
retains four explicitly threaded cycles and the real service-privacy
`wait_worker` between them. The mock's 5 ms cadence, 60 ms stable-unmute
rule, 25 ms authority refresh and 2 ms wait loop remain host-time, as do
actual between-cycle child exits (`pilot_faults.rs:2334`,
`service_privacy.rs:155,235,361`). Merely changing its fixture enum would
leave that cross-cycle clock domain behind.

## Smallest reviewable next changes and blockers

1. **Prepare the coordinated provider clock and complete-poll boundary, then
   migrate only the original early-end link-drop case.** Keep production
   defaults identical. Advance only after the worker's original control/input
   slices, service activity and output work complete at the current instant.
   A yield, empty socket, successful send or clock read is not that receipt.
   Keep 2 ms polling, original sample/packet schedules, host startup/cleanup
   guards and actual child ownership/reap. This exposes a reusable path for
   the other nine frozen coordinator scenarios without changing assertions.

2. **The direct name-note case is a smaller independent candidate.** Add
   `test-util` explicitly to voice's dev Tokio features and opt only this
   identity into paused test time; retain both branches and all quiet windows.
   Voice currently does not declare that feature directly, and Gemini's dev
   features are not an isolated-package guarantee. Check retained capture-age
   behavior and complete in-memory exchanges in a focused before/after gate.
   This is a proposal, not an assertion that an attribute-only change passes.

3. **Then migrate the 20-second provider capacity case using the proven
   shared clock.** It has the largest explicit single-case playback wait.
   Keep its PCM, packet order, flow credit, buffer occupancy, hold, starvation
   and finish assertions. No nominal reduction or measured saving is claimed
   by this audit. The other provider cases can follow as separate changes.

4. **Optionally remove fixture-only cleanup overhead from the six stepped
   endpoint doubles in a separate change.** Existing cooperative-ready cats
   can replace sentinels only with retained stdin ownership, Stop-triggered
   closure and actual exit observation. Leave the real endpoint grace cases
   and their teardown assertions intact. This is not a command-only swap.

The hard shared boundary is explicit in current code. `transport.rs` uses
ScenarioClock only inside voice unit tests, but `provider_worker.rs:20` and
`provider_link.rs:5` still import the host monotonic source. Intake converts
capture age to host `Instant` (`provider_worker.rs:2885`); Gemini checks that
age against another host `Instant` at acceptance and write
(`gemini/src/session.rs:333,792,1704,2399`). `LinkClock` samples Tokio between
host reads and rejects clocks that stop independently
(`provider_link.rs:222,287`). Pausing Tokio alone can leave real-time helper
loops spinning, weaken elapsed-age coverage, or invalidate correlation.
Changing timestamps without a shared clock and complete-work boundary would
not preserve the test contract.

Eighth's supervisor/native-head work is outside this audit's implementation
scope. Coordinate any required Gemini boundary with that owner; do not edit
their work or #866. Ownership of the early-end link-drop slice follows the
parent's latest #739 context. No GitHub state was fetched by this audit.

## Changes already merged since the frozen inventory

The frozen inventory describes `8ddffb6290557fbfd28ea02c8982d16cd5d0743a`.
It is historical evidence, not the current wait map. `8bcde112d` merged
scoped fixture stabilization; current call tracing above determines which
identities actually use it. `39eb93085` and `740e1e892` changed continued
request notes/early-end coverage. Privacy waiting, capture gaps and narrowly
bounded late IPC handling also changed in later commits, including
`1edb50971`, `09d791b2b`, `fbd560f05`, `c1c40a524`, `d1ee7c567`,
`531300b12` and `71a15e75d`. Current 100 ms normal transport freshness includes
the merged sender/receiver held-up rules bounded at 1500 ms; do not restore
the old transport behavior as part of a clock migration.

Fifty selected function bodies differ from the old snapshot: 31 pilot,
10 endpoint and 9 early-end. All five selected provider function bodies are
byte-identical, although their shared worker has upstream changes. Direct
assertion-macro counts increased in two selected functions: early-end
`lamp_stop...` has two added `assert_eq!`, and stalled-metadata SIGTERM has
one added `assert!` and one added `assert_eq!`. Counts alone do not prove
assertion equivalence; original/current function hashes are retained instead
of making that claim. Future migrations must preserve the **current** checks.

The two new top-level early-end scenarios (four pieces and link drop after
a continued request), new clock/freshness guard tests, other Voice tests and
`capture_hold_worker` do not enlarge the frozen 63. The original 63 names
match the frozen verified-measurement selector exactly, with no ignored or
paused attributes found in current source. The completed native outcome join
above confirms all 63 ran once. Isolated serial timing and future migration
qualification remain separate root-owned verification.
