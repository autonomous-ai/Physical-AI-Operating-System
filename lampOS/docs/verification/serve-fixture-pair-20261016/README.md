# Serve fixture paired evidence

Audited 2026-10-16 00:28:47 +0700.

Both complete runs passed the same 14 compiled tests with zero ignored, filtered or skipped cases. Original test and assertion-helper bodies are byte-identical; the constructor-only migration retains all 18 run sites and all 22 expanded attempts (18 controlled, 4 retained host). The five fault-loop variants are unchanged.

Nextest execution: **396.970 s before → 111.763 s after**, saving 285.207 s (71.85%; 3.552×). Compilation and listing are excluded.

| Subset | Tests / attempts | Before (s) | After (s) | Reduction |
|---|---:|---:|---:|---:|
| Controlled after migration | 10 / 18 | 334.354 | 49.402 | 85.22% |
| Retained host clock | 4 / 4 | 62.610 | 62.338 | 0.43% |

Subset numbers sum the rounded individual PASS durations, not separate invocations. The host subset comprises three recorder cases and the delayed transport / first-preparer case. All 14 measured comparisons, routes, attempts, body hashes and exact identities are in `comparison.json` and `timings.tsv`.

Both used x86_64 Mac debug binaries, nextest profile `realtime`, `--test-threads 1 --retries 0 --no-fail-fast`. **The after run occurred first** (00:06:45–00:08:37 +0700); before followed (00:11:39–00:18:17). The evaluator helper SHA-256 was identical across the swap: `2ea287c65beee3968b41d7e4aff95a342e965ab301f026e1b8f8a06fc516602c`. Only test constructors changed between the two source snapshots; current integration test source matches the restored after snapshot.

**Load limitation:** independent finite full gates ran on the same host. The saved finite release-test interval (00:09:02–00:19:47 +0700) overlaps the entire before invocation; available gate timestamps do not establish overlap with the after invocation. Load was unequal, so this is not a quiet benchmark. These measurements establish neither ARM64 timing, the full realtime group, nor PR push-to-result latency. No retries were used in this pair; the two before `SLOW` notices are notifications for eventual PASS rows, not extra attempts.

The scratch Python audit remains outside the Rust-only repository. It parsed the
retained nextest listings/results, compared each compiled identity and ignored
flag, checked all 14 original test-body hashes and expanded all fault-loop run
sites, and compared the helper/source hashes. It ran no tests itself.
`comparison.json` records commands, run IDs, shell-date boundaries and input
hashes; raw logs and command records are beside it. The `source-*` files retain
constructor reversal, exact original assertions and all 22 attempt routes.
No binaries are committed. The constructor migration is not yet a ready PR;
its dependency merges and complete native qualification remain pending.

| Test identity | Before (s) | After (s) | Route / attempts |
|---|---:|---:|---|
| `a_declared_capture_end_before_the_next_prompt_and_its_window_withholds_it` | 24.625 | 24.897 | host / 1 |
| `a_late_first_copy_or_failed_player_withholds_every_later_turn` | 28.903 | 4.638 | clock / 2 |
| `a_nonzero_runtime_exit_never_completes_a_serve_attempt` | 11.197 | 1.431 | clock / 1 |
| `a_provider_outage_is_waited_out_by_event_and_a_terminal_link_ends_it` | 36.859 | 5.000 | clock / 2 |
| `a_recorder_that_exits_zero_mid_conversation_withholds_the_rest_and_certifies_nothing` | 8.037 | 7.272 | host / 1 |
| `a_room_recording_that_ends_early_withholds_the_rest` | 7.225 | 7.563 | host / 1 |
| `a_runtime_stop_mute_or_ignored_sigterm_is_retained_and_reaped` | 51.091 | 7.720 | clock / 3 |
| `an_extra_candidate_during_the_wait_withholds_the_rest` | 14.138 | 2.704 | clock / 1 |
| `delayed_readiness_is_awaited_by_event_not_by_completion` | 31.510 | 4.387 | clock / 1 |
| `normal_eight_turn_conversation_in_one_serve_session` | 68.799 | 9.099 | clock / 1 |
| `serve_readiness_comes_only_from_the_fresh_cue_never_from_stdout` | 11.218 | 1.504 | clock / 1 |
| `stale_duplicate_reordered_foreign_and_lost_cues_end_the_conversation` | 68.700 | 11.479 | clock / 5 |
| `startup_cues_stay_fresh_through_a_slow_transport_and_a_slow_first_preparer` | 22.723 | 22.606 | host / 1 |
| `the_runner_forwards_pilot_runtime_settings_through_the_relay` | 11.939 | 1.440 | clock / 1 |
