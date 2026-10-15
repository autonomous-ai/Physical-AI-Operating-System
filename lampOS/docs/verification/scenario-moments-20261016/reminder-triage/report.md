# Reminder failure in the Scenario Moment gate

Reviewed at 2026-10-16 05:19:57 +0700 (`date`). Worktree: `/private/tmp/lampos-tests-scenario-moments`; base `7ae0eb647914e286817c29bff2abc2934fb19546`. Read-only review; no test, reproduction, rerun, build, source edit, Git mutation, issue/comment, or device operation.

**Finding: this exact failure is already accepted into #739. No new issue is needed. Its cause remains unproved.** Eighth is the existing investigation owner; Third owns Schedule. There is no evidence here supporting routing this case to Fifth.

## Existing issue and readiness rule

- [Seventh’s original report](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6197246324) names the exact test and `consumer/tests.rs:644`, with `AckConfirmed` expected, on #858’s full serial gate at `c5959c003`. It reports a pass alone and unchanged test/consumer/client/store source.
- [Eighth’s acceptance](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6198245051) explicitly says “Taken into #739” for this identity, and labels the 250 ms explanation a source-reading hypothesis.
- [Later Eighth report](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6200616813) says the reminder test passed eight full runs and 300 targeted loaded runs there; that does not explain this failure. These are reported historical results, not runs performed or independently verified by this review.
- The other current failure, rapid mute/reopen, is recorded in [#739](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6198780338) and [#893](https://github.com/autonomous-ai/lampOS/issues/893).

`joining.md:322–327` permits readiness when only known timed Voice failures remain locally **and the exact-head native ARM64 run is green**, with the failed names disclosed and no whole-gate rerun to obtain green. The additional unknown-failure objection is therefore removed: both current failures are known identities. This review establishes no current native result and does not turn the failed local gate into a pass. It supersedes my preliminary message sent before the issue replies were retrieved.

## Exactly what failed

The retained `root-tests.log:1510–1521` records:

```text
thread 'reminders::consumer::tests::completed_playout_survives_both_real_store_reopens_then_acks_without_respeech' ... panicked at crates/voice/src/reminders/consumer/tests.rs:644:5:
assertion failed: matches!(restored.ledger.view(&offered.token.key).unwrap().unwrap().resolution,
    Some(Resolution::AckConfirmed { .. }))
```

The first consumer obtained an offer, persisted its supplied completion proof, passed the assertion that no Ack had yet been captured, and was dropped together with the first socket/engine. Both actual stores reopened successfully. The restored `tick(...).unwrap()` returned successfully, `no_offer` passed, and both ledger-view unwraps succeeded. Its resolution was **not AckConfirmed**; the actual value is not printed.

The subsequent `engine.counts(...) == (0, 0, 0)` and exactly-one-captured-Ack assertions at lines 653–661 **did not run**. There is no retained request list, ledger snapshot, exchange error, per-hop duration, or proof that the Ack was absent, sent, committed, or rejected. Temp cleanup removes the stores on unwind. This fixture supplies authored qualified-completion data; it is not acoustic or actual playout evidence.

The complete retained gate is 1,960 passed / 2 failed / 13 ignored, exit 101; the Voice library is 1,210 / 2 / 8. All 25 new Moment cases passed, and retained fmt, strict Clippy and workspace build succeeded. The gate ran serially (`--test-threads=1`) and did not stop at the library failure. New Moment tests appear later in the same log than this reminder failure. No additional timing or causal conclusion follows from that ordering.

## Call path and clock domains

- `consumer/tests.rs:94–159`: a real Unix listener, one owned server thread, 1 ms host accept polling, 2 s stream read/write bounds, real JSON requests/replies and a captured request vector. Drop signals and joins the thread. The Schedule clock samples are authored fixed values (`sample(1000)`), separate from elapsed-host budgeting.
- `tests.rs:625–642`: **both stores reopen before the restored tick**, not inside the server thread’s request budget. `Consumer::new` has no current offer and starts recovery at the first durable entry.
- `consumer.rs:283–313,355–449`: one `std::time::Instant::now() + 250 ms` deadline covers recovery and subsequent socket work. Recovery gets the current item, reconciles it, persists local Ack intent via `DeliveryLedger::plan_ack`, sends Ack, validates positive matching evidence and persists `AckConfirmed`. The exact same deadline reaches every exchange. Several exchange/validation failures intentionally leave the entry unresolved and return `Ok(())` for a later normal tick.
- `schedule/client.rs:80–236`: nonblocking connect/write/read and poll share the supplied absolute **host Instant** deadline; errors include timeout, unavailable, IO and invalid response. A sent request without a qualifying response proves neither commit nor failure.
- `delivery.rs:692–727,758–843,879–895` and `engine.rs:573–609`: local Ack intent and Schedule Ack handling perform actual durable commits. `store.rs:146–166,332–362` uses real SQLite, FULL/fullfsync, directory sync and file checks. Those filesystem operations can consume host time and are not interrupted by the socket deadline.
- `ipc/lib.rs:43–56` reads the actual OS monotonic clock; the consumer and socket client use direct host `Instant`. No ScenarioClock/Moment path appears in this call graph. There is no Tokio paused runtime or global clock replacement.

A host scheduling or durable-write delay consuming the shared 250 ms budget is plausible, and matches the already-recorded hypothesis. So are suppressed request/response/validation or Schedule-storage refusal paths. The final assertion distinguishes none of them. No production deadline change, retry, weakened assertion, or fixture correction is justified by this log alone; the owner needs the original per-hop/error/ledger/request evidence before choosing a fix.

## Independent preservation and ownership proof

`audit.json` records 194 tracked files across Voice, Schedule, IPC and workspace manifest/lock inputs against base Git blob IDs. **193 are byte-identical; only `voice/src/scenario_clock.rs` differs.** Fifteen directly relevant source/rule files were separately compared with `git show <base>:<path>` and saved as exact snapshots. The entire failed test and all its helpers, consumer, Schedule client/ledger/engine/store/time, IPC clock, and Voice module declarations match the base.

The clock module is behind `#[cfg(test)]` in `voice/src/lib.rs:81`. Its actual diff adds explicit thread-local observation and Moment values; without a root/observer the fallback still takes the same direct host sample. The original seven root cases are byte-identical (module SHA-256 `ef98675980c838fc13ad4632aa2ecdcb0aa04f2df9dd509c499063ed5f1add20`). An unchanged call graph is strong evidence against a direct clock-path regression; it is not proof of every possible indirect resource or runtime effect.

Read-only blame attributes every line of this case to `20bf8cabe0bd4ada2521f41daa15b301543fd250` (gated Schedule tools/due speech). Schedule storage history also includes `74e833eb830eb6f5f0694b4a80150c3d08285b36`. Roles/work-split identify Third for Schedule and Eighth for #739; the issue’s explicit acceptance supplies current investigation ownership. This reminder identity is absent from the immutable original-123 inventory, whose hash is retained; do not add it to that denominator.

Raw evidence is in `/private/tmp/tests-823-moment-reminder-failure-triage/`: full issue responses, exact search responses (including initial sandbox connection failures), selected original comments, source snapshots/hashes/history, unmodified failure excerpts, gate metadata and the independent audit script. Raw gate log SHA-256: `09512be214ccec8075d682155907c4dc0794f86fafc9eb1cfe8d178c38aadc94`. The Records handoff and its ten source files remain untouched.
