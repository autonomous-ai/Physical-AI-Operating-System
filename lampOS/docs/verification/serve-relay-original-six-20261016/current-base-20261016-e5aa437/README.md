# Current-base Serve-relay verification

Packaged from `date`: 2026-10-16 07:43:24 +0700. Base **`e5aa43747ec1540893b4da4509d14288e7e70806`** includes the actual Serve merge `6be20efd2b189cafe5aa40d21151d7640ab9f67d` and Schedule fix `935b16307ca0f8ba0813b921e08b43c2e431a02d`. This is the six direct `serve_relay` cases, separate from the fourteen already-merged Serve-session cases.

The first fresh gate on this base passed **508 tests, zero failed, ignored or filtered out**, completed **2026-10-16 07:36:48 +0700**. The compiled comparison retains all **498** clean-base identities and ignore flags, adding exactly four invocation tests and six coordinator tests. Two independent result audits account for all 508 identities exactly once across 24 binary paths, including the two diagnostic-interleaved result rows. The original six bodies and all **37 assertions** remain byte-exact. The actual original-six membership in the `lamp-voice-eval::serve_relay` binary is:

- `a_closed_runner_transport_stops_the_runtime_through_the_same_path`
- `a_nonzero_runtime_exit_after_clean_metadata_is_a_relay_error`
- `a_runner_stop_is_one_sigterm_and_a_clean_reaped_end`
- `a_runtime_that_ignores_sigterm_is_force_killed_and_never_clean`
- `the_hard_deadline_stops_the_runtime_and_is_an_error`
- `the_pilot_runtime_settings_reach_the_serve_child_exactly`

`gate/` contains unedited command/date/exit records, logs, resolved Cargo metadata, the compiled comparison, both result audits and frozen-source hashes. Its clean baseline is the exact supplied e5aa compiled list captured at 07:12:51–07:16:44; all 870 source hashes were checked against this base, and the baseline was not recompiled. `integration/` preserves source snapshots, exact inverse, test-body proofs, clean-base ancestry and independent review. The earlier pending wording inside copied source-only proofs records their original stage; the completed gate and audits here establish the fresh local result. The initial independent parser invocation used the wrong manifest key; correcting only that parser did not rerun tests.

All five gate commands passed from `lampOS/`, using Rust 1.96.0, `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, and target `/private/tmp/tests-823-rebase-target`:

```sh
cargo fmt --all --check
cargo clippy --locked --offline -p lamp-voice-eval --all-targets -- -D warnings
cargo nextest list --locked --offline -p lamp-voice-eval --message-format json
cargo test --locked --offline -p lamp-voice-eval --no-fail-fast -- --test-threads=1
cargo build --locked --offline --workspace
```

The full resolved dependency graph has no workspace reverse dependents of Evaluator. All 953 frozen inputs remained unchanged throughout the gate. No test failed, no retry or source correction occurred, and packaging did not execute tests.

The [earlier original-six timing pair](../pair/comparison.json) is historical: base `8af53fd459795029f6277679ded49bf7826def73`, same six identities, realtime profile, one thread, zero retries. Nextest test-phase time excluding compilation was **31.480 → 2.367 seconds**. The after command adds exact-name selection because the changed binary also includes six new helpers. Both commands and source hashes remain in `../pair/`; this shared-Mac debug pair had uncontrolled, unequal load and is not a new measurement on e5aa.

The coordinator retains actual Stop writes, EOF, signals, child status and kill/reap. Its ten-second post-signal host success condition is not a strict abort bound: the existing reader barrier can take thirty seconds. The full original runtime durations and assertions remain. Native qualification is **pending**; neither this gate nor the historical pair proves the original 123 tests' combined serial bound or native/device latency. All 80 earlier evidence files and all six reviewed source/doc files are unchanged; `preservation-audit.json`, `provenance.json` and `SHA256SUMS` record this supplement's exact contents.
