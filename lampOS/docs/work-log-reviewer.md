# Reviewer work log

Reviewer merged #751, #765, #780, #790, #792, #807 by rebase; manually closed #752, #753 and #777. #703 remains open for its other work; #732 stays open for its unmerged #793 follow-up. First builder merged #778, #789, #796, #798, #809, #816, #817 and #835; devops merged #821, #824, #825 and #826 (the last into main). No lamp actions or production source edits by this reviewer. The reviewer/log branch records the review work under the builder commit/push exception in CLAUDE.md (61d1ea6fc).

Updated: 2026-10-15 19:40:41 +0700

## PR #842 — first pass

[Pull request](https://github.com/autonomous-ai/lampOS/pull/842)

Reviewer: First pass at f4143ff35: I read #836 and the PR's scope, switch comparisons and stated device limits. The full Eyes/Voice source review and fresh touched-crate gates are running. I am checking request/still matching, bounded waits, cancellation, image limits and preservation of camera permissions. No exact-head ARM64 check is attached in the metadata read so far; the Linux-only eyes integration needs that gate.

Reviewer: for the first builder: this changes the camera privacy path and explicitly leaves the lamp run and ten-object before/after accuracy/latency comparison unshown. Host validation cannot establish those results. I will report code findings and gate results here; no lamp or camera action is part of this review.

Checked at 2026-10-15 19:37 +0700.

## PR #822 — b8 malformed ack

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: The committed-write/lost-acknowledgement defect is fixed at b8e3534e4: the retained 1,700 ms delayed-reply proof now refuses journalling, and forgetting leaves recall empty. A missing-ID reply and overlap with a successful ordinary remember also preserve the uncertainty barrier.

Reviewer: [P2] Validate successful rule acknowledgements before accepting them (lampOS/crates/voice/src/rules.rs:250,256-262; remember.rs:1261-1263). Through the actual DaysClient and real Days store, replacing only a successful keep response's ID with 0 or u64::MAX still returns kept_rule:true and admits a later journal write with that invalid rule ID. The store then rejects that journal write. Replacing a real removed:0 response with removed:false or a string returns forgotten:true although nothing was deleted. These are malformed-response/false-confirmation and journal-loss cases; the proof does not show persisted content leaking past the store's link validation.

Require a kept ID in the store's valid positive signed-ID range, and a correctly typed deletion result before reporting success and lowering uncertainty. Preserve valid null/zero missing-rule results, and leave uncertainty raised for invalid shapes. Add these malformed-positive reply cases alongside the missing-ID/lost-reply regression. The other five retained independent rule cases pass; the fresh full gate is still running.

Checked at 2026-10-15 19:37 +0700.

## PR #815 — afa6 interruption gap

[Pull request](https://github.com/autonomous-ai/lampOS/pull/815)

Reviewer: [P2, existing recovery finding] Record the pending host work before the mutations it covers (proposed afa6ea21a, lampOS/crates/update/src/install.rs:420-422). The new debt is still first saved after all units, settings and the zone are written. I ran the real install() in a child process and exited at its existing body-waits logging callback at lines395-398, just before that save. The child exits86 with units/settings/zone present, installed=None, systemd_owed=false and three saved settings hashes. Retrying the real installer returns success with changed=[] and host.calls=[]: no tmpfiles or enable. This is a deterministic process stop, not a copied installer implementation.

Persist/reconcile the recovery intent before writing files that require these host actions, then clear it only after they succeed. Add the process-stop-before-debt regression alongside the failed-enable case. The earlier failed-enable retry and 98/99/100 provenance cases now pass; custom and pre-existing settings are preserved. This remaining interruption window keeps the recovery P2 open. The actual PR head is still73a1fffa8; this finding is on the separately proposed branch.

Checked at 2026-10-15 19:32 +0700.

Completed gate at 2026-10-15 19:35 +0700: fmt and Update all-target clippy pass; full serial tests pass 65 / 0 failed / 1 ignored, without retries. Five independent positive cases pass (failed enable, default provenance, edited/pre-existing settings and tmpfiles retry). Proposed docs now accurately name the missing keeper/setup image prerequisite. That addresses the documentation claim, while the complete blank-lamp bootstrap remains outside this implementation; #811 must stay open for remaining work unless its scope is explicitly completed elsewhere.

## PR #839 — full review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/839)

Reviewer: No patch-introduced source finding at 5494331c4. I independently checked that all 188 original assertions and 10 test identities remain, and the new helpers retain the complete bytes for privacy/journal assertions before the same SIGKILL/reap. Seven independent teardown checks pass, including partial-record timeout/cleanup paths at about 5.01 s and retaining unexpected/private suffixes. Bypassing either wait makes the corresponding proof fail.

Fmt and Health/Memory all-target clippy pass. The full serial gate exits 101: 205 passed / 2 failed / 5 ignored. One is the existing disk-sensitive Health fixture on this 97%-used host filesystem; the affected fixture and its companion pass from the same binary on an isolated healthy 256 MiB filesystem, which was detached and removed afterward. That yields 206 distinct passes, with one inherited unresolved failure: Health's normal-exit usage output at lampOS/crates/health/tests/program.rs:719, missing its usage tail (#837). This was not retried or counted as green. Memory passes all 151 tests, including both changed crash/restart scenarios. Exact-head ARM64 run 38731261120 is green.

Reviewer: for the first builder: #837 concerns the production IPC relay's normal-exit completion, outside these test-only changes. The completed review does not establish a green local Health gate; that remaining gate failure needs resolution or an explicit applicable rule before my merge. #783 remains open for the eighth's broader qualification.

Separate unchanged sweep follow-up: Bus tests/cli.rs:148-159 asserts complete JSONL while polling a live recording, before the kill. The half-line assertion already failed in #795's ARM64 run 38728246092; observing exact records later does not eliminate that live-read race. This is outside this patch, and should stay visible in the sweep handoff rather than be treated as resolved by these two helpers.

Checked at 2026-10-15 19:28 +0700.

## PR #810 — fix 1d06 final

[Pull request](https://github.com/autonomous-ai/lampOS/pull/810)

Reviewer: The remaining timing P2 is fixed at 1d06e35a0. The independent call=1,100 ms / audio=1,102 ms / answer=1,104 ms case now has lead_in=false, audio_before_answer=true and no post-answer delay sample; the first-sound metric remains intact. All seven independent checks pass, including rotated/order/endings cases and the real answer_with logging path: the send timestamp differed by 5 microseconds while stderr was blocked for 100 ms. Fmt, Voice all-target clippy and full serial tests pass: 1,200 passed / 0 failed / 8 ignored, without retries. Exact-head ARM64 run 38732141623 is green. No source finding remains.

Reviewer: for the first builder: the PR and #799 explicitly reserve the model/played lead-in comparison for lamp-4ace, and Second has no lamp. Those outcomes, including avoiding a repeated read-back and measuring first sound, were not established by the host checks. #799 remains open.

Checked at 2026-10-15 19:25 +0700.

## PR #839 — first pass

[Pull request](https://github.com/autonomous-ai/lampOS/pull/839)

Reviewer: First pass at 5494331c4: this is a test-harness-only change to Health and Memory, with production behavior unchanged. I have read the description and evidence limits; the source review and fresh fmt, strict touched-crate clippy and serial tests are now running. I am checking that bounded journal draining preserves the original SIGKILL/recovery and privacy assertions, including repeated terminal-record counts. Exact-head ARM64 run 38731261120 is green. The documented normal-exit stderr failure #837 remains a separate, explicit limitation; this is not yet a merge approval, and #783 stays open for its broader qualification.

Checked at 2026-10-15 19:17 +0700.

## PR #815 — proposed e130 review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/815)

Reviewer: I reviewed second/install-gates at e130016253ee274e0ceb7b013b7fb34e7e8d5bee. Its fmt, Update all-target clippy and full serial gate pass: 63 passed / 0 failed / 1 ignored. Independent tests now preserve both a locally edited setting and a custom setting predating first install. All 14 target services have their own marker condition. The author's systemd-255 VM results support that marker fix; I have not independently run systemd or a lamp.

The actual PR still points to 73a1fffa8, and no ARM64 run for proposed e130 was found. Nine or the branch owner needs to incorporate the fixes and obtain the resulting head CI.

The original installer retry P2 remains at proposed lampOS/crates/update/src/install.rs:395-398: a failed enable followed by a retry still returns changed=[] with no host calls. Its recovery transaction also needs to preserve settings provenance (line367 and lines419-421). An independent case starts with untouched default98/hash98; actual install writes99, then enable fails before state is saved. Passing that durable state to the later-default100 settings boundary misclassifies untouched99 as a local override and leaves it at99. Persist/reconcile both pending host work and the written-default hashes across failure, with regressions for both cases.

The bootstrap P2 remains: docs/install.md:274 claims the keeper unit is installed, but PRODUCT still supplies neither keeper nor setup. The explicit installed-image prerequisite/partial scope or completed bootstrap path is still needed. These are not resolved by the two proposed fixes.

Checked at 2026-10-15 19:17 +0700.

## PR #738 — rebase f848 final

[Pull request](https://github.com/autonomous-ai/lampOS/pull/738)

Reviewer: No new finding at f84889c91. Fresh fmt and Gemini/Voice all-target clippy pass. Full serial tests: 1,497 passed / 1 failed / 10 ignored. The sole failure, coordinator::optional_ring::tests::failed_startup_is_inhibited_through_late_ready_and_bounded_reaping, passes one exact same-binary retry; the original failure remains recorded, for 1,498 distinct passing tests. Both added continuation regressions pass. Six independent current-library checks cover all flag combinations, old wire inputs and false-field omission; continues and continues_cut_off survive independently and remain behind their separate switches. Exact-head ARM64 run 38731001626 is green.

Reviewer: for the first builder: #723 and the builder still explicitly request the rest-labelled probe and five pause scenes on lamp-4ace; those service/hardware judgments were not performed here. The PR body also says full realtime tests run on a lamp-voice push after merge, but #825 now runs them in the full manual/nightly workflow; please correct that CI sentence.

Checked at 2026-10-15 19:15 +0700.

## PR #818 — rebase 134 final

[Pull request](https://github.com/autonomous-ai/lampOS/pull/818)

Reviewer: No source finding at 1347823c3. Fresh fmt, Voice all-target clippy and full serial tests pass: 1,244 passed / 0 failed / 8 ignored, without retries. The opt-in comparison, actual provider-worker sound cases, continuation integration and short-turns join/count tests all pass. The description now agrees with these counts.

The PR currently conflicts with lamp-voice, and exact-head ARM64 run 38729832401 is being retried after its Memory kill/log failure. A rebase and the resulting green head are still required.

Reviewer: for the first builder: Second's current role gives it no lamp, and this PR explicitly says the new sound-response behavior has not been run on one. The 19:03 reviewer rule reserves that case for you. #800 remains open for the recorded-sound count and activation work. No lamp actions were taken.

Checked and handoff clarified at 2026-10-15 19:15 +0700.

## PR #822 — b7 gate docs 2c3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: Completed gate for runtime b7a4dd344, which is byte-identical under lampOS/crates to the current docs-only 2c3d2b692. Fmt and Msgs/Voice/Memory all-target clippy pass. Full serial tests: 1,482 passed / 2 failed / 13 ignored (Msgs 84/0/0; Voice 1,245/0/8; Memory 153/2/5). The two unchanged Memory binary failures are the truncated usage line at tests/binary.rs:152, which also fails on its one exact retry, and the missing asynchronous forget log after SIGKILL at :505. This is not a clean host gate; the original failures are retained. Exact-head ARM64 run 38731294392 is green.

The five earlier independent cases now pass, including atomic six-rule admission and linked-rule deletion; the six new or expanded Memory integration tests pass. The new committed-rule/lost-acknowledgement finding at remember.rs:1222-1226 remains open and applies unchanged to this head: after the delayed acknowledgement, a derived answer escapes without rule_ids and survives forgetting the rule.

I read all four added verification files. They record synthesized speech and an injected known-person stand-in on lamp-52e6, with implicit correction, new-session obedience, correct rule deletion and person isolation. That evidence does not change the lost-acknowledgement defect.

Checked at 2026-10-15 19:13 +0700.

## PR #778 — rebase 5881 final

[Pull request](https://github.com/autonomous-ai/lampOS/pull/778)

Reviewer: Post-merge verification of 5881a3f06 is complete: fmt, Voice all-target clippy and the full serial gate pass, 1,299 passed / 0 failed / 8 ignored, without retries. All eleven independent checks pass: the nine privacy/history/tool-confirmation regressions and two shared flag-parser cases. The rebase preserves keeper pause admission, person-scoped history, written-confirmation provenance, and both routes' privacy revocation. Exact-head ARM64 run 38730179626 is green. No new source finding. The first builder has merged it; the planned opt-in lamp minute is separate from these host results.

Checked at 2026-10-15 19:13 +0700.

## PR #818 — rebase and ci status

[Pull request](https://github.com/autonomous-ai/lampOS/pull/818)

Reviewer: The 1347823c3 rebase preserves both continued and ended_as_sound; the only other own-patch adjustment is sounds_checked:false in the new continuation test. The earlier 84c02b2a3 host gate passed fmt, strict Voice clippy and all 1,192 serial tests (8 ignored), without retries. A fresh gate is now running on the successfully checked-out and asserted 1347823c3 head.

Its exact-head ARM64 run 38729832401 failed the existing Memory kill/log test at crates/memory/tests/binary.rs:309, (4,3) versus (4,4): 1,801 passed, 1 failed, 116 skipped, 2,700 unrun. This is a red head gate, independent of the short-sound changes. No CI rerun by this reviewer.

Checked at 2026-10-15 19:03 +0700.

## PR #835 — gate and privacy proofs

[Pull request](https://github.com/autonomous-ai/lampOS/pull/835)

Reviewer: No source finding at 282e31382. Fmt and Voice all-target clippy pass. The full serial gate passed 1,206 tests with 8 ignored and one inherited motor-fixture failure at coordinator/motor/tests.rs:611; its exact same-binary isolated retry passes, for 1,207 distinct passes. The original failure is retained. Exact-head ARM64 run 38728829369 is green.

Eleven independent real Worker/Channel cases pass: missing/stale/future/reordered acquisition, authenticated duplicate sequence/future envelope/wrong boot, late-then-fresh recovery, and recovery after freshness loss. Every heartbeat keeps microphone/camera denied; recovery requires fresh stable input. The recovery proof fails against the pre-PR wait code, as expected. Host timing was 1.008 s with no observations and 1.055 s after a lone fresh sample, including cleanup: the latter includes the 50 ms freshness window before the one-second unavailable grace.

Reviewer: for the first builder: default privacy/startup behavior and the explicitly requested lamp-4ace power-cut acceptance remain yours. No lamp action was taken; #762 remains open.

Checked at 2026-10-15 19:03 +0700.

## PR #793 — combined rebase verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/793)

Reviewer: The combined 8563d1a34 host review is complete. Fmt and Msgs/Tasks/Policy/Voice all-target clippy pass. Serial tests: Msgs 83, Tasks 122, Policy 162 (2 ignored), Voice 1,197 passed with 2 connection-reset failures (8 ignored); each failed Voice test passes once in isolation, for 1,199 distinct Voice passes. The original failures are retained. All six independent checks pass: new-run/unread baselines, newly watched and restarted tasks with failed first reads, plus the actual TaskNews bus path proving unread-only status neither fetches nor offers progress and withdraws an already queued note. No remaining source finding from this review. The exact-head ARM64 run is being retried by others after the recorded Memory failure; it must finish green. Reviewer: for the first builder: the default task-progress behavior and final merge remain yours; #732 stays open until its follow-up work is merged.

Checked at 2026-10-15 18:59 +0700.

## PR #834 — nested run releases parent

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: [P2] Release only a lock this run invocation acquired (`lampOS/tools/lamp-lock.sh:55,81-82`). acquire() returns success for an existing matching holder/session, but run always installs a release trap. In a real-script stand-in test, an outer run invokes a nested run with the same holder/session; the inner command exits and removes the outer live lock, letting a rival acquire before the outer command finishes. This also affects an explicitly acquired session wrapped by run. Track whether this invocation created the lock, use safe reference ownership, or reject nesting without releasing the parent. The fixture uses only local fake SSH and no lamp.

Checked at 2026-10-15 18:56 +0700.

## PR #834 — release check delete race

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: [P2] Make release ownership checking and removal atomic against acquire (`lampOS/tools/lamp-lock.sh:67`). The separate ours() and rm SSH calls allow a delayed cleanup to remove a successor's lock. A bounded stand-in proof pauses release A just before rm; another cleanup of A releases it; B acquires; the first rm resumes and deletes B; C then acquires while B still owns its work. Move check/remove and acquire under the same remote serialization mechanism, or use a token-aware protocol that cannot unlink a replacement. A fixed-string owner comparison alone does not close this race. Reproduced with unchanged 32d4c3253 and local fake SSH only.

Checked at 2026-10-15 18:56 +0700.

## PR #834 — ci memory kill family

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: Fmt --all --check and bash -n pass on 32d4c3253; no Rust crate changed. Exact-head ARM64 run 38728254688 failed the existing Memory kill/log test at `lampOS/crates/memory/tests/binary.rs:309`, (3,4) versus (4,4). It passed 1,801, failed 1, skipped 116 and left 2,694 unrun. This is separate from the posted script findings and #833 amendment; all remain to be resolved before merge. No CI rerun or device action by this reviewer.

Checked at 2026-10-15 18:52 +0700.

## PR #795 — ci now bus kill family

[Pull request](https://github.com/autonomous-ai/lampOS/pull/795)

Reviewer: Exact-head ARM64 run 38728246092 at 862fec7d0 has ended red in a different, existing kill-test family: `lampOS/crates/bus/tests/cli.rs:152`, a_killed_recorder_leaves_whole_lines_and_its_socket_goes_with_the_next_publish, “half a line”. It passed 565 tests, failed 1, skipped 117 and left 4,013 unrun. This aborted before the Linux privacy regression, so that regression is still unverified on this head. The original ENOTCONN assertion is fixed in source. I have not rerun CI; #783 owns the kill/log failures.

Checked at 2026-10-15 18:52 +0700.

## PR #825 — nightly replacement verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/825)

Reviewer: Update to my earlier rollout finding: #826 is now MERGED into main. Its manually invoked nightly trigger [38727430837](https://github.com/autonomous-ai/lampOS/actions/runs/38727430837) launched the full lamp-voice run [38727449008](https://github.com/autonomous-ai/lampOS/actions/runs/38727449008) at e12a6cc83. I verified all four ARM64 shards and the first shard's doc/own-harness checks succeeded. This resolves the missing-nightly replacement and green-full-run prerequisite noted earlier. The first scheduled trigger itself is still prospective; the reported schedule is 19:00 UTC. Failure-report behavior was skipped on this green run and remains unexercised. No CI run was started or rerun by this reviewer.

Checked at 2026-10-15 18:52 +0700.

## PR #822 — lost rule ack journal

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: [P2] Fence ambiguous rule writes before sending them (`lampOS/crates/voice/src/remember.rs:1222-1226,1236-1247`). At b7a4dd344, an independent real Days-store proof commits keep_rule but delays only its acknowledgement 1,700 ms, beyond the 1,500 ms call budget. The call returns unavailable; the next rule-derived answer is nevertheless journalled with source_ids=[] and no rule_ids. After forget_rule(1) returns forgotten:true, recall still retrieves that answer. Rule IDs are registered only after the acknowledgement, and this path bypasses the uncertain_writes barrier used by ordinary remember/forget at1029-1035. Enter a bounded ambiguity/provenance barrier before dispatch and clear it only after a validated acknowledgement; no unlinked journal write should escape on timeout or malformed acknowledgement. Add the committed-write/lost-reply/forget regression. The six-rule concurrent admission case and the prior invalid-ID and blocked-bus cases now pass.

Checked at 2026-10-15 18:52 +0700.

## PR #808 — gate clean rebase needed

[Pull request](https://github.com/autonomous-ai/lampOS/pull/808)

Reviewer: Review of c9d997e51 is complete with no code finding. Fmt and strict all-target clippy pass for Bus, Msgs and Settings; the full serial gate passes 209 tests with 2 manual timing tests ignored. An independent actual-CLI temporary-directory/socket check also passes disabled no-entry behavior, guarded save, retry/stale refusal, restart persistence and shutdown. Exact-head ARM64 run 38726383908 is green, but GitHub currently reports mergeStateStatus=DIRTY. Please rebase onto lamp-voice, preserving both new declarations and existing ones, and let the new head CI finish. I will check the conflict resolution and resulting head. The documented unwired readers and persistent-log integration remain follow-up scope; a merge will not claim those are deployed.

Checked at 2026-10-15 18:52 +0700.

## PR #809 — postmerge gate complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/809)

Reviewer: The independent host gate for merged head 9bda9dcd3 is complete: fmt and Voice/Voice-eval all-target clippy pass; serial tests pass 1,626 (Voice 1,193, Voice-eval 284, Memory 149), with 13 ignored, no failures and no retries. Exact-head ARM64 run 38724198733 is green. These checks do not resolve the separately posted P1: the documented model-complete endpoint does not reach the continuation path that sets continues_previous=true. That behavior still needs a coordinator-path regression and follow-up fix.

Checked at 2026-10-15 18:48 +0700.

## PR #795 — rebase final host gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/795)

Reviewer: The 862fec7d0 delta is exactly the expected Linux test error kind plus its explanatory comment; the bounded exit, authority and expired-lease assertions remain. Fmt and Voice all-target clippy pass at this head. The complete six-crate host gate on its byte-identical host runtime/source passed 1,842 tests with 11 ignored and five failures: motor unresolved cleanup, optional-ring failed startup, optional-ring versus mandatory privacy startup, early-end duplex, and reminder completed-playout/reopen. Each failed test passed once in isolation, giving 1,847 distinct passing tests; the original failures/logs are retained under the Queue timing/kill-test families. The Linux-only privacy worker cannot execute on this Mac, so its new exact-head ARM64 run still has to pass. Reviewer: for the first builder: physical microphone/privacy-switch acceptance and merge remain yours.

Checked at 2026-10-15 18:48 +0700.

## PR #797 — footprint misses hysteresis

[Pull request](https://github.com/autonomous-ai/lampOS/pull/797)

Reviewer: [P2] Include release hysteresis before calling this the new touched footprint (`lampOS/crates/controls/src/touch_calibrate.rs:441-448`; `lampOS/docs/controls.md:274-276`). A legal zero-debounce profile with quiet delta 0 and own levels 30 gives touch 13/release 8. For sustained successive blocks [30,0,0], [9,30,0], [9,9,30], previously touched pads remain active at 9 until falling below 8. The real replay CLI reports all 300 readings as one pad; applying those thresholds with hysteresis gives 100 one-pad, 100 two-pad and 100 three-pad readings. Therefore a reported footprint of one/two does not prove a chord cannot qualify. Replay touch/release state with explicit debounce/baseline limits, or label the numbers as instantaneous threshold crossings and remove that gesture conclusion. This is a bounded synthetic case, not a hardware measurement; the physical slider/chord decision stays with the first and third builders.

Fresh 4543f1cb9 gate: fmt and Controls all-target clippy pass; all 385 serial tests pass without retries. The head full CI run 38727552759 failed the existing Memory kill/log test; the separate shard run was still in progress when checked.

Checked at 2026-10-15 18:48 +0700.

## PR #797 — half cutoff rounds down

[Pull request](https://github.com/autonomous-ai/lampOS/pull/797)

Reviewer: [P2] Preserve the stated 50% own-swipe cutoff without rounding it down (`lampOS/crates/controls/src/touch_calibrate.rs:263-267`). For own-swipe peaks [33,16,27,30], 16 is below half of 33, but floor(33*50/100) admits it. The real calibrate-touch --replay CLI with measured noise 3 writes touch 8/release 5 instead of using level 27 and touch 12/release 7. A graze can thus set the much lower threshold this rule intends to reject. Compare peak*100 >= strongest*OWN_SHARE_PERCENT, or round the cutoff upward, and cover odd strongest values. Reproduced at 4543f1cb9 using independent recorded-input fixtures; no lamp was used.

Checked at 2026-10-15 18:48 +0700.

## PR #793 — rebased ci memory failure

[Pull request](https://github.com/autonomous-ai/lampOS/pull/793)

Reviewer: Exact-head ARM64 run 38727844448 on 8563d1a34 failed again in the existing Memory binary kill/log test (`lampOS/crates/memory/tests/binary.rs:309`), with (3,4) observed versus (4,4) expected. It ran 1,802 tests (1,801 passed, 1 failed), skipped 116 and left 2,699 unrun. This remains a red head gate; I have not rerun it. The rebase preserves the own fixes and supplies the three new Voice unread literals. My fresh Msgs, Tasks and Policy host gates pass; Voice is still running. The Memory failure is already tracked with the tests/devops owners, rather than being a new #793 source finding.

Checked at 2026-10-15 18:46 +0700.

## PR #835 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/835)

Reviewer: for the first builder: #835 changes the default startup privacy path and explicitly requests lamp-4ace's power-cut acceptance before merge. I have read #762 and the full diff at 282e31382; I am checking invalid/stale/sequence observations, the one-second unavailable bound and the fresh-unmute requirement, and running the full Voice gate. #762 remains open for its other startup causes and real power-cut evidence.

Checked at 2026-10-15 18:46 +0700.

## PR #834 — invocation and owner amendment

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: [P2] Make the documented command executable (`lampOS/tools/lamp-lock.sh:1`; `lampOS/docs/joining.md:420-428`). The file is committed as 100644, so the direct lamp-lock.sh invocations fail with permission denied. Set executable mode or consistently document bash invocation. bash -n passes and no Rust crate is changed. #833 now authorizes reviewer merge, subject to its requested age/non-answering-holder status and person-only stale removal rule; those amendments are not yet in head 32d4c3253. The two separately posted lock-identity/transport findings also need fixing.

Checked at 2026-10-15 18:46 +0700.

## PR #834 — unreachable is not free

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: [P2] Preserve transport failures in status/release (`lampOS/tools/lamp-lock.sh:32-38,64-68`). With the transport returning 255, status prints “is free” and exits 0, and release prints “was not held” and exits 0. The preliminary reachability check only covers acquire. An unavailable lamp must be reported as unknown/error, not free or successfully released. Distinguish a successful remote absent-file result from SSH/read failure and test both commands with the transport down. Reproduced using the real script and a fake transport; no lamp was contacted.

Checked at 2026-10-15 18:46 +0700.

## PR #834 — literal lock owner

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: [P2] Compare holder/session literally (`lampOS/tools/lamp-lock.sh:40`). plain() explicitly permits dots, but ours() interpolates those values into grep regular expressions. With the real script and a local stand-in for SSH, acquiring as builder/sessionX1 then releasing as builder/session.1 succeeds and deletes the other session's lock. The same match also lets acquire report an unrelated session's lock as already ours. Use fixed-string, whole-line comparisons and add this distinct-session case. Reviewed head 32d4c3253; no SSH or lamp action was performed.

Checked at 2026-10-15 18:46 +0700.

## PR #818 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/818)

Reviewer: First pass at 84c02b2a3: I have read #800, the description and worker-loop switch tests. The new quiet-room-checked value is opt-in; I am checking the first-output decision and the short-turns joins/counts, then running the Voice gate. Exact-head ARM64 run 38724241108 is green. #800 remains open for the recorded-sound count and activation acceptance described there. No lamp actions will be part of this review.

Checked at 2026-10-15 18:42 +0700.

## PR #808 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/808)

Reviewer: First pass at c9d997e51: these are additive logging/settings interfaces with consumers in the same PR; existing first_topics and message types remain unchanged. #699 assigns this scope to the third builder, so this remains in the reviewer merge lane under the 15:22 rule. I am reviewing the complete implementation and running fmt plus the Bus, Msgs and Settings gates. Exact-head ARM64 run 38726383908 is green; that is not yet a final approval.

Checked at 2026-10-15 18:42 +0700.

## PR #817 — reset task history

[Pull request](https://github.com/autonomous-ai/lampOS/pull/817)

Reviewer: [P3] Include the active task-news history in the reset plan (`lampOS/crates/setup/fixtures/reset-plan-example.json:11-14`). The default `/var/lib/lampOS/voice/tasks-told` (`voice/src/task_news.rs:33`) is absent. An independent fixture remaps the shipped plan under a temporary directory, calls production reset::reset, and confirms reset completes and removes its marker/journal while the old store/task/observation identifiers remain. Add this file and a reset-coverage assertion. The retained data is identifiers, not task contents; I am not claiming a cross-reset suppression or credential leak.

Post-merge audit of b21a26bb3: fmt and Setup all-target clippy pass; all 27 serial tests pass. Exact-head ARM64 run 38722230047 is green. The two independent regression assertions for reachability and reset fail as described. The first builder merged this PR; no device actions were performed in this review.

Checked at 2026-10-15 18:42 +0700.

## PR #817 — own page port conflict

[Pull request](https://github.com/autonomous-ai/lampOS/pull/817)

Reviewer: [P2] Make own-page mode coexist with the AP script's nginx start (`lampOS/crates/setup/src/main.rs:284,340`; `fixtures/serve-own-page.json:2`). The supplied fallback binds 0.0.0.0:80, but the preceding device-ap-mode unconditionally restarts nginx (`scripts/imager/build-orangepi.sh:753`). On a V1 image with nginx installed but without the reused React bundle, selecting this fallback therefore leaves nginx owning port 80 and Setup fails its bind. Stop/suppress nginx for own-page mode or use a compatible separate port/proxy, and cover that configuration. The normal behind-nginx fixture is unaffected. Evidence is the shipped call/script/bind order; no lamp or systemd run was performed.

Checked at 2026-10-15 18:42 +0700.

## PR #817 — tcp is not tls

[Pull request](https://github.com/autonomous-ai/lampOS/pull/817)

Reviewer: [P2] Preserve the bounded TLS fallback for reachability (`lampOS/crates/setup/src/wifi.rs:408-412`). With ICMP blocked, a TCP listener that never speaks TLS is currently accepted as Internet. An independent fixture invoking the actual ImageHost::internet with a failed ping and a loopback listener reproduces true. Join can therefore save the credential/marker and leave setup even though an intercepting network cannot reach the service. V1's fallback completed a bounded TLS handshake (`system/network/reachability.go:38-49`). Restore that check, including bounded resolution, and cover a listener that accepts TCP but cannot complete TLS. This finding is about reachability, not credential disclosure.

Checked at 2026-10-15 18:42 +0700.

## PR #817 — hostname before hotspot

[Pull request](https://github.com/autonomous-ai/lampOS/pull/817)

Reviewer: [P2] Read the device hostname after starting the hotspot (`lampOS/crates/setup/src/main.rs:279-284`). On a fresh image with the generic image hostname, this caches the old name before device-ap-mode assigns lamp-<suffix> (`scripts/imager/build-orangepi.sh:725-726`). If the AP disappears before the phone receives lan_ip, V1's handoff either disables mDNS for that generic name or probes the wrong .local address and reports a failed join. Move the read after hotspot() and cover a hotspot start that changes the name. This is verified from the shipped script and UI handoff code; no phone or lamp run was performed.

Checked at 2026-10-15 18:42 +0700.

## PR #795 — linux close before reap test

[Pull request](https://github.com/autonomous-ai/lampOS/pull/795)

Reviewer: [P2] Handle the Linux close-before-reap error without dropping the exit checks (`lampOS/crates/voice/tests/privacy_worker.rs:469-480`). Exact-head ARM64 run [38726030376](https://github.com/autonomous-ai/lampOS/actions/runs/38726030376) fails the new losing_controls_ends_the_real_worker_and_expires_its_lease_despite_current_allowed_authority test with ENOTCONN (Transport endpoint is not connected, OS 107) at line 474, after “physical privacy source expired”. This loop already tolerates the worker closing IPC before try_wait reports death, but accepts only BrokenPipe, ConnectionReset and ConnectionRefused. Linux can report NotConnected in the same window. Handle that specific case or wait for confirmed exit through expected close errors, retaining the bounded deadline, still-current controller authority, abnormal-exit and expired-lease assertions. The run stopped at 4,233 passed / 1 failed, with 293 not run and 103 skipped; this is not a green head gate. I have not rerun it.

The rebase's own patch is preserved; range-diff changes only its placement beside the merged attention/tasks declarations. The fresh six-crate host gate is running, but this Linux-only test cannot execute on the Mac host.

Checked at 2026-10-15 18:34 +0700.

## PR #794 — rebase verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/794)

Reviewer: Rebase `abbd3ce8d` is verified with no new finding. #770's first waiting answer is preserved; a previously reported waiting task is described as still waiting on its computer without inventing a question. FIFO waiting/tell, news selection and store/task/observation identity are byte-identical to the previously reviewed implementation. Fmt and Voice all-target clippy pass; the fresh full serial Voice gate passes 1,189 tests with 8 ignored and no retries. Exact-head ARM64 run 38725512979 is green. Reviewer: for the first builder: this changes default speech/task-check behavior, so your story/follow-up acceptance and merge remain.

Checked at 2026-10-15 18:33 +0700.

## PR #810 — audio during call still negative

[Pull request](https://github.com/autonomous-ai/lampOS/pull/810)

Reviewer: [P2] Exclude all audio earlier than the tool answer from the post-answer metric (`lampOS/crates/voice/src/turn_times.rs:279-286`). The new lead_in label tests audio < called_us, but an audio event between called_us and sent_us still passes the filter and is subtracted from sent_us. An independent replay at `6cc4f2c09` with call=1,100 ms, first_audio=1,102 ms and answer_sent=1,104 ms reports lead_in=false and first_audio_after_answer_ms=-2.0. Worker and coordinator timestamps are recorded on different processing paths, and audio can be forwarded while a task call is outstanding. Exclude samples earlier than sent_us regardless of the before-call label, or record a separate first-post-answer audio event. Add this between-call-and-answer case. The original order/rotated-files, 900 ms lead-in and terminal-ending proofs now pass; the blocked-stderr proof records about 1 microsecond drift despite a 100 ms blocked log. This one measurement finding remains.

Checked at 2026-10-15 18:33 +0700.

## PR #815 — target condition does not gate services

[Pull request](https://github.com/autonomous-ai/lampOS/pull/815)

Reviewer: [P1] Gate each lamp service, or let the keeper exclusively choose the first-boot target (`lampOS/deploy/product/lampos.target:8`; `lampOS/crates/update/src/install.rs:152-161`, `:394`). ConditionPathExists on a target does not gate its dependencies. install enables the product services with WantedBy=lampos.target and enables that target under multi-user.target. On boot, systemd queues those services before checking the target's missing setup marker; Voice and Ears have no marker condition of their own. In particular, Ears needs no provider credential to open its room microphone. The promised “nothing listens before setup is done” is therefore not enforced by this unit graph.

This follows from the actual unit graph and [systemd 252's Conditions and Asserts contract](https://manpages.debian.org/bookworm/systemd/systemd.unit.5.en.html#Conditions_and_Asserts): condition checks occur after dependency jobs are queued and do not condition those dependencies. I have not run a native boot or touched a lamp. Add the marker condition to every gated part, or avoid pulling the target in at boot until the keeper selects it, and test marker-absent/marker-present starts against real systemd. This also corrects the symptom in my bootstrap comment: a skipped target does not mean its parts stay stopped. For the first builder: this is the setup privacy boundary.

Checked at 2026-10-15 18:29 +0700.

## PR #815 — bootstrap not installed

[Pull request](https://github.com/autonomous-ai/lampOS/pull/815)

Reviewer: [P2] Supply or explicitly require the bootstrap path before calling this a complete blank-lamp install (`lampOS/docs/install.md:263-268`; `lampOS/crates/update/src/install.rs:29-112`). The doc says install installs the keeper's unit, but PRODUCT contains neither a keeper unit/binary nor the setup service. No keeper/setup service is installed to own the first-boot setup and post-setup hand-over on the documented base image. The separate target-condition finding below addresses premature starts of its dependencies. Add the keeper/setup integration (including the #817 dependency) and test that declared first-boot path, or make it an explicit installed-image prerequisite and mark this PR partial with #811 left open. A successful file-copy test alone does not complete that boot path.

Checked at 2026-10-15 18:25 +0700.

Clarified the boot symptom at 2026-10-15 18:29 +0700; the missing bootstrap dependency remains.

## PR #789 — final gate after first merge

[Pull request](https://github.com/autonomous-ai/lampOS/pull/789)

Reviewer: Final host evidence for merged `07e935f8a`: the 29-to-32 docs correction is verified, both other patches are exact range-diff matches, and the Voice tree is identical to the prior reviewed head. Msgs/Tasks/Policy dependency trees match merged #751. Fresh fmt and Voice all-target clippy pass. The full serial run reported 1,216 passed / 2 failed / 8 ignored: the unchanged reconnect-heartbeat fixture (ConnectionReset) and reminder completed-playout/real-store-reopen fixture (AckConfirmed assertion). Both exact same-binary isolated retries pass; originals are retained, giving 1,218 distinct Mac passes after retries. Exact-head ARM64 run 38725112153 is green. No remaining source/doc finding. The first builder merged this PR; #739 remains open for part 2.

Checked at 2026-10-15 18:28 +0700.

## PR #825 — merged nightly gap

[Pull request](https://github.com/autonomous-ai/lampOS/pull/825)

Reviewer: Follow-up after the concurrent merge: #825 is merged as `0fd1db7f2`; #826 remains open against main as checked below. The automatic-full-run gap is therefore current, not just a merge-order concern. The first builder's 18:26 condition explicitly requires keeping full push runs until the nightly trigger is merged and has run green once. Devops needs to restore that coverage until the condition is satisfied. The successful manual full run validates this head but does not install an automatic trigger.

Checked at 2026-10-15 18:28 +0700.

## PR #809 — model cut continuation missing

[Pull request](https://github.com/autonomous-ai/lampOS/pull/809)

Reviewer: [P1] Wire name preservation into the actual model-complete comma-cut path (`lampOS/crates/voice/src/coordinator.rs:5059`, `:5153`). With LAMP_CUT_KEEPS_NAME=on and the documented smart-turn-fast / 600 ms / kept-start options, early_end_blocks is None. A deterministic replay through the real TurnDetector gives model_complete after 400 ms of the comma pause, then a fresh Activity::Start when speech resumes. That candidate goes through commit_admitted_input, which hardcodes continues=false at line 5059, so Attention starts afresh and loses the first part's call exactly as before.

The only switch-controlled true assignment is in continue_after_early_end at line 5153, reached through EarlyEnd::hear only with the separate --early-end-ms option and before detector confirmation. Even with that option, AdmissionStep::End confirms/clears the pending early end at lines 3851-3854. The added worker tests manually inject continues=true and do not exercise this coordinator boundary. Carry bounded predecessor/session/privacy identity through the post-model-end admission path and add a coordinator-level regression for a model-complete comma pause followed by the rest of the same request. The independent detector/options proof passes on the reviewed head 9bda9dcd3 and establishes the event path; it is not a full coordinator run. This needs a follow-up now that the PR has merged.

Checked at 2026-10-15 18:28 +0700.

## PR #825 — first builder validation

[Pull request](https://github.com/autonomous-ai/lampOS/pull/825)

Reviewer: for the first builder: this changes default merge CI and the validation behind update artifacts, so merge belongs with you/devops. YAML parses, all seven run scripts pass bash -n after expression substitution, fmt and diff checks pass; no Rust crate changes. I verified exact-head 6cd157ce1 full run 38723605504 succeeded on all four ARM64 jobs. The failure-report job is skipped, so its failure behavior remains unexecuted as the PR states. The earlier red-full-run merge-hold finding remains unresolved: this workflow opens an issue but does not stop later merges or artifact publication. Please also remove the generated attribution/footer from the PR description.

Checked at 2026-10-15 18:25 +0700.

## PR #825 — nightly dependency

[Pull request](https://github.com/autonomous-ai/lampOS/pull/825)

Reviewer: [P2] Land and verify the nightly trigger before removing the remaining automatic full run (`.github/workflows/lampos-arm64.yml:102-103`). #826 is still OPEN against main. This head selects the filtered pr profile for every push, and only an explicit workflow_dispatch full=true runs realtime tests. If this PR lands first, no automatic event runs them until that separate dependency is installed and working, despite docs saying the nightly run does. Keep push coverage until #826 is merged/verified, or make that dependency an explicit merge prerequisite.

Checked at 2026-10-15 18:25 +0700.

## PR #815 — preserve local settings

[Pull request](https://github.com/autonomous-ai/lampOS/pull/815)

Reviewer: [P2] Preserve existing per-lamp settings when install is run again (`lampOS/crates/update/src/install.rs:362-363`). Every entry in SETTINGS is replaced with the embedded default whenever its bytes differ. With the real installer and a temporary root, install once, change the primary button's line in /etc/lampOS/controls/controls-pins.json from 99 to 100, then install the same signed build again: it silently restores line 99. #811 explicitly puts lamp-specific pins/calibration in settings or drop-ins, and rerunning install is also the documented updater self-upgrade path. Seed defaults only when absent, or provide a migration that preserves local overrides; add a rerun case with a valid changed pins/settings file.

Checked at 2026-10-15 18:25 +0700.

## PR #815 — retry failed enable

[Pull request](https://github.com/autonomous-ai/lampOS/pull/815)

Reviewer: [P2] Finish the host actions after an interrupted install (`lampOS/crates/update/src/install.rs:392-395`). They run only when this invocation changed a file. An independent test against the real install() function injects failure from Host::enable after the users/files/links have been written, then runs install again. The retry returns success with changed=[] and host.calls=[]: it never retries enable (or daemon-reload inside it), so a blank lamp can remain unbootable after an otherwise recoverable install failure. The same gap exists after tmpfiles failure or a process stop before these actions. Persist/reconcile unfinished host work independently of file changes, and add the failed-first-run/retry regression. Fmt, Update all-target clippy and the existing serial gate pass: 59 tests, 1 ignored.

Checked at 2026-10-15 18:25 +0700.

## PR #822 — runtime fixes gated provenance pending

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: At frozen `8a4304da5`, independent checks now pass for full rule-state reading, nondestructive missing/invalid forget IDs, and the bus work leaving the note path (118 ms acquisition while the publisher is blocked for 800 ms). Fmt and Msgs/Voice/Memory all-target clippy pass. Full serial gate: 1,419 passed / 1 failed / 13 ignored. The sole failure was the unchanged reconnect-heartbeat test's ConnectionReset; its exact isolated retry passes, with the original failure retained. Memory's earlier killed-process log-count test passed in this full run.

The provenance proof passes only because journalling is disabled, which the first builder explicitly rejected. That finding and the separately posted atomic six-rule admission finding remain open. The latest `37b52aab5` wording/test/docs delta was read, but this full gate is for `8a4304da5`. Neither fix head has an attached ARM64 run in the checked branch history; older green runs do not validate these fixes. Corrected spoken acceptance is also still pending.

Checked at 2026-10-15 18:20 +0700.

## PR #791 — reference gap fixed ring pending

[Pull request](https://github.com/autonomous-ai/lampOS/pull/791)

Reviewer: At `6735459f6`, the controlled-reference finding is fixed: hold_up sends a reference every 10 ms while withholding authority, and its sequence/sample cursor is shared with streaming. Three gaps match the unchanged three-reseeds-per-minute recovery policy. Your native old-guard 5/5 failures and new-guard 5/5 passes are explicit execution evidence, separate from PR CI; I have reviewed that report, not independently run it. Please retain its command and raw per-test output with the evidence. Fmt and Voice all-target clippy pass. Production and all Mac test inputs are byte-identical to the prior gate, so its Voice 1,190 passed / 8 ignored and Audio 160 passed / 2 ignored apply unchanged.

Reviewer: for the first builder: your newer all-workers failure remains open. The test at `lampOS/crates/voice/tests/capture_hold_worker.rs:642-665` has no ring worker or real coordinator, and this revision changes no production code. Ring expiry is terminal in ring_worker, but coordinator.rs:5230-5244 handles a failed optional ring; the log alone does not identify what caused Voice's final authority-snapshot expiry. Please keep the all-workers reproduction and trace, including the ring, as the remaining integration check.

Checked at 2026-10-15 18:20 +0700.

## PR #817 — first review handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/817)

Reviewer: for the first builder: `b21a26bb3` is now ready, but #806 step 4 still requires a timed wiped-lamp-to-conversation run with a phone, and the PR explicitly says that is unperformed. The reset's durable wipe/reboot and setup-to-keeper transition also need that integration judgment. I am reviewing the host implementation and gates; please keep #806 open for its unperformed acceptance even if the code is merged separately.

Checked at 2026-10-15 18:20 +0700.

## PR #809 — first review handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/809)

Reviewer: for the first builder: `9bda9dcd3` keeps both changes opt-in and has green exact-head ARM64 run 38724198733, but #782 reserves the lamp-4ace 400/550 ms comparison and this PR has no own-lamp demonstration. The named local loading-test failure is retained. I am reviewing the continuation/attention boundaries, endpoint option validation, replay evidence and full touched-crate host gate; lamp acceptance and merge remain with you.

Checked at 2026-10-15 18:20 +0700.

## PR #793 — second fixes verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/793)

Reviewer: Both follow-up findings are fixed at `ce2b47116`. The actual-Tracker new-run/unread proof now asks at the first observed count 1, and its control passes (2/2); both real-service unread/recovery checks also pass (2/2). Fmt and Msgs/Tasks/Policy all-target clippy pass. Fresh serial gates: Msgs 83, Tasks 122, Policy 161 passed, with 2 Policy ignores and no failures.

The exact-head ARM64 run [38724419493](https://github.com/autonomous-ai/lampOS/actions/runs/38724419493) failed in unchanged Memory `a_killed_process_leaves_a_store_the_next_one_loads_and_a_socket_it_replaces` (`crates/memory/tests/binary.rs:309`): log counts (3, 4), expected (4, 4). It stopped after 1,802 passed / 1 failed, with 2,632 tests not run and 102 skipped. This is not a green head gate; I have not rerun CI.

#765 is now merged as `4ad0ec5c3`. Please rebase onto it and update the new Voice TaskStatus literals for `unread`, then gate Voice all targets as well as the three changed crates. The present head predates those literals. No remaining source finding on this reviewed head; the combined tree and its green exact-head ARM64 remain to verify. For the first builder: this still changes the enabled progress path on lamp-4ace and carries your lamp check.

Checked at 2026-10-15 18:14 +0700.

## PR #765 — merged issue 732

[Pull request](https://github.com/autonomous-ai/lampOS/pull/765)

Reviewer: merged #765 at 18:14 as `4ad0ec5c3`, completing the live consumer after #751; #732 stays open because its start/read/journal follow-up #793 is still unmerged.

## PR #765 — merged issue 752

[Pull request](https://github.com/autonomous-ai/lampOS/pull/765)

Reviewer: merged #765 at 18:14 as `4ad0ec5c3` — opt-in live progress decisions and fresh, restart-safe Voice news; #751 is already merged, and hello remains the separate follow-up agreed here.

## PR #765 — final rebase source verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/765)

Reviewer: Final rebase `b5917f9f5` is verified. Its sole patch is an exact range-diff match to reviewed `ea47efd01`; the dependency commits are gone because #751 is merged. Git object identities match for the complete crates tree, Cargo.toml, Cargo.lock and toolchain, so the completed source gates and independent regressions apply unchanged. The only inherited tree changes are CI configuration and its docs. No remaining review change is requested. The new exact-head ARM64 run is required separately before merge.

Checked at 2026-10-15 18:04 +0700.

## PR #822 — concurrent rule admission

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: [P2] Make the six-rule admission atomic (`lampOS/crates/voice/src/remember.rs:1158-1174`). The new limit first reads the scope, then performs remember in a separate socket operation. ToolDesk runs up to eight memory calls concurrently (`provider_worker.rs:1699-1709`). Against the real Days store with one shared DaysClient and five existing rules, eight simultaneous keep_rule calls all returned kept_rule=true; list_rules then contained 13 rules. The claimed six-rule bound can therefore be exceeded by ordinary parallel tool calls. Make count-and-insert one store transaction, or serialize the complete admission/write for the scope without exceeding the tool deadline, and add the concurrent real-store case. Reproduced at `8a4304da5`; the relevant code is unchanged in `37b52aab5`.

Checked at 2026-10-15 18:04 +0700.

## PR #789 — rebased full gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/789)

Reviewer: Full host validation for `417724d26` is complete. Fmt and Voice all-target clippy pass. The first full serial run reported 1,217 passed / 1 failed / 8 ignored. The sole failure was the existing threaded EarlyEnd test the_link_dropping_in_between_fails_the_request_unheard_and_the_next_is_answered: 595 ms at its 600 ms assertion. Its exact same-binary retry passes at 603 ms; the original failure is retained under the Queue's timed-fixture exception, with no asserted root cause. That gives 1,218 distinct passing Mac tests after the retry. All three added pause cases passed in the full run; production pause handling and the non-test Instant::now() boundary are preserved. Current-head ARM64 run 38722576777 is green. Only the 29-to-32 documentation count remains to change before merge.

Checked at 2026-10-15 18:02 +0700.

## PR #791 — new linux test skipped

[Pull request](https://github.com/autonomous-ai/lampOS/pull/791)

Reviewer: [P2] Execute the new Linux regression before claiming it was covered by ARM64 CI (`lampOS/crates/voice/tests/capture_hold_worker.rs:621`). Run 38722569693 is green, but its actual command uses --profile pr; the inherited default filter excludes the entire capture_hold_worker binary. The downloaded logs contain no execution of five_stalls_in_a_row_each_reopen_the_microphone_while_the_reference_goes_on. The Mac gate cannot execute that Linux-only target either. Arrange a native full-profile or explicit target run with devops and retain its per-test result, alongside fixing the controlled reference-gap coverage above. The PR comment that “ARM64 CI runs it” is not supported by this run. Green compilation and the reported SIGSTOP lamp exercise are separate evidence.

Checked at 2026-10-15 18:02 +0700.

## PR #765 — rebased host gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/765)

Reviewer: Fresh host validation at `ea47efd01` is complete. Workspace fmt and Msgs/Tasks/Policy/Voice all-target clippy pass. The first full serial no-fail-fast run reported 1,559 passed / 2 failed / 10 ignored: Msgs 83 and Tasks 118 passed; Voice passed all 1,199 with 8 ignored; Policy had 159 passed / 2 failed / 2 ignored. The Policy failures were `the_lamps_name_heard_while_someone_spoke_is_joined_to_what_they_said` (old name-hearing context at policy.rs:728) and `bad_arguments_print_the_usage_and_end_with_status_2` (missing usage text at program.rs:448). Both exact same-binary isolated retries pass. A subsequent fresh full serial Policy run also passes all 161 with 2 ignored. The initial two failures are retained; this is not a clean first gate. All three independently recompiled consumer-restart proofs pass. Exact-head pull_request ARM64 run [38721851499](https://github.com/autonomous-ai/lampOS/actions/runs/38721851499) is green. No remaining code finding; the final rebase after #751 remains before merge.

Checked at 2026-10-15 18:00 +0700.

## PR #791 — controlled reference gap test

[Pull request](https://github.com/autonomous-ai/lampOS/pull/791)

Reviewer: [P2] Make the new reference-gap regression control the reference traffic during the gap (`lampOS/crates/voice/tests/capture_hold_worker.rs:627-629`; helper `:374-378`). The renamed five-stall test calls hold_up(400), which sleeps the thread owning the sole reference sender, and only then calls stream(). It therefore sends no reference during that 400 ms interval, despite the name/PR claim that reference continues through every gap. Resume scheduling may incidentally exercise the race, but the test does not reliably exercise the closed-microphone/reference branch changed in `00de8ec9e`. Keep reference traffic active while authority is withheld, wait for CaptureGap, deliver a valid-generation reference before renewing authority, and show that controlled case fails at `46cb96268` and passes with the fix. This is a regression-coverage finding, not a claim of a newly reproduced runtime fault; the reported five SIGSTOP lamp run remains separate evidence.

Checked at 2026-10-15 17:55 +0700.

## PR #824 — native result verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/824)

Reviewer: Completed-run evidence now verifies the parallel runtime statement. Exact-head pull_request run [38721013037](https://github.com/autonomous-ai/lampOS/actions/runs/38721013037) is green at `2797292ee`: the runner reports 4 cores; nextest reports 4,399 passed, 102 skipped, in 55.079 seconds. Test compilation took 5m29s and the separate release build 2m17s; those are outside the 55-second execution figure. Doc/custom-harness steps also succeeded. The earlier pending-runtime caveat is resolved. Devops has merged this PR; the nightly-full-run and failed-full-run merge-hold findings inherited from #821 remain separate.

Checked at 2026-10-15 17:54 +0700.

## PR #789 — pause fixture count

[Pull request](https://github.com/autonomous-ai/lampOS/pull/789)

Reviewer: [P3] Update the scenario count after the keeper-pause rebase (`lampOS/docs/voice-test-fixtures.md:21`; `lampOS/docs/pilot-faults.md:31`). At `417724d26`, pilot_faults.rs now has 32 stepped Script::question scenarios plus the unchanged threaded mute/reopen scenario; both docs still say 29 stepped scenarios. The three new pause tests account for the increase. Change the count to 32 or avoid a fixed count. The conflict resolution otherwise preserves the pause behavior, assertions and production clock boundary; the fresh full Voice gate is running.

Checked at 2026-10-15 17:54 +0700.

## PR #822 — lamp evidence followup

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: At `5c6c6dc17`, the delta adds lamp evidence only; runtime crates, Cargo files and toolchain are identical to reviewed `d381ef9c2`. Fmt/clippy pass, and all four independent regression checks still fail (the stalled bus case took 923 ms on this run). The completed identical-source full gate remains 1,416 passed / 1 failed / 13 ignored; the unchanged Memory failure was not rerun again.

The lamp record is now present, so the earlier absence of evidence is partially addressed. It documents two remaining acceptance failures: an implicit standing correction was acknowledged without keep_rule, and a request to forget the weather rule removed the newer addressing rule instead. Later-session unprompted obedience is still unshown. Fix those paths and provide the successful spoken cases; update the PR description, which still says no lamp run was shown. None of the four code findings is fixed in this evidence-only head.

Checked at 2026-10-15 17:51 +0700.

## PR #791 — 46cb fixes verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/791)

Reviewer: The four earlier code findings are resolved at `46cb96268`. All three independent coordinator regressions now pass (reopen between drains, capture expiry during output, and a short explicit XRun); the final per-frame maintain error enters gap recovery. An additional real-socket check passes for the new one-late/second-late rejection/fresh-reset behavior. Workspace fmt, Audio/Voice all-target clippy and the full serial gate pass: 1,350 tests, 0 failures, 10 ignored. Exact-head ARM64 run 38720361917 is green on all four shards.

The new head `00de8ec9e` and its reference-during-gap fix are under review now. The previous three-stall fixture proves expiry/reopen cycles but does not itself pause the capture thread while speaker-reference packets queue; it does not replace the reported SIGSTOP lamp evidence. The exact pause between try_read and final maintain is still not injected by a test. Reviewer: for the first builder: the new late-message allowance changes freshness acceptance on every Voice channel, and this default/privacy behavior remains your judgment even after host checks pass.

Checked at 2026-10-15 17:51 +0700.

## PR #795 — current pr ci green

[Pull request](https://github.com/autonomous-ai/lampOS/pull/795)

Reviewer: The exact-head pull_request ARM64 run [38712595748](https://github.com/autonomous-ai/lampOS/actions/runs/38712595748) is now green on all four shards at `4c154fee3`. The earlier failed push run is a separate run and was not rerun by this reviewer. The reviewed code/docs fixes and host gate remain as recorded above.

Reviewer: for the first builder: the remaining acceptance is the physical privacy-switch behavior with the lamp; this is still your merge under the privacy-path rule.

Checked at 2026-10-15 17:49 +0700.

## PR #765 — parent merged

[Pull request](https://github.com/autonomous-ai/lampOS/pull/765)

Reviewer: #751 is now merged into lamp-voice as `ecca16554` (its exact-head ARM64 and fresh Msgs/Tasks/Policy gates passed). Please rebase this dependent PR onto current lamp-voice as planned. For `ea47efd01`, the range-diff preserves the reviewed change, including #770's waiting() behavior; fmt and all-target clippy pass, and all three independent actual-Tracker/real-Bus consumer restart proofs pass after recompilation. The full serial gate is still running. I will verify the final rebase and its exact-head ARM64 before merging.

Checked at 2026-10-15 17:49 +0700.

## PR #793 — unread runtime test

[Pull request](https://github.com/autonomous-ai/lampOS/pull/793)

Reviewer: [P2] Update the existing runtime regression to account for the newly published unread state (`lampOS/crates/tasks/tests/runtime.rs:487-489`). The next status after task acceptance may now legitimately be unread:[id], tasks:[]; the test takes exactly one bus value and indexes tasks[0]. The full serial Tasks gate fails with index out of bounds, and selecting only this test fails again at the same line. Wait with a bounded event loop for that task's first read value while asserting the intervening unread state, instead of assuming the next publication is already read.

The original unread-versus-empty finding is fixed: independent real-service checks pass for both a newly accepted task with failed reads and a persisted task restarted before its first successful read; get_progress correctly returns not_read and recovery publishes the readable value. Fmt and Msgs/Tasks/Policy all-target clippy pass. Serial tests: Msgs 83 passed; Policy 161 passed / 2 ignored; Tasks 121 passed / 1 failed as above. The two findings in this follow-up remain open.

Checked at 2026-10-15 17:48 +0700.

## PR #793 — unread loses run reset

[Pull request](https://github.com/autonomous-ai/lampOS/pull/793)

Reviewer: [P2] Preserve each task's new-run reset until its first readable progress snapshot (`lampOS/crates/policy/src/progress.rs:516-523`, `:540-544`). At `ceffaa68e`, a new-run unread status sets self.run and judged=0 but retains the old run's last progress. If the first successful count-0 snapshot is missed, the next real read at count 1 enters status() with new_run=false; the lower-count branch overwrites judged with 1 and next() suppresses that new progress. An independent proof using actual Tracker output reproduces old-run count 2 -> new-run unread:[3] -> first observed read count 1: asked=false. The control without the unread placeholder correctly asks. Keep the per-task reset active while tracked.unread, or otherwise distinguish the first read of a new run from a legacy same-run counter reset; add this sequence to the regression.

Checked at 2026-10-15 17:48 +0700.

## PR #751 — merged issue

[Pull request](https://github.com/autonomous-ai/lampOS/pull/751)

Reviewer: merged #751 at 17:47 — opt-in task progress on the bus and Policy shadow decisions, with restart identity and cooldown fixes; #732 stays open for the remaining live progress speech in #752 / #765.

## PR #822 — host gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: Host gate completed for `d381ef9c2`: workspace fmt and all-target clippy for Msgs, Voice and Memory pass. Full serial no-fail-fast tests: Msgs 80 passed; Voice 1,187 passed / 8 ignored; Memory 149 passed / 1 failed / 5 ignored. Total 1,416 passed, 1 failed, 13 ignored. The sole failure is the inherited Memory binary test `a_killed_process_leaves_a_store_the_next_one_loads_and_a_socket_it_replaces`, whose log-count assertion sees (3,3) instead of (4,4); it also fails on its exact isolated retry. This is not recorded as a passing gate. The test and Memory production source are unchanged here. All nine added or extended rule/switch tests executed and passed, but the four independent regression proofs reported above still fail. Exact-head pull_request ARM64 run [38719168444](https://github.com/autonomous-ai/lampOS/actions/runs/38719168444) is green on all four shards.

Checked at 2026-10-15 17:42 +0700.

## PR #751 — rebase verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/751)

Reviewer: Rebase `16f08fa11` is verified: range-diff preserves the four reviewed patches, and the Msgs resolution keeps both attention and tasks exports. Freshly cleaned/rebuilt Msgs, Tasks and Policy pass workspace fmt, all-target clippy with -D warnings, and serial tests: 83 / 118 / 160 passed respectively, 0 failures, 2 Policy tests ignored. All nine independent restart/stale-answer checks also pass against this head. No remaining code finding; GitHub now reports MERGEABLE. Exact-head ARM64 run 38720578121 is still pending.

Checked at 2026-10-15 17:39 +0700.

## PR #824 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/824)

Reviewer: Reviewed `2797292ee`: the matrix size, partition denominator and job name agree for one PR/manual runner and four push runners, and doc/custom-harness tests still run once. Workspace fmt, YAML parsing, all seven shell-block syntax checks and diff whitespace checks pass. No Rust crate source changes here, so there is no changed-crate clippy/test gate. Current-head ARM64 run [38721013037](https://github.com/autonomous-ai/lampOS/actions/runs/38721013037) is still in the test step; the parallel test count and runtime are not yet verified. Please record the completed run before presenting “about a minute in parallel” as a measured result (`lampOS/docs/realtime-tests.md:15-18`), and remove the attribution footer from the PR description.

Reviewer: for the first builder: this changes CI behavior by default and belongs to the human devops team. Devops has now merged #821; its previously reported missing nightly full run and failed-full-run merge hold remain unchanged here (`.github/workflows/lampos-arm64.yml:33-40`, `:119-140`). This review of the one-runner delta does not resolve those two findings.

Checked at 2026-10-15 17:38 +0700.

## PR #789 — green needs rebase

[Pull request](https://github.com/autonomous-ai/lampOS/pull/789)

Reviewer: Current-head pull_request ARM64 run [38712405805](https://github.com/autonomous-ai/lampOS/actions/runs/38712405805) is now green on all four shards at `dbabac931`. The reviewed source, documentation correction and host gate have no remaining finding. GitHub now reports CONFLICTING/DIRTY against lamp-voice, however. Please rebase the reviewed test changes onto the current branch and obtain the new-head run; I will check the conflict resolution and any source delta. This remains Part of #739, which must stay open for the motor-fixture work.

Checked at 2026-10-15 17:37 +0700.

## PR #822 — blocking policy publication

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: [P2] Move optional bus publication off the bounded session-acquisition path (`lampOS/crates/voice/src/remember.rs:1168-1173`; `lampOS/crates/voice/src/rules.rs:344-365`). The 300 ms deadline bounds the memory socket reads but not the synchronous Bus::publisher_declared/publish_json filesystem work afterward. OUT.try_lock only avoids waiting for the mutex. With the real Days client and a temporary declaration FIFO held open for 800 ms, told_for_session(..., true) took 941 ms; this runs before provider connection in thread.rs. A blocked bus therefore delays voice readiness despite the documented budget. Use a bounded background/coalescing publisher and test a stalled publication without extending note/tool completion deadlines.

Reviewer: for the first builder: #813 explicitly requires spoken corrections on the builder's own lamp before lamp-4ace, and the PR says that demonstration has not happened. That evidence is still needed. The new message type alone is permitted by the amended reviewer role. Current-head ARM64 is green; local Msgs/Voice/Memory gates are still being collected. These four findings were independently reproduced against compiled d381ef9c2 code.

Checked at 2026-10-15 17:35 +0700.

## PR #822 — forgotten rule provenance

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: [P2] Preserve rule provenance and forget invalidation when a rule influences a journalled answer (`lampOS/crates/voice/src/remember.rs:1120-1148`, `:1168-1173`). The new sibling r- scope is read into provider context without joining the provenance/invalidation path. Real Days-store proof: keep the synthetic ORCHID addressing rule; open a session that receives it; finish an ordinary answer quoting it. That answer is persisted with append_linked source_ids=[]. After successful forget_rule, both the earlier derived answer and another answer from stale provider context are accepted and returned by ordinary recall. Thus the rule can return through the person's memory after deletion; the statement that sibling scopes keep rules out of recall does not cover derived exchanges. Add scope-aware provenance and deletion/inhibition for rule-derived context, or conservatively prevent such journalling. Verify that both already persisted derivatives and post-forget stale context cannot reintroduce the removed rule. This concerns persisted recall, not retracting words already sent to a provider.

Checked at 2026-10-15 17:35 +0700.

## PR #822 — invalid number deletes

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: [P2] Reject an explicitly invalid rule number instead of treating it as omission (`lampOS/crates/voice/src/rules.rs:152-159`). The as_u64/filter chain sends number=0, negative numbers, and non-integer values to forget_last. The real-store regression keeps one rule and calls forget_rule {number:0}; the result is forgotten=true and list_rules becomes empty. Only an absent number should select the most recent rule. Validate a present value and return an error without mutation; test absent, valid, zero, negative, and wrong-type cases.

Checked at 2026-10-15 17:35 +0700.

## PR #822 — older rules disappear

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: [P2] Keep the complete standing-rule state separate from the prompt byte budget (`lampOS/crates/voice/src/rules.rs:275`, `:332-338`). At `d381ef9c2`, the same newest-first 600-byte `kept` slice drives the session note, list_rules, and policy booleans/count. Against the real Days store, keeping "Never speak first" with never_first, followed by three valid 200-byte corrections, succeeds four times but then publishes quiet=false, quiet_mornings=false, rules=3; list_rules and the next session omit the unforgotten quiet rule. Adding an unrelated correction silently reverses an existing instruction. Retain/evaluate all active rules for policy and listing, and define an explicit bounded admission or summarization policy for session instructions instead of silently discarding older accepted rules. Add this real-store regression.

Checked at 2026-10-15 17:35 +0700.

## PR #810 — final host gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/810)

Reviewer: Host gate complete at `d91b5fb90`: `cargo fmt --all --check` and `cargo clippy -p lamp-voice --all-targets -- -D warnings` pass. The full serial no-fail-fast test run had 1,173 passed, 1 failed, 8 ignored. The failure was `coordinator::tests::pilot_faults::provider_failure_during_output_never_becomes_a_completed_answer` (capture discontinuity); its exact isolated retry passes. This is the recorded timing-family exception, not a clean first run: 1,174 distinct tests passed after that retry. Current-head pull_request ARM64 run [38714841106](https://github.com/autonomous-ai/lampOS/actions/runs/38714841106) has all four shards green. The four independently reproduced measurement findings above remain open; the first builder's lamp comparison remains required.

Checked at 2026-10-15 17:35 +0700.

## PR #821 — nightly full route

[Pull request](https://github.com/autonomous-ai/lampOS/pull/821)

Reviewer: [P2] Provide the required nightly full run and route non-PR validation to it (`.github/workflows/lampos-arm64.yml:25-33`, `:82`). The current expression selects default only for push; workflow_dispatch selects pr and excludes realtime tests too. There is no schedule or nightly dispatcher in this PR. Add an actual nightly route on lamp-voice and make it run the full/default profile, with the same failure reporting and merge hold.

Reviewer: for the first builder: your explicit do-not-merge instruction remains in force; CI ownership has also moved to devops. Current-head validation: YAML and TOML parse, all seven shell run blocks pass bash -n, and the three repeated filtersets agree. No Rust crate source changed, so no crate build/test was needed for this configuration review. The PR description also still contains a generated-with attribution footer; remove it to follow the no-attribution rule.

Checked at 2026-10-15 17:26 +0700.

## PR #821 — missing merge hold

[Pull request](https://github.com/autonomous-ai/lampOS/pull/821)

Reviewer: [P2] Complete the required hold after a failed full run before excluding tests from PRs (`.github/workflows/lampos-arm64.yml:113-131`). At `2a6cff170`, the failed-push job only opens/comments on an issue. If merge A breaks one of the excluded capture/mute/conversation tests, its later full run goes red, but B's reduced green PR run can still merge: no gate consumes that issue or failed default-branch result. The first builder's conditional fallback requires the red result to block the next merge. Implement and verify that hold, or keep the full PR test selection. The rule-file edits have correctly been removed from this head.

Checked at 2026-10-15 17:26 +0700.

## PR #751 — restart fixes verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/751)

Reviewer: Both additional restart findings are fixed at `1969921e3`. The original four independent checks and all five edge checks pass against the freshly rebuilt library: a lower first-observed count in a new run is judged; a four-second publisher outage followed by the normal read retains the cooldown at 34 seconds; old-run or unavailable-source replies remain outdated. Workspace fmt, rebuilt Policy all-target clippy and full serial Policy tests pass: 160 passed, 0 failed, 2 ignored, with both new regressions present. Msgs and Tasks trees are unchanged from their verified gates. No remaining code finding. GitHub currently reports this head CONFLICTING/DIRTY and attaches no checks; please rebase it onto current lamp-voice so the exact-head ARM64 gate can run. #765 follows this merge and will need its own rebase/gate.

Checked at 2026-10-15 17:26 +0700.

## PR #807 — merged issue

[Pull request](https://github.com/autonomous-ai/lampOS/pull/807)

Reviewer: merged #807 at 2026-10-15 17:24 +0700 — Voice and Expression honour keeper pause/resume, with host tests and the recorded lamp-52e6 conversation check.

## PR #810 — timeout counting

[Pull request](https://github.com/autonomous-ai/lampOS/pull/810)

Reviewer: [P2] Count timed-out or closed sessions as failures in the timing summary (`lampOS/crates/voice/src/main.rs:138-145`; `crates/voice/src/tool_turns.rs:231-235`). A silent 14-second timeout becomes a normal Timeline.result row, while summary excludes only rows with a failed field. The independent timeout fixture reports failed=0, plain.n=1 and plain_fact.as_expected=1 despite receiving no answer. Connection-ended has the same route. Record these terminal failures explicitly, keep their counts/reasons, and exclude incomplete samples from successful-run aggregates. Add silent-timeout and early-connection-end cases beside the failed-connect regression.

Checked at 2026-10-15 17:23 +0700.

## PR #810 — send boundary

[Pull request](https://github.com/autonomous-ai/lampOS/pull/810)

Reviewer: [P2] Timestamp the tool response before blocking journal output (`lampOS/crates/voice/src/provider_worker.rs:1725-1743`, `:1769`). answer_with sends the response, calls eprintln!, then line() obtains sent_us. A focused harness using the unchanged method bodies holds stderr's lock for 100 ms after the send callback: the callback runs at 3 us, but recorded sent_us is 105,617 us. That adds 105,614 us of journal delay to the measured tool-response latency and subtracts it from the inferred model delay. Capture the send timestamp immediately after the callback, before logging, and pass it into the record. Add a blocked-stderr regression; keep this measurement off the slow logging boundary.

Checked at 2026-10-15 17:23 +0700.

## PR #810 — lead in latency

[Pull request](https://github.com/autonomous-ai/lampOS/pull/810)

Reviewer: [P2] Do not report the lead-in as post-answer audio latency (`lampOS/crates/voice/src/turn_times.rs:267`). The summary subtracts answer_sent_ms from the first audio of the entire turn. With the behavior this PR introduces—first audio at 900 ms, function call at 1,100 ms, answer sent at 1,104 ms—the independent check reports first_audio_after_answer_ms=-204. That is not the second generation's delay. Use an actual first-audio-after-answer boundary when available; otherwise report that measurement as unavailable for lead-in turns, and keep the valid first-sound measurement separately. Cover a lead-in followed by a function answer.

Checked at 2026-10-15 17:23 +0700.

## PR #810 — rotated record order

[Pull request](https://github.com/autonomous-ai/lampOS/pull/810)

Reviewer: [P2] Make turn reconstruction independent of rotated-file discovery order (`lampOS/crates/voice/src/turn_times.rs:146-158`). Audio, speaker and outcome records are retained only if local_endpoint has already created that turn. `lines_under` uses read_dir order, and the service rotates numbered slots: an independent real-files fixture reading generation 4 in events-0 before generation 3 in events-3 loses all three fields. The same records in chronological order yield audio 1,000 ms, sound 1,200 ms, completed and plain.n=1; the rotated order yields nulls and plain.n=0. Collect per-turn fields in two passes or order by actual generation/time, not slot filename, and add a rollover regression.

Checked at 2026-10-15 17:23 +0700.

## PR #765 — incarnation verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/765)

Reviewer: The Voice restart-identity finding is fixed at `88dce47f8`. Three independent checks use actual Tracker output and the real Bus: repeated change 1 in a new run gets a distinct persisted key and is offered; fetched words from another run are refused; a queued old-run note is refused at final tell. Workspace fmt and Msgs/Tasks/Policy/Voice all-target clippy pass. Current Policy serial tests pass: 159, with 2 ignored and the acting regression present. Voice reports 1,175 passed, 1 failed, 8 ignored; the sole unchanged motor-child timeout passes on its exact isolated retry. Msgs (80) and Tasks (118) evidence is reused only after exact source-tree equality with the gated #751 dependency. No remaining Voice finding. #751 must be fixed and merged first, then this branch rebased and its exact-head ARM64 run green; this head currently has no attached ARM64 run.

Checked at 2026-10-15 17:19 +0700.

## PR #810 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/810)

Reviewer: for the first builder: this explicitly requests your silent comparisons and played lead-in measurements on lamp-4ace, and the new voice_tool_call records affect its already-enabled LAMP_FIRST_OUTPUT path. Acceptance and merge remain yours. At `d91b5fb90`, workspace fmt and Voice all-target clippy pass; the full serial gate is running. The switch-off function/next-text comparisons preserve the existing task prompts, and the lead-in is within #799's approved scope. I am checking the new measurement readers and send-time boundaries with independent cases before giving a final code conclusion. #799 remains open until the fix is measured on the lamp.

Checked at 2026-10-15 17:19 +0700.

## PR #816 — integration review complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/816)

Reviewer: Integration review complete at `dd5376453`; no concrete code finding. In particular, Keeper's normal `--act` path only restarts already-active units, so it does not start the inactive lamp parts held back by the setup marker. Keeper's required gate passes (23 tests); the dependent Body clippy/test check also passes (291 tests). One integration limit for the final setup wiring: the existing updater waits up to 45 seconds for ready:true, while marker-absent setup deliberately stays not ready through a human setup flow. That transaction behavior still needs to be reconciled/tested when the marker-enabled package is wired into installation; this PR does not install that switched unit. ARM64 and the first-builder default-change decision remain outstanding.

Checked at 2026-10-15 17:19 +0700.

## PR #778 — docs verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/778)

Reviewer: Verified `983a87b60`: only `docs/second-voice.md` changes from `811a09edc`. It now accurately records the eight answered lamp-52e6 questions, separates the host-only switching/return coverage, and consistently states the 30-second input queue. The separate 15-second STT finish timeout is correctly retained. All reported code and documentation findings are resolved; the completed 1,224-test host gate applies to the identical source. Exact-head ARM64 and the reserved first-builder acceptance remain required.

Checked at 2026-10-15 17:19 +0700.

## PR #816 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/816)

Reviewer: for the first builder: the updater-installed command link is installed/replaced unconditionally (`lampOS/crates/keeper/src/main.rs:222`), and the author explicitly reserves this default change for your merge. I have read the PR, diff, task, scope and latest comment. At `dd5376453`, workspace fmt, Keeper all-target clippy and full serial tests pass: 23 passed, 0 failed or ignored. The marker-present equivalence, absent→present transition and atomic-link tests exercise their claims. I am finishing the setup/readiness integration review; full marker-absent boot and setup on a lamp have not been demonstrated, and ARM64 is still running. #806 remains open because this is only part of it.

Checked at 2026-10-15 17:15 +0700.

## PR #778 — pending note fix gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/778)

Reviewer: The pending-note privacy finding is fixed at `811a09edc`. Nine independent checks pass: cancellation/person switch, pending-note forget, failed and silently declined Written turns, wrong-person answers, completed written history, the real forget path, and Written-confirm refusal followed by allowed spoken confirmation. Workspace fmt, Voice all-target clippy and the full serial Voice gate all pass without retries: 1,224 passed, 0 failed, 8 ignored. No remaining code finding from this review. The earlier P3 doc correction is still outstanding (`docs/second-voice.md:6-8`, `:143`, `:376-377`): describe the recorded lamp-52e6 scope and use the actual 30-second audio queue, rather than no lamp run and 15 seconds. Exact-head ARM64 and the first builder's lamp acceptance remain outstanding.

Checked at 2026-10-15 17:15 +0700.

## PR #791 — post frame maintain

[Pull request](https://github.com/autonomous-ai/lampOS/pull/791)

Reviewer: [P2] Apply recoverable-stall handling to the final per-frame authority check too (`lampOS/crates/voice/src/audio_workers.rs:702`). The top-of-loop maintain, install and try_read calls now catch recoverable discontinuities, but after DSP/VAD the loop still calls `microphone.maintain()?` directly. If a 400 ms worker pause lands after try_read returned and before this check, maintain closes the expired microphone and returns Discontinuity(AuthorityExpired); this path immediately ends capture instead of reporting a gap and reopening. Drop that old frame and enter the same bounded recovery path, retaining the permission check before any audio is sent. Add a pause injected between the read and this final check, including after one successful reopen. This is a source-audited fatal path; I have not established which worker instruction caused the first builder's second-pause failure.

Checked at 2026-10-15 17:11 +0700.

## PR #751 — publisher loss cooldown

[Pull request](https://github.com/autonomous-ai/lampOS/pull/751)

Reviewer: [P2] Preserve the task's cooldown and told history when its publisher disappears (`lampOS/crates/policy/src/progress.rs:543-545`; caller `crates/policy/src/policy.rs:1427-1430`). The new docs (`docs/policy.md:989-994`) promise that the quarter-hour hold survives a Tasks restart, but the normal SourceGone path calls gone(), which clears every Tracked value including told_at and notes. With actual Tracker snapshots: accept one say_now, observe publisher loss/restart of the same store, leave Tasks unavailable for 4 seconds (past the bus's 3-second lease), then receive its next ordinary changed-line read 34 seconds after the first note; Policy asks again and held_recent remains 0. Keep the per-store/task cooldown/history separately from unavailable live progress, while still rejecting old-run answers. Add a restart regression that includes SourceGone instead of calling status(new_run) directly.

At `2ec94dcf7`, workspace fmt and Msgs/Tasks/Policy all-target clippy pass. Full serial tests pass: Msgs 80, Tasks 118, Policy 158, 2 ignored, no failures. These additional restart cases remain blockers despite the clean gate.

Checked at 2026-10-15 17:08 +0700.

## PR #790 — merged issue

[Pull request](https://github.com/autonomous-ai/lampOS/pull/790)

Reviewer: merged #790 at 2026-10-15 17:08 +0700 — opt-in hand slumps rest 5 seconds; overloads keep 120 seconds. #703 remains open for its other work.

## PR #807 — lamp docs verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/807)

Reviewer: The verification-doc finding is fixed at `d1655b09e`. The only change since `0282c27f9` is `docs/keeper.md`: it links the recorded lamp-52e6 pause/resume run and accurately retains the untested Expression, unasked-turn and stop-lamp cases. The source remains identical to the completed host gate (Expression 230 and Voice 1,168 passed, 8 ignored, no failures). No remaining finding from this review; the new exact-head ARM64 run is required before merge.

Checked at 2026-10-15 17:06 +0700.

## PR #791 — output stall proof

[Pull request](https://github.com/autonomous-ai/lampOS/pull/791)

Reviewer: [P2] The output-active case also fails without any CaptureGap event (`lampOS/crates/voice/src/coordinator.rs:2712-2716`). A stronger independent fixture pauses capture for 500 ms after the speaker accepts 3 of 40 answer chunks. Because admission is already idle, gap_cut never cancels the active reply; its input lease expires and Voice exits with Err(StaleOwner), rather than recovering for a later question. The fixture passes the ownership/privacy invariants first and then fails the service-continuation assertion. This confirms the earlier output finding is not dependent on the timing of a simulated close notification. Include active reply/output in the recoverable expiry handling, as well as handling the explicit short-XRun close before resumed input. The final three focused regressions all fail for their intended production errors; the unmodified gate and ARM64 run remain green.

Checked at 2026-10-15 17:05 +0700.

## PR #751 — new run lower count

[Pull request](https://github.com/autonomous-ai/lampOS/pull/751)

Reviewer: [P2] Keep the new-run judging reset when its first observed count is lower (`lampOS/crates/policy/src/progress.rs:504-519`). At `2ec94dcf7`, a changed run resets judged to 0, but the older count-decrease branch immediately overwrites it with the incoming count. An independent check using real Tracker output reaches run A change 2, restarts, misses run B's initial snapshot, then receives B change 1: no question is asked, although the same-count restart regression promises that every new-run change is eligible. Skip the legacy count-decrease baseline for a distinct run, and cover old count 2 → new count 1 with the initial new-run snapshot missed. The original held-answer incarnation bug is fixed; the four earlier independent checks now pass.

Checked at 2026-10-15 17:05 +0700.

## PR #790 — rebase verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/790)

Reviewer: Verified rebase head `5583dd812`: Body source and its manifest/lockfile are unchanged from the reviewed `89f144b56`; the branch now contains only this follow-up's three commits after merged #780. I also reran the required gate on this head: workspace fmt, Body all-target clippy and full serial tests all pass, 291 passed, 0 failed or ignored. No remaining finding. Exact-head ARM64 `38715628914` is still running; #703 remains open for its other work.

Checked at 2026-10-15 17:03 +0700.

## PR #738 — continuation regressions verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/738)

Reviewer: The missing continuation coverage is fixed at `961c9ca33`. Both new production-path regressions pass: a four-piece utterance gives each successor exactly one note and plays only the final answer; the link loss after a continued request fails it unheard and leaves the unrelated post-reconnect question without a note. I read the exact wire, ownership and outcome assertions and the documented slow-service/reconnect limits. Workspace fmt and Gemini/Voice all-target clippy pass. The full serial command reports 1,423 passed, 1 failed, 10 ignored; only the existing rapid-mute/reopen timing fixture fails with capture discontinuity, and its exact isolated retry passes. All other targets ran, for 1,424 distinct passing tests after that retry. Exact-head ARM64 `38714631359` is green. No remaining requested change from this review; the first builder's reserved lamp judgment and #723's broader acceptance remain outstanding.

Checked at 2026-10-15 17:03 +0700.

## PR #791 — gap cancellation

[Pull request](https://github.com/autonomous-ai/lampOS/pull/791)

Reviewer: [P2] Cancel the affected turn for an explicit capture close, including during output and before a short reopen (`lampOS/crates/voice/src/coordinator.rs:2712-2716`, `:2854-2860`). The new cancellation condition requires active input and a gap of at least INPUT_LEASE_US; CaptureGap itself only updates closed_epoch. Two independent production-coordinator fixtures fail at `25b34758b`: a 400 ms CaptureReopen after three speaker chunks ends the service with `current reply rejected at speaker`, because admission is idle while answering; a 50 ms XRun reopen inside an admitted question ends with `accepted capture lineage changed`, because the new epoch arrives before the age-based cancellation. Both traces contain capture_gap_closed and no cancellation. Process the explicit close as a turn/lease boundary before resumed frames or more output, abandon the affected question/reply safely, and remain available for the next fresh question. Cover both output-active and sub-100 ms XRun cases.

The required unmodified gate passes: workspace fmt; Audio/Voice all-target clippy; full serial tests, 1,332 passed, 0 failed, 10 ignored. Exact-head ARM64 run `38714452377` is green. These additional interleaving regressions expose cases that the gate does not cover. Your lamp acceptance and merge remain with the first builder.

Checked at 2026-10-15 17:01 +0700.

## PR #791 — reopen control race

[Pull request](https://github.com/autonomous-ai/lampOS/pull/791)

Reviewer: [P2] Re-read capture lifecycle events before rejecting a frame from a reopened epoch (`lampOS/crates/voice/src/coordinator.rs:5548-5557`). At `25b34758b`, `validate_capture_frame` only drains control when the cached epoch is None. If the normal drain saw epoch 1, and the worker then queues Gap(1), Started(2) and frame(2) before the data receive, the cached epoch is still Some(1) and the legitimate frame ends the service with `microphone lineage changed during test`. An independent extension of the existing two-socket first-frame regression reproduces this deterministically. Drain pending lifecycle changes when the received epoch differs, then apply the resulting closed/open epoch checks; add the reopen-between-drains regression.

Checked at 2026-10-15 17:01 +0700.

## PR #791 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/791)

Reviewer: for the first builder: this changes default capture/voice behavior, and you have explicitly reserved the three capture-worker pauses and evening run on lamp-4ace. Your lamp acceptance and merge remain yours. I have read the PR, diff, issue and scope at `25b34758b`; workspace fmt and Audio/Voice all-target clippy pass. The full serial gate is running. I am also checking capture-reopen interleavings before giving a final code conclusion; exact-head ARM64 is pending.

Checked at 2026-10-15 16:53 +0700.

## PR #807 — lamp docs

[Pull request](https://github.com/autonomous-ai/lampOS/pull/807)

Reviewer: [P3] Update the earlier verification limitation now that the lamp run is recorded (`lampOS/docs/keeper.md:335-339`). It still says the pause was shown only by host tests and that a mid-conversation pause on lamp-52e6 is still to be shown. `0282c27f9` now records exactly that pause/resume demonstration. Link the new verification folder here and retain its actual remaining limitations (Expression had no request to suppress; unasked turns and stop lamp were not exercised). I verified that this update adds only the three evidence files; the completed host gate applies unchanged. The new journal/transcript evidence supports the reported Voice pause/resume result.

Checked at 2026-10-15 16:53 +0700.

## PR #797 — fixes verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/797)

Reviewer: Both calibration findings are fixed at `1c77f641a`. The independent whole-run measurement/apply check now rejects the original insufficient-margin case, and a sufficient swipe yields safe touch/release thresholds. Four independent regressions pass, including hidden-bit bounds and 6,151,950 accepted percentage/boundary combinations. Workspace fmt, Controls all-target clippy and full serial Controls tests pass: 382 passed, 0 failed. Voice source is byte-identical to `e4431d40a`, so its completed gate evidence applies. No remaining code finding; current-head ARM64 and the requested first-builder hardware acceptance remain outstanding.

Checked at 2026-10-15 16:53 +0700.

## PR #778 — pending written note

[Pull request](https://github.com/autonomous-ai/lampOS/pull/778)

Reviewer: [P1] Bind a pending written note to its request and person, and clear it on cancellation/forgetting (`lampOS/crates/voice/src/thread.rs:365-368`, `:415`, `:483-490`). At `47c7329e8`, the completed-written-history and heard-provenance fixes pass independent checks, including a Written confirm being refused and a later spoken confirm succeeding. But `Thread::written` stores one unkeyed `note`, which any next answer consumes; retirement only retires the Engine, and `forget()` leaves this note intact. Reproduction: A's Written request contains ORCHID; retire it before its chat starts; B's next spoken turn completes. B's following chat now contains A's ORCHID as a system note, attached to B's exchange. A separate pending-note → forget → B-answer case also preserves it. Store the request and person with pending context, consume it only for the matching answer, and discard it on retirement, failure and forgetting. Add both cancellation/person-switch and pending-note-forget regressions.

Checked at 2026-10-15 16:49 +0700.

## PR #792 — merged issue

[Pull request](https://github.com/autonomous-ai/lampOS/pull/792)

Reviewer: merged #792 at 2026-10-15 16:49 +0700 — opt-in attention/addressed signal and near/facing rule; lamp threshold recordings remain the first builder’s activation check.

## PR #807 — final host gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/807)

Reviewer: Host gate complete at `798425f39`: workspace fmt, Expression/Voice all-target clippy, and both full serial test suites pass without retries. Expression: 230 passed, 0 failed. Voice: 1,168 passed, 0 failed, 8 ignored. No behavior finding from this review. Current-head ARM64 and #777's requested keeper pause/resume demonstration on a lamp remain outstanding; the host fixtures do not establish that physical result.

Checked at 2026-10-15 16:46 +0700.

## PR #794 — final host gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/794)

Reviewer: No new finding at `09e53f99e`. Workspace fmt and Voice all-target clippy pass. The full serial Voice gate passes without retries: 1,166 passed, 0 failed, 8 ignored; all three new regressions pass. Exact-head ARM64 run `38703309129` is green. Default speech behavior and the requested story/follow-up lamp acceptance remain with the first builder.

Checked at 2026-10-15 16:46 +0700.

## PR #793 — unread watched tasks

[Pull request](https://github.com/autonomous-ai/lampOS/pull/793)

Reviewer: [P2] Distinguish an unread watch from an empty watch (`lampOS/crates/tasks/src/progress.rs:298-302`, `:311-319`, `:394-399`). The new contract (`crates/msgs/src/tasks.rs:88-91`, `docs/tasks.md:901-907`) says `tasks:[]` means nothing is being worked on. A real `serve_with_adapter` restart of the persisted store contradicts that: an accepted, unsettled task remains watched, but a timeout on the first `ps` leaves the published status empty and `get_progress` returns not_found. It stays that way through subsequent read failures. `watching()` only removes already-read entries; it never represents known watched tasks awaiting their first successful read. Represent unread/unavailable progress separately from an empty watched set, and add restart/failed-first-read and newly-watched/failed-read regressions.

At `81d645b72`, workspace fmt and Msgs/Tasks/Policy clippy pass; serial tests pass (80, 120 and 156 respectively, 2 Policy ignores). Exact-head ARM64 run `38707045219` is green. #751 remains an unresolved dependency. Reviewer: for the first builder: this PR requests your lamp check and changes the already-enabled progress behavior on lamp-4ace.

Checked at 2026-10-15 16:46 +0700.

## PR #780 — merged issue

[Pull request](https://github.com/autonomous-ai/lampOS/pull/780)

Reviewer: merged #780 at 2026-10-15 16:43 +0700 — opt-in whole-lamp hand slump; #703 remains open for its remaining work.

## PR #797 — final host gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/797)

Reviewer: Local gate at `e4431d40a` is complete. Workspace fmt and Controls/Voice all-target clippy pass. Controls serial tests report 380 passed/1 failed; Voice reports 1,162 passed/1 failed/8 ignored. Both failures are unchanged fixtures and pass individually: Controls' empty-stderr replay case and Voice's ready-ring startup case. Four base-versus-head replay comparisons (legacy/touch, shadow/enabled) produce byte-identical stdout and stderr. Exact-head ARM64 run `38704582746` is green. The two calibration findings remain: a full production `measure()`/`Report.apply()` fixture also reproduces the unsafe quiet thresholds, beyond the direct threshold-function check. No hardware behavior is claimed; your requested lamp-4ace acceptance stays with the first builder.

Checked at 2026-10-15 16:37 +0700.

## PR #789 — final host gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/789)

Reviewer: Host review is complete for the source tree shared by `73b50b4b8` and documentation-only `dbabac931`. Workspace fmt and Voice all-target clippy pass. The serial Voice gate returned 101 with 1,001 passed, 1 failed, 8 ignored: only `coordinator::motor::tests::pending_inspection_is_revoked_before_worker_data_and_release_is_separate` timed out in the motor preflight. It passed on its exact retry. All 193 remaining binary/integration tests and doc targets pass, for 1,195 distinct passing tests after that retry. The privacy-fixture cases pass; the motor timing work remains the separate #739 part described here. The initial sandbox-denied attempt is retained separately and is not treated as code evidence. No remaining finding from this review; current-head ARM64 is still required.

Checked at 2026-10-15 16:37 +0700.

## PR #778 — written confirmation

[Pull request](https://github.com/autonomous-ai/lampOS/pull/778)

Reviewer: [P1] Preserve whether a tool call came from heard speech or an autonomous Written turn (`lampOS/crates/voice/src/second_provider/worker.rs:269-272`). The runner always passes `heard=true` to `tools::answer`, but Written is also accepted by this route. `agent_tasks.rs:497-498` relies on that flag to require a person's later reply before confirming prepared work. Reproduction at `91e574057`, using the real Engine and tool handlers with local task sockets: prepare task 7 during heard request 7; on autonomous Written request 8, have the model call `confirm_agent_task`. The fallback returns `handing_over:true` and sends `op:confirm` to `voice.sock` although no person replied. The same call with `heard=false` is correctly refused without confirm IPC. The Live route explicitly marks Written and derives `heard=false` (`provider_worker.rs:1667-1671`, `:2620-2623`). Carry request provenance through the fallback's tool runner, pass false for Written, and cover both denied autonomous confirmation and allowed later spoken confirmation.

Checked at 2026-10-15 16:37 +0700.

## PR #778 — written history

[Pull request](https://github.com/autonomous-ai/lampOS/pull/778)

Reviewer: [P2] Preserve written-turn input context when replacing the engine cache (`lampOS/crates/voice/src/second_provider.rs:223-234`, `lampOS/crates/voice/src/second_provider/worker.rs:343-354` and `:370-376`). Both privacy regressions now pass at `91e574057`: A → B → A stays person-scoped, and actual forget followed by B → A does not restore forgotten content. However, Written calls `answer` directly without producing an input Transcript, while the canonical thread obtains its input only from those Transcript events. The old engine cache retained that written input. Reproduction: complete a Written turn containing ORCHID, then request a follow-up in the same person's session without forgetting. The next chat contains an empty user message and the previous assistant answer, with no ORCHID context anywhere. Preserve written-input context in the canonical, person-scoped history, without representing an internal note as words a person spoke, and add a written-turn/follow-up regression alongside the privacy cases.

Checked at 2026-10-15 16:32 +0700.

## PR #794 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/794)

Reviewer: for the first builder: this changes the default task-news and `check_agent_task` wording, and the builder explicitly requests the lamp-4ace story/follow-up replay. It therefore remains your merge under the reviewer rule. Head `09e53f99e` has green ARM64 run `38703309129`; independent code review and local gates are underway. The issue #757 should close only after the fix is merged and its requested behavior is accepted.

Checked at 2026-10-15 16:29 +0700.

## PR #797 — threshold rounding

[Pull request](https://github.com/autonomous-ai/lampOS/pull/797)

Reviewer: [P2] Keep the saved touch threshold strictly below the weakest accepted swipe (`lampOS/crates/controls/src/touch_calibrate.rs:209-211`). The accepted `--touch-percent 90` with noise 2 and weakest swipe 7 returns touch 7/release 3. MPR121 requires delta strictly greater than the touch threshold ([NXP AN3892, page 6](https://www.nxp.com/docs/en/application-note/AN3892.pdf)), so that weakest swipe no longer activates the saved profile when the hidden baseline bits are zero. The actual library reproduces this result. Round down or cap below the weakest swipe while preserving the noise/release margins, reject an impossible interval, and cover every accepted percentage rather than only 40/50.

Checked at 2026-10-15 16:29 +0700.

## PR #797 — baseline low bits

[Pull request](https://github.com/autonomous-ai/lampOS/pull/797)

Reviewer: [P2] Include the baseline's unread low bits in the quiet margin (`lampOS/crates/controls/src/touch_calibrate.rs:279-282` and `:203-212`). MPR121 exposes only the upper eight bits of its internal ten-bit baseline ([NXP AN3892, pages 5–6](https://www.nxp.com/docs/en/application-note/AN3892.pdf)). With internal baseline 515 and quiet filtered data 512, this code reads baseline 512 and reports noise 0 although the chip compares a delta of 3. A weakest observed swipe of 5 is accepted; the default rule saves touch 2/release 1. The measured quiet state can therefore trigger touch and cannot release it. An independent test against `e4431d40a` reproduces those exact thresholds. Include the quantization uncertainty in the quiet upper bound, require the release threshold above that bound, and reject calibration when there is insufficient remaining margin; test nonzero hidden baseline bits.

Reviewer: for the first builder: the PR explicitly requests your lamp-4ace calibration and ten-minute watch, and reports no lamp/MPR121 run. Host gates are underway; exact-head ARM64 is green.

Checked at 2026-10-15 16:29 +0700.

## PR #807 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/807)

Reviewer: for the first builder: #777's required pause/resume demonstration on a keeper-enabled lamp remains unshown, as this PR and `lampOS/docs/keeper.md:335-339` state. Host tests cannot establish that physical result. I have read the description, full diff, issue and scope: the new candidate gate preserves an accepted turn, all five unasked-turn start sites use the pause predicate, and Expression drops both outgoing requests before publishing. No concrete behavior defect found in that review. Formatting passes at `798425f39`; clippy/full serial tests and exact-head ARM64 are still outstanding. The missing lamp demonstration must be supplied before this switched behavior can satisfy the reviewer merge rule.

Checked at 2026-10-15 16:26 +0700.

## PR #795 — docs fixed

[Pull request](https://github.com/autonomous-ai/lampOS/pull/795)

Reviewer: Verified `4c154fee3`: only `service-privacy.md` and `ring-choreography.md` changed. They now distinguish no-child startup failures from failures after a writer starts; the documentation finding is fixed. The previously reported source gates apply unchanged. Current-head ARM64 and the first builder's privacy/lamp acceptance remain outstanding.

Checked at 2026-10-15 16:26 +0700.

## PR #789 — docs fixed

[Pull request](https://github.com/autonomous-ai/lampOS/pull/789)

Reviewer: Verified `dbabac931`: the only tree change from reviewed `73b50b4b8` is the verification README. It now correctly records six clean runs and four failures, so the documentation finding is fixed. The source gate in progress still applies to this identical source tree; current-head ARM64 is required separately.

Checked at 2026-10-15 16:26 +0700.

## PR #765 — fixed three latest gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/765)

Reviewer: At `ec4bcd6eb`, the original three consumer findings are fixed and all six new regression tests pass. Workspace fmt and clippy pass. The full serial Msgs/Tasks/Policy/Voice gate returned 101: 1,528 passed, 2 failed, 10 ignored across 55 suites. Both failures now pass individually: Policy's usage/exit test, and Voice's provider-process-loss privacy fixture (the full run observed a 51,463 µs gap; the retry observed 8,269 µs). The persisted restart-key finding remains at `lampOS/crates/voice/src/task_news.rs:570` and `:749`; your proposed run identity addresses the missing dimension, and I will verify the resulting commits on #751 and here. Current-head ARM64 remains pending.

Checked at 2026-10-15 16:26 +0700.

## PR #798 — final gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/798)

Reviewer: No finding at `e9516dadb`. `cargo fmt --all --check`, `cargo clippy -p lamp-voice --all-targets -- -D warnings`, and `cargo test -p lamp-voice -- --test-threads=1` all exit 0: 1,166 passed, 0 failed, 8 ignored, with no retries. The assigned issue #770 authorizes the Voice wording change, and the docs match it. Exact-head ARM64 run `38705400331` remains pending. Reviewer: for the first builder: this changes default wording, so the merge remains yours; your comment accepts the host proof for now.

Checked at 2026-10-15 16:21 +0700.

## PR #796 — final gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/796)

Reviewer: Final independent host evidence for merged head `93c5504d2`: workspace fmt and Audio/Voice clippy pass. Audio's serial tests pass (159 passed, 2 ignored). Voice's first serial run exits 101 with one unchanged reminder-consumer AckConfirmed failure; that exact test passes individually, and all remaining integration targets pass. Voice totals 1,164 distinct passing tests and 8 ignored after that retry. ARM64 run `38704325132` is green on this head. No additional finding. The first builder's five-start lamp evidence and merge are recorded above; #762 correctly remains open.

Checked at 2026-10-15 16:21 +0700.

## PR #795 — docs startup failure

[Pull request](https://github.com/autonomous-ai/lampOS/pull/795)

Reviewer: [P3] Qualify the indication-failure contract (`lampOS/docs/service-privacy.md:33-35`). It still says startup/worker indication failures remain errors even after reaping and cannot become a successful unmute. The fix at `1c00814d5` intentionally allows a bind/spawn failure before any writer starts to be diagnostic only, then continues without an indicator. State that distinction: a failed started writer must satisfy the verified destruction contract; a startup failure with no child does not block fresh unmute.

The original no-child optional-ring finding is fixed; both regressions pass. Six-crate clippy passes, and the latest full Voice suite exits 0 with 1,177 passed, 0 failed, 8 ignored. Other touched crates are unchanged from the completed gates; Controls' sole usage-output failure passed individually. Workspace formatting still fails at the inherited `voice/src/agent_tasks.rs:396`. Exact-head ARM64 remains pending. Reviewer: for the first builder: privacy-path and lamp acceptance remain yours.

Checked at 2026-10-15 16:21 +0700.

## PR #792 — docs fixed gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/792)

Reviewer: The `LAMP_BUS=on` prerequisite is now documented in all four relevant places at `1b8f585a2`; that finding is fixed. The delta from tested `c4b68ea58` is documentation only. Msgs/Voice clippy passed; their full serial gate returned 101 with 1,241 passed, 1 failed, 8 ignored. The unchanged reminder-roundtrip AckConfirmed test passed individually; it is recorded as the Queue's timed-test flake, not silently counted as a clean first run. `cargo fmt --all --check` still returns 1 solely at inherited `crates/voice/src/agent_tasks.rs:396`; the base version produces the identical formatting diff and all changed Rust files pass formatting. No remaining behavior finding; current-head ARM64 is pending. The first builder's revised role assigns this additive, switched topic to the reviewer.

Checked at 2026-10-15 16:21 +0700.

## PR #790 — final fixes

[Pull request](https://github.com/autonomous-ai/lampOS/pull/790)

Reviewer: All reported findings are fixed at `89f144b56`, including the command's help text: hand slumps rest for 5 seconds, overloads for 120. I independently ran `cargo fmt --all --check`, `cargo clippy -p lamp-body --all-targets -- -D warnings`, and `cargo test -p lamp-body -- --test-threads=1`: all exit 0, 291 tests passed, none failed or ignored. No remaining finding from this review. Under the amended reviewer role this is assigned to me, after #780 merges and this exact head's ARM64 run is green. #703 remains open because this is part of it.

Checked at 2026-10-15 16:21 +0700.

## PR #789 — loaded run count

[Pull request](https://github.com/autonomous-ai/lampOS/pull/789)

Reviewer: [P3] Correct the full-run summary (`lampOS/docs/verification/voice-fixtures-loaded-eighth-20261015/README.md:71`). It says none of the ten runs was clean, but the table and retained logs show six clean runs (2, 3, 4, 6, 9, 10), each with 1,181 passed, 0 failed and 8 ignored; four runs failed. Say that the requested ten clean runs were not achieved, with six passing and four failing. This preserves the limitation without contradicting the evidence.

At head `73b50b4b8`, the fixture audit found no weakened behavioral assertions; formatting and Voice clippy pass. The full serial Voice suite is running, and current-head ARM64 is still pending.

Checked at 2026-10-15 16:18 +0700.

## PR #780 — fixes verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/780)

Reviewer: All three findings are fixed and verified at `4f19f418d497aa7efdd30d18d19e881566394a2a`: only overloads update the repeat-overload timestamp, the limp-off test preserves give-way and checks the emitted writes, and the verification note is anonymous English. `cargo fmt --all --check`, `cargo clippy -p lamp-body --all-targets -- -D warnings`, and `cargo test -p lamp-body -- --test-threads=1` all pass: 291 passed / 0 failed / 0 ignored. I have read the narrowed reviewer rule at `d731283a5`; this opt-in behavior shown on lamp-52e6 is now assigned to the reviewer. Its new-head ARM64 runs are still queued, so it is not merged yet. #703 stays open because this is Part of it.

Checked at 2026-10-15 16:02 +0700.

## PR #765 — restart key

[Pull request](https://github.com/autonomous-ai/lampOS/pull/765)

Reviewer: [P2] Give progress announcements an identity that survives a Tasks restart without collisions (`lampOS/crates/voice/src/task_news.rs:513` and `:678-683`). The persisted told key is only store/task/change, but Tasks recreates `Tracker` on restart (`tasks/runtime.rs:371`), resets its counter to zero, and retains the same persisted store/task IDs. A real-Tracker/real-Bus reproduction at `93f6a771b` creates two different progress lines at change 1 on opposite sides of a restart. With the first generation's `store:3:progress-1` already told, the second line is silently discarded before fetching (`get_progress` requests: 0). Announcements remain suppressed until their new counters exceed the previously told keys. Preserve a non-resetting revision, or carry a progress incarnation consistently through status, decision, fetched progress and the persisted told key. Add the restart case with different content at the same count; it is the consumer-side counterpart of #751's pending-reply collision.

Checked at 2026-10-15 16:02 +0700.

## PR #790 — help remains

[Pull request](https://github.com/autonomous-ai/lampOS/pull/790)

Reviewer: The overload timestamp, give-way comparison and anonymous verification-note fixes are present in `8b2e94638`. The separate help finding remains: `lampOS/crates/body/src/bin/lamp-body.rs:9` still says a hand-induced slump is held again 120 seconds later. Please change that hand case to five seconds, keeping the overload case at 120 seconds. The new-head local gate is running; #780 must precede this PR.

Checked at 2026-10-15 15:58 +0700.

## PR #778 — forget history

[Pull request](https://github.com/autonomous-ai/lampOS/pull/778)

Reviewer: [P1] Invalidate the second route's history when memory is forgotten (`lampOS/crates/voice/src/second_provider.rs:373-377` and `:480`). The original cross-person finding is fixed at `fb3b0cc37`: my independent A-to-B request check now passes. A separate session transition still restores forgotten content. Reproduction against this head: finish A's exchange; call the actual `remember::run_for(forget)` with a successful removed-item response; verify `{forgotten:true}` and that the canonical Thread is empty; open B's session, then return to A. A's next chat request contains the forgotten exchange from `Shared.history`, which only filters by person and five-minute age. `remember.rs:217-220` explicitly clears the thread so forgotten material is not told to the next session, and #754's first-builder conditions require that behavior. Clear or version the second-route cache with the same forget invalidation, including in-flight results, and add an A → forget → B → A regression. This concerns a new session after forgetting, not ordinary retention within the current conversation.

Checked at 2026-10-15 15:58 +0700.

## PR #751 — restart generation

[Pull request](https://github.com/autonomous-ai/lampOS/pull/751)

Reviewer: [P2] Invalidate pending decisions when the progress publisher restarts (`lampOS/crates/policy/src/progress.rs:581-585`). Both original reported transitions are fixed at `306dc048d`, and their independent regressions now pass. However, `current()` still treats equal store/task/change values as the same progress across a restart. This is reachable with the real producer: Tasks creates a fresh `Tracker` on restart while its persisted store/task IDs survive. Two real `Tracker::read/take` instances produce change 1 for "Tests pass", then after restart change 0 and, on the next normal 30-second read, change 1 for "Found a regression". With a held reply and the supported 60-second judge deadline, Policy accepts the old yes as `say_now` for the reused count and gives the new generation a 15-minute cooldown. My producer-backed regression fails at this head while the three original checks pass. Capture and validate a publisher/tracker incarnation with pending work, invalidating it on publisher loss/restart or counter reset, and test reset then catch-up to the same counter.

Checked at 2026-10-15 15:58 +0700.

## PR #765 — gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/765)

Reviewer: Local gate recorded at `93f6a771b68fe0bffe147d6f7b8477aeec9bb553`. Formatting and Clippy (`-p lamp-msgs -p lamp-tasks -p lamp-policy -p lamp-voice --all-targets -- -D warnings`) passed. Serial tests: Msgs 80 passed; Tasks 118 passed; Policy 154 passed / 1 failed / 2 ignored; Voice lib 958 passed / 15 failed / 8 ignored, with all 15 failures passing individually afterward and all 196 integration tests passing. Original failures remain recorded. Policy `bad_arguments_print_the_usage_and_end_with_status_2` still fails alone at `lampOS/crates/policy/tests/program.rs:448` on missing usage output. That test and its log relay are unchanged from the dependency, whose gate passed it, and direct invocation prints the usage; inherited stderr loss is suspected, but this reviewer gate is not fully green. The three posted progress findings and the first-builder contract/lamp check also remain open.

Checked at 2026-10-15 15:51 +0700.

## PR #738 — gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/738)

Reviewer: Local validation recorded at `21c6dd812e41dc3b88c5fef5d58a183f4f244faa`. `cargo fmt --all --check` and `cargo clippy -p lamp-gemini -p lamp-voice --all-targets -- -D warnings` passed. The serial test command initially exited 101: Gemini 256 passed / 2 ignored; Voice lib 943 passed / 11 failed / 8 ignored. All 11 failures passed when run individually, and the previously unreached Voice binary/integration targets then passed (193 tests); both doctest targets passed. The original failures are retained, including the motor/timing cases covered by #739. ARM64 run 38673872924 is green at this exact head. The two missing continuation cases and first-builder lamp judgment remain outstanding.

Checked at 2026-10-15 15:51 +0700.

## PR #795 — optional ring

[Pull request](https://github.com/autonomous-ai/lampOS/pull/795)

Reviewer: [P2] Keep a ring that never started outside the Voice failure path (`lampOS/crates/voice/src/service_privacy_controls.rs:161-164`). During a muted wait, a private-channel bind or optional-child spawn failure returns `RingStartup::Unavailable { worker: None, error }` (`process/ring_startup.rs:140-181`): no SPI writer was created. The privacy worker can continue to receive valid samples and reach a stable unmute, with cleanup succeeding, but line 163 still turns `Allowed` into an error; `coordinator/service.rs:23` then exits Voice. This conflicts with the optional-light isolation rule in `lampOS/AGENTS.md:41`. Keep a no-child startup failure diagnostic-only and resume without the ring. Preserve the existing verified-black/exit requirements for a writer that actually started, and add a no-child failure followed by stable-unmute regression.

Checked at 2026-10-15 15:45 +0700.

## PR #780 — gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/780)

Reviewer: Local gate complete at `7a89102a2b24b5beb29f6c71c340f5a7c0003b08`: `cargo fmt --all --check`, `cargo clippy -p lamp-body --all-targets -- -D warnings`, and `cargo test -p lamp-body -- --test-threads=1` all passed, with 290 passed / 0 failed / 0 ignored. ARM64 run 38700741466 is green at this exact head. The three posted findings still need changes; the lamp judgment and merge remain with the first builder, and #703 remains open because this is only part of it.

Checked at 2026-10-15 15:45 +0700.

## PR #738 — missing cases

[Pull request](https://github.com/autonomous-ai/lampOS/pull/738)

Reviewer: [P2] Add the two continuation regressions explicitly required by #723 step 2 (`lampOS/crates/voice/src/coordinator/tests/early_end.rs:740-748` and `:915-943`). The new note assertions cover one successor, while the dropped-link test uses two independent questions and asserts `given_up() == 0`; it never fails a connection during a labelled continuation. Add a four-piece utterance and a service failure between continuation pieces, asserting the exact note placement, suppression of abandoned answers, and no continuation note on an unrelated request after reconnect.

Checked at 2026-10-15 15:45 +0700.

## PR #792 — bus docs

[Pull request](https://github.com/autonomous-ai/lampOS/pull/792)

Reviewer: [P3] Include `LAMP_BUS=on` in the enable instructions (`lampOS/docs/attention.md:624-628` and `:750-754`). The new path opens its ears, eyes and publisher only through `BUS_ROOT` (`provider_worker.rs:859`, `:895`, `:906`), but the coordinator forwards `--bus-root` only when `LAMP_BUS=on` (`bus_out.rs:49-65`). With only the two documented switches, the judgement has unknown signs and `attention/addressed` is never published. State the bus prerequisite in the setup and verification instructions, or explicitly wire the bus root for this switch.

Checked at 2026-10-15 15:40 +0700.

## PR #790 — off comparison

[Pull request](https://github.com/autonomous-ai/lampOS/pull/790)

Reviewer: [P2] The switch-off comparison gap from #780 remains at `lampOS/crates/body/src/body/tests.rs:4252-4255`: the `limp` flag also disables `give_way`, so the test does not prove unchanged `--give-way return` behavior with limp unset. Keep give-way enabled in both cases and verify the off case's writes and stop outcome against the existing baseline.

Checked at 2026-10-15 15:40 +0700.

## PR #790 — help

[Pull request](https://github.com/autonomous-ai/lampOS/pull/790)

Reviewer: [P3] Update the user-facing recovery time (`lampOS/crates/body/src/bin/lamp-body.rs:9`). The help still says a hand-induced slump is held again after 120 seconds, while `HAND_REST` now makes that five seconds. State five seconds for the hand case and retain 120 seconds for an overload.

Checked at 2026-10-15 15:40 +0700.

## PR #790 — identities

[Pull request](https://github.com/autonomous-ai/lampOS/pull/790)

Reviewer: [P3] Remove the personal attribution from `lampOS/docs/verification/v1-limp-lamp-52e6-20261015/README.md:3`, `:10`, and the new `:22`. The reviewer merge rule excludes colleagues' names. Keep the measured observations as anonymous English text.

Checked at 2026-10-15 15:40 +0700.

## PR #790 — overload

[Pull request](https://github.com/autonomous-ai/lampOS/pull/790)

Reviewer: [P2] The hand/overload timestamp finding from #780 remains at `lampOS/crates/body/src/body.rs:3586-3588`. A hand release updates `let_go_at`, so the first real overload within ten minutes is treated as a repeated overload and permanently ends the run. Track only actual overloads for this cutoff and carry the hand-then-overload regression into this head.

Checked at 2026-10-15 15:40 +0700.

## PR #780 — off comparison

[Pull request](https://github.com/autonomous-ai/lampOS/pull/780)

Reviewer: [P2] Hold the existing give-way mode constant in the switch-off comparison (`lampOS/crates/body/src/body/tests.rs:4252-4255`). `give_way: limp.then_some(GiveWay::Return)` turns off both give-way and limp in the purported limp-off case. That does not demonstrate that adding `--limp` preserves the existing `--give-way return` behavior when limp is unset. Keep `give_way` enabled in both cases and compare the off case's actual writes and stop outcome against that existing baseline.

Checked at 2026-10-15 15:40 +0700.

## PR #780 — identities

[Pull request](https://github.com/autonomous-ai/lampOS/pull/780)

Reviewer: [P3] Remove the personal attribution from the verification note (`lampOS/docs/verification/v1-limp-lamp-52e6-20261015/README.md:3` and `:10`). The reviewer merge rule excludes colleagues' names, and lampOS notes are English-only. Retain the measured observations as anonymous English text.

Checked at 2026-10-15 15:40 +0700.

## PR #780 — overload

[Pull request](https://github.com/autonomous-ai/lampOS/pull/780)

Reviewer: [P2] Do not count a hand slump as an overload (`lampOS/crates/body/src/body.rs:3580-3582`). `let_go_at` is updated for every release, including `by_hand`, but the next non-hand overload uses it to decide this is the second overload inside ten minutes. A hand slump followed by the first genuine overload therefore ends the run with torque off instead of resting and recovering after 120 seconds. Keep a separate last-overload timestamp, update it only for an overload, and add a hand-then-overload regression.

Checked at 2026-10-15 15:40 +0700.

## PR #778 — gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/778)

Reviewer: Local gate complete at `63c265fde070c8e989f8986b86609701ae982c9d`: `cargo fmt --all --check`, `cargo clippy -p lamp-voice --all-targets -- -D warnings`, and `cargo test -p lamp-voice -- --test-threads=1` all passed. Tests: 1,219 passed, 0 failed, 8 ignored across 21 suites. ARM64 run 38701241734 is also green at this exact head. The separate public-API reproduction still fails on the cross-person history finding at `lampOS/crates/voice/src/second_provider.rs:459-460`; the existing suite does not cover that transition. This remains with the first builder for that fix and the requested lamp minute.

Checked at 2026-10-15 15:40 +0700.

## PR #778 — docs

[Pull request](https://github.com/autonomous-ai/lampOS/pull/778)

Reviewer: [P3] Bring the second-route document up to date (`lampOS/docs/second-voice.md:6-8`, the opening of "A turn on the second route", and the final "Not shown" line). It still says nothing has run on a lamp although the same PR includes eight lamp-52e6 questions and their latency results. It also says only 15 seconds of audio waits for STT, while `second_provider.rs` sets `AUDIO_QUEUE = 3000` ten-millisecond packets (30 seconds), as the later Pace section already says. State the measured lamp-52e6 scope and remaining lamp-4ace work explicitly, and use the actual 30-second bound consistently.

Checked at 2026-10-15 15:32 +0700.

## PR #765 — progress expiry

[Pull request](https://github.com/autonomous-ai/lampOS/pull/765)

Reviewer: [P3] Include fetch time in the progress-note age limit (`lampOS/crates/voice/src/task_news.rs:731-735`). `wanted.age_ms` is computed before the two socket requests, but its remaining allowance is added to `Instant::now()` after those requests. A stand-in socket taking 400 ms per read leaves a note tellable about 60.8 seconds after its decision, beyond the documented 60-second bound. Preserve the absolute expiry calculated when the decision is received, reject a result that reaches it during fetching, and use that same expiry at the final take.

Checked at 2026-10-15 15:30 +0700.

## PR #751 — quiet window

[Pull request](https://github.com/autonomous-ai/lampOS/pull/751)

Reviewer: [P2] Start the quiet interval when active voice ends (`lampOS/crates/policy/src/progress.rs:536-537`). `Voice::Resting` leaves `last_talk` at the preceding active heartbeat. The voice publisher repeats active state once per second (`crates/voice/src/bus_out.rs:369-384`), so Speaking at t=0, Resting at t=0.9, and a pending progress change is asked at t=15 after only 14.1 seconds of silence. A controlled-clock reproduction against `1cbe0bbe9` confirms this. Refresh `last_talk` on the transition from active voice to Resting, without refreshing repeated Resting, and test that the documented 15-second quiet gate starts at the transition.

Checked at 2026-10-15 15:26 +0700.

## PR #778 — person history

[Pull request](https://github.com/autonomous-ai/lampOS/pull/778)

Reviewer: [P1] Keep the second route's recent history inside the person's session (`lampOS/crates/voice/src/second_provider.rs:459-460`, with the unkeyed history at `:351-371`). `Sessions::open` in `second_provider/worker.rs:246` changes the thread/memory to the new person, but `answer()` then appends every exchange in the engine-wide five-minute history, which has no person/session identity and is never cleared by that transition. Reproduced on `63c265fde` using the public Engine API and scripted services: finish person 1's turn, retire it, change the system callback to person 2, then submit person 2's turn. The second chat request contains person 2's system message followed by person 1's private user/assistant exchange. Thus the new per-person thread test passes while the actual text-model request still mixes people. Key or clear this history when the person/session changes, or build it from the same filtered thread; add a test that inspects the messages sent to chat across a person change.

Checked at 2026-10-15 15:26 +0700.

## PR #798 — handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/798)

Reviewer: for the first builder: this intentionally changes the default needs-input wording in lampOS/crates/voice/src/task_news.rs and agent_tasks.rs. There is no opt-in switch, so this reviewer role cannot merge it. I have read the full diff and will add the local gate results; the requested lamp-4ace question scenario remains with you. Head be4511a69 still needs a green ARM64 result. (15:25)

## PR #796 — handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/796)

Reviewer: for the first builder: this changes every lamp at startup (the busy microphone retry and first-control-message age allowance), so the reviewer role cannot merge it. Your five-restart check on lamp-4ace is the required judgment; the full diff is read and local gates will follow. Current head 93c5504d2 still has pending ARM64 checks. This is Part of #762, so the issue must remain open for the unexplained power-cut exits. (15:25)

## PR #765 — progress queued

[Pull request](https://github.com/autonomous-ai/lampOS/pull/765)

Reviewer: [P2] Withdraw queued progress when the task changes or finishes (`lampOS/crates/voice/src/task_news.rs:213-227`). Queued notes survive until their timer expires even after `tasks/status` removes/completes the task or advances its change. Reproduction on `93f6a771b`: offer Working/change 1, publish the same store with `tasks: []`, allow 600 ms for the reader, then `waiting()` still returns the note saying it is still working. The production coordinator waits for quiet/presence/provider availability at `coordinator.rs:4396-4411` before calling `tell()`, so a task can finish during that wait and still be announced as working. Invalidate queued notes on status changes and recheck store/task/state/change at the final take without blocking the coordinator; add a completion-during-wait regression.

Checked at 2026-10-15 15:23 +0700.

## PR #765 — progress fetch

[Pull request](https://github.com/autonomous-ai/lampOS/pull/765)

Reviewer: [P2] Validate the fetched progress state and exact change (`lampOS/crates/voice/src/task_news.rs:713-719`). The code only rejects `changes > wanted.change` and never checks the fetched `state`. If the task becomes idle between the bus snapshot and `get_progress`, this queues a note saying it is still working; a lower change is also accepted under a newer decision's authorization. Both reproduce against `93f6a771b` using the public `TaskNews` API and a local stand-in tasks socket. Require a valid Working snapshot with `changes == wanted.change` before constructing the note, and drop or retry a mismatched snapshot. Please add both race regressions.

Checked at 2026-10-15 15:23 +0700.

## PR #751 — stale context

[Pull request](https://github.com/autonomous-ai/lampOS/pull/751)

Reviewer: [P2] Discard outdated progress replies before publishing or updating task state (`lampOS/crates/policy/src/progress.rs:749-764`). Hold a yes reply for store A/task 3/change 1, then publish an empty `tasks/status` and `Voice::Listening`: the reply returns `say_now` with `guard=None` because the talking guard is reached only through `tasks.get()`. Alternatively replace status with store B/task 3: the old A reply marks B judged/told and gives B an unearned 15-minute cooldown. Both reproduce against `1cbe0bbe9` with a channel-controlled `Decider`. Invalidate pending work when its publisher/store/task/change identity is no longer current, verify that identity before touching `Tracked`, and apply the global talking guard even when the original task disappeared. Please add regressions for both transitions.

Checked at 2026-10-15 15:23 +0700.

## PR #795 — handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/795)

Reviewer: for the first builder: agreed with your review, this changes shared controls/state.privacy and voice/privacy contracts and needs a hand on the privacy slide to verify both microphones and the ring. I have picked up the new head 1a0e81d4a, including the added 50 ms pipeline test and trimmed verification folder. The green ARM64 result for 504244b64 does not clear this new head; its runs are still queued. Local review and gates will follow. (15:19)

## PR #792 — handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/792)

Reviewer: for the first builder: this adds the attention/addressed message contract in lampOS/crates/msgs/src/attention.rs, and the PR explicitly needs the live provider/bus path and near/facing thresholds judged on lamp-4ace. Head c4b68ea58 has a reported green native ARM64 attempt, but those contract and lamp checks keep its merge with you. Local review and gate results will follow. (15:19)

## PR #790 — handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/790)

Reviewer: for the first builder: this depends on #780 and changes the rest after a physical hand-induced slump from 120 s to 5 s. That recovery behavior needs the lamp judgment reserved by this role. Run 9 on lamp-52e6 is noted; head 0dec0e679 still needs its own green ARM64 run, and #780 must merge first. Local gate and code findings will follow. This remains Part of #703, not a complete closure of that issue. (15:19)

## PR #780 — handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/780)

Reviewer: for the first builder: this changes the physical contact-release and recovery behavior, and your existing review reserves the merge after the ARM64 run. The lamp-52e6 hand evidence is useful, but the feel and safety of the slump/recovery remain a lamp judgment. Current head is 7a89102a2; its ARM64 run 38700741466 is not yet green. I will add the local gate and code findings separately. This is Part of #703, so #703 must stay open after any merge. (15:19)

## PR #778 — handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/778)

Reviewer: for the first builder: #754 and the existing review require the switch-off conversation/tool/hand-over minute, then an outage and return-to-Gemini check on lamp-4ace. Those sound and switch-over judgments are outside this no-lamp reviewer role. Head 63c265fde is under local code review; cargo fmt --all --check passed and clippy is running. ARM64 run 38701241734 was still in progress when checked. (15:18)

## PR #765 — handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/765)

Reviewer: for the first builder: this includes the shared TaskStatus and Decision.task contracts from #751, and #752 explicitly requires both parts installed together with a progress line spoken on lamp-4ace before merge. Head 93f6a771b is under local review and gates; ARM64 run 38703256438 was queued when checked. I am leaving the merge and lamp judgment with you. (15:16)

## PR #751 — handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/751)

Reviewer: for the first builder: this changes shared contracts read by other components: lampOS/crates/msgs/src/lib.rs:863 adds Decision.task, and lampOS/crates/msgs/src/tasks.rs:149 adds tasks/status. That puts the merge outside this reviewer role even with both switches off. Head 1cbe0bbe9 is under local review and gates; ARM64 run 38703250546 was queued when checked. (15:16)

## PR #738 — handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/738)

Reviewer: for the first builder: the requested rest-labelled probe and five pause scenes need lamp-4ace, and the existing review reserves this for the next MVP voice minute. ARM64 run 38673872924 is green at head 21c6dd812. Local code review and gates are in progress; I will not merge or close #723 before the requested lamp judgment. (15:16)
