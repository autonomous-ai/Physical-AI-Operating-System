# Relative reminder catalog: focused host checks

The follow-up requested by First in [PR #747](https://github.com/autonomous-ai/lampOS/pull/747#issuecomment-6179199142)
starts at integration `cf2f5b43c8113a704324c2743af3cd92bbc4fdda`. It changes
only two enabled Actions descriptions, the published-catalog regression and
their English docs. Relative phrases use the requested label and seconds in
`set_timer`; clock times use `set_reminder`. No scheduling arithmetic, schema,
transport, gate, character or deployment file is changed.

Source identity: [711 frozen Rust/manifest/toolchain paths](source-frozen.json),
manifest SHA-256 `517af82a7dde4930ccacc48934bd635dd59df983b9d410e9b8c49e2917822c1c`.
The manifest's Git base precedes the uncommitted implementation; the per-file
hashes identify the exact tested source. Every stage verified those hashes
before and after running.

On Third's Intel i7-10700K x86_64 Mac, macOS 26.6.2 / Darwin 25.6.0,
64 GiB RAM, Rust/Cargo 1.96.0, all five focused stages passed once:

| Stage | Result | Launch through exit/log closure, seconds |
| --- | --- | ---: |
| [format](format.txt) | passed | 3.927770221 |
| [clippy](clippy.txt) | passed | 43.721301555 |
| [catalog-regression](catalog-regression.txt) | 1 passed / 0 failed / 0 ignored | 12.510845972 |
| [voice-tools](voice-tools.txt) | 18 passed / 0 failed / 0 ignored | 15.757857421 |
| [workspace-build](workspace-build.txt) | passed | 4.950818759 |

[The full commands and timestamps](focused.json) include `cargo fmt --all
--check`, strict all-target Clippy for Actions, Voice, Voice Eval and Memory,
the exact new integration test, Voice's `tools::` filter and the offline locked
workspace build. [The dependency scope](affected-packages.json) includes dev
dependents; Schedule is a dependency, not an affected dependent of Actions.
[Counts](counts.json) are test executions, not a complete affected-package gate.

The initial format probe printed [one assertion-layout diff](initial-format-diff.txt).
Its scratch shell also ran metadata reporting afterward, so its final shell
exit is not used as a formatting result. The layout was corrected before the
frozen validation map and five-stage run above. [The preceding source map](source-before-format-correction.json)
and original diff remain unchanged. No test failure was hidden by that probe.

The catalog regression reads the actual `tools.json` published by an enabled
Actions server. It checks the model guidance and examples, without contacting
Schedule, a provider or a lamp. Existing Voice tool tests include disabled
filtering of an enabled Actions catalog and actual Actions/Schedule routes.
[Retention hashes](retention.json) preserve the original records.

The complete serial host gate, independent source-copy/disabled-byte proof
and native ARM64 qualification are retained separately in
[Third's qualification record](https://github.com/autonomous-ai/lampOS/blob/lamp-third/lampOS/docs/verification/relative-reminders-qualification-20261015/README.md).
This folder records the focused checkpoint and does not claim those later
stages passed. First's [lamp replay](../../schedule.md#relative-time-follow-up-on-the-lamp)
must verify actual model choices and due speech.

These process durations are not interaction latency. The change adds no I/O
or timer work to the route; its existing healthy local tool-call target remains
250 ms. The host regression cannot establish live model selection or acoustic
delivery timing. Third used no lamp or real provider.
