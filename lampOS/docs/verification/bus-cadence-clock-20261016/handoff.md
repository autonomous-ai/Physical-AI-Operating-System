# Explicit publisher cadence clock — #823 handoff

Recorded **2026-10-16 00:58:19 +0700** (from `date`). Worktree `/private/tmp/lampos-tests-bus-clock`; parent-supplied base `6b08a70d5`. Source edits are stable and ready for the parent's Cargo gate, not a claim that compilation or tests passed.

Only these four repository files were edited:

- `lampOS/crates/bus/src/publish.rs`
- `lampOS/crates/bus/src/lib.rs`
- `lampOS/crates/bus/src/publisher_clock_tests.rs` (new)
- `lampOS/docs/bus.md` (one short cadence section)

## API and preserved behavior

`Bus::publisher_with_cadence_clock(topic, kind, source, now)` and `Bus::publisher_declared_with_cadence_clock(topic, declaration, source, now)` accept `now: impl Fn() -> Instant + Send + Sync + 'static`. `Publisher` owns the callback; reads must return the caller-owned current instant without advancing it. Both existing Bus constructor bodies remain byte-identical and their Publisher wrappers supply the function value `Instant::now`, without sampling it. `Send + Sync` is retained.

The shared constructor performs the original validation and initialization; undeclared construction makes zero cadence reads. The declared constructor installs the callback before the original initial show, samples it exactly once after the actual declaration write, and retains the original error/result path. A failed declaration write or constructor validation makes no cadence read.

Only the three existing cadence samples were replaced: declared first show, `alive`, and `deliver` after its original size guard. Input/encoding/payload serialization/bus-size validation still occurs before delivery's sample. An operating-system send rejection remains after that sample, exactly as before. No clocks were added to `show`, `due`, scan/send/keep, headers, or logging. Default rescan remains 200 ms and refresh remains min(1 s, lease/3), with the default 3 s lease. Header UTC, supplied `at_us`, boot creation, real mtime lease expiry, datagram sends, totals, dropped/dead outcomes, one-time error suppression and recovery are unchanged.

No existing fixture or Memory implementation opts into these constructors in this change. This is a scheduling source seam; it neither advances a reporter nor acknowledges completion and claims no speedup.

## Source preservation

`audit_source.py` removes only the additive constructors/docs/state/module and reverses the three cadence substitutions. Both complete `publish.rs` and `lib.rs` are recovered byte for byte from their saved baselines. No existing body is replaced from the baseline during reconstruction. Removing the one added docs section also restores `bus.md` exactly.

The original in-file test tails of both changed Rust files are byte-identical, all 11 remaining original Bus Rust files are unchanged (including all integration and separate unit-test sources), and all 24 Memory Rust files are unchanged. `source-preservation.json` records the exact hashes, identities and sample checks; `publish.diff`, `lib.diff` and `bus.diff` show the entire owned change.

## New boundary tests (written, not run here)

- `undeclared_construction_and_invalid_names_or_declarations_do_not_read_the_clock`
- `a_real_declaration_write_failure_precedes_the_first_cadence_sample`
- `declaration_and_first_delivery_take_separate_samples_and_send_the_real_header`
- `invalid_messages_do_not_read_cadence_or_advance_accepted_counts`
- `frozen_reads_hold_scans_and_presence_until_the_exact_default_boundaries`
- `a_root_obstruction_is_discovered_at_the_real_rescan_boundary_and_recovers`
- `a_presence_obstruction_waits_until_one_second_and_unlisted_recovery_keeps_its_time`
- `the_real_mtime_lease_remains_independent_of_a_frozen_publisher_clock`
- `ordinary_constructors_keep_host_samples_and_publishers_remain_send_and_sync`

Tests freeze and externally advance an `Arc<Mutex<Instant>>`; the read counter observes calls without advancing time. Successful real Unix datagram sends are read nonblocking; there are no sleeps, polling retries, global clock switches, shortened default intervals, or fabricated deliveries. Filesystem obstructions prove exact scan/presence equality and original failure suppression/recovery. A deliberately stale real mtime proves that lease removal remains on wall time.

## Validation and limits

Rustfmt 1.96.0 on the three owned Rust files and its `--check` passed. The source-reversal/hash audit passed. No Cargo compilation, clippy, tests, Git, network, device access or shared target-cache use was performed here; the parent owns those gates. The new unit-test filter is `publish::clock_tests` (nine identities). No runtime result or latency reduction is claimed.

## SHA-256

- `lampOS/crates/bus/src/publish.rs`: `53a43999a7035b31e06d29abfb4d0975f514ca0402f4e2614d46796894e1e563`
- `lampOS/crates/bus/src/lib.rs`: `3ab858b5195382e190b4988415177c7cf0b567cae33ecdbf097167eb7978b919`
- `lampOS/crates/bus/src/publisher_clock_tests.rs`: `62ab586860f422f223017ab690dbf4897f8cb8c593cdca94e8f5253e8ebc3e54`
- `lampOS/docs/bus.md`: `d7785924ca711fb58d57480e37407eb6ec66dca6fb3aeda10cc7ae03c61839b0`
