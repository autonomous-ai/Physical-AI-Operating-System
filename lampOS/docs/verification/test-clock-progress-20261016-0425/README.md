# Test-clock progress and retained failures

Recorded 2026-10-16 04:27 +0700 from `date`. This folder keeps unedited raw output and source/design proofs. It establishes neither a combined serial original-123 time nor lamp behavior. No workflow/profile/runner/device changes or flaky retries were made.

## Serve native result and Reporter dependency

Serve #909 head `e057db620acb2bd61bac1ae7f157d87cdf635a18` passes normal ARM64 [38806386829](https://github.com/autonomous-ai/lampOS/actions/runs/38806386829): **4,939 unique passes**, with doc tests, own-harness tests and artifact stages green. The raw summary says **130 total skipped, including 93 by the PR default filter**. Earlier work-log entries sometimes called the whole skipped total “profile exclusions”; only the subset identified by the raw Starting line has that meaning. The normal profile executes none of the original Serve-session cases and cannot complete their qualification.

`serve-native/` contains complete metadata, raw log, every parsed outcome, the two-minute watcher and the membership audit. The earlier full run remains failed/incomplete: 93/123 original identities and 13/14 Serve. Its unedited failure is in [the preceding evidence](../clock-migration-integration-20261016-0350/README.md). Eighth's later #914 response identifies a production clock-read-before-receive race from a separate diagnostic stress run and proposes an owner-reviewed fix. `body-readiness-owner-response.json` preserves that response; it is the owner's evidence, not an experiment performed here. No fix has been integrated or retried by Tests.

Reporter #910 actually merged as `7ae0eb647914e286817c29bff2abc2934fb19546`; `reporter-merge.json` retains the observed PR result. The Memory health continuation is being integrated separately on that actual merge, with all 33 previously qualified Memory Rust sources unchanged. Its fresh gate is not a result in this folder.

## Gemini preparation: complete gate remains red

On base `48b3737d5e83efa00278172cb602d3525d368a43`, default and `fixture-clock` Gemini gates each complete **268 passed / zero failed / two existing ignored**. Compiled inventory keeps all 258 old identities and their ignored flags, adding exactly 12 new cases. Fmt, default/feature Gemini strict Clippy, dependent strict Clippy, and default/feature workspace builds pass. Voice/Surround/Evaluator completes **2,011 passed / one failed / eight existing ignored**, exit 101. Supplemental Memory dependent coverage completes **162 passed / zero failed / five existing ignored**, with strict Clippy. `gemini/` retains every executed command, date, exit code, complete raw log, identity proof and frozen-source result; the separate stages must not be summed as one test inventory.

The sole failing identity is `coordinator::tests::pilot_faults::a_stall_while_the_lamp_answers_cuts_the_reply_and_the_voice_goes_on`. Its Threaded host-clock run returns `StaleOwner` before the intended physical mute. This is the same scenario as #866, with a different error from its older invalid-read-interval failure. The read-only call-graph audit shows synchronous scripted IPC peers and `cat` children; no Gemini connector, InputSender or Actor is constructed on this path. The new capture clock is unreachable there. That does not identify the failing ownership check or exclude indirect host/resource effects. Ten scoped Voice/turn-owner source files match the recorded base, and all five Gemini change files match their frozen snapshots. Source-consistent lease-expiry reasoning remains a hypothesis. No assertion, lease, production capture worker or failure outcome was changed; no rerun was made.

## Separate direct Serve-relay continuation

The clean six-file continuation based on Serve head `e057db620` completes **432 passed / zero failed or ignored**, with fmt, strict Evaluator Clippy and workspace build. `serve-relay/` retains full gate output, all source hashes and the compiled comparison: every prior 422 identity plus exactly ten new cases, including all six original relay identities. It is unpublished pending the Serve dependency, and its gate is not a new benchmark. Historical serial before/after measurements remain separately identified in [the earlier pair](../serve-relay-fixture-pair-20261016/README.md).

## Remaining work

[The original-123 map](../original123-remaining-20261016/tests-823-original123-remaining-map-20261016.md) partitions the frozen identities without admitting new regression tests. Historical concurrent-shard case sums are marked as such and cannot be added to local paired timings to claim a serial target.

`plans/` retains the Records source/clock plan, Provider worker proposal and independent review. Records preparation leaves its original integration test unchanged. The Provider review corrects a false symmetric LinkClock-refusal assumption and separates pure Moment projection, worker receipts and later Actor/service/controller integration. These are design/source records, not execution results. #783 remains with eighth for its loaded policy qualification and close decision.

`copy-provenance.json` records the source path and SHA-256 of each copied raw/proof file. `SHA256SUMS` covers this folder. Measuring/auditing Python remains outside the repository, as lampOS requires.
