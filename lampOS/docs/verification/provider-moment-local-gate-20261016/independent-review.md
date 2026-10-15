# Independent Provider Moment review

Read-only source review completed at `2026-10-16 02:09:51+0700` from `date`.
Worktree: `/private/tmp/lampos-tests-provider-moment`.

No blocking safety or correctness defect was identified in this preparation
slice. Compilation and runtime qualification remain with root; no Cargo,
compiler/test binary, Git, network, device or repository edit was performed.

## Checked contracts

- `scenario_clock.rs:49–118`: Moment owns an immutable Arc origin plus copied
  Duration/timestamp values. Its private fields prevent external construction
  of inconsistent values. Arc allocation identity cannot alias another live
  root while an old Moment retains it, including equal TLS IDs/epoch values.
  These fields make Moment Send/Sync; observer and root guards remain thread
  bound through their `PhantomData<Rc<()>>` markers.
- `scenario_clock.rs:93–118,199–216,281–295`: checked creation/adoption validates
  active origin, microsecond conversion/addition and exact Instant addition.
  Root snapshots also verify the unchanged epoch. Observers compare complete
  Duration, so 950-to-900-ns regression is rejected even with equal integer
  microseconds. Invalid, foreign or regressing input is rejected before the
  admitted state is assigned. Re-admitting the same live Moment is allowed.
- `scenario_clock.rs:73–79,318–331,420–435`: State Drop release-invalidates the
  origin on normal Scope removal, callback panic removal and owning-thread TLS
  teardown, even when a guard was forgotten. The callback/pump still drops
  outside the TLS borrow. Old Scope IDs cannot remove/snapshot a successor.
- `scenario_clock.rs:150–168,177–195,199–216,221–233`: after invalidation, pure
  observer reads intentionally retain the last accepted time and perform no
  active-origin check, clock advance, callback or host-clock sample. A later
  enter/observe checks the origin and rejects it; Drop/unwind removes only its
  matching observer state and restores host fallback. This behavior matches
  the stated contract rather than pretending invalidation is a new time value.
- `scenario_clock.rs:177,243,386`: root/observer nesting is rejected in both
  directions. Observer `step` rejects both Run and Shutdown; fault injection
  requires an actual root Run callback. Root and observer identity counters
  have checked increments instead of wrap/reuse.

Admission concurrent with root invalidation may linearize at the active-origin
load before the invalidation store. An overlapping operation is not a promise
that the root remains active after enter/observe returns. Future provider
coordination must retain its explicit bounded cancellation/join protocol; frozen
reads and `is_active()` are not an origin-liveness or process-exit receipt. This
is an integration boundary, not a blocker or a requested semantic change.

## Preservation independently verified

A separate read-only Python check reversed every recorded addition and recovered
the entire original `scenario_clock.rs` byte-for-byte. More specifically, the
root `step` implementation is exactly the old body after removing only the new
observer-rejection guard: peers still pump before the 2-ms Run advance; callback
errors preserve the pump without advancing; Shutdown freezes logical time and
passes the supplied host deadline unchanged. Root watchdog and real-child
exit-observation implementations are preserved.

The original seven test names/bodies and all 43 static assertion sites are
byte-identical. Their whole module SHA-256 is
`ef98675980c838fc13ad4632aa2ecdcb0aa04f2df9dd509c499063ed5f1add20`.
All 160 other existing Voice Rust files match the original hash inventory, and
the only new Rust file is `scenario_moment_tests.rs`. `lib.rs:80` still includes
`scenario_clock` only under `#[cfg(test)]`; no production cfg path was changed.

The 13 new tests (78 static assertion sites) cover pure snapshot reads and
adoption, scope restrictions, submicrosecond regression, real independent root
identity, normal/panic/TLS invalidation, stale handles, unchanged reads after
invalidation, malformed values, both timestamp overflow domains, thread isolation
and observer unwind. Their synchronization uses explicit bounded channels rather
than sleeps or retries. These tests were inspected, not run by this reviewer.

Reviewed SHA-256 values:

```text
4bb0811dbc62ad2345755eca06daa8f91a4e64a8dc1bfab66f763afe5cd7a5f6  scenario_clock.rs
c372eac927457059471d7a6bc3b56c8855f530aab1b7b81e11305fe2dddfe210  scenario_moment_tests.rs
```

Machine-readable independent preservation results are saved at
`/private/tmp/tests-823-provider-moment-review-proof.json`. The implementation
README's limitations are accurate: this does not yet provide the later provider,
Tokio, capture-clock or shutdown controller and establishes no latency result.
