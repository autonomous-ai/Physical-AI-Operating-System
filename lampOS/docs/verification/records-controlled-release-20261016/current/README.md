# Records controller integration onto the merged preparation

Recorded 2026-10-16 06:05:32 +0700, from shell `date`. The release tree is `/private/tmp/lampos-tests-records-fixture`, branch `tests/823-records-fixture`, based on actual merged preparation `9b28324e2232d62ff905e705d16f53c7836599f9`.

**Integration is complete and all ten source hashes are frozen. Current-base execution and native qualification are still pending.** No Cargo command or test was run during integration.

## Exact transfer

The qualified source came from `/private/tmp/lampos-tests-records-controlled`, based on `e3d3366cad8113c3374b8e29faedb099b5e47d92`. Before copying, the clean release tree's **6,861 tracked blobs and modes** were independently checked against its base, with empty status and unchanged index. [Clean proof](clean-base-proof.json) and [full initial manifest](clean-base-tracked-manifest.json) retain that result.

For all ten owned paths, the old and new bases have identical pre-edit blobs and modes: four existing paths match exactly; six new paths are absent from both. [Owned-base proof](owned-base-proof.json) records each one. The ten qualified files were copied byte-for-byte with their modes; no redesign, conflict resolution or source correction was needed.

After the copy, all **6,857 unowned tracked entries** still match the new base's blobs and modes. This preserves the 83 changed paths between the old and new bases, including the newer Memory/Voice work and Cargo.lock. [Complete post-integration manifest](post-integration-tracked-manifest.json), [base delta](base-delta-paths.json) and [integration record](integration.json) retain the evidence. The Git index was not changed.

The existing `lampOS/docs/verification/records-controlled-20261016` package was copied as **82 byte-identical files**, including its README, raw logs, proof JSON and SHA256SUMS. Its original assertions remain historical; none was rewritten to imply current-base qualification. [Package copy manifest](historical-package-copy-manifest.json) maps every source and destination byte hash. This separate release record supplies the new context without modifying that historical package.

## Baseline identity reuse

[Static manifest graphs](dependency-graph-new.json) include all normal, development, build and target-specific path dependency tables, including optional paths. The selected local closure is `lamp-records`, `lamp-settings`, `lamp-bus`, and `lamp-msgs`. Their full package directories, workspace manifest, pinned toolchain/config, and the local workspace patch directory were compared conservatively.

The first check deliberately rejected whole-Cargo.lock byte equality. The only differing path among **102** compared inputs is Cargo.lock. Recursive lockfile resolution from the selected Records/Settings roots then proves that all **39 selected package records, registry checksums and dependency edges are identical**. [Full lock delta](Cargo-lock-delta.json) shows only `lamp-memory 0.1.0` changed, which is outside that closure. All other 101 conservative source/config paths and the static manifest graphs are exact. [Final reuse proof](baseline-reuse-proof.json) retains both the reason reuse is valid and its limits; [the initial conservative rejection](baseline-conservative-first-check.json) is preserved too.

`root-baseline-list.log` is the unmodified qualified **172-identity** pre-controller list. Its bytes match the earlier corrected Records preparation list. It may support comparison against a fresh release list, expecting the same original identities/ignore flags plus 20 added cases (**192** total). This is source/identity evidence only. It does not reuse the old execution pass, demonstrate a native result, or eliminate the required fresh affected gate and workspace build.

## Historical results and remaining gate

The copied historical package records **190 pass / 0 fail / 2 existing ignores**, with fmt, strict Records/Settings Clippy, compiled identity preservation and workspace build passing on the old dependency head. The exact original-case serial pair is **5.614 → 0.735 seconds**, both passing with zero retries. These are shared-Mac debug observations under unequal background load; they are not an isolated benchmark, current-base measurements, ARM64 results, or #823's combined original-123 serial proof.

Root must now run fresh format, strict all-target Records/Settings lint, compiled identity comparison, complete serial affected tests and workspace build on this frozen release source. Native qualification follows draft publication. The source still preserves the original 16 assertions, actual Bus Events, full periods, original deadlines and actual child ownership/reaping; Checked does not prove delivery or exit. No Git mutation, network call, test execution, retry or device access occurred during this integration.
