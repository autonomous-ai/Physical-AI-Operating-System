# Provider observation first-gate handoff

Recorded 2026-10-16 06:48:06 +0700, using shell `date`. The first gate is **red and preserved**. Actual source remains frozen at the four hashes in `first-source-snapshot.json`; no correction has been applied and no corrected gate has started.

Tree: `/private/tmp/lampos-tests-provider-observations`; base `e7d0f673d0c4bde4a770b19783d6084a8d935ec3`. Commands ran from its `lampOS/` directory with Rust 1.96.0, locked/offline dependency use, jobs 2, incremental 0, and target `/private/tmp/lampos-tests-capacity/lampOS/target`. Real sockets and children ran with the approved escalation. Native command handle 85484 completed with exit 1; no background gate remains.

| Stage | Started (+0700) | Finished (+0700) | Exit |
|---|---|---|---|
| fmt | 06:14:51 | 06:14:57 | 0 |
| clippy | 06:14:57 | 06:15:59 | 0 |
| list | 06:15:59 | 06:19:45 | 0 |
| focused | 06:19:45 | 06:21:21 | 100 |
| tests | 06:21:21 | 06:45:00 | 101 |
| workspace-build | 06:45:00 | 06:46:35 | 0 |

The focused nextest run was 19 passed and one failed (20 executed, 1,410 excluded by the explicit new-test filter; zero retries). The complete affected Cargo run was 2,262 passed, two failed and 13 ignored, with zero filtered or measured tests. All 82 test binaries and four zero-test doc-test groups completed. Workspace build passed afterward. No gate or failed case was rerun.

The compiled comparison retains every one of the clean e7d baseline's 2,257 binary/test identities and ignored flags, and adds exactly the 20 reviewed cases: 2,277 total. Memory remains 210 listed/5 ignored; Surround 260/0; Voice changes 1,410 to 1,430 with 8 ignored; Evaluator remains 377/0. See `first-compiled-list-comparison.json` and `first-compiled-package-counts.json`.

At completion all 6,792 protected files and all four owned source hashes were exact. Whole-worker mechanical reversal still reconstructs the original worker; the old inline test module contains **105 test functions (including two ignored) and 544 assertions**, all byte-exact. The earlier source handoff's regex count of 103 is corrected by `handoff-inventory-addendum.md`; the compiled identity check independently covers all ignored cases.

## Preserved failures

1. New `provider_worker::tests::observation::unpolled_real_worker_drop_never_claims_a_queued_stop_or_return`: both focused and full execution fail at new test line 141 because the post-peer-Drop receive returns OS 54 `ConnectionReset`, then the test unwraps it. The preceding `DroppedBeforePoll` and no-Stop assertions passed. Its final await of the actual connector attempt-future Drop receipt was **not reached**. `first-focused-triage.json` and both raw logs retain this failure.
2. Original `coordinator::tests::reminder_roundtrip::actual_consumer_ipc_coordinator_and_two_retirements_ack_only_after_whole_completion`: line 338 fails to match `Some(Resolution::AckConfirmed { .. })`. The preceding checks established `pending() == false` and exactly one Ack request. The actual resolution was not printed and is unknown. Its source is unchanged; no root-cause claim follows from that fact. `first-reminder-failure.txt` and `first-reminder-triage.json` retain the exact block and source hash. Root owns issue mapping.

## Exact proposed correction, awaiting root review

`proposed-unpolled-close-only.diff` changes only the one new socket test. Before Drop it requires a successful empty receive, so every event and error fails. After Drop a match accepts only `Ok(None)` or the precise `ErrorKind::ConnectionReset`; any event or different error panics. It preserves the body/no-Stop assertions and the final actual attempt-Drop wait with the existing immutable deadline. No test identity, original test, observer implementation, worker, clock, policy or document changes.

Proposal diff SHA-256: `e23bb033b990c80af1fe2d8baf1e97d9469c2fd7dd8f83ca429034fe42d12015`. Proposed test source SHA-256: `25e230b6ccec6a5273f65a6d37f97f6efbbb4cada473c6b7bf0a3d742fbbc3f8`. The candidate is saved only under this evidence directory's `proposed-source/`; the actual tree still has test source SHA-256 `671b70c3d4e528ce5d8c330e34acf5d5897c314b1e34a73223f7607ef090a640`. Neither proposed compilation nor execution has occurred. Do not treat the proposal as validated source.

## Exact commands

`cargo fmt --all -- --check`

`cargo clippy --locked --offline -p lamp-voice -p lamp-memory -p lamp-voice-eval -p lamp-surround --all-targets -- -D warnings`

`cargo nextest list --locked --offline -p lamp-voice -p lamp-memory -p lamp-voice-eval -p lamp-surround --message-format json`

`cargo nextest run --locked --offline -p lamp-voice --profile default --test-threads 1 --no-fail-fast --retries 0 -E test(/^provider_worker::observation::tests::/) | test(/^provider_worker::tests::observation::/)`

`cargo test --locked --offline -p lamp-voice -p lamp-memory -p lamp-voice-eval -p lamp-surround --no-fail-fast -- --test-threads=1`

`cargo build --locked --offline --workspace`

The `first-*.command.json`, `.started`, `.finished`, `.exit`, and `.log` files preserve commands, dates and outcomes. `first-result.json`, `first-completed.json`, and `first-gates.json` summarize this attempt; `first-source/` stores the exact frozen four-file input. No Git mutation, GitHub/network write, package publication or device operation occurred.
