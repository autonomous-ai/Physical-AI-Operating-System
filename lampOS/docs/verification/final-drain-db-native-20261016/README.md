# Final-drain clock: db first-attempt native qualification

Captured 2026-10-16 11:36:55 +0700. PR #963 head `db7201d0bc4c1d368b14ac203f04a3a99e3d845f` passed both native ARM64 workflows
on their first attempts. No job was restarted and no failed test was retried.

| Run | Actual checkout | Nextest PASS | Evaluator IDs | Original 123 | New helper cases |
| --- | --- | ---: | ---: | ---: | ---: |
| [Normal 38849194888](https://github.com/autonomous-ai/lampOS/actions/runs/38849194888) | `50de8ca4b070ab1d070d69ecfc458d0b0870395d` synthetic merge | 5,360 | 463 | 0 | 0 |
| [Full 38849189990](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990) | `db7201d0bc4c1d368b14ac203f04a3a99e3d845f` on all four jobs | 5,516 | 521 | 123 | 6 |

Every reported nextest identity is unique, with complete per-job ordinals and
matching start/summary counts. The full Evaluator membership exactly equals the
frozen local 521-identity list; the normal membership is exactly the 463 allowed
by its unchanged profile filter. All 123 canonical originals and six additions
appear once in the full run. Neither is expected in the normal run because its
profile excludes those identities. There are no FAIL, LEAK, retry or unknown
result rows. Each workflow separately passed four doc cases, the IPC
`relay_signals` harness and all 12 `turn-end-wire::process` cases; those results
are not folded into the nextest totals.

| Full shard | Job | PASS |
| ---: | --- | ---: |
| 1/4 | [116806405928](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405928) | 1,355 |
| 2/4 | [116806405934](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405934) | 1,416 |
| 3/4 | [116806405555](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405555) | 1,303 |
| 4/4 | [116806405950](https://github.com/autonomous-ai/lampOS/actions/runs/38849189990/job/116806405950) | 1,442 |

All four full checkouts match db and its canonical Git tree. All 1,094 frozen
inputs and 56 owned publication files match the published head. The normal
merge has parents `ede9ce8ef81f67f5b8cea5b42a0660fbbafcf3d5` and db; it preserves
all 56 owned files, the complete Evaluator subtree and execution configuration.
Its 12 additional upstream paths include 10 changed Voice inputs, so this audit
does not claim the entire normal dependency context matches the manual head.
The raw merge/head trees and exact upstream diff retain that distinction.

The canonical 123 identity denominator is pinned by the original inventory hash
and prior independently audited inventory. Source definitions and ignore
attributes are checked against actual f5/db source. Their assertion lists are
unchanged by this fixture delta; equality to every historical original assertion
byte is not claimed.

[The descending top 20](full/original123-top20.md) identifies binary, full
identity, shard/job and the single printed duration. [All 123 observations](full/original123-durations.json)
retain raw line references and per-binary sums. Their **196.159-second sum is a
case-duration sum across concurrently running shards**, not standalone serial
wall time. The selected original reports 0.055 seconds in this run. Neither
number is a matched timing pair, device result or a new benchmark execution.
The original standalone under-120-second target remains unproved.

`normal/` and `full/` preserve unedited complete gh run logs, per-job API logs,
run/job/commit metadata and dated command receipts. Removing exactly the two gh
job/step tab prefixes reproduces each API job log byte-for-byte. The raw current
workflow and nextest configuration are retained under `source-snapshots/head/`.
The normal metadata-parser diagnostic, its failure and corrected proof are
preserved in `metadata-parser-diagnostic.md`; no test or CI retry was involved.

Earlier c2 native failures and c5 qualification remain separate historical
evidence. This package does not erase or relabel them. `provenance.json` records
each copied input; `SHA256SUMS` covers every other file in this package. All
orchestration/audit Python remains external, with hashes only. No repository,
source or device was changed by this audit.
