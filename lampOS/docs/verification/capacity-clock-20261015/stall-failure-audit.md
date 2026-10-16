# Retained capacity-gate stall failure audit

Recorded 2026-10-15 22:20:55 +0700, from `date`. Read-only inspection; no build, test, retry, Git mutation or network action. Source HEAD `71a15e75de4345afb6c1937e15ff5a223c7bfb53`; fetched `origin/lamp-voice` at inspection `74eb16c73a2075f7e2ae9f2f7359d1717fd17d23`.

## Observed failure

`rebase-tests.log:1403–1420` retains `coordinator::tests::pilot_faults::a_stall_while_the_lamp_answers_cuts_the_reply_and_the_voice_goes_on`. It failed `assert!(is_mute(&run.result), "{:?}", run.result)` at `crates/voice/src/coordinator/tests/pilot_faults.rs:1963`, after the common invariants returned. Expected terminal result is `PhysicalMute`; actual error was `invalid original microphone read interval`. Voice library outcome: 1,127 passed, 1 failed, 8 existing ignored, 283.91 s. The complete gate has exit 101; it started 2026-10-15 21:44:44 +0700 and finished 22:11:28 +0700, as retained in its date files.

The scenario injects a 500 ms capture stall after the speaker accepts three chunks, then expects mute at capture frame 400. It reached capture-gap cancellation, recorded a 430 ms source gap with 42 late frames dropped, and stopped with 251 attempted capture frames. Recorded producer maximum send-attempt gap was 511,148 us and oldest source age at an attempt 502,133 us; consumption age was explicitly unmeasured. All four peers stopped, none was abandoned. The exact failing frame, interval arguments and socket-send result were not recorded.

## Source-supported likely path, not a proved interleaving

`CaptureSchedule::next_due` (pilot_faults.rs:577–585) increments `next_sequence` before returning the immutable acquired-at timestamp. Capture catch-up drains up to 64 due frames per peer tick (881–934). The frame is then sent through nonblocking `Channel::send`, but **any send error only breaks that catch-up loop**, after the source sequence was already consumed. There is no retained pending frame. `Endpoint::try_send` explicitly reports a full kernel queue as `WouldBlock`, including normalized macOS ENOBUFS (`ipc/src/lib.rs:186–206`). Consequently a full queue can silently lose an intended fixture frame.

After the coordinator drops the old portion of a stalled backlog, it resets read lineage at the first fresh frame (`coordinator.rs:3492–3524`). A later missing fresh frame creates a short sequence gap that is not another 100 ms capture gap. `observe_prospective_input` passes that sequence to `PlaybackMask::remember_read` (`coordinator.rs:4975`), whose consecutive-sequence condition rejects it (`turn_taking.rs:759–773`) with this exact error. This is a plausible explanation for the retained failure. The same error also covers malformed/overlapping intervals, future completion and age over the 600 ms history bound; the captured log does not select a predicate. No assertion or production bound should be relaxed on this evidence.

A separate fixture-only correction could keep one unsent frame across `WouldBlock`, preserving its original acquisition timestamp, sequence and payload, and prevent generating its successor until successful enqueue. Other send errors should be explicit diagnostics, not silent drops. Prove the transport-backpressure path deterministically (an enqueue refusal followed by success preserves the exact same frame and its next sequence), while retaining the existing deliberate-drop and genuine-source-gap rejection test. This is transport progress within a test, not rerunning a failed test. A stepped migration may separately remove host scheduling dependence, but merely changing Threaded to Stepped would not fix this demonstrated source-level frame-loss path.

## Independence from the capacity clock edit

The serial log records this failure at line 614; the new paused capacity test runs later and passes at line 1035. The failed test is synchronous `#[test]`, explicitly `Fixture::Threaded`. `run_in` creates a scenario clock scope only for `Fixture::Stepped`; this path instead spawns ordinary privacy and worker threads, runs synchronous `conversation`, and uses std Instant / host monotonic time. It never enters the capacity test's Tokio runtime or shared Speaker helper. Tokio's paused clock belongs to its runtime; std Instant remains normal. There is no paused-clock state path from this capacity change to the failure.

The fixture, `coordinator.rs`, `turn_taking.rs`, and `scenario_clock.rs` are byte-identical to both HEAD and the fetched origin ref. The capacity patch does not alter them. Enabling Tokio test-util is a dev feature and does not redirect these host clocks.

## Prior evidence and limits

Cached #739 comments describe earlier `capture discontinuity` failures and the correction to preserve fixed source acquisition times, especially comment `6179142611` (v4). That history is relevant to this harness but does not establish this exact error as an existing known failure. The inspected cached #739 snapshots and local capacity evidence contain no earlier occurrence of the exact failing assertion. No fresh remote issue search was performed. Root should retain this result, file the concrete fault separately, and avoid calling it fixed until the deterministic transport regression and normal gates establish the correction.
