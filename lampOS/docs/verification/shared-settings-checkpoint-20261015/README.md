# Shared logging and settings checkpoint — 2026-10-15

Ready for lamp-voice: no

Unfinished task #699, paused for new Third MVP task #755 and proposal #754.
This is preservation evidence, not release qualification. No lamp, provider,
account, key or deployment was used. No existing runtime adopts the new APIs.

Third used an Intel i7-10700K x86_64 development Mac, macOS 26.6.2 /
Darwin 25.6.0, Rust/Cargo 1.96.0. No tests were executed in this checkpoint.

- First `cargo test --offline -p lamp-settings --no-run`: FAILED (101),
  Diagnostic `Vec<KeyValue>` mismatch and unavailable `rustix::fs::mkfifoat`
  on this Mac. Both original diagnostics are retained without modification.
- After correcting those and completing the current edits,
  `cargo test --offline -p lamp-settings -p lamp-bus -p lamp-msgs --no-run`:
  passed (0), 14.578518622 s. This compiled test executables only.
- Direct rustfmt was used on changed Rust files. This is not the complete
  `cargo fmt --all --check` gate.

The source hash manifest records the checkpoint's Rust/manifests/lock/toolchain.
The source has no measured log-admission or settings round-trip latency. Strict
Clippy, executed tests, dependents, workspace build, disabled parity, standalone
source copy and native ARM64 qualification are not completed. Any CI caused
by the preservation push is an unfinished-branch run, not a ready handoff.

Resume from docs/logging.md and docs/settings.md. Client reply validation,
canonical path spelling, log retention, full shutdown/backpressure coverage,
latency measurement and a concrete lamp runbook remain unfinished. Known
limits and the original failed compile must remain in the final evidence.
