# Silent-service clock: retained failed qualification

Prepared 2026-10-16 07:19:06 +0700, from shell `date`. Frozen source base `8cafa989338fb30c3f38da0cdfd0e307f3752974`. The full local gate is **failed: 2,252 passed, one failed, 13 existing ignores**. This package makes no readiness or accepted-exception claim. No test was retried and no native qualification was run.

## Exact failure and limits

The single failure is the identity tracked by [#935](https://github.com/autonomous-ai/lampOS/issues/935):

`lamp-voice` → `coordinator::tests::reminder_roundtrip::actual_consumer_ipc_coordinator_and_two_retirements_ack_only_after_whole_completion`.

The assertion at `crates/voice/src/coordinator/tests/reminder_roundtrip.rs:338` expected `Some(Resolution::AckConfirmed { .. })`. The actual resolution was not printed. Its complete source remains byte-identical to the clean base and frozen gate (SHA256 `f2259a3446933ba7193c95829e6ab092881b828be9d458c79cf6e3b1463bb684`); an exact source copy is included.

In the one-thread serial transcript, the failure is at result row 1150; the migrated silent-service case later passes at row 1486. The full panic excerpt and binary summary remain unedited in `raw/tests.log`. This records the observed ordering and unchanged source. It does not prove the cause, establish an accepted known-local exception, or qualify the full gate. No assertion or bound was relaxed.

## Complete result and source audit

The same **2,266 compiled identities and 13 ignored flags** appear before and after. Every identity has exactly one matching result or existing-ignore row in the complete transcript. No test was added, removed, renamed or newly ignored. All four doc-test commands completed with zero doc tests.

| Package | Passed | Failed | Existing ignored |
|---|---:|---:|---:|
| `lamp-memory` | 205 | 0 | 5 |
| `lamp-surround` | 260 | 0 | 0 |
| `lamp-voice` | 1410 | 1 | 8 |
| `lamp-voice-eval` | 377 | 0 | 0 |

The two implementation changes select paused current-thread Tokio time for the original silent-service case and use Tokio Instant for that case's elapsed origin. A two-substitution inverse restores the entire original worker file. All five assertions, both Legacy/Typed iterations, source capture ages, actual worker/service frames, the two-second quiet barrier and its 1,900 ms minimum remain unchanged. The matching fixture documentation is the only other changed path. All 849 frozen inputs remained exact throughout the gate and subsequent build.

The initial full gate completed 2026-10-16 07:12:34 +0700. Formatting and strict all-target Voice/Memory/Evaluator/Surround Clippy passed; compiled comparison passed; the full serial `--no-fail-fast` test stage returned 101. Its complete raw log SHA256 is `a3a71700d69ac126b755e0c0002b9ab4fcc8515e069c5c54ac31c3dc52886516`. The original gate stopped before the workspace build, and its completion record is preserved unchanged.

Root then explicitly requested the previously unstarted workspace build once as a separate post-failure stage. `cargo build --locked --offline --workspace` returned 0, from 2026-10-16 07:14:08 +0700 to 2026-10-16 07:15:46 +0700, on the same source, target, pinned Rust 1.96.0 environment and two-job setting. `raw/post-failure-build.json` records that distinct stage. Its success does not convert the failed test gate into a qualified result.

## Focused pair and retained preflight failures

The untouched original case and reviewed changed case each ran once with the same nextest command, realtime profile, one thread and zero retries. Nextest case time was **4.097 → 0.337 s**; summary time was **4.102 → 0.361 s**. These are one shared-Mac pair with uncontrolled load recorded at command boundaries, excluding compilation from case time. They do not establish ARM64, device, repeated-run, or original-123 standalone serial timing.

The initial preflight failed because nextest was absent from the inherited PATH; it ran no test. The complete failure remains under `raw/baseline-*`; the corrected environment used the already installed binary and writes separate `raw/baseline-corrected-environment/*` files. Nothing was installed. A first raw-result audit also failed because three tests printed diagnostics between their name and a separate `ok` line. That parser failure is retained; the corrected parser matches all identities and records both line ordinals, without executing tests again.

Pre-after proof documents retain their original then-pending wording. Later completion, failed-result audit and separate build records supply the actual outcomes. Historical records have not been rewritten. All raw copies are byte-exact and listed in `provenance.json`; `SHA256SUMS` covers every other package file. Orchestration scripts remain outside this package and the repository. This package is frozen read-only for root to copy; no shared builder log, Git index, commit, push, GitHub or device was changed by packaging.
