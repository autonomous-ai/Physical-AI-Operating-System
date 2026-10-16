# #963 final-drain native ARM64 qualification

Audited at shell date **2026-10-16 09:49:49 +0700** for head `c5bcc45b905458c1a8adcd8bcf06885f35dbfdf7`. Both runs succeeded on attempt 1; no job or test was retried. [Normal run 38839828555](https://github.com/autonomous-ai/lampOS/actions/runs/38839828555) and [full run 38839828296](https://github.com/autonomous-ai/lampOS/actions/runs/38839828296) are separate executions on the sources below.

| Check | Normal | Full |
|---|---:|---:|
| Unique nextest PASS identities | 5,251 | 5,376 |
| Exact Evaluator identities | 452 | 504 |
| Fixed original123 identities | 0, expected filter | 123, exactly once |
| Six new fixture cases | 0, expected filter | 6, exactly once |
| FAIL / LEAK / duplicate result identities | 0 / 0 / 0 | 0 / 0 / 0 |
| Separate doc tests passed | 4 | 4 |
| Separate IPC relay harness | passed | passed |
| Separate turn-end-wire process cases | 12 passed | 12 passed |

Full shard jobs `116777121667`, `116777122005`, `116777121941` and `116777121946` passed **1,316 / 1,379 / 1,269 / 1,412** tests respectively, with exact ordinals and matching start/summary totals. Each log checkout is the exact head. All 1,023 frozen gate inputs and all 56 published files match immutable local Git objects and the complete GitHub head tree. All old 498 Evaluator identities and ignore flags are retained; the local inventory has 504 cases with zero ignores.

The normal job `116777119417` tested synthetic merge `3b41a399c6ff693f7afc342241119940bcbd0953`, whose parents are upstream `f4b5b0cff8450de33ae394e363ac6f9cc0eab615` and the exact head. Its 39 upstream path differences are enumerated. Every published file and the entire Evaluator source subtree remain exact, as do the workflow and filters. Twenty-six frozen inputs differ, including Voice and Cargo.lock, and the upstream Eyes source has an added file. This is not a claim that both runs had identical dependency bytes.

The selected original final-drain case passed at printed **0.053 s**. The 123 printed case durations total **232.213 s** across concurrent shards, each executing its assigned cases serially. Neither number proves a native before/after pair or the final standalone original123 serial bound. The earlier local 1.272 → 0.444 s pair remains one observation per version on an uncontrolled shared host, separately documented in [the source verification](https://github.com/autonomous-ai/lampOS/tree/c5bcc45b905458c1a8adcd8bcf06885f35dbfdf7/lampOS/docs/verification/final-drain-clock-20261016).

`full/full-final-audit.json`, `full/full-results.json` and `full/full-original123-and-six-audit.json` retain every actual result, shard, ordinal, raw line and original/new identity. `normal/normal-final-audit.json` and its row array retain the separate filtered run. `full/full-source-audit.json` maps every frozen input and published blob; `normal/normal-source-and-merge-audit.json` maps the actual merge differences. Raw log/API bytes and dated command/exit/hash receipts are preserved under each stage. The [root's independent outcome audit](root-independent-native-audit.json) is a separate exact publication-manifest entry, with its hash in `provenance.json`; it is not duplicated inside this package.

All 70 preparation files and all 36 earlier normal evidence files are copied byte-exact. Their pending/preliminary wording describes their original stage; the completed audits here supply the terminal result. Parser-only diagnostics are retained: the normal header's 274 binaries were incorrectly equated with 248 distinct printed names; a preview expected a map instead of the selected identity's two-element list; and an unanchored doc regex included the job prefix/timestamp. `full/full-doc-and-own-harness-audit.json` reparses the four original normal raw lines and supersedes only those earlier doc-name strings, preserving their raw references and counts. The initially incorrect line-location explanation and its correction are both retained. No test, source or raw log was changed or rerun for a parser correction.

Source assertions are unchanged from the selected base `1fb07c6`. The fixed123 identity denominator does not imply every assertion byte matches the original historical inventory: the preparation retains three inherited upstream differences. Six new cases are outside that denominator. This audit touches no lamp and makes no device or whole-issue-completion claim. No Python or binary build artifact is included. One NUL-delimited local Git metadata stream stays external; its reversible text rendering and the complete raw remote tree are included.

`SHA256SUMS` covers this package's files. The exact external publication manifest additionally covers the single independent root-proof file. Preparation and normal manifests are retained under `manifests/`.
