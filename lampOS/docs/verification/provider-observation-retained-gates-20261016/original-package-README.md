# Provider observation gate evidence

Frozen 2026-10-16 07:31:09 +0700 from shell `date`, base `e7d0f673d0c4bde4a770b19783d6084a8d935ec3`.

**UNREADY:** the corrected complete affected gate returned exit 101 with 2,263 passed, one failed and 13 ignored. The remaining existing identity is `coordinator::tests::pilot_faults::a_microphone_closed_by_a_stall_reopens_under_a_new_epoch_and_the_voice_goes_on`. Its observed result was `service privacy freshness lost before next observation`; the cause remains unclassified. This exact identity is distinct from #866 and outside the frozen original 123. Owner classification or a fix is required before readiness.

Start with [corrected-gate-handoff.md](corrected-gate-handoff.md). It includes the source hashes, complete stage results and precise limits of the diagnosis. The corrected focused gate passed all 20 additions; fmt, strict affected Clippy and workspace build passed. All 2,257 original compiled identities and 13 ignored flags remain, with exactly 20 added identities. Both runtime audits account for all 82 binaries and 2,277 statuses.

[first-gate-handoff.md](first-gate-handoff.md) and its original manifest preserve the earlier failed source without edits: focused 19 passed/1 failed; full 2,262 passed/2 failed/13 ignored. That attempt's new socket-close assertion failure and existing reminder assertion failure are retained. The corrected source changed only the reviewed new socket test. The reminder passing later does not erase its first failure or classify #935.

This directory copies all 137 source evidence files byte-for-byte. Full first/corrected source snapshots, clean baseline and both compiled lists, stage command arrays/logs/timestamps/exits, original failure blocks, unchanged failure-source snapshots, source proofs and validation scripts are included. Historical absolute paths in these records identify the original capture locations; the same relative suffixes are present here. Historical in-progress/proposed records are preserved, and the completed records supersede their status. The original inline test count is 105 including two ignored; see [handoff-inventory-addendum.md](handoff-inventory-addendum.md) for the correction to the earlier regex count of 103.

The command arrays in `*-completed.json` and `*.command.json` are the exact invocation records. To render the focused command as shell text, its filter argument must remain quoted:

```sh
cargo nextest run --locked --offline -p lamp-voice --profile default --test-threads 1 --no-fail-fast --retries 0 -E 'test(/^provider_worker::observation::tests::/) | test(/^provider_worker::tests::observation::/)'
```

This is evidence only; do not execute the copied gate scripts for package verification. The Cargo target has been released for another agent to reuse; this package needs no target binaries. No unchanged failed case was retried after the corrected full gate, and no causal classification is inferred from untouched source or producer-side gap measurements.

[manifest.json](manifest.json) gives payload hashes and lengths. `SHA256SUMS` covers every payload file plus the manifest. Payload files are read-only (0444), directories 0555. An external receipt records the completed copy/hash/mode checks.
