# Terminal native audit checklist

Preparation does not query workflow status, read unfinished logs or qualify a run. Root owns watcher 77511 and all job mutations.

1. After root reports terminal status, preserve complete attempt-specific run metadata, all job/step metadata, raw logs and stdout/stderr capture receipts. Never overwrite a failed capture. Normal and full evidence remain separate.
2. Parse every nextest PASS/FAIL/LEAK record by binary and full test name. Require no duplicate identities, ordinal sets exactly 1..N per shard, matching total N, one start and one matching summary. Examine raw error, panic and test-retry markers, not just workflow conclusion. Preserve any failure without retrying tests.
3. For normal, require precisely the frozen 452 Evaluator identities, no fixed original123 and no new six: the whole final_dispatch binary is excluded by the existing profile. Check the actual checkout merge includes the head and whether all relevant source/config/dependency bytes match before using the frozen expectation.
4. For full, require all 504 Evaluator identities, all six additions and all fixed123 exactly once across disjoint shards. The selected original remains a_session_end_first_seen_in_the_final_drain_withholds_and_is_kept. New helpers are never counted in the original denominator.
5. Check every job and step independently, treating ordinary skips such as the normal failure-report job and non-shard-1 doc/artifact steps separately. Verify doc tests, relay_signals and the process harness from their own raw outputs; do not add these to nextest totals.
6. If a dependency/build failure occurs before a test step, retain the complete failed-job log and skipped-step proof, with zero test start/outcome/summary markers. Notify root. This auditor never restarts jobs. Test failures or already executed tests are not retried.
7. If root restarts only an unexecuted shard, compare the full original successful logs byte-for-byte, every step and original start/end times. Preserve both original execution job IDs and new GitHub record IDs. The new shard must supply exactly missing identities and no previously executed identity. Keep the first failed attempt visible.
8. Compare every full checkout with the exact head and refreshed commit/tree. Verify all 1,023 frozen inputs and 56 publication entries; all 77 Evaluator source blobs are pinned. Retain normal merge-source differences explicitly. Never infer branch-head source from a run's reported PR head alone.
9. Freeze the complete raw/audit package with SHA-256 and an exact publication manifest. Root independently reviews, publishes and changes PR readiness. Printed process-duration sums and the historical local pair never become a standalone serial123 or native before/after claim.

Prepared scripts remain outside this evidence directory. They have been syntax checked only; preparation has not executed them on #963 logs.
