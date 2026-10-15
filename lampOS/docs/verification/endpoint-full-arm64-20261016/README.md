# Endpoint cleanup: complete native qualification

Recorded 2026-10-16 02:03 +0700, from `date`. PR #872 head `22d369ad43b51c52f9293df5059c9769fbe2f1a9`, rebased on the actual merged relay and Tasks producer fix `587a361a3`.

- Normal ARM64 [38787251675](https://github.com/autonomous-ai/lampOS/actions/runs/38787251675): **4,762 PASS**, 127 profile exclusions. This profile excludes the affected endpoint module.
- Full ARM64 [38787351254](https://github.com/autonomous-ai/lampOS/actions/runs/38787351254): **4,900 distinct PASS, zero FAIL/LEAK and no duplicates** across four shards. All original 123 identities occur exactly once.

| Actual endpoint case | Outcome | Seconds |
| --- | --- | ---: |
| `a_worker_still_loading_leaves_the_baseline_window` | PASS | 4.379 |
| `unconfirmed_endpoint_exit_fails_mute_cleanup_and_never_reopens_service` | PASS | 1.519 |
| `endpoint_fixture_reaps_on_panic_before_and_after_handoff` | PASS | 3.027 |

No native retry was used. The previous head's unchanged Tasks stderr failure remains in [prior evidence](../endpoint-full-arm64-20261015/README.md). After the actual owner fix landed, this new head passed fresh local fmt, strict Voice/Memory/Evaluator clippy, workspace build and all three targeted real-child cases (9.030 s, no leaks/retries); both owned files were unchanged across rebase. The original complete 1,767-pass/13-existing-ignore local gate remains retained at its original base in the PR evidence, not relabelled as a new full local gate.

The full native original-123 sum is 848.976 s across concurrent shards, not standalone serial elapsed time. This branch predates the separately merged finite-loopback migration; it is endpoint cleanup qualification and is not a new measurement of the finite-clock work. This change preserves every original test identity, all 111 assertions, inherited stderr, supervisor reaping and production bounds. No lamp/device execution was made.

The JSON result parser matches each nextest outcome's status, duration, binary and test name, rejects duplicates/missing original identities, and retains all original log bytes. Metadata identifies the exact head. A scratch Python orchestrator performed parsing only; none is committed under lampOS.
