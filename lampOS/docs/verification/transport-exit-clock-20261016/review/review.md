# Independent transport-exit source review

Reviewed 2026-10-16 08:28:11 +0700, from shell date. Base `69bae3831449bc03d1f11e3ed0c55eccfa1e6f3d`. **Pass for the corrected source; no remaining blocker found.** Executable qualification is pending and belongs to root. This review ran no Git command, Cargo command, test or target operation and changed no source.

Both findings are resolved in the frozen bytes: the document now limits the immutable host watchdog to checks before logical advances, and every new child's fallible pre-wait observation/wait has explicit fixture cleanup ownership. The live sleeper guard disarms only after successful handoff. The successful/failed-exit cases keep their guard through the real wait. The seventh new regression catches a panic before clock installation and requires actual reaped SIGKILL status.

## Exact preservation

All 27 original inline mac.rs test identities and all 104 original assertion macros are byte-exact and ordered. All complete original test bodies are exact except the selected initializer; reversing just that helper call restores the complete selected body. The helper still uses runtime exit zero, the same direct `/bin/sleep 30` child, `reaped = false`, and the same fixture label. No original certification expectation changes.

The extracted default wait loop restores byte-for-byte by substituting only its original monotonic read and original thread sleep. `wait_child` supplies those exact function items. Each iteration still tries the real child first, then samples the deadline, expires at equality, kills and waits in the same order, returns None on expiry, and otherwise waits 20 ms. The ordinary finish expression and constant are unchanged. The test-only field defaults to None; exactly one new helper installs Some for the selected original unit test. Production and integration compilation omit the cfg(test) field and module.

Nine mac.rs inverse hunks and one appended-document hunk restore both supplied clean baseline files exactly; deleting the new test module completes the inverse. The other 1,019 entries in the 1,020-file supplied baseline manifest remain exact. This is the supplied manifest scope, not a whole-repository untracked inventory. Full records are in `source-inverse.json`, `original-tests-proof.json`, `source-freeze.json` and `complete-reviewed-delta.diff`.

## Actual process and clock boundaries

Before installation, the migrated original helper's Backend owns Session, so preparation/setup assertion unwind uses its existing Drop cleanup. After finish takes Session, Clock::wait installs Cleanup before deadline arithmetic or OS polling. Watchdog, arithmetic or fixture assertions therefore retain kill/reap ownership. Successful return records the same actual Child's status; no logical receipt substitutes for exit. The selected case requires all 250 waits, the full 5,000,000 logical microseconds and actual SIGKILL status before returning to the unchanged certification assertions. Early natural exit causes those fixture expectations to fail.

Clock reads only load Cell. Logical time advances only at the original 20 ms wait point. Known fixture callers use at most 250 such iterations. The separate five-second host deadline is fixed and checked before each advance. It does not preempt process startup, try_wait, kill or blocking wait/reap, and does not assert an end-to-end host bound; final documentation states that limitation. No service-ready claim is made beyond successful spawn and observed child liveness. The direct sleep child needs no service initialization handshake.

No definite type, visibility, import or Clippy defect was found by source inspection. This is not a compilation result. The actual-exit cases verify original exit-status preservation and exit-before-expired-deadline ordering; the expired-watchdog case verifies no logical advance and reaping during unwind; the seventh case covers ownership before clock installation.

## Gate boundary

The immutable baseline list contains 498 identities and zero ignores. Its original selected case passed once in 5.055 seconds with the original real five-second wait. The corrected source adds exactly seven new cases, so the required candidate comparison is **498 retained + 7 = 505, zero ignores**. Root must still run the corrected format/strict Clippy, compiled identity comparison, selected original plus seven new cases, full affected gate, workspace build and applicable native qualification. No candidate speedup is measured by this review.

Root's earlier attempt passed format/metadata/Clippy and was stopped during list compilation before any test body executed; session 52573 ended with exit 130. Its raw termination record is copied unchanged as `root-earlier-zero-execution-termination.json`; those pre-correction stages do not qualify the corrected source and are not a test retry.

## Final frozen hashes

- mac.rs: `fe7c6e74eb2dda37b14e05b358207ee0a6e9fa1a5060924a41779b50d996f5ed`
- transport_exit_clock_tests.rs: `32a653355cf0a2f42df38bc2e71d5d2ed682ff97cc2edb8321d32373b52910d3`
- voice-test-fixtures.md: `8ae2d7745236ce42fe60ac91c8b814f68c19bfdc75b96e282dfc73ff0d81b1f8`

The parent directory retains the historical six-case review snapshot. Only this `final/` record describes the corrected seven-case source.
