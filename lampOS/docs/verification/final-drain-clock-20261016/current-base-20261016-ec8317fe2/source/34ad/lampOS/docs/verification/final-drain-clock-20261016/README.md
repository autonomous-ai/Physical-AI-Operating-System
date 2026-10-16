# Final-dispatch delayed-line clock: first local qualification

Packaged 2026-10-16 09:18:14 +0700, from shell date. Base `1fb07c6f57ee280a49de7b4ddd74008c2a32a20d`, branch `tests/823-final-drain-clock`. The first executable candidate gate passed **504 tests, zero failures, zero ignores**, plus workspace formatting, strict all-target Evaluator Clippy and the workspace build. Native ARM64 qualification remains pending.

The original `lamp-voice-eval::final_dispatch / a_session_end_first_seen_in_the_final_drain_withholds_and_is_kept` measured **1.272 s before → 0.444 s after**, one first sample per version using the same nextest command, default profile, one test thread and zero retries. This preserves the whole 300 ms logical delay, 800 ms dispatch target and every original assertion. Times are nextest's complete case durations, including fixture creation, real shell/pipe/reader and cleanup, excluding compilation and nextest startup. Shared host load was uncontrolled. This is no full-suite before/after, native, device, CI push-to-result or original-123 standalone serial result.

## What changed

Only the selected original's fixture initializer changes. The ordinary helper still uses its existing real sleep. Its existing configuration literal is extracted without changing values or construction order. A new integration-test-local module uses the already existing explicit `PhysicalBackend::new_with_clock` / `FixtureDriver` seam; no product implementation or default is edited.

The first real `cat` must finish and the backend's actual reader must enqueue its complete line before time leaves the initial epoch. At the original 300 ms boundary a nonblocking private FIFO releases the unchanged later bytes through the actual shell stdout pipe. The second `cat` marker and actual cumulative enqueue count two are both required before further clock advance. The driver consumes no backend line: the original final drain at 800 ms still discovers the terminal line, withholds playback, and retains the RunEnd for the runner.

Clock reads are pure. One immutable ten-second host deadline is checked before and after marker reads and before logical advances. It does not interrupt filesystem operations, spawn or OS cleanup. Driver Drop attempts an abort token and closes its FIFO; the existing backend still owns stdin close, real exit and its unchanged two-second kill/wait fallback.

## Inventory and executable evidence

The clean compiled inventory contains **498 identities with zero ignores**. The candidate retains every identity and ignore flag, adding exactly six reviewed helper tests for a total of **504**. All original 13 final-dispatch cases remain; 12 functions are byte-identical, and reversing only the selected initializer restores the thirteenth. The full original test file, including the extracted helper configuration, restores byte-exactly through the retained inverse. Root independently matches both baseline source/doc snapshots to immutable 1fb.

The full serial Cargo gate executed all 504 identities exactly once across 24 binary paths, including one zero-test main binary. The selected original also appears once in its separate timing stage. Its two executions belong to distinct documented stages, not a failure retry. The six additions cover the exact release boundary through a real backend, held-shell actual exit/reap, released-shell actual stdin EOF/reap, expired-watchdog refusal, marker/enqueue conjunction and malformed/excess-frame rejection. The two predicate cases use explicit counters; they are not claimed as real reader observations.

[Identity and execution proof](audit/identity-execution-proof.json), [case timing](audit/original-case-timing.json), [per-binary timing table](audit/per-binary-timing.md), [source review](review/review.md) and [terminal audit](audit/terminal-source-gate-audit.json) retain the exact evidence. The libtest binary durations sum to **167.54 s**; this is not the Cargo command's wall time or a baseline comparison. The full test stage ran 09:12:22–09:15:12 and workspace build finished 09:16:38 on 2026-10-16 (+0700), as retained from date. All 1,023 enumerated source inputs remained exact throughout the gate and the independent audit.

Commands ran from `lampOS/` using Rust 1.96.0, locked/offline dependencies, two build jobs and incremental disabled:

```sh
cargo fmt --all -- --check
cargo clippy --locked --offline -p lamp-voice-eval --all-targets -- -D warnings
cargo nextest list --locked --offline -p lamp-voice-eval --message-format json
cargo test --locked --offline -p lamp-voice-eval --no-fail-fast -- --test-threads=1
cargo build --locked --offline --workspace
```

The same exact original-case nextest selector appears in both raw gate records. Cargo metadata finds no workspace reverse dependent on lamp-voice-eval. No test was ignored, retried or weakened.

## Review history and limits

Before any changed-source test ran, review corrected a late-positive watchdog acceptance gap and added the two predicate negatives. Both source-review versions and the exact correction/inverse remain retained. The first executable candidate passed without a test failure. The dated review's pending-gate statements remain unchanged; this later qualification supplies the execution result.

No dedicated new regression executes abandonment before the initial marker. That path still relies on the unchanged backend cleanup owner and kill/wait fallback; the held-shell test starts after the marker. Actual transport closure for the original `Then::Close` case remains separate work, and only one original of the 13-case family migrates here. The original-123 serial target remains open.

All raw logs and source copies are byte-exact. `provenance.json` maps copied files; `SHA256SUMS` covers every package file except itself. External Python orchestration is represented only by checksums and command descriptions. No Python or compiled artifact is committed, and no lamp/device was used.
