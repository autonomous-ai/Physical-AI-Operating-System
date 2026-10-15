# Records controlled reporter on the merged preparation

Recorded 2026-10-16 06:16:10 +0700, from shell `date`. Base: `9b28324e2232d62ff905e705d16f53c7836599f9`, the actual merged Records preparation. **The first fresh complete local gate passes: 190 passed / zero failed / two existing ignores. Native ARM64 remains pending; this is a draft preparation for review.**

The source is exactly the already reviewed ten-file controller migration, integrated without redesign. One original test, `real_reporter_reloads_switch_and_policy_and_requires_a_matching_receipt`, now uses a dedicated real fixture child with bounded inherited pipes and one explicit pure clock. All six original status/receipt transitions and all 16 assertion macros remain, together with actual files, policy/switch reloads, real Bus Diagnostic Events, full one-second periods, original status and host lifetime bounds, and owned-child kill/reap.

A Checked response is withheld until actual construction/publication completes and the application reaches its next reporting wait. ReturnedOk does not prove delivery; Checked does not replace a Diagnostic or prove child exit. Failure, partial frames, stale identities and invalid progress latch without committing the parent's clock. The ordinary Records application and Reporter remain the merged preparation's bytes; the controlled executable is explicitly selected.

## Fresh gate and identity evidence

The first current-base gate ran from `2026-10-16 06:11:44 +0700` through `2026-10-16 06:14:33 +0700`, with pinned Rust 1.96.0, two Cargo build jobs and incremental compilation disabled on Darwin x86_64. Exact commands and date boundaries are in [root-gates.json](current/root-gates.json); toolchain/host details are in [root-environment.json](current/root-environment.json).

| Stage | Exact command | Result |
| --- | --- | --- |
| Format | `cargo fmt --all --check` | Pass |
| Strict lint | `cargo clippy --locked --offline -p lamp-records -p lamp-settings --all-targets -- -D warnings` | Pass |
| Compiled list | `cargo nextest list --locked --offline -p lamp-records -p lamp-settings --message-format json` | 172 originals and every ignored flag retained; exactly 20 added; 192 total |
| Complete affected tests | `cargo test --locked --offline -p lamp-records -p lamp-settings --no-fail-fast -- --test-threads=1` | **190 pass / 0 fail / 2 existing ignores**, ten Cargo summaries |
| Workspace build | `cargo build --locked --offline --workspace` | Pass |

[Compiled identity comparison](current/root-identity-proof.json), [raw tests](current/root-tests.log) and [outcome audit](current/root-test-outcome-audit.json) show the migrated original case and all 20 new cases pass once. The parser accounts for actual child stderr between a libtest name and its final `ok`; no test was rerun to accommodate output formatting. Six added schedule cases and fourteen actual-child/controller/framing cases cover genuine Events, held work, invalid or partial messages, full periods, failed commit, actual exit64 and cleanup. The one-second child lifetime and pipe-pressure host guards remain real waits.

[The final result](current/root-result.json) confirms all 807 frozen source inputs and every historical package byte stayed unchanged through execution. This local gate is not native ARM64 evidence, a lamp run or a timing benchmark. No failed test was retried, and no current-base performance pair was run.

## Integration and compiled baseline provenance

Before copying, all **6,861** tracked blobs/modes matched the clean base. Four existing owned paths match their old pre-edit blobs/modes exactly; six additions were absent from both bases. After copying the ten qualified files, all **6,857** unowned current-base entries remained intact, preserving the newer Memory/Voice changes. [Integration](current/integration.json), [owned-base proof](current/owned-base-proof.json) and complete [before](current/clean-base-tracked-manifest.json)/[after](current/post-integration-tracked-manifest.json) manifests retain the evidence. The Git index was untouched by integration and packaging.

The reused 172-identity pre-controller baseline is supported by static local dependency graphs for Records, Settings, Bus and Msgs. All 101 non-lock conservative inputs are exact. Cargo.lock differs globally only in `lamp-memory`, outside the exact **39-node** selected locked dependency closure; all selected versions, registry checksums and dependency edges match. The [baseline proof](current/baseline-reuse-proof.json), [local graph](current/dependency-graph-new.json), [locked graph](current/locked-dependency-graph-new.json) and [full lock delta](current/Cargo-lock-delta.json) distinguish this scoped equivalence from whole-lock byte identity. The initial conservative rejection remains retained. Reuse applies only to compiled identity comparison, not execution or native qualification.

## Historical review and original-case timing

The separate [historical controller package](../records-controlled-20261016/README.md) remains **82 byte-identical files**, including its original README, source review, logs and checksums. It contains the independent [review](../records-controlled-20261016/review/tests-823-records-controlled-independent-review.md), mechanical reversal proof and the corrected causal close-before-read boundary test. The review correction happened before any execution; no original scenario assertion changed.

Its old-base complete gate was also 190 / 0 / 2. The original-case pair remains **5.614 → 0.735 seconds**, both passing with the same serial nextest command/profile and zero retries. These are shared-Mac debug case durations with unequal background load, excluding compilation; they do not establish isolated speedup, current-base timing, ARM64 performance or the combined original-123 serial target. No historical raw result was rewritten as current qualification.

## Boundaries and retained files

Native ARM64 is pending. The normal PR profile excludes the migrated original Records identity, so it cannot alone demonstrate that case's native coverage; full-profile evidence must identify its execution. This change does not complete #823's original-123 under-two-minute goal. No original check, production timeout, retry policy, workflow/profile, runner, repository setting or device changed.

Every file under `current/` is a byte-exact copy of the release evidence. Its integration-stage README/handoff predate the fresh gate and correctly retain their then-pending status; the final gate result above supplies the completed execution. [Provenance](provenance.json) records copy origins and source hashes; [SHA256SUMS](SHA256SUMS) covers this supplemental folder except itself. No scratch script or binary is committed. Historical checksums remain unchanged.
