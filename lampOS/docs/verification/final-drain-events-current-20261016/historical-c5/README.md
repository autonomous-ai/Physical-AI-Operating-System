# Three final-drain event fixtures: first local qualification

Packaged 2026-10-16 09:49:29 +0700, from shell date. Base and prerequisite: `c5bcc45b905458c1a8adcd8bcf06885f35dbfdf7` (#963). This continuation changes exactly three original test initializers plus documentation, using the prerequisite's unchanged explicit clock helper. Publication is held until that prerequisite merges. Native ARM64 qualification is pending.

The first local gate passed **504 / 0 / 0**, formatting, strict all-target Evaluator Clippy and workspace build. Before and after compiled lists are identical: no added, removed or newly ignored identity. The full gate executed every compiled identity exactly once across 24 binary paths (one contains no tests). All 1,031 enumerated Rust/TOML/lock/document inputs remained exact. Cargo metadata finds no workspace reverse dependent on Evaluator.

| Original final_dispatch case | Before (s) | After (s) |
| --- | ---: | ---: |
| a_relay_framing_error_first_seen_in_the_final_drain_withholds | 1.263 | 0.442 |
| an_invalid_cue_first_seen_in_the_final_drain_withholds | 1.226 | 0.442 |
| an_unobserved_event_in_the_final_drain_requires_revalidation | 1.242 | 0.437 |

The same exact nextest selection ran with default profile, one test thread and zero retries. Its reported three-case serial summary was **3.733 → 1.322 s**. These are one first sample per version on the same shared Mac with uncontrolled load. Per-case duration includes setup, actual shell/pipe/reader, the original dispatch assertions and cleanup, excluding compilation and nextest startup. These local observations are not native, device, complete-suite before/after, CI latency or the fixed-123 standalone serial result.

The unchanged helper requires actual cat completion and the reader's actual enqueue count before logical advancement; queued lines remain untouched for the original final drain. The original 300 ms release, 800 ms dispatch, authored bytes and every assertion remain. A complete malformed line is queued as its actual parse error; the unchanged backend decides protocol failure. ListeningReady still forces revalidation, is consumed by the original next_event, and the original second same-time dispatch starts the recording-only player exactly once. No extra release or synthetic event is introduced.

[Reviewed source and causal boundaries](review/README.md), [source inverse](review/source-proof.json), [compiled/executed identity proof](audit/identity-execution-proof.json), [per-binary times](audit/per-binary-timing.md), and [root terminal/timing audit](audit/terminal-source-and-timing-audit.json) retain the evidence. Reversing only the three initializers restores the complete test file to c5. Removing the documentation append restores its original bytes; the existing helper is byte-exact. The three original functions also match the initial 8dd inventory before substitution. No original assertion or bound changed.

Commands from lampOS, Rust 1.96.0, two build jobs, incremental disabled:

```sh
cargo fmt --all -- --check
cargo clippy --locked --offline -p lamp-voice-eval --all-targets -- -D warnings
cargo nextest list --locked --offline -p lamp-voice-eval --message-format json
cargo test --locked --offline -p lamp-voice-eval --no-fail-fast -- --test-threads=1
cargo build --locked --offline --workspace
```

The same exact three-case nextest commands are retained in raw/baseline/gates.json and raw/first-gate/gates.json. Full tests ran 09:42:33–09:45:50 and build finished 09:47:18 (+0700), from date. The sum of libtest binary durations is **193.67 s**; no complete baseline suite was run for comparison, and this sum is not Cargo wall time. The three selected cases each also ran in the separate timing stage. These are distinct planned stages, not failure retries; no test failed in this gate.

The inherited ten-second watchdog does not interrupt blocking filesystem/OS calls. Backend Drop remains the owner of actual child exit, kill and reap, including the original two-second fallback. The closed-transport case remains on its original helper; no new pre-initial-marker abandonment regression is claimed. The original 123 under-120-second standalone serial target remains unproved.

Raw files and source snapshots are exact copies. provenance.json maps them and SHA256SUMS covers every package file except itself. External Python scripts are represented by hashes only, with no compiled artifact or Python committed. No workflow, runner, settings, lamp or device change.
