# Explicit microphone selection, 12 October 2026

The capture worker can use `--microphone mic1|mic2`; omission preserves mic2
and its existing child arguments. The selected named PCM is recorded in the
run metadata and forwarded by the evaluator's typed runtime options. This
changes only source selection, with no mixer write or new processing stage.

The source-only export is based on `813ebf61f` plus the seven-file selection
change; its 482-file manifest is SHA256
`114063ae5789fa0d5760f81515f45beb548162dda7be87ab972dc8b456743abb`.
[Results](results.json) retain the exact checks and artifact identities.

- Host: format and strict live/evaluator all-target Clippy passed. Live options
  passed 19 tests. The initial relay selector matched zero tests, so its gate
  failed rather than accepting an empty test run. The corrected
  `runtime_flag_tests` selector passed 4 tests without a source change.
- Emulated Linux ARM64: format, strict live/evaluator all-target Clippy,
  21 live options tests, 4 relay option tests and both release binaries passed.
  Workspace crate build outputs were removed before compilation from the
  read-only export; third-party dependencies were reused. The generated host
  `target/` directory is excluded from the source manifest.
- The new `lamp-live` binary was copied to its own hash-named release directory
  on Lamp-4ace and verified there. Installation changed no service and started
  no runtime. The existing default remains unchanged.

Native startup, microphone quality and latency for the selected alternate path
are pending. No new acoustic score, latency improvement or preferred microphone
is established by these checks. Full workspace tests were not repeated for this
source-selection change. See [microphone topology](../../microphone-topology.md)
for the comparison boundary and unchanged privacy/AEC requirements.
