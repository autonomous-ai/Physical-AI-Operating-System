Native audit preparation recorded 2026-10-16 10:09:47 +0700.

PR #966 head `8de22aef8d5526f83b48b279e71706d71e7cdf5b`; owner-reported first
normal ARM64 run 38843507733, job 116788702286. No native run status or log has
been requested by this audit. The parent monitors it and will notify terminal.

All 7,152 frozen local inputs match the head's actual Git blobs. All 192
publication files match their exact hashes/modes: six owned source files and
186 evidence files. The complete base 1fb to head Git tree difference is exactly
those 192 paths. The head has 7,558 tracked blobs; 220 already-tracked paths
omitted by the earlier rg inventory are independently unchanged to base. They
are not silently treated as new upstream differences or new publication files.

The unchanged PR filter selects all 18 TaskJoins additions and the original
failed native_head identity. Gemini's local compiled membership is 289 across
five binaries, with 287 expected normal executions and two existing ignores.
Static application of those filters to the full local five-package list gives
2,569 normal executions from 2,726 identities; 143 are filter-excluded and 15
ignored, with one overlap. This local subset is not a native-workspace count:
Linux-only identities and synthetic-merge upstream changes remain to be audited.

The pinned native workflow runs release workspace binaries/examples, nextest
profile pr on hash:1/1, workspace doc tests, and the two own-harness tests
`lamp-ipc::relay_signals` and `lamp-turn-end-wire::process`. It has no formatting
or Clippy step; strict Clippy is a separate retained host qualification.
The final audit must distinguish that status rather than claim native Clippy.

After the terminal notification, capture complete job logs and run/job
metadata, derive the actually checked-out synthetic merge from checkout output,
and retain its commit/parents/tree. Compare that tree with the exact head and
classify upstream dependency changes separately. Check all 18 additions, all
Gemini membership and the known failed native_head identity for exactly one
execution, preserving skips, failures, retries, result recaps and the original
local failed gate. Require actual build/docs/harness statuses before readiness.

`head-tree.raw` is a NUL-delimited external capture; it must not enter Git.
`head-tree.txt`, `head-source-proof.json`, pinned workflow/config and the
expectation JSON are plain-text evidence. The preparation note preserves the
initial failed inventory assumption and its full-tree resolution. Nothing in
this directory is an ARM64 result or a readiness approval yet.
