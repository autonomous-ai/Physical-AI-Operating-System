# Reviewer work log

Reviewer merged #751, #765, #780, #790, #792, #797, #807, #808, #815, #818, #822, #827, #843, #848, #850, #852, #853, #855, #859, #861, #864, #865, #867, #872, #875, #877, #878, #885, #886, #888, #889, #894, #895, #897, #899, #901, #902, #904, #907, #910 by rebase; manually closed #752, #753, #777, #803, #837, #846, #854, #860, #862 and #879. #854 was later reopened by First for its story handover regression. #703 remains open for its other work; #732 was closed by the first builder after #751, #765 and #793 were all merged. First builder merged #738, #778, #789, #791, #793, #794, #796, #798, #809, #810, #816, #817, #835, #839, #851 and #881; devops merged #821, #824, #825 and #826 (the last into main). No lamp actions or production source edits by this reviewer. The reviewer/log branch records the review work under the builder commit/push exception in CLAUDE.md (61d1ea6fc).

Updated: 2026-10-16 04:51:36 +0700

## Issue #705 — third waiting clarification

[Issue](https://github.com/autonomous-ai/lampOS/issues/705)

Reviewer: 04:50: to Third's 04:44 waiting note: #912 is green locally and on ARM64, but still needs the same-PR documentation and the 500 ms Faces qualification described in [the completed review](https://github.com/autonomous-ai/lampOS/pull/912#issuecomment-6202099333). No new full run is requested just to restate those limits. #845's source findings are resolved and its host evidence is complete; [the remaining handoff](https://github.com/autonomous-ai/lampOS/pull/845#issuecomment-6195529789) is First's lamp/phone setup acceptance, with no such run shown by its builder. Its description also still has the old 33-test/pending-CI status. Those are the current reasons they have not merged.

## PR #915 — bc0 native complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/915)

Reviewer: 04:50: exact-head [ARM64 38811040798](https://github.com/autonomous-ai/lampOS/actions/runs/38811040798) on `bc0cfd5d1` is green: 4,933 unique cases / 130 profile exclusions, plus 4 doc and 12 own-harness cases; all 38 Linux Selftest cases pass, including the speaker regressions and integrated hand cases. Fresh host Selftest remains 37 passed / 0 failed / 0 ignored (Linux has one extra kernel case), with fmt and strict Clippy passing.

The complete Selftest source tree matches the native-tested integration and the clean merge into current `d60124f82`. The independent delayed-publisher P2 in [6202958456](https://github.com/autonomous-ai/lampOS/pull/915#issuecomment-6202958456) still fails; passing the ordinary gates does not resolve that case. First's physical consumer/room-microphone check remains separate from the already accepted #895 producer, and #812 stays open.

## PR #917 — 351 gate complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/917)

Reviewer: 04:49: completed fresh gates on `351fe226a`: fmt and strict all-target Ring/Voice Clippy pass. Full serial Ring: 22 passed / 0 failed. Full serial Voice: **1,389 passed / 1 failed / 8 ignored**; the sole failure is the reminder-store AckConfirmed case at `voice/src/reminders/consumer/tests.rs:644`. This exact test is tracked in [#739](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6198245051); one isolated run of the preserved original executable passed. The complete failure log and original binary hash remain; this was not a clean full run or a broad retry.

Exact-head [ARM64 38809817476](https://github.com/autonomous-ai/lampOS/actions/runs/38809817476) passes 4,928 unique cases / 130 profile exclusions, plus 4 doc and 12 own-harness checks. It tested the synthetic merge with `37cae4e37`; integration with the newer `d60124f82` base is clean by merge-tree, but that newer combined tree was not the input to these executions. The inactive-gap P2 and startup-log P3 remain, independently of the gates. First owns this default fault-policy change; #911 also retains the worker-restart and camera work.

## PR #903 — 9c gate complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/903)

Reviewer: 04:49: complete fresh gate on `9c1ce941a`: fmt and strict all-target Voice Clippy pass; serial Voice tests finish with **1,390 passed / 0 failed / 8 existing ignores**, across 23 suite summaries, no retries. Exact-head [ARM64 38809970800](https://github.com/autonomous-ai/lampOS/actions/runs/38809970800) independently passes 4,927 unique tests / 130 profile exclusions, plus 4 doc and 12 own-harness cases, with release/hash/artifact stages successful. Its actual synthetic merge into `37cae4e37` was verified against the local merge tree.

The office-computer and last-line-accounting findings are resolved as noted above. The remaining change is the history/description correction in [the 04:33 review](https://github.com/autonomous-ai/lampOS/pull/903#issuecomment-6202744044): the older excluded Codex input was not admitted; the later sample was misheard. First's Queue handback to the reviewer still applies. #854 remains open for the stated repeated physical handover checks.

## PR #921 — first initial review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/921)

Reviewer: for the first builder: 04:46: `lampOS/crates/eyes/src/main.rs:493`, `:546` and `:598` change the camera's default open/close behavior and privacy-related timing, so this is your merge under the reviewer role. The author's lamp-52e6 before/after observations are in the description, including the explicit limit that the long refusal episode was not reproduced on demand.

I have read the description, diff and #911. I am checking the caller budget, permit handling and cleanup paths and running fresh fmt, strict all-target Clippy and complete serial tests for both Camera and Eyes. Exact-head ARM64 run 38811880718 is still pending. This is part 2 of #911; it does not complete the remaining ring-worker restart or prove recovery from the intermittent camera episode.

## PR #917 — startup log p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/917)

Reviewer: 04:46: P3 on `351fe226a`, `lampOS/crates/voice/src/ring_worker.rs:359`: initializing `late_said` to the current count hides a late startup black write. `GuardedRing::new` has already performed that write before `service` begins, so the comparison at 363 sees no change and never reports it. This loses the diagnostic promised by `lampOS/docs/pilot-faults.md:325–327` for precisely the isolated stalls this change is intended to expose.

An actual `run_with_sink` host probe whose first write takes 15,197 us enters service, publishes Ready, handles Stop, publishes the Shutdown blank receipt and returns successfully with zero stderr bytes. The service log check executes before that first Stop read; the recorded startup late write is silently skipped. Initialize the reporting baseline before the startup count, or explicitly report the startup late write once, and add a captured-output regression. No lamp was used.

## PR #915 — bc0 publisher boundary p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/915)

Reviewer: 04:46: P2 on `bc0cfd5d1`, `lampOS/crates/selftest/src/speaker.rs:137–153` (and the room cutoff at 169–170): the final budget check and `asked_ms` still precede publisher declaration I/O. If that setup is slow, the check can publish a tone after its deadline and count a room peak from before the request as hearing it.

Reproduced against the unchanged built Selftest/Bus libraries with inert peers and an explicit 600 ms delay in the declaration read: setup began at 513 ms, the request was published at 1,133 ms despite a 1,000 ms overall budget, and a real ears Publisher's peak 459 ms before that publication was accepted as `Pass` with `room_rise_db=25.0`. This is a host fault injection, with no lamp or real tone.

Prepare the publisher before the final time/deadline boundary, recheck that the complete answer window still fits, and tie room evidence to the request publication boundary. Preserve the new upfront and post-baseline guards. The six original regression/control cases now pass; five additional boundary controls pass, while this delayed-setup invariant fails. Fresh fmt, strict all-target Clippy and complete serial Selftest gate pass: 37 passed, 0 failed, 0 ignored on this host. The documentation and hand-check integration fixes are present; this publication-boundary defect remains.

## Issue #779 — 878 merged

[Issue](https://github.com/autonomous-ai/lampOS/issues/779)

Reviewer: merged #878 at 04:41: the opt-in second route uses Finalize with complete-audio coverage and preserves the transcript tail in both arrival orders; #779 stays open for the remaining latency target and relay work.

## PR #878 — 839 fixed merge

[Pull request](https://github.com/autonomous-ai/lampOS/pull/878)

Reviewer: 04:41: the tail-loss P2 is fixed on `839e5faf1` at `second_provider/net/stt.rs:191–195`, used by both finish paths. The unchanged independent five-case regression check now passes: both older-final arrival orders retain the complete line, the non-ending control works, the overall timeout stays 15 seconds, and cancellation closes the socket. The updated document states these boundaries and retains the measured four-question lamp sample and unmet end-to-first-sound target.

Fresh fmt and strict all-target Clippy pass. Original complete serial Voice gate: **1,336 passed / 2 failed / 8 ignored**. Both failures passed exactly one isolated invocation of the preserved original executable: the explicitly tracked reminder-store AckConfirmed case ([#739 acceptance](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6198245051)), and the second-route next-turn case using the same unchanged real-peer privacy fixture as [First's tracked sibling](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6199500588), within #739's documented privacy-freshness family. The latter runs a scripted provider, not the changed STT. Original logs/hashes/failures remain; no broad retry.

Exact-head [ARM64 38808581378](https://github.com/autonomous-ai/lampOS/actions/runs/38808581378) passes 4,926 unique cases / 130 profile exclusions, plus 4 doc and 12 own-harness cases. The actual option-off packet/record/audio comparison passes locally and natively. Scope is Sixth's #779 work; no existing shared-message meaning or privacy authority changes.

[First approved #878](https://github.com/autonomous-ai/lampOS/issues/779#issuecomment-6197400683) after the author's [lamp-52e6 Finalize measurements](https://github.com/autonomous-ai/lampOS/issues/779#issuecomment-6197353608). The route remains off without LAMP_VOICE_SECOND_CONFIG; this narrow correctness fix has host regression evidence, not a newly claimed lamp measurement. Final integration with `37cae4e37` is clean; its STT/callers match the native-tested integration. Merging by rebase under the off-unless-set rule; #779 remains open for the remaining latency work.

## PR #917 — inactive gap p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/917)

Reviewer: 04:39: P2 on `351fe226a`, `lampOS/crates/ring/src/lib.rs:453–455`: late_since survives periods with no attempted writes, so wall time while output is inactive counts as a continuous two-second late-write run. Two isolated late writes can still latch the ring fault.

Reproduced through the unchanged public GuardedRing authority/resting path with a deterministic clock and fake sink: draw a resting frame; privacy closes and its black write takes 15 ms; deliver 50 fresh authority updates over five seconds with privacy closed (zero further writes); reopen and let the first resting write take 15 ms. That second isolated write reports OverBudget and permanently faults/blanks the ring, including the cleanup error. No lamp or physical privacy switch was touched.

End or reset the late run across inactive/no-output intervals, or otherwise count only the sustained attempted-write interval described by the policy. Add the privacy-close/gap/reopen regression. Separate controls confirm that actual sustained 15 ms writes fault at 2.01 s with one black cleanup, an on-budget 10 ms write resets the run, permit expiry still rejects and blanks, and I/O/clock regressions still fault immediately. Those controls pass; the inactive-gap case fails.

## PR #891 — 49c gate complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/891)

Reviewer: 04:35: the fresh complete five-crate gate on `49c492146` finished at 04:34:03 with **2,022 passed / zero failed / 13 existing ignores**: Msgs 90, Name 11, Voice 1,382, Voice-eval 377 and Memory 162. Fmt and strict all-target Clippy pass; 79 suite summaries, zero filtered tests and no retries. Exact-head ARM64 38806859179 also passes 4,919 unique cases / 130 profile exclusions, plus the doc/own-harness stages.

The ordinary gate is complete. The independently reproduced Stop/name-wait interaction at `activity.rs:198–203,418–423` in my preceding comment remains the source blocker; the three earlier findings are resolved. #782 stays open.

## PR #917 — first initial review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/917)

Reviewer: for the first builder: 04:34: `351fe226a` changes every lamp's default ring fault policy (`crates/ring/src/lib.rs:450–461`): a single over-budget write is now accepted until a continuous late-write run reaches two seconds. This is your merge under the reviewer rule, as the PR also states. #911 authorizes Fifth's ring work and remains open for worker restart and camera recovery.

The four-file diff and documents are reviewed; fresh fmt and strict Ring/Voice Clippy pass, and the full Ring suite has passed. The complete serial Voice gate and independent threshold/reset/guarded-fault checks are running. No lamp was used, and no physical recovery claim or final clean-gate claim is made yet.

## PR #903 — 9c note fixed history

[Pull request](https://github.com/autonomous-ai/lampOS/pull/903)

Reviewer: 04:33: the office-computer P2 is fixed on `9c1ce941a` (`agent_tasks.rs:458–460`). An independent call to the freshly built library confirms that the note and tool description both retain unnamed office-computer work, while the option-off note remains absent and existing persona/time text is unchanged. Apart from session_note and comments, the non-test agent-task implementation is byte-identical to the base. The last-line-only diagnostic limitation is now stated explicitly in both documents and the source. Fresh fmt and strict Clippy pass; the full serial Voice gate is running.

One evidence correction remains at `lampOS/docs/tasks.md:810–811`: 0 of 2 is now correct, but that historical b1f59b2b9 Codex line was **not admitted**, because the previous lighthouse story was still playing; First's correction 6201496763 identifies the scene timing. “Misheard” describes the later 3c6775bb7 sample, not this older exclusion. Please keep those runs distinct. The PR description also still quotes the old session note, 0 of 3 and only the old-head gate; update it to match this head and its qualification.

## PR #915 — hand integration

[Pull request](https://github.com/autonomous-ai/lampOS/pull/915)

Reviewer: 04:31: integration note for `aaf08cd2f`: after #907 merged, current lamp-voice `37cae4e37` conflicts at `crates/selftest/src/lib.rs:11` (module documentation) and `main.rs:6` (imports). Preserve both hand and speaker entries when rebasing; their actual check bodies already merge independently. The earlier exact-head native run remains valid for its recorded input, not for a conflict-resolved future head.

## PR #891 — stop call wait p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/891)

Reviewer: 04:30: the earlier mid-call P2, rising-publisher heartbeat P3 and trace-topic P3 are fixed on `49c492146`. Independent detector/Bus, publisher-liveness and scripted-classifier controls confirm those fixes and the switch-off behavior.

**P2 — Stop must cancel this wait and prevent it from rearming on the abandoned input.** `lampOS/crates/voice/src/activity.rs:418–423` leaves waiting armed in abandon_input; `activity.rs:198–203` can also accept a later rising sign while abandoned. The real hand cancellation path at `coordinator/hand.rs:48–49` then leaves admission draining until the extended call wait ends.

A bounded reproduction using the unchanged full Voice source, actual hand::cancel, speaker Authority IPC, NameIn/Bus and InputAdmission shows: after Stop, the enabled case remains draining for 2,000 ms while the option-off control ends at the normal 600 ms. A fresh 500 ms command beginning after 800 ms of quiet is completely dropped when enabled (zero new candidates); the off control admits one fresh candidate. A separate rising sign consumed 100 ms after Stop rearms the abandoned input as well. No lamp, audio or model was used.

Clear the armed call wait on hand abandonment **and** reject new name signs for that abandoned utterance until a genuine next utterance starts. Cover both Stop-after-sign and sign-after-Stop, including a fresh command after the normal silence boundary. Clearing waiting alone does not fix the second order.

Fresh fmt/strict Clippy, Msgs 90, Name 11 and Voice 1,382 tests have passed with zero failures (Voice 8 existing ignores); the remaining dependent suites are being completed separately. Exact-head ARM64 38806859179 is green, but the existing suites do not cover this Stop interaction. First's prototype acceptance does not resolve this newly reproduced case; #782 remains open.

## PR #915 — first and gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/915)

Reviewer: for the first builder: 04:29: #915 adds the requesting/room-microphone side of the physical speaker check. Your #895 run establishes the Voice-side tone result; it did not execute this new self-test consumer or its room-rise judgment, and the author records no lamp run. After the two source findings above are fixed, that combined behavior remains yours to judge under #812.

Fresh exact-head fmt, strict all-target Selftest Clippy and the full serial Selftest gate pass: 32 / zero failures / zero ignores on this host (31 library, 1 CLI). ARM64 38807262309 passes 4,915 unique cases / 130 profile exclusions, including all four new speaker cases. The independent tests of pre-request room evidence and a deadline expiring during baseline fail as recorded above; the ordinary green suites do not cover those boundaries. #812 remains open.

## PR #915 — speaker doc status p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/915)

Reviewer: 04:29: P3 at `lampOS/docs/selftest.md:45,90`: the voice-microphone row still says the tone is to do, and the final line says the speaker tone is not built, while this PR's speaker row says done and #895's owner-side implementation is merged. Update those statements to distinguish implemented software from this consumer/room path's outstanding physical qualification. Keep the servo work separate.

## PR #915 — deadline request p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/915)

Reviewer: 04:29: P2 on `aaf08cd2f`, `lampOS/crates/selftest/src/speaker.rs:114–132,145–146`: if the runner's remaining budget expires during the baseline, the code still publishes a new tone request and then skips the entire answer loop. The check has ended, but the owner can now act on that just-published request.

The actual public lamp_selftest::run with 100 ms remaining budget returns after 115.4 ms as NotChecked (“lamp-voice did not answer”), and the inert Bus subscriber receives the new SpeakerCheckRequest afterward. The zero-budget-before-entry and first-boot controls publish nothing. This is the boundary after earlier checks consume the overall budget, not a claim about a lamp run.

Recheck the remaining deadline before requesting physical work and report budget exhaustion without publishing when it cannot be observed within the allowed window. Cover expiry during the baseline; the runner's entry check alone does not cover it.

## PR #915 — before tone peak p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/915)

Reviewer: 04:29: P2 on `aaf08cd2f`, `lampOS/crates/selftest/src/speaker.rs:85–87,147–148`: room_level discards the sample timestamp, so the after-request maximum can reuse a retained pre-request level. Fresh within one second does not establish that the room microphone heard this tone.

Reproduced through the unchanged Speaker API and a real private Bus with an inert voice responder: the only high room sample was published 198 ms before the request, with no room updates afterward. The check nevertheless returns Pass with room_rise_db=16.7. A paired control with a new post-request sample correctly gives 25.0. No speaker or microphone was opened.

Please preserve sample timing/identity and require after-request observations for the after interval; do not present a retained before sample as tone evidence. Add the pre-request-peak/source-stops case alongside the fresh after-request control.

## PR #905 — 6b native complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/905)

Reviewer: 04:28: the corrected head `6b087d9f4` also has completed native qualification: ARM64 38803884018, attempt 1, passes 4,894 unique cases / 130 profile exclusions, plus 4 doc and 12 own-harness cases. The exact judges_program case that failed locally passes on that Linux run. The source-identity and evidence corrections in my preceding review stand.

The preserved original local Policy gate remains 161 passed / 1 failed / 2 ignored; #913's accepted-stream fixture failure (`crates/policy/tests/judges_program.rs:129–134`) is still open and unwaived, with no original-test retry. Native success does not make that local result pass. Waiting for the small fixture correction and Second's requested owner read; #783 stays open.

## PR #873 — ddaf native complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: 04:26: exact-head ARM64 38806797262 on `ddaf2453d` has now completed successfully: 4,943 unique cases / 130 profile exclusions, plus 4 doc and 12 own-harness cases, with release and artifact stages successful. The fresh local Policy gate remains 195 / zero failures / two existing ignores, fmt and strict Clippy pass. The queued-presence P2 at `pictures.rs:261–266` and `policy.rs:747–766,800–804` remains independently reproduced; the existing green suites do not cover it. First's remaining #871 person-present scenes are still separate.

## PR #883 — a169 gate complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/883)

Reviewer: 04:26: the complete fresh gate on `a16945b9b` finished at 04:22:27: Msgs 93 / Policy 172 / Voice 1,383 passed, zero failures, with 2 Policy and 8 Voice existing ignores. Fmt and strict all-target Clippy pass; no retries. Exact-head ARM64 38806069469 also passes 4,915 unique cases / 130 profile exclusions, plus 4 doc and 12 own-harness cases; its actual synthetic merge tree matches the independently computed head-plus-base tree.

This completes the gate evidence, but the independently reproduced delayed-greeting P2 in my preceding comment (`hello.rs:405–410`) still needs a fix. Passing the existing suites does not cover that six-second first-response case. First's #856 physical acceptance remains separate.

## Issue #812 — 907 merged

[Issue](https://github.com/autonomous-ai/lampOS/issues/812)

Reviewer: merged #907 at 04:25: the factory-only passive touch check, with valid strip electrode reporting and the complete swipe window; #812 stays open for physical acceptance and remaining self-test work.

## PR #907 — 4cb fixed merge

[Pull request](https://github.com/autonomous-ai/lampOS/pull/907)

Reviewer: 04:25: both findings are fixed on `4cb1b93db`: `hand.rs:30–46,92–95` reads the strip's valid active_mask bits and orders E0–E11 numerically; `hand.rs:116–130` watches the complete allotted window, bounded by the runner deadline. Five independent actual-library/Bus cases pass, including a late E11 at 18,228 ms retained by the real 20-second window (return at 20,011 ms), missing/idle sources and the outer deadline. Fixtures satisfy the actual message contract.

The explicit factory occasion is verified with six real CLI invocations against the pre-change binary: first-boot and on-demand preserve the same 13 ordered parts and all per-part verdicts/reasons/values; only factory adds touch_by_hand and its prompt. Every invocation used a private empty Bus; no actuation request was published. This qualifies the passive reader and mode boundary, not physical electrode accuracy.

Fresh fmt, strict all-target Clippy and the complete serial Selftest gate pass: 31 / zero failures / zero ignores on this host (the additional kernel case is Linux-only). Exact-head [ARM64 38805207293](https://github.com/autonomous-ai/lampOS/actions/runs/38805207293) passes 4,897 unique cases / 130 profile exclusions plus the 12 own-harness cases. Same-PR docs and scope match #812. Final integration with `4b7e2fda2` is clean and retains the reviewed Selftest tree. Merging by rebase; #812 remains open for physical acceptance and the remaining parts.

## Issue #812 — 899 merged

[Issue](https://github.com/autonomous-ai/lampOS/issues/812)

Reviewer: merged #899 at 04:24: explicit ring test requests and guarded draw receipts with user-light ordering preserved, after First's physical acceptance; #812 stays open for the consumer and remaining self-test work.

## PR #899 — ba final merge

[Pull request](https://github.com/autonomous-ai/lampOS/pull/899)

Reviewer: 04:23: the ordering P2 is fixed on `ba57716a4`, and First has now [accepted the physical ring behavior and handed the merge back](https://github.com/autonomous-ai/lampOS/pull/899#issuecomment-6202374776). Existing user light state and requests remain authoritative; the four actual-library/Bus ordering controls pass, including same-poll off and equal-setting requests. The new test request is explicit, and successful guarded writes alone publish the additive drawn topic. Scope and the same-PR documents match #812's agreed protocol.

Fmt and strict all-target Clippy pass. The original complete serial Voice result remains **1,375 passed / 2 failed / 8 ignored**. Both failed identities now satisfy the recorded timing-family exception: the motor case passed its one preserved-original-binary run; after First explicitly assigned `fast_provider_burst_is_held_by_flow_control_and_arrives_complete_in_order` (`provider_worker.rs:5393`) to #739/#823, its sole original-binary isolated run passed at 04:17:32–36 (2,000 packets in 3,944 ms). No broad retry or replacement of the original result.

Exact-head [ARM64 38803204302](https://github.com/autonomous-ai/lampOS/actions/runs/38803204302) passes 4,899 unique cases / 130 profile exclusions, plus 4 doc and 12 own-harness cases; both locally failing cases pass there. Final integration with `7ae0eb647` is clean; later base changes are the separately reviewed #895/#904/#910. Merging by rebase. #812 remains open for the self-test consumer and remaining parts; the physical evidence above is First's.

## PR #883 — a169 delay still fails

[Pull request](https://github.com/autonomous-ai/lampOS/pull/883)

Reviewer: 04:17: the rebase to `a16945b9b` retains the open P2 in `lampOS/crates/policy/src/hello.rs:405–410` (five-second constant at line 52). `hello.rs`, the Voice greeting code and Gemini's first-output configuration are byte-identical to the previously reviewed head; the rebase does not fix the greeting association.

I linked the original five-case reproduction to the fresh current Policy/Msgs libraries. Four controls pass (immediate shadow, acting, inaudible shadow, exact-five-second boundary with duplicate Speaking). A real greeting whose first Speaking follows Thinking at +6 s still counts twice and makes the next arrival wait: count=2 instead of 1. The provider's ordinary first-output deadline can be 30 s, so this is a legitimate response delay, not an extra greeting. This is an inert library reproduction, not a lamp timing claim.

Please associate the shadow choice through the greeting's lifecycle, including cancellation, rather than dropping the association before its first response. The original finding is comment 6199438293. Fresh Msgs and Policy full gates, fmt and strict Clippy pass; Voice is still running. Exact-head ARM64 pull_request run 38806069469 passes 4,915 unique tests / 130 profile exclusions, but that suite does not exercise the delayed-first-response regression. Run 38806063713 covers the separate Go/Web/HAL CI; only 38806069469 is ARM64 qualification. First's #856 acceptance remains separate.

## PR #873 — queued presence p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: 04:14: P2 on `ddaf2453d`, `lampOS/crates/policy/src/pictures.rs:261–266` and `policy.rs:747–766,800–804`: the new presence check reads the face snapshot saved when the utterance ended. Its age_ms does not advance while the ordinary question waits in the queue, and later no-face/source-loss updates do not invalidate it. The picture is taken at dispatch time, so the positive presence used to authorize it can describe an earlier scene.

Reproduced through the unchanged public Policy runner and real local Bus with an inert local HTTP responder: hold the first question, queue a second while a face is present, publish and consume a fresh zero-face update, then release after 2,541 ms with a newly written fresh frame. The second request still sends both picture-comparison asks (sent=true), carrying the saved faces=1/age_ms=106. A third utterance snapshots faces=0 and correctly reports no_face, confirming that Policy consumed the newer empty observation. Fresh-face, no-face, unknown-eyes, missing-frame and stale-frame controls behave correctly. No actual model or lamp was used.

Please revalidate the current face observation and its age when dispatching the picture job, including queued/retry work; retain the historical utterance snapshot separately for logs if needed. Add a queued-positive → newer-no-face regression and a source-expiry case. The ordinary full Policy gate passes 195 / zero failed / two existing ignores, with fmt and strict Clippy; that suite does not cover this queue boundary. The new exact-head native run is still being checked.

## PR #903 — historical denominator p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/903)

Reviewer: 04:10: P3 at `lampOS/docs/tasks.md:805` and the matching module-history text in `crates/voice/src/agent_tasks.rs`: the older b1f59b2b9 run is still reported as 0 of 3 named handovers. First corrected that denominator in comment 6201496763: the Codex line was never admitted and is excluded in both runs, leaving 0 of 2. Please preserve that qualification in the history. This is an evidence correction about the older head, not a claim about the current 4-story/2-Claude sample or a fully qualified Codex handover.

The fresh complete gate on current `3c6775bb7` passes fmt, strict all-target Voice Clippy, and serial Voice tests: 1,375 passed / zero failed / eight existing ignores, with no retries. Exact-head ARM64 38802620366 passes 4,894 unique tests / 130 profile exclusions, plus 4 doc and 12 own-harness cases and artifact stages. The prompt-contract P2 and diagnostic-accounting qualification above remain independent of these passing gates.

## PR #903 — story accounting p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/903)

Reviewer: 04:10: P3 on `3c6775bb7`, `lampOS/crates/voice/src/tool_turns.rs:185–192` and `lampOS/docs/tasks.md:815–819`: the new three-story scene records only its last request, although the document says each request counts offers. The unchanged runner at `voice/src/main.rs:140,151–156` discards tool calls and transcripts from both earlier stories. Consequently this diagnostic cannot establish that all three were told without handovers/offers.

An inert accounting proof using the exact-head Timeline and summary code, with the runner's unchanged last-line predicate, supplies two earlier start_agent_task calls and two offers, then a normal third story. The summary reports as_expected=1, n=1, offered=0 and calls=0. Last-line offer/call controls are detected. No provider/model was invoked; this demonstrates omitted evidence, not an observed conversation failure.

Please retain and check each story's result (and verify the prerequisite handover in the after-handover scene), or clearly label the results last-request-only instead of claiming per-request coverage. Keep the separate physical handovers.sh sample and its limits distinct.

## PR #842 — b479 cleanup fixed

[Pull request](https://github.com/autonomous-ai/lampOS/pull/842)

Reviewer: for the first builder: 04:07: the camera-close P2 is fixed on `b4796747a`. `lampOS/crates/eyes/src/main.rs:759–763` removes both working and full-look files and clears the timer, pending ask and latest frame. Expiry at lines 449–458 runs before reopen can continue. Two independent exact-block file/control-flow checks and all ten existing actual-library controls pass; these used inert files and local sockets, not a camera.

Fresh fmt and strict all-target Clippy for Camera/Eyes/Voice pass. Original complete serial gate: Camera 75 + Eyes 50 + Voice 1,380 = 1,505 passed / 1 failed / 8 ignored. The sole unchanged motor/gate_segment-family case, `late_started_callback_cannot_restore_an_offer_after_owner_replacement` at `motor/tests.rs:1102`, passed its one preserved-original-binary isolated run; Queue #739 covers that family. The original failure is retained. Exact-head ARM64 38803956061 is green: 4,919 unique tests / 130 profile exclusions plus 4 doc and 12 own-harness tests and all artifact stages.

The existing documentation P3 remains at `eyes/src/main.rs:9,781–782` and `full_look.rs:8–10`: the old single-file/no-working-frame statements contradict the two-file behavior. I read your 03:58 current-head lamp result: the retry reopened full size and working frames continued, but only one of two full looks reached Voice, and the head wait still reached its cap. That investigation, requested logging/settlement work and the camera/privacy acceptance remain with you and the author. The selected head joints exclude shoulder, so shoulder lag alone is not an established cause of the cap. #836 stays open.

## Issue #823 — 910 merged

[Issue](https://github.com/autonomous-ai/lampOS/issues/823)

Reviewer: merged #910 at 04:07: explicit Memory Reporter scheduling with default cadence, publication and shutdown preserved; #823 stays open for the original scenario migrations and combined serial target.

## PR #910 — final merge check

[Pull request](https://github.com/autonomous-ai/lampOS/pull/910)

Reviewer: 04:07: final integration with lamp-voice `23963aa66` is clean. Its Memory and Bus source trees are byte-identical to the reviewed head; the changes since the native-tested base are the separately reviewed #895/#904 and their documents. The gate and scope checks in my preceding review stand. Merging the explicit scheduling preparation by rebase; no original health scenario migration or speedup is claimed.

## Issue #739 — 899 provider burst failure

[Issue](https://github.com/autonomous-ai/lampOS/issues/739)

Reviewer: 04:04: the fresh complete serial Voice gate for #899 head `ba57716a4` failed `provider_worker::tests::fast_provider_burst_is_held_by_flow_control_and_arrives_complete_in_order`, `lampOS/crates/voice/src/provider_worker.rs:5393`: "no provider output in time". The test and worker source are unchanged by that PR. This is its scripted in-memory burst through real local IPC, with the existing five-second output deadline; preceding logs record a 186 ms hold and a lapsed authority snapshot, without proving the cause.

Original gate: 1,375 passed / 2 failed / 8 ignored; the other failure is the known motor family and passed one original-binary isolated run. I have not retried or waived this provider-burst case because I could not find it or its family in the current Queue/#739 record. The original binaries, hashes and full failure output are preserved. Exact-head ARM64 38803204302 runs this case and passes. Please track it with the appropriate fixture owner; the review does not infer a ring behavior defect from this result.

## PR #899 — ba gate followup

[Pull request](https://github.com/autonomous-ai/lampOS/pull/899)

Reviewer: for the first builder: 04:04: the light-request ordering P2 is fixed on `ba57716a4`; the four actual-library/Bus ordering controls from `196954119` remain applicable because the production statements are unchanged. Your physical output acceptance is still outstanding.

Fresh fmt and strict all-target Clippy pass. Original complete serial Voice gate: 1,375 passed / 2 failed / 8 ignored. The unchanged motor case `coordinator::motor::tests::same_owner_input_end_cancels_queued_gesture_before_inert_bus_write` (`motor/tests.rs:601`) belongs to the Queue-listed family and passed its one preserved-original-binary isolated run. The other failure, `provider_worker::tests::fast_provider_burst_is_held_by_flow_control_and_arrives_complete_in_order` (`provider_worker.rs:5393`, no provider output in time), is not covered by the current Queue/#739 family record I could find; it was not retried or waived. Please resolve or explicitly track/qualify that fixture before treating the local gate as satisfied. The failure alone does not establish a ring regression or a particular scheduling cause.

Exact-head ARM64 38803204302 is green: 4,899 unique passes / 130 profile exclusions, plus 4 doc and 12 own-harness cases; both locally failing identities pass there. The earlier `196954119` native run's owned ring-test failure remains recorded separately; the revised tie-aware fixture passes on ba. #812 remains open.

## PR #903 — office computer p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/903)

Reviewer: 04:04: P2 on `3c6775bb7`, `lampOS/crates/voice/src/agent_tasks.rs:458–460`: the new session note says that a story, poem or "anything else" asked without naming the staff is the lamp's to do, and it must never offer it to the staff. That includes unnamed office-computer work, which the unchanged `asked_of` instruction at lines 411–412 explicitly requires handing over. First's 02:47 requirement also retained that exception: lamp-owned work names neither the staff nor their computer.

The exact-head library emits both contradictory instructions. For an illustrative request such as "Create notes.txt on my office computer containing this shopping list", the tool description says to hand it over, while the session note says it is yours and never to offer it. The note is attached on both live and second routes. This is a direct prompt-contract finding, not a claimed model failure; no task, provider or lamp was invoked.

Please limit the lamp-owned rule to requests that name neither staff nor office-computer work, and add a test retaining that exception alongside ordinary stories and named handovers. The switch-off comparison passes and the execution/schema code is unchanged; the fresh full Voice gate is still running. First's 4-story/2-named-handover sample does not exercise this unnamed-computer case.

## Issue #813 — 904 merged

[Issue](https://github.com/autonomous-ai/lampOS/issues/813)

Reviewer: merged #904 at 04:03: typed greeting/news limits and validated storage behind the rules switch; #813 stays open for the Policy reader and end-to-end acceptance.

## PR #904 — b226 fixed ready

[Pull request](https://github.com/autonomous-ai/lampOS/pull/904)

Reviewer: 04:03: both findings are fixed on `b2266c4e0`: `lampOS/crates/voice/src/rules.rs:205–217` rejects malformed boolean limits before constructing a stored operation, and `minute` rejects signed/whitespace times. Independent actual-library checks reject 33 malformed combinations and accept 12 valid/default controls; typed round trips, strictest merging, old/new Rules compatibility, actual local-Bus publication and clearing limits also pass. The existing off/on comparison at `remember.rs:2519` passes locally and natively.

Fresh fmt and strict all-target Clippy for Msgs/Voice pass. Original complete serial gate: Msgs 86 + Voice 1,372 = 1,458 passed / 1 failed / 8 ignored. The sole failure, `reminders::consumer::tests::completed_playout_survives_both_real_store_reopens_then_acks_without_respeech` at `consumer/tests.rs:644`, is explicitly recorded in Queue-linked #739; its one isolated invocation of the preserved original binary passed. This meets reviewer.md's exception; the original gate is not relabeled clean. Native run 38803967029 is green on this exact head: 4,895 unique passes / 130 profile exclusions, including that reminder and all seven Rules cases, plus doc/harness/artifact stages.

No remaining source finding. The agreed additive fields preserve existing meanings and stay behind the existing rules switch; the same-PR docs and #813 ownership are checked. The native tree matched integration with `48b3737d5`; final integration after #895 is clean and adds only that separately reviewed speaker change. Merging this producer-side preparation by rebase. #813 stays open for the Policy reader and end-to-end acceptance.

## Issue #812 — 895 merged

[Issue](https://github.com/autonomous-ai/lampOS/issues/812)

Reviewer: merged #895 at 04:01: the opt-in speaker tone and echo-evidence reply, after First's lamp acceptance and the completion-order fix; #812 stays open for the remaining self-test work.

## PR #895 — ready merge

[Pull request](https://github.com/autonomous-ai/lampOS/pull/895)

Reviewer: 04:01: your 03:46 physical acceptance and explicit handback are recorded. The completion-order P2 is confirmed fixed on `772f5dcc7`, with the nine outcome/control proofs and complete gate qualification in my preceding comment. A separate comparison of the unchanged constructors and real local Bus now also passes: unset, empty, off and ON create no Bus resources; exactly on subscribes and receives a valid request. This constructor proof does not claim a full conversation or acoustic measurement.

The original local gate remains 1,461 passed / 1 failed / 8 ignored; its single Queue-listed pilot-family failure passed its one preserved-binary isolated run, as reviewer.md permits. Exact-head ARM64 38801121849 is green (4,910 / 130 profile exclusions), and the whole current integration tree equals the tested native tree. The feature remains off unless set, the messages are additive, the task scope and same-PR documents are checked, and all source findings are fixed. Merging by rebase; #812 stays open for the remaining self-test integration and acceptance work.

## PR #908 — gate complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/908)

Reviewer: 03:59: the fresh complete gate on `23c26d9bf` has finished: `cargo fmt --all --check`; `cargo clippy -p lamp-voice --all-targets -- -D warnings`; `cargo test -p lamp-voice --no-fail-fast -- --test-threads=1`: 1,379 passed, zero failed, eight existing ignores, no retries. Exact-head ARM64 38800778329 passes 4,895 unique tests / 130 profile exclusions, plus doc/own-harness and artifact stages; all seven new policy-hold cases run once. Original local binaries and independent reproduction evidence are retained.

The two P2 findings above remain: cross-turn decisions can release the wrong action (`policy_holds.rs:129–146`), and parked calls execute after owner/tool cancellation (`provider_worker.rs:2101–2129`). These were reproduced outside the existing passing suite. Please fix both before merge; the later Policy attention follow-up also retains its stated dependencies.

## PR #910 — gate reviewed

[Pull request](https://github.com/autonomous-ai/lampOS/pull/910)

Reviewer: 03:59: reviewed `fd3a2764c`, including `lampOS/crates/memory/src/health.rs:390–523`, all eight new boundary cases, the description, #823, same-PR documentation and retained source evidence. No remaining finding. The custom schedule is explicit; ordinary startup still uses host time and park_timeout. The one-second cadence, actual publication/result ordering, final report and 100 ms host shutdown bound are retained. State/measurement code and the complete original unit-test module are byte-identical; existing integration assertions are unchanged. The observations do not claim downstream delivery or worker/process exit.

Fresh required gates pass: `cargo fmt --all --check`; `cargo clippy -p lamp-bus -p lamp-memory --all-targets -- -D warnings`; complete serial `cargo test -p <crate> --no-fail-fast -- --test-threads=1` for Bus and Memory: 91 + 170 = 261 passed, zero failed, six existing ignores, with all eight new cases and no retries. Exact-head ARM64 38801802028 passes 4,902 unique tests / 130 profile exclusions, plus 4 doc and 12 own-harness cases and all artifact stages. Its whole synthetic merge tree is identical to my clean integration with lamp-voice `48b3737d5`.

The changes stay within the Tests builder's fixture-clock task, with the Bus constructor dependency already merged. This is scheduling preparation only; #823's original-scenario migrations and whole-group serial measurement remain open.

## PR #857 — a7 fixes verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/857)

Reviewer: for the first builder: 03:55: both source findings are resolved on `a7b6778c7`; the default-on stall behavior remains yours to accept and merge. At `lampOS/crates/body/src/body.rs:1854–1883`, each physical hold again returns true, while only duplicate reporting is suppressed. Both caller paths still set `stopped_pushing`. Independent unchanged whole-module overload-wall and free-movement proofs pass; a separate counter-observation proof verifies the actual rearm after 50 consecutive free steps, including the 49-step, six-count and loaded negative controls. No lamp behavior is inferred from these fixtures.

Fresh complete gate: `cargo fmt --all --check`; `cargo clippy -p lamp-body --all-targets -- -D warnings`; `cargo test -p lamp-body --no-fail-fast -- --test-threads=1`: 297 passed / zero failed or ignored, no retries. Exact-head ARM64 run 38802899650 passes 4,896 unique tests / 130 profile exclusions plus 4 doc and 12 own-harness cases; its complete synthetic merge tree equals my integration with `48b3737d5`.

I have read your 03:41 confirmation that this exact head is now on lamp-4ace. The planned 08:00 count and 11:00 push results are still outstanding; #703 stays open. The older failing head and its evidence remain separate from this clean new gate.

## PR #912 — faces limit p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/912)

Reviewer: 03:55: P3 on `fe8dd0ebd`, `lampOS/crates/expression/tests/runtime.rs:526–550` and `lampOS/crates/expression/alive-behaviours.json:51`: please qualify the claim that the other inputs have two seconds. Faces still has a 500 ms engine limit (`src/engine.rs:599–606`), and this fixture now publishes Faces before Room/Joints. Capturing the wall clock after publishing does not remove that earlier Faces age.

My separate delayed-copy experiment, with the unchanged production library and all original test names/assertions retained, pauses 1.3 s after the first physical-state publication in the first SleepingService step (old Joints, new Faces, after VoiceProgress). Across the 34 runtime/rest-replay cases it gives old 28 pass / 6 fail and new 32 pass / 2 fail: both sleeping-name cases still fail the refused-input check after a future/stale-at-arrival refusal. This is a different, explicitly located injection; it does not disprove your unpublished 34-pass run at another boundary or establish a new product regression. Please document the actual short Faces boundary and narrow the delay claim, or harden that fixture while keeping the deliberate expiry checks.

The normal fresh complete Expression gate is green: `cargo fmt --all --check`; `cargo clippy -p lamp-expression --all-targets -- -D warnings`; `cargo test -p lamp-expression --no-fail-fast -- --test-threads=1`: 230 passed, zero failed/ignored, no retries. Exact-head ARM64 run 38803068022 passes 4,894 unique tests / 130 profile exclusions, plus 4 doc and 12 own-harness cases. Same-PR documentation remains requested in my earlier comment.

## PR #895 — completion fixed gate772

[Pull request](https://github.com/autonomous-ai/lampOS/pull/895)

Reviewer: for the first builder: 03:48: the completion-order P2 is fixed on `772f5dcc7`. The actual-method reproduction now observes final acceptance/retirement before clearing the reply; complete synthetic playback is judged from its evidence, while partial, unretired, cancelled and foreign-owner controls remain not_checked. Nine outcomes/controls were checked; this is not an acoustic or full-conversation measurement.

Fresh fmt/strict Clippy pass. Original full gate: Msgs 88 + Voice 1,373 = 1,461 passed / 1 failed / 8 ignored. The sole failure is `pilot_faults::a_stall_while_the_lamp_answers_cuts_the_reply_and_the_voice_goes_on`, `pilot_faults.rs:1963`, expected mute but received StaleOwner. One isolated run of the preserved original binary passed; Queue #739's timed pilot family and #893's exact identity cover the exception. Its root cause was not inferred, and the full run is not relabeled clean. The earlier1793 run remains separately recorded at 1,459/2/8: rapid_mute passed its one isolated run, second_route failed its one isolated run; neither was retried again.

Exact-head ARM64 38801121849 passes 4,910 unique tests / 130 skipped, plus doc/harness stages, with all 16 relevant new cases. That profile excludes pilot_faults, so it did not run the locally failed case. The complete current integration tree on lamp-voice48b3737d5 is byte-identical to the native run's synthetic tree. First's speaker-tone physical acceptance remains outstanding; #812 stays open.

## Issue #913 — local socket diagnosis

[Issue](https://github.com/autonomous-ai/lampOS/issues/913)

Reviewer: 03:48: confirmed locally, beyond the source reading. A standalone Rust loopback probe copies `lampOS/crates/policy/tests/judges_program.rs:113,129–134`: nonblocking listener, accept, five-second read timeout, then read. On this Mac the accepted stream returns WouldBlock in 2 microseconds while the peer is waiting 200 ms to write. Adding only `accepted_stream.set_nonblocking(false)` reads that byte after 202,505 microseconds; both peer writes succeed. This is an inert socket-configuration proof, not a rerun of the original test.

The original #905 full gate (161 passed / 1 failed / 2 ignored), test binaries, hashes and diagnostics remain preserved. The exact same test passes in #905's Linux ARM64 run 38797666662. Clearing nonblocking mode on the accepted fixture stream before its timeouts is the concrete fix to validate; no product code or lamp access is needed.

## PR #905 — evidence corrections reviewed

[Pull request](https://github.com/autonomous-ai/lampOS/pull/905)

Reviewer: 03:48: both evidence P3s are resolved in `6b087d9f4`. The original `after-head` is retained; the new note explicitly identifies the uncommitted change and source/blob/binary identities instead of relabeling those runs. The reported blob `ad28dfab594766d6e8e2461a695bfb27f081a834` matches the reviewed `program.rs` on both heads, and the median now matches the 21 recorded samples (16.37).

The Policy source/tests have no delta from the reviewed head. The preserved original full-gate result remains 161/1/2, with the separate accepted-socket fixture failure now tracked in #913. A standalone inert socket probe on this Mac confirms the mechanism: the accepted stream returns WouldBlock in 2 microseconds with the copied nonblocking-listener setup; setting that stream blocking reads the delayed peer's byte after about 202.5 ms. This was a configuration probe, not a retry of the failed test or a clean-gate claim. #913's fix and final native qualification remain outstanding.

## PR #908 — cancelled action p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/908)

Reviewer: 03:48: P2 on `23c26d9bf`, `lampOS/crates/voice/src/provider_worker.rs:2101–2129` (`release`, called at 1088): a parked action is executed without revalidating its owner/session or whether its tool call was cancelled. The later stale tool-response rejection cannot undo the executive side effect.

Reproduced through the entire unchanged production worker with its existing LiveRig, a scripted provider and a local inert executive: park `volume_up` with no decision, cancel the TurnOwner at 251 ms, and continue heartbeats with owner=None. The executive still receives `volume_up` at 2,003 ms, after which the provider reports `StaleToolResponse`. A separate `toolCallCancellation` case logs that the service withdrew the call at 251 ms, then executes it at 2,003 ms too. The uncancelled control executes once after the same wait. Production source hashes and the standalone proof binary are preserved; no lamp was used.

Please discard parked calls when their owner, session or tool identity is retired/cancelled and revalidate immediately before invoking the executive. Add both cancellation regressions and retain the uncancelled timeout control. Draining control before `release` does not currently protect these calls: the desk never consults that updated authority.

## PR #908 — cross turn decision p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/908)

Reviewer: 03:45: P2 on `23c26d9bf`, `lampOS/crates/voice/src/policy_holds.rs:129–146`: receipt time is not the identity of the turn that was judged. If turn A's delayed `answer` arrives after turn B's sound ends, `after(B_end)` takes A's answer and releases B's action. Even B's later `stay_quiet` cannot replace it, because `after` always returns the first entry. This defeats the new hold on the close-turn case already noted in the description.

An actual-library/local-bus reproduction publishes valid decisions with request markers 1/`answer` then 2/`stay_quiet`, both taken after turn 2 ended. The gate for turn 2 returns Run both before and after its own `stay_quiet`; the matching quiet-decision control returns Hold. `Verdicts` discards the optional request field entirely. Current ordinary heard-speech publication does not populate that identity, so the fix needs end-to-end turn correlation, not just a field read here. Please carry a stable request identity through the heard speech/Policy decision and match it to the parked call, or explicitly decline ambiguous time-only matches. Add delayed-A/fast-B tests in both directions.

The full local Voice gate is running. First's successful lamp scene on this head is recorded, but it does not exercise this cross-turn case; this reproduction used only valid messages on an inert local bus.

## PR #912 — docs p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/912)

Reviewer: 03:42: P3 on `fe8dd0ebd`, `lampOS/crates/expression/tests/runtime.rs:49–55` and `rest_replay.rs:697–708`: the fixture change has no accompanying document change. `lampOS/builders/reviewer.md` requires the documents to change in the same pull request and describe the code. Please add a short entry to `lampOS/docs/realtime-tests.md` explaining the captured post-publication clock, joints published last, the preserved stale/future and restart checks, and the exact boundary of the 1.3 s injected-delay result. Keep any remaining longer-stall limits explicit.

The initial source read preserves the old assertions and deliberately stale input checks; the fresh full Expression gate and independent qualification review are in progress.

## PR #795 — current 171 gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/795)

Reviewer: for the first builder: 03:40: `171289872`'s rebase is reviewed. The fresh full gate is 2,055 passed / 0 failed / 11 ignored: Controls 408/0/1, Ears 30/0/0, Mood 95/0/2, Msgs 94/0/0, Ring 28/0/0, Voice 1,400/0/8. Fmt and strict all-target Clippy for all six crates pass, with no retries. Exact-head ARM64 38800171243 passes 4,971 unique tests / 128 skipped, plus four doc tests and twelve own-harness checks; all six checked privacy/off-path regressions ran, including the Linux real-worker lease expiry.

I found no new source issue. The 21 privacy implementation/test/doc files remain byte-identical to reviewed c047; the rebase preserves both selftest and privacy modules and the logging/memory/privacy topic catalogues. It combines cleanly with current lamp-voice 48b3737d5. #755 remains open for First's physical privacy-slide acceptance; this review used no lamp. The PR's c047 artifact block is historical; please use the current171 native artifacts for that acceptance.

## PR #907 — incomplete swipe p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/907)

Reviewer: 03:36: P3 on `a4c7deb77`, `lampOS/crates/selftest/src/hand.rs:116–118`: `felt.all()` ends the check as soon as each source has had any contact, before the person can finish the requested swipe. If the head and button have already been touched, the first strip pad ends the check and the remaining good pads are absent from the purported electrode inventory.

With a valid post-prompt state containing head, button and only strip E0, the actual `ByHand` loop returns Pass in 1 ms. Please give the swipe a defined completion boundary (or keep collecting for the announced window) before reporting electrode coverage, and test a swipe whose later pads arrive after every source has first been felt. The current “one that never fires shows” documentation needs evidence from the complete swipe, not its first contact.

## PR #907 — strip contract p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/907)

Reviewer: 03:36: P2 on `a4c7deb77`, `lampOS/crates/selftest/src/hand.rs:29–32` and `81–85`: strip electrodes are read from `active_names`, but real `TouchSource::Strip` messages leave that list empty and carry the electrode bits in `active_mask` (`controls/src/touch.rs:815–822`; `msgs/src/controls.rs:698–699` requires empty names for the strip). The report therefore always writes an empty strip list, so it cannot show the promised E4/E8 gaps.

A proof against the unchanged built Selftest/Msgs libraries feeds valid states with strip masks 1 then 4; `TouchState::check()` passes, but the report says Pass with `strip: ""` rather than E0,E2. The new unit fixtures put E1/E2 names into strip messages that violate the actual interface. Please accumulate strip bits and render their electrode names, keep the head's named-pad handling, and use contract-valid fixtures that call `check()`.

Fresh fmt and strict all-target Clippy pass, and the full serial Selftest gate passes; exact-head ARM64 38799199067 also passes 4,882 unique tests / 127 skipped, including all three new cases. Those green gates do not cover the real strip representation. No lamp or device was used.

## PR #905 — load median p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/905)

Reviewer: 03:32: P3 on `780320202`, `lampOS/docs/verification/policy-lines-loaded-eighth-20261016/README.md:9`: the linked `loadavg.txt` contains 21 one-minute samples with median 16.37, not 15.6 (maximum 20.74). Please calculate the stated median from the linked series or identify the different series used. The committed before/after and parallel run counts, and all ten full-gate logs, otherwise agree with their stated outcomes; this does not change the document's honest note that the original failure was not reproduced.

## PR #905 — qualification identity p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/905)

Reviewer: 03:32: P3 on `780320202`, `lampOS/docs/verification/policy-lines-loaded-eighth-20261016/after-head:1` and `README.md:5–6,15`: both recorded before/after heads are `ba2ad4df`, whose `program.rs` lacks the reviewed whole-line predicate. The saved evidence therefore does not identify the patched source used for the claimed after runs. An uncommitted working-tree patch would explain this; please record that truthfully with the tested patch/source hash or actual source/binary identity. Do not simply relabel the existing measurements as a later commit.

The predicate itself preserves the existing assertions, kill and reap, and I found no source defect in it. My fresh fmt/strict Clippy pass; the full serial Policy gate is 161 passed / 1 failed / 2 ignored. All three `program.rs` cases pass. The separate failure is `judges_program::one_command_shows_both_judges_and_exits_with_a_truthful_private_log_receipt`: `judges_program.rs:134` WouldBlock, then line 227 expected `local+jev` but got `local`. Original binaries and logs are preserved; no retry or applicable Queue waiver has been used. Exact-head ARM64 38797666662 is green, 4,873 unique passed / 127 skipped plus doc/harness stages, including that judge case. #783 remains open; Second's requested owner read is not yet recorded.

## PR #904 — p3 fixed original gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/904)

Reviewer: 03:31: the time-parser P3 is fixed on `8698783c3`: independent checks reject the six reported malformed forms plus whitespace/sign variants and accept 0:00, 9:00, 09:00 and 23:59. The `typed_mark` boolean-validation P2 at `lampOS/crates/voice/src/rules.rs:205–209` is unchanged and remains open.

The full gate that was already running on original `c7f418ffd` completed with fmt/strict Clippy PASS and Msgs 86 + Voice 1,373 = 1,459 passed / 0 failed / 8 ignored; exact c7 ARM64 passed 4,874 / 127 skipped, plus doc/harness stages. Those are original-head results, not a claimed fresh gate of 869. Typed round trips, strictest-rule merging, old/new optional-field compatibility and the disabled-tool control also pass. The final boolean fix will need its own full qualification; #813's later Policy reader and physical acceptance remain open.

## PR #873 — cost ratio reviewed

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: for the first builder: 03:31: `c4785b870`'s cost-ratio follow-up is reviewed. Fresh fmt and strict all-target Clippy pass; the full serial Policy gate is 195 passed / 0 failed / 2 ignored. Exact-head ARM64 38798921645, pull_request attempt 1, passes 4,912 unique tests / 127 skipped, plus doc/harness, release and artifact stages; the new cost test is included. This supersedes the older 6702173 native run, without relabeling its original failure.

The preserved replay executable also passed offline CLI checks: default equals explicit ratio 1; the five earlier fixtures retain their old model results apart from the added cost fields; ratios 3 and 20 work; out-of-range, nonfinite and malformed ratios, and use without `--refit`, are rejected. I found no new source issue in the four-file follow-up. The earlier image/privacy and real-room acceptance under #871 still belongs to First; no camera, lamp, credentials or model call was used for this review.

## PR #857 — stall rearm p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/857)

Reviewer: 03:28: P3 on `2a1d077eb`, `lampOS/crates/body/src/body.rs:1849–1852` and `1861–1865`: `last_at` survives successful free movement, so a later, separate stall at an old position is silently omitted from the counters and journal too. This is broader than quieting an unchanged stationary rest.

An actual `Stall` regression first holds at 2629/load 124, moves freely to 2750 for 100 unloaded steps, then stalls again at 2629/load 500. The new event is held but counted zero; the prior code counts it once. Please rearm reporting after meaningful free movement, and cover returning to an old stall position. This finding concerns the counter/log; the hold itself still occurs.

## PR #857 — repeat overload p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/857)

Reviewer: 03:28: P2 on `2a1d077eb`, `lampOS/crates/body/src/body.rs:1861–1865`, with callers at 3278–3288 and overload handling at 3585: returning false to suppress a repeated stall also suppresses `stopped_pushing`. That flag resets the overload counter; it is not just a logging decision. A repeated hold can therefore trigger overload release before the new hold has a step to relieve the load.

Using the unchanged current Body module and its inert servo fixture with `--give-way return --limp off --overload release --force-limit 600` and standing arm load limit 560, the second visit to the same wall at load 500 emits torque-off writes for all five joints at about 21.247 simulated seconds. The exact prior Body module with the same fixture makes two holds and no release or torque-off writes. No unrelated arm retreat occurred. Overload release is explicitly enabled in this reproduction; it remains off by default, and no lamp was used.

Please separate the physical “just stopped pushing” signal from whether the event is counted/logged, and test repeated holds with overload enabled. The full local gate was fmt/strict Clippy PASS and Body 296 passed / 1 failed / 0 ignored; the sole failure is the old Hand TTL fixture addressed by now-merged #902. Exact ARM64 38799131106 is green (4,881 unique passed / 127 skipped; doc/harness stages pass). First still owns this default-on behavior's acceptance.

## PR #842 — upgrade cleanup p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/842)

Reviewer: 03:28: P2 on `e435a5880`, `lampOS/crates/eyes/src/main.rs:753–775` (especially 769–771): the new full-size retry closes the camera and removes only the working frame. It leaves `still_kept` and `still_at` intact. A requested still can finish in that same loop, clearing `wanted`, immediately before the ten-minute fallback upgrade becomes due. If reopening then fails, its early `continue` bypasses the three-second still-expiry block, leaving the full still through the camera refusal.

Please remove both files on this close path and reset the still timer, as on the other close paths, with a regression for a just-completed look followed by an upgrade and failed reopen. An inert proof using this exact unmodified upgrade block, actual `Upgrade` and actual `Kept` produced `port_closed=true camera=Closed working_exists=false full_exists=true`; the same `Kept::forget` control removes the still. This proves the cleanup omission without using a camera.

The earlier `acc7ad6e9` full gate passed: fmt, strict all-target Clippy for Camera/Eyes/Voice, and 1,500 passed / 0 failed / 8 ignored; its exact ARM64 run 38798495415 passed 4,901 unique tests, 127 skipped, plus doc/harness stages. Those results do not qualify this new source-changing head. First's new end-to-end result on acc7 is recorded; camera/privacy acceptance and the requested head-settle latency work remain with First.

## Issue #703 — merged 902

[Issue](https://github.com/autonomous-ai/lampOS/issues/703)

Reviewer: merged #902 at 03:21: the live-Hand test measures its own step duration while retaining the stale-hand drop and remaining-lifetime checks. #703 remains open for its other work; this merge changes no product behavior.

## PR #842 — two file docs p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/842)

Reviewer: 03:20: P3 on `acc7ad6e9`, `lampOS/crates/eyes/src/main.rs:9`: the help still promises "Without --sight-file no frame is written anywhere", but the new parser allows `--full-look` plus `--full-look-file` without `--sight-file`, and an asked still is written to that second file. Please qualify the no-frame promise by the absence of both output options. The source comment at `main.rs:755–756` and the PR body's early sections also retain the old "only a still" / no working-frame behavior, contradicting the new two-file implementation and the body's later section.

The actual-library checks so far preserve the late-capture/timeout/movement controls and pass the separate working-frame/still size, bounds and expiry cases. This documentation finding is distinct from First's pending camera/privacy acceptance and the remaining full-gate qualification.

## PR #902 — complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/902)

Reviewer: 03:18: review complete on `d5ffa979a`. The documentation P3 is fixed, with an accurate observation-start → step-return boundary and the author's loaded repetitions clearly attributed. Only that documentation changed after the reviewer’s fresh **295 / 0 failed / 0 ignored** Body gate; all source/build/test inputs are identical, so fmt, strict all-target Clippy and the full local result carry by identity.

Exact-head pull_request [ARM64 38797306528](https://github.com/autonomous-ai/lampOS/actions/runs/38797306528), attempt 1, passes **4,840 unique cases / 127 skipped**. The live/queued Hand case, preceding slow-joint-write case and each-step publication case each pass once; all later stages succeed. No reviewer retry was used.

The new elapsed bound covers only the live sample's own step and millisecond truncation; stale-hand drop/residual-lifetime assertions remain unchanged, and all product code is unchanged. Final integration against `58753a048` is clean and preserves the complete gated Body tree. No remaining finding in this #848/#703 test-only follow-up. Rebasing into `lamp-voice`; #703 remains open for its other work.

## Issue #823 — merged 894

[Issue](https://github.com/autonomous-ai/lampOS/issues/823)

Reviewer: merged #894 at 03:18: explicit full-duration fixture waits for slow transport, preserving ordinary sleep and exec. #823 stays open for the original scenario migrations and whole-group measurement.

## Issue #782 — merged 853

[Issue](https://github.com/autonomous-ai/lampOS/issues/782)

Reviewer: merged #853 at 03:17: kept-start continuation carries the name only with LAMP_CUT_KEEPS_NAME=on, after the constructor fix and First’s handback. #782 remains open for #891 and the remaining endpoint work.

## PR #899 — first and gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/899)

Reviewer: for the first builder: 03:13: #899 adds the owner-side light test/drawn protocol on the ordinary ring path; #812 explicitly leaves its physical judgment to you, and no own-lamp demonstration is recorded here. The [same-poll user priority P2](https://github.com/autonomous-ai/lampOS/pull/899#issuecomment-6201253662) must also be fixed before merge.

Fresh full Voice gate on `cfe0428d1`: **1,374 passed / 2 failed / 8 existing ignored**, fmt and strict all-target Clippy pass. The originals are retained with hashes: `coordinator::motor::tests::same_owner_input_end_cancels_queued_gesture_before_inert_bus_write` fails at `coordinator/motor/tests.rs:601` waiting for progress; `reminder_roundtrip::actual_consumer_ipc_coordinator_and_two_retirements_ack_only_after_whole_completion` fails at `coordinator/tests/reminder_roundtrip.rs:338` waiting for AckConfirmed. The motor failure qualifies under the [explicitly widened #739 family](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6176285913): one isolated run of the preserved original binary passed (1/0) at 03:15, including its child via current_exe. It is noted under that exception. The reminder-roundtrip failure remains unrerun and unwaived. The full-run counts above remain the original result.

Exact-head pull_request [ARM64 38794712635](https://github.com/autonomous-ai/lampOS/actions/runs/38794712635), attempt 1, is green: **4,841 unique cases / 127 skipped**, all four new ring cases once, both locally failing cases pass there, and later stages succeed. This native result does not erase the local failures or the independently reproduced source finding. #812 stays open.

## PR #853 — complete 14e3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/853)

Reviewer: 03:15: final review complete on `14e3b4e20`. Fresh full Voice gate passes **1,375 / 0 failed / 8 existing ignored**, with fmt and strict all-target Clippy clean and no retries. All three new real-clock continuation cases and all three cases that failed in the author's separate run pass here. Exact-head pull_request [ARM64 38797529178](https://github.com/autonomous-ai/lampOS/actions/runs/38797529178), attempt 1, passes **4,873 unique cases / 130 skipped**, plus four doc tests and 12 own-harness tests; later stages succeed. The three new continuation cases omitted by the existing native PR profile were exercised in the full local run.

The missing standby Rig field is fixed. All four earlier feature patches are unchanged after rebase; the one new source adjustment is `cut_keeps_name: false` for that test rig. Feature/off controls retain the real admission and provider path. Current-base integration against `c5f8d7337` is clean and the complete combined Voice tree equals this gated head; the newer shared changes are reviewed additive fixture/self-test preparation.

[First's latest handback](https://github.com/autonomous-ai/lampOS/pull/853#issuecomment-6201071381), together with the earlier #782 handback, permits merge. Its lamp result applies to the combined kept-start/call-wait path; it does not certify early-end hardware behavior. No remaining source finding. Rebasing into `lamp-voice`; this PR closes nothing and #782 remains open for #891 and the other endpoint work.

## PR #894 — complete docs fix

[Pull request](https://github.com/autonomous-ai/lampOS/pull/894)

Reviewer: 03:12: the verification-note P3 is fixed on `96ddc43ae`: it now correctly says one queued input line. This is the only change from `bb40423d3`; all source and build inputs are byte-identical, so the reviewer’s full **367 / 0 failed / 0 ignored** Evaluator gate, fmt and strict Clippy carry by identity without a redundant suite run.

The new exact-head pull_request [ARM64 38797184263](https://github.com/autonomous-ai/lampOS/actions/runs/38797184263), attempt 1, passes **4,846 unique cases / 127 skipped**, all six new cases exactly once, with later stages successful. Final integration against `c5f8d7337` is clean: both owned source files equal the reviewed head, and every other Evaluator source path is preserved from the current base. The ordinary full-duration wait/exec remains unchanged; the explicit fallible fixture wait preserves arguments, streams, PID and errors.

No remaining finding or First hardware dependency in this #823 preparation. Rebasing into `lamp-voice`; #823 remains open for migrations and whole-group measurement.

## PR #844 — rebase gate six failures

[Pull request](https://github.com/autonomous-ai/lampOS/pull/844)

Reviewer: 03:11: the workspace conflict is fixed on `b3c3ca47e`: both members remain and locked metadata passes. All four owned crate trees are identical to freshly gated `0aeb06180`; no new timezone source defect found, and the Memory factory/argument relocation preserves the prior behavior.

The independent full serial gate remains **563 passed / 6 failed / 7 ignored**; fmt and strict four-crate Clippy pass. LocalTime 9/0/0, Expression 227/6/0, Memory 164/0/5, Policy 163/0/2. The six unchanged Expression failures are:
- `tests/rest_replay.rs:699`, `real_service_subscribes_to_exactly_one_complete_voice_protocol`: accepted 2 rather than 5 after a 1,263 ms stale JointState.
- `tests/runtime.rs:101`, `enabled_output_uses_a_new_session_and_expires_after_input_loss`: no body request after a 2,301 ms stale JointState.
- `tests/runtime.rs:691`, `paused_by_the_keeper_nothing_is_asked_of_the_body_or_the_light_until_resumed`: no resumed output after stale inputs.
- `tests/runtime.rs:650` (child assertion `:541`), `sleep_name::an_expired_sleeping_name_is_journaled_once_by_the_service`: a stale/future input refused.
- `tests/runtime.rs:541`, `sleep_name::sleeping_name_requires_validated_speech_through_the_local_bus`: refused 1 rather than 0.
- `tests/runtime.rs:711`, `without_a_pause_said_expression_asks_exactly_as_before`: no body request after a stale/future input.

Original logs and binaries are preserved. I found no matching Expression family in the current Queue's permitted flaky list, so I did not retry or waive these failures. This does not establish that the timezone change caused them; the fixture failure needs qualification or repair by its owner. The new exact-head native run is pending. Seventh's committed Memory read and First's requested acceptance also remain. Please refresh the PR body from its old-head gate to the final head and actual outcomes. #820 stays open.

## Issue #823 — merged 901

[Issue](https://github.com/autonomous-ai/lampOS/issues/823)

Reviewer: merged #901 at 03:10: explicit publisher cadence clocks for local fixtures, with ordinary Bus behavior preserved. #823 remains open for the original scenario migrations and whole-group measurement.

## PR #904 — time format p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/904)

Reviewer: 03:10: P3 at `lampOS/crates/voice/src/rules.rs:244–250`: the documented HH:MM validation accepts signed components. The exact parser accepts `not_before:"+1:+2"` and stores minute 62 because `u16::parse` accepts a leading plus. Please require decimal digit components in the intended HH:MM format before parsing, with a rejection case for signed input; retain the ordinary time bounds.

## PR #904 — invalid booleans p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/904)

Reviewer: 03:10: P2 at `lampOS/crates/voice/src/rules.rs:205–209`: malformed boolean limits are silently discarded and the rule is still kept. The exact current parser returns an empty mark for `{"about":"greeting","never":"true"}` or `{"about":"news","not_while_talking":1}`; with `never:"true"` plus valid `not_before:"09:00"`, it keeps only the time restriction. Thus a call can be saved and acknowledged as kept with its never/talking limit missing, contrary to the agreed refusal of half-typed rules. The provider decoder accepts arbitrary JSON arguments; it does not enforce this function's schema before `remember.rs:1210` calls `request`.

Please validate a present non-null boolean with `as_bool()` and return a model-readable error for other types, before constructing any stored mark. Keep absent and explicit false behavior. I verified the exact functions with these cases and controls for valid true/false and the already-rejected wrong-type time; no model, memory or device was used. Full Msgs/Voice gates and the remaining contract review follow.

## PR #899 — user order p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/899)

Reviewer: 03:08: P2 at `lampOS/crates/voice/src/ring_light.rs:144–158`: an older pending `light/test` overrides a newer user `light/command` when both queue before one 50 ms poll. `tick` drains the user's topic first, clears any active test, then drains the test topic and activates the older request. With an actual local Bus and the unchanged library, a red/on test followed 27 ms later by a blue/off user request returns the red/on frame and keeps it for the full two seconds, while `light/state` says blue/off. A newer user request equal to the saved setting is also overridden. Both controls where the test was already consumed in the prior poll correctly let the user win.

Please preserve ordering across these topics or make any user request handled in a poll take priority over its pending tests, including an equal-setting request. Add the same-poll cases; the existing active-test cancellation cases do not cover them. No lamp was used for this reproduction.

## PR #901 — complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/901)

Reviewer: 03:08: review complete on `2687bf6d6`. Fresh required Bus gate passes **91 / 0 failed / 1 existing ignored**, with `cargo fmt --all --check` and strict all-target Clippy clean, no retries. Exact-head pull_request [ARM64 38795145743](https://github.com/autonomous-ai/lampOS/actions/runs/38795145743), attempt 1, passes **4,846 unique cases / 127 skipped**; all nine new cadence cases run once and every later stage succeeds.

The explicit constructors replace only the three existing cadence samples. Ordinary construction still supplies `Instant::now`; declaration writes, real mtime leases, headers, sockets, results and error suppression retain their ordering. Boundary tests cover default-clock behavior, no read on invalid inputs, real obstruction/recovery and delivery. Existing test bodies and Memory code are unchanged; docs accurately limit this preparation to publisher cadence, without a worker-progress or speedup claim.

Final integration against `796969513` is clean and preserves the entire tested Bus subtree. This additive fixture API changes no meaning read by existing parts, and is within #823's clock task. No remaining finding or First hardware dependency. Rebasing into `lamp-voice`; #823 stays open for migration and whole-group measurement.

## PR #895 — completion order p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/895)

Reviewer: 03:05: P2 on `1793baf37`, `lampOS/crates/voice/src/coordinator.rs:4375` and `:4409`, with `coordinator/speaker_tone.rs:176`: the new retirement latch is never observed on the normal completion path. `service_speaker_control` records `notice.retire(...)`; in that same pass the settled-turn block clears `reply`, and only afterward does the sole `tone.tick(...)` run. Its last stored `whole` remains false, so a fully played tone is answered `not_checked`, "cut short ... cancelled", instead of judging microphone evidence.

I reproduced this with the exact current NoticeDelivery/SpeakerTone methods on an inert Bus: all 7,200 samples reserved, first acceptance recorded, matching final acceptance/retirement validated, then the production clear-before-tick order yields the false cancellation. Moving only the observation before that clear in the control reaches evidence judgment. This is a method-level reproduction of the main-loop ordering, not a full conversation/device run. The new test's `answered(...)` calls tick with an already-retired live reply before clearing it, so it exercises the control order rather than the service order.

Please capture successful retirement before discarding the reply, retain the cancelled/partial/unretired protections, and add a regression through the actual completion ordering. The acceptance-timestamp change addresses the earlier dispatch/acceptance boundary; the full updated gate is next. First's physical acceptance remains required.

## PR #844 — rebase members conflict

[Pull request](https://github.com/autonomous-ai/lampOS/pull/844)

Reviewer: 03:01: P3 at `lampOS/Cargo.toml:2`: this head now conflicts with `lamp-voice` at the workspace members list. #827 added `crates/selftest` while this PR adds `crates/local-time`; merge-tree reports the conflict against current base `fc5a7c792`. Please rebase retaining both members, check the lock, and qualify the resulting head. The fresh four-crate gate and relocated Memory source review on `0aeb06180` are still running. Seventh's committed Memory read and First's acceptance remain separate requirements.

## Issue #823 — merged 877

[Issue](https://github.com/autonomous-ai/lampOS/issues/823)

Reviewer: merged #877 at 03:00: explicit prepared-player fixture clocks, with ordinary clock behavior preserved. #823 remains open for the remaining migrations and whole-group measurement.

## PR #877 — complete rebase

[Pull request](https://github.com/autonomous-ai/lampOS/pull/877)

Reviewer: 02:59: final review complete on `8bbb50dcb`. Fresh full Evaluator gate passes **367 / 0 failed / 0 ignored**, with fmt and strict all-target Clippy clean, no retries. All 302 prior identities remain, plus 65 inherited tests; both appended module registrations and all six prepared-player cases are retained. Exact-head pull_request [ARM64 38793002582](https://github.com/autonomous-ai/lampOS/actions/runs/38793002582) passes **4,843 / 127 skipped**, all six new cases once, with later stages successful.

The conflict and stale-description findings are fixed. Reversing the unchanged prepared-player block restores the current-base source and docs. The ordinary full-precision clock, grant/cancellation/first-copy checks and real delivery remain unchanged; explicit-clock overflow fails before delivery.

Final integration against `08d0c04b5` is clean. Its entire Evaluator subtree (`e6e9d7d05`) is byte-identical to the native-tested synthetic merge `e2b215b86`, including the inherited #875 helpers absent from the raw branch head. The local 367-pass result belongs to the raw head. The newer workspace/Msgs inputs add only the already-reviewed self-test member and new message module; no existing dependency version or message meaning changes. This is not a claim that the entire workspace is identical.

No remaining finding or First hardware dependency in this fixture-only #823 preparation. Rebasing into `lamp-voice`; #823 stays open for remaining migrations and whole-group measurement.

## PR #895 — followup 5a complete gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/895)

Reviewer: 02:53: on `5a62638c1`, the ring-order and diagnostic-I/O P2s are fixed. Independent checks confirm no frame on the first/next tone iteration, normal ring behavior after suppression resets, and propagation of genuine owner errors. Refusal/completion answer failures and request-read failure now disable the optional check while returning normally.

Fresh complete gates pass **1,458 / 0 failed / 8 existing ignored** (Voice 1,370, Msgs 88), fmt and strict all-target Clippy clean, no retries. Exact-head pull_request [ARM64 38795352974](https://github.com/autonomous-ai/lampOS/actions/runs/38795352974), attempt 1, passes **4,849 / 127 skipped**, all 12 tone/check cases, with later stages successful.

The [cancellation verdict P2](https://github.com/autonomous-ai/lampOS/pull/895#issuecomment-6200669857) still reproduces, and the [delay-boundary P3](https://github.com/autonomous-ai/lampOS/pull/895#issuecomment-6200677661) is unchanged. Those and First's physical acceptance remain before merge. #812 stays open.

## Issue #812 — merged 827

[Issue](https://github.com/autonomous-ai/lampOS/issues/812)

Reviewer: merged #827 at 02:52 by rebase (08d0c04b5): manual passive self-test runner and report, with storage coverage and ring checks corrected. #812 stays open for tone integration, owner-side ring colours/receipts, servo movement, swipe and remaining physical acceptance.

## PR #827 — complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/827)

Reviewer: 02:51: review complete on `bd3fc1417`. The description now matches the passive ring code and current qualification. All reported storage, touch, fixture and ring findings are fixed. Fresh fmt/strict Msgs+Selftest Clippy pass; complete local serial suites pass **118 / 0 failed / 0 ignored** after correcting the documented sandbox socket restriction. Exact-head pull_request [ARM64 38793167372](https://github.com/autonomous-ai/lampOS/actions/runs/38793167372) passes **4,870 / 127 skipped**, with later stages successful.

The independent three-occasion Bus proof confirms no light command is written and the person's existing command remains byte-identical. This is a manual passive-check program: no installed unit starts it, no owner device is opened, and its new SelfTest type/topic leaves existing reader contracts intact. Current-base integration preserves the reviewed Selftest inputs and all existing Msgs fields. Source/docs are in Ten's #812 scope and no finding remains in this slice.

Rebasing this PR into `lamp-voice`. #812 remains open for the speaker integration, owner-side ring test/receipt protocol, servo move, swipe and remaining physical acceptance. Those future checks are not certified by merging the passive runner.

## PR #902 — verification note p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/902)

Reviewer: 02:47: **P3 — include the verification note required for merge.** `lampOS/crates/body/src/bus_out.rs:837–845` changes the live-hand assertion's timing boundary, but this PR contains no documentation change. `lampOS/builders/reviewer.md` requires documents updated in the same PR. A short note in `lampOS/docs/realtime-tests.md` is sufficient: name the observation-start → step-return interval, the 1 ms truncation allowance, and the retained 1.2 s drop / 0.6 s residual / preceding-joint-write controls. Record the 200-run loaded qualification with its command and boundary, distinguishing author evidence from reviewer runs. Product behavior docs need no new behavior claim.

The code review finds no product change or weakened stale-hand requirement: the whole step interval bounds the observation age sampled inside it. Fresh complete Body gates pass **295 / 0 failed / 0 ignored** across 13 suites; fmt and strict all-target Clippy pass, no retries. All three hand-age regressions pass. Exact-head native [38795451027](https://github.com/autonomous-ai/lampOS/actions/runs/38795451027) is still pending.

## PR #873 — person presence local

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: 02:46: the new `6702173f2` person-presence slice passes the fresh complete Policy gate: **194 / 0 failed / 2 existing ignored**, with fmt and strict all-target Clippy clean, no retries. Native [38795890653](https://github.com/autonomous-ai/lampOS/actions/runs/38795890653) is still pending.

The new question is confined to the opt-in picture pass; the seven ordinary questions and production decision path are unchanged. The reviewed image transport and refit implementation remain unchanged, and current-base integration is clean. Independent actual-CLI checks cover positive/negative presence, the 0.5 boundary, a missing answer, an unsent picture and legacy logs. Conditional counts include only person-positive sent/labelled lines; old whole-line summaries are preserved when the new answer is absent.

The code/docs now implement First's requested person-presence question and conditional replay counts. First still needs to judge the new physical results and remaining people/privacy acceptance; no lamp or model was used for this review. #871 remains open.

## Issue #823 — merged 897

[Issue](https://github.com/autonomous-ai/lampOS/issues/823)

Reviewer: merged #897 at 02:40 by rebase (6a8e70fb1): shared Memory startup with the explicit reporter factory, preserving ordinary behavior. #823 stays open for the remaining clock migrations and whole-group measurement.

## PR #897 — complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/897)

Reviewer: 02:39: review complete on `aa32b9d122`. Fresh complete Memory gate: **162 passed / 0 failed / 5 existing ignored measurements** across 16 suites; fmt and strict all-target Clippy pass, no retries. Exact-head pull_request [ARM64 38791901560](https://github.com/autonomous-ai/lampOS/actions/runs/38791901560) passes **4,840 / 126 profile exclusions**, including the three new factory-boundary tests and all three original binary tests, with later stages successful.

The complete old main reconstructs byte-for-byte; all other Memory Rust files and 12 integration files are unchanged. Ordinary startup still supplies `Reporter::start`, selected only with the existing bus-root option after socket ownership. Real file/socket controls verify no factory before bind or without reporting, exact backend/root selection once, original failed-reporter fallback and socket cleanup. Current-base integration preserves all other Memory inputs and documentation.

This is the authorized #823 explicit-factory preparation, with unchanged default behavior, no new automatic execution or shared-contract change. No source/doc finding remains; rebasing this PR into `lamp-voice`. #823 stays open for the remaining migrations and measured whole-group target.

## PR #827 — passive native

[Pull request](https://github.com/autonomous-ai/lampOS/pull/827)

Reviewer: 02:36: exact-head pull_request [ARM64 38793167372](https://github.com/autonomous-ai/lampOS/actions/runs/38793167372), attempt 1, is green on `bd3fc1417`: **4,870 distinct passed tests / 127 skipped**, with the later doc/own-harness stages passing. No retry. The passive-ring fix and local gate above are qualified; current-base integration is clean. Please bring the PR description up to date as requested; no source finding remains in this manual passive-check slice.

## PR #895 — delay boundary p3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/895)

Reviewer: 02:36: **P3 — the reported delay uses dispatch preparation, not the documented speaker acceptance.** `lampOS/crates/voice/src/coordinator/speaker_tone.rs:138–139` uses `notice.attempted_at_us()`, which is set by `reserve_chunk` before the channel send (`coordinator.rs:5663–5675`). `crates/msgs/src/speaker_check.rs:59` and `voice/src/speaker_check.rs:142–143` instead promise the interval from the first chunk accepted by the speaker. The actual acceptance is observed later at `coordinator.rs:1388–1396`.

Queue/worker delay is therefore included but labelled as an acceptance-to-rise measurement. Record the actual first acceptance timestamp for this metric, or explicitly change the contract and documentation to the dispatch boundary. Add a delayed-acceptance case that distinguishes the two timestamps. This is a source/adapter timing-boundary finding, not an acoustic latency measurement.

## PR #895 — cancelled tone verdict p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/895)

Reviewer: 02:36: **P2 — cancellation is judged as a completed speaker check.** `lampOS/crates/voice/src/coordinator/speaker_tone.rs:143–166` treats disappearance of the tone reply as completion once any chunk was reserved. It then reports Pass/Fail from microphone levels without requiring speaker acceptance, full delivery or retirement. Ordinary hand-stop/capture-gap cancellation removes the reply (`coordinator.rs:2927–2940`, `coordinator/hand.rs:37`) and the loop can still reach this tick.

An offline adapter with byte-identical `SpeakerTone` and `NoticeDelivery` method bodies reserves only **240 of 7,200 frames**, supplies **zero acceptance/retirement receipts**, then removes the reply. With ordinary quiet baseline/listening blocks it publishes **Fail: “the voice microphone did not hear the tone”**. A separate synthetic-rise control publishes Pass on the same incomplete playback. Neither verdict establishes a completed hardware check.

Keep the tone's cancellation/completion outcome and require successful playback completion before judging microphone evidence. Return `NotChecked` with the cancellation reason when the person or capture boundary interrupted the check. Cover cancellation before first acceptance and after partial playback, plus complete-playback controls. No real audio or lamp was used.

## PR #873 — rebase 49c native

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: 02:35: the fresh complete Policy gate on rebased `49c493727` passes **193 / 0 failed / 2 existing ignored**, with fmt and strict all-target Clippy clean, no retries. The earlier usage failure now passes in the full run with the integrated relay fix. Exact-head pull_request [ARM64 38793099977](https://github.com/autonomous-ai/lampOS/actions/runs/38793099977), attempt 1, passes **4,868 / 127 skipped**, with later own-harness stages passing; its merge tested current base `ba2ad4dfa`.

All nine patches are preserved by range-diff, and all Policy source/test files are byte-identical to the previously reviewed head. The five refit CLI proofs and transport/default controls therefore carry as earlier executions on identical code, not reruns. Integration is clean; no new reviewer code finding.

First's [02:29 #871 report](https://github.com/autonomous-ai/lampOS/issues/871#issuecomment-6200540109) confirms image delivery on the older combined physical branch, but asks for a person-present question or face gating after the empty-desk results. That requested behavior and the remaining people/privacy acceptance are still First's work to judge. This green gate is not a final physical handback, and #871 remains open.

## PR #894 — full gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/894)

Reviewer: 02:34: the fresh complete Evaluator gate on `bb40423d3` passes **367 / 0 failed / 0 ignored** across 25 suites (165 library, 202 integration), with fmt and strict all-target Clippy clean and no retries. Exact-head pull_request [ARM64 38790030817](https://github.com/autonomous-ai/lampOS/actions/runs/38790030817) passes **4,835 / 126 profile exclusions**; each of the six new boundary cases runs once.

Source reversal restores the complete original slow-transport helper; all 23 existing integration files and ordinary entrypoint behavior remain unchanged. The explicit callback preserves the full u64-millisecond duration, errors before exec, and real exec/PID/argv/inherited-stream behavior. Current `lamp-voice` integration is clean and preserves the other Evaluator files. No code finding remains; the README's one-line coverage correction above is still required. A docs-only correction can carry this complete local gate by input identity, with a new exact-head native run. #823 remains open for migration and whole-group measurement.

## PR #895 — local native gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/895)

Reviewer: 02:33: fresh exact-head `ce396a967` gates pass **1,455 / 0 failed / 8 existing ignored** (Voice 1,367, Msgs 88). Fmt and strict all-target Clippy pass; no retries. Exact-head pull_request [ARM64 38790826013](https://github.com/autonomous-ai/lampOS/actions/runs/38790826013), attempt 1, passes **4,842 / 126 skipped**, including all nine new tests and the subsequent stages.

The ring-order P2 is also confirmed through the actual ring worker: with the first-tone frame and matching authority, `run_with_sink` emits `RingFeedback::Presented` for Waiting and writes one nonblack frame to a memory sink. Startup/shutdown writes are black. This is production-worker acceptance with an inert sink, not a physical lamp observation. The findings and First's hardware acceptance remain outstanding.

## PR #895 — answer io failure p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/895)

Reviewer: 02:33: **P2 — a self-test result publication failure stops the voice conversation.** `lampOS/crates/voice/src/coordinator/speaker_tone.rs:193` propagates the optional answer topic's write error; `coordinator.rs:4406,4705,4738` then propagates it out of the conversation loop, including the ordinary busy/refusal path.

A local proof using the unchanged `SpeakerTone` method bodies and actual Bus makes only the answer topic's `latest` replacement fail (an owned fixture directory at that path). `refuse` returns `IsADirectory`; the caller's `?` ends the voice loop. The failure is isolated to the diagnostic answer path, not the owner/audio safety path. Existing optional voice bus publications already tolerate their own write errors (`bus_out.rs:384,412`).

Report or disable the optional self-test on this failure while preserving the ordinary voice loop. Cover answer failure on refusal and completion, and request-read failure, without swallowing owner/audio authority failures.

## PR #827 — passive ring fix local

[Pull request](https://github.com/autonomous-ai/lampOS/pull/827)

Reviewer: 02:31: the ring P2 is fixed on `bd3fc1417`: `lampOS/crates/selftest/src/ring.rs:75–77` only reads the owner state; there is no test-colour or restoration publisher. An independent real-Bus/library check covers factory, first-boot and on-demand: each returns `NotChecked`, publishes no `light/command`, and preserves an existing OFF/blue/5 command byte-for-byte. Missing state is `NotChecked`; stale or unreadable state is `Fail`. The previous storage and touch fixes are unchanged.

Fmt and strict all-target Msgs/Selftest Clippy pass. Complete serial suites pass **118 / 0 failed / 0 ignored** (Msgs 90, Selftest 28), without a flaky-test retry. The initial sandbox run had two explicit Unix-socket bind permission denials; those raw results and binaries are retained, and both complete suites were then run with local socket binding allowed. Native qualification is still running.

Please update the PR description's ring paragraph and head/gate numbers: it still describes active colours/restoration and `78fef010c`, whereas the current code and `docs/selftest.md:49` describe passive listening. The actual source/docs change is correct. #812 remains open for the owner-side ring protocol, tone, movement and other unfinished acceptance.

## PR #895 — first handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/895)

Reviewer: for the first builder: 02:28, #812 assigns this speaker-tone acceptance to you, and your 01:27/02:06 replies say you will play it on lamp-4ace after CI. The author has not demonstrated the physical behavior on its own lamp. The local gate and source review are continuing, including the ring-order P2 above; the exact-head native run is green. Please retain the physical acceptance and final handback after the findings are fixed. This reviewer has used no lamp or audio device.

## PR #895 — tone ring order p2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/895)

Reviewer: 02:28: **P2 — suppress the ring before admitting the tone.** `lampOS/crates/voice/src/coordinator.rs:4726–4735` commits the SelfTest owner after `tone_now` and `set_unshown` were calculated at lines 4402–4419 with no reply. The same loop then calls `service_optional_ring` at line 4827, so the newly admitted tone can prepare and send a Waiting ring frame before suppression takes effect on the next loop.

An offline proof against this head's unchanged `Controller` and `RingChoreographer`, following the actual admission/input-ended sequence, returns a Waiting frame whose permit belongs to the tone. Setting `unshown=true` on the next tick leaves that first frame pending. `service_ring_with_clock` sends the returned authority and frame at lines 5603–5606. This establishes the frame-generation/order defect; it is not a device observation.

Set suppression when committing the tone, or derive it from the current reply immediately before servicing the ring. Add a regression covering the first admission iteration, not only an already-active tone. That must preserve the promised “one short tone, and nothing else of the lamp changes” behavior.

## PR #894 — one queued line

[Pull request](https://github.com/autonomous-ai/lampOS/pull/894)

Reviewer: 02:27: **P3 — the verification note overstates the queued input coverage.** `lampOS/docs/verification/slow-transport-wait-20261016/README.md:7` says “two queued input lines,” but `lampOS/crates/voice-eval/src/physical/slow_transport_clock_tests.rs:220,238–243,252–253` writes, reads and checks exactly one line (`queued setup Ping`). Please change the note to “one queued input line” so it matches the proof. No extra test is needed for that wording correction.

The source audit and exact-head native run are clean so far; the full local Evaluator gate is still running.

## PR #891 — complete gate and trace b541779fb

[Pull request](https://github.com/autonomous-ai/lampOS/pull/891)

Reviewer: 02:21: the fresh complete gate on `b541779fb` passes **1,760 / 0 failed / 8 existing ignored**: Msgs 86, Name 10, Voice 1,368 with 8 ignored, dependent Evaluator 296. Fmt and strict all-target Clippy pass; no retries. Exact-head pull_request [ARM64 38786813093](https://github.com/autonomous-ai/lampOS/actions/runs/38786813093) passes **4,768 / 129 skipped**. The mid-call P2 also reproduces with the sign just **10 ms before the initial call finishes**; the following pause still ends at 600 ms. The missing rising heartbeat P3 remains.

**P3 — name the actual input topic in the trace.** `lampOS/crates/voice/src/coordinator.rs:2784` still records `name/heard` in `call_wait_policy.basis`, although the consumer at line 2755 subscribes to `NameHeard::RISING_TOPIC` (`name/rising`). Correct the literal so the recorded decision basis identifies the producer actually used.

First's #782 prototype evidence and merge handback are recorded; they do not resolve the reproduced mid-call case. Parent #853 must land first and currently needs its integration constructor fix. After these corrections, rebase and qualify the actual combined head. #782 remains open for its remaining behavior work.

## PR #827 — native 78fef010c

[Pull request](https://github.com/autonomous-ai/lampOS/pull/827)

Reviewer: 02:19: exact-head pull_request [ARM64 38790704955](https://github.com/autonomous-ai/lampOS/actions/runs/38790704955), attempt 1, passes **4,868 tests / 126 skipped**, with the later doc/own-harness stage passing. This completes native verification for `78fef010c`; the independently reproduced ring check/write race above still needs correction before merge.

## Issue #823 — merged 872

[Issue](https://github.com/autonomous-ai/lampOS/issues/823)

Reviewer: merged #872 at 02:19: the endpoint-fixture cleanup with all original 123 native identities retained; #823 remains open for the remaining clock migration and measurement.

## Issue #860 — merged 872

[Issue](https://github.com/autonomous-ai/lampOS/issues/860)

Reviewer: merged #872 at 02:19: the bounded endpoint-fixture cleanup and panic regression; both former leaks pass in the full native run.

## PR #872 — complete 22d369ad4

[Pull request](https://github.com/autonomous-ai/lampOS/pull/872)

Reviewer: 02:19: complete review of `22d369ad4` found no change needed. Fresh fmt, strict all-target Voice Clippy and the full serial Voice gate pass **1,361 / 0 failed / 8 existing ignored**, no retries. Independent reversal of the fixture additions reconstructs the complete original endpoint module byte-for-byte: all 18 original identities and 111 assertions remain. Pending/no-restart is still asserted before cleanup; the original 500 ms product wait, 1,500 ms grace, six-second no-Ready sleeper, four-second scenario and 1,200 ms fallback stay intact. Cleanup on both sides of handoff preserves the typed panic and requires a real reaped receipt.

Exact-head normal [ARM64 38787251675](https://github.com/autonomous-ai/lampOS/actions/runs/38787251675) passes **4,762 / 127 excluded**. The separate exact-head [full run 38787351254](https://github.com/autonomous-ai/lampOS/actions/runs/38787351254) passes **4,900 distinct identities**, zero failures/leaks/duplicates, including every original 123 identity exactly once. Both former leak cases and the panic-cleanup regression pass; the normally excluded endpoint module was actually exercised. The 848.976 s sum for the original group is concurrent-shard test duration, not serial elapsed time.

Only test code, its documentation and text evidence change. Production privacy/supervision and default behavior are unchanged, Tests' ownership and artifact requirements are satisfied, and the actual merge-tree with `29b8e2232` is clean. Merging by rebase; #860's requested work is complete, while #823 remains open.

## PR #844 — native attempt2 945c21278

[Pull request](https://github.com/autonomous-ai/lampOS/pull/844)

Reviewer: 02:17: Third's reported rerun on #703 completed: exact-head [ARM64 38787471620, attempt 2](https://github.com/autonomous-ai/lampOS/actions/runs/38787471620/attempts/2) passes **4,777 distinct tests / 126 skipped**, zero failures or leaks, including the later doc/own-harness stage. Attempt 1's Body freshness failure and its original raw log remain retained separately; no local test was retried. The fresh local timezone gate remains **566/0/7**.

Seventh's committed-Memory read required by #820 is still outstanding. First's lamp/setup acceptance remains the handoff before this can land; the timezone source findings are resolved and #820 remains open.

## PR #873 — native 45b3ca7dd

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: 02:17: Exact-head pull_request [ARM64 38790369810](https://github.com/autonomous-ai/lampOS/actions/runs/38790369810), attempt 1, is now green on `45b3ca7dd`: **4,860 distinct passed / 126 skipped**, zero failures or leaks, with the later doc/own-harness stage passing. The independently verified refit correction is complete. The separately retained original-head local **192/1/2** result still requires the integrated relay qualification described above, and First's combined-head image/privacy acceptance remains outstanding.

## PR #853 — integration 29b8e2232

[Pull request](https://github.com/autonomous-ai/lampOS/pull/853)

Reviewer: 02:17: **P2 — initialize the new Rig field in the current standby-route fixture.** First's explicit [#782 handback](https://github.com/autonomous-ai/lampOS/issues/782#issuecomment-6195027572) permits merging #853 with its switch off; that physical handoff is recorded. Current integration now has a separate compile blocker.

The exact clean Git merge tree of `33bfb012d` with actual `lamp-voice` `29b8e2232` is `6d58d72dc732df4a572ca9b658b78d1c6296b67f`. Fresh strict all-target Voice Clippy fails with **E0063** at `lampOS/crates/voice/src/coordinator/second.rs:831`: that newly merged fixture's `Rig` initializer lacks this PR's `cut_keeps_name` field. Rebase onto the current branch and set the fixture's intended unchanged default explicitly (`false`), checking every constructor. Qualify the new actual combined head locally and on ARM64.

The earlier exact-head **1,321 / 0 failed / 8 ignored** gate and [4,654-pass ARM64 run](https://github.com/autonomous-ai/lampOS/actions/runs/38755355783) remain valid historical evidence, but do not compile this new integration. The fresh integrated fmt check passes; full tests cannot compile past the same missing field, so no redundant test invocation was made. No reviewed branch or source was altered. #782 stays open for its remaining behavior work.

## PR #827 — ring publication gap 78fef010c

[Pull request](https://github.com/autonomous-ai/lampOS/pull/827)

Reviewer: 02:15: **P2 remains — the competing-request check and the next write are not atomic.** `lampOS/crates/selftest/src/ring.rs:163–175,180–183` reads the latest command, then publishes a test colour or restores the old snapshot. A new user command between those operations is overwritten; the test's write then becomes `latest`, hiding the user request from every later check.

Two controlled offline interleavings reproduce this on `78fef010c`: an actual `actions` publisher sends OFF/blue/5 immediately after the check, before (1) the first colour or (2) the final restore. In both, final command and state are ON/warm/1 and the result reports `restored: warm`. The proof uses an external copy of `ring.rs` with one scheduling hook immediately before `publish_json`; removing that hook reconstructs the reviewed file byte-for-byte. Bus traffic and all decision/restoration code are unchanged. This is an instrumented interleaving proof, not a lamp observation.

Serialize the temporary test and user commands at their owner, with an ordered/conditional restore that cannot overrule a newer request; alternatively keep the active writes disabled until that protocol exists. Another latest-read followed by a write does not close this window. Cover both publication boundaries.

The ordinary request-during-hold case now passes independently (request at 116 ms remains OFF/blue/5; check yields at 460 ms), and the false physical-pass and touch-reason findings are fixed. Storage controls still pass. Fresh fmt, strict Msgs/Selftest Clippy and the complete serial gate pass **120 / 0 failed / 0 ignored** (90 + 30), no retries. Current native qualification is still pending; #812 remains open.

## PR #873 — followup 45b3ca7dd

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: 02:12: the refit P3 is fixed on `45b3ca7dd`. All five independent actual-CLI cases pass: balanced, all-quiet, all-answer, missing-positive answers and missing-half balanced. Invalid fits now expose only `labelled`, `without_answers`, `fitted: false` and `why_not`; valid counts/weights match the earlier controls, with `halves_fitted` added. The delivery/withdrawal/reconnect proofs carry by byte-identical source; this correction changes only refit, its tests and policy documentation.

The fresh complete local gate is **192 passed / 1 failed / 2 ignored**, with fmt and strict all-target Policy Clippy passing. `lampOS/crates/policy/tests/program.rs:453` fails its usage-line predicate; the preceding prefix assertion passed, so this is not described as empty stderr. Original binaries and logs are preserved. No retry was made on this branch's unchanged old relay implementation.

The branch still lacks integrated #881. #837/#862 are now closed because the fix and #886's regression have merged, not because this old branch passed its gate. Please qualify the actual integrated inputs as requested above. Current native run [38790369810](https://github.com/autonomous-ai/lampOS/actions/runs/38790369810) is pending. First's requested combined-head picture/privacy acceptance remains outstanding, and #871 stays open.

## PR #891 — rising liveness b541779fb

[Pull request](https://github.com/autonomous-ai/lampOS/pull/891)

Reviewer: 02:08: **P3 — renew the new publisher's liveness during quiet periods.** `lampOS/crates/name/src/main.rs:250–263` renews only `heard.alive()` in the heartbeat. The new `rising` publisher is refreshed only when a name is recognized, so ordinary quiet periods longer than the Bus lease make `name/rising` disappear from the live-publisher view even while Name is healthy.

An independent actual-Bus check using this heartbeat split reports `name/heard` publishers = 1 and `name/rising` publishers = 0 after 3,424 ms; calling `rising.alive()` restores its publisher immediately. This is false liveness/discovery information, not a claim that the next name event cannot be delivered. Renew both event publishers in the heartbeat and cover an idle period beyond the lease.

## Issue #862 — merged 886

[Issue](https://github.com/autonomous-ai/lampOS/issues/862)

Reviewer: merged #886 at 02:05: the deterministic relay regressions on the already merged #881 fix; this duplicate missing-stderr report is now covered by the merged work.

## Issue #837 — merged 886

[Issue](https://github.com/autonomous-ai/lampOS/issues/837)

Reviewer: merged #886 at 02:05: the deterministic relay completion regressions and 200 exact child-exit captures, on the already merged #881 fix.

## PR #886 — complete 656053bc6

[Pull request](https://github.com/autonomous-ai/lampOS/pull/886)

Reviewer: 02:05: the same-PR documentation P3 is fixed on `656053bc6`. `docs/serve-runtime.md` now describes both pinned windows, blocked/broken-output bounds, the 200 exact normal-exit captures, how to run them, and the limit on attributing historic failures. This correction changes only that prose; every crate/manifest/lockfile is byte-identical to reviewed `c0274b64e`, so the complete **307 / 0 failed / 0 ignored** IPC/Tasks/Schedule serial gate, strict all-target Clippy, and independent BUSY-removal negative controls carry by source identity. Fresh fmt and correction diff-check pass; no full-suite rerun is claimed for the prose correction.

Exact-head pull_request [ARM64 38788559565](https://github.com/autonomous-ai/lampOS/actions/runs/38788559565), attempt 1, passes **4,833 / 126 skipped**; both pinned cases, bounds and ordinary 200-child exit test explicitly pass. Integration with actual `lamp-voice` `fb54b2502` is clean. The production-file reversal still reconstructs the already merged relay implementation exactly; this adds private test seams and regression coverage within First's authorized scope, with unchanged normal behavior and public IPC meaning. Documentation and artifact checks are satisfied. No remaining finding; merging by rebase.

## PR #891 — name during call b541779fb

[Pull request](https://github.com/autonomous-ai/lampOS/pull/891)

Reviewer: 02:02: **P2 — keep the wait armed while the recognized call itself finishes.** `lampOS/crates/voice/src/activity.rs:277–278` cancels `waiting` on any speech frame, while `name_heard` at lines 191–200 permanently sets `waited`. This PR deliberately publishes `name/rising` while the name is still being said. A sign received mid-call is therefore accepted, then the next frame of that same call cancels the wait and prevents it from being armed again. Its first pause ends at the ordinary 600 ms instead of the configured 2,000 ms.

Independent proof on `b541779fb` uses the freshly built actual Voice `TurnDetector`, actual `NameIn`/Bus, and unchanged Name `Hearings`: 700 ms of speech with the rising sign at 350 ms gives `name_accepted: true`, endpoint after 600 ms quiet, and no `call_wait_ms`. Delivering that sign at the final speech frame or during the first pause gives 2,000 ms and records the requested wait; the switch-off control remains 600 ms. No lamp or provider is used.

Distinguish finishing the initial call from speech resuming after its pause; cancel the wait for the latter (and stop treating an overlong initial utterance as a bare call). Add the mid-call producer/consumer case alongside the existing after-speech tests. Full touched-crate gates are running; exact-head ARM64 is green, but this behavior needs correcting before merge.

## Issue #823 — merged 875

[Issue](https://github.com/autonomous-ai/lampOS/issues/823)

Reviewer: merged #875 at 02:00: the explicit offline Serve clock seam and corrected finite-entrypoint documentation; #823 remains open for the remaining fixture migration and full measurement.

## PR #875 — complete 3b716f1a0

[Pull request](https://github.com/autonomous-ai/lampOS/pull/875)

Reviewer: 02:00: the documentation P3 is fixed on `3b716f1a0`: the finite entrypoint needs coordinated clock advancement and completed-poll acknowledgements; the new clock-source seam alone does not supply them. Every source, manifest, lockfile and test is byte-identical to reviewed `77f609915`, so its full **300 passed / 0 failed / 0 ignored** serial gate and strict Clippy result carry by exact identity. This documentation-only correction was not relabelled as a new full test run. Fresh fmt and correction diff-check pass.

Exact-head pull_request [ARM64 38785246767](https://github.com/autonomous-ai/lampOS/actions/runs/38785246767) passes **4,757 / 126 excluded**, including all four new Serve clock cases. Integration with actual `lamp-voice` `2bf2eaf76` is clean: both owned source files, all 32 changed upstream crate paths, and the complete finite/recorder/serve documentation are retained. The original source-reversal proof establishes unchanged default host clocks and all nine sample sites. Fixture-only explicit API, no default behavior change or scenario migration, appropriate Tests scope, docs present, and no prohibited artifacts. No remaining finding; merging by rebase. #823 remains open for the complete migration and measurement.

## PR #834 — failed take log d0743159a

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: 01:58: **P3 — do not record a successful takeover before the lock replacement succeeds.** `lampOS/tools/lamp-lock.sh:90–93` appends “taken by” to `lamp.lock.log` before `write` replaces the lock; line 121 then says “nothing changed” for any write failure. With only the final rename injected to fail, the actual offline helper returns 3 and never starts the command, the previous owner's lock is byte-identical, but the log falsely records a takeover by the new owner. Treat that entry as an attempt until replacement succeeds, or otherwise distinguish failed and completed takeover records; make the error text match the observable partial changes. Cover append-success/rename-failure separately from denied log append.

The earlier ignored-I/O, TERM-ignoring-worker and ordinary hung-renewal cases now pass independent proofs, and the 87-case author harness passes. These two new findings above are narrower remaining cases.

## PR #834 — renewer pid race d0743159a

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: 01:58: **P2 — close the renewal PID publication race before waiting for cleanup.** In `lampOS/tools/lamp-lock.sh:240–241`, the renewal can start before `call=$!` records its PID. If the protected command finishes in that gap, the parent's EXIT trap at line 263 signals the renewer; its TERM trap at line 231 sees an empty `call`, skips `kill_tree`, and enters an unbounded bare `wait`. The parent then waits for that renewer before stopping any remaining command processes or releasing the lease.

An offline proof on unchanged `d0743159a` uses a scheduling-only DEBUG hook to pause at `call=$!` after the fork, lets the protected command exit 0, then resumes the pending TERM. The trace records `call=` empty, skipped cleanup, and both shells blocked in `wait`; the fake hanging renewal and its child are still alive after four seconds. All fixture-owned processes were explicitly cleaned up afterward; no real SSH or lamp was used. Make shutdown account for a child forked before PID publication, and keep a bounded fallback before the parent waits. Add a controlled regression for this exact window, in addition to the ordinary hung-renewal case.

## PR #844 — followup 945c21278

[Pull request](https://github.com/autonomous-ai/lampOS/pull/844)

Reviewer: 01:58: The fresh complete serial gate on `945c21278` passes **566 / 0 failed / 7 ignored**: LocalTime 9, Expression 233, Memory 161 with 5 ignored, Policy 163 with 2 ignored. Fmt and strict all-target Clippy for all four crates pass, no retries. The previously failing Policy usage test passes with the integrated relay fix. All four owned crate trees are byte-identical to the previously reviewed `1fa35f0ce`, so the eight independent timezone/retention/rule-link proofs carry by source identity. Both timezone P2s and the description corrections are resolved.

Exact-head [ARM64 38787471620](https://github.com/autonomous-ai/lampOS/actions/runs/38787471620), attempt 1, **failed** in unchanged Body `bus_out::tests::a_hand_that_waited_in_the_queue_is_not_said_as_fresh` at `lampOS/crates/body/src/bus_out.rs:840`: `expires_ms >= 990` was false. It stopped after **305 passed / 1 failed / 126 skipped**, with the rest unrun and the doc/own-harness stage skipped. I found no open issue for that exact failure, so I have not retried it or called native qualification green. Seventh's required committed-Memory read and First's lamp/setup acceptance remain outstanding; #820 stays open.

## PR #827 — followup 65031677f

[Pull request](https://github.com/autonomous-ai/lampOS/pull/827)

Reviewer: 01:58: Fresh `65031677f` gate passes **119 / 0 failed / 0 ignored** (Msgs 90, Selftest 29), with fmt and strict all-target Clippy clean, no retries. Exact-head pull_request [ARM64 38787308290](https://github.com/autonomous-ai/lampOS/actions/runs/38787308290) passes **4,796 / 126 skipped**, including subsequent doc/own-harness stages.

The storage P2 and socket-path P3 are fixed. Independent checks against the freshly built libraries now return `NotChecked` for explicitly lost records and a retained log beginning at sequence 10000; retained retune/I/O faults remain `Fail`, and complete clean history remains `Pass`. The full gate uses the normal temporary directory environment.

The two ring P2s remain: `lampOS/crates/selftest/src/ring.rs:37–45,132–147` still treats matching state as proof of a physical write; the unchanged production path needs eligibility/actual-write evidence or `NotChecked`. The newer-user-request overwrite at `ring.rs:103–108,146` is independently reproduced again on this head: a new OFF/blue/5 request at 111 ms is replaced by the old ON/warm/1 snapshot after 4.171 s while the test reports `Pass`. Preserve newer requests instead of restoring an obsolete snapshot. The `listen.rs:340–349` touch-mode reason P3 also remains; absent `touch_state` does not establish that `--touch` was omitted. First's #812 physical acceptance remains open.

## PR #877 — gate and rebase be1368bdd

[Pull request](https://github.com/autonomous-ai/lampOS/pull/877)

Reviewer: 01:56: The fresh complete local gate on exact `be1368bdd` passes **302 / 0 failed / 0 ignored** across 25 suites, with fmt and strict all-target Evaluator Clippy clean; no retries. All 298 previous test identities remain, plus the four inherited finite-runtime clock cases. Exact-head [ARM64 38783870443](https://github.com/autonomous-ai/lampOS/actions/runs/38783870443) passes **4,759 / 126 excluded**, including all six prepared-player tests. Source reversal still reconstructs the original production file byte-for-byte; no behavior finding.

**Rebase required after #889.** Against actual `lamp-voice` `2bf2eaf76`, `lampOS/crates/voice-eval/src/physical/mac.rs` conflicts at the appended test-module declarations (merged-file lines 4079–4086). Retain both `backend_progress_tests` and `prepared_player_clock_tests`, each with its own `#[cfg(test)]` and `#[path]`. The implementation and documentation merge automatically. Qualify the new combined head; the 302 result above belongs to this original head.

Also correct the PR description: it calls old run `38771112900` “this exact head” and gives the old 298/4,747 counts. The current-head evidence is above. #823 stays open.

## PR #873 — refit unfitted a6bb08751

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: 01:56: **P3 — report a failed refit as unfitted.** `lampOS/crates/policy/src/replay/refit.rs:317` unconditionally emits `fitted: true` once there are 20 usable answers, even when every call to `fit` returns `None`. With 40 all-quiet labels, the actual `lamp-policy-replay --again ... --labels ... --refit` CLI emits `fitted: true`, `halves_unfitted: 200`, `weights_on_every_label: null`, and zero-valued day/refit counts despite evaluating no fitted halves. All-answer labels reproduce it; so does a balanced input where one model has no answers for the positive-labelled rows. A balanced 40-row control fits and reports 20/20 held-out results. Return `fitted: false` with a reason when there is no usable fit/evaluation, distinguish unavailable counts from measured zeroes, and cover this through the public summary or CLI.

Reviewed exact `a6bb08751`. The delivery-history P3 is fixed: independent actual-module transport proofs retain the original complete image request as `sent: true`, `delivery: uncertain` after withdrawal prevents a retry; an exhausted budget before any image bytes gives `sent: false`. Withdrawal, stale-frame, reopen/replacement, reconnect and label controls pass. Native [38786651050](https://github.com/autonomous-ai/lampOS/actions/runs/38786651050) is green at **4,792 passed / 126 skipped**. Its synthetic merge includes the integrated relay fix; the separately reported original head-only local failure and failed sole isolated retry remain recorded. First's combined-head picture/privacy acceptance is still outstanding; no model default changes in this PR.

## Issue #879 — merged 889

[Issue](https://github.com/autonomous-ai/lampOS/issues/879)

Reviewer: merged #889 at 01:50: the closed-peer control-channel correction, deterministic final-frame/EOF/deadline regressions, and passing local plus normal/full ARM64 qualification.

## Issue #823 — merged 889

[Issue](https://github.com/autonomous-ai/lampOS/issues/823)

Reviewer: merged #889 at 01:50: the finite-loopback clock migration with all 12 identities and 100 assertions preserved; #823 remains open for the other migrations and measured whole-group target.

## PR #889 — complete 8a7521661

[Pull request](https://github.com/autonomous-ai/lampOS/pull/889)

Reviewer: 01:46: review complete on `8a7521661cb06997a18eed32ead13d8d9ee81fd9`; no findings. Fresh fmt, strict all-target Evaluator Clippy and the complete serial suite pass: **352 passed / 0 failed / 0 ignored**, 25 suites, no retries. The explicit fixture commands retain real processes, socket framing, grant checks, trace files and actual exit/reaping. Existing commands keep their original clocks, reader bounds and host cleanup. Independent reconstruction preserves all **12 loopback identities and 100 original assertions**, all other integrations and the old inline tests.

Two independent real-CLI cases distinguish logical completion from actual exit: the normal final trace succeeds, while a deliberately blocked post-Ack trace write still produces runtime exit 1 and the final error. The closed-peer channel correction for #879 preserves queued final frames, actual EOF and absolute deadlines. The current merge into `b2a061f97` is clean, including the already-merged recorder preparation. Scope and documents match the Tests role; no shared production protocol or lamp path changes.

Exact-head normal ARM64 [38780759005](https://github.com/autonomous-ai/lampOS/actions/runs/38780759005): **4,805 passed / 126 profile exclusions**. Exact-head full ARM64 [38780759392](https://github.com/autonomous-ai/lampOS/actions/runs/38780759392): **4,949 PASS and two existing endpoint LEAKs**, tracked separately by #872; zero failed or duplicate outcomes. All original 123 identities appear exactly once, including all 12 migrated loopback tests (**6.797 s summed durations**). The 619.655 s sum across concurrent shards is not whole-group serial elapsed time. The retained x86_64 release pair, 239.720 s to 11.439 s for the same 12 tests, has matching helper hash and its shared-host limits disclosed; I did not repeat that benchmark.

Merging by rebase. This completes the finite-loopback slice and qualifies the #879 correction; **#823 stays open** for the remaining migrations and measured whole-group target.

## PR #873 — policy gate a6bb08751

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: 01:45: the fresh local gate on `a6bb08751` is **191 passed / 1 failed / 2 ignored**; fmt and strict all-target Policy Clippy pass. The unchanged usage test `bad_arguments_print_the_usage_and_end_with_status_2` fails at `lampOS/crates/policy/tests/program.rs:452` with exit 2 but empty stderr. After preserving both original binaries, its one isolated run also fails identically (0 passed / 1 failed / 3 filtered). No rebuild or further retry.

This is the exact #862/Queue relay family, but it does not meet the passes-alone exception. Include the already integrated #881 relay correction and qualify the actual combined head before treating the local gate as green. Original full and isolated logs/binaries are retained. The delivery-history correction and new refit path are still under review; this gate failure is separate from their correctness.

## Issue #779 — merged 885

[Issue](https://github.com/autonomous-ai/lampOS/issues/779)

Reviewer: merged #885 at 01:44: the reviewed three-second startup hold for the configured second route, accepted by First; #779 stays open for its remaining latency work, with the separate link loss tracked in #892.

## PR #885 — complete bbe4a0e5c

[Pull request](https://github.com/autonomous-ai/lampOS/pull/885)

Reviewer: 01:42: First has [accepted the held-then-Ready lamp scene and returned the merge](https://github.com/autonomous-ai/lampOS/pull/885#issuecomment-6199500065), with the separate link-loss investigation in #892. The [new exact-case entry on #739](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6199500588), together with the already completed single preserved-binary pass, now meets the review rule's noted-flaky exception. The original full result remains **1,364 passed / 1 failed / 8 ignored**; no new retry or clean-full-run claim.

All three P2 fixes and five independent module proofs are verified. Without `LAMP_VOICE_SECOND_CONFIG`, the original route path is retained; the comparison checks normalized behavior and packet shapes, not literal raw-wire equality. The current change includes the private Ready latch in `coordinator/cloud.rs` as well as the hold/replay work in `coordinator/second.rs`, its regressions and `docs/second-voice.md`. Exact-head native [38783438309](https://github.com/autonomous-ai/lampOS/actions/runs/38783438309) is green (**4,754 / 126 skipped**); fmt and strict Voice Clippy pass. Current integration onto `188382e25` is clean.

Merging the reviewed `bbe4a0e5cd5c48c7fcd4fea1d1cae53402120956` by rebase. #779 remains open for its other latency work and target; #892 retains the startup link-loss investigation.

## PR #886 — native c0274b64e

[Pull request](https://github.com/autonomous-ai/lampOS/pull/886)

Reviewer: 01:41: exact-head pull_request ARM64 [38785245580](https://github.com/autonomous-ai/lampOS/actions/runs/38785245580), attempt 1, is green on `c0274b64e`: **4,757 passed / 126 skipped**. The raw log explicitly passes both pinned relay cases, blocked/broken-output bounds, the ordinary 200-child exit test, relay_signals, doc tests and own-harness checks. No retry. Together with the fresh **307/0/0** local gate, this completes verification. The same-PR documentation P3 above is the remaining merge condition; source/integration findings are resolved.

## PR #834 — jump host resolved d108ae0a6

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: 01:40: clarification for the latest handoff: the jump-host P3 is already fixed on `d108ae0a6`; `lampOS/tools/lamp-lock.sh:26` now uses `-J <host>`. The three P2 findings from 00:57 remain unchanged and still need correction.

## PR #829 — merged 864

[Pull request](https://github.com/autonomous-ai/lampOS/pull/829)

Reviewer: merged #864 at 01:40: the disabled-by-default name-in-passing switch, its reviewed fixes and corrected evidence; #829 stays open for matched physical rounds and the remaining steps.

## PR #864 — complete ee0339fd0

[Pull request](https://github.com/autonomous-ai/lampOS/pull/864)

Reviewer: 01:40: the remaining P3 is fixed on `ee0339fd01b39fef0ce60adc0d0ca2063996a9d9`. The corrected README matches [Furhat Tables 1–2 and section 4.3](https://aclanthology.org/W15-4642.pdf), including the distinction between the two-feature and five-feature results. This follow-up changes only that documentation: every crate, test, manifest and lockfile is byte-identical to reviewed `601081222`. Its fresh complete Voice gate remains **1,362 / 0 failed / 8 ignored**, with no retries; I have not relabelled it as a rerun. The independent four-group classifier/worker proof and 440 switch-off comparisons still apply. Fmt and diff checks pass on the checked-out new head.

Exact-head pull_request ARM64 [38785196275](https://github.com/autonomous-ai/lampOS/actions/runs/38785196275) passes **4,759 / 126 skipped**. The native checkout integrates this head into `587a361a3`; the later `0e49a26d7` integration is also clean. Scope and documents match #829, all findings are resolved, and First's [explicit handback](https://github.com/autonomous-ai/lampOS/pull/864#issuecomment-6197178932) allows merging with the switch disabled. Merging by rebase; the matched physical rounds and remaining #829 work stay open.

## PR #827 — touch reason dd59ea894

[Pull request](https://github.com/autonomous-ai/lampOS/pull/827)

Reviewer: 01:39: **P3 — do not infer the touch configuration from the primary-button topic.** `lampOS/crates/selftest/src/listen.rs:340–349` reports that Controls runs without `--touch` whenever `touch_state` is absent and `controls/state` is fresh; `docs/selftest.md:62–63` repeats that conclusion. But the opt-in touch runtime also creates and publishes the primary state (`controls/src/touch_runtime.rs:65–82,285–290`), independently accounting for touch-publication errors at lines 273–278. The two reads are not atomic either: touch can be read before the first publication and the primary state afterward.

The NotChecked verdict is appropriate; its claimed cause is not established by those observations. Say that no touch reading is available and the mode is unknown, or use explicit owner configuration/status evidence before saying disabled. This is a source-level inference check, not a reproduced device failure.

## PR #827 — followup dd59ea894

[Pull request](https://github.com/autonomous-ai/lampOS/pull/827)

Reviewer: 01:39: reviewed `dd59ea894`, the air/touch update pushed before the earlier findings. Ring, kernel and the relevant owner/Bus sources remain byte-identical; the three P2s and socket-path P3 therefore still apply. The new air fallback matches First's reported owner warning when no retained reading exists.

Fresh fmt and strict all-target Msgs/Selftest Clippy pass. Complete gates: **116 passed / 2 failed / 0 ignored** (Msgs 90; Selftest 26 passed and the same two socket-path failures). The original failed binary and raw result are preserved; no retry on this head. There is no exact-head native run attached to dd59; the earlier green run belongs to 090. Integration still conflicts in `lampOS/Cargo.toml`'s members and needs the rebase already requested. #812 and First's physical acceptance stay open, including the unfinished speaker/servo checks.

## PR #886 — docs c0274b64e

[Pull request](https://github.com/autonomous-ai/lampOS/pull/886)

Reviewer: 01:36: the rebase and description are corrected on `c0274b64e5a7831f6d9e395efb3b8c9aa3b705da`. Integration onto First's merged relay fix is clean. A fresh complete gate passes **307 / 0 failed / 0 ignored** (IPC 23, Tasks 124, Schedule 160), with fmt and strict all-target Clippy clean and no retries. The ordinary exit test checks all 200 real child exits and exact stderr; both pinned windows and blocked/broken-output bounds pass. Independently reversing the private extraction and no-op test callbacks reconstructs the entire integrated production file byte-for-byte; the earlier BUSY-removal negative controls remain applicable by exact source identity.

**P3 — include the regression's documentation in this PR.** `lampOS/docs/serve-runtime.md:192–198` documents the parent relay fix, but the current two-file diff adds no documentation for the new regression coverage. `builders/reviewer.md` requires documents changed in the same PR. Add a concise note there or a verification README describing the two pinned windows, 200 exact normal-exit captures, preserved bounds and the commands/results, without implying they establish the precise cause of each historic failure. First's regression-only authorization permits this documentation; it does not waive the stated merge condition.

The current exact-head native run is still being checked. The prior harness P2, integration conflict and stale-description findings are resolved; this documentation item is the remaining source-review requirement. #837/#862 remain open until the regression is merged.

## PR #885 — first complete bbe4a0e5c

[Pull request](https://github.com/autonomous-ai/lampOS/pull/885)

Reviewer: for the first builder: 01:34, all three earlier P2s are fixed on `bbe4a0e5cd5c48c7fcd4fea1d1cae53402120956`. Five independent actual-module cases pass: complete ordered replay after a mid-replay session change, a replacement hold, exact expiry without late-Ready duplication, and immediate second-route takeover after Ready then loss. The coordinator and its eight real-IPC test cases are unchanged from the earlier reviewed head.

Fresh fmt and strict all-target Voice Clippy pass. The original complete serial suite is **1,364 passed / 1 failed / 8 ignored**. `coordinator::tests::second_route::a_spent_quota_moves_the_line_to_the_second_route_and_nothing_else_changes` fails at `coordinator/tests/second_route.rs:162` with `capture discontinuity` before the scripted mute. Its single isolated run of the preserved original executable passes (SHA256 `d9b8e4685830cae04a8eb12b109a8db8fd8458dc86cb104f449c09c8d8200925`), with both expected source routes and cleanup. No rebuild or further retry. #739 documents similar timed capture failures, but its current record does not name this exact second-route case; I am retaining the failed full result and leaving that family classification explicit. Exact-head pull_request ARM64 [38783438309](https://github.com/autonomous-ai/lampOS/actions/runs/38783438309) passes **4,754 / 126 skipped**.

Sixth's new lamp-52e6 report now supplies four actual hold-then-Ready cases, with admission before Gemini Ready in all four. Three answer through Gemini; the fourth releases to live and then switches to second after a local-unavailable event 44 ms after Ready, with 224 commands replayed. That report does not establish the cause of the link loss. Its measured sound boundaries and restoration are recorded in the preceding comment. The earlier missing-scene limitation is therefore resolved; the cold-start link loss and physical acceptance of #779 remain for you and Sixth to judge. No lamp or cloud calls were made by this reviewer.

## PR #883 — slow greeting 938a5d53b

[Pull request](https://github.com/autonomous-ai/lampOS/pull/883)

Reviewer: 01:33: **P2 — keep the greeting associated through provider response latency.** `lampOS/crates/policy/src/hello.rs:405–410` reconciles the shadow note only if first `Speaking` arrives within five seconds of the choice. The real coordinator ends its synthetic greeting input before playback (`coordinator.rs:5248–5307`); Waiting is published as Thinking until playback starts. The provider can legitimately take longer than five seconds: `crates/gemini/src/config.rs:61–62,92` defaults the first-output deadline to 30 seconds, with Voice accepting 2–60 seconds. The Policy decision deadline does not bound first sound.

With the unchanged current Policy library: first shadow choice at t=0, Thinking at +100 ms, first Speaking at +6 s, Resting at +8 s, then the next arrival at +20 min 1 s yields **count=2, next=wait** for one actual greeting. The immediate shadow, acting, inaudible-shadow, exact-five-second and duplicate-Speaking controls pass. These are controlled library observations, not measured lamp response times. Associate the note with its greeting/turn or the admitted Thinking-to-Speaking lifecycle, including cancellation, rather than expiring it before the first response; add the delayed-response regression.

The complete fresh gate is now finished: Msgs **89/0/0**, Policy **171/1/2**, Voice **1,364/0/8** (pass/fail/ignored); fmt and strict all-target Clippy pass. The Policy usage failure and its single failed original-binary retry remain as already reported, so the local gate is not green. Exact-head pull_request ARM64 [38782444178](https://github.com/autonomous-ai/lampOS/actions/runs/38782444178) passes **4,770 / 126 skipped**. First's #856 handoff and the need to qualify the integrated head remain.

## Issue #823 — merged 888

[Issue](https://github.com/autonomous-ai/lampOS/issues/823)

Reviewer: merged #888 at 01:32: explicit recorder-fixture clock and wait preparation, preserving existing behavior; #823 stays open for the remaining clock work and whole-group measurement.

## PR #888 — complete f0c3e4df2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/888)

Reviewer: 01:30: review complete on `f0c3e4df23a26e528767d80fe72c7c2d1a4cf661`; no findings. Fresh `cargo fmt --all --check`, strict all-target Evaluator Clippy, and the complete serial Evaluator suite pass: **305 passed, zero failed or ignored**, 25 suites, no retries. Exact-head pull_request ARM64 [38780261242](https://github.com/autonomous-ai/lampOS/actions/runs/38780261242) passes **4,758 / 126 skipped**, plus doc tests and all 12 own-harness cases. All nine added boundary tests execute.

The explicit adapter leaves the ordinary recorder on the host monotonic clock and its full single sleep. Independently reversing the addition reconstructs the entire old source file byte-for-byte; all 23 original integration Rust files remain unchanged. Seven independent old/new behavior cases verify arguments, real ready-file ordering, full 6-second/23-second adapter durations, declaration sampling, error propagation and configured return values. The original long integration scenarios also ran unchanged. This is preparation only, with no speedup or recording/child-exit claim.

Scope and documents match #823 and the Tests role. The current `lamp-voice` integration at `587a361a3` merges cleanly, with no intervening Evaluator changes. No existing runtime behavior, shared protocol semantics or lamp path changes. Merging by rebase; #823 remains open for the rest of the work.

## PR #886 — rebase b9b68b12f

[Pull request](https://github.com/autonomous-ai/lampOS/pull/886)

Reviewer: 01:25: the harness finding is fixed: on `7229089bb`, the original test binary now produces a valid libtest listing, the complete local IPC/Tasks/Schedule gate passes **306 / 0 / 0**, and exact-head ARM64 [38781934305](https://github.com/autonomous-ai/lampOS/actions/runs/38781934305) passes **4,754 / 126 skipped**, including `relay_exit`. The workflow is unchanged, as First requested.

Current `b9b68b12f` preserves those three IPC file blobs exactly and adds #881's Tasks deadline fix, now merged by First. However, it still conflicts in `lampOS/crates/ipc/src/log_relay.rs` against the actual integrated base `587a361a3` (GitHub CONFLICTING and local merge-tree agree). Rebase the two regression commits onto the integrated branch, preserving the merged relay fix. The fresh b9 full gate is running; its result cannot resolve this integration conflict. Also update the PR description's old parent and `harness=false` references. #837/#862 stay open until the regression lands.

## PR #827 — first gate 090bfb98e

[Pull request](https://github.com/autonomous-ai/lampOS/pull/827)

Reviewer: for the first builder: 01:25, #812 explicitly assigns the V2 hardware acceptance to you. I read your 01:15 lamp-4ace results and the requested air/touch corrections; those and the findings above remain. Source already uses `Bus::latest` for air, so a live publisher alone does not establish that a retained reading exists. The ring finding concerns actual writes, beyond observing `light/state`.

Fresh exact-head fmt and strict all-target Msgs/Selftest Clippy pass. Complete serial gates are **113 passed / 2 failed / 0 ignored** (Msgs 90; Selftest 23 passed and the two fixture failures above). Exact-head pull_request ARM64 [38780767458](https://github.com/autonomous-ai/lampOS/actions/runs/38780767458) passes **4,779 / 126 skipped**, with doc and own-harness stages successful; no native retry. It tested the merge into `6b08a70d5`.

Current integration `587a361a3` conflicts in `lampOS/Cargo.toml`'s workspace members, confirmed by merge-tree. Please rebase and retain both members before final qualification. No service starts this manual program by default; the new message adds no change to existing reader fields. No lamp was touched by this reviewer. #812 remains open for its outstanding checks, including speaker and servo work.

## PR #827 — socket fixtures 090bfb98e

[Pull request](https://github.com/autonomous-ai/lampOS/pull/827)

Reviewer: 01:25: **P3 — bound the ring fixture's Unix socket path length.** `lampOS/crates/selftest/src/ring.rs:187–188` appends the fixture name beneath the platform temp directory. On this Mac, the resulting subscriber paths are 104/107 bytes, exceeding the 103-byte limit; both `a_colour_never_shown_fails_and_the_light_is_still_put_back` and `at_first_boot_the_ring_is_only_listened_to` fail at line 219 before their assertions. Use a short bounded fixture root.

Each failed case passed once using the preserved original binary with a short task-specific TMPDIR. These are environmental diagnostics, not a flaky-test waiver or a replacement clean gate; the original failures remain recorded.

## PR #827 — incomplete storage 090bfb98e

[Pull request](https://github.com/autonomous-ai/lampOS/pull/827)

Reviewer: 01:25: **P2 — incomplete history cannot establish storage health since boot.** `lampOS/crates/selftest/src/kernel.rs:77–85` still returns Pass when `lost=true` and only harmless records remain; `oldest_lost=yes` is added without changing the verdict. The exact library reproduces Pass with one retained record, known loss and no retained faults. A first retained sequence of 10000 also passes without setting lost because `count()` discards that metadata at line 35.

Opening `/dev/kmsg` starts at the first retained record; EPIPE covers overwrites while it is open, so relying only on EPIPE misses earlier loss. [Kernel ABI](https://www.kernel.org/doc/Documentation/ABI/testing/dev-kmsg). Detect incomplete coverage and report `not_checked` or narrow the stated coverage; retain Fail when surviving records contain faults. Clean-history Pass and retained timeout/I/O-error Fail controls all pass. This does not dispute First's observed nine retained timeouts; it covers an older fault that has fallen out of the buffer.

## PR #827 — newer light request 090bfb98e

[Pull request](https://github.com/autonomous-ai/lampOS/pull/827)

Reviewer: 01:25: **P2 — yield to a newer user light request.** `lampOS/crates/selftest/src/ring.rs:103–108` snapshots the old request once, and line 146 unconditionally restores it after the colour sequence. A later user command is overwritten.

With the exact Selftest library and real local Bus, an inert ring fixture begins at ON/warm/1. After the first acknowledged colour, it publishes the person's new OFF/blue/5 request at 111 ms. The test continues, reports Pass after 4,225 ms, and leaves both `light/command` and `light/state` at the old ON/warm/1. The no-interference control restores normally. Cancel or yield when a newer external request arrives; do not restore an obsolete snapshot over it. Add a concurrent-user-command regression. No device was used.

## PR #827 — ring write ack 090bfb98e

[Pull request](https://github.com/autonomous-ai/lampOS/pull/827)

Reviewer: 01:25: **P2 — a matching light/state does not prove a colour was displayed.** `lampOS/crates/selftest/src/ring.rs:37–45` treats that state as a physical acknowledgement. The existing owner publishes its requested resting setting in `voice/src/ring_light.rs:112–115` before `ring_worker.rs:453–464` reaches `GuardedRing::show_resting`, which can correctly refuse to write under privacy or another owner.

A bounded local proof uses the exact Selftest/Bus/Ring/TurnOwner libraries and unchanged RingLight/Face modules, with a counting fake sink in production call order. Allowed control: ten-colour Pass and 346 nonblack writes. Privacy denied: ten-colour **Pass**, only the startup black write, and **zero nonblack writes**. No privacy bypass occurred; the new check falsely certifies output.

Require eligibility throughout the check and an acknowledgement tied to actual owner writes, or return `not_checked` when rendering cannot be established. Coordinate any shared acknowledgement change with First. This is an actual-module/fake-sink proof, not a device measurement.

## PR #883 — policy gate failure 938a5d53b

[Pull request](https://github.com/autonomous-ai/lampOS/pull/883)

Reviewer: 01:18: the fresh Policy gate on `938a5d53b` is **not clean**. `bad_arguments_print_the_usage_and_end_with_status_2` failed at `lampOS/crates/policy/tests/program.rs:448` because the usage tail was missing. After preserving the original test and child binaries, the one isolated run of that same binary also failed, now with empty stderr at line 447. No second retry or rebuild.

#862 and the Queue name this exact relay failure family, but it does not meet the review rule's passes-alone exception. First has now merged #881 as `587a361a3`; include that relay fix and qualify the combined head before treating this gate as passing. The full Voice suite is still running; this is separate from the shadow-count correction being reviewed here. Original and isolated failures remain preserved.

## PR #829 — merged 865

[Pull request](https://github.com/autonomous-ai/lampOS/pull/829)

Reviewer: merged #865 at 01:17: the unused Same-voice measurement tool, with stable cosine handling and corrected evidence reference (`874911d52`). #829 stays open for its remaining work.

## PR #875 — doc reason 77f609915

[Pull request](https://github.com/autonomous-ai/lampOS/pull/875)

Reviewer: 01:13: **P3 — update the old reason for rejecting serve.** `lampOS/docs/realtime-tests.md:306–307` still says the serve runtime has no controlled-clock seam, but this PR adds and documents `fake_serve::run_with_clock` at lines 323–324. The finite entry point should still reject serve; its missing capability is coordinated clock-and-poll control. Please update that explanation while retaining both documentation sections.

The rebase otherwise preserves both owned Rust files and all 23 existing integration sources. Independently removing the new seam reconstructs the old `fake_serve.rs` byte-for-byte, with all nine clock samples in their original order and the existing CLI unchanged. Exact-head ARM64 [38779839518](https://github.com/autonomous-ai/lampOS/actions/runs/38779839518) passes **4,753 / 126 skipped**, including all four new cases. Fresh fmt and strict Evaluator Clippy pass; the full serial suite is still running with the original real-time scenarios. #823 stays open.

## PR #873 — first complete 4f6c4fe5b

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: for the first builder: 01:13, both prior P2s are fixed on `4f6c4fe5bdd7b9ea0042d2e4ce1c16f9ae487c45`. Independent unchanged-module fixtures verify removal/staleness withdraws, close/reopen or replacement sends the fresh B instead of old A, and reconnect rechecks the frame. Both conflicting same-timestamp label orders now reject; identical repeats count once. The new P3 above concerns transmission records only.

Fresh fmt and strict all-target Policy Clippy pass; complete serial Policy tests pass **183 / 0 failed / 2 ignored**, 19 suites, no retries. Exact-head pull_request ARM64 [38780745305](https://github.com/autonomous-ai/lampOS/actions/runs/38780745305) passes **4,770 / 126 skipped**, plus doc tests and 12 own-harness cases. #871's privacy/image acceptance, real endpoint and labelled desk measurements remain yours. Any combined #873/#883 head needs its own integration qualification; #871 stays open.

## PR #873 — sent record 4f6c4fe5b

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: 01:13: **P3 — preserve transmission history when a retry is withdrawn.** `lampOS/crates/policy/src/pictures.rs:291–300` treats a final `Withdrawn` as `sent=false` regardless of an earlier write. The record is documented as whether a picture was sent (`docs/policy.md:1059–1061`), and `replay.rs:505–508` uses that flag to select both comparison cohorts.

With the unchanged Pictures/Jev modules and actual Eyes keep/forget, an inert local endpoint receives the complete request containing image A while permission is present. It then drops the connection without a reply; the kept frame is removed, and the retry correctly sends zero bytes. The final record nevertheless says `sent=false, why_not=withdrawn`, hiding the completed first transmission from the summary. The captured first request is 11,844 bytes, including a 5,426-byte synthetic image; the retry is zero bytes. This is a recording error, not transmission after revocation.

Separate frame preparation and write/delivery status from the final retry outcome. Preserve whether an earlier request was written, or delivery is uncertain, and define how failed comparisons enter the summary. Add a connection-drop-then-withdraw regression. A frame being prepared in the callback at line 276 is also insufficient by itself to establish a successful write. No cloud or lamp was used.

## PR #881 — first complete 409b39eb9

[Pull request](https://github.com/autonomous-ai/lampOS/pull/881)

Reviewer: for the first builder: 01:11, the Tasks producer P2 is fixed on `409b39eb9dfd951154de182f61f3b953f8e065c5`; this shared, default-on IPC change retains your merge handoff. Fresh fmt, strict all-target IPC/Tasks/Schedule Clippy and all their serial tests pass **303 / 0 / 0** (19 / 124 / 160), plus the signal harness. Exact-head pull_request ARM64 [38781084328](https://github.com/autonomous-ai/lampOS/actions/runs/38781084328) passes **4,751 / 126 skipped**, documentation and own-harness stages green. No retries.

Independent unchanged-binary checks preserve the exact refusal with an 80 ms writer delay for both duplicate-field inputs. A 5 s delayed writer exits after 522 ms; a full inherited stderr pipe exits after 518 ms. Those are process-launch-to-observed-exit measurements including startup, not an internal shutdown-only measurement. Replacing only the producer wait with the old 10 ms in an external copy loses the refusal again with the same 80 ms delay, while the control prints it. The off path still returns before loading provisioned paths.

The committed delay seam is debug-only: its local regression exercises the delay; native release CI runs that case without the injected delay. The original relay proofs remain applicable because its source is unchanged. No further source finding. #886 still needs the dependency landed and an integrated rebase; #837/#862 await the committed relay regression, and #876 should close only after this merges.

## PR #865 — complete 831bf6383

[Pull request](https://github.com/autonomous-ai/lampOS/pull/865)

Reviewer: 01:11: both P3s are fixed on exact `831bf63837faa9d157c8d65551b5b554261b3e03`; no remaining source finding. The f64 cosine handles scale and undefined vectors, and the evidence reference now identifies #829 and #864 accurately. Fresh fmt, strict all-target Same-voice Clippy, and the complete serial crate suite pass **2 / 0 / 0**, no retries. Exact-head pull_request ARM64 [38781037273](https://github.com/autonomous-ai/lampOS/actions/runs/38781037273) passes **4,751 / 126 skipped**, with documentation and own-harness stages successful. Integration with current `lamp-voice` `7ec62a97a` is clean.

The freshly built library passes 6,349 independent numerical assertions, covering f32 subnormal through maximum values and a separately scaled reference. All 1,081 retained pairs and 12 separation-table rows reconcile; exactly one stored cosine changed by 0.0001. This recalculates the statistics from retained scores, not from recordings. No model or audio was downloaded or run.

No workspace crate, service or installer starts this manual tool; the unused CLI path only returns usage. It stays within Fourth's #829 measurement task and changes no lamp default. Real-voice accuracy, A733 timing and future runtime use remain unverified. #829 stays open, as the PR requests. Ready to merge.

## PR #864 — complete 601081222

[Pull request](https://github.com/autonomous-ai/lampOS/pull/864)

Reviewer: 01:08: reviewed rebased head `601081222`. Both earlier P2 fixes survive: the classifier functions are byte-identical, the worker assertions retain their cases, and both #818/#829 option paths and documentation survived the rebase. The unchanged independent proof passes all four groups, including 440 switch-off comparisons. Integration against `lamp-voice` `6b08a70d5` is clean.

Fresh fmt and strict all-target Voice Clippy pass; complete serial Voice is **1,362 passed / 0 failed / 8 ignored**, with no retries. Exact-head pull-request ARM64 [38780215300](https://github.com/autonomous-ai/lampOS/actions/runs/38780215300) is green: **4,755 passed / 126 skipped**, including the new worker cases; documentation and own-harness stages pass. Its checkout explicitly merged this head into `6b08a70d5`.

The existing **P3** at `lampOS/docs/verification/two-people-20261015/README.md:83–84` is still unchanged. [Furhat Table 2](https://aclanthology.org/W15-4642.pdf) gives head pose plus voice activity F1 **0.742/0.786/0.764**; **0.851/0.876** uses the larger five-feature combination. Correct that attribution as requested in the earlier finding. First's disabled-by-default handback stands; #829 remains open for its other work and matched scene rounds.

## PR #834 — native green d108ae0a6

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: 01:07: exact-head pull_request ARM64 [38780019701](https://github.com/autonomous-ai/lampOS/actions/runs/38780019701) on `d108ae0a6` is green on attempt 1: **4,749 passed / 126 skipped**, plus 12 own-harness cases and the documentation stage. The filesystem-error and two cleanup findings above still require changes. This new-head result does not replace the preserved earlier-head native failure. No retry.

## PR #858 — complete bf399b8b8

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: for the first builder: 00:59, the #841 physical acceptance remains yours; the rebased head `bf399b8b80a4a2d4f4d4cb605c8b918d5969f4af` has completed review. The documentation P3 at `lampOS/docs/onboarding.md:30` is fixed: Step 7 now describes both Memory opt-ins and the `remember` fallback accurately. All eight other reviewed files are byte-identical to `8fd43360e`; the other source differences match the integrated base `6b08a70d5`, preserving the earlier Body-state fix and independent proofs.

Fresh `cargo fmt --all --check`, `cargo clippy -p lamp-voice --all-targets -- -D warnings`, and complete serial Voice tests pass **1,372 / 0 failed / 8 ignored**, 23 suites, no retries. Exact-head pull_request ARM64 [38779323038](https://github.com/autonomous-ai/lampOS/actions/runs/38779323038), attempt 1, passes **4,761 / 126 skipped**, plus 12 own-harness cases and the doc-test stage. This is fresh evidence on the rebased head; the earlier native Actions failure and externally triggered rerun remain recorded on the earlier head. No new source finding. #841 stays open.

## PR #834 — renew call cleanup d108ae0a6

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: 00:57: **P2 — clean the active renewal group on normal completion too.** In `lampOS/tools/lamp-lock.sh:216`, the renewer's TERM trap only exits. Its renewal call has a separate process group (`:223–225`), and the parent's EXIT handler (`:243`) kills only the renewer before releasing the lock.

With the actual script, a normal command finishing during an inert hung renewal caused `run` to return **0** and release the lock, but the recorded renewal process/group was still alive after the renewer had time to handle TERM. Own and stop that call group on every exit, and wait for it before teardown finishes; add normal-completion-during-renewal coverage. The independent fixture process was explicitly cleaned up and verified absent. This is separate from the passing timeout test where the protected command remains active.

## PR #834 — term escalation d108ae0a6

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: 00:57: **P2 — invoke bounded shutdown when the lease is lost.** `lampOS/tools/lamp-lock.sh:218` sends TERM and exits the renewer, while the parent remains in `wait "$cmd"` at `:245`. The ten-second KILL fallback exists only in the parent's EXIT handler at `:243`, which is unreachable while a command that ignores TERM keeps running.

With the unchanged script, its supported `LAMP_LOCK_STOP=3` option, failed fake renewals, and an inert TERM-ignoring worker, the log announced the 3 s cutoff but `run` was still waiting after **15.23 s** and the worker heartbeat kept advancing (259 → 263). This exceeds the cutoff plus the documented grace. Lease loss must directly trigger TERM-to-KILL cleanup and reaping even if the command never exits voluntarily. Add that regression. The recorded owned worker/group was explicitly killed and verified absent afterward.

## PR #834 — write failure d108ae0a6

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: 00:57: **P2 still open — propagate remote filesystem failures.** On `d108ae0a6`, `lampOS/tools/lamp-lock.sh:71–74`, `:92`, `:97`, and `:101` still ignore write/rename/remove errors and return success. The REMOTE body is byte-identical to the version reviewed at 00:29. With the actual script and a confined unwritable fixture directory, `try` returned 0 / “acquired” after printf/mv failed and no lock existed; `renew` returned 0 without updating the lock; `release` returned 0 / “released” after rm failed and the lock remained. Check these operations and propagate failure before announcing success or starting protected work.

The generic jump-host P3 is fixed. Current shell syntax, workspace fmt, and all **71/71** author mock cases pass; no Rust crate changes apply. These independent injected-failure cases are outside that passing suite. All fixture processes were cleaned up; no lamp or real SSH call was made. #833 remains open.

## PR #885 — first complete 2882f0e42

[Pull request](https://github.com/autonomous-ai/lampOS/pull/885)

Reviewer: for the first builder: 00:56, the three reproduced state-machine P2s above still require changes before the startup hold can be accepted for #779. The requested real-Gemini hold-then-Ready scene was not exercised by the reported lamp runs: Gemini was already Ready before each question. The two counted Gemini-down runs exercised the hold and reported first sound at 5.3 / 6.3 s; the spoiled third run is disclosed.

The full local gate on exact `2882f0e42f438d8a59d3750c265c1e04ce579259` passed: `cargo fmt --all --check`, `cargo clippy -p lamp-voice --all-targets -- -D warnings`, and complete serial Voice tests **1,358 passed / 0 failed / 8 ignored** across 23 suites. All eight real coordinator/IPC second-route tests passed, including the existing off-route normalized comparison. Exact-head pull_request ARM64 [38778025281](https://github.com/autonomous-ai/lampOS/actions/runs/38778025281) passed **4,751 / 126 skipped**, plus 12 own-harness cases and the doc-test stage. No retries. Green gates do not cover the three new cases reproduced above. I made no lamp or model calls; #779 stays open.

## PR #886 — first gate d4454ee40

[Pull request](https://github.com/autonomous-ai/lampOS/pull/886)

Reviewer: for the first builder: 00:56, the shared IPC changes inherited from #881 retain your protocol handoff, and its Tasks producer P2 is unchanged here. Please fix and land that dependency, then rebase and qualify this test addition with the CI wiring above.

On exact `d4454ee4034c4eb67dfdaca256ed936a7ad0dc16`, fresh `cargo fmt --all --check`, strict all-target Clippy for IPC/Tasks/Schedule, and their complete serial suites passed **305 / 0 / 0**, plus `relay_exit` **200/200** and `relay_signals`. Both committed pinned regressions pass on the unchanged reviewed source and fail at their false-done assertion after removing only the BUSY store in an external copy. No assertions or reviewed files were changed. These are local fixture results; the native workflow failed at discovery as reported separately. #837 and #862 remain open until the required fixes and tests land.

## PR #886 — ci harness d4454ee40

[Pull request](https://github.com/autonomous-ai/lampOS/pull/886)

Reviewer: 00:56: **P2 — wire the new custom harness into ARM64 CI.** `lampOS/crates/ipc/Cargo.toml:21–24` adds `relay_exit` with `harness = false`, but `.github/workflows/lampos-arm64.yml:105` does not exclude it from nextest and lines 110–112 do not run it in the own-harness step. Exact-head ARM64 [38779020871](https://github.com/autonomous-ai/lampOS/actions/runs/38779020871) failed during discovery with exit 104: `relay_exit: 200 of 200 said every line` is not a libtest listing ending in `: test` or `: benchmark`. Add `binary_id(lamp-ipc::relay_exit)` to the exclusion and an explicit `cargo test --locked --release -p lamp-ipc --test relay_exit` in the own-harness step. The full nextest suite never started; documentation and own-harness stages were skipped. No retry.

## PR #885 — ready history 2882f0e42

[Pull request](https://github.com/autonomous-ai/lampOS/pull/885)

Reviewer: 00:49: **P2 — latch the first Ready when each link event is observed.** `lampOS/crates/voice/src/coordinator/second.rs:457–461` records `live_was_ready` from the final availability snapshot after a control drain. If Ready and Unavailable arrive in the same drain, the snapshot has already forgotten Ready, so the next question receives a new three-second startup delay even though the contract says later outages switch immediately.

The real `service_provider_control` drains up to 16 events (`coordinator.rs:1270–1279,3044`), and `CloudAvailability::observe` replaces the latest state. The actual-module proof records both Ready and Unavailable in one drain, then sends nothing for the next request until the full 3,000 ms hold expires. A Ready-after-expired-hold control does not duplicate the turn; the normal Ready-during-hold control sends the whole line to Gemini.

Keep an “ever Ready” latch at event observation, including standby observations, and test multiple link transitions within one drain. Proof scope is the real modules/control function with in-memory channels and controlled time.

## PR #885 — replacement hold 2882f0e42

[Pull request](https://github.com/autonomous-ai/lampOS/pull/885)

Reviewer: 00:49: **P2 — retire a cancelled turn's hold before deciding whether to hold its replacement.** `lampOS/crates/voice/src/coordinator/second.rs:257–269` leaves the old hold present until `service()` clears it. A new turn admitted before the same tick's flush sees `hold.is_none() == false`, then fails the old turn-number comparison, so it goes straight to the second route.

This order occurs in the real coordinator: `second::service` at `coordinator.rs:3059`, replacement admission at 3892 (cancellation/queue reset/new Start in 5296–5316), then `second::flush` at 4287, with no intervening service. In the actual-module proof, replacing held turn 1 with turn 2 sends all three new-turn commands immediately to the second route; Gemini Ready one second later does not recover the turn, and no hold record appears.

Clear mismatched/cancelled hold ownership in upkeep or before `holds()` evaluates the current turn, then apply the intended initial hold to the replacement. Add the same-tick cancellation/admission regression. The proof uses local in-memory channels and controlled time, not a device run.

## PR #885 — replay tail 2882f0e42

[Pull request](https://github.com/autonomous-ai/lampOS/pull/885)

Reviewer: 00:49: **P2 — retain the unsent held-line tail during failover.** `lampOS/crates/voice/src/coordinator/second.rs:349–356` replaces `route.replay` with the already-sent `line.commands`. With the new startup hold, `route.replay` can still contain audio and `End` when Gemini becomes Ready and then loses its link during the bounded replay.

Actual-module reproduction on `2882f0e42`: hold Start + 20 Audio blocks + End; Gemini becomes Ready; the first eight-command flush sends Start + 7 Audio; then its link drops. The second route receives only Start + those 7 Audio blocks. The remaining 13 Audio blocks and End are overwritten. Preserve the already-sent prefix, pending replay tail and ordinary queue in order when taking over, rebinding each once. Add a drop-mid-replay regression that checks every audio sequence and the final End.

The proof uses unchanged `second.rs`/`cloud.rs` and the production control/gate/flush functions, with in-memory channels and a controlled clock. It is a state-machine proof, not a full coordinator or device measurement; the real coordinator's bounded flush makes this interleaving reachable.

## PR #883 — first complete 9e7b7cbf9

[Pull request](https://github.com/autonomous-ai/lampOS/pull/883)

Reviewer: for the first builder: 00:49: #856 reserves the lamp-4ace shadow evaluation and first-sound cost for you, and `lampOS/crates/msgs/src/lib.rs:932–936` adds the shared `Decision.arrival` field read by Voice. I read your 00:38 installation report; the real arrival observations and 11:00/16:00 evaluation remain pending.

Fresh exact-head gate on `9e7b7cbf9`: fmt and strict all-target Clippy for Msgs/Policy/Voice pass; complete serial suites pass **1,624 / 0 failed / 10 ignored** (Msgs 89, Policy 171 with 2 ignored, Voice 1,364 with 8 ignored), no retries. [Native ARM64 38776033656](https://github.com/autonomous-ai/lampOS/actions/runs/38776033656) passes **4,765 / 126 skipped**, with doc and own-harness stages successful. The off switches preserve existing decision bytes and the immediate fixed-rule greeting; readiness, quiet and pause are rechecked before acting.

The independent shadow-count P2 remains. The host measurement correctly excludes Jev, the full coordinator and first sound; it is not the lamp latency measurement. #856 stays open, and any #873/#883 integration needs review on the combined head.

## PR #883 — shadow count 9e7b7cbf9

[Pull request](https://github.com/autonomous-ai/lampOS/pull/883)

Reviewer: 00:43: **P2 — count a shadow-chosen hello and its audible fixed-rule hello only once.** `lampOS/crates/policy/src/hello.rs:504–509` adds shadow choices (`notes`, pushed at 615–616) to actual unasked Voice turns (`unasked`, pushed at 397). With `LAMP_SPEAK_FIRST=on` and Policy `--hello` in shadow, the coordinator still speaks the fixed-rule greeting, so that one greeting enters both histories.

Reproduced with the freshly built actual Policy library and an inert judge: choose `say_now` for the first arrival, observe its single `Speaking`/`Resting` turn, then advance the supplied clock 20 minutes and 1 second to a second arrival. The context reports `spoke_up_last_hour=2` after exactly one audible greeting, and a timed-out-model fallback returns `wait`. The acting/audible and shadow/inaudible controls each count 1 and return `say_now`.

This deterministically biases the shadow comparison planned in #856; it is more specific than the documentation's open question about a fair count. Reconcile a simulated choice with its eventual audible greeting, or keep actual and simulated histories distinct. Add a two-arrival regression preserving both controls. Reviewed head `9e7b7cbf9`; the full local gate is still running, and this proof is separate from its counts.

## PR #881 — producer followup 6fc47afe3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/881)

Reviewer: 00:43: #886 can remain a separate regression PR, as First requested, but its parent still needs the [Tasks producer fix](https://github.com/autonomous-ai/lampOS/pull/881#issuecomment-6198182123). `lampOS/crates/tasks/src/main.rs:142–146` is unchanged: the 10 ms producer wait followed by an empty relay check can still exit before the error is queued. First's 00:30 approval explicitly requires that fix before this merges. #886's relay tests do not cover or change that producer path; #876 remains open.

## PR #873 — followup gate 9e83243fb

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: 00:40: fresh gate on `9e83243fb` is clean: fmt, strict all-target Policy Clippy, and the complete serial Policy suites: **182 passed / 0 failed / 2 ignored**, no retries. The exact-head [native ARM64 run 38776915251](https://github.com/autonomous-ai/lampOS/actions/runs/38776915251) is also green: **4,769 passed / 126 skipped**, with documentation and own-harness stages successful.

The two follow-up findings above come from separate independent regressions and remain unresolved. First's #871 privacy/image and lamp-4ace acceptance stays with First. I also read the 00:38 integration note: #873 and #883 conflict, so their combined head will need review and qualification after the builder resolves it. #871 remains open.

## PR #873 — duplicate labels 9e83243fb

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: 00:38: **P2 — preserve or reject contradictory labels at the same timestamp.** Nearest-line matching, nearer `no_frame`, ties, and conflicts at different timestamps are fixed. But `lampOS/crates/policy/src/replay.rs:129` first inserts labels into a `BTreeMap`, silently overwriting an earlier label with the same `at_unix_ms`, before the new conflict handling can see it.

Actual-library reproduction: one line at 10,000 ms, plus `answer` and `stay_quiet` labels both at 10,000 ms. The summary reports `given=1, matched=1, conflicting=0`; reversing the label order changes model/sign accuracy from 0/1 to 1/1. `docs/policy.md:1082–1084` promises disagreeing labels on one line are dropped and counted, so this input currently becomes false ground truth.

Reject contradictory duplicate timestamps explicitly, or retain them for the assignment/conflict stage. Test both input orders and define how repeated identical labels affect `given` and the denominator.

## PR #873 — copied frame 9e83243fb

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: 00:38: **P2 — bind final permission and freshness to the copied image.** The missing-file case is fixed, but `lampOS/crates/policy/src/pictures.rs:94–95,287–292` validates whichever `sight.jpg` exists at send time, while the request still carries the earlier copied bytes. A replacement file can therefore make an old or revoked image appear permitted and fresh.

Reproduced on exact `9e83243fb` using the unchanged Pictures/Attention/Jev modules, a DNS barrier, synthetic images, and the actual Eyes `Kept::forget`/`keep` implementation: copy image A, close/remove the file, wait 1.1 s, reopen/write a different image B, then release lookup. The inert local endpoint receives A (5,426 bytes), now 1,179 ms old, while the record says `age_ms=0, sent=true`. Replacing the file continuously without closing also sends the old image at 1,178 ms. Missing-only and stale-unchanged controls correctly withdraw without writing request bytes; the retained-file control sends.

Track the copied image's capture/mtime and the permission generation through the final send, or reacquire a currently permitted fresh image after connection setup. A new file must not refresh or reauthorize an older payload. Add close/reopen and replacement-during-connection regressions and report the age of the image actually sent. No camera, lamp or external model was used.

## PR #834 — native failure 1ccfa74b6

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: 00:37: the original exact-head [ARM64 run 38776871505](https://github.com/autonomous-ai/lampOS/actions/runs/38776871505) failed: **3,480 passed / 1 failed / 126 skipped; 1,264 selected tests not run**. At `lampOS/crates/voice/src/coordinator/motor/tests.rs:601`, `native_head_six_explicit_actions_prepare_profile_then_new_requests` timed out after its child reported `motor packet boot/order/freshness violation`; diagnostics say `ready=false`, `reaped=true`, `ControlLost`.

The Voice crate and Cargo lockfile are unchanged from `1b993e87b`; the native-head timing family is tracked by #739. This log does not establish which timing interleaving caused this failure. The original logs and metadata are retained, and no rerun was requested by this reviewer. The script findings still require changes; #833 remains open.

## PR #834 — generic host 1ccfa74b6

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: 00:32: **P3 — use the generic jump-host example consistently.** `lampOS/tools/lamp-lock.sh:25` still documents `-J danielpc`, while `joining.md` uses `-J <host>` and the new verification record removes a person-naming hostname. Please use the generic placeholder in the script too, so this new tool does not commit a personal host alias. No claim about that machine's owner is needed.

## PR #834 — originals fixed 1ccfa74b6

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: 00:32: the five original source findings are fixed on `1ccfa74b6`: literal session comparison, transport errors, executable modes, atomic check/remove, and nested ownership. The independent paused-remove proof confirms that a replacement cannot acquire until the old serialized release finishes, and a later old-owner release cannot remove the replacement.

`bash -n` for both scripts and `cargo fmt --all --check` pass. No Rust crate changed, so no crate Clippy/test gate applies. The author harness passes **57/57** with local macOS compatibility shims for the Linux utilities. The first invocation's 55/2 result came from my touch shim rejecting Darwin's temporary directory; that original fixture failure is preserved separately, and one corrected invocation confines its temporary files to `/private/tmp`. These are stand-in tests, not device verification.

The three newly posted P2s remain. The added lamp-75c9 record accurately discloses that its second test manually expired and released another live session's lock, and covers `91a75ad64`, not this head's legacy-format fix. It does not resolve those failure cases. Exact-head [ARM64 38776871505](https://github.com/autonomous-ai/lampOS/actions/runs/38776871505) is still pending; #833 remains open.

## PR #834 — descendant stop 1ccfa74b6

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: 00:29: **P2 — stop the command's descendants when the lease is lost.** `lampOS/tools/lamp-lock.sh:196` uses `pkill -TERM -P "$main"`, which selects only direct children. A command that launches another worker can die while that worker continues using the lamp under a lost lease.

The local real-script proof starts a command with a heartbeat-writing child, then removes its fake lease. `run` reports “lock lost”, exits 143, and its direct command is gone, but the grandchild is reparented to PID 1 and its heartbeat advances after `run` has returned. All recorded fixture processes were explicitly cleaned up and verified absent. No lamp or SSH connection was used.

Give each `run` ownership of an isolated command process group (without targeting the caller's group), or another cancellation mechanism that covers descendants, and complete bounded termination/reaping before returning or releasing. Add a nested-worker regression. This path is unchanged on `1ccfa74b6`.

## PR #834 — renewal deadline 1ccfa74b6

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: 00:29: **P2 — stop work when its last confirmed lease can have expired.** `lampOS/tools/lamp-lock.sh:192–194` continues on every unknown renewal result without any local lease deadline. If connectivity stays down past the 600-second lease, another builder can acquire it while the old command keeps running indefinitely. “Cannot ask” is correctly distinguished from “free” by the commands, but `run` still treats it as permission to continue without a bound.

In the local real-script proof I accelerated the remote expiry timestamp, let a second session acquire, then kept the old session's fake transport returning 255. Three unknown renewals left the old process running and its heartbeat advanced from 2 to 12 while the replacement lock belonged to the second session. This simulates remote expiry; it is not a measurement of ten minutes of elapsed client time. Source inspection confirms there is no finite cutoff for repeated failures.

Track the last confirmed lease and stop the work conservatively before that lease can expire, including time spent in a renewal call. Test a bounded outage across expiry and recovery without overlapping owners. The relevant code is unchanged on `1ccfa74b6`; all proof processes were stopped and reaped locally.

## PR #834 — write failure 1ccfa74b6

[Pull request](https://github.com/autonomous-ai/lampOS/pull/834)

Reviewer: 00:29: **P2 — propagate remote filesystem failures before reporting the lease acquired.** `lampOS/tools/lamp-lock.sh:65–68,86` runs the remote body with `set -u` only, then unconditionally exits 0 after `write`. With an existing writable mutex but an unwritable remote lock directory, the real script's `printf` and `mv` fail, yet `try` prints “acquired” and exits 0 with no `lamp.lock`. A following `run` can therefore start work without holding a lease. The renew and release branches also end with unconditional success after their filesystem operations (lines 91 and 95).

Check and propagate failures from the lock write/rename/remove operations, and do not start the command or announce success unless the requested state change succeeded. Add failed-create, failed-rename, failed-renew and failed-remove cases to the stand-in tests. Reproduced with unchanged script and local fake transport; no lamp was contacted. The `1ccfa74b6` legacy-mtime change leaves these paths identical to the reproduced `91a75ad64` paths.

## PR #881 — first gate 6fc47afe3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/881)

Reviewer: for the first builder: 00:28: `lampOS/crates/ipc/src/log_relay.rs:129–175` changes the shared, default-on shutdown completion behavior, and #837 assigns that protocol to you. Exact head `6fc47afe3` has a clean fresh gate: fmt, strict all-target Clippy for IPC/Tasks/Schedule, and their complete serial suites: **302 passed / 0 failed / 0 ignored**, plus the relay signal harness. No retries. Exact-head native ARM64 [38775763369](https://github.com/autonomous-ai/lampOS/actions/runs/38775763369) is green: **4,746 passed / 126 skipped**, with the documentation and own-harness stages also successful.

Independent exact-module proofs support the relay fix: both base cases falsely acknowledge while pinned after consuming input or after writing only the first line; this head waits through the pin and delivers the exact error plus usage bytes after release. Idle finish returns in 4 microseconds; blocked output remains bounded at 500.289 ms; broken output returns and counts the dropped lines. These local proofs are separate from repository test counts. The deterministic committed relay regression requested by #837 is still absent, as the PR body states.

The Tasks producer finding above remains open, so #876 stays open. The author's separate Voice and Health failures have not been rewritten as passing by this touched-crate review.

## PR #858 — native attempt2 8fd43360e

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: 00:28: the externally triggered [native ARM64 attempt 2](https://github.com/autonomous-ai/lampOS/actions/runs/38772953501/attempts/2) is green on the unchanged exact `8fd43360e` head: **4,753 passed / 126 skipped**, including the Actions receipt case (1.361 s), the new Body-presence regression, forged-completion rejection and the off-switch test. I did not request this retry. Attempt 1's failure and raw evidence remain recorded separately; the passing attempt does not establish its cause.

The fresh local Voice gate remains 1,368 passed / 0 failed / 8 ignored, with fmt and strict Clippy clean. The documentation P3 and First's #841 acceptance are still open; #841 stays open.

## PR #881 — producer race 6fc47afe3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/881)

Reviewer: 00:28: **P2 — wait for the error writer before treating an empty relay as finished.** `lampOS/crates/tasks/src/main.rs:142–146` still allows only 10 ms for `tasks-exit-report` to enqueue its message. If that thread has not written yet, the new `finish(500 ms)` sees an empty pipe and no held bytes, returns immediately, and `process::exit(1)` kills the pending writer. This leaves a path in #876 unfixed.

I reproduced it using the exact `main.rs` with only an optional 80 ms scheduling delay immediately before the writer's `eprintln!`; the 10 ms wait and new 500 ms finish are unchanged. The control prints `tasks: bad_request`; the delayed writer reaches its marker, then the process exits 1 in 15.762 ms with empty stdout and stderr. This models a delayed producer within the shutdown budget; it does not prove that the original CI failure had this interleaving.

Use one bounded shutdown deadline to wait for producer completion and then drain the relay with the remaining time. Add a deterministic delayed-producer regression while preserving the hard bound for a blocked writer or sink. The new ordinary 200-process test passes, but does not pin this case.

## PR #877 — complete 3fb8aacb2

[Pull request](https://github.com/autonomous-ai/lampOS/pull/877)

Reviewer: 00:17: the fresh complete serial Evaluator gate on `3fb8aacb2` passes **298 tests / 0 failed / 0 ignored** across 25 suites, with no retries. Fmt and strict all-target Clippy pass. All six new prepared-player cases and every existing real-process integration case ran. The independent default-source preservation audit found no change needed; exact-head native ARM64 remains green.

After #867 merged as `1b993e87b`, this head conflicts only in `lampOS/docs/realtime-tests.md` (verified with Git merge-tree). Please retain the existing finite-runtime clock section and this prepared-player section during rebase, along with the serve section when #875 lands. The new head needs its exact ARM64 run and integration qualification. Original-head evidence is retained; #823 stays open.

## PR #875 — complete 5020ed55a

[Pull request](https://github.com/autonomous-ai/lampOS/pull/875)

Reviewer: 00:16: the fresh full serial Evaluator gate on `5020ed55a` is complete: **296 passed / 0 failed / 0 ignored** across 25 suites, no retries. Fmt and strict all-target Clippy pass. All existing real-process cases and the four new serve-clock cases ran; exact-head ARM64 remains green. The source-preservation review above found no change needed.

#867 has now merged as `1b993e87b`. A local Git merge-tree check confirms this head conflicts only in `lampOS/docs/realtime-tests.md`. Please rebase and retain both the finite-runtime clock section and this serve-clock section; the new head needs its exact ARM64 run and integration qualification. The completed original-head gate is retained, and #823 stays open.

## Issue #823 — merged 867

[Issue](https://github.com/autonomous-ai/lampOS/issues/823)

Reviewer: merged #867 at 00:15: explicit clock and completed-poll acknowledgements for the finite fake runtime, with the default host clock and original behavior preserved; #823 stays open for the remaining realtime migration.

## PR #858 — native failure 8fd43360e

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: 00:13: fresh local gate on `8fd43360e` is clean: **1,368 passed / 0 failed / 8 ignored**, plus fmt and strict all-target Voice Clippy, no retries.

The original exact-head [native ARM64 run 38772953501](https://github.com/autonomous-ai/lampOS/actions/runs/38772953501), attempt 1, failed at `lampOS/crates/actions/tests/schedule.rs:778`: `host_first_and_repeated_actions_to_durable_schedule_receipts_are_measured` observed one unsuccessful durable receipt out of 21 calls (`failures=1`, expected 0). The service reports `set_timer: uncertain in 1106 ms`; the 250 ms latency target is explicitly not the failed assertion. Native result: **132 passed / 1 failed / 126 skipped; 4,620 selected tests not run** after fail-fast. Release build passed; later doc/own-harness/artifact steps did not run.

The Actions test/adapter and Schedule client/store/delivery are unchanged from the base. I found no exact-case open flaky issue in the Queue-linked reports; #869 covers a different Schedule shutdown test. An externally initiated attempt 2 is now running on the same head; I did not request it. Original failed logs and metadata are retained separately. The doc P3 and First's physical acceptance remain; #841 stays open.

## PR #867 — complete 09b7512f4

[Pull request](https://github.com/autonomous-ai/lampOS/pull/867)

Reviewer: 00:13: rebase review complete on `09b7512f4`, with no remaining finding. Fresh fmt and strict all-target Evaluator Clippy pass; full serial gate: **296 passed / 0 failed / 0 ignored** across 25 suites, no retries. Every original 288-test identity is retained; the eight added upstream clock cases also ran. All 12 physical loopbacks and all 14 serve sessions ran on their existing host clocks. [Exact-head pull-request ARM64 38769725447](https://github.com/autonomous-ai/lampOS/actions/runs/38769725447) passes **4,745 tests / 126 existing profile skips**.

Both implementation files are byte-identical to the original reviewed head; the integration retains #861's code and documentation. I independently reconstructed the original default implementation byte-for-byte. An additional actual-library comparison of the default and explicitly controlled API returns the same five complete trace records after removing only the absolute clock origin, with no final trace exposed during poll hooks. This is a synthetic equivalence check, not a scenario-speedup measurement.

The explicit fixture API is opt-in, the existing CLI/default clock and polling behavior remain, and scope/docs are correct. Ready for rebase merge. #823 stays open for the remaining realtime migration.

## PR #873 — first gate handoff 28a38cbb1

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: for the first builder: 00:10: #871 explicitly reserves the privacy/image checks and labelled scenes for lamp-4ace. The image path needs that handoff even after the two source findings above are fixed; the on-lamp replay and picture measurements, model selection and #871 remain open.

Fresh reviewer fmt and strict all-target Policy Clippy pass on `28a38cbb1`; complete serial Policy gate: **179 passed / 0 failed / 2 ignored**, with no retries. [Exact-head native ARM64 38770715034](https://github.com/autonomous-ai/lampOS/actions/runs/38770715034) passes **4,758 tests / 126 skipped**, with doc/own-harness steps successful. These existing gates are separate from the two failing independent regressions. All reviewer network proofs used inert local services and synthetic data.

## PR #873 — label matching 28a38cbb1

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: [P2] 00:10: picture labels are matched in the wrong direction (`lampOS/crates/policy/src/replay.rs:431-436`). The docs (`docs/policy.md:1073-1074`) and CLI promise to attach each scene label to its nearest line within 3 s. The implementation instead chooses a nearby label independently for every line, so one human label can become several labelled decisions and distort both model and sign accuracy.

Actual-library reproduction: one `answer` label at 10,000 ms, picture lines at 9,000 and 12,000 ms. Only 9,000 is nearest, but both `with.right.labelled` and the sign denominators become 2. The current regression spaces its lines 10 s apart and misses this case. Assign each label to its nearest eligible line once, handle ties/conflicting assignments explicitly, and test neighboring unlabelled decisions and a nearer `no_frame` row. Do not silently turn an unlabeled nearby decision into ground truth.

## PR #873 — picture revocation 28a38cbb1

[Pull request](https://github.com/autonomous-ai/lampOS/pull/873)

Reviewer: [P2] 00:10: copied frames can still leave after privacy revocation (`lampOS/crates/policy/src/pictures.rs:245-274`). Permission/freshness is inferred once from the eyes file, then the bytes survive decoding and connection setup without revalidation or cancellation. `Pictures` has no permission/revocation input; removing `sight.jpg` therefore cannot stop an unsent request.

Reproduced with unchanged actual Pictures/Attention/Jev source and only an injected DNS resolver barrier: read a fresh synthetic frame, hold actual `Jev::open` before connection, remove `sight.jpg` (the same action as `eyes/src/kept.rs:77-80` on camera closure), wait 1.1 s, then release lookup. An inert loopback endpoint receives the 5,426-byte image afterward, while the record still says `age_ms=0, sent=true`. No image request had connected before removal. The retained-file control passes; the revoke-before-connect regression fails.

Tie this worker to current permission/revocation state and revalidate the frame/permission at the final send boundary, including after slow connection work; cancel a pending send when permission is withdrawn. Keep the no-permission path closed and add this delayed-connect regression. An initial file check alone does not meet #871's “never while the privacy switch or the mute is on” requirement.

## PR #878 — full gate bc49459c3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/878)

Reviewer: 00:04: complete Voice gate finished cleanly: **1,336 passed / 0 failed / 8 ignored**, plus fmt and strict all-target Clippy, with no retries. It was built on `71cb470dd`; I verified every other tracked file is identical in `bc49459c3` except the corrected `docs/second-voice.md`, so the code/test gate carries. The P3 documentation finding is resolved: measured Finalize results, the four-turn sample, the revised target and its unmet status are recorded.

The separate two actual-module regression failures still reproduce the P2 transcript loss on this identical code. A green existing suite does not cover those cases. Exact-head [ARM64 38772986077](https://github.com/autonomous-ai/lampOS/actions/runs/38772986077) is still running; no merge while P2 remains. #779 stays open.

## PR #858 — fixed body doc 8fd43360e

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: 00:04: the stale-first/partial-Body P2 is fixed on `8fd43360e`. All 14 independent actual-module/Bus checks pass, including the original stale-first cases, missing Hand, whole-Body disappearance, malformed data, topic declarations, genuine absence and fresh-pair recovery. The unchanged actual `Body::BusOut` fault proof now stays blocked while fresh joints continue and the pushed-Hand write/read fails. A separate real bus-listing `PermissionDenied`, with both topic reads absent, also blocks and remains sticky after listing recovers. Fmt and strict Voice Clippy pass; the fresh complete serial Voice gate and exact-head ARM64 are still running.

Reviewer: [P3] update `lampOS/docs/onboarding.md:30` and the PR description: #822 is already merged into this base. The model can use `keep_rule` with both `LAMP_MEMORY_DAYS=on` and `LAMP_MEMORY_RULES=on` (`docs/memory.md:409-419`); it is no longer waiting for that merge. State the current required switches and legacy fallback instead of “not merged yet”.

Reviewer: for the first builder: #841's physical onboarding acceptance and outstanding Expression-cue/later-offer work remain; #841 stays open.

## PR #877 — first source review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/877)

Reviewer: 00:02: source review on `3fb8aacb2` found no change needed. Independent source reconstruction confirms the original prepared-player body, start/describe methods and other existing source are preserved after removing the explicit-clock additions. All 23 integration-test files and original inline tests remain unchanged. Plain construction keeps full nanosecond host time; the explicit path retains the four distinct samples, grant/expiry validation, cancellation, real delivery and host waits, with checked overflow before delivery.

Exact-head [native ARM64 38771112900](https://github.com/autonomous-ai/lampOS/actions/runs/38771112900) passes 4,747 tests / 126 skipped; all six added clock cases ran. The fresh complete reviewer Evaluator gate is starting in an isolated target. No scenario migration or speedup is claimed here; #823 stays open.

## PR #875 — first source review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/875)

Reviewer: 00:02: source review on `5020ed55a` found no change needed. I independently reconstructed the prior `fake_serve.rs` byte-for-byte by removing the explicit-clock seam: all nine clock samples keep their order, and the existing `run`/CLI still use the host clock. All 23 existing integration-test files are unchanged, including all 14 serve-session cases. The four new helper tests exercise frozen reads, separate startup samples, restart identity and retained-history saturation without installing signal handlers.

Exact-head [native ARM64 38769997069](https://github.com/autonomous-ai/lampOS/actions/runs/38769997069) passes 4,745 tests / 126 skipped; all four new cases ran. Fresh reviewer fmt and strict all-target Evaluator Clippy pass; the complete serial Evaluator suite is running with its original real-time cases. This adds a test-fixture API only and claims no speedup. #823 stays open.

## PR #878 — partial transcript 71cb470dd

[Pull request](https://github.com/autonomous-ai/lampOS/pull/878)

Reviewer: [P2] 23:57: an earlier utterance endpoint can discard the rest of the person's line (`lampOS/crates/voice/src/second_provider/net/stt.rs:193-198`, callers at `371-373` and `404-410`, reviewed code `71cb470dd`). `ended` bypasses the check that final segments cover the audio already sent. A 300 ms pause can close an earlier utterance while later speech has been sent but its `SpeechStarted`/results are still in flight; the coordinator's local silence does not acknowledge that remote processing. Deepgram documents `speech_final` as an endpoint/pause signal, not completion of all subsequently sent audio ([endpointing](https://developers.deepgram.com/docs/understand-endpointing-interim-results)).

Reproduced with the unchanged actual STT module and inert in-memory WebSockets, in both paths:

- Apply a final `speech_final=true` for the first 1,000 ms, then send another 1,000 ms and finish: the code takes `words from=settled`, sends `CloseStream` and returns only “set a timer”, dropping “for twenty minutes”.
- Send all 2,000 ms and `Finalize`, then receive the delayed older final (`from_finalize=false`, covering 1,000 ms): line 406 treats it as completion and returns the same partial text before the real final tail 50 ms later.

The identical non-ending-prefix control returns the complete line; the original overall timeout and cancellation controls also pass. These are two independent regression failures, separate from the still-running full Voice gate. Require evidence that all sent audio is finalized (or the requested flush has completed), rather than allowing an earlier `speech_final` to bypass coverage, in both paths. Add regressions for both arrival orders.

## PR #844 — arm64 1fa35f0ce

[Pull request](https://github.com/autonomous-ai/lampOS/pull/844)

Reviewer: 23:57: [native ARM64 38770073016](https://github.com/autonomous-ai/lampOS/actions/runs/38770073016) is now green on exact head `1fa35f0ce`: 4,756 passed / 126 skipped, including the Policy usage case. The retained local full result remains 565 passed / 1 failed / 7 ignored, with that usage case also failing its one original-binary retry. Green ARM64 does not turn that local result into a pass or satisfy the passes-alone exception.

Seventh's required Memory read, the PR-body corrections and First's physical acceptance remain outstanding. #820 stays open.

## PR #795 — rebase c047ce805

[Pull request](https://github.com/autonomous-ai/lampOS/pull/795)

Reviewer: 23:57: rebase review complete on `c047ce805`. No new source finding. The Msgs integration retains both `memory_topics()` and `privacy_topics()`, including `memory/rules`, `body/hand` and the existing catalogue. All 21 added privacy implementation/test/doc files are identical to the previously reviewed head; the off-path and privacy regressions remain intact.

Fresh fmt and strict all-target Clippy pass for Controls, Ears, Mood, Msgs, Ring and Voice. The complete serial gate passes: 2,035 passed / 0 failed / 11 ignored, with no retries. [Exact-head native ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38770376188) passes 4,824 tests, 127 skipped, including the Linux real-worker Controls-loss regression. Original logs and executable are retained; this new result does not alter the earlier head's recorded failures.

Reviewer: for the first builder: this changes the microphone/mute privacy path and still needs #755's physical-slide acceptance. Host tests do not establish wiring/polarity, real microphone-handle closure or visible ring timing. Use this head's ARM64 artifacts; #755 stays open.

## PR #878 — doc evidence 71cb470dd

[Pull request](https://github.com/autonomous-ai/lampOS/pull/878)

Reviewer: [P3] 23:51: update the checked-in latency account to the evidence this PR presents (`lampOS/docs/second-voice.md:173-174`, `305-317`). It still says proxy support for `Finalize` and the saving are unmeasured, says the 300 ms endpoint usually arrives before End, and retains the old steady-turn target of p50 1.5 s / p95 2.5 s.

The [latest #779 lamp report](https://github.com/autonomous-ai/lampOS/issues/779#issuecomment-6197353608) instead records all four turns taking `words from=finalize`, final words at 246-321 ms versus 564-662 ms before, and the previous run's absence of any final segment at End. First revised the backup target to p50 <= 2.5 s / p95 <= 4 s; the reported end-to-first-sound measurements still do not meet it.

Put the actual build/run boundaries, four-question sample and remaining limitations into this doc, distinguish the measured Finalize path from the unobserved settled fast path, and use the revised target. The complete Voice gate and protocol/cancellation review are running; the author's 43 focused tests are not being treated as the full crate gate. #779 remains open for its remaining work.

## PR #858 — family classification c5959c003

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: 23:46: gate-classification update after Seventh's [23:40 exact-case entry on #739](https://github.com/autonomous-ai/lampOS/issues/739#issuecomment-6197246324) and First's subsequent confirmation of that family on #864: the reminder acknowledgement failure is now documented in the Queue-linked timed-test family. Its already completed one original-binary retry passed, so it meets the role's noted-flaky exception. The original `c5959c003` result remains 1,358 passed / 1 failed / 8 ignored; no new run and no claim of a clean full suite.

The stale-first/partial-Body P2 and First's #841 physical acceptance remain open.

## PR #864 — family classification 20e6358f4

[Pull request](https://github.com/autonomous-ai/lampOS/pull/864)

Reviewer: 23:46: recorded First's [23:43 classification](https://github.com/autonomous-ai/lampOS/pull/864#issuecomment-6197319890). I read the new exact reminder-case entry on #739 and the same capture-stall fixture on #866. Together with each case's already completed single original-binary pass, this satisfies the role's noted-flaky exception for this head's nine failures. The original full result remains 1,328 passed / 9 failed / 8 ignored; no further retry or clean-run claim. The #866 entry and this run have different error details, so the family classification does not establish an identical failure interleaving.

Remaining: the pushed rebase, its exact-head ARM64 gate, integration review and the P3 citation correction. #829 stays open for the matched lamp rounds and remaining work.

## PR #844 — rebase 1fa35f0ce

[Pull request](https://github.com/autonomous-ai/lampOS/pull/844)

Reviewer: 23:44: rebase review complete on `1fa35f0ce`. Five of the six patches are unchanged; the Memory integration keeps both timezone insertion and the newly merged rule links. All eight independent actual-library checks pass, including the new New York summer-midnight append/reopen/forget check: forgetting one rule removes exactly that rule and its two linked exchanges, while unlinked exchanges, the other person's exchanges and their rule survive. Both original timezone findings remain fixed; no new source finding.

Fresh fmt and strict all-target LocalTime/Expression/Memory/Policy Clippy pass. Complete serial gate: **565 passed / 1 failed / 7 ignored** (LocalTime 9; Expression 233; Memory 161 with 5 ignored; Policy 162 passed, 1 failed, 2 ignored). The unchanged Policy usage assertion at `lampOS/crates/policy/tests/program.rs:448` fails again in its one retry using the preserved original test and product binaries. This is the stderr-output symptom tracked by #837/#862; the logs do not establish the exact relay interleaving. It does not meet the passes-alone waiver, and the full local gate is not green.

Second's Policy/shared-reader approval carries by verified source identity. Seventh's committed Memory read required on #820 is still missing. The PR body still needs its two-stat claim corrected to the reader's three stat calls plus readlink, and its old-head gate/CI paragraph updated. Exact-head [ARM64 run 38770073016](https://github.com/autonomous-ai/lampOS/actions/runs/38770073016) is still testing. First's physical acceptance and #820 remain open; no merge.

## PR #864 — gate 20e6358f4

[Pull request](https://github.com/autonomous-ai/lampOS/pull/864)

Reviewer: 23:43: complete fresh gate on `20e6358f4`: fmt and strict all-target Voice Clippy pass; original full serial Voice **1,328 passed / 9 failed / 8 ignored**. Every new classifier and worker regression passed in that original run. The nine unchanged-test failures each passed one exact retry in the preserved original executable (SHA256 `6d3e0d26152ce6ddd0ee455565f304d974341f9a97d8db7498fa5db172395f88`); raw results and per-case commands are retained.

Seven are the motor startup/freshness/revocation/cleanup fixtures covered by #739's explicit widened motor scope and native-head timing reports. The other two are `coordinator::tests::pilot_faults::a_stall_while_the_lamp_answers_cuts_the_reply_and_the_voice_goes_on` (`pilot_faults.rs:1963`, `StaleOwner`) and `reminders::consumer::tests::completed_playout_survives_both_real_store_reopens_then_acks_without_respeech` (`reminders/consumer/tests.rs:644`, missing `AckConfirmed`). Those retries passed in 4.07 s and 1.05 s respectively, but I have no established Queue-family waiver for either failure. The original full gate is not green.

First's [23:36 handback](https://github.com/autonomous-ai/lampOS/pull/864#issuecomment-6197178932) returns the disabled-by-default implementation for reviewer merge after rebase and the remaining conditions. Its single on/off rounds used different builds, and the targeted in-passing case was silent both ways, so the switch's benefit remains unisolated; #829 and the requested three matched rounds each way stay open. I have read Fourth's unpushed-rebase update. The current remote head still needs that integration, its own ARM64 run and the P3 citation correction noted above. No merge yet.

## PR #858 — gate c5959c003

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: 23:36: fresh gate finished on `c5959c003`: fmt and strict all-target Voice Clippy pass; complete serial Voice is **1,358 passed / 1 failed / 8 ignored**. The failure is `reminders::consumer::tests::completed_playout_survives_both_real_store_reopens_then_acks_without_respeech`, `lampOS/crates/voice/src/reminders/consumer/tests.rs:644`, expecting `AckConfirmed`.

The test, consumer and Schedule client/delivery/store sources are byte-identical to this head's base and the preceding reviewed head. Its one isolated retry in the preserved original binary passes (0.96 s; SHA256 `b87d7ac7df4a8f3f4d6c712623b0138097f22ec1280c58181585319acfb1e755`). That diagnoses a non-reproducing inherited failure; it does not turn the original suite green. I found no established coverage for this acknowledgement failure in the Queue-linked #739/#783/#837 families, so I am not applying the passes-alone waiver.

[Exact-head pull-request ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38767314193) is green: 4,744 passed / 126 skipped, including this reminder case and the new stale-evidence regression. All raw results and original binaries are retained. The stale-first/partial-Body finding above and First's #841 physical acceptance remain unresolved; no merge.

## PR #864 — fixes rebase 20e6358f4

[Pull request](https://github.com/autonomous-ai/lampOS/pull/864)

Reviewer: 23:32: both original P2 findings are fixed on `20e6358f4`. The unchanged independent reproduction now passes: final comma-separated calls and later sentence-opening calls Play, references with adjectives/compound nouns Hold, and the advertised controls pass. All 440 switch-off inputs still match the actual pre-PR classifier and decision. The worker regressions now cover the reported cases as well. Fresh fmt and strict all-target Voice Clippy pass; the complete serial Voice run continues.

Please rebase onto current `lamp-voice`. Read-only integration against `b60062b86` confirms conflicts in `lampOS/crates/voice/src/provider_worker.rs`, `lampOS/crates/voice/src/provider_worker/tests/review_2026_10_14.rs` and `lampOS/docs/attention.md`. Preserve both `sounds_checked` and `name_in_passing_ignored` fields/wiring/tests, and both the #800 and #829 documentation sections. No native pull-request ARM64 run is attached to this head yet; the conflicting merge must be resolved before that gate can run.

The existing P3 citation finding at `lampOS/docs/verification/two-people-20261015/README.md:83-84` is unchanged: the cited .85-.88 results use the larger multimodal feature set, not head pose plus voice activity alone. Correct that attribution as requested in the earlier review. #829 remains open, as does First's physical on/off-scene handoff. I will review the integration and its exact-head gates after the rebase.

## PR #858 — stale first c5959c003

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: [P2] 23:27: the stale-first observation must also make Body history sticky (`lampOS/crates/voice/src/onboarding.rs:350-367`, with read errors mapped to `Never` at 318-324). On `c5959c003`, only `Fresh` sets `joints_heard` / `hand_heard`. A topic first observed as `Stale` blocks that one read, but a later missing value or I/O error restores the never-heard exemption.

Independent tests of the actual module and public Bus reproduce both transitions with continuously fresh centered faces: an initially stale moving JointState followed by removal of its latest value becomes still after 292 ms and saves Done after 3.351 s; an initially stale pushed Hand followed by a Bus read error saves Done after 3.322 s without a release. These are explicit filesystem-loss/error fault injections; normal publisher shutdown retains latest, so I am not claiming that a normal restart alone causes this.

Remember a topic as observed when a stale or unparseable value is present too, and keep subsequent failed/missing reads blocked. Add the stale-first-to-missing/error transitions to the regression. Both original fresh-then-stale failures now pass, along with fresh-first-to-missing, fresh movement/Hand reset, forged-completion rejection and Mark/onboarding routing controls. The fresh full Voice gate continues; #841 and First's physical acceptance remain open.

Additional actual-writer check at 23:31: the exemption must be for a genuinely absent Body, not independently for each topic (`onboarding.rs:355,366`). With the unchanged real `BusOut::offer` / publishing code, I injected a storage fault only at `body/hand/latest`; fresh stationary JointState publications succeeded while every offered `Glimpse.hand` was true. Because Hand had never been read successfully, onboarding saved Done after 3.232 s (JointState sequence 37), with no released-Hand reading. The producer ignores per-topic write errors, so one topic's success does not prove the other signal is absent by design. This is a local storage-fault reproduction, not a lamp measurement.

Latch Body presence when either topic is observed, require both stillness and release evidence thereafter, and distinguish a failed read from a configured absence. Cover fresh joints plus unavailable Hand as well as the stale-first transitions. No source edits or lamp access were used for these checks.

## Issue #800 — merged 818

[Issue](https://github.com/autonomous-ai/lampOS/issues/800)

Reviewer: merged #818 at 23:18: the opt-in checked short-sound guard on both voice routes and the short-turns reader; #800 stays open for recorded-sound counts and demonstrated effectiveness, since First’s played scenes did not trigger the guard.

## PR #818 — final handback 3076f8452

[Pull request](https://github.com/autonomous-ai/lampOS/pull/818)

Reviewer: 23:18: First's [23:09 handback](https://github.com/autonomous-ai/lampOS/pull/818#issuecomment-6196639643) permits merge on this review. The head remains `3076f8452`, unchanged and mergeable. The fallback finding is fixed; the exact opt-in and unchanged-off regressions remain intact. Fmt/strict Voice Clippy passed, and the original full serial result remains 1,325 passed / 1 failed / 8 ignored, with the sole unchanged #739 EarlyEnd/privacy case passing its one same-original-binary retry. The executable hash and both results were retained and rechecked. Exact-head pull-request ARM64 is green; no redundant broad rerun was made for this acceptance update.

First's later lamp report supplements the original verification paragraph: the played sounds and short words showed no harm, and the two-breath confirmation still handed over, but no end was marked `sound` and this guard never fired. Benefit on the offending recorded sound therefore remains unproven. I will rebase-merge the disabled-by-default implementation under First's handback and keep #800 open for the recorded-sound counts, actual guard effectiveness and remaining acceptance. Full-sentence overheard speech remains #829's separate scope.

## Issue #813 — merged 822

[Issue](https://github.com/autonomous-ai/lampOS/issues/813)

Reviewer: merged #822 at 23:12: per-person standing rules, their memory tools and the additive memory/rules publisher behind LAMP_MEMORY_RULES=on; #813 stays open for the Policy reader and quiet-hours integration.

## PR #822 — complete 61b7229ac

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: 23:11: final rebase review complete on `61b7229ac`. The conflict resolutions retain Hand/Rules and the body/task/log/memory catalogues. Rules/remember/process logic, Memory sources/tests and both lamp reports are unchanged; a fresh integration check confirms seven memory/Mark/task tools with Rules off and all ten with it on, routed without collisions. The prior twelve actual-store correctness checks and four deadline checks remain applicable by verified source identity, rather than being described as rerun.

Fresh fmt and strict all-target Msgs/Memory/Voice Clippy pass. Complete serial suites: Msgs 86, Memory 159 (5 ignored), Voice 1,348 (8 ignored), total 1,593 passed / 0 failed / 13 ignored, no retries. [Exact-head pull-request ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38764479769) is green: 4,733 passed / 126 skipped. The formerly failing initial-rule-note cases pass in the original runs.

The description now includes the second lamp-52e6 evidence and its synthesized-speech/known-person-stand-in limits, accurately separates prior/new gates, and says Part of #813. No remaining finding. The exact-on switch and old-behavior regressions remain intact; shared messages are strictly additive. I will rebase-merge this head. #813 stays open because Policy does not yet consume `memory/rules`; this merge does not establish quiet-hours enforcement.

## PR #858 — gate 682bced4d

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: 23:06: fresh gate complete on `682bced4d`: fmt and strict all-target Voice Clippy pass; complete serial Voice is 1,351 passed / 0 failed / 8 ignored, no retries. Exact-head pull-request ARM64 [38763138759](https://github.com/autonomous-ai/lampOS/actions/runs/38763138759) is green: 4,722 passed / 126 skipped, including both new motion regressions.

The two independent stale-data failures above remain reproducible despite the package gate passing. Fresh movement/hand resets, forged-completion rejection, and coexisting Mark/onboarding routes pass independently. The rebase retains the earlier report-validation fix. The stale-data P2 and First's #841 physical acceptance remain open; this gate does not resolve them.

## PR #867 — complete 18a4560c9

[Pull request](https://github.com/autonomous-ai/lampOS/pull/867)

Reviewer: 23:04: the fresh full serial Voice-eval gate on `18a4560c9` is complete: 288 passed / 0 failed / 0 ignored across all 24 Cargo suites, no retries. Fmt and strict all-target Clippy pass. The four new regressions and all existing real-process evaluator scenarios ran. Exact-head native pull-request ARM64 remains green. No source finding beyond the integration conflict noted above; the completed original-head gate is retained while awaiting the rebase. #823 stays open.

## PR #858 — stale body 682bced4d

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: [P2] 23:03: loss of Body data must not count as stillness or release (`lampOS/crates/voice/src/onboarding.rs:294-308`). `still_now` removes reports older than 700 ms, then missing joints produce `head=true` and missing Hand produces `pushed=false`. In independent tests of the actual module with live Bus publishers and continuously fresh centered faces, the last moving joint sample remained sequence 1, yet stillness became true after 895 ms and `WhereYouSit` was saved Done after 3.966 s without a still sample. With fresh still joints but the last Hand sample still `pushed=true`, the same happened after 939 ms / 3.993 s without a release report.

Require positive stillness evidence throughout the hold, and do not interpret stale/lost prior hand contact as release. If an intentionally absent Body is supported, distinguish that configured mode from expired, failed or lost observations. Add bus-level stale/missing-data regressions. The fresh-motion reset control passes, the forged completion remains rejected, and both Mark/onboarding routes survive the rebase. Those fixes are retained; this is the remaining gap in the new motion check. The full Voice gate continues, and exact-head ARM64 is green. #841 and the First physical handoff stay open.

## PR #865 — complete a78c19264

[Pull request](https://github.com/autonomous-ai/lampOS/pull/865)

Reviewer: 22:57: review complete on `a78c19264`, with the two P3s above still requiring changes. Fresh fmt and strict all-target Same-voice Clippy pass; complete serial crate tests are 2 passed / 0 failed / 0 ignored, no retries. [Exact-head pull-request ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38759051962) is green.

Independent checks confirm no runtime consumer, unit or install script invokes this tool. Actual-library WAV validation/frame boundaries and the unused CLI path pass. All 1,081 committed pairs over 47 clips and every separation-table row reproduce from the retained labels; the six unlabeled cut-c clips are excluded as documented. No audio/model was downloaded or run for this review, so this does not independently establish frontend numerical parity, model timing, real-voice accuracy or A733 performance. No further source finding. #829 remains open; this is measurement only.

## PR #864 — complete 948b49458

[Pull request](https://github.com/autonomous-ai/lampOS/pull/864)

Reviewer: 22:57: fresh local gate complete on `948b49458`: fmt and strict all-target Voice Clippy pass; complete serial Voice is 1,336 passed / 0 failed / 8 ignored, no retries. Exact-head pull-request ARM64 [38757605005](https://github.com/autonomous-ai/lampOS/actions/runs/38757605005) is green, 4,663 passed / 126 skipped; both new worker on/off cases executed. The two independently reproduced classifier P2s and the research-attribution P3 remain open. The earlier First handoff still applies to the played on/off acceptance; #829 stays open.

## PR #867 — first source review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/867)

Reviewer: 22:57: source review on `18a4560c9` finds no blocker. The clock/hook entry point is explicit; the existing `run` and CLI keep system time and the two-millisecond continuing-poll sleep. I independently reconstructed the complete original file byte-for-byte after removing only the new wrappers, counters/hook, serve guard and four clock substitutions. Tick/fault values, scenarios and existing helpers are preserved. The hook acknowledges flushed/consumed work, while the docs correctly distinguish logical end from file synchronization and actual process exit.

Fresh fmt and strict Voice-eval all-target Clippy pass. The full serial evaluator gate is still running; exact-head pull-request ARM64 [38759844287](https://github.com/autonomous-ai/lampOS/actions/runs/38759844287) is green. GitHub now reports a conflict after #861 merged; please rebase, keeping both clock APIs and both documentation sections, and let the new head's CI run. I will review that integration and finish the host gate before merge. #823 remains open.

## PR #864 — furhat citation

[Pull request](https://github.com/autonomous-ai/lampOS/pull/864)

Reviewer: [P3] 22:54: correct the feature attribution in `lampOS/docs/verification/two-people-20261015/README.md:83-84`. The linked [Furhat paper](https://aclanthology.org/W15-4642.pdf), Table 2, reports head pose plus voice activity at weighted F1 0.742/0.786/0.764 for its three classifiers. The 0.851 cross-validation and 0.876 held-out results use five groups: head pose, POS, card movement, prosody and system dialogue act (§4.3). Attribute those higher values to that combination, rather than head pose plus voice activity alone.

## PR #865 — evidence link

[Pull request](https://github.com/autonomous-ai/lampOS/pull/865)

Reviewer: [P3] 22:52: the evidence link in `lampOS/docs/same-voice.md:11` points to `verification/two-people-20261015/README.md`, which is absent from this PR's head. Please link the existing #829 evidence directly, include the required report, or make the dependency on #864 explicit and wait for it before merge. The committed same-voice measurement files themselves are present, and their separation counts were independently reproduced.

## PR #865 — cosine scale

[Pull request](https://github.com/autonomous-ai/lampOS/pull/865)

Reviewer: [P3] 22:52: `lampOS/crates/same-voice/src/lib.rs:194-197` does not preserve cosine under scaling. Against the freshly built public library, `cosine([0.0001, 0], [0.0001, 0])` returns about 0.083886 rather than 1 because the product of norms is clamped to f32 epsilon; finite `[1e20, 0]` compared with itself returns NaN from f32 overflow. Use stable normalization/accumulation and explicitly handle undefined inputs, with small- and large-scale regression cases. This is a public helper correctness finding; it does not establish that the recorded CAM++ embeddings had those magnitudes.

## Issue #823 — merged 861

[Issue](https://github.com/autonomous-ai/lampOS/issues/823)

Reviewer: merged #861 at 22:50: explicit clocks for the evaluator backend and fake player, with default system clocks and host cleanup bounds retained; #823 stays open for the remaining realtime migration.

## PR #861 — complete b34d181d9

[Pull request](https://github.com/autonomous-ai/lampOS/pull/861)

Reviewer: 22:49: the complete fresh serial gate on `b34d181d9` has finished: 289 passed / 0 failed / 0 ignored, no retries. All 12 physical-loopback and 14 serve-session cases ran, in addition to the five new clock regressions. Fmt and strict all-target Voice-eval Clippy pass; exact-head pull-request ARM64 remains green. The independent source-preservation and frozen-clock drain/actual child-reap checks are recorded above. No remaining finding. This is an explicit evaluator API with unchanged default constructors and documented host cleanup bounds; #823 remains open for the full clock migration.

## PR #864 — first handoff

[Pull request](https://github.com/autonomous-ai/lampOS/pull/864)

Reviewer: for the first builder: 22:49: Fourth has no lamp and #829 explicitly requests your on/off played scenes. The two source findings above need correction before those scenes can establish this behavior. The switch-off path matched the actual pre-PR classifier/rule across 440 independent text cases; fresh fmt and strict Voice Clippy pass, and the complete serial Voice gate is running. The current head's ARM64 run is green. #829 remains open for its other steps and physical acceptance.

## PR #864 — modified nouns

[Pull request](https://github.com/autonomous-ai/lampOS/pull/864)

Reviewer: [P2] 22:49: implement the documented positive call positions in `lampOS/crates/voice/src/attention.rs:278-280`. The current test only rejects a name immediately after one listed determiner; it accepts every other occurrence. In independent strict-busy-room runs, “I bought a new lamp yesterday”, “The old lamp stopped working” and “Did you see her desk lamp?” all still produce Play with the switch on. None is first, after a greeting, or set off as a call. Account for ordinary modified noun phrases and add those held-answer regressions, while keeping the explicit vocative controls. The PR's claim that it counts the name only when addressed is not yet true.

## PR #864 — vocative punctuation

[Pull request](https://github.com/autonomous-ai/lampOS/pull/864)

Reviewer: [P2] 22:49: preserve comma-separated direct calls in `lampOS/crates/voice/src/attention.rs:273-280`. Splitting away punctuation before checking the preceding word makes “Can you explain that, Lamp?” and “What is this, Lamp?” return false because `that`/`this` is in `SPOKEN_OF`. An independent run through `Attention::first_output_busy` on this head changes both from Play with the switch off to Hold with it on, contradicting the documented final-vocative behavior. Preserve punctuation/clause boundaries so these explicit calls remain positive, and add both classifier and worker-loop regressions.

## PR #822 — rebase needed

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: 22:45: GitHub now reports `4f8208412` conflicting with `lamp-voice` after the intervening merges. Please rebase while retaining both sides of any topic/catalogue additions, and refresh the description's second lamp run and gate/CI paragraphs as requested above. The completed review of this head is retained; I will inspect the rebase and verify the new head's gates/CI rather than treating the old head as current. #813 remains open for the Policy reader.

## PR #842 — ci green 9dc615f35

[Pull request](https://github.com/autonomous-ai/lampOS/pull/842)

Reviewer: 22:45: the replacement exact-head pull-request ARM64 run [38760659983](https://github.com/autonomous-ai/lampOS/actions/runs/38760659983) is now green on `9dc615f35`: 4,730 passed / 126 skipped. This completes the CI item in the preceding review; the documentation finding is resolved and the recorded source-identical local gate remains applicable. The camera-privacy and #836 physical acceptance handoff to First remains.

## PR #858 — complete 7c06dfd28

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: 22:45: fresh gate complete on `7c06dfd28`: fmt and strict all-target Voice Clippy pass. Full serial Voice retained 1,337 passed / 2 failed / 8 ignored. The two unchanged Second-route/privacy-freshness cases at `coordinator/tests/second_route.rs:158` each passed one isolated retry from the preserved original executable: `spoken_over_the_second_routes_answer_stops_as_gemini_does` and `without_a_second_route_the_packets_and_records_are_as_before`. The full-run failures remain recorded; no broad retry. [Exact-head pull-request ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38757458025) is green.

The independent forged-completion reproduction passes with the fix, as recorded above. The physical-stability P2 at `lampOS/crates/voice/src/onboarding.rs:375-383` remains: framing can complete during head/hand motion because no such state reaches the hold. Please implement and cover First's motion requirement, or obtain an explicit scope decision on #841.

Reviewer: for the first builder: real onboarding behavior and the unresolved #841 acceptance remain with you. The local gate does not establish those physical conditions; #841 stays open.

## PR #861 — first source review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/861)

Reviewer: 22:45: source review of `b34d181d9` finds no blocker. The new clock APIs are explicit opt-ins; existing constructors keep the original system-clock calls, ordering and host cleanup bounds. I independently compared all 27 existing test bodies/assertions, 15 constants and nine sleep expressions against the parent: unchanged. An additional scratch proof using the unchanged backend with a frozen logical clock completed the host drain in 328.5 ms, sampled the logical clock once and verified the actual owned child was reaped.

Fresh fmt and strict all-target Voice-eval Clippy pass; all five new regressions pass in the initial unit suite. The complete serial evaluator gate is still running through its real-process scenarios, so this is not yet a completed local gate. Exact-head pull-request ARM64 [38756122123](https://github.com/autonomous-ai/lampOS/actions/runs/38756122123) is green, 4,662 passed / 126 existing profile skips. #823 remains open for the remaining scheduler/fixture migration; this preparation claims no scenario speedup.

## Issue #854 — merged 859

[Issue](https://github.com/autonomous-ai/lampOS/issues/854)

Reviewer: merged #859 at 22:44: explicit staff requests hand over behind LAMP_TASKS_ASKED=on, with one recipient-naming confirmation; First accepted the completed #854 scenes.

## PR #859 — complete c7d41bcf3

[Pull request](https://github.com/autonomous-ai/lampOS/pull/859)

Reviewer: 22:43: review complete on `c7d41bcf3`. The description and `lampOS/docs/tasks.md:756-757` now match the final recipient-naming read-back and `after_own_story`; the P3 is resolved. Exact `LAMP_TASKS_ASKED=on` is required, the off test retains the old function declarations and note, and neither dispatch/confirmation nor privacy/shared message meanings change. No remaining source finding.

Fresh fmt and strict Voice all-target Clippy pass. The correctly permitted full serial Voice run retained 1,331 passed / 3 failed / 8 ignored. The three unchanged EarlyEnd/privacy-freshness cases at `coordinator/tests/early_end.rs:472` each passed one retry from the preserved original executable: answer after sure, resumed speech at three tenths, and dropped continued link. These are the Queue #739 timing family; the original failed full run remains recorded. All PR-specific regressions passed in that run. The initial sandbox-denied attempt is separately retained as invalid local-socket/worker execution; correcting permissions used the same binaries and is not counted as a flaky-test retry.

The locally gated `afe87d6d9` production/tests, Cargo inputs, toolchain and workflows are byte-identical to this docs-only head. [Exact-head pull-request ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38759237170) is green: 4,686 passed, 126 skipped. No CI rerun was requested.

First's current-code lamp scenes and [22:06 model result](https://github.com/autonomous-ai/lampOS/pull/859#issuecomment-6195305574) resolve the earlier acceptance handoff: `after_own_story` 5/5, `mid_story` 4/4, the other five scenes 5/5, with the one failed session disclosed. This is First's evidence, not a reviewer lamp run. I will rebase-merge this reviewed head and close #854 as completed; #799's broader latency work remains open.

## PR #842 — evidence resolved 9dc615f35

[Pull request](https://github.com/autonomous-ai/lampOS/pull/842)

Reviewer: the evidence finding is resolved at 9dc615f35 (lampOS/docs/verification/full-look-lamp-52e6-20261015/README.md:52-58). First-word latency is now explicitly unmeasured, and the unbound audio gap is labelled only as an unverified observation that may belong to another turn. The PR description now agrees on six matched answers plus one ambiguous, and on the coordinator-acceptance timing boundary.

Only the verification README changed from tested 2798717b9. All production/tests, Cargo inputs, toolchain and workflows are byte-identical, so the prior gate is carried by source identity: fmt and strict Camera/Eyes/Voice Clippy pass; the first full serial run was 1456 passed / 2 failed / 8 ignored; both unchanged timed failures passed their single original-binary retries; all eight independent regressions passed. No redundant full suite was run for these docs commits. The new exact-current ARM64 run 38760659983 is still running; the superseded475 run was cancelled after its tests passed and is not counted as green.

Reviewer: for the first builder: camera privacy ownership and the remaining #836 real-object/person and question-to-first-word acceptance remain with you; #836 stays open.

Checked at 22:35.

## PR #795 — complete f0c55a49c

[Pull request](https://github.com/autonomous-ai/lampOS/pull/795)

Reviewer: 22:35: rebase review complete on `f0c55a49c`. The Msgs conflict keeps both logging and privacy catalogues; all 21 added privacy implementation/test/doc files are identical to the previously reviewed head. No new source finding.

Fresh fmt and strict all-target Clippy pass for Controls, Ears, Mood, Msgs, Ring and Voice. The original full serial gate is 1,991 passed / 2 failed / 11 ignored. Both unchanged failures passed once from the original preserved executable: EarlyEnd resumed-speech/privacy freshness at `coordinator/tests/early_end.rs:472` in 6.40 s (known #739 family), and the reminder room-change socket reset at `provider_worker.rs:5298` in 0.66 s. No matching Queue waiver was established for the latter; its isolated pass is diagnostic, not a claimed clean full gate. No broad retry. [Exact-head native ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38756093472) passed 4,737 tests, 127 skipped, including the Linux Controls-loss privacy-worker regression.

Reviewer: for the first builder: this changes the microphone/mute privacy path and still needs the #755 physical-slide acceptance. The host review does not establish wiring/polarity, real microphone-handle closure or visible ring timing. #755 stays open.

## PR #858 — report bypass fixed 7c06dfd28

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: 22:30: the forged completion P2 is fixed at `7c06dfd28`. The unchanged independent reproduction now receives an error from `onboarding::run` for `where_you_sit=done`, and the actual coordinator module leaves the step incomplete with no face/active step. The mailbox also requires the currently led step and accepts only a skip for this measured step. I rebuilt this proof against the current module and matching current libraries.

The previous head's complete serial gate is retained: 1,337 passed / 1 failed / 8 ignored; the unchanged EarlyEnd/privacy timing test passed its one same-original-binary retry. A fresh complete gate is running for this code-changing head. The separate physical-stability P2 remains unchanged: the three-second face hold still has no head-motion or body-hand input, as requested in First's #841 additions. #841 and the existing First hardware handoff remain open.

## PR #855 — merged issue 803

[Pull request](https://github.com/autonomous-ai/lampOS/pull/855)

Reviewer: merged #855 at 22:30 (`3b146029d099`): opt-in spoken marking of the last 60 seconds, local listing and seven-day expiry on startup/mark/list, with failed-read and expiry fixes; complete Voice gate and exact-head ARM64 passed.

## PR #848 — merged issue 711

[Pull request](https://github.com/autonomous-ai/lampOS/pull/848)

Reviewer: merged #848 at 22:28 (`59697b459ac2`): optional body/hand publication with source-age expiry preserved through queue and filesystem delays; #711 stays open for its touch reader and physical qualification.

## PR #797 — merged issue 711

[Pull request](https://github.com/autonomous-ai/lampOS/pull/797)

Reviewer: merged #797 at 22:27 (`aab0133d98dd`): the manual touch calibration/replay tool, with the corrected own-swipe cutoff and release/debounce footprint; #711 remains open for head-pad interactions and physical qualification.

## PR #842 — residual measurement 4754aa2ab

[Pull request](https://github.com/autonomous-ai/lampOS/pull/842)

Reviewer: [P2] The README's score and capture-boundary correction is right at 4754aa2ab: six answers matched to their displayed trials, the banana result explicitly ambiguous, and 622–1060 ms labelled as ask to coordinator acceptance. One measurement claim remains at lampOS/docs/verification/full-look-lamp-52e6-20261015/README.md:52-55: “2.0–3.6 s in seven looks; in three, no answer came before the next question” is still presented as measured, immediately before noting that per-request first-word timestamps were not kept. The retained look-timing.txt contains ask/still durations only, and the late banana remains unbound to its request. Remove this timing/count claim or label it clearly as an unverified observation; retain request-bound first-word events before using it as the issue's comparison. Please also update the PR description's old 7/10 and latency/cause claims to match the corrected README.

All production and test trees, Cargo files, toolchain and workflow files are identical to tested 2798717b9; no full gate was repeated for this README-only commit. The carried result is unchanged: fmt and strict Camera/Eyes/Voice Clippy pass; the first full serial run was 1456 passed / 2 failed / 8 ignored, both unchanged timed failures passed once from the original binary, and all eight independent regressions passed. Exact-current ARM64 is still running. Camera privacy and the remaining #836 real-object/person and question-to-first-word acceptance remain with the first builder.

Checked at 22:27.

## PR #797 — ready description corrected

[Pull request](https://github.com/autonomous-ai/lampOS/pull/797)

Reviewer: 22:26: the description now matches `10585b465` and its reviewed own-swipe cutoff, hysteresis/debounce footprint, gate and ARM64 evidence. All four earlier calibration findings remain fixed. The retained independent CLI and debounce proofs pass; fmt and strict Controls Clippy passed. The original complete serial run had 382 passes and four unchanged missing-stderr/output failures; each passed its one same-binary isolated retry, retained under the existing #783/#837 program-output investigation. That is 386 distinct passing cases after retries, not a clean first run. [Exact-head native ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38736016630) is green.

Under [First's explicit #711 direction](https://github.com/autonomous-ai/lampOS/issues/711#issuecomment-6192051506), this is ready to merge as a manual calibration/replay tool. It does not enable touch or change served behavior. #711 stays open for the head-pad interaction and physical qualification work.

## PR #855 — complete 7336b004e

[Pull request](https://github.com/autonomous-ai/lampOS/pull/855)

Reviewer: 22:23: both marked-record P2s are fixed at `7336b004e`. The original independent tests now pass: listing removes an eight-day-old mark without another mark being made, and unreadable metadata returns an error rather than a successful empty mark. The code/docs explicitly run expiry on enabled voice startup, new marks and listing; this is maintenance-triggered expiry, not a new periodic timer.

Fresh `cargo fmt --all --check`, `cargo clippy -p lamp-voice --all-targets -- -D warnings`, and `cargo test -p lamp-voice --no-fail-fast -- --test-threads=1` pass: 1,325 passed, 0 failed, 8 ignored, without retries. This is the full new-head gate; the old head's failed run remains recorded above. [Exact-head native pull-request ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38755647012) is green.

Switch-off declaration/routing behavior is covered. The committed lamp-52e6 report shows the spoken mark, one thanks, the 60 s copy and listing, and discloses the first split-turn failure and host-only expiry verification. No microphone/camera opening or mute path changes. The source and docs now match the #803 scope; no remaining finding. Ready for rebase merge.

## PR #848 — complete df932e22a

[Pull request](https://github.com/autonomous-ai/lampOS/pull/848)

Reviewer: 22:23: the freshness follow-up is fixed at `df932e22a` (`lampOS/crates/body/src/bus_out.rs:341`). Hand's age is now taken after JointState publication. The unchanged independent test blocking the real earlier JointState file open for 1.2 s now passes: no fresh Hand is created from an expired observation. Delayed queue/stop and partial remaining-lifetime cases also pass.

Fresh fmt, strict Body/Msgs all-target Clippy and complete serial tests pass without retries: 379 passed, 0 failed, 0 ignored. [Exact-head native ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38756164405) passed 4,662 tests, 126 skipped, including the FIFO regression. All six independent checks and four switch subprocess cases pass. Six actuator/report/existing-message scenarios match the base byte for byte (1,644,402 bytes).

No source finding remains. This adds the optional Hand publication under the existing explicit Bus switch and leaves motor output and existing message meanings unchanged. It is ready for rebase merge; #711 remains open for the touch reader and physical qualification work.

## PR #822 — complete 4f8208412

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: 22:23: the updated deadline/test follow-up is reviewed on `4f8208412`. Both production entry points retain the nominal 300 ms bound. Four independent real-store checks show one budget shared across room/person snapshots and rules: the production note returned in 302.2 ms with unfinished rules omitted; a requested 10 s production open still returned in 300.4 ms; an explicit 120 ms test budget returned in 120.2 ms; an explicit 2 s budget completed the same delayed reads. These are host measurements, not lamp latency.

Fresh fmt and strict Msgs/Memory/Voice all-target Clippy pass. Full serial suites pass with no retries: Msgs 85, Memory 159 (5 ignored), Voice 1,338 (8 ignored), total 1,582 passed / 0 failed / 13 ignored. Both earlier PR-owned note failures and the inherited usage-tail test pass in this original run. [Exact-head pull-request ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38755864147) passed 4,670 tests, 126 skipped. The previous twelve independent Rules correctness proofs remain applicable by exact source identity; the logging/catalogue rebase retains both topic sets. No further source finding.

The [second lamp-52e6 report](https://github.com/autonomous-ai/lampOS/blob/4f8208412787a9ab4f9bf356612153edeb664472/lampOS/docs/verification/rules-second-run-lamp-52e6-20261015/README.md) supplies the spoken implicit-correction and correct-rule deletion demonstration, with its known-person stand-ins explicitly disclosed. Please refresh the PR body's still-pending second run and stale gate/CI paragraphs before merge. #813 stays open: `lampOS/docs/policy.md:81-89` correctly says Policy does not yet read `memory/rules`; that reader/integration is separate work.

## PR #815 — merged issue 811

[Pull request](https://github.com/autonomous-ai/lampOS/pull/815)

Reviewer: merged #815 at 22:19 (`74eb16c73a20`): manual product installation and link-based updates, including recovery from interrupted and repeated failed writes; #811 stays open for setup/keeper bootstrap and Harness boot service/linger.

## PR #815 — complete d5c033983

[Pull request](https://github.com/autonomous-ai/lampOS/pull/815)

Reviewer: 22:17: all posted installer findings are fixed at the actual PR head `d5c033983`. The nine original independent recovery checks now pass, including real child exits before systemd debt handling and after a settings write, retrying failed enable, preserving custom/pre-existing files, and two successive partial-write failures. The latter retains the previous written hash before replacing the pending intent, so the third run installs the new defaults instead of treating the old default as a per-lamp customization. No new source finding.

Fresh `cargo fmt --all --check`, `cargo clippy -p lamp-update --all-targets -- -D warnings`, and `cargo test -p lamp-update --no-fail-fast -- --test-threads=1` pass: 69 passed, 0 failed, 1 ignored (the existing external-network release-file test), no retries. The [actual #815 pull-request ARM64 run](https://github.com/autonomous-ai/lampOS/actions/runs/38757082810), rather than draft #863's run, is successful on this exact head.

The manual install/product-link path leaves existing named-build units unchanged, covered by the unchanged legacy-round controls and mixed-path tests. The scope and documents match the install work reviewed by Second and handed here by First at 22:04. This is ready for rebase merge. #811 will remain open: setup/keeper bootstrap and the requested Harness boot service/linger work are not supplied by this PR.

## PR #797 — final followup 10585b465 2216

[Pull request](https://github.com/autonomous-ai/lampOS/pull/797)

Reviewer: 22:16: the reviewed code remains `10585b465`; no further test rerun is needed for an unchanged head. The remaining description correction from 19:56 is still outstanding: item 4 says thresholds use the weakest swipe, but `lampOS/crates/controls/src/touch_calibrate.rs` now uses the weakest own swipe whose peak reaches 50% of that pad's strongest, with hysteresis/debounce replay for the footprint. Please update that item and the old head/test-count paragraphs to the current reviewed behavior and evidence. First's #711 direction keeps this a manual tool; the unrelated head-touch work stays open.

## PR #845 — final followup 9b4f499f3 2216

[Pull request](https://github.com/autonomous-ai/lampOS/pull/845)

Reviewer: 22:16: the DNS-worker lifetime P2 is fixed for the shipped setup path on `9b4f499f3`. The original independent reproduction runs three actual failed setup attempts with fresh ImageHost instances: only one resolver is started and remains active across all three. HTTP serializes joins and ImageHost uses one fixed address, so the process-wide slot now bounds the worker's lifetime across retries.

Fresh fmt, `cargo clippy -p lamp-setup --all-targets -- -D warnings`, and complete serial Setup tests pass: 34 passed, 0 failed, 0 ignored, no retries. All six independent checks pass: repeated setup, same-host control, shared DNS/TLS deadline, slow TLS records, actual port ownership and startup-failure short-circuit. Both serving CLIs remain disabled without `--enable`. [Exact-head pull-request ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38744254417) is green.

Two API limits matter if Reach gains callers beyond today's serialized, fixed-address setup: at `lampOS/crates/setup/src/wifi.rs:410-414`, the blocking slot mutex lets an 80 ms check wait 602 ms behind a concurrent 600 ms check; at lines 417-423, a completed lookup for a different address remains reserved and later addresses never start. The independent API probes reproduce both. They are not reached by the current product path and are not a current setup merge blocker; bound lock acquisition and retire completed other-address receivers before supporting those uses.

Reviewer: for the first builder: Third has no lamp/phone/captive-network evidence, so the existing wiped-lamp-to-conversation acceptance remains yours and #806 stays open. The PR body should also refresh its old 33-test/pending-CI status to the completed evidence above.

## PR #842 — measurement attribution 2798717b9

[Pull request](https://github.com/autonomous-ai/lampOS/pull/842)

Reviewer: [P2] Keep the object score and timing claims tied to the recorded request and measurement boundary (`lampOS/docs/verification/full-look-lamp-52e6-20261015/README.md:37-47`). The retained `looks-after.txt:9-10` has the previous book answer during the banana trial; the banana answer appears under headphones at lines 19-20. The table supports six answers matched to their displayed trial. Without answer request IDs/timestamps it cannot establish that the late banana belongs to the banana trial, yet the README counts it in 7/10 and says only three trials had no timely answer. Preserve the corresponding request-bound raw events, or label that seventh result ambiguous and correct the scored/timely-answer claims.

The 622–1060 ms values are also a different boundary from camera readiness. `coordinator.rs:4564-4583` records `look_full_still` only after the lamp's previous reply has ended and the provider/readiness gates permit the next turn; a still already ready during “Let me look” waits there. These values measure ask→coordinator acceptance, not ask→capture completion. All three retained full-look journal lines (`eyes-journal.txt:1,5,9`) report head wait 0 ms, so “mostly the settle wait” is not supported. Label the measured boundary accurately, remove the unsupported cause, and retain per-request camera-ready and first-word timestamps before comparing those targets. No additional lamp run is requested of this reviewer.

The request-ID source fix itself passes the independent late-old-capture regression on 2798717b9. This finding concerns the evidence and completion claims, not a reproduced remaining request-correlation failure. Objects held by a person, turning toward that person and the requested question→first-word comparison remain unshown as the document already notes. #836 should remain open for those acceptance items unless the first builder explicitly changes its scope.

Checked at 2026-10-15 22:00:51 +0700.

## PR #859 — followup afe87d6d9 2211

[Pull request](https://github.com/autonomous-ai/lampOS/pull/859)

Reviewer: [P3] Update the review description and regression reference to the final unclear-name behavior (`lampOS/docs/tasks.md:756-757`). `afe87d6d9` removes the `not sure` choice, uses one recipient-naming read-back, and replaces the model-check scene with `after_own_story`. The PR body still promises `not sure`, nothing prepared, and the old `asked_again` scene; the doc cites removed test `a_name_not_made_out_is_asked_about_and_never_guessed`. Change those to the final behavior and existing `a_name_not_made_out_is_named_in_the_read_back_and_asked_about_once` test, so the reviewable scope and evidence agree.

I have read First's current-head scene acceptance and 22:06 model-check result, including `after_own_story` 5/5 and one failed session. The host source review/gate is continuing; I am not transferring the prior head's gate to this one. Checked at 22:11.

## PR #844 — followup a31223254 2211

[Pull request](https://github.com/autonomous-ai/lampOS/pull/844)

Reviewer: 22:11: both timezone P2s are fixed on `a31223254`. All seven independent current-library regressions pass: fallback-file reload with missing/broken localtime, spring/fall retention, fixed offsets, half-hour DST, arithmetic bounds and person isolation. The rebase retains both fixes byte-for-byte and preserves the new Settings workspace member. No new timezone source finding.

Fresh fmt and strict all-target Clippy pass for LocalTime, Expression, Memory and Policy. Complete serial gate: 555 passed, 3 failed, 7 ignored. These inherited stderr assertions failed: Memory `tests/binary.rs:216` (usage), Memory `tests/binary.rs:540` (legacy-refusal text), Policy `tests/program.rs:448` (usage). Each received one same-original-binary retry: legacy refusal passes; both usage tests still fail. Those two failures cannot receive the role's passes-alone waiver; the local gate is not green. Original results and binaries are retained, with no broad rerun.

Second's committed Policy/shared-reader approval is recorded. Seventh's committed Memory read required by First on #820 is still missing from the PR and issue. The PR body's two-stat claim also needs updating to the final reader's three stat calls plus readlink. The previous First hardware handoff and #820 acceptance remain; I have not merged this head.

## PR #842 — followup 2798717b9 2211

[Pull request](https://github.com/autonomous-ai/lampOS/pull/842)

Reviewer: 22:11: the request/capture correlation source finding is fixed on `2798717b9`. Eight independently rebuilt regressions pass, including selecting an old capture before a newer real Unix-datagram ask, then finishing that old write: it is rejected until the new request's own capture completes. The stationary, movement, switch-off and parser/bounds controls also pass. Voice retains its previous 720-height / 2-MiB bounds while Camera/Eyes allow the explicit full-look mode.

Fresh fmt and strict all-target Clippy pass for Camera, Eyes and Voice. Complete serial tests: Camera 75 passed; Eyes 46 passed; Voice 1,335 passed, 2 failed, 8 ignored. Both unchanged timed cases (motor preflight at `coordinator/motor/tests.rs:453`, reminder AckConfirmed at `coordinator/tests/reminder_roundtrip.rs:338`) passed once from the same original test binary. The first full run remains 1,456 passed / 2 failed / 8 ignored. [Exact-head native ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38754157425) passed 4,613 tests, 124 skipped, including the Linux-only negotiated camera-format and Eyes bound tests.

The evidence/measurement P2 above remains. Reviewer: for the first builder: camera privacy ownership and the remaining #836 real-object/person and question-to-first-word acceptance stay with you; #836 remains open.

## PR #857 — review complete 81009f917

[Pull request](https://github.com/autonomous-ai/lampOS/pull/857)

Reviewer: 22:09: host review complete on `81009f917`. The explicit `--give-way off --limp off` parser values match the old defaults, and the scripted off comparison checks identical stop, goal-write and register-write output against `Limits::FOLDED`; the enabled comparison differs, so the test can detect a change. No source finding.

Fresh `cargo fmt --all --check`, `cargo clippy -p lamp-body --all-targets -- -D warnings`, and `cargo test -p lamp-body --no-fail-fast -- --test-threads=1` pass: 292 passed, 0 failed, 0 ignored, without retries. [Native pull-request ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38746301909) is green on this exact head.

Reviewer: for the first builder: this changes default physical behavior at `lampOS/crates/body/src/bin/lamp-body.rs:438-439`; your hand-push, let-go and limp acceptance remains scheduled for the owner's 11:00 minute. This remains your merge and #703 stays open.

## PR #853 — review complete 33bfb012d

[Pull request](https://github.com/autonomous-ai/lampOS/pull/853)

Reviewer: 22:09: host review complete on `33bfb012d`. The actual coordinator path now retains a provider-completed turn only while it has no returned audio/playback and the turn, connection and microphone generation still match; the next admission can use that retained turn within the one-second bound. The new tests exercise actual completion and later admission, including switch-off behavior. No further source finding; the removed Python finding is resolved.

Fresh `cargo fmt --all --check`, `cargo clippy -p lamp-voice --all-targets -- -D warnings`, and `cargo test -p lamp-voice --no-fail-fast -- --test-threads=1` pass: 1,321 passed, 0 failed, 8 ignored, with no retries. The [native pull-request ARM64 run](https://github.com/autonomous-ai/lampOS/actions/runs/38755355783) is successful on this exact head.

Reviewer: for the first builder: the quiet-room continuation and busy-room floor-hold acceptance for #782 still need your lamp judgement. Fourth reports no lamp run; host tests do not establish that acceptance. #782 stays open.

## PR #818 — review complete 3076f8452

[Pull request](https://github.com/autonomous-ai/lampOS/pull/818)

Reviewer: 22:09: the fallback checked-sound P2 is fixed on `3076f8452`. I reran the original independent proof against the fresh library with the accepted worker flags: `--attention --sounds-checked`, `End.sound=true`, and the original `¿Qué?` transcript now produce zero audio packets (previously two). The checked flag reaches the Second Engine, which preserves the sound marker through transcription; accepted, unchecked and unmarked controls remain covered. No new source finding.

Fresh `cargo fmt --all --check` and `cargo clippy -p lamp-voice --all-targets -- -D warnings` pass. Complete `cargo test -p lamp-voice --no-fail-fast -- --test-threads=1`: 1,325 passed, 1 failed, 8 ignored. The sole unchanged EarlyEnd/privacy timing case at `lampOS/crates/voice/src/coordinator/tests/early_end.rs:472` passed its one isolated retry from the same original binary (6.40 s). This is the existing Queue #739 timing family, so it is noted; the first full run was not clean. [Exact-head ARM64](https://github.com/autonomous-ai/lampOS/actions/runs/38743411053) passed 4,592 tests, 118 skipped.

Reviewer: for the first builder: Second has no lamp and requests the activation minute; recorded-sound effectiveness and short-word preservation still need the #800 acceptance. #800 remains open.

## PR #850 — issue 846 close

[Pull request](https://github.com/autonomous-ai/lampOS/pull/850)

Reviewer: 22:06: merged #850 (`c0f79cac95bd`) into lamp-voice, fixing the live Bus-recording test snapshot race while preserving all original assertions; all 66 Bus tests and the exact-head ARM64 gate passed.

## PR #850 — full review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/850)

Reviewer: 22:01: review of 21c9b55b8 is complete. Fresh cargo fmt --all --check, cargo clippy -p lamp-bus --all-targets -- -D warnings, and cargo test -p lamp-bus --no-fail-fast -- --test-threads=1 all pass: 66 passed, zero failures or ignored tests, including all 11 CLI cases. No retries.

I independently reversed only the new helper/regression and four polling predicates: the original cli.rs is byte-identical, preserving all 190 assertions and all 10 original test identities. Partial live snapshots wait; the post-exit strict JSON/LF parser still rejects malformed or partial output. Production recording and shared Bus contracts are unchanged. Exact-head pull_request ARM64 run 38741600267 is green. No finding; #846 is fully addressed by this scoped change, while #783 and #837 remain separate.

## PR #852 — issue 823 merged

[Pull request](https://github.com/autonomous-ai/lampOS/pull/852)

Reviewer: merged #852 at 21:59: explicit evaluator relay clock API with the system-clock path preserved; #823 stays open for migrating the realtime tests.

## PR #852 — full review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/852)

Reviewer: 21:57: the exact fef1377ef gate passed: cargo fmt --all --check; cargo clippy -p lamp-voice-eval --all-targets -- -D warnings; cargo test -p lamp-voice-eval --no-fail-fast -- --test-threads=1. All 287 tests passed, zero failures or ignored cases, including the three new clock regressions and every existing realtime case. No retry. The complete run finished at 21:56:05 from date.

The explicit API is opt-in; existing callers retain their original system-clock path, sampling order, ten-second grace and host cleanup bounds. No existing shared contract changes, source finding or undocumented timing claim remains. Exact-head pull_request ARM64 run 38739080219 is green. Ready for rebase merge; #823 remains open for the actual multi-process clock migration.

## PR #853 — python resolved

[Pull request](https://github.com/autonomous-ai/lampOS/pull/853)

Reviewer: 21:55: verified 33bfb012d removes the Python file and records its counting method in the README. That finding is resolved; all production and test source files are unchanged from 71f3888b7. The fresh full local Voice gate is now on 33bfb012d. The lamp-4ace handoff above remains.

## PR #855 — full gate

[Pull request](https://github.com/autonomous-ai/lampOS/pull/855)

Reviewer: 21:55: the full host gate on 8e5df5650 finished. Formatting and strict all-target Voice Clippy pass. Serial tests: 1,321 passed, 2 failed, 8 ignored; all new mark/tool tests pass. Each inherited failure was run once from its original executable: diagnostics::audio_fault_end_is_incomplete_even_with_good_written_pcm passes, but reminders::consumer::tests::completed_playout_survives_both_real_store_reopens_then_acks_without_respeech still fails its AckConfirmed assertion at reminders/consumer/tests.rs:644. Original failures and both isolated results are retained; no broad rerun. The complete host gate is not green. The separately reproduced retention and failed-read P2s above remain unchanged.

## PR #795 — rebase 97df complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/795)

Reviewer: 21:54: the 97dfbc78e rebase preserves the reviewed privacy implementation; the coordinator adaptation retains the newer startup/Second-route supervision. Fresh fmt and strict all-target Clippy pass for Controls, Ears, Mood, Msgs, Ring and Voice. Full serial tests: 1,978 passed, 1 failed, 11 ignored. The only failure was the unchanged ring_trace_full_600_seconds_keeps_voice_transitions_and_fault_within_cap IPC-timing test; its one isolated invocation from the same original executable passes (30,000 real socket frame/receipt pairs). The original failure is retained. Exact-head pull_request ARM64 run 38746376142 is green: 4,675 passed, 125 skipped, including the Linux worker-loss privacy regression. No new source finding.

Reviewer: for the first builder: this changes microphone/mute privacy and still needs your physical slide-switch judgement. #755 remains open. No lamp was used by this reviewer.

## PR #858 — model bypasses framing

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: [P2] 21:52: lampOS/crates/voice/src/onboarding.rs:483–489 accepts any uncompleted step reported through setup_step, including where_you_sit. The function schema at lines529–544 also offers that measured step to the model. In an independent run of the unchanged module, setup_step {step:where_you_sit,outcome:done} followed by tick with no face and no active step wrote Done with framing:null. That bypasses the three-second framing measurement completely. Only the runner should commit Done for where_you_sit; reject that model outcome and constrain reports to the active step, with an explicit skip path if desired. Add a no-face forged/computed tool-result regression.

## PR #848 — delayed joint publication

[Pull request](https://github.com/autonomous-ai/lampOS/pull/848)

Reviewer: [P2, freshness follow-up] Measure Hand's age after the preceding publication (lampOS/crates/body/src/bus_out.rs:328-339 at 6d7ac1b83). The original delayed-queue case is fixed, but `now` is captured before `self.joints.publish_json`, which performs filesystem I/O. A delay there is omitted from Hand's age; the Bus then assigns Hand a new wall timestamp with this overstated remaining life.

An independent current-source test blocks the actual JointState kept-value file open for 1.2 seconds using a local test FIFO, then releases it. No production method is copied or edited. Actual Publishing::step and Bus.latest return Hand { pushed: true, since_us: 3303 }, expires_ms=999 and expired=false, while the observation is already 1.205 seconds old and zero Body publishers remain. This injects a delay at the real earlier I/O boundary; it is not a claim about its frequency on a lamp.

Calculate age from a fresh Instant immediately before the Hand expiry/drop decision, after the JointState call; add the earlier-publication-delay regression alongside the queue-delay regression. Existing topic semantics need not change.

The original queue/stop replay now passes: a 1.205-second-old queued sample yields no Hand; a 605 ms sample retains only 395 ms, carries its observation timestamp, and expires after the original observation's lifetime. All required gates pass on this exact head: fmt; Body/Msgs all-target Clippy with -D warnings; serial tests 378 passed / 0 failed / 0 ignored. ARM64 pull_request run38741710781 is green for 6d7ac1b83. Six paired actuator/report/existing-message scenarios still match the base byte for byte (1,644,402 bytes), and unset/off/true/on switch subprocess checks pass. Five independent checks pass; the earlier-I/O freshness case fails101. No lamp was used; #711 remains open for its remaining reader and physical qualification work.

Checked at 2026-10-15 21:49:29 +0700.

## PR #822 — 512 complete review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: 21:50: all earlier Rules findings are resolved on 512edc585. Twelve independent actual-store cases pass, including complete rule state, invalid IDs, linked deletion, lost/malformed replies, overlapping writes and the real blocked declaration FIFO (session note returned in 140 ms while the Bus stayed blocked for 800 ms). Exact-head ARM64 run 38739754790 is green.

Fresh fmt and Msgs/Voice/Memory strict all-target Clippy pass. The complete serial gate finished with 1,565 passed, 3 failed, 13 ignored. Two new Memory voice_people assertions lacked their initial rule note; each passed its one isolated retry. Those retries used a clean rebuilt exact512 binary because the original artifacts had already been cleaned, and the original failures remain recorded. A shared 300 ms acquisition deadline is a plausible explanation, not a measured cause.

The third failure remains: memory/tests/binary.rs:217, a_relative_path_or_an_unknown_option_gets_the_usage_line_and_exit_code_two, loses the usage tail. Its single rebuilt retry also failed. That test and Memory main are unchanged here; it is consistent with the still-open #837 normal-exit stderr problem. The full local gate is therefore not green, and it does not meet the role exception requiring the failed test to pass alone. No further retry or CI rerun was performed.

The lamp-52e6 synthetic speech/known-person-stand-in evidence is recorded separately. Policy still does not consume memory/rules; #813 must not be closed as though quiet-hours policy is already enforced. Please refresh the PR body’s stale gate counts and one-shard CI description to the current head.

## PR #808 — issue 699 merged

[Pull request](https://github.com/autonomous-ai/lampOS/pull/808)

Reviewer: merged #808 at 21:47: explicit-start shared logging and durable lamp-settings; #699 remains open for next-morning log retention and reader integrations.

## PR #808 — merged

[Pull request](https://github.com/autonomous-ai/lampOS/pull/808)

Reviewer: 21:47: merged #808 by rebase as 8da8216a95e5481016fc6a64177c744be7f22d79. The reviewed head was 6715c079c. #699 remains open for retained logs and the remaining owner integrations.

## PR #852 — initial review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/852)

Reviewer: 21:47: the initial source review of fef1377ef is complete. Existing run callers still use monotonic_us through the new adapter; the timestamp/deadline sites and the separate post-SIGTERM sample retain their order. The new tests use real owned children and explicit read/progress barriers, and the documented host-clock cleanup remains unchanged. I found no issue in this API slice. Fresh formatting, strict Clippy and the complete serial lamp-voice-eval gate are running; I will report their final results before merging. #823 remains open for the coordinated clock migration.

## PR #855 — read failure

[Pull request](https://github.com/autonomous-ai/lampOS/pull/855)

Reviewer: [P2] 21:47: lampOS/crates/voice/src/marked.rs:177–189 converts failure to open the metadata directory or a segment into an empty/partial successful window. mark then writes mark.json and run returns ok:true, so the lamp thanks the person as though the diagnostic evidence was preserved. An independent exact-library case made metadata unreadable as a directory; mark returned success and kept zero records. Propagate I/O errors from the window reader (while still deliberately skipping incomplete/invalid individual lines), and test that a failed segment read cannot acknowledge a complete saved window.

## PR #855 — retention

[Pull request](https://github.com/autonomous-ai/lampOS/pull/855)

Reviewer: [P2] 21:47: lampOS/crates/voice/src/marked.rs:126 is the only call to the seven-day sweep. If the person makes no further mark, these transcript copies remain indefinitely: lamp-records deliberately excludes marked/, and even the nightly marked listing at lines233–257 does not sweep. An independent call through this exact library created an eight-day-old mark, listed it, and confirmed events.jsonl still exists. #803 requires deletion after seven days. Run expiry from a bounded periodic/startup maintenance path independent of another mark, and test expiry with no subsequent mark; make the retention wording match that boundary.

## PR #808 — final review 6715

[Pull request](https://github.com/autonomous-ai/lampOS/pull/808)

Reviewer: 21:45: fresh review of 6715c079c is complete. cargo fmt --all --check and cargo clippy -p lamp-bus -p lamp-msgs -p lamp-settings --all-targets -- -D warnings pass. Complete serial tests pass: Bus 81, Msgs 84, Settings 44; 209 passed, zero failed, 2 manual timing tests ignored, no retries. All workspace package artifacts were cleaned before building.

The real CLI proof passes disabled/no-created-path behavior, guarded durable save, exact retry, stale rival refusal, persistence/receipt after restart and clean shutdown. Production sources are byte-identical to the earlier reviewed c9d997e; the changed slow-follower test still reconciles every lost record with subscriber drops. Existing shared contracts and default startup are unchanged. Exact-head pull_request ARM64 run 38748726886 is green. No remaining code finding.

This implements the explicit-start log/settings slice. #699 remains open: next-morning log retention and reader integrations are still documented as unfinished. No lamp was used.

## PR #858 — first builder

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: for the first builder: 21:44: #858 has no coordinator run with a person on lamp-52e6 yet, as its description records. The first-five-minutes voice, consent, framing and timing need that evidence and your requested lamp-4ace walk. I am continuing the host review; #841 remains open for the other steps and the approved design additions.

## PR #858 — seating motion

[Pull request](https://github.com/autonomous-ai/lampOS/pull/858)

Reviewer: [P2] 21:44: lampOS/crates/voice/src/onboarding.rs:375–383 finishes the seating measurement from three seconds of acceptable face framing without any head-motion or body/hand condition. The coordinator at lines4673–4686 supplies no motion state either. This misses the explicit #841 acceptance condition added by First: hold framing only while the head is still, so tracking or a hand turning the lamp cannot qualify it mid-move. Gate the continuous three-second hold on the current motion/hand state and reset the hold when motion begins; add moving-face and held-lamp cases. The requested Expression cues and later re-offer of newly available steps are also absent from this version and should be accounted for in its scope before calling the design complete.

## PR #815 — d64 settings recovery

[Pull request](https://github.com/autonomous-ai/lampOS/pull/815)

Reviewer: [P2, recovery follow-up] Preserve the recovered setting hash before replacing its pending intent (proposed d64bae34e, lampOS/crates/update/src/install.rs:496-501). The original body-waits interruption now recovers, but a second failed attempt can still lose ownership of an untouched default.

I linked an independent test to this head's actual lamp_update library and used real filesystem write failures plus Records::save_state throughout. Run 1 writes first.json=default99, then the second setting's write fails, leaving hash99 in settings_writing and no settled hash. Run 2 brings default100 and recognizes the old file from hash99, but lines496-501 replace that pending hash with hash100 before writing; the first write then fails. Run 3 succeeds yet keeps first.json=default99 as “this lamp's own” and only updates the second file. No local setting was edited. The final assertion fails deterministically (exit101).

Reconcile a file that matches the old pending hash into durable settled ownership before overwriting that pending intent, or otherwise preserve both the observed owned value and the new intended value across each failed attempt. Add the two successive write failures with a changed default as a regression.

The exact proposed head passes fmt, Update all-target strict Clippy and the full serial gate: 68 passed / 0 failed / 1 ignored, without retries. Eight independent positive cases also pass, including the original real child exit86 before the old debt-save boundary, a real child exit87 after a settings write but before its commit, failed-enable/tmpfiles replay, custom and pre-existing settings, the 98/99/100 case, and unreadable-settings preflight. The old debt P2 is resolved on this proposal; this settings-recovery case remains.

This is still a separate proposed branch. #815 remains at 73a1fffa8 and no ARM64 run exists for second/install-gates. The documented setup/keeper image prerequisite is accurate, but the missing blank-lamp bootstrap keeps #811 incomplete.

Checked at 2026-10-15 21:42:05 +0700.

## PR #859 — first builder

[Pull request](https://github.com/autonomous-ai/lampOS/pull/859)

Reviewer: for the first builder: 21:43: I read your #854 lamp-4ace result. The unclear-name scene still guesses and prepares work rather than asking, and the model check reports asked_again 0/5. The declared not sure option at lampOS/crates/voice/src/agent_tasks.rs:264–273 and its deterministic test do not establish that the model uses it. Please retain that acceptance item while Second applies the chosen correction; host review continues. #854 and #799 remain open.

## PR #857 — first builder

[Pull request](https://github.com/autonomous-ai/lampOS/pull/857)

Reviewer: for the first builder: 21:43: lampOS/crates/body/src/bin/lamp-body.rs:438–439 enables give-way return and limp by default. Your documented lamp-4ace hand/limp check is still pending, so this remains your merge. I read the explicit off path and the off/on output-comparison regression; local gate results will follow.

## PR #853 — first builder

[Pull request](https://github.com/autonomous-ai/lampOS/pull/853)

Reviewer: for the first builder: 21:43: this fixes the actual smart-turn-fast/kept-start path requested for lamp-4ace, and Fourth reports no lamp run. The quiet-room continuation and busy-room floor-held limitation in #782 still need your judgement. I will finish the host review independently; #782 remains open.

## PR #853 — python evidence

[Pull request](https://github.com/autonomous-ai/lampOS/pull/853)

Reviewer: [P2] 21:43: lampOS/docs/verification/cut-keeps-name-20261015/floor_holds.py:1 adds Python under lampOS. The reviewer merge rule excludes Python, and joining.md explicitly keeps measurement scripts outside the repository. Keep the raw count evidence and document the measurement in the README, moving this script to scratch or replacing it with a Rust tool. Source review and the local gate are continuing.

## PR #851 — gate complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/851)

Reviewer: 21:40: independent verification of merged head 67ff28423 is complete. cargo fmt --all --check and cargo clippy -p lamp-voice --all-targets -- -D warnings passed; cargo test -p lamp-voice --no-fail-fast -- --test-threads=1 passed with 1,318 tests, zero failures and 8 existing ignored tests, completed at 20:21:23 from date. All workspace package artifacts were cleaned before the build. The three added false fields restore compilation without changing production behavior.

## PR #843 — issue 823 merged

[Pull request](https://github.com/autonomous-ai/lampOS/pull/843)

Reviewer: merged #843 at 21:40: owner-EOF cleanup for final_dispatch test transports; the remaining clock migration in #823 stays open.

## PR #843 — merged

[Pull request](https://github.com/autonomous-ai/lampOS/pull/843)

Reviewer: 21:40: merged by rebase as a583fda2889c7282014680f40d0a2ac95862ffa4. The exact reviewed head a25450bd7 passed formatting, strict all-target Clippy and all 284 lamp-voice-eval tests, including all 13 final_dispatch cases; its ARM64 run was green. #823 remains open for the clock migration.

## PR #844 — gate complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/844)

Reviewer: Fresh b5971fede gate: fmt and strict all-target Clippy pass for Local-time, Expression, Memory and Policy. Full serial tests: 551 passed, 1 failed, 7 ignored. The sole failure is Policy's bad_arguments_print_the_usage_and_end_with_status_2 at program.rs:448, missing its usage substring; one exact-test retry at the same source head passes. Cargo rebuilt the single-package feature selection for that retry, so this is not described as a same-binary retry. Original full-run failure retained.

The two posted P2s remain independently reproduced in four cases; three controls pass (fixed-offset retention, Lord Howe half-hour DST arithmetic, and person separation). Exact-head ARM64 38735994188 failed in the inherited Memory killed-process journal-count assertion: 1,813 passed before abort, 2,756 unrun. The agreed Memory/Policy owner reads were still absent at the last read. No lamp result or green full gate is inferred.

Checked at 2026-10-15 20:14 +0700.

## PR #842 — late old capture

[Pull request](https://github.com/autonomous-ai/lampOS/pull/842)

Reviewer: The original 2.7-second pre-ask-file timeout case and both moving-head regressions are fixed in 3774b2398; the independent stationary/parser/off controls also pass. The relevant source is unchanged in the current rebase 29dd45558.

Reviewer: [P2] A later write time still does not identify this request's capture (lampOS/crates/voice/src/sight.rs:115-118,155-160,174-179; eyes/src/main.rs:688-706). An earlier request can already have selected its frame and be fitting/writing it when Voice cancels that look and asks for another. Eyes cannot read the new datagram until its synchronous fit/keep finishes. That older selected frame then gets a modification time after the new ask, so Look::still, due and turn immediately accept it as the new request's picture.

The current-library proof uses the real Wanted, fit, Kept and Look, without sleeps or forged file timestamps: select the old request's frame, make the new ask, then finish the old keep. It reports still=true, due=true, with_picture=true, and the worker accepts the older frame. Reproduced on 3774b2398 and confirmed against identical relevant source in 29dd45558; this is an allowed host interleaving, not a measured frequency on a lamp. Carry an explicit request identity and capture result through the ask/still path and verify it at consumption; file mtime alone cannot establish that correspondence. New-head gate evidence will be reported separately.

Checked at 2026-10-15 20:14 +0700.

## PR #843 — review complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/843)

Reviewer: Review complete at a25450bd7, with no source finding. Fresh fmt and strict Voice-eval all-target Clippy pass; the full serial package run passes all 284 tests with no failures, skips or retries. All 13 final_dispatch identities execute and pass in that run (9.58 s in this debug build; this is not a comparison with the author's release timing).

Reversing the single helper replacement and its comment restores the entire original Rust file byte-for-byte. Independent shell checks show control lines are consumed without interpretation or output, stdout remains open until owner EOF, and the child exits successfully and is reaped in about 1.6 ms after EOF. The deliberately closing transport is untouched. Documentation records the removed helper lifetime and keeps #823's broader clock-migration target open.

Exact-head pull_request ARM64 run 38735742284 is green. This is within the tests builder's harness scope, changes no production behavior or shared reader contract, and contains no device action. Final merge-head/state check follows.

Checked at 2026-10-15 20:14 +0700.

## PR #848 — stale hand publication

[Pull request](https://github.com/autonomous-ai/lampOS/pull/848)

Reviewer: [P2] Preserve acquisition age when publishing queued hand state (lampOS/crates/body/src/bus_out.rs:331-338 at 3731dd46e). A queued Glimpse gets a new timestamp, pushed_since and full one-second expiry only when the publisher drains it. If publication is delayed, an old push becomes fresh evidence; stopped() also drains that backlog before the publishers disappear.

An independent source-level replay queues one hand=true sample, delays only the publication consumer for 1.2 seconds, then calls the actual stopped(Cancelled). The real Bus.latest returns pushed=true, expired=false and a full 1,000 ms expiry although the source observation is already 1.238 seconds old and no Body publisher remains. A head-touch reader can therefore suppress a current touch based on an expired observation, and since_us reports dequeue time rather than the push's origin.

Carry observation time/age with the queued sample and discard or expire stale Hand values instead of renewing them during drain. Cover publication backlog plus stop, as well as ordinary live cadence. This finding concerns the newly added Hand topic; no change to existing reader semantics is being requested here.

Checked at 2026-10-15 20:12 +0700.

## PR #818 — fallback sound bypass

[Pull request](https://github.com/autonomous-ai/lampOS/pull/818)

Reviewer: [P2] Enforce the checked-sound mode on the fallback route as well (lampOS/crates/voice/src/provider_worker.rs:194,213; second_provider/worker.rs:198-220; second_provider.rs:220 at 20e1f9605). The shared parser now accepts --sounds-checked for both workers, but second_provider::worker::serve does not use files.sounds_checked, and Engine discards End.sound. When Gemini is unavailable and a marked short sound goes through the Second route, the new check is silently absent.

The independent current-library proof parses --attention --sounds-checked successfully, feeds the actual Second Engine Start/Audio/End(sound=true), scripts STT as the original ¿Qué? case with no near/face evidence, and receives two audio packets. Source tracing confirms serve constructs that Engine without carrying the accepted flag. The primary worker's checked-mode regression passes and withholds the same kind of input. The existing second-voice.md attention limitation explains the gap but does not make the new mode enforced on this route.

Apply the checked gate to the Second route too, or explicitly suppress/refuse marked short sounds there while the mode is enabled. Extend the new both-workers test beyond parsing to the decision/output behavior. Fresh full 20e1 gate otherwise passes: fmt, strict Voice Clippy, 1,307 serial tests passed / 0 failed / 8 ignored; exact-head ARM64 is green. First-builder hardware acceptance for #800 remains separate.

Checked at 2026-10-15 20:11 +0700.

## PR #851 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/851)

Reviewer: I read the complete three-line fix at 67ff28423 and the reported integration failure. All additions are inside the Second worker/coordinator tests; false is consistent with those non-continuation Start fixtures and changes no production constructor or behavior. The first builder has already merged this as 426ad0991. Its ARM64 run is still pending; I will retain the independent Voice gate evidence separately from the author's completed gate and the merge decision. No new source finding from this first pass.

Checked at 2026-10-15 20:05 +0700.

## PR #845 — gate complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/845)

Reviewer: Completed host verification at ed7b24992: fmt, Setup strict all-target Clippy and all 33 serial tests pass without retries. Six independent cases verify the original fixes and expose the separate DNS lifetime finding above. DNS plus TLS consume one deadline (400 ms scripted DNS plus a silent TLS peer returns at 601.456 ms for a 600 ms budget); a dripping TLS record returns at 501.188 ms for a 500 ms budget. Real ephemeral-port checks confirm own-page startup can bind only after the AP's listener is released, and startup failures stop the later actions. Both real CLI modes without --enable remain disabled and invoke no system commands.

Exact-head ARM64 38736072232 failed in the inherited Memory killed-process journal-count assertion at memory/tests/binary.rs:309: 1,801 passed before the abort, with later tests unrun. No CI rerun was requested. The new DNS retry-lifetime P2 remains open; #806 still needs its first-builder phone/lamp acceptance.

Checked at 2026-10-15 20:01 +0700.

## PR #848 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/848)

Reviewer: First pass at 3731dd46e: I read the proposed body/hand publication and #711's head-touch suppression use. The main review checks are whether pushed/since_us survive the full push/release lifecycle, whether the stated 80 ms cadence holds, and whether the new publication leaves existing messages and body behavior unchanged. Fresh Body/Msgs gates follow the active Setup review. This is an additive topic; I will check its reader/switch boundary rather than treat it as an existing-field change. The PR explicitly reports no real hand-on-lamp demonstration, and no hardware action is part of this review.

Checked at 2026-10-15 20:00 +0700.

## PR #845 — dns retry lifetime

[Pull request](https://github.com/autonomous-ai/lampOS/pull/845)

Reviewer: [P2] Keep the pending DNS-work limit across setup attempts (lampOS/crates/setup/src/wifi.rs:385-389; main.rs:311-324 at ed7b24992). Reach retains a single pending resolver only for that instance. Each accepted setup attempt creates a fresh ImageHost, and dropping the previous Host does not stop its detached resolver thread. A lookup that outlives a failed join therefore remains active while the next attempt starts another; the service-wide claim of never more than one lookup does not hold.

An independent proof through the actual serve::run and Reach completes three failed setup attempts with the same per-attempt Host lifetime as main: active resolver workers grow 1, 2, 3 after each Host is dropped. Only the radio-join clock is accelerated; resolver threads, Reach deadlines and status lifecycle are real. A control with three checks on one Reach keeps exactly one worker. Keep a resolver slot owned by the setup service across retries, or use cancellable/bounded resolver work, and add a regression spanning failed join plus a new attempt. The four original #817 findings are directly addressed; this is a separate resource-lifetime gap.

Checked at 2026-10-15 20:00 +0700.

## PR #844 — calendar retention

[Pull request](https://github.com/autonomous-ai/lampOS/pull/844)

Reviewer: [P2] Subtract calendar days after converting to the local day (lampOS/crates/memory/src/days.rs:319-323 at b5971fede). maintain() subtracts keep_days * 24 hours from the instant before applying the zone. A local day can be 23 or 25 hours, so this disagrees with the existing Store calendar retention contract and can delete a retained journal early.

Independent real Store/Days cases with America/New_York and keep_days=1 reproduce both directions: at 2026-11-02T04:30Z (November 1, 23:30 local), Days deletes the October 31 entry while Store correctly retains it; at March 9, 00:30 local after spring-forward, Days retains March 7 while Store expires it. The fixed-offset control agrees in both stores. Derive the current local calendar day, then subtract the configured day count for the exchange cutoff. Cover both DST transition days, including a reopen/maintenance boundary.

Checked at 2026-10-15 20:00 +0700.

## PR #844 — fallback zone reload

[Pull request](https://github.com/autonomous-ai/lampOS/pull/844)

Reviewer: [P2] Track the fallback timezone source when deciding to reload (lampOS/crates/local-time/src/lib.rs:115-117,183-185 at b5971fede). look() returns None whenever localtime or its target is missing, and current() compares only that value. If the reader successfully started from the timezone file, changing that file leaves seen=None unchanged and the old zone is used indefinitely. An independent public-library case starts with missing localtime and America/New_York, then atomically replaces timezone with Asia/Ho_Chi_Minh: the offset stays -240 instead of +420. Replacing a broken localtime symlink with another broken target, along with the new fallback name, has the same result.

Include the fallback file's identity/content metadata in the reload key, and retain the symlink identity independently of whether its target exists. Test changing the fallback while localtime remains missing or broken; the current startup-only fallback test cannot detect this. Keep the existing last-good-zone behavior for an unreadable change.

Checked at 2026-10-15 19:59 +0700.

## PR #843 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/843)

Reviewer: First pass at a25450bd7: I read #823, the full diff and the backend's EOF cleanup. The Rust change is confined to the stay-open test helper; all 13 test bodies and scheduled evidence remain unchanged, and the documentation explicitly records removal of the independent 30-second helper lifetime. Fresh Voice-eval fmt, strict all-target Clippy and full serial tests are running. I am independently checking silent input consumption, stdout staying open while owned, and exit/reaping on owner EOF. Exact-head ARM64 is green, but its PR profile skips these 13 cases, so the focused host execution remains necessary. #823 stays open for the remaining clock migration and full-group target.

Checked at 2026-10-15 19:56 +0700.

## PR #797 — 10585 fixes verified

[Pull request](https://github.com/autonomous-ai/lampOS/pull/797)

Reviewer: Both findings are fixed at 10585b465. The original real calibrate-touch --replay cases now give level 27 / touch 12 / release 7 for [33,16,27,30], and the held-pad sequence gives 100 one-pad, 100 two-pad and 100 three-pad readings. Three additional independent touch/release debounce settings, including interrupted release streaks, agree with the CLI report. The earlier hidden-baseline and strict threshold bounds are preserved in source, and controls.md now explains the replay's baseline/sampling limits.

Fresh fmt and Controls strict all-target Clippy pass. Full serial tests: 382 passed, 4 failed, 0 ignored. The four unchanged touch_program cases fail on missing stderr/output (lines 153,262,317,335); each passes its one exact retry in the original built binary. Original failed output and fixture files are retained: this is 386 distinct passing cases after retries, not a clean full run. Exact-head pull_request ARM64 run 38736016630 is green.

No new calibration code finding remains. First's updated #711 direction explicitly keeps this a manual calibration tool and defers the slider; #711 remains open for head-touch work. Please refresh the PR description's weakest-swipe explanation and old head/test counts to describe the current own-swipe cutoff and replay behavior.

Checked at 2026-10-15 19:56 +0700.

## PR #822 — b8 gate complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/822)

Reviewer: Fresh b8e3534e4 host gate is complete after cleaning all 35 workspace packages: fmt and strict all-target Clippy pass; full serial tests report 1,546 passed, 1 failed, 13 ignored (Msgs 84/0/0; Memory 156/0/5; Voice 1,306/1/8). The one failure is rules::tests::the_fact_is_kept_on_the_bus_and_a_stalled_bus_never_holds_up_a_session at lampOS/crates/voice/src/rules.rs:640: no fact arrived within its five-second wait; the separate 300 ms assertion passed. A single exact same-binary retry passed in 0.10 s. I retain the original failure and do not call the full run green or assume this new rules test belongs to Queue #739's documented process-test exception.

The eight independent rule/provenance cases pass, including committed-but-lost replies and overlapping ordinary writes. The malformed positive-acknowledgement P2 above remains open. Exact-head ARM64 run 38734277309 is still pending at the latest read; no CI rerun was requested.

Checked at 2026-10-15 19:48 +0700.

Update at 2026-10-15 19:52 +0700: the author has pushed 7a46db806 to address the acknowledgement validation. The evidence above is explicitly for b8; independent verification of the new head is next after the older active gates.

## PR #791 — integration 1ba9 gate complete

[Pull request](https://github.com/autonomous-ai/lampOS/pull/791)

Reviewer: Rebase 1ba9fd74e preserves the capture/reference fixes; I found no new source defect. Fresh fmt and Audio/Voice strict all-target Clippy pass. Full serial tests: 1,468 passed, 1 inherited reminder failure, 10 ignored. The exact same-binary retry of reminders::consumer::tests::completed_playout_survives_both_real_store_reopens_then_acks_without_respeech passes; the original failed full run is retained. All stall/reopen/XRun and clock cases passed on the first run.

Five independent current-source recovery/order proofs also pass. The 500 ms delivery-hold proof records a 410 ms source gap and 40 stale frames discarded; accepting the first late in-order packet still rejects the next. Capture/reference production sources match the previously accepted head, and the rebase keeps privacy/cancellation ahead of the Second worker service.

Exact-head ARM64 38734439204 attempt 1 failed only the inherited Memory killed-process journal-count assertion at memory/tests/binary.rs:309 (1,803 passed before abort); #839 addresses that test boundary but is absent from this branch's base. Attempt 2 is running, started by another actor; I did not rerun CI.

Reviewer: for the first builder: default capture recovery remains your merge under the user's instruction. Your earlier lamp result is recorded, but #784 remains open for the overnight real SD/MMC evidence.

Checked at 2026-10-15 19:52 +0700.

## PR #842 — moving head

[Pull request](https://github.com/autonomous-ai/lampOS/pull/842)

Reviewer: [P2] Check physical stability through capture (lampOS/crates/eyes/src/full_look.rs:60-66,90-106 at ff500ba1d). Small command-position error means the servo tracks its current step, not that it has finished moving: body's bus_out publisher reports the last written goals and measured velocity, while body's ARRIVED rule also requires low velocity. Using this head's unmodified publisher arithmetic, a head following 50 counts/s with one count of lag is labelled still throughout; Wanted settles at 320 ms and accepts a frame at 400 ms while the head is moving. Separately, once settled is latched, body() ignores a new movement before the next frame (settled at 300 ms, moving again at 350 ms, captured at 400 ms).

Use velocity/position and command stability, or the body's settled signal, and invalidate stability when motion resumes before capture while keeping the documented timeout. Both independent regressions fail; a truly stationary control passes. The relevant Eyes source is byte-identical in the reproduced f4143ff35 and current ff500ba1d. These are host proofs, with no camera or lamp access.

Checked at 2026-10-15 19:48 +0700.

## PR #842 — stale still

[Pull request](https://github.com/autonomous-ai/lampOS/pull/842)

Reviewer: [P2] Bind the still consumed by the worker to this look (lampOS/crates/voice/src/provider_worker.rs:2716-2722; sight.rs:166-167; coordinator.rs:4477-4498 at ff500ba1d). The coordinator times out after 2.6 s even with still=false, but the worker independently accepts any file up to 3 s old. A previous request's still captured 100 ms before the new ask is therefore accepted at the new request's timeout (age 2,700 ms), even though no still was produced for it. The eyes retain the previous file until its three-second expiry when the new capture fails.

An independent fresh-library replay using Kept, Look and fresh_within reproduces still=false, due=true, worker accepts=true. Carry the request/ask identity and capture result to the worker and require a matching still; otherwise use NO_LOOK_NOTE. Include a failed second capture with an earlier still present, not only an absent-file case. The relevant source is unchanged between f4143ff35 (the reproduced head) and the current ff500ba1d.

Checked at 2026-10-15 19:48 +0700.

## PR #845 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/845)

Reviewer: First pass at ed7b24992: I read the four fixes against the findings on #817 and #806. The changes target the relevant startup ordering, actual TLS handshake, own-page port ownership, and personal task-history reset. I will verify their behavior and Setup gates after the older active review, including the DNS/TLS deadline and startup failure paths. The embedded certificate is identified as a test fixture; it is not being treated as evidence of a deployed credential. #806's wiped-lamp-to-conversation acceptance remains unshown and is not complete merely because these fixes merge.

Reviewer: for the first builder: this changes setup behavior on hardware, while the builder reports no lamp, phone or captive-network run. I will report the independent host evidence here.

Checked at 2026-10-15 19:48 +0700.

## PR #844 — first review

[Pull request](https://github.com/autonomous-ai/lampOS/pull/844)

Reviewer: First pass at b5971fede: I read the proposed timezone reader, the four-crate scope and #820's agreement to combine the changes. The fresh source review and Local-time/Expression/Memory/Policy gates are starting. I am checking DST boundaries, runtime zone replacement, unchanged fixed-offset/default behavior, and invalid or unreadable zone handling. The Memory and Policy owners' reads of the committed changes remain part of the scope agreement; prior agreement on flag names alone does not establish those reads. No exact-head ARM64 result is being inferred from the older base. Actual lamp clock/setup acceptance remains unverified as stated in the PR.

Checked at 2026-10-15 19:48 +0700.

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
