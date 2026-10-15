# Memory health scenarios on an explicit child clock

Recorded 2026-10-16 04:30:44 +0700 from `date`. Part of #823, based on actual Reporter merge `7ae0eb647914e286817c29bff2abc2934fb19546` (#910), with the earlier shared application (#897) and Bus cadence API (#901) already merged.

## Behavior and scope

The dedicated `lamp-memory-fixture` includes the ordinary application body and supplies the explicit Reporter schedule. The original seven `health` identities retain their checks, real private stores, server sockets/requests, Diagnostic receives, stderr bytes, actual child readiness, exit and reap. Four constructor sites select five controlled reporting children; four other children keep their original host path. Ordinary `lamp-memory` arguments, environment and default clock are unchanged. No workflow, nextest profile, product policy, original deadline bound or assertion changes. No ignore or retry is added.

Only Reporter/Bus cadence uses the supplied clock. Its full one-second period and all three sustained-failure periods remain. Actual completed cycles and positive wait boundaries permit logical steps; they do not substitute for received reports, real stderr markers or process status. Each logical reporting/logging operation keeps one original logical window and a cumulative real host watchdog. Request timestamps, State timing/count age, UTC, subscriber leases, real file/socket operations and Reporter shutdown retain their original clocks. The fixture is explicit, with no global or environment activation.

The delta is ten code/manifest/document paths. All 33 Memory Rust sources match the earlier independently reviewed and qualified fixture exactly. The already merged Reporter/application files are not part of this change. Cargo.lock preserves every package/version/checksum; only Memory's existing dependency list gains the already resolved rustix. `integration.json` records exact hashes and inherited dependency changes that required the fresh gate.

## Preservation and current gate

`source-proof/` contains independent child, parent and seven-scenario reviews plus exact reversal proofs. All seven scenario names and 33 scenario assertion macros remain; all 42 original whole-file assertion sites reconstruct exactly after only two diagnostic-reader renames (`log` to `raw_log`). One new helper assertion prevents duplicate controller ownership. Both entirely host-timed scenario bodies, all request/value helpers and original argv/environment/stdio settings remain exact. The fixture binary disables its own libtest harness so the shared application does not duplicate ordinary test identities.

Fresh checks from this tree's `lampOS/`, using Rust 1.96.0, locked/offline dependencies, two build jobs and a separate shared debug target:

- `cargo fmt --all --check`
- `cargo clippy --locked --offline -p lamp-bus -p lamp-memory --all-targets -- -D warnings`
- `cargo nextest list --locked --offline -p lamp-memory --test health --message-format json`: all seven original names, none ignored.
- `cargo test --locked --offline -p lamp-bus -p lamp-memory --no-fail-fast -- --test-threads=1`: **296 passed / zero failed / six existing ignored**. Includes the 35 new child/parent boundaries and actual seven health scenarios.
- `cargo build --locked --offline --workspace`.

`current-gate/` retains every complete unedited log, exact command/cwd/target, date markers, exit code and frozen-source result. The complete gate ends at **2026-10-16 04:29:04 +0700**, from its saved `date` marker. Native ARM64 qualification on this PR head is still required; its normal PR profile excludes these original health cases, so full coverage is also required before ready.

## Historical pair, not current-head/native timing

`historical-pair/` retains the earlier seven-case serial nextest pair, both real executable hashes unchanged between runs: **39.498 to 14.547 seconds**, seven passed each, zero skips or retries. The five controlled identities total **33.437 to 5.590 seconds**; the two host identities total **6.057 to 8.893 seconds**. Both runs use the same helper binaries and original test list; the before and after source hashes match this migration's preserved inputs.

These are shared x86_64 macOS debug measurements with unequal concurrent load, from the earlier integration. They are not a fresh measurement of this merged dependency base, not an ARM64 or lamp result, and not the original-123 serial target. The prior new-child preflight failure and scoped synchronization fix are preserved in the [full earlier record](https://github.com/autonomous-ai/lampOS/blob/99611db2bbcf3724c83a28db6ec3b6bddce53997/lampOS/docs/verification/memory-health-fixture-pair-20261016/README.md); nothing was retried to hide an unchanged failure.

Control/marker watchdogs do not preempt arbitrary kernel filesystem or kill/wait calls. The log-capacity argument applies only to the documented fixed scenario interval. No real-device latency, arbitrary process-load guarantee or full #823 completion is claimed.

`copy-provenance.json` records unedited source-file hashes; `SHA256SUMS` covers all files here. Scratch measuring/auditing scripts remain outside lampOS.
