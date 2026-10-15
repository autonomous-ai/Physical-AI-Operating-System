# New child boundary test park-gate correction

Date from `date`: 2026-10-16 02:59:11 +0700. Only `lampOS/crates/memory/tests/fixture_child.rs` changed. No Cargo, builds/tests, Git, network or devices. Pinned rustfmt and `--check` pass; root owns the new execution gate.

The original run is retained unchanged as `preflight-boundaries.log`, SHA256 `2776a0ceb0ae712cc43b9b11bdeb65711b4e9c127f2a115adedc5b87e54a9c5f`. Its actual result was 11 child cases passed, one failed at old fixture_child.rs:340 on worker.done.recv_timeout; all 23 parent cases passed. This is a recorded failing new-test fixture, not a hidden or retried flaky original test.

The failed test explicitly unparks the worker, but the worker then had a blocking std mpsc recv_timeout before calling the park under test. That intervening receive can consume the same per-thread park token, leaving the later park blocked until the test's full HOST_WAIT elapsed. The test intended to hold the worker before park; it accidentally inserted another consumer of the token.

The narrow fix replaces only that second gate with a distinct Mutex<bool>/Condvar wait_timeout_while. It uses the same full HOST_WAIT, retains a predicate for wake-before-wait, and uses the condition variable's separate wait state. Parent unpark still occurs before opening the gate; the next token-consuming park is the actual FixtureSchedule::park. Owned Worker cleanup opens that gate and closes/wakes the schedule. The first pre-work mpsc gate remains before the tested unpark.

All 45 assertion macro byte strings and all 12 test identities are identical before/after. Every other test source section is byte-identical. Before SHA256 `95934ea55a18e979fc2396255ff8b7c87ca0b623a94305c570eff6aec7cf9e6b`; after `26928e7411dd17789addf27b648760d38d7f69bfb18a0464412e5f2c6c741982`. The old child handoff/proof hashes remain historical pre-fix evidence; this JSON records their precise test-file amendment.

Actual Reporter path audit: health.rs emits CycleComplete, computes the positive wait, calls schedule.observe(WaitChecked), then immediately calls schedule.park(left). The fixture observer uses current-thread registration and nonblocking try_send; it has no blocking channel receive between WaitChecked and park. Registration is already established by the initial CycleComplete before any controlled advance. The control thread's recv_timeout is on a separate thread and does not consume the Reporter's token. Thus the discovered gate is test-only; no Reporter/fixture-schedule behavior change is justified or made.

The pinned standard-library source/doc component is not installed locally, so this audit does not claim a locally inspected std implementation. The observed source ordering and retained timeout support the park-token diagnosis; the changed-source gate remains for root to execute. No new pass is claimed by this report.
