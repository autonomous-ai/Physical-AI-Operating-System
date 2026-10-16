# Preserved parser-only correction

The first independent audit stopped at 2026-10-16 11:20:22 +0700 after its
source and metadata checks, because its compiled-list check incorrectly
required every identity to have `filter-match.status == "matches"`.
The actual list contains 2,727 matching nonignored identities and 15 unchanged
ignored identities with `status: "mismatch", reason: "ignored"`.

The corrected invocation completed at 2026-10-16 11:21:07 +0700, requiring that
exact distinction and independently comparing the same 15 ignore flags against
the fresh baseline. Both invocation commands, date boundaries, exit codes,
stdout/stderr and the first partial proofs are preserved under `diagnostics/`.
The external script hashes are retained. No source, original evidence, compiled
list, execution log or test was changed or rerun for this correction.
