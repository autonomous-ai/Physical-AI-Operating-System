# Transport-exit deadline migration: source and first gates

Packaged 2026-10-16 08:37:44 +0700, from shell date. Base `69bae3831449bc03d1f11e3ed0c55eccfa1e6f3d`, branch `tests/823-transport-exit-clock`. The corrected local macOS gate is green: **505 passed, zero failed, zero ignored**. Format, strict all-target Clippy and workspace build passed. Native ARM64 PR qualification remains pending.

The same original transport-expiry test passed in **5.055 s before** and **0.047 s after**, a measured 5.008 s reduction for one first passing local sample per version. Both used the same serial/default/no-retry nextest selector. The test still creates the same direct `/bin/sleep 30` child, keeps the complete five-second logical deadline and all original certification assertions, and requires actual kill/reap. Logical time replaces only the original 20 ms wait points. This is a fixture execution measurement, not a process-shutdown guarantee, physical-device measurement, full-suite comparison or CI push-to-result improvement. Shared host load was not controlled or measured. See [exact timing audit](audit/original-case-timing.json).

## Source and unchanged checks

The change owns exactly three files: `physical/mac.rs`, the adjacent new `transport_exit_clock_tests.rs`, and `docs/voice-test-fixtures.md`. A generic clock/wait seam retains the original `wait_child` body and its default host functions. Clock installation is cfg(test), defaults to None and is explicitly selected only by the one original unit case. Ordinary construction and production/integration compilation keep the original host samples, 20 ms sleeps, child-first check, expiry comparison, kill/wait order and return values.

The independent review proves all 27 original inline test identities and all 104 assertion macros unchanged; only the selected original test's helper initializer differs. All original test bodies restore exactly after reversing that one initializer. Nine mac.rs inverse hunks and one appended-document hunk restore the clean inputs; remove the new module for the complete inverse. Seven added cases cover clock purity, live and zero-budget expiry, actual successful/failed exit status, watchdog unwind, and panic before clock installation. [Review](review/review.md), [exact inverse](review/source-inverse.json) and [original checks](review/original-tests-proof.json) retain the evidence.

All 1,022 corrected-gate source inputs were checked unchanged at completion and independently rehashed for packaging. The package copies only the reviewed three final sources and the two baseline existing sources, plus raw hashes/diffs/review records; it does not copy ignored build or cache trees. Source-manifest checks cover their explicitly enumerated inputs, not a whole-repository untracked-file inventory.

## Separate attempts and full execution proof

| Attempt/stage | Result |
| --- | --- |
| Clean baseline list | 498 compiled identities, zero ignores |
| Clean baseline selected original case | One pass, 5.055 s |
| Earlier candidate gate | Format/metadata/Clippy passed; interrupted during listing for a source-review cleanup correction; zero test bodies executed |
| Corrected compiled list | All 498 originals retained with exact ignore flags, plus exactly seven reviewed additions = 505 |
| Corrected selected original case | One pass, 0.047 s |
| Corrected full affected gate | 505 passed / 0 failed / 0 ignored, every compiled identity once |
| Corrected workspace build | Passed |

The first attempt's raw bytes, source hashes and stop/termination records are preserved under `raw/first-gate/`. It stopped at review request during list work and ended with session exit 130, before any test body. Its partial list is not a completed inventory or a passing test gate. The corrected source then received the complete gate; there was no failure retry. The selected original case appears once in its dedicated timing stage and once in the independently checked full suite. Each stage's exact command, shell date, exit and raw hash is retained.

The full gate ran at 08:27:49–08:30:35 on 2026-10-16 (+0700). All 505 results map exactly to the compiled list across 24 binary paths, including the zero-test main binary. Two fake-lamp cases interleave real child output before their separate `ok` line; the audit records both result-line associations. A separate doc-test target has zero tests. See [identity audit](audit/identity-execution-proof.json) and [per-binary timing table](audit/per-binary-timing.md). The 162.33 s sum of libtest binary durations is not the complete Cargo command's wall time. No full baseline suite was executed for a binary-level speed comparison.

The corrected workspace build finished at 08:32:04. Locked/offline Rust 1.96.0, two build jobs and incremental compilation disabled are recorded by the retained runner provenance and raw commands. Cargo metadata finds no workspace reverse dependent on lamp-voice-eval, so its full crate/all-target checks cover the affected test scope, with the additional workspace build.

## Watchdog and process limits

The fixture's clock reads only load its Cell; each explicit wait advances 20 ms, and the selected case asserts all 250 steps. Each loop still checks the actual Child before the deadline. The fixture records actual exit status from that same child and requires SIGKILL after expiry; logical time cannot fabricate exit or reaping. Cleanup guards own the child before fallible liveness observations or waits, including pre-clock panic, and the migrated original helper retains Backend cleanup before handoff.

The immutable five-second host watchdog is checked **before each logical advance**. Process startup and actual try_wait/kill/wait/reap remain the original OS operations. The watchdog does not interrupt blocking OS cleanup and does not establish an end-to-end host upper bound. There is no lamp, audio-device, SSH or network operation in this selected direct-child fixture, and no device latency result is claimed.

## Package verification

Every copied log, source and metadata file is byte-exact to its origin. Historical source-review statements that executable gates were pending remain unchanged in their dated records; the separate completed-gate audit supplies the later result. `provenance.json` maps each copied source/hash and lists exclusions. `SHA256SUMS` covers every included file except itself. Python scripts are excluded and documented only by external checksums. Root owns later code/evidence publication; this packaging ran no Cargo/test command, Git mutation, GitHub write or device action.
