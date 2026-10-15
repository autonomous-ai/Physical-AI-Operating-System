# Full ARM64 coverage after dispatch cleanup

Recorded 2026-10-15 21:47 +0700 from `date`. Exact PR head `a25450bd742cca4108920d1dff5bc5fc9e7a6c4e`; existing manual workflow option `full=true`, run [38737583302](https://github.com/autonomous-ai/lampOS/actions/runs/38737583302). This supplements merged #843 and #823; no workflow or profile change.

[Coverage audit](native-inventory.md) compares all 123 frozen identities: none missing, 121 PASS and two LEAK warnings. Across the full run, 4,633 unique tests passed (4,631 PASS plus two LEAK). All 13 final-dispatch cases passed, totaling 5.226 s. The 123 durations sum to 848.449 s across four shards, not a serial group wall time and not proof of the under-two-minute target. Aggregate changes cannot be attributed to this one patch because upstream source also changed.

[Structured comparison](native-inventory.json), [dispatch details](full-arm64-dispatch.json), and [run metadata](full-arm64-result.json). `nextest-results.log` is an exact-line excerpt of every PASS/LEAK result and four summaries from `gh run view 38737583302 --log`; no line text is modified. The complete downloaded log remains at the linked run. Partition skip counts are not added; the remaining compiled identities are not classified without enumeration.

The two endpoint-session leak warnings have their own issue [#860](https://github.com/autonomous-ai/lampOS/issues/860); their original evidence is retained. Assertions passing is not proof of clean fixture teardown. No tests or retries were launched to obtain this report, and no lamp was accessed.
