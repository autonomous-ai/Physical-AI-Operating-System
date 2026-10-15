# Complete Serve and transport preparation: native evidence

Recorded from `date`: 2026-10-16 06:25:52 +0700. These are GitHub Linux ARM64 results, not observations on a lamp. All raw run logs, metadata, source proofs, command/date/exit records, membership audits and initial retrieval errors are retained without editing.

## Full all-14 Serve migration

PR [#909](https://github.com/autonomous-ai/lampOS/pull/909), previous published head `8af53fd459795029f6277679ded49bf7826def73`, passed its first full [38820115320](https://github.com/autonomous-ai/lampOS/actions/runs/38820115320). All four recorded checkouts equal that head and the GitHub/local commit tree matches. All four shard ordinals are complete. The full raw log (`909-38820115320.retrieved.log`) contains **5,205 unique PASS, zero FAIL/LEAK**, with **every original 123**, every new **121**, and all **498 Evaluator identities** exactly once. Both independent and root membership audits agree.

The **14 original Serve cases total 19.402 seconds**: ten base cases 16.222, three Recorder cases 2.187, and one SlowTransport case 0.993. All original 123 printed case durations sum to **269.137 seconds**. These are sums across four concurrent, internally serial shards, **not** one standalone serial elapsed measurement or a controlled before/after experiment. The unchanged 37 source-ignore attributes are consistent with the 5,242 listed/5,205 executed difference, but this workflow does not retain the complete native identity list of those 37 ignored tests. No ignore, skip, retry or workflow change is introduced.

The current PR head is `bce32d696fe4bb90db172bfab580619dbf5f11c8` on newer `e7d0f673`. Its 25 changed Rust files remain identical to 8af, with the document conflict resolved and current Memory/Voice inputs retained. Its fresh local complete gate passes 498/0/0, but **its own normal ARM64 38822580456 is separate and pending**. This old-head full result does not qualify that newer head. The original failure 38801026642 remains recorded at `d9cc28b141e317c51e71a0d06b450be628103cc7`; success here is not proof that Body #914 is fixed. Initial sandbox-denied log/API retrievals here were read attempts after the run completed; their errors are preserved alongside distinct successful `.retrieved.*` captures. The workflow itself was not retried.

## Transport success observations

PR [#927](https://github.com/autonomous-ai/lampOS/pull/927), exact head `3401b9e5c651627c556930e0d9b0d3f87833c848`, passed first normal [38820672032](https://github.com/autonomous-ai/lampOS/actions/runs/38820672032): **5,010 unique PASS**, all **14** new probe cases once, no duplicate/failure/leak, and all 14 job steps successful. The tested merge is `f40986c364d4503f6535920015bd5d0d71618852`; all four changed files match the head byte-for-byte and Git blob hashes match. See its `.audit.json`, `.source-*.response.json` and full `.log`.

The complete local affected gate remains **1,946 passed / 5 failed / 13 existing ignores**. All five exact failed Motor identities are present and PASS once in this native run, as recorded in `927-38820672032.local-failure-native-membership.json`; that does not resolve their local causes. The full local failure/triage and newly reported frozen-preflight identity remain in the PR and #739. Under joining’s explicit known-local-timed-failure exception, this preparation is ready for review while those failures stay disclosed. No original scenario is migrated. Normal profile excludes **all original 123** and establishes no serial target.

The supplementary 925 membership map confirms its local reminder AckConfirmed identity passed once in normal native 38819671092 (0.026s), while rapid-mute is excluded. Its authoritative raw log remains in [the preceding clock evidence](../gemini-moments-arm64-20261016/README.md). This does not establish either failure’s cause.

## Continuing work

Records controller [#931](https://github.com/autonomous-ai/lampOS/pull/931), head `42405bb318ddd91c4883e5a16183630c0282b675`, passes fresh 190/0/2 on the actual merged preparation; normal 38822753449 and full 38822752215 are running. The direct six-case relay continuation passes 508/0/0 and a single shared-Mac pair 31.480 → 2.367 s with zero retries; it awaits Serve’s actual merge and later native qualification. Memory health #919 and Records preparation #922 are already merged. No workflow/profile, runner, repository setting or device was changed. The complete original 123 standalone serial under-two-minute target remains unproved.
