# Final-drain clock: fixed-upstream local integration

Captured 2026-10-16 11:15:23 +0700. PR #963 candidate `db7201d0bc4c1d368b14ac203f04a3a99e3d845f` is one change on
`f5a20ba28f3227ab06979b6e662883aede96b046`. Its first fresh local integration gate completed at
2026-10-16 11:07:13 +0700: **521 passed, 0 failed, 0 ignored**.
This supplement verifies local qualification; native qualification of this head
remains separate.

The clean rebase preserves all 56 owned files exactly from `c2eacfa4e29a5c9db21d4d919a77d33ce8112238`:
three source/documentation files and all 53 historical evidence files, including
52 valid inner checksums. The source/documentation diff is identical to the
previous change. All 13 original `final_dispatch` test bodies restore exactly
after reversing the one selected backend initializer; all their assertion bytes
are preserved. The new helper retains the same six compiled identities.

The current base is a real upstream change. From the previous `ec8317fe` base,
31 commits changed 143 paths, including 32 Voice paths. The final base commit
adds the two missing `say: None` fields and the offline fixture's additional
`false` argument (the parent task identifies this as #976). Evaluator directly
depends on that changed Voice source. Cargo.lock retains its 470 package keys,
versions and sources, while `lamp-setup` gains the `rustls` dependency edge.
There is no claim that the dependency context is identical to the earlier gate.
See `independent/dependency-delta.json`, `independent/buildfix-f5.diff` and
`independent/metadata-audit.json`.

The root gate used Rust 1.96.0, two Cargo jobs, incremental compilation disabled,
and the declared environment in `gate-environment.json`. All stages returned 0,
in this order:

1. Clean f5 baseline: `cargo nextest list --locked --offline -p lamp-voice-eval --message-format json` — 515 identities, 0 ignored.
2. `cargo fmt --all -- --check`.
3. `cargo metadata --locked --offline --no-deps --format-version 1` — no declared workspace reverse dependents of Evaluator.
4. `cargo clippy --locked --offline -p lamp-voice-eval --all-targets -- -D warnings`.
5. Candidate: `cargo nextest list --locked --offline -p lamp-voice-eval --message-format json` — 521 identities, precisely the six known additions, no removals or ignore changes.
6. `cargo test --locked --offline -p lamp-voice-eval --no-fail-fast -- --test-threads=1` — all 521 compiled identities executed exactly once across all 24 binary paths; 0 failed, ignored, measured or filtered. The separate doc-test result has 0 tests.
7. `cargo build --locked --offline --workspace`.

`raw/gate/gates.json` preserves the actual commands, working directories,
start/finish dates, exits and raw log hashes. `independent/result-audit.json`
independently maps every result row to its compiled identity and binary path,
including the two interleaved diagnostic/status rows. Both the selected original
case and all six helper cases passed once. All 1,094 frozen candidate inputs and
56 owned files still match; the static audit also matched all 1,085 frozen base
inputs to the exact Git commit.

The four `prior-c2-reference` files support only the documented earlier source
and identity comparison. The existing 53-file historical package remains at
`lampOS/docs/verification/final-drain-clock-20261016` in the source commit;
`historical-files.json` pins every byte. Its original local measurements and
older native evidence are separate from this new f5/db gate. No new timing pair,
standalone original-123 bound, or device result is claimed here. No test was
retried or executed by this audit, and no source or Git state was changed.

Every copied input is listed in `provenance.json`. `SHA256SUMS` covers every
other file in this supplement. Audit and gate Python scripts remain external;
only their hashes are recorded.
