# Serve and Records native qualification

Recorded from `date`: 2026-10-16 06:42:41 +0700. Linux ARM64 GitHub Actions, not a lamp. Raw logs and metadata are copied without editing; SHA256SUMS covers every evidence file.

## Complete Serve migration, current integration

PR [#909](https://github.com/autonomous-ai/lampOS/pull/909), current head `bce32d696fe4bb90db172bfab580619dbf5f11c8`, passes its first normal [38822580456](https://github.com/autonomous-ai/lampOS/actions/runs/38822580456): **5,122 unique PASS**, all **121** new cases once, complete nextest ordinals, no FAIL/LEAK or duplicate, and all 14 job steps successful. The tested merge is `5ff93e018bc3217e0eedc84f8d67e8ff11a38210`; all 26 changed source/document files match the head bytes and Git blob hashes. Nextest reports 82.250 s for its test phase and 130 total skips, including 93 via the PR default filter. The normal run excludes all fixed original 123 identities.

The preceding all-14 head `8af53fd459795029f6277679ded49bf7826def73` has a separately [audited first full run](../serve-transport-native-20261016/README.md): 5,205 unique PASS, all original 123 and all 121 additions once. The 25 migration Rust files are identical between those heads, and the current integration passes a fresh 498/0/0 local evaluator gate with complete 377-before/498-after identities. All 14 original Serve cases, 22 expanded attempts and 114 assertion sites remain unchanged. The prior full run reports 19.402 s summed for Serve's 14 and 269.137 s for the original 123, across four concurrent shards. Those sums are not a standalone serial measurement. Its success does not claim to fix the earlier retained Body #914 failure.

Ready for lamp-voice: **yes (#909)**. The current head has its own green native qualification; the full-run source/head distinction remains explicit.

## Records controller

PR [#931](https://github.com/autonomous-ai/lampOS/pull/931), head `42405bb318ddd91c4883e5a16183630c0282b675`, passes first normal [38822753449](https://github.com/autonomous-ai/lampOS/actions/runs/38822753449): **5,021 unique PASS**, all **20** new cases once, no FAIL/LEAK/duplicate, complete ordinals and all 14 job steps successful. All ten source/document files exactly match tested merge `70c34b63255262a5a7aa4fba687325de4961091c`. Nextest reports 86.019 s and the same 130 total/93 profile skips. All original 123 are excluded in normal.

Its first full [38822752215](https://github.com/autonomous-ai/lampOS/actions/runs/38822752215), attempt 1, is **failed**: shard 3 could not fetch `security-framework` through crates.io, with curl 56, `Recv failure: Connection reset by peer`, during the release build. **No tests ran in that shard.** The other three shards completed **3,950 unique PASS**, including **83/123** original identities and **15/20** added cases. Complete ordinals were verified for each completed shard, and all four checkout SHAs match the head. The migrated original `lamp-records::records` reporter case passed once in **0.010 s**. This partial run is not a full passing run and cannot certify its missing 40 original identities or five added cases. See the attempt1 audit's exact missing lists and retained failure excerpt.

Only the unexecuted infrastructure-failed shard is being restarted. Completed shards and tests are not restarted; no failed test is retried. The later attempt will be recorded separately, preserving this first failure and distinguishing composite coverage from a single-attempt result. Records remains draft pending that qualification.

## Limits

No production behavior, assertion, duration, ignore, retry setting, profile, workflow, runner or device changes accompany this evidence. The normal runs cannot qualify excluded realtime tests. Historical Mac before/after pairs retain their shared-host load limitations. The complete frozen original-123 standalone serial under-two-minute target remains unproved. #783's merged sweep awaits Eighth's loaded policy qualification and close decision.
