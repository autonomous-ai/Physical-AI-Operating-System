# Scenario Moment preparation: local failures retained, native pending

Packaged at **2026-10-16 05:26:08 +0700**, from `date`, on base `7ae0eb647914e286817c29bff2abc2934fb19546`. This is a **draft preparation**, with exact-head native ARM64 validation pending. The fresh local affected gate remains **1,960 passed / 2 failed / 13 existing ignored**, exit **101**. All **25 new Moment tests and seven unchanged original clock tests passed**. Formatting, strict Memory/Voice/Evaluator all-target Clippy, compiled listing and workspace build passed.

## What this qualifies

The test-only `scenario_clock` module gains immutable origin-bound Moment snapshots, explicit thread-local observers, and inclusive `Moment::between` projection at an absolute nanosecond offset. Reads and projection do not run peers or advance time. An admitted observer remains frozen after its root is invalidated; further adoption fails, and the owning future controller must cancel it. A value or clock reading is not a work, delivery, readiness, or exit receipt.

This changes no production clock, provider worker, Tokio runtime, capture source, real watchdog, dependency, or scenario constructor. It migrates **zero** original #823 realtime scenarios and makes no speedup or device claim. Existing root stepping/pump/Shutdown semantics and all original checks are preserved.

## Fresh retained gate

[Exact commands, paths, target and dates](fresh/root-gates.json) accompany the unmodified raw logs and stage exit files. Commands ran from `lampOS/`, using the pinned toolchain and `/private/tmp/tests-823-rebase-target`:

| Stage | Command | Result |
| --- | --- | --- |
| Format | `cargo fmt --all --check` | exit 0 |
| Strict lint | `cargo clippy --locked --offline -p lamp-memory -p lamp-voice -p lamp-voice-eval --all-targets -- -D warnings` | exit 0 |
| Compiled identities | `cargo nextest list --locked --offline -p lamp-memory -p lamp-voice -p lamp-voice-eval --message-format json` | exit 0 |
| Complete affected tests | `cargo test --locked --offline -p lamp-memory -p lamp-voice -p lamp-voice-eval --no-fail-fast -- --test-threads=1` | **exit 101; 1,960 / 2 / 13** |
| Workspace build | `cargo build --locked --offline --workspace` | exit 0 |

The serial test command ran from 04:50:09 to 05:10:29 +0700; the subsequent build finished at 05:11:55. These are recorded `date` boundaries, not a performance comparison. See [full test output](fresh/root-tests.log), [derived totals](fresh/root-result.json) and [32 clock outcomes](clock-test-results.txt).

The [before](fresh/root-baseline-list.log) and [after](fresh/root-list.log) compiled lists preserve every one of **1,950 prior identities and ignored flags**, adding exactly 25 tests for 1,975 total. The baseline list came from the same-base Records preparation tree; [637 dependency-closure input hashes](fresh/baseline-compiled-inputs.json) establish why its unrelated Records changes do not affect these packages. The [identity proof](fresh/root-identity-proof.json) retains the exact new names. No old case was filtered out of the full test command.

## Both local failures remain failures

- `coordinator::tests::pilot_faults::rapid_mute_and_reopen_never_overlaps_workers_or_carries_speech_across`: `pilot_faults.rs:2395`, `service privacy freshness lost before next observation`. The raw diagnostics record runtime cycle 4, no frames, and a producer privacy gap of 8,114 µs. That producer gap does **not** measure privacy age when the runtime consumed it or prove the cause. This identity is already recorded in [#739](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6198780338) and [#893](https://github.com/autonomous-ai/lampOS/issues/893).
- `reminders::consumer::tests::completed_playout_survives_both_real_store_reopens_then_acks_without_respeech`: `consumer/tests.rs:644`, the restored ledger resolution is not `AckConfirmed` after one successful-returning tick. The exact resolution, request list, transport error and elapsed hop times were not printed. The following engine-count and exactly-one-Ack assertions did not run. The 250 ms host request budget and real persistence make delay plausible, not proved. Both stores reopen **before** this tick. Eighth [explicitly accepted this exact identity into #739](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6198245051); the [full independent triage](reminder-triage/report.md), original issue responses and source hashes are retained. This identity is outside the immutable original 123.

[Joining’s known-failure exception](../../joining.md#one-pull-request-per-task-fifth-p6-decided-2026-10-14) requires the native ARM64 run on the pull request head to be green and the local failures to be named. The exact identities are already tracked; their causes remain unresolved. Until the native result exists, this preparation remains draft. The failed local gate is never described as green, and no whole-gate rerun was used to erase these failures.

## Source and earlier evidence

[Integration proof](fresh/integration.json) records all 6,740 base blobs/modes before editing and all 6,738 nonowned base files unchanged afterward. The three Rust files exactly match the independently reviewed preparation; the docs add only its reviewed section to the newer base. Reversing the recorded additions recovers the base clock and document byte-for-byte. The original seven root test bodies/43 assertion sites remain exact. [Frozen hashes](source-hashes.json), [the actual diff](fresh/integration.diff), [Moment review](reviews/moment-independent-review.md), [between review](reviews/between-independent-review.md) and their source proofs are included. These source reviews themselves did not execute tests; the fresh gate above provides the current execution evidence.

The older **1,741 passed / 4 failed / 8 ignored** gate remains durably available at [commit `2fdb5d7a2fc5472351e1bb3736776966181a4b60`](https://github.com/autonomous-ai/lampOS/tree/2fdb5d7a2fc5472351e1bb3736776966181a4b60/lampOS/docs/verification/provider-moment-local-gate-20261016). It failed three motor cases (`late_started_callback_cannot_restore_an_offer_after_owner_replacement`, `pending_inspection_is_revoked_before_worker_data_and_release_is_separate`, `same_owner_input_end_cancels_queued_gesture_before_inert_bus_write`) and `second_route::without_a_second_route_the_packets_and_records_are_as_before`. That package retains the full failed output and the separate invalid, interrupted sandbox socket-denial attempt. Its [original README](historical/provider-moment-original-README.md) is copied unchanged here. The old gate precedes `between` and this newer base; it is not the current source’s qualification or a discarded retry.

[Provenance](provenance.json) maps exact copies to their original paths; historical reports retain those paths verbatim. Capture scripts and binaries are excluded. `SHA256SUMS` covers every packaged file except itself. No source bytes, raw logs or prior failure evidence were changed while packaging, and no test, build or rerun was performed for packaging.
