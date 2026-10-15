# Memory Reporter scheduling seam — #823

Source audit recorded **2026-10-16 01:14:44 +0700**, read from `date` by the retained audit script. Worktree: `/private/tmp/lampos-tests-memory-clock`; parent-supplied base: `6b08a70d5`.

## Ready for parent compilation

Only these owned repository files changed:

- `lampOS/crates/memory/src/health.rs` — `013c99487db64189c2ae78ca4588cb8449358e1496707311c3d79a7e0fd0632b`
- `lampOS/crates/memory/src/reporter_schedule_tests.rs` — `c324372847d39007cf5302890890947a844fdaa4e4b7853ec62ce13d278eaf28`
- `lampOS/docs/memory.md` — `2ce9d55196cc076a7133954a953786ac5b7b9638a859e7187bcee1118aa5a5d6`

The parent separately copied the four stable Bus dependency files; its provenance is `/private/tmp/tests-823-memory-clock-evidence/bus-dependency.json`. This worker did not edit those files in the Memory worktree.

`Reporter::start_with_schedule(root, counts, Arc<dyn ReportSchedule>)` installs one explicit schedule for the worker and the lazy declared Bus publisher. `ReportSchedule` has pure `now`, park-token-compatible `park`, and optional nonblocking `observe`. The public progress enum distinguishes a completed cycle and a checked positive wait; its publication outcome distinguishes no attempt, actual success and actual error. All are local observations, not subscriber receipts, downstream log flushes, OS parking, publisher cleanup or worker exit.

Ordinary `Reporter::start` supplies `SystemSchedule`. Its two worker clock reads remain at the original cycle start and each original elapsed check. The publisher retains its three existing cadence sample positions. There are no constructor-time clock reads or global changes. System waiting remains `thread::park_timeout`; the one-second period, stop checks, `unpark`, 100 ms completion wait, publication drop before completion notification, and lack of unbounded thread join are unchanged. The private original-test constructor now has `#[cfg(test)]` because production uses the explicit constructor's shared worker.

## Source preservation

`audit_source.py` reverses only the explicit constructor/clock/wait substitutions, typed observation additions and separate test-module declaration. The resulting **entire health.rs is byte-for-byte equal** to `health.before.rs`. It separately verifies:

- All original health unit-test bytes and identities remain unchanged.
- The other 23 original Memory Rust files are byte-identical, including all 12 integration files and main/server/state users.
- State, diagnostic construction and the legacy generation fence are unchanged; their latency/count-age clock remains real.
- State access, finish, Drop and completion cleanup ordering are unchanged.
- The documentation change is exactly one short scheduling subsection.

Retained artifacts include `source-preservation.json`, complete before/after source, the reversed source and text diffs. The same audit can be rerun without Git, builds or network.

## Added boundary checks, not yet executed here

Eight new tests cover pure reads and the host default, original worker sample positions, frozen/spurious/pre-deadline wakes and equality at one second, actual held publication before completion with no catch-up queue, shutdown while frozen, shutdown while real publication is held, actual error/recovery outcomes, and a real Bus subscriber created after a successful no-subscriber Event publication. The final check receives sequence 2 through an actual Unix datagram after shared logical cadence permits the subscriber rescan; sequence 1's successful return is explicitly not treated as a receipt.

Progress queues are bounded. Test-controlled waits use the real thread park token; publication holds have explicit release channels and release-on-drop guards. Host deadlines only bound waiting for real progress and the original shutdown behavior. There are no sleeps to advance fixture time and no test retries.

Owned Rust files were formatted successfully with rustfmt 1.96.0, edition 2024, `skip_children=true`. The source-reversal audit passed. **No Cargo, compilation, clippy or tests were run by this worker.** The parent owns those gates. It reported a separate earlier Bus-plus-original-Memory gate failure in the already-known Memory usage-log family (#837/#862); no main/log-relay source was changed here to address it.

This is an API preparation step. No CLI/control protocol, existing scenario migration, full-process deterministic clock or speedup is claimed. A future controlled process still needs real downstream log and exit acknowledgements. Callers must keep observations nonblocking and wait through the worker's park token so the existing shutdown wake works even when fixture time is frozen.
