# Memory health migration handoff

Recorded from `date`: 2026-10-16 03:10:06 +0700. Source is held for independent review and root execution. This agent ran only direct pinned rustfmt and source inspections/audits; no Cargo, build, test, Git, network or device commands.

## Owned changes

- `lampOS/crates/memory/tests/health.rs` — `0d829067a29a9e5105326b1e0b50c09436e6aefe5091849a683be282910f9d91`
- `lampOS/docs/memory-fixture-clock.md` — `22fc06ead6ad7e0a8d5a1791adf74a340be755c91ddb3d0ec9d4f7a5a6b60de4`

Four explicit constructor sites select the shared-body fixture binary for five live reporting children. Scratch, Reports and Process share one local `Rc<RefCell<Option<Controller>>>`. The two default-off and two failed-open children still execute the ordinary binary. The existing real argument/env/stdio builder is unchanged; only the selected fixture replaces stdin with its explicit bootstrap through the reviewed parent helper.

Readiness still checks the actual ready file and real live Child under its original host loop. `report_when` settles one held instant, drains actual subscriber messages in order, and advances only after an actual empty receive. Its one Window covers the entire matching loop. `logged` also retains one Window. `log_in` shares the sustained loop's Window through its final zero-left check; `wait_cadence_in` calls `advance_to` on that same Window for each original 200 ms step, retaining all three full report periods while the actual obstruction exists. Receipts never become Diagnostics or child-exit evidence.

Raw log reads are separate from synchronizing assertion reads, so error formatting cannot recursively borrow control. A controller error remains fatal rather than selecting a host fallback. Child ownership enters Process before slot publication. Drop retains actual kill/wait, then takes and drops the Controller after releasing the slot borrow.

## Preservation proof

`health.before.rs` is the exact pre-edit source, SHA256 `31add261e22dda578ca5f5c0b9016a1382f7a1e3d87f6968a1d083ec722e394d`. `health-source.diff` lists every source edit. `source-preservation.json` records the following successful mechanical checks:

- All seven original test identities remain exact.
- All 33 scenario assertion macro byte strings remain exact. The entire scenario section beginning at `counts_through_real_requests` reconstructs byte-for-byte with only the four constructor substitutions and sustained-loop Window/log/two clock/one wait substitutions listed in that JSON. `scenario-tail-restored.rs` contains the reconstructed section.
- Both complete host-only test bodies remain byte-identical, including the two 2200 ms/100 ms absence windows and two 250 ms post-reap receive windows.
- Every original request/value/clock helper and the original argv/env/stdio builder remains byte-identical.
- There were 42 original assertion sites and now 43: the sole new assertion rejects an already attached child controller. Two original helper timeout assertions preserve their check/message but rename the diagnostic call from `self.log()` to `self.raw_log()`; their macro bytes are deliberately **not** claimed identical. The readiness panic uses the same diagnostic-only rename.
- All 33 other snapshotted Memory source/manifest files remain unchanged, including both parent-helper files, child protocol/schedule, State/runtime, application, and original timing tests. `final-hashes.json` rechecks all held hashes after formatting.

The append-only migration documentation describes those boundaries and their limits. The sustained loop retains its exact body/order and dynamic answered accounting. Its old host-driven iteration count depended on request duration, so no fixed before/after request-iteration equality is claimed.

## Validation and limits

Direct pinned `rustfmt --check --edition 2024 --config skip_children=true` passed for the owned Rust file with exit 0 and no output; command/result is recorded in `rustfmt-check.json`. All source audits above pass. Independent peer review and root's compiled-list comparison, paired seven-case execution, strict Clippy and full gates are still required; this handoff makes no runtime success or speed claim for the migration.

The original request 3 s watchdogs, real State handler/count-age clocks, UTC headers, subscriber leases, real file/socket operations and ordinary 100 ms Reporter shutdown are unchanged. Eight-second Windows bound cumulative control/marker waits; they do not preempt potentially blocking filesystem or kernel kill/reap calls. Actual cleanup ownership is retained, with no detached reaper or synthetic completion. This is development-host fixture evidence, not lamp latency evidence.
