# Clock preparations: first normal ARM64 results

Captured and packaged from `date`: 2026-10-16 06:12:25 +0700. These are GitHub Linux ARM64 checks, not device observations.

| PR | Published head | First run | Unique PASS | New tests, each once | Tested merge |
|---|---|---|---:|---:|---|
| [#924](https://github.com/autonomous-ai/lampOS/pull/924) | `a22664ac9cb4ca7efbe28b501ec9810c52a00c5d` | [38819652589](https://github.com/autonomous-ai/lampOS/actions/runs/38819652589) | 4,987 | 12 | `a3eeb12cd478c4af0e29cf51cffffd2c6e7cd9bc` |
| [#925](https://github.com/autonomous-ai/lampOS/pull/925) | `dee1cdd455072a92491bc800fd3993a124ee411a` | [38819671092](https://github.com/autonomous-ai/lampOS/actions/runs/38819671092) | 5,000 | 25 | `67c3d4b00551eb8c981923d69272aa255cf31eb7` |

Both tested merges have `e7d0f673d0c4bde4a770b19783d6084a8d935ec3` and the exact published PR head as parents. All five Gemini and four Moment source/document files match their PR heads byte for byte; the content API responses and Git blob verification are retained. All 14 ARM64 job steps succeed in each run. Every parsed result is PASS, with consecutive ordinals and no duplicated identity. The complete raw logs, run metadata, command/exit/date records, source API responses and derived per-test outcomes are unedited copies. See `*.audit.json` for membership/source proofs and `SHA256SUMS` for file integrity. The audit was run outside the repository; no measuring Python script is introduced into lampOS.

The normal PR profile excludes **all original 123 identities**. These green results establish that the preparations and their new tests build and pass on ARM64; they do not qualify any original-123 migration or the serial under-two-minute target. Neither preparation migrates an original scenario. No rerun was made.

Local failures remain failed and are retained in the PR evidence:

- Gemini default and feature suites each pass **268/0/2 existing ignores**. Its complete affected gate is **2196/1/13**, failing the original rapid-mute case tracked by #893/#739 (`service privacy freshness lost`). Its older #866 StaleOwner failure remains in `99611db2bbcf3724c83a28db6ec3b6bddce53997`.
- Moment complete affected gate is **1960/2/13**, failing the same rapid-mute identity and `reminders::consumer::tests::completed_playout_survives_both_real_store_reopens_then_acks_without_respeech` (expected AckConfirmed). Eighth accepted that exact reminder identity into #739 in comment 6198245051. Its cause remains unproved; the 250 ms host budget is only a hypothesis. The reminder is outside the original 123. The older **1741/4/8** failure remains in `2fdb5d7a2fc5472351e1bb3736776966181a4b60`.

Joining's explicit known-local-timed-failure exception applies to these green native preparations; it does not reclassify their local results or show the excluded rapid-mute case passing. Every existing assertion, clock boundary and production default remains as recorded by each PR's source-preservation proof.
