# Transport native qualification, 2026-10-16

Audited 2026-10-16 09:21:03 +0700, from shell `date`, for [PR #957](https://github.com/autonomous-ai/lampOS/pull/957), exact head `ad0e7f447e7a67e4c00666207f537d2f260e498e`. The normal PR check and completed full-run composite pass. The first full attempt remains a preserved infrastructure failure, not a first-attempt pass.

| Evidence | Unique nextest PASS | Fixed original 123 | Seven additions | Evaluator identities |
|---|---:|---:|---:|---:|
| [Normal 38836333333, attempt 1](https://github.com/autonomous-ai/lampOS/actions/runs/38836333333/attempts/1) | 5,227 | 0 | 7 | 459 |
| [Full 38836332567, attempt 1](https://github.com/autonomous-ai/lampOS/actions/runs/38836332567/attempts/1) | 4,091 | 83 | 6 | 371 |
| [Full 38836332567, completed composite](https://github.com/autonomous-ai/lampOS/actions/runs/38836332567/attempts/2) | 5,357 | 123 | 7 | 505 |

Every counted identity passes exactly once with zero FAIL, LEAK or duplicates, complete per-shard ordinals and matching starts/summaries. Normal contains exactly the 505 locally compiled Evaluator identities minus its existing 46 PR-profile exclusions. The full composite contains all 505. The raw logs separately confirm four doc tests, the IPC `relay_signals: ok` harness, and twelve turn-end-wire process cases in both normal and retained full shard 1; these are not added to nextest totals.

First shard 3 job **116766212679** failed before tests while downloading the `rustls-platform-verifier-android` registry entry, with curl 55 / OpenSSL SSL_ERROR_SYSCALL. Its test step was skipped, with zero nextest start, outcome or summary markers. Its complete 30,140-byte API log is byte-exact to the already published [fb0a020f9 evidence](https://github.com/autonomous-ai/lampOS/tree/fb0a020f977d54f3d4d96ea3e3ce743471c192e3/lampOS/docs/verification/transport-native-first-failure-20261016), SHA-256 `347b11aeb905bdd67309d03e2c05677ef0095055db0ee79272310e5cc7f012a5`. API bytes and the CLI display form are both retained; removing exactly two tab-prefix fields makes them identical.

Root restarted **only that unexecuted job** at shell date **09:04:55**. New shard 3 job **116773129888** contributes **1,266 first executions**, exactly the missing 40 original identities, one new helper case and 134 Evaluator identities. The three successful original shards retain byte-exact complete logs, unchanged execution start/end times and every step state/timestamp. No failed or successful test was retried. Attempt-2 job record IDs are recorded separately and are not presumed equal to original execution IDs.

| Shard | Actual execution attempt | Actual execution job ID | Attempt-2 record ID | PASS |
|---|---:|---:|---:|---:|
| 1 | 1 | 116766212790 | 116773131164 | 1314 |
| 2 | 1 | 116766212750 | 116773169615 | 1375 |
| 3 | 2 | 116773129888 | 116773129888 | 1266 |
| 4 | 1 | 116766212430 | 116773170830 | 1402 |

All four full checkouts and the refreshed remote commit/tree equal `ad0e7f447e7a67e4c00666207f537d2f260e498e`. All **1,022 frozen local-gate inputs** match that full head. Normal tested merge `96ccd9e30c840c5e322b54e04b0a9c5b1800cc22`, whose parents are `1fb07c6f57ee280a49de7b4ddd74008c2a32a20d` and the PR head. The normal merge contains exactly 19 known upstream path changes, including 13 frozen Voice/Gemini inputs; those dependency sources are explicitly not claimed identical to the full head. All **77 Evaluator files**, the three owned source/doc files, workflow, nextest configuration and Cargo lock/workspace files are exact in both tested trees. The retained compiled comparison proves all 498 existing identities/ignore flags plus exactly seven additions, zero ignored.

The original transport-expiry identity passed once on original full shard 4 at **0.005 s**. The fixed original 123 printed durations sum to **228.101 s**, across concurrent shards with shard 3 first executing in a later infrastructure attempt. Neither number is a controlled native before/after pair or a standalone serial 123-test measurement. The separate local one-sample pair **5.055 → 0.047 s** retains its shared-Mac load and source limits in the [code PR evidence](https://github.com/autonomous-ai/lampOS/tree/ad0e7f447e7a67e4c00666207f537d2f260e498e/lampOS/docs/verification/transport-exit-clock-20261016). The original-123 standalone serial target remains unproved.

`first-attempt/` remains 77 byte-exact files, including the complete original logs, metadata, compiled/source proofs, published-failure comparison, root restart receipt and parser-only diagnostics. `final/` holds the refreshed attempt-2 logs/API capture receipts, composite source/identity audit, and separate doc/harness audit. All first-stage bytes are checked against `first-attempt-manifest.json`. The final raw file has only the newly executed shard 3 plus the unchanged three original executions. Root owns publication and readiness; this auditor made no source edits, Cargo/test executions, Git/GitHub mutations or device actions. `SHA256SUMS` covers every other package file.
