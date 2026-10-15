# Explicit Memory Reporter schedule: preparation

Recorded 2026-10-16 03:24:25 +0700. This isolated preparation adds `Reporter::start_with_schedule` after the Bus cadence dependency (#901) merged at `c5f8d73372cf5addf14dcf7990425265beb7bd90`. Ordinary startup still selects host Instant and thread::park_timeout. Original cycle sample positions, the full one-second period, final best-effort report and 100 ms bounded shutdown remain. The existing application factory (#897) is already in the base.

Eight new cases cover pure reads/default host time, original sample positions, frozen/spurious/predeadline wakes and exact due time, actual publish completion and overrun, frozen-stop wake/final report, held-publish shutdown, actual failure/recovery, and real Bus socket rescans under shared cadence. No original scenario is migrated in this preparation and no speedup is claimed.

## Complete local qualification

From lampOS/, two build jobs, incremental compilation disabled, dedicated target directory:

- `cargo fmt --all --check`
- `cargo clippy --locked --offline -p lamp-bus -p lamp-memory --all-targets -- -D warnings`
- `cargo test --locked --offline -p lamp-bus -p lamp-memory --no-fail-fast -- --test-threads=1`
- `cargo build --locked --offline --workspace`

All passed. Complete Bus/Memory tests: **261 pass, zero fail, six existing ignores** across 22 summaries. All eight new cases pass once. Raw logs, exact commands and shell-date boundaries are in integration-*. integration-result.json records counted results and source hashes. Tests use real local Unix sockets and owned threads, without retries. This is shared x86_64 Mac qualification, not ARM64 or device evidence.

## Source preservation and limits

The [independent current-base review](current-base-review/review.md) explicitly reconstructs the entire original health.rs and memory.md against this HEAD. All nine active health unit cases plus its existing ignored timing case remain exact, with all 42 assertions. All 12 integration files and all other 25 tracked Memory Rust files match HEAD. Both owned Rust hashes also match the earlier qualified seam. The application factory and merged Bus behavior remain intact.

CycleComplete means actual publication returned and the log decision was made. WaitChecked means a positive remaining wait was checked. Neither means downstream receipt, log visibility, worker exit or process exit. State/request measurements, diagnostic values, real Event and cleanup ordering remain authoritative. The schedule contract requires pure nondecreasing reads, the existing park token and prompt observations; it cannot enforce arbitrary custom implementations or preempt filesystem/kernel calls.

source-preservation.json and the original reversal are historical design evidence; after-bus-merge.json and current-base-review/ provide current integration proof. The later child/parent protocol and seven-test health migration are separate changes.

The PR's own ARM64 run is pending at this snapshot. Its exact head, result and eight-case membership will be linked on the PR before readiness. #823 stays open for all original 123 and the combined serial target.

The scratch collector copied raw logs, checked source hashes, counted complete summaries and matched the eight new PASS rows. It did not execute tests and stays outside the Rust-only repository. No binaries are committed.
