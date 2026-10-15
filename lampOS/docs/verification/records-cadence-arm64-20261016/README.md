# Records cadence preparation — ARM64 qualification

Recorded 2026-10-16 05:22:44 +0700, from shell `date`. PR [#922](https://github.com/autonomous-ai/lampOS/pull/922), head `e3d3366cad8113c3374b8e29faedb099b5e47d92`.

Its first native [run 38812973372](https://github.com/autonomous-ai/lampOS/actions/runs/38812973372) is green: **4,946 unique PASS**, no duplicate outcome, and every one of the **12 new boundary cases exactly once**. All 14 ARM64 job steps passed; the separate failure-report job was conditionally skipped. Four doc tests and the 12 process-harness cases passed, with `relay_signals: ok` separately recorded. No retry was used.

The PR checkout was merge `101c8a0978692e5609c71f66710d1be0474d1fb8`, with parents `d60124f8280f9c76576d6ca7bf859bc5c3983828` and the exact PR head. GitHub source API bytes for all six qualified source/doc files match the local reviewed SHA-256 values and Git blob identities. [Source proof](922-38812973372.source-proof.json) preserves the API responses; [parsed outcomes](922-38812973372.parsed.json) and the complete raw log preserve the execution evidence.

The normal profile reports **130 total skipped entries in eight binaries**, including **93 by the PR default filter in six binaries**. None of the frozen original 123 executes in this profile. This preparation does not migrate an original scenario, so all its new boundaries execute here. The nextest test phase is **80.897 seconds** under parallel execution; that is neither push-to-result elapsed nor standalone serial time. No original-123 speedup or combined under-two-minute result is claimed.

Local qualification remains **170 passed, zero failed, two existing ignores** across Records/Settings, all 160 original compiled identities retained plus 12 new cases, with fmt, strict all-target Clippy and workspace build passed. [Qualified source and local evidence](https://github.com/autonomous-ai/lampOS/tree/e3d3366cad8113c3374b8e29faedb099b5e47d92/lampOS/docs/verification/records-cadence-20261016) retain the first 164/6/2 failure caused by excessive new fixture socket paths and the narrow root-path repair. Original test bodies and assertions remained unchanged.

Initial scratch parsing stopped because its ordinal regex omitted whitespace padding; the corrected parser matches every unedited raw row and the summary. This was an audit correction, not a test rerun. The bounded child/controller and original Records scenario migration are a separate next change. No workflow, profile, runner, setting, device or production policy was changed.

All files except this checksum list are covered by [SHA256SUMS](SHA256SUMS). Absolute scratch paths in raw manifests record original capture locations; matching basenames are retained here.
