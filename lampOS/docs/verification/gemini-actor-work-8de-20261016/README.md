# Actor-work receipts: local 8de gate, Git-safe evidence

Prepared 2026-10-16 11:23:03 +0700. The independently checked first frozen gate passed on base
`8de22aef8d5526f83b48b279e71706d71e7cdf5b` with candidate diff SHA256
`84736e2e20e51b7732bd95cb2baad0e7d23a688487531db2dede9c9fd543a790`. This is a five-file uncommitted source
candidate and local evidence, not native or PR-readiness qualification.

Fresh compiled baseline **2,726 identities / 15 ignored** became **2,742 / 15**:
exactly the 16 named Actor-work cases were added, with no loss or ignore change.
The focused invocation passed 16/16. The separately planned full invocation
passed **2,727, failed 0, with 15 unchanged ignores**, each eligible identity
exactly once. It listed 88 binary paths; 80 contained eligible execution rows.
One slow marker was followed by PASS and is retained in the raw log. Full counts:
Gemini 303, Memory 205, Surround 260, Voice 1,461, Evaluator 498.

The actual full Cargo metadata independently yields the same five-package
resolved reverse-dependency graph for baseline and candidate. Format, strict
all-target Clippy, compiled/focused listings, focused/full execution, workspace
build and all five affected doc-test targets returned 0. The doc targets contain
zero cases. The gate used Rust 1.96.0, locked/offline Cargo, two jobs, incremental
compilation disabled, default nextest profile, one test thread, no-fail-fast and
zero retries. The clean baseline listed tests without executing them.
Commands, environment, start/finish dates, stdout/stderr and exits remain under
`evidence/baseline/` and `evidence/gate/`.

All 7,558 clean-baseline inputs match their actual Git blobs. All 7,560 candidate
inputs match the frozen hashes at every recorded boundary and this audit.
Only the five authorized paths differ; 7,555 unowned inputs are exact. The
complete inverse was independently applied in memory and restored the three
old files while removing the two new files. Original handlers and existing
fixture bodies remain exact behind the observation hooks, and original test
files/assertions are preserved. The first external candidate and subsequent
entry-frontier correction remain separate historical preparations.

Maintenance receipts capture returned-command and post-stop-discard counters
at **pass entry**. Completion publishes those captured counts and samples buffer
facts then. Commands replayed late by an image write are covered only by a later
completed pass; both the real image regression and explicit frontier case verify
this distinction. These are historical observations. They do not certify live
idle, remote/service handling, worker/child joins or permission to advance time.
No original scenario was migrated by this change.

Prerequisite #966 remains draft for the separately requested direct ordinary-
versus-retained TaskJoins comparison. This evidence does not include that later
correction, an updated base or native qualification. The existing unobserved
factory case in these 16 tests does not substitute for that separate comparison.
No device, standalone original-123 bound or speedup claim is made.

The immutable original external package has **174 manifested files plus its
manifest (175 actual files)**. This payload copies **162 original evidence
files byte-for-byte**, preserves the original manifest under
`provenance/original-manifest.json`, and **excludes all 12 Python scripts**.
The original manifest is historical inventory provenance, not this payload's
shipping manifest. `original-inventory-reconciliation.json` accounts for every
original file. `excluded-external-scripts.json` records the 12 external paths,
descriptions, sizes and hashes. New audit scripts also remain external with
hashes only. No Python script or binary artifact is included here.

`independent/` contains the source, graph, identity, row, gate and doc audits.
The first parser invocation mistakenly required ignored identities to match the
run filter; the preserved diagnostic and corrected invocation distinguish the
15 unchanged ignores explicitly. This was an audit-parser correction only;
no tests, source or raw evidence were changed or rerun.

`provenance.json` records every copied file, and `SHA256SUMS` covers every other
file in this payload. The exact external payload manifest is delivered alongside
this directory. Packaging left the worktree and original package unchanged.
