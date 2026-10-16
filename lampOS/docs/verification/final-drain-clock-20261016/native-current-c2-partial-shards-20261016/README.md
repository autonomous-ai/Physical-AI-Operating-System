# Final-drain c2: terminal three-shard supplement

Recorded 2026-10-16 11:11:38 +0700, from shell `date`. This is a separate supplement to the [frozen first native failure record](../native-current-c2-first-attempt-20261016/README.md), whose 37 files remain byte-exact. It records the terminal successful shards from the same **full run 38846183455, attempt 1**, all actually checking out `c2eacfa4e29a5c9db21d4d919a77d33ce8112238`. The full run remains **failed** because shard 4 failed before testing. The separate normal run also remains failed before testing on synthetic merge e7b. No result from the newer f5/db720 integration is included.

| Full shard/job | Unique passing tests | Nextest reported execution span |
| --- | ---: | ---: |
| [1 / 116797126621](https://github.com/autonomous-ai/lampOS/actions/runs/38846183455/job/116797126621) | 1,344 | 148.459 s |
| [2 / 116797126863](https://github.com/autonomous-ai/lampOS/actions/runs/38846183455/job/116797126863) | 1,401 | 160.646 s |
| [3 / 116797126912](https://github.com/autonomous-ai/lampOS/actions/runs/38846183455/job/116797126912) | 1,287 | 141.415 s |

The union contains **4,032 distinct PASS identities**, with contiguous result ordinals in each shard, no duplicate identities between shards, and no retry or failed-result markers. These are partial observations, not a successful full gate. Each aggregate nextest skipped count also includes partition/filter exclusions; those numbers are neither ignore counts nor summed here. Raw logs and job/run APIs remain unedited. The release workspace binaries/examples build passed on each successful shard. Shard 1 additionally passed 39 documentation targets (four actual doc cases), relay_signals, and all 12 process-harness cases. Those extra stages are intentionally skipped in shards 2/3. This workflow does not run fmt or Clippy; no native strict-Clippy claim is made.

| Expected identity set | Observed once, PASS | Not observed |
| --- | ---: | ---: |
| Immutable original 123 | 93 | 30 |
| Prior local c2 compiled 521 | 397 | 124 |
| Six final-drain fixture additions | 4 | 2 |

`coverage/` gives every expected identity's actual status and the complete missing lists. Missing means no result was observed in these successful shards; it does not mean pass, ignored or deleted. The failed fourth shard supplies no execution or compiled per-test list. A per-identity assignment to that unexecuted shard is not fabricated.

The missing additions are `a_complete_real_line_is_released_only_at_the_full_logical_boundary` and `abandoning_the_initial_hold_stops_and_reaps_the_actual_shell`, both under `lamp-voice-eval::final_dispatch` / `final_drain_clock::tests`. The selected original `a_session_end_first_seen_in_the_final_drain_withholds_and_is_kept` passes once in shard 1 at the reported **0.055 s**. This is a single native observation, not a comparable before/after pair, complete six-case qualification or complete-123 timing proof.

`source-input-proof.json` checks the immutable c2 tree, all three owned source/doc paths, all 53 prior evidence files and five workflow/filter/build inputs against their previous freeze. Every raw checkout log prints c2. No active worktree HEAD is used; the normal run's e7b synthetic merge and its upstream compilation defect remain separately documented in the first package. `expected-full-membership.json` is copied unchanged from that earlier expectation snapshot, including its then-pending wording; `partial-result-audit.json` supplies this subsequent terminal observation without rewriting history.

The original fourth-shard raw log remains SHA-256 `145700567748d132015d45c863f61d2599344823e0f85f4aa04cc9ede96447dc`: hound registry transfer failed with HTTP2 framing error before test execution. The normal raw log remains `40823d902d2869d25db480701b1914212c3a9f9ebea6638776dc662382cfa1e8`: upstream Reply.say initializers failed compilation. Neither is retried or erased, and neither diagnostic proves an unobserved deeper cause. This supplement establishes no readiness and does not complete the native gate.

The external capture/parser scripts are documented by hash only in `scripts.json`; no script, compiled artifact, symlink or executable file is in this payload. `runtime-identities.tsv` retains all observed outcomes; `per-binary-observed-timing.tsv` sums only reported case durations within each successful shard, not overall elapsed pipeline time. `SHA256SUMS` covers every other payload file, and the external exact destination manifest enumerates the publication set.
