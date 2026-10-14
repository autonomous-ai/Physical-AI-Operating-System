# Standalone 7f host verification

The isolated source-copy check passed on Darwin 25.6.0, x86_64, Rust 1.96.0
(`ac68faa20`, 2026-05-25): **280 test executions passed, zero failed, two explicit timing
probes ignored**, and **72 default-off cases / 144 stdout comparisons matched
exactly**. Every stage passed on its first attempt. Filtered suites may overlap, so this
is not a unique-test count. This subset does not qualify
the full host gate, which remains red because of the separately retained
inherited motor-test failures in `motor-failure-audit.md`.

The checked copy is `/private/tmp/lamp-third-schedule-voice-7f-standalone-ttewwzqn/lampOS`. It combines base
`7fbb03e1f2e61ad6dfaf184cd768bad6524c2d76` with 46 frozen task overlays: 1,454
source files, plus two scratch helpers. All 675 frozen Rust/manifest paths
and all copied source/helper bytes remained unchanged. All 34 local package
manifests resolved inside that copy. The active verification directory was
omitted from the copy; its 88 omitted paths are listed in
`standalone-final.json`. No linked target tree was used.

All commands below ran from the standalone workspace above with
`CARGO_TARGET_DIR` set to its own `target` directory. Cargo was offline and
locked. Tests ran serially with local Unix sockets permitted. The runner's
ordinary environment allowlist is documented in `default-off-probes.md`.

| Command | Result |
| --- | --- |
| `cargo build --offline --locked --workspace` | Passed |
| `cargo build --offline --locked -p lamp-actions -p lamp-voice --example schedule_off_actions --example schedule_off_voice` | Passed |
| `cargo test --offline --locked -p lamp-actions --all-targets -- --test-threads=1` | 33 passed |
| `cargo test --offline --locked -p lamp-schedule --all-targets -- --test-threads=1` | 160 passed |
| `cargo test --offline --locked -p lamp-voice --lib reminder -- --test-threads=1` | 68 passed, two ignored |
| `cargo test --offline --locked -p lamp-voice --lib tools -- --test-threads=1` | 19 passed |

The ignored tests are `reminders::tests::parent_poll_cost` and
`reminders::transport::tests::poll_cost`; both are explicit standalone host
timing probes. No benchmark was run during this check. Offline locked Cargo
metadata also passed and is retained with the stage logs.

The metadata comparison used the exact 7f baseline report and unchanged
helper bytes, with identical baseline fixture paths and per-case environments.
Actions catalog, Voice argument/tool/route metadata and ordinary Written
serialization matched without normalization. All helper invocations exited
zero, stderr was empty, and invalid relative Schedule paths were not created.
This does not demonstrate live audio, provider behavior, a real device,
end-to-end latency, or Linux scheduler readback on this Darwin host.

`standalone-final.json` is an exact copy of the original report. The exact
baseline report is `default-off-baseline.json`; their SHA-256 values are
`5accebf0f8081168cce97f9aba081e2a11217a2a1a30fc796b39fd7621c3dd1f` and
`6304d8fb2486646e7bd4320a048b01a4ad05a5220f8827f85e74a613c45e8926` respectively. `default-off-comparison.json` extracts the
72 comparison records with identity and result checks. The 144 final stdout
readings are mapped by `default-off-final-files.json`; they reuse the same
three retained payloads as the baseline. Each empty stderr has an explicit
original-path entry pointing to the retained zero-byte artifact.

`standalone-evidence-index.json` maps all retained originals and every absolute
path referenced in the final report. Archives, target directories, binaries
and scripts are not copied. Previous prepared reports, d5/3773 evidence and the
current 7f baseline remain intact. Retention changed only evidence and documentation; no runtime source or Git
changes were made and no Cargo command ran during retention.
