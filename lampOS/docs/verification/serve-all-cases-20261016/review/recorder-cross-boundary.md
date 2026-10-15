# Recorder cross-boundary review

Read-only review of `/private/tmp/lampos-tests-recorder-progress`, started at **2026-10-16 00:40:23 +0700** from `date`. The final verification timestamp and inspected hashes appear below.

**No concrete source blocker found.** This review does not establish compilation, passing tests, real-process qualification, or a speedup. No repository source edits, Cargo, Git, network, or device operations were performed. Root owns the runtime gates.

## Boundaries checked

1. **Actual startup and readiness.** `mac.rs:2632` retains the clock sample immediately before the real recorder spawn. The startup hook at `2640` passes the remaining original 15 s host budget; the original real-file presence loop at `2657` remains. `serve_fixture_driver.rs:237` accepts Recorder first, holds the initial 1,000,000 us epoch, and performs one zero-target Step. `fixture_commands.rs:117` reaches the unchanged shared recorder body only after that Step; its first Ack occurs only from the post-write callback. `mac.rs:2673` still reads `ready.json` through the original bounded parser (`1236`: 64 KiB, separate 1 s parse deadline and 20 ms host polling), then stores the real child. Progress supplies neither the ready receipt nor its declaration. Startup control failure drops controls and kills/waits for the just-spawned local child before returning withheld (`2648`); explicit cleanup covers the interval before the child enters `self.room`.

2. **Logical completion remains separate from actual exit.** Protocol validation (`fixture_control.rs:528`, `735`) forbids input/signal targets, out-of-range Recorder clock updates, missing ready, changed wait boundaries, and inconsistent terminal flags. The CLI uses the same truthful pure nanosecond callback for optional declaration before the write and a fresh post-write sample (`fixture_commands.rs:121`). Its deadline is the complete original minimum wait; terminal Ack only lets the shared body return its configured code. The driver marks a pending real exit (`serve_fixture_driver.rs:156`); ordinary advancement refuses that state (`162`, `230`, `435`). `mac.rs:1544` observes its own `Child::try_wait` under the remaining host deadline, checks repeatedly that time is held, and only then calls `recorder_exit_observed`. It does not call `room_ended` or populate evidence/status from the Ack. The later unchanged `wait_child`/metadata import at `3302` remains authoritative for the actual return code and unavailable recording.

3. **Deadline and poll ordering.** The driver caps the next logical step at Recorder due time (`serve_fixture_driver.rs:171`), preserving the original 2,000 us cadence and runtime deadline. If Recorder ends immediately before or on a runtime poll, `471` returns to the backend first and leaves `next_poll_us` unchanged. After actual-exit notification, the next wait with remaining logical work performs that original poll at its original instant. Bootstrap likewise stays pending until exit notification when Recorder completes before/on it (`392`). Recorder `accept_until` and `step_until` share the caller's startup/preparation deadline. Ordinary backend exit settling takes its 30 s budget before the driver call (`mac.rs:1459`); preparation passes its original 10 s deadline through both driver and actual-exit settling (`1486`). Each control exchange retains the earlier endpoint watchdog. Original status-first host polling semantics remain; a new recorder round does not reset the overall drain deadline.

4. **Failure remains closed and nonblocking.** `mac.rs:1518` latches the first failure, replaces the live driver with `FailedRecorderFixture`, drops the old controls, and actually kills/reaps the owned recorder. The sentinel retains `fixture.is_some()` and returns the latched error from future wait/grant/finish operations (`1092`). Thus `wait_line` retains its nonblocking fixture path (`2324`) despite a frozen logical clock; it cannot silently fall back to a host receive wait calculated from nonadvancing time. Repeated failures do not repeat closure or queue duplicate failure events (`1522`, `1645`). Backend Drop still closes controls before original transport/child cleanup (`2390`). The protocol itself never kills a process.

5. **Coverage stays separate from lifetime.** At the fixed 1 s epoch, both 6 s recorders are due at **7,000,000 us**, with original return codes 1 and 0. The declaration case writes **21,000,000,000 ns** in its real ready file and retains the complete 23 s wait, due at **24,000,000 us**. Preflight rejects arithmetic overflow before output creation; default recorder behavior remains unchanged. Original coverage logic at `mac.rs:1780`, `1822`, and `1861` retains player-failure precedence, actual child-status checks, full speech/observation coverage, and the first latched reason. Exit zero does not create valid audio or metadata.

6. **Grant and finish ordering.** Real Start/Closed input precedes its player-control acknowledgement (`mac.rs:1963`, `1975`; `fixture_players.rs:206`). The original grant, expiry, 100 ms lateness bound, first-copy validation, and real room send remain. Player settlement precedes ordinary clock advancement (`serve_fixture_driver.rs:436`), and a continuing runtime poll must consume every counted room send (`289`). After actual relay/transport completion, `mac.rs:3212` drains Recorder under one original `(session_seconds + 60)` s host budget, settles its real exit at the held instant, and only then creates the original player-join budget (`3225`). Driver `finish_recorder` (`538`) pumps pending players while advancing by at most 2 ms, never steps terminal runtime/relay peers, and returns at Recorder terminal Ack. `finish_players` (`581`) rejects both a continuing Recorder and an unobserved terminal Recorder before advancing time. Post-session recorder draining does not invoke coverage checks or overwrite the prior declaration refusal.

## Source preservation and runtime limits

Current `mac.rs` is byte-identical to the backend handoff's `mac.after.rs`. That handoff includes full reversible source proof against its pre-recorder baseline and unchanged coverage, grant, readiness, cleanup, and default methods. Protocol and command files exactly match their stable handoff hashes and reversal proof.

The current driver differs from its saved review by four rustfmt hunks: three wrapped `invalid` calls and one unwrapped `advance_with_recorder` call, including optional trailing commas. Reversing precisely those four format changes reproduces the complete reviewed driver byte-for-byte. Existing driver tests are unchanged.

The authored tests cover distinct layers: actual private files/control sockets for Recorder commands, explicitly mock peer receipts for scheduling, and real pipe-gated children for backend exit observation. A mock receipt or a joined mock thread is never real process evidence. Compilation and actual combined execution, followed by the full unchanged serve binary, remain required. This source review claims none of those gates passed.

The inspected source preservation handoffs are:

- `/private/tmp/tests-823-recorder-backend-evidence/README.md`
- `/private/tmp/tests-823-recorder-driver-evidence/handoff.md`
- `/private/tmp/tests-823-recorder-control-evidence/handoff.txt`

## Constructor migration

The minimal recommendation was one predicate change at `tests/serve_session.rs:139`:

```diff
-    let mut backend = if config.room_recorder.is_none() && control.transport_delay_ms.is_none() {
+    let mut backend = if control.transport_delay_ms.is_none() {
```

Root applied this change while the read-only review was running. The current diff has exactly that predicate change and two introductory documentation lines that now describe recorder lifetimes and the remaining host-clock delayed transport. Reversing these two hunks reproduces the prior serve source, SHA-256 `cffbb51a9f21b03f2262099b23de526842eb098c600c02f3ec8eca187f800923`, byte-for-byte. No import, test body, assertion, fault configuration, plan, timeout, or other helper branch changed.

It enables exactly these three additional existing identities:

- `a_room_recording_that_ends_early_withholds_the_rest` (line 624): 6000 ms, original default exit 1.
- `a_recorder_that_exits_zero_mid_conversation_withholds_the_rest_and_certifies_nothing` (line 729): 6000 ms, exit 0.
- `a_declared_capture_end_before_the_next_prompt_and_its_window_withholds_it` (line 755): 23000 ms, exit 0, 20000 ms declaration.

The strict driver rewrite recognizes their existing direct local fake recorder and preserves all options plus `{out}`/`{seconds}`. `startup_cues_stay_fresh_through_a_slow_transport_and_a_slow_first_preparer` (line 648) still executes the original complete host branch because its transport delay is `Some(400)`; the original 2300 ms preparer stays there too.

All **14 identities**, **18 source `run` calls**, and **22 expanded attempts** remain. The five-fault loop accounts for four additional attempts. The selected set changes from **10 clocked identities / 18 attempts plus 4 host identities / 4 attempts** to **13 clocked identities / 21 attempts plus 1 host identity / 1 attempt**. This remains a partial clock migration, and the full binary must continue to execute the remaining host case.

## Final verification and inspected SHA-256

Verified **2026-10-16 00:50:07 +0700** from `date`. Exact backend snapshot comparison, four-hunk driver reversal, and two-hunk serve constructor/documentation reversal passed. The unchanged serve source contains 114 static assertion macros. No runtime gate was executed by this review.

- `src/physical/fixture_control.rs`: `16f31e16dcb8c03168c8f8d2e76323c6d09882936c88bd05c1691bdaa7a888fa`
- `src/physical/fixture_commands.rs`: `464820ece7888f7d0767d7024d7b1e399d9d18601f09922f14dba5b0e328234c`
- `src/physical/serve_fixture_driver.rs`: `b7deaea4fb28cc44f48675cade095d38aea77f9dba668f7b9e32e9060e5170bb`
- `src/physical/mac.rs`: `c14c40669ad423baf9c21672587bd6e97e47a64e102d6d22c2714e3e399d78e2`
- `src/physical/fake_lamp.rs`: `db8026ff943fc5367198e48be7ec7336823504a009414a5dc6961cd7c8e3f41a`
- `src/physical/fixture_players.rs`: `2378acf5a32c53b013cc90b96d827bdb82307dbb054d7c94e30ede8752dc5dab`
- `src/physical/recorder_backend_tests.rs`: `a9ed66168aea14d40b3ae28a17c46450ca8e966e6d6f46f550565daea2e7bbde`
- `src/physical/recorder_fixture_driver_tests.rs`: `da5c382c25fd8d63f7fac9e3ff17d6caff216df95ae2289af40da6c1225c3ff1`
- `tests/serve_session.rs`: `485ef6bc04c292666b633c819baa7c16465aaa756e97271c9c0708cd2c3e6186`
