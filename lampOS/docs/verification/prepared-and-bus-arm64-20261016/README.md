# Prepared-player and Bus native qualification

Recorded 2026-10-16 02:57 +0700, from `date`. Unedited run metadata/logs and exact new-test audits are retained beside this note.

| PR | Exact head | ARM64 run | Passed | Profile exclusions | New cases, once each |
| --- | --- | --- | ---: | ---: | ---: |
| #877 | `8bbb50dcbd6493635d9d15edc2fef909797a368d` | [38793002582](https://github.com/autonomous-ai/lampOS/actions/runs/38793002582) | 4,843 | 127 | 6 |
| #901 | `2687bf6d63158cf24ba897c8b7e22a43e9f42e80` | [38795145743](https://github.com/autonomous-ai/lampOS/actions/runs/38795145743) | 4,846 | 127 | 9 |

Both PRs are ready for reviewer merge. All later native stages succeeded. These explicit fixture preparations do not migrate existing realtime scenarios or establish #823's combined serial target. Their full local gates and source reversal remain in the PR verification folders. No retry, test exclusion change, device or workflow change.
