# Provider progress identity correction: executable prototype handoff

Recorded 2026-10-16 08:17:44 +0700; all timestamps below came from shell `date`. The approved four-file correction passed this first executable prototype gate: formatting, strict all-target Clippy, the compiled inventory, all 56 selected tests once, and workspace build. This is old-base prototype evidence only, not ready for a pull request or a replacement for the actual-base/full/native gates.

Tree `/private/tmp/lampos-tests-provider-progress`; root-supplied base `8cafa989338fb30c3f38da0cdfd0e307f3752974`, branch `tests/823-provider-progress`. The reviewed delta remains byte-exact to `67adac6b258588c83ae0fccdfc04ccebbaf91d9b33776bcdd2a8eb2ba997bbc3`. Rust `1.96.0`, host `x86_64-apple-darwin`, cargo-nextest `0.9.148`, locked/offline, jobs 2, incremental 0. No source edits, Git/GitHub, device access, target cleaning, mtime adjustment or test retries occurred.

The full before/after lists, filtered list and execution all retain the same four package selectors: `lamp-voice`, `lamp-memory`, `lamp-voice-eval`, `lamp-surround`. Strict all-target Clippy additionally selects `lamp-gemini`, whose dependency edges are recorded from actual Cargo metadata. Gemini's standalone tests are not silently included in the established 2,318-test denominator. The reachable workspace dependency graph and all of its manifest hashes remained exact; the metadata proof explicitly does not infer selected test features from metadata's workspace resolve.

[compiled-list-comparison.json](compiled-list-comparison.json) proves all **2,318 old identities** and their ignore flags survive, with **exactly four added identities**, **2,322 total**, **13 unchanged original ignores**, no removals, and the same 83 package/binary records. The prior raw list is [prior/list.log](prior/list.log), SHA-256 `47f55900365f10148ddb9b2b2d2921d7c3e557eb7f091aa3a633442d51faf184`; the corrected raw list is [list.log](list.log), SHA-256 `0c5b7262310347b8c437b357cc3ffecf02379473ee372160ba93281786c338c3`.

The four additions, all executed and passed:

- `lamp-voice` / `provider_worker::tests::progress::actual_worker_keeps_authority_and_transport_boots_distinct`
- `lamp-voice` / `provider_worker::tests::progress::distinct_transport_boots_do_not_authorize_a_wrong_controller`
- `lamp-voice` / `provider_worker::tests::progress::progress_factory_rejects_mismatched_transport_receive_boots`
- `lamp-voice` / `provider_worker::tests::progress::progress_factory_rejects_mismatched_transport_send_boots`

[focused-compiled-selection-corrected.json](focused-compiled-selection-corrected.json) proves the exact 56-case selection before execution: 22 progress, 20 body-observation and 14 transport cases. [result.json](result.json) records every exact runtime binary/test identity and proves each ran once. Raw [focused.log](focused.log) reports **56 passed in 2.608 seconds**, run ID `7c864a4a-4eb9-4d22-9ae5-1236dffe62f9`, default profile, test-threads 1, no-fail-fast, retries 0. No test failed or was retried. The 1,210 cases and 82 binaries shown skipped by the focus filter were not executed; this is not a full affected-suite result.

An external evidence-checker stop is preserved, not erased. Session `95960` exited 2 after both Cargo list commands exited 0: the checker incorrectly required filtered JSON to retain testcase maps for all binaries. Nextest retained all 83 binary records/paths, but marked 82 excluded binaries skipped and omitted their case maps. The corrected checker consumed those same logs without rerunning either list, verified all 1,266 retained Voice identities/ignore flags and the exact 56 matches, and verified all 83 binary records/paths and the selected binary's hash. Session `44564` then performed the first focused execution and first workspace build and exited 0. See [first-phase-completed.json](first-phase-completed.json), the original [focused-compiled-selection.json](focused-compiled-selection.json), [execution-handles.json](execution-handles.json), [runner.py](runner.py), and [continuation-runner.py](continuation-runner.py). No compile or test failure is hidden by this correction.

| Stage | Start from date | Finish from date | Exit |
|---|---|---|---:|
| rustc-version | 2026-10-16 08:09:46 +0700 | 2026-10-16 08:09:46 +0700 | 0 |
| cargo-version | 2026-10-16 08:09:46 +0700 | 2026-10-16 08:09:46 +0700 | 0 |
| nextest-version | 2026-10-16 08:09:46 +0700 | 2026-10-16 08:09:46 +0700 | 0 |
| metadata-before | 2026-10-16 08:09:46 +0700 | 2026-10-16 08:09:47 +0700 | 0 |
| fmt | 2026-10-16 08:09:47 +0700 | 2026-10-16 08:09:52 +0700 | 0 |
| clippy | 2026-10-16 08:09:52 +0700 | 2026-10-16 08:10:40 +0700 | 0 |
| list | 2026-10-16 08:10:40 +0700 | 2026-10-16 08:14:09 +0700 | 0 |
| focused-list | 2026-10-16 08:14:09 +0700 | 2026-10-16 08:14:10 +0700 | 0 |
| focused | 2026-10-16 08:15:54 +0700 | 2026-10-16 08:15:58 +0700 | 0 |
| workspace-build | 2026-10-16 08:15:58 +0700 | 2026-10-16 08:16:30 +0700 | 0 |
| metadata-after | 2026-10-16 08:16:30 +0700 | 2026-10-16 08:16:30 +0700 | 0 |

Exact commands, working directory, environment, timestamps and raw log hashes are in [gates.json](gates.json), each `*.command.json` and [preflight.json](preflight.json). The build was `cargo build --locked --offline --workspace`. No broad full affected test run or native ARM64 run was authorized here.

All **6,870 frozen source inputs** matched before and after execution. Relative to the original prototype, only the four approved paths differ and all **6,866 unowned inputs** remain exact. The original worker, original EarlyEnd fixtures and original observation tests are unchanged. The previous source proof preserves all 18 existing progress identities and 113 assertions, with only 11 authorized private-call argument removals; its inverse and before/after snapshots are copied under [correction/](correction/).

| Corrected path | Frozen SHA-256 |
|---|---|
| `lampOS/crates/voice/src/provider_worker/observation.rs` | `e9bd7b9af2b46c4f0d55fb58169426559429c90fc94f549547bdb2e0e971544c` |
| `lampOS/docs/voice-test-fixtures.md` | `44a898606f0dafa32b7f20db983ab89c0a06c422341680f9e378d8e00b50aee9` |
| `lampOS/crates/voice/src/provider_worker/progress.rs` | `2b7758236d14c83d7c6fb37b4f3aaf63eb1e375c58ff5a0b8384ef9072097eae` |
| `lampOS/crates/voice/src/provider_worker/tests/progress.rs` | `6eccf65e62082059d7f9dd826b1d4e3c8456212008095ae5e2351eb13c8ba1a0` |

The valid three-boot case proves actual authority/capacity handling, a completed worker tick, Ready, Stop handling and the real outer body result with independent authority and transport identities. The wrong-authority case still receives the original guard's `WrongBoot`; malformed receive/send peer pairs are refused before polling. These observations do not prove Actor/service join or private provider task/child exit, and no clock advance or original frozen-123 scenario migration is claimed.

The target `/private/tmp/tests-823-slow-progress-target` is released to root; no further target or binary inspection is needed. The package retains all first raw logs, source/compiled-list proofs, the checker stop and complete successful execution. Actual-base integration, full affected-suite qualification and native ARM64 validation remain root-owned next gates.
