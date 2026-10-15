# Memory health fixture migration: local evidence

Recorded 2026-10-16 03:22:10 +0700. The paired runs execute the same seven original compiled `lamp-memory::health` tests serially, with no ignored/filtered cases and `--retries 0`: **39.498 → 14.547 seconds**. Both normal and fixture executable SHA-256 values match across the pair; only the original health test harness is migrated. Five cases use five controlled children, and the two default-off/open-failure cases retain four ordinary children.

All **33 original scenario assertions** remain byte-identical; all 42 original assertion macros reconstruct after only two diagnostic `log` → `raw_log` call renames. One added helper assertion prevents duplicate child ownership. The original ready files, requests, real Diagnostic subscriber, marker contents, final sustained window, actual exit/reap and post-exit log drain remain. `source-preservation.json` and the independent review retain exact changes and limits.

`comparison.json` records the commands, shell-date boundaries and actual helper hashes. `timings.tsv` retains every identity and duration. The controlled five sum **33.437 → 5.590 seconds**; retained host cases sum **6.057 → 8.893 seconds**. Subset sums use rounded per-case PASS rows, not separate invocations. Compilation is excluded from all these timings.

Complete Bus and Memory tests passed **296 / zero fail / six existing ignores**, including 12 new child and 23 parent-control cases. Fmt, strict Memory all-target Clippy, and workspace build also passed. Raw gates and outputs are retained under `root-*`.

This is a shared x86_64 Mac debug measurement. Independent Serve/Reporter gates overlap the pair with unequal load. It establishes neither quiet-host or ARM64 latency, a result on a lamp, nor the combined original-123 serial target. The pure Reporter scheduling dependency remains a separate PR; the child adapter and original health migration still need clean integration and native qualification before readiness.

The initial preflight of the new child helper had one failure: its test used a consumable thread wake token as the final park witness. That new test now uses a separate predicate/condition-variable handshake and retains all its original new assertions. The original failure and concrete test-source fix are retained in the child evidence; there was no retry of unchanged failing source. Original health assertions were untouched by that fix.

The scratch collector only parsed retained raw logs, matched identities/helper hashes and wrote derived summaries. It lives outside the Rust-only repository; no binary or device evidence is committed.
