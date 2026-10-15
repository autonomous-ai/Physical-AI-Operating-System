# SlowTransport control and command cross-review

Reviewed at `2026-10-16 01:51:28 +0700` (from `date`). Worktree:
`/private/tmp/lampos-tests-slow-transport-progress`.

No concrete correctness, validation, or cleanup blocker found in the four
assigned files. This is a source review, not a successful compilation or test
run. I did not inspect the changing driver, run Cargo/tests, use Git/network,
or modify repository files.

## Contract checks

- `fixture_commands.rs:121–173`: validation precedes connecting the new role.
  The shared adapter receives the original argument slice unchanged. The
  callback computes one fixed start and due time from the first validated
  Step, rejects arithmetic/domain overflow before any wait Ack, and reaches
  terminal progress only at or after the complete delay. Public input is
  integer milliseconds, so the microsecond representation does not truncate
  any accepted delay. Clock reads are pure; same-time rounds retain the due
  time. There is no stdin/stdout access in this new adapter.
- `fixture_commands.rs:176–214`: preflight requires the exact delay shape,
  an absolute current evaluator executable, the controlled serve relay,
  the same control socket, and the existing controlled serve-runtime
  validation. It parses without rewriting forwarded arguments or adding
  shell interpretation. The indexed runtime-prefix access follows the
  existing validation requiring its first three entries.
- `fixture_control.rs:522–688,820–871`: new-role input/signal targets are
  rejected; the first start equals the held Step; start/due cannot change;
  terminal is equivalent to the deadline comparison. Existing consecutive
  round/poll, monotonic time, role, frame-size, failure-latch, and host I/O
  bounds continue to apply. Existing role wire shapes are preserved.
- `fixture_commands.rs:148–153` explicitly drops the Participant before
  returning to real exec. Both UnixStream owners in its Channel then drop;
  the separately cloned FixtureClock owns no stream. `final_ack` is only a
  completed-delay receipt. The listener has distinct SlowTransport and
  ServeRelay bits (`fixture_control.rs:251–258`), so the latter still requires
  its own actual connection and validated Hello. No exec success, relay
  registration, OS exit, or consumed-input evidence is synthesized here.

## Test review and cleanup

The 12 new protocol tests cover zero-input and signal rejection, both directions
of old/new role mismatch, immutable boundaries, terminal iff due, counters,
pure reads, invalid-Step clock preservation, bounded missing progress, and
separate relay registration. The 9 command tests cover fixed 400 ms and zero
delays, overflow before Ack, parent loss, argument restrictions, and real exec.

`slow_transport_fixture_commands_tests.rs:374–425` queues two distinct Ping
lines before the delay, proves no exec marker at initial/same-time/due-minus-one
steps, then checks the actual shell exit code 37, unchanged PID, literal spaced
and semicolon arguments, and both queued lines after real exec. The two other
owned-child cases check the original real exec error and parent-loss prevention
of exec. Those cases intentionally call the shared private adapter with test
targets; they do not claim public-handler product relay startup.

The child owner (`slow_transport_fixture_commands_tests.rs:318–370`) targets
only the current test executable, uses child-local environment selection,
polls actual `try_wait` with a 10-second host deadline, kills/reaps on deadline,
and kills/reaps on unwinding. Success collects actual process output/status.
These fixed shell commands spawn no descendant process. The finite wait
threads use 2-second control watchdogs and completion reception before joining.
No added skip, retry, sleeps, or changed original assertion was found.

## Independent preservation check

I evaluated only the read/transform portion of `audit_source.py` in memory,
then independently compared its output and current hashes; I did not execute
the script's write section. Both complete original files reconstruct exactly
against the supplied `before/` snapshots:

- `fixture_control.rs` original SHA-256:
  `16f31e16dcb8c03168c8f8d2e76323c6d09882936c88bd05c1691bdaa7a888fa`.
- `fixture_commands.rs` original SHA-256:
  `464820ece7888f7d0767d7024d7b1e399d9d18601f09922f14dba5b0e328234c`.

All 41 reported existing test files, including all 23 integration Rust files,
match their before hashes. The sole additional old-file change is the recorded
recorder helper exhaustiveness arm; reversing that one exact line restores
its hash. No assertion, input list, or existing test identity changes there.
The new files contain the reported 12/9 tests and 45/42 static assertion sites.

Current reviewed SHA-256 values match the handoff:

```text
fd764a6a67b3c83295e3fad677a39ef0c6a10a70a7305f9d92aa2fe10d64daf2  fixture_control.rs
df424358eb99ab6982ef8cd3ea3d7484f55bf3c033aabcf29ca0b07c0df5ab0a  fixture_commands.rs
1032118bfae8aec9a555f87ca6f3f334c16599c361a90b38768d5c1d454d3359  slow_transport_fixture_control_tests.rs
dee37c32bd463a6603ab0b2f1dc5f0b2a9ae4b62da98b26a9b9aea560b7ef7c1  slow_transport_fixture_commands_tests.rs
```

Limits: the supplied original snapshot's Git ancestry was not independently
queried. The protocol validates a self-reported fixed due boundary; the
adapter's checked calculation supplies the configured duration. Parent
clock-holding, fresh relay acceptance after real exec, backend readiness,
and integration timing remain the separate driver/integration review and
gate's responsibility. No test execution or timing result is claimed here.
