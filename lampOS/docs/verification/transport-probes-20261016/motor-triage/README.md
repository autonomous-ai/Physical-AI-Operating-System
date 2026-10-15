# Transport probe preparation: five Motor failures

Read-only triage recorded 2026-10-16 05:50:56 +0700, on Darwin x86_64. No test was run or rerun, no source was edited, and no Git or GitHub state was changed. Source base: `7ae0eb647914e286817c29bff2abc2934fb19546`. The complete affected gate remains **RED: 1,946 passed / 5 failed / 13 ignored**, across 64 Cargo summaries. The focused transport gate passed 36 / 0 / 1, including all 14 added probe tests. Formatting, strict Clippy, compiled-list preservation and the workspace build passed. These are Mac results, not ARM64 or lamp evidence.

## Disposition

The five failures can be disclosed under the existing **#739 timed Motor fixture family**, with the qualifications below. They are not the three exact identities in #906. The joining rule permits this preparation to become ready **only after its own ARM64 run is green**, while preserving this failed local gate and naming all five tests. This assessment is based on the recorded historical identities or identical preflight boundary, preserved source, and the actual call paths—not just the word `motor` in the names. It does not establish a root cause for the current ready-state failure, prove that the modified binary has no scheduling effect, or turn this local gate green.

Before promotion, report these exact five outcomes to the #739 owner. In particular, `frozen_fixture_preflight_is_not_motor_startup` is a new exact identity relative to the fetched #739/#906 text, although its failing shared preflight boundary is already recorded there. Do not describe it as a historically named failure. #739's separate clock/preflight work remains Eighth's. No test retry, deadline widening or missing-file acceptance is proposed.

## Exact failures and existing coverage

Every name in the following table is under `coordinator::motor::tests::`.

| Identity | Current retained outcome | Existing issue match |
| --- | --- | --- |
| `explicit_current_segment_reaches_inert_counts_without_completing_voice` | `tests.rs:1135`: `assertion failed: motor.is_ready()`. The preceding trace says `motor_validated_offer_to_channel_us=95`. | [#739 comment 6176285913](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6176285913) names explicit-current-segment in the loaded-host Motor family. Its historical symptom was the owned-child timeout, **not this exact assertion**. |
| `fixture_preflight_rejects_missing_wrong_and_failed_markers_and_reaps` | `tests.rs:581`: reading the preflight PID file returned `ENOENT`. | [#739 comment 6190894395](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6190894395) names this exact test and missing-PID symptom; the owner attributes that earlier result to its fresh script not starting inside the 3-second preflight. [Comment 6198245051](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6198245051) discusses the subsequent separate self-test preparation. |
| `frozen_config_and_executable_are_exact_and_source_changes_are_not_adopted` | `tests.rs:453`: `TimedOut: motor fixture preflight did not exit`. | [Comment 6176285913](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6176285913) names frozen-config/executable, with a historical owned-child timeout. The shared first-exec preflight exceeding 3 seconds is independently recorded in [comment 6186529471](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6186529471). The current boundary is preflight, not Motor Ready. |
| `frozen_fixture_preflight_is_not_motor_startup` | The identical `tests.rs:453` preflight timeout. | This exact test name does **not** occur in either fetched issue. Its first call is the same `prepare_fixture("normal")` and fails in the same helper before any scenario-specific assertion. The shared preflight boundary is recorded in [comment 6186529471](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6186529471). |
| `idle_ready_never_inspects_moves_or_claims_listening` | The identical `tests.rs:453` preflight timeout. | [Comment 6176285913](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6176285913) names idle-ready, previously an owned-child timeout. The current symptom matches the shared first-exec boundary recorded in [comment 6186529471](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6186529471). |

[#906](https://github.com/autonomous-ai/lampOS/issues/906) instead names `late_started_callback_cannot_restore_an_offer_after_owner_replacement`, `same_owner_input_end_cancels_queued_gesture_before_inert_bus_write`, and `pending_inspection_is_revoked_before_worker_data_and_release_is_separate`. All three pass in this transport gate. Their earlier missing terminal files / privacy-release assertion are not the five failures above; the same cause must not be inferred.

## What was and was not reached

**Explicit current segment.** The unchanged test completed fixture startup, observed Ready, submitted and observed inspection, then submitted an 80,000-us segment. Its helper waited for `pending.is_none() && active.is_none()` and made two additional polls. That predicate does not prove success: `OptionalMotor::disable` and `begin_shutdown` also clear pending/active. `is_ready()` combines adapter disabled/stopping flags with the Body supervisor's ready/failure/stop/exit state. The retained log does not identify which disjunct failed, the adapter or supervisor failure, the child exit status, or the child terminal receipt. Therefore permit expiry, Body packet freshness, worker failure, and scheduling delay are possible source paths, not demonstrated causes. The 95-us offer-to-channel figure measures the synchronous submission only, not completion or cleanup.

The assertion failure prevents all later checks: new-inspection requirement, original owner retention, active input, successful shutdown/finish, the positive bounded write count, exact final counts `[2000, 2000, 2000, 2000, 1980]`, and emitted mask `16`. These checks remain unchanged but were not shown by this execution.

**Three shared preflight timeouts.** `prepare_fixture` freezes the executable/configuration and calls `preflight` with unchanged `WORKER_STARTUP_TIMEOUT = 3 s`. The absolute host `Instant` deadline starts before `Command::spawn`. The shell's `--fixture-preflight` arm only prints `R` and exits: it executes before exporting worker variables or launching `inert_child`. The nonblocking marker/exit loop parks for 1 ms per wait. The actual error shows that `try_wait` had not observed child exit before this helper checked its deadline; it does not measure a specific scheduling pause or establish whether a marker was already present.

On this timeout path, the helper attempts kill and calls `child.wait()` before returning the original error. The observed `TimedOut` instead of `motor fixture preflight cleanup failed` is consistent with that wait returning successfully; the actual exit status is not retained. That cleanup is not a successful Motor run. Neither adapter nor worker startup occurs after the failing `.unwrap()`. The frozen-config test's hash/source-immutability checks, the dedicated preflight test's no-directory/inode/hash checks, and the idle test's zero-offer/no-inspection/no-listening/Shutdown-receipt checks are all unreached.

**Missing preflight PID.** The self-test accepts any preflight error with `.is_err()`, then `assert_preflight_reaped` reads the shell-written PID. The log records only the absent PID file. It does not record the original preflight `Err` variant, which loop body was active (`exit 0`, wrong marker, or failed exit), whether spawn succeeded, or its child status. Thus the earlier owner's 3-second first-exec explanation is an exact historical symptom match, not direct proof of the current cause. The `waitpid(..., NOHANG) == ECHILD` assertion is never reached, so this run does not demonstrate the exact child's reaping. Subsequent loop cases and directory cleanup are not established.

## Probe reachability and source preservation

An independent byte audit of **221 tracked files across Voice, Body and IPC** finds exactly one changed existing file: Voice `src/transport.rs`. Every other existing file—including all Motor tests, `OptionalMotor`, `process.rs`, Body `runtime.rs`, channels, supervisor and worker—is byte-identical to the base. The snapshot and base Git blob/SHA-256 manifest are retained. Removing the seven specified additive `cfg(test)` snippets from `transport.rs` reconstructs the original file byte-for-byte (one snippet occurs twice). The original two transport tests and their nine assertions remain exact.

The new `Channel::install_probe` is reachable only from `transport_probe_tests.rs`; the repository-wide Voice search finds no call in an original test or Motor path. Ordinary test channels construct an unattached `Slot` (`writer: None`); attempt marks a local flag and successful operations call a no-op `record` when no writer is installed. Allocation, atomics and publication occur only following explicit installation. Every probe insertion and the probe module are `cfg(test)`, so non-test code excludes them entirely. Neither hook changes the original clock samples, send commit order, or receive validation order.

More specifically, these five Motor paths do not use the modified Voice `transport::Channel` for their Motor protocol. `motor/tests.rs` imports `lamp_body::runtime::channel::Channels`; `TestLink` sends/receives through that type; `OptionalMotor` delegates to `lamp_body::runtime::supervisor::Supervisor`. Parent, child core, and worker retain Body `SystemClock`, which reads the unchanged IPC host monotonic clock. The inert child is selected by the original `--exact coordinator::motor::tests::inert_child` command, not by a new probe test. Four current failures occur wholly in shell preflight/PID handling before that child runs. The fifth follows the unchanged Body channel/supervisor/worker path.

This reachability evidence rules out an attached probe changing these protocol observations. It does not prove absence of indirect host load, executable layout, or timing effects from a new test binary. No baseline rerun or causal experiment was performed, and none is claimed.

## Retained evidence

The sibling directory `tests-823-transport-motor-failure-triage/` contains:

- Raw complete/focused gate logs and their original gate/result metadata; the five-failure excerpt is an additional extraction, not a replacement.
- Full fetched JSON for #739 (33 comments) and #906, capture date, and `origin/lamp-voice` joining rule/revision.
- Exact relevant source snapshots, all 221 tracked-file comparisons, the independent reachability/reversal proof, and the original author's proof.
- `SHA256SUMS` for the retained files.

The joining rule says the native ARM64 head must be green and the only local failures must be the known timed process tests; it expressly prohibits repeating the whole gate to seek a clean result. This triage neither runs that native gate nor marks the preparation ready. It leaves the five local failures visible and the exact causes unproved where diagnostics stop.
