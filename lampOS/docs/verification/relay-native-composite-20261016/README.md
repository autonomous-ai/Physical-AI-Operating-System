# Relay native qualification, 2026-10-16

Audited at `2026-10-16 08:26:49 +0700` from shell `date`, for [PR #942](https://github.com/autonomous-ai/lampOS/pull/942), head `917a19c5c36bdecb965f404385df386c16b22af4`. The normal PR run and the completed full-run composite pass. The first full attempt remains a preserved infrastructure failure; it is not represented as a first-attempt pass.

| Evidence | Nextest passes | Original 123 covered | Relay additions covered |
| --- | ---: | ---: | ---: |
| [Normal run 38831311751, attempt 1](https://github.com/autonomous-ai/lampOS/actions/runs/38831311751/attempts/1) | 5,204 | 0 | 4 of 10 |
| [Full run 38831310695, attempt 1](https://github.com/autonomous-ai/lampOS/actions/runs/38831310695/attempts/1) | 4,065 | 83 | 9 of 10 |
| [Full run 38831310695, completed composite](https://github.com/autonomous-ai/lampOS/actions/runs/38831310695/attempts/2) | 5,321 | 123 | 10 of 10 |

Every counted identity passes exactly once; there are zero FAIL, LEAK, or duplicate rows. The normal run contains exactly the 456 Evaluator identities expected after its existing 52 PR-filter exclusions. The full composite contains all 508 locally compiled Evaluator identities. Both normal and full shard 1 also pass all four doc tests and twelve own-harness tests, counted separately.

Shard 3's first release build failed while downloading `serde` (`curl 55`, `SSL_ERROR_SYSCALL`). Its test step was skipped, with no nextest start, summary, or test outcomes. Only this unexecuted job was restarted. Shards 1, 2, and 4 retain byte-exact original logs, identical execution start/end times, and identical step states/timestamps. GitHub created new attempt-2 records for them; those record IDs are not claimed to be unchanged.

| Shard | Actual execution attempt | Actual execution job ID | Attempt-2 record ID | Passes |
| --- | ---: | ---: | ---: | ---: |
| 1 | 1 | 116750485473 | 116757404816 | 1309 |
| 2 | 1 | 116750485561 | 116757369718 | 1365 |
| 3 | 2 | 116757368540 | 116757368540 | 1256 |
| 4 | 1 | 116750485264 | 116757394226 | 1391 |

The restarted shard adds 1,256 first executions, covering the remaining 40 original tests, one new invocation case, and two original Relay cases. No failed or successful test was retried. All four full checkouts are the exact head above. Normal tested merge `c8f8fb249b3e4165e0c9370110bf3f9dc5efe1b7` includes that head as a parent. All 79 frozen Evaluator inputs match both commit trees; the six changed source/doc files, workflow, and nextest config are also retained.

The six original Relay durations printed by this composite are:

| Original Relay test | Shard | Seconds |
| --- | ---: | ---: |
| `a_closed_runner_transport_stops_the_runtime_through_the_same_path` | 3 | 0.046 |
| `a_nonzero_runtime_exit_after_clean_metadata_is_a_relay_error` | 1 | 0.042 |
| `a_runner_stop_is_one_sigterm_and_a_clean_reaped_end` | 2 | 0.039 |
| `a_runtime_that_ignores_sigterm_is_force_killed_and_never_clean` | 4 | 0.150 |
| `the_hard_deadline_stops_the_runtime_and_is_an_error` | 1 | 0.078 |
| `the_pilot_runtime_settings_reach_the_serve_child_exactly` | 3 | 0.042 |

Their sum is **0.397 s**; the fixed original 123 sum to **207.660 s**. These are printed process-duration sums across concurrent shards, with shard 3 executing later. They are not a standalone serial 123-test run, a controlled native before/after pair, or proof of the serial target. The separately retained shared-Mac historical six-test pair remains historical evidence with its own source and unequal-load limitations; no new timing comparison is inferred here.

`AUDIT-METHOD.md` explains identity, ordinal, source, and composite checks. Raw commands, complete unedited logs, API metadata, both result arrays, fixed inventory, local compiled/source proofs, and the root's job-only restart receipt are retained. `published-b654-infra/` is byte-exact to the [already published first-failure evidence](https://github.com/autonomous-ai/lampOS/tree/b65454e57bb30fb4a8dd57fe53528d2b3aecc5e2/lampOS/docs/verification/relay-native-first-shard-failure-20261016). The root's first prefixed/unprefixed log-hash check stopped before the restart API call; its corrected check changed no log bytes or tests.

`SHA256SUMS` covers every other file in this folder. No Python scripts, new test execution, source edits, Git mutations, or GitHub mutations were performed by the auditor.
