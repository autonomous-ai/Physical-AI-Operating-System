# Remaining physical dispatch boundary review

This is an intentionally red review checkpoint based on Agent 2 `ec84cf72`
(tested code `37b398a7`). Do not merge it as a deployable runtime. The four
previous audit regressions pass, as do the focused terminal/provider/relay
suites (28), library tests (40), and original trigger-delay reproduction (1).

Two additional tests appended to `tests/final_dispatch.rs` fail on that source:

1. After the runner's eligibility check, time may elapse before the backend
   dispatch. `PhysicalBackend::inject` drains relay input without a per-call
   budget and starts the Player without rechecking the original target's
   100 ms deadline. A known overdue target started once (`late_us=282368`,
   `withheld=None`). Check the local deadline immediately before Player start;
   budget drain work and revalidate or withhold when it cannot finish.
2. `physical/session.rs` begins relaying Trace only after the runtime has exited
   or was killed, with SessionEnd emitted after the trace. The backend stores a
   consumed post-exit trace and returns no live event, leaving dispatch open.
   The reproduction consumes a run_end trace and still starts the Player once.
   Close live dispatch on this already-observed post-exit state; retain the
   evidence stream without inventing successful completion.

These concern observed state and the local clock; no atomic guarantee over
not-yet-arrived cross-process events is requested. Both tests use the actual
PhysicalBackend with its local pipe relay and a recording-only Player; they
make no device/cloud or acoustic claim. Run:

```sh
cargo test --locked --offline -p lamp-voice-eval --test final_dispatch audit_physical_boundary -- --nocapture --test-threads=1
```

Root's private device runner is separate and does not use the affected Player
path. The evaluator overlay remains unintegrated pending these corrections.
