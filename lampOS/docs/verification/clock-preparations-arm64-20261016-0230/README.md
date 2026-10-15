# Exact-head ARM64 qualification of two preparations

Recorded 2026-10-16 02:30:53 +0700, from `date`. Both runs are successful, attempt 1 pull-request checks. Logs and metadata are retained byte-for-byte. Scratch Python outside the repository fetched `gh run view --json` and `--log`, checked head/conclusion and matched each new PASS exactly once.

| PR | Head | Run | PASS | Existing profile exclusions | New boundary tests |
|---|---|---|---:|---:|---:|
| [#894](https://github.com/autonomous-ai/lampOS/pull/894) | `bb40423d37df96e9d3a533fda83dbaaecb079d1a` | [38790030817](https://github.com/autonomous-ai/lampOS/actions/runs/38790030817) | 4,835 | 126 | all 6 |
| [#897](https://github.com/autonomous-ai/lampOS/pull/897) | `aa32b9d1222f0bb4c66152cfacfc431d3f584a1c` | [38791901560](https://github.com/autonomous-ai/lampOS/actions/runs/38791901560) | 4,840 | 126 | all 3 |

The first exposes the original full transport delay to an explicit wait function; the second shares Memory's actual application body with an internal reporter factory. Neither migrates an original scenario or claims a speedup. Both are ready for reviewer merge. Their distinct prior complete gates and fresh rebased gates remain in their PR verification folders. These normal-profile runs alone do not establish the original 123-test serial target. No device run, assertion weakening, retry or workflow/runner/profile change.
