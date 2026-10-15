# Records real-child control handoff — #823

Current source update at **2026-10-16 05:22:13 +0700**: the close-order subcase below supersedes only the parent/framing test-file hash from the initial freeze. All other source remains frozen.

Source frozen at **2026-10-16 05:12:30 +0700**, from `date`, in `/private/tmp/lampos-tests-records-controlled`, branch/base supplied by root: `tests/823-records-controlled`, `e3d3366cad8113c3374b8e29faedb099b5e47d92` (preparation PR #922). No Git command, Cargo, compiler, runtime test, network or device operation was run by this worker. Root owns independent review, the exact before/after measurements and execution gates.

## Implemented scope

Exactly the ten planned paths changed. `application.rs` and `report.rs` are byte-identical. The fixture binary includes the actual shared application, keeps its parser/exit handling and passes the explicit reporting schedule/factory only at the existing report-command hook. Its manifest entry has `test=false`/`bench=false`; no dependencies or lockfile changed. Ordinary main gained only three test-module declarations; all five old root tests retain their bytes/names.

The child owns synchronous inherited-pipe control and one pure checked nanosecond clock shared with Bus cadence. Bootstrap is version/nonce validated with a four-second absolute startup bound. Each reporting wait uses the unchanged real `host_started + host_seconds` deadline, so the original selected `--seconds 30` also bounds a blocked control read/write. An idle control timeout is an actual error exit 64 through the shared application; it never manufactures ordinary success. No worker thread, socket listener, stderr marker, provider/IPC clock or wall-clock replacement was added.

The first Step must settle time zero after actual cycle 1 completes. Same/pre-due Steps stay within that wait with the same completed-cycle/outcome fields. An exact due Step releases the original one-second wait but is acknowledged only at the next positive wait after actual loading/construction/publication/error handling and host-lifetime evaluation. Zero residual and invalid context are rejected without a parked receipt. Offset/Instant/due/round/count arithmetic is checked. Protocol/time/host errors close and permanently latch; publication/construction failures remain truthful ReturnedErr observations and preserve the existing next-cycle reconstruction behavior.

`Checked {version, nonce, round, at_ns, completed_cycles, cycle_started_ns, next_due_ns, construction_or_publish}` does not claim a publication attempt, delivery or exit. Every frame is LF-delimited and at most 4096 bytes. Nonblocking actual pipe I/O uses one absolute deadline through partial writes/reads and EINTR/poll; queued bytes are read before EOF, with no socket timeout mutation. All actual endpoints are unbuffered File/ChildStdin/ChildStdout. Disposable test pipes use `std::io::pipe`; raw child probe/stdio duplicates use close-on-exec duplication.

The parent owns Child immediately after spawn and before descriptor setup. It retains `.0` as the actual Child, validates the complete response before committing time and closes permanently on a failed exchange. `Window` retains one original four-second host deadline and four-second logical interval; it is passed through all steps in a search. The actual try_wait/kill/wait cleanup body is preserved, with explicit control close first. Kernel/filesystem/stderr/reap operations do not acquire a universal hard-return guarantee.

## Original case preservation

Only helper/import/spawn code changed in `real_reporter_reloads_switch_and_policy_and_requires_a_matching_receipt`. Its exact original four-second deadline expression remains at the same helper entry, before added control work. It first settles the child, then drains the actual Subscriber with zero-wait reads. Only an empty receive may request the next full due interval. Each successful result is still the original actual Diagnostic with the original matching predicate; Checked cannot supply it. At a logical window's exhausted end no result is accepted.

All six original states and their actual file writes remain in order: disabled/no receipt; enabled/old fingerprint; matching 111; age-policy mismatch; matching 222; disabled with the final actual receipt still enabled/current/222. The scenario after `let tree` is byte-identical after projecting out only the explicit spawn expression. All **16 assertion macro texts** are byte-identical, including the two original host/alive assertions. `field` and `receipt` were not edited. Exact argv tail remains `report CONFIG RESULT --seconds 30`; stderr stays inherited, and only the dedicated executable/private stdin/stdout route changes. The whole migrated function is not described as byte-identical.

## New boundary source coverage

Six main-binary module tests cover pure reads; exact same/pre/due intervals and delayed replies; initial identity/round/future time and post-settle stale/backward/future rejection; zero wait and changed real lifetime; two genuinely held real publisher factories (first and after an actual config/bus-root reload); and actual construction failure with next-due recovery. The held operations use bounded channels/scoped owned threads. A real pipe probe and real Subscriber prove no Checked/Event before release, rather than passing an already-expired read deadline.

Fourteen separate integration tests cover actual child Events/sequence at three full due intervals; ReturnedOk without an actual subscriber; the **real one-second allowed child lifetime** expiring while idle (actual exit 64); control close and actual reap; invalid bootstrap before any Event; partial setup and parent panic cleanup; a complete queued receipt before peer close; every initial identity/cycle field; same/pre-due fabricated work/outcome; invalid due reply after peer advancement; immutable full-period/logical-end limits; malformed/truncated/unknown fields; fragmented actual-pipe reads and oversized byte bounds; and actual full-pipe backpressure under the **full four-second absolute deadline**. The latter two host-bound cases intentionally retain real waiting. No synchronization sleeps or flaky retries were added.

Validation-controller tests use deliberately malformed peer receipts only to exercise validation; they do not claim real application work. Real application/factory/datagram/child tests separately establish that boundary. All new cases remain **unexecuted by this worker**.

## Evidence and gate handoff

`audit-source.py` mechanically reverses named edits in all four existing changed files and reconstructs each saved original byte for byte. `source-proof.json` records ten hashes, 16 assertion hashes, old IDs, the unchanged main test-module hash and the 20 added source IDs. `before/`, `after/`, `restored/`, and `owned.diff` retain the exact reviewed bytes. All 17 other captured original files, including the two corrected Scratch tests, production application/report and Bus dependencies, match their initial hashes.

`prior-preparation-compiled-list.log` is an exact raw copy of root's completed preparation list, with its original build-prefix lines retained; its hash is recorded. It contains 127 Records identities (1 ignored on that host). A fresh same-platform Records list should therefore add the 20 source cases, without dropping or renaming any original identity or ignore flag. This is an expected shape, not a fresh compiled result. Source audits initially needed scratch-only selector/parser fixes; no executable test was rerun or failed here.

Pinned Rust 1.96 rustfmt with edition 2024 and `skip_children=true` passed `--check` on the eight owned Rust files; exact command/time/exit 0 and empty stdout/stderr are in `format-result.json`/`rustfmt.*`. Root should perform formatting, strict Records all-target Clippy, fresh list/identity comparison, the new boundaries, serial paired original-case execution, all appropriate Records/workspace tests/build and native gate. Retain every first failure and its exact source before any repair; no speedup is claimed until the paired real-child run succeeds.

Review focus: child due request must remain pending until the next actual positive wait; complete response validation must precede parent clock commit; late/invalid exchanges must stay permanently closed; actual Events and actual exit/reap remain separate from receipts; EOF and all partial-setup paths must retain owned cleanup. No production API correction was needed beyond the already published preparation.

## Frozen SHA256

| File | SHA256 |
| --- | --- |
| `lampOS/crates/records/Cargo.toml` | `d624cddfc993a78af2510a9fef47ef192221da0a49c00051aa2ab63da25dada1` |
| `lampOS/crates/records/src/main.rs` | `40d8eba533e42937e71b2995ae022acb2d7e1994e8e162005c166726041a2400` |
| `lampOS/crates/records/src/bin/lamp-records-fixture.rs` | `56ed3d6bd78679b9f93b911ba531487fc02507abd88f4903939920aa360604d7` |
| `lampOS/crates/records/src/fixture_protocol.rs` | `4d2bd95d5b273b7af70a408d513fa47a6ceb4e9d16fb81c68e604520dfdb48d3` |
| `lampOS/crates/records/src/fixture_schedule.rs` | `54a8e0c9562ceada01ce3b0d26bdc2d92c0d5d8e954e880e011ddaef71a9a187` |
| `lampOS/crates/records/src/fixture_schedule_tests.rs` | `cb7ff4cf0928ef0dd539ce696f6036943e788f8fbe80482b7815205a9b1e46c1` |
| `lampOS/crates/records/tests/support/records_control.rs` | `b769a36f1cf542c436704cbf90ca1a50bf3b6ab371d907be95c5c1f0f67e8234` |
| `lampOS/crates/records/tests/records_control.rs` | `04083e277b0d02d65ab4193afe7814218b60e287059a81e7cf7a46e2a7d51924` |
| `lampOS/crates/records/tests/records.rs` | `ed3504349c3a48400f2db666a5653588eee25cc602d664b95bea5962a20535f9` |
| `lampOS/docs/records.md` | `940ea122d4b3328d5aac12a3cdf2cecf2eb2e262a75a8244e06d43c5c1014ce2` |

## Deterministic closed-writer addendum — 2026-10-16 05:22:13 +0700

Root authorized only a new subcase inside the existing `a_complete_checked_queued_before_peer_close_is_consumed_and_committed` identity. The subcase synchronously writes a complete Checked using the existing bounded nonblocking Channel and one absolute host deadline, then explicitly closes the peer's sole reply writer **before** the parent Channel reads. It requires the exact complete receipt followed by actual EOF. The existing threaded Controller subcase, all its assertions and the other 13 parent/child/framing cases are byte-identical. The six schedule cases and implementation are unchanged; total new identities remain 20. No sleeps, retries, worker, synthetic Event or process-exit claim was added.

Only `lampOS/crates/records/tests/records_control.rs` changed, from SHA-256 `d2e2a7cfc165b966006c949859ddea8ace92989fde73c45218e3cd0be0fcf22f` to `04083e277b0d02d65ab4193afe7814218b60e287059a81e7cf7a46e2a7d51924`. `closed-peer-order-before/` preserves the prior exact test source, source proof, handoff, full diff, format result and independent review. Removing the 21-line addition reconstructs the full pre-edit test file byte for byte. `closed-peer-order.diff` and `closed-peer-order-proof.json` record this exact delta, unchanged other nine hashes, four continuing original-file reversals and all 16 original scenario assertions.

The refreshed `source-proof.json`, `after/` snapshot and `owned.diff` contain the new source. Its proof SHA-256 is `4f061f36b6c26b489ba9eb3e7125f581bdc3a9fc05d8bc729631ab79fb459905`. Pinned Rust 1.96 `rustfmt --edition 2024 --config skip_children=true --check lampOS/crates/records/tests/records_control.rs` passed; the exact command, date boundaries, exit 0 and empty output are preserved in `closed-peer-order-format.json` and `closed-peer-order-rustfmt.*`. No Cargo, compilation, runtime test, timing, Git, network or device operation was run. Root retains runtime qualification.
