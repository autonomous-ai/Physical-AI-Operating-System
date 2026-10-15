# Serve relay original-case pair and complete gate

Recorded 2026-10-16 02:57 +0700, from `date`. The same helper executable ran all **six original Serve-relay identities** serially with no retries: **31.474 seconds before, 2.204 seconds after**. All six original test bodies and 37 assertions remain byte-identical. Six new helper tests are excluded only from that old-case pair; they run in their own six-pass gate and in the full gate. Four new controlled-invocation tests are also present.

The complete Evaluator gate passes **467 tests / zero failed / zero ignored** across 25 summaries: 259 library plus 208 integration. Formatting, strict all-target Evaluator Clippy and workspace build pass. All raw command/date/exit records, compiled before/after lists, helper hashes and final source hashes are retained.

The private invocation parser uses the actual argv suffix, actual stdout records and the real Session child/reader threads. The original three-second Stop/close limit, six-second scenario limit and full ten-second logical ignored-SIGTERM grace remain. A runtime acknowledgment is not process exit: actual status, signal, process-group cleanup and reaping remain authoritative.

The ten-second post-signal host budget is checked before and after polls. Existing reader enqueue can still delay a failing poll up to thirty seconds; this is not a strict ten-second abort guarantee. `root-watchdog-clarification.json` records the documentation correction without a code change.

This is a local debug x86_64 macOS pair under shared load, not native ARM64 or the combined 123-test target. The unpublished tree contains the reviewed Serve/Recorder dependencies and awaits per-change integration. No old assertion, retry or workflow change.
