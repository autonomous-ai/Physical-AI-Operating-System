# #966 direct off/on comparison candidate

Date from `date`: 2026-10-16 11:17:35 +0700. External candidate only, against immutable published `8de22aef8d5526f83b48b279e71706d71e7cdf5b` at `/private/tmp/lampos-tests-gemini-task-joins`. Two paths: append to `lampOS/crates/gemini/src/testing/task_joins/tests.rs`; one short paragraph in `lampOS/docs/gemini-session-reliability.md`. No active worktree was edited, and the frozen Actor gate is independent.

One new identity: `testing::task_joins::tests::ordinary_and_retained_supervisors_have_the_same_ordered_protocol_and_terminal_result`. It invokes actual `connector_with_capture_clock` plus `Supervisor::new` first and actual `supervisor_with_capture_clock_and_joins` second, using one shared protocol routine and one immutable five-second host guard across both and the final join. Ordinary connector ownership is asserted `JoinSource::Plain` before construction. Existing fake-service receive bounds remain subordinate to that same outer deadline.

Both paths consume the actual setup JSON and Ready, send and receive Start / acquired PCM / End in order, consume InputStarted and two exact ordered output frames with complete lineage and sequence, then receive remote close, Unavailable, TurnLost and the terminal Failure. New admission after failure is refused. Complete setup/input wire values and every semantic consumer field are compared directly. Ready elapsed duration is inherently a distinct real host measurement in each run: each remains below the common host bound, and only that elapsed scalar is excluded from equality. No clock advance, sleeps, retries, scheduling-count synchronization, new ActorWork API, or substituted connector/consumer is used.

After normal owner drops, the retained collector must report exactly two distinct task IDs: Dialing's actual returned join consumed by Supervisor and Actor's actual returned unit join owned by the fixture. A complete report is required; body drop, terminal state and abort request do not substitute for joining. The ordinary path creates no collector and its actual connector is Plain.

The complete previous 18-test file is an exact prefix of the candidate, retaining every helper, body, assertion and deadline. Removal of the single appended suffix restores it byte-for-byte; removing the new documentation paragraph restores the docs. `inverse.diff` was applied to an external copy and restores both exact source hashes. All 7,558 original worktree inputs remained unchanged.

Validation so far is source inspection plus pinned rustfmt parsing/formatting of only the external appended fragment. No Cargo or tests were run for this candidate. Suggested first execution after root approval uses the same five-package graph and exact filter, locked/offline, default profile, serial, no-fail-fast and retries 0:

```sh
cargo +1.96.0 nextest run --locked --offline -p lamp-gemini -p lamp-memory -p lamp-surround -p lamp-voice -p lamp-voice-eval --profile default --test-threads 1 --no-fail-fast --retries 0 -E 'test(=testing::task_joins::tests::ordinary_and_retained_supervisors_have_the_same_ordered_protocol_and_terminal_result)'
```

Root should first run fmt/strict Clippy/list and prove the exact single addition with unchanged ignore flags. This comparison supplements the existing panic/cancellation/reconnect/buffered-drain cases; it does not replace or weaken them. Local success would not certify service/child joins, live idleness or clock-advance readiness.
