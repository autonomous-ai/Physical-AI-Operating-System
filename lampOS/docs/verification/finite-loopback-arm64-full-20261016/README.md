# Finite-loopback ARM64 qualification (#823, #889)

Recorded 2026-10-16 01:16:57 +0700, from `date`. Exact head `8a7521661cb06997a18eed32ead13d8d9ee81fd9`.

Normal PR run [38780759005](https://github.com/autonomous-ai/lampOS/actions/runs/38780759005) is green: **4,805 passed, 126 profile exclusions**. Its four-shard full run [38780759392](https://github.com/autonomous-ai/lampOS/actions/runs/38780759392) is also green: **4,951 unique outcomes, 4,949 PASS and two LEAK**, zero failed. The two leaks are the pre-existing real-process endpoint fixtures already addressed separately by pending #872: `unconfirmed_endpoint_exit_fails_mute_cleanup_and_never_reopens_service` and `a_worker_still_loading_leaves_the_baseline_window`. Green workflow status is not a claim of clean endpoint teardown.

All **123 original real-clock identities** appear exactly once: no missing or duplicated identity. Their test-duration sum is **619.655 s**, versus **976.935 s** for the unrounded per-test maximum of the two frozen baseline runs. This is a sum across four concurrent serial shards, not a newly measured whole-group serial elapsed time or a quiet-host controlled comparison. Other builders have also changed fixtures since the baseline; the total reduction is not attributed to this PR alone. The under-two-minute target remains open.

All **12 finite-loopback identities PASS**, totalling **6.797 s** in this native full run. The normal PR profile excludes those identities; only the full run establishes their native execution. The existing matched local release pair remains separately recorded in the PR: 12/12 before and after, 239.720 s to 11.439 s, identical compiled lists and helper hash, all 100 assertions retained. This ARM64 total is not the after side of that macOS pair.

## Original group membership

| Binary | Original identities found | Current sum (s) | Baseline max sum (s) |
|---|---:|---:|---:|
| `lamp-actions::server` | 1 | 4.112 | 4.081 |
| `lamp-health::program` | 1 | 5.009 | 5.013 |
| `lamp-memory::health` | 7 | 36.043 | 36.070 |
| `lamp-records::records` | 1 | 5.007 | 5.008 |
| `lamp-voice` | 63 | 137.499 | 241.717 |
| `lamp-voice-eval` | 1 | 5.023 | 5.019 |
| `lamp-voice-eval::final_dispatch` | 13 | 5.249 | 29.187 |
| `lamp-voice-eval::physical_loopback` | 12 | 6.797 | 235.864 |
| `lamp-voice-eval::serve_relay` | 6 | 31.121 | 31.137 |
| `lamp-voice-eval::serve_session` | 14 | 369.857 | 369.907 |
| `lamp-voice::capture_hold_worker` | 4 | 13.938 | 13.932 |

## Reproducing the audit

The unedited `gh run view --log` output and run metadata are adjacent. A scratch parser outside lampOS selected nextest PASS/LEAK/FAIL result rows, keyed each by `(binary, test)`, rejected duplicate keys, and joined them against the 123 rows in the frozen `verified-measurements.json` inventory. It required all 123 and all 12 finite-loopback rows, then summed the printed nextest seconds without rounding until presentation. [All full-run rows](889-full-results.json), [the exact original 123 with baseline durations](889-full-original-123.json), and [counts and limitations](889-full-summary.json) retain the comparison. No retry, filter change, workflow change or device access was used.
