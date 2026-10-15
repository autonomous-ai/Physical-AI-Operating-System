# Clock preparation native qualification

Recorded 2026-10-16 00:16 +0700, from `date`. These are three explicit evaluator fixture APIs,
all green on their own PR heads and ready for the reviewer. Existing integration
scenarios retain host time in these preparations; this evidence makes no speedup
claim and does not establish the complete #823 realtime target.

| PR | Head tested | ARM64 run | Passed | Existing PR-profile skips | Clock checks shown |
| --- | --- | --- | ---: | ---: | ---: |
| [#867](https://github.com/autonomous-ai/lampOS/pull/867) | `09b7512f464afc02745ed5eb0b067e5b72ee39e3` | [38769725447](https://github.com/autonomous-ai/lampOS/actions/runs/38769725447) | 4,745 | 126 | 9 |
| [#875](https://github.com/autonomous-ai/lampOS/pull/875) | `5020ed55a373e2cd57c25dc5f45a1924685ad85c` | [38769997069](https://github.com/autonomous-ai/lampOS/actions/runs/38769997069) | 4,745 | 126 | 4 |
| [#877](https://github.com/autonomous-ai/lampOS/pull/877) | `3fb8aacb2597ae234d94ad499f69446411baf239` | [38771112900](https://github.com/autonomous-ai/lampOS/actions/runs/38771112900) | 4,747 | 126 | 6 |

Each subdirectory retains the unedited `gh run view --json
headSha,conclusion,event,url,jobs` result and exact clock-test/summary rows from
`gh run view --log`, including its original timestamps. The nine shown for
#867 include its four new fake-runtime tests and the five merged backend tests.
All four added serve and six added prepared-player tests ran. These are normal
PR-profile runs; the realtime exclusions still apply and are stated explicitly.

Full local evaluator gates, source-reversal proofs and raw commands remain in
the respective PR verification folders. No workflow/profile changes, added
retries or devices are involved.
