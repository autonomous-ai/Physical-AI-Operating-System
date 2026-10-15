# Evaluator relay clock entry point (#823)

Recorded 2026-10-15 19:58:21 +0700, from shell `date`. Base `bd37eb8087ff61148b48be889dfb640f6617831d`; Rust 1.96.0 on x86_64 macOS. This is preparation for clocked process fixtures, **not an existing-test speedup**.

`session::run` remains the system-clock wrapper used by the CLI. `run_with_clock` takes a pure `Fn() -> lamp_turn_owner::MonoTime`, compatible with a body's `Clock` as `|| clock.now()`. It reads that source for protocol receipts, the session deadline, stop request and signal timestamps, and the stop-grace comparison. Every original sampling point and its order remain, including a distinct fresh clock read after a successful SIGTERM. Callers must keep the source monotonic, use the child's timestamp domain and synchronize participants separately; clock reads never step time.

The 2 ms relay poll, 10 s grace and all bounds remain. Process-group cleanup and the late stdout drain still use host time. The CLI has no clock-selection option. No existing test or assertion changed; no profile or workflow changed. The source audit reverses the clock substitutions and confirms the original relay/stop bodies, existing session tests, cleanup and late-drain code: [audit](source-audit.json). The audit predates a diagnostic-only addition to the new integration helper; [final source hashes](gates.json) identify the bytes that passed the full gate.

## New evidence

One unit regression proves a successful stop takes two separate samples while a pure clock returns the same time. Two integration regressions run the actual relay against owned Unix transports and an inert FIFO-controlled shell:

- Repeated reads do not advance time; explicit advancement controls stdout, cue, Pong, child-exit and SessionEnd receipt timestamps. The shell actually exits and its group is reaped.
- The hard deadline remains inactive one microsecond below its original boundary, sends SIGTERM exactly at the boundary, and preserves the full ten-second grace. A child that acknowledges but ignores SIGTERM remains alive just below the grace boundary and is force-killed/reaped exactly at it.

The driver waits for explicit output and complete subsequent relay checks; Pong alone is not treated as proof that a deadline check finished. It uses no sleeps to synchronize these cases. Failed assertions wake both endpoints, request child exit and use a ten-second host watchdog around cleanup. This tests relay boundaries, not real voice runtime shutdown or device latency.

## Validation

Final commands ran from `lampOS/` with `CARGO_BUILD_JOBS=2`, once per gate, with no flaky retry layer:

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --locked --offline -p lamp-voice-eval --all-targets -- -D warnings` | Passed |
| `cargo test --locked --offline -p lamp-voice-eval --no-fail-fast -- --test-threads=1` | **287 passed**, zero failed or ignored; all existing six `serve_relay` and 14 `serve_session` tests included |
| `cargo build --locked --offline --workspace` | Passed |

Before the full gate, the two new integration tests passed in 0.07 s on the final source. The initial delegated sandbox attempt failed both at startup because local Unix socket creation was forbidden; [original failure](sandbox-startup-failure.log) remains recorded. The primary-context approval then allowed the same owned local fixtures. This environment correction is distinct from retrying a flaky assertion.

[Commands, dates and exit codes](gates.json); raw [fmt](fmt.log), [clippy](clippy.log), [whole package](tests.log), [workspace build](build.log), [focused clock tests](focused-clock.log). Native ARM64 qualification is recorded on the PR for its current head. No lamp, hardware, cloud service or repository setting was used.
