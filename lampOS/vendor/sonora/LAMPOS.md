# Sonora dependency snapshot

Sonora 0.2.0 from crates.io (the pure-Rust port of WebRTC's audio
processing the voice's echo canceller runs on), under its BSD-3-Clause
license. `PROVENANCE.json` records the upstream crate checksum and the
SHA-256 of the two changed files before and after; the other 34 files are
the upstream ones, byte for byte. Its lockfile is left out.

The change (#828): sonora builds its echo canceller, AEC3, with
`EchoCanceller3Config::default()` and gives its caller no way to choose
another. The lamp needs to try AEC3's suppressor settings (its near-end
detector and tuning), because on lamp-4ace a short "stop" said over the
lamp's own voice is mostly taken out with the echo (`docs/stop-word.md`,
`docs/duplex-listening.md`). So:

- `src/config.rs`: `Config` has one more field, `echo_canceller3:
  Option<EchoCanceller3Config>`, and `EchoCanceller3Config` is re-exported.
- `src/audio_processing_impl.rs`: the echo canceller starts with that
  configuration when it is given, and with the default when it is `None`.
  `EchoCanceller::transparent_mode` is applied on top, as before.

With `echo_canceller3: None`, the default, nothing else changes: the same
configuration is built as in sonora 0.2.0. The package is excluded from
workspace membership and replaces the crates.io one through
`[patch.crates-io]` in `lampOS/Cargo.toml`.

Replace this snapshot when an upstream release lets the caller give the
AEC3 configuration; then the patch can go.

Not shown: sonora's own test suite is not run here.
