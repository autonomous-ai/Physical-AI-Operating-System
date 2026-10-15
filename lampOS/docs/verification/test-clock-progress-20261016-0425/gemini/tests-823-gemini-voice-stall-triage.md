# Gemini affected-gate capture-stall failure: read-only triage

Checked **2026-10-16 04:25:13 +0700** (shell `date`) in
`/private/tmp/lampos-tests-gemini-capture-clock`, recorded base
`48b3737d5e83efa00278172cb602d3525d368a43`.

**This is the same test and threaded fixture tracked by #866, with a different
failure signature. The new Gemini capture-clock seam is not reached by this
fixture's call graph.** The affected gate remains RED. Unchanged source alone
is not causal proof; neither is direct seam non-reachability proof against every
indirect build, resource or host-scheduling effect.

## Saved result

`full-gates.json` records this command, run from the tree's `lampOS` directory:

```text
cargo test --locked --offline -p lamp-voice -p lamp-surround -p lamp-voice-eval --features lamp-gemini/fixture-clock --no-fail-fast -- --test-threads=1
```

Target: `/private/tmp/lampos-tests-capacity/lampOS/target`. Saved `date` markers:
**2026-10-16 03:45:59–04:10:46 +0700**; exit **101**. Parsing all 68 result
summaries confirms **2,011 passed, one failed, eight ignored**. Voice lib alone:
1,171 passed, one failed, eight ignored. Sole failed identity:

```text
coordinator::tests::pilot_faults::a_stall_while_the_lamp_answers_cuts_the_reply_and_the_voice_goes_on
```

Exact excerpts, `full-affected-tests.log:1594–1604`:

```text
fixture Threaded, packet and policy clock host, real conversation and cleanup 2.155685401s
result Err("turn ownership has been revoked or superseded")
peer privacy gap 6513 us, refused writes 0, end-to-speaker revocation [(1, 869)] us
capture send-attempt gap 11225 us, oldest source age at send attempt 1508 us (producer; consumption age unmeasured)
frames 200 starts [1] ends [1] chunks 13 accepted 13 fired [(CaptureStall, 2625196230369)] stopped ["privacy", "speaker", "capture", "provider"] abandoned []
```

The panic is **`Err(StaleOwner)` at `pilot_faults.rs:1964`**, the unchanged
`assert!(is_mute(&run.result), "{:?}", run.result)`. `is_mute` requires an actual
`PhysicalMute` error. The preceding `run.assert_invariants()` completed, including
its actual child-exit/reaping, cleanup, Stop and accepted-write checks. The later
capture-gap cancellation count/reason assertions were not reached.

Recorded event kinds end in `turn_finished("runtime_failed")`, with no
`hand_control_cancel`, `capture_gap`, or `physical_mute`. Only CaptureStall fired,
not the later Mute. The summary lacks the originating error's call-site backtrace
and per-operation ownership/clock samples.

## Exact reachability

The test selects `Fixture::Threaded, Answer::Hold(40)`, injects CaptureStall at
Accepted(3), and expects Mute at Block(400) (`pilot_faults.rs:1954–1967`). Its path:

```text
run -> run_in -> cooperative fixture children + real conversation
             -> synchronous Peers::tick / tick_provider over actual Channels
             -> finish_active_run and real supervisor cleanup
```

* `process.rs:149–194`: cooperative children execute **`cat`**, whose stdin closes
  after the peer observes Stop. They do not run production capture/provider code.
* `pilot_faults.rs:1409`: ScenarioClock is created only for Stepped. This case
  uses an independent privacy thread and synchronous peer thread; the latter
  calls `state.tick(None)` and sleeps one host millisecond (`1491–1530`). Unscoped
  clock reads use actual monotonic time and `std::Instant`. There is no Tokio
  runtime on this fixture path.
* `Peers::tick_provider` (`974–1045`) receives real `ProviderInput`, builds scripted
  `ProviderOutput`, and sends through `OutputSender`. Audio input is discarded by
  this fixture peer at 1018; End selects the scripted answer. Provider-worker
  imports here are wire types, not calls to its serving loop.
* No Gemini connector, InputSender, setup or Actor is constructed by this path.
  Coordinator references to Gemini size constants do not invoke session code.
  **Neither System nor Fixture capture-clock reads occur through this scenario's
  provider peer.** This is a stronger source relation than byte identity alone.

The broader gate enabled `fixture-clock`; that does not select it globally.
Ordinary `setup_and_spawn` still selects System (`gemini/src/session.rs:644–653`),
as do ordinary pair (`testing.rs:150–161`) and connector (`225–233`). Only the
explicit custom factories select Fixture. The frozen source has no global
mutable capture-clock selector. Those default factories are not even needed by
this fixture, but their behavior also remains explicit under feature unification.

## #866 relation and owner hypothesis

The retained `issue-866-current.json` identifies open #866, **“Threaded
capture-stall fixture loses original microphone read interval before mute.”**
It records the exact same identity and Threaded host-clock fixture, but its
original release-gate error was **“invalid original microphone read interval”**.
That older trace had a recovered 430 ms gap, 42 dropped late frames and frame 251.
The issue's possible unsent-frame loss was expressly an inference, not an
observed failed send. Its owner comment asks fifth and tests to distinguish
fixture timing from product capture-gap handling around #791.

Today's **StaleOwner at frame 200** has no recorded gap recovery or hand-cancel.
The 11.225 ms maximum send-attempt gap updates only on a new attempt, so it cannot
measure an ongoing silent stall. These data do not establish the earlier
catch-up/unsent-frame mechanism. Route this as **a different symptom in the same
scenario**, not a proven recurrence of that precise cause.

A concrete existing-source boundary for the owner to inspect is expiry of the
**100 ms capture lease between host samples in one coordinator iteration**:

1. CaptureStall holds sends for 500 ms while preserving the 10 ms source schedule
   (`pilot_faults.rs:670–674`). Its eventual catch-up loop (`883–934`) still calls
   `next_due` before enqueue, the path discussed in #866. This log does not prove
   catch-up was reached.
2. `gap_cut` uses one monotonic sample near `coordinator.rs:2968–2972`, before
   speaker/provider work. Its `hand_control_cancel` event is recorded only after
   `hand::cancel` returns.
3. Later, `dispatch_speaker_chunk` is called at 4367 and does
   `owner.issue_output(now(), current.plan, ...)` at 5673. That is a fresh sample.
   `Controller::issue_output` advances expiry before checking the planned owner
   (`turn-owner/src/controller.rs:229–240`). `advance` revokes the active turn at
   capture/admission expiry (`391–411`); `require_owner` returns StaleOwner when
   the turn is gone (`372–377`).

Crossing the lease boundary after an earlier non-gap observation is therefore a
**source-consistent hypothesis** for StaleOwner before the next cancellation
trace. It is not an identified failing call site: other owner checks exist, and
this log has neither the needed samples nor a backtrace. `finish_active_run`
records `runtime_failed` for a non-mute error and preserves the original error
when cleanup succeeds (`coordinator.rs:2447–2489`, `service_privacy.rs:47–54`),
consistent with the raw StaleOwner and completed cleanup checks here.

Do not widen leases, change assertions, suppress this result, retry it, or apply
#866's unconfirmed send-retention fix on this evidence. No fix was made.

## Preservation and limits

The frozen gate manifests cover Gemini, not Voice. Read-only `git show` of the
locally available recorded base verifies **ten scoped Voice/turn-owner files
are byte-identical to base**: pilot fixture, coordinator, hand cancellation,
transport, process, ScenarioClock, provider flow, service privacy, and turn-owner
controller/types. Four capture-clock code/manifest files match the frozen
03:38:38 preflight hashes; its English doc matches the frozen implementation
snapshot. All five change files are thus still stable. Selected hashes:

| File | SHA-256 |
|---|---|
| `voice/.../pilot_faults.rs` | `e7c292977de467d24c921a59e4012e471131e953af89dfe5764f87257333075b` |
| `voice/src/coordinator.rs` | `752b2179533236813b4cbf63cc87094294a982c0522f40508eae58086fa705b5` |
| `turn-owner/src/controller.rs` | `b8237eddfd8d17d9c38496aa5b48eac14277bc3ca124da9de8a889440e44788a` |
| `gemini/src/session.rs` | `27f0d1b329d5407e2b85941a60fd747d12eed3fc7aa07accbf0ceda373ae6073` |
| `gemini/src/testing.rs` | `ac34ac1c456b9e87557bec5aa9c6c4f1a5bc86fcbd09d9518bc13c98859081d3` |

Complete hashes and parsed totals are in
`/private/tmp/tests-823-gemini-voice-stall-triage-evidence.json`. Raw inputs remain
under `/private/tmp/tests-823-gemini-capture-gate-evidence/`: `full-gates.json`,
`full-affected-tests.log`, `full-result.json`, `issue-866-current.json`,
`baseline-source.json` and `preflight-source.json`.

No test execution/retry, source/cache edit, Git mutation, network or device action
was performed. This read-only triage does not establish root cause or production
capture behavior and does not turn the affected gate green.
