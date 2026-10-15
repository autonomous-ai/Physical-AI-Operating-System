# Full ARM64 audit for PR #872

Recorded 2026-10-15 23:33:27 +0700, from `date`. Read-only audit of the saved [full run](https://github.com/autonomous-ai/lampOS/actions/runs/38766285196) at `b34cd3b90a3be77fa9bce5c03cc3ae9b64070318` (`workflow_dispatch`); conclusion **failure**. No retry, source edit, GitHub write, network call or escalation was made.

The exact failing identity is **`lamp-tasks::runtime command_line_rejects_duplicate_fields_before_connecting`**, shard 2/4, **0.005 s**. It panicked at `crates/tasks/tests/runtime.rs:430:9`:

```
assertion failed: String::from_utf8_lossy(&output.stderr).contains("bad_request")
```

Raw log lines 7073 and 7088–7089 provide the result and assertion. The test loops over two malformed requests, but the log does not print which input failed or the captured child stderr. It is not enough to diagnose the cause or label the failure flaky.

| Endpoint target | Full ARM64 result | Shard | Time | Log line |
|---|---|---|---|---|
| `unconfirmed_endpoint_exit_fails_mute_cleanup_and_never_reopens_service` | PASS, no LEAK | 1/4 | 1.518 s | 4480 |
| `a_worker_still_loading_leaves_the_baseline_window` | PASS, no LEAK | 3/4 | 4.379 s | 9082 |
| `endpoint_fixture_reaps_on_panic_before_and_after_handoff` | **Not run** | 2/4, inferred from partition completion | — | no result row; fail-fast warning at 7096 |

The new panic-cleanup identity is present without `#[ignore]` in the tested head; the full invocation includes its binary, and the other three partitions completed. The failed partition stopped before its Voice tests. Do not claim all three endpoint targets passed on ARM64. No `LEAK` status appears anywhere in the full log, but 446 selected tests did not run.

| Shard | Selected | Run | Passed | Failed | Nextest execution |
|---|---:|---:|---:|---:|---:|
| 1/4 | 1,211 | 1,211 | 1,211 | 0 | 290.227 s |
| 2/4 | 1,237 | 791 | 790 | 1 | 49.299 s |
| 3/4 | 1,140 | 1,140 | 1,140 | 0 | 335.623 s |
| 4/4 | 1,276 | 1,276 | 1,276 | 0 | 292.348 s |

Across the four disjoint nextest partitions: **4,864 selected; 4,418 run; 4,417 passed; one failed; 446 not run because shard 2/4 stopped on failure**. The repeated final FAIL row is a summary of the same failure, not a second execution. The per-shard “3,625 / 3,690 / 3,664 / 3,761 skipped” counts are partition/filter exclusions and must not be summed or described as globally skipped cases. Doc tests and the separate own-harness step succeeded on shard 1/4; `relay_signals: ok` is at line 5045 and turn-end-wire reports 12 passed at line 5063. These are separate from the nextest totals.

The actual rebased PR base is `a3dba0c0af457774e5fe7e09af42828c311738b9`, the head's first parent and the committed gate's `rebased_gate_base`. The complete failing file is identical at base/head, Git blob `84c8f8d5d58ede3aae3012dfc1157ead804d6b41`. The task CLI source is also identical, and the **entire `lampOS/crates/tasks` tree is identical** (`a41ee9ca94c3ccea4372e260d6eaf4eab297fa23`). PR #872 changes endpoint-test fixtures and documentation/evidence only. Classification: an unchanged task-test failure outside this PR's modified source; the log alone does not establish whether the base would fail on a repeat or the underlying mechanism.

CI's own report job linked [issue #874](https://github.com/autonomous-ai/lampOS/issues/874) at log line 9464. This audit did not fetch or change that issue. Exact rows, source object comparisons, input hashes and all deduplicated nextest results are in `arm64-full-extracted.json`.
