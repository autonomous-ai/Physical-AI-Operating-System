# d8 standalone host evidence

The clean baseline and Task31 implementation copy both use base `d8e9556dea3f1867a5af5297d1ebddbd165d9102`. The implementation matches the frozen **697-path Rust/manifest map**. Its six planned Cargo stages completed once with **282 passed test executions, 0 failed, 2 ignored probes**. All **72 default-off cases / 144 exact stdout comparisons** passed against the new clean d8 baseline.

These are measurements on **Darwin 25.6.0, x86_64, rustc 1.96.0 (ac68faa20 2026-05-25)**. No lamp, live provider, acoustic path or ARM64 execution was used. **The complete host gate remains failed and Task31 remains not ready.** This bounded source-copy check does not replace that gate.

The [baseline report](reports/baseline.json) and [implementation report](reports/implementation.json) retain their exact original bytes, command arrays, working directories, machine metadata, source hashes and output paths. [Provenance](provenance.json) records counts, timing boundaries and exclusions. The [retention index](retention-index.json) maps each original artifact to its retained bytes and hash; [the completed source-copy audit](reports/implementation-filesystem-audit.json) records its final filesystem verification. The initial [preparation audit](reports/preparation-audit.json) and [execution plan](reports/pre-execution-plan.md) retain their earlier pending-status wording as historical records.

| Stage | Result | Host command seconds |
| --- | --- | ---: |
| Clean baseline helper build | Passed | 29.346817001 |
| `workspace-build-first` | Passed | 81.328243692 |
| `helper-build-first` | Passed | 26.886553336 |
| `actions-tests-first` | 33 passed / 0 failed / 0 ignored | 30.280789031 |
| `schedule-tests-first` | 160 passed / 0 failed / 0 ignored | 35.574009614 |
| `voice-reminder-tests-first` | 70 passed / 0 failed / 2 ignored | 33.457532221 |
| `voice-tools-tests-first` | 19 passed / 0 failed / 0 ignored | 2.352309432 |

These command wall times include any compilation and test execution; they are not lamp interaction latency. Filtered suites may overlap, so 282 is an execution count, not a unique-test count. The `2` ignored probes were not run. This standalone command uses the broader `tools` filter; it must not be substituted for a `tools::` filter when comparing counts. All six stages completed; there was no skipped stage or automatic retry in either d8 runner.

| Source corpus | Original source files | Original source bytes | Scratch helpers |
| --- | ---: | ---: | ---: |
| Clean git archive | 1,596 | 25,397,904 | 2 files / 1,755 combined bytes |
| Implementation copy | 1,614 | 25,829,091 | 2 files / 1,755 combined bytes |

The helper byte count is the sum of both helpers in each copy. The [source maps](source/) record lengths and SHA256 values. The implementation applies 46 task overlays and excludes/lists 467 untracked nonruntime paths. Each offline Cargo metadata result resolves 34 local crate manifests inside its own copy. Git state, credentials, target trees and generated binaries were not imported into the source copies; tracked source fixtures/assets remained part of the archive. The final audit verified 1,481 historical evidence paths unchanged.

The [readings and pair map](comparison-readings.json) covers **288 raw stdout files and 288 raw stderr files**: 144 of each per copy. Each row identifies stage, case, profile, both switches, helper role, original path, retained path, byte length and SHA256. Every stderr file is empty. Each side's stdout readings total **562,392 bytes**; storage deduplication retains all exact bytes as **3 distinct stdout payloads / 14,428 bytes**, plus the zero-byte stderr payload. No output normalization occurred. The fixture Actions catalog maps to its identical retained stdout payload.

The unchanged matrix is 2 profiles (`ordinary`, `tasks`) × 6 values of `LAMP_SCHEDULE_TOOLS` × 6 values of `LAMP_SCHEDULE_SPEAK`: absent, `off`, `ON`, `true`, `1`, and ` on `. Exact lowercase `on` is deliberately excluded. Paired runs use the same new baseline fixture paths; their path-bearing hashes are specific to this run. Invalid relative Schedule socket and ledger paths were not created. Ordinary mode declares 14 tools; Tasks mode declares 17, including `check_agent_task`.

The exact [Actions helper](helpers/schedule_off_actions.rs.txt) captures its public catalog. The exact [Voice helper](helpers/schedule_off_voice.rs.txt) captures environment-derived options, declared tools, pure routes for its fixed old-name list, and ordinary `Written` serialization. It does not directly route-check `check_agent_task`, compare newer `End.sound` or other input variants, contact a provider, exercise audio or motors, or measure lamp timing. Both helpers are stored with `.txt` suffixes to avoid adding runtime Rust sources.

The clean baseline came solely from the exact git archive, independently of main-worktree changes. Its immutable [prepared report](reports/baseline-prepared.json) was checked against the completed baseline source/root/helpers/archive/matrix before the implementation runner sealed the final baseline-report hash. The [implementation prepared report](reports/implementation-prepared.json) retains its pre-execution identity; its null completed-baseline hash describes that earlier state only. Both executed reports and all source/helper bytes stayed fixed through the actual comparison.

The first d8 baseline and implementation executions both passed with the authorized local Unix-socket permission. Earlier restricted bc3 failures and 1a/dcc/7f gate failures remain in their own evidence folders and retain their original status. No prior result is rewritten or relabeled. No binary, archive, target tree, Python script or file ending in `.rs` is retained here. Original paths and prior evidence references in the unmodified reports are provenance; historical artifacts outside this d8 scope remain in the parent verification folder.
