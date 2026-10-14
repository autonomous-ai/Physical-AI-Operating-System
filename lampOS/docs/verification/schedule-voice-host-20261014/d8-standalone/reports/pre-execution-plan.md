# d8 source-copy preparation — execution pending

Base: `d8e9556dea3f1867a5af5297d1ebddbd165d9102`.
Frozen source: `/private/tmp/lamp-third-schedule-voice-d8-source-frozen.json`, 697 paths, SHA256 `d14cdfb15ec1e03142fdc48e7e8de71ad30448895cbe891578a8ceaae38e8512`.
This preparation ran no Cargo, test, metadata or helper executable. It wrote only new scratch files outside the repository. The complete host gate remains failed; Task 31 remains not ready. No bounded source-copy result can replace the required complete gate.

Prepared copies:

- Clean archive: `/private/tmp/lamp-third-schedule-d8e9556dea-baseline-5qvw62kj/lampOS`, 1,596 source files / 25,397,904 bytes.
- Implementation: `/private/tmp/lamp-third-schedule-voice-d8-standalone-8r3lfpw_/lampOS`, 1,614 source files / 25,829,091 bytes, 46 overlays, exact 697-path frozen map. Excluded/listed 467 untracked nonruntime/evidence paths.
- Each copy adds the same two helpers, 1,755 bytes combined, from the completed bc3 baseline. Actions SHA256 `8fc55e05703d1aa5ca58348132e3f4b0d7eed202e443f608f0d5af562c8f35e1`; Voice SHA256 `12d36f2d201b1117df21a55574855c8b44bcd2ec06f7b6fa980603e809b41695`.
- Each copy has 35 Cargo.toml files including the workspace manifest. Static declared-path resolution stays inside each copy (baseline 89 path fields; implementation 92); actual offline Cargo metadata containment is pending.
- All source/helper file sets and bytes match their preparation reports. There is no target, Git state, or invalid relative Schedule path in either copy. All 1,481 recorded historical evidence paths remain unchanged.

Reports and immutable preparation snapshots:

- `/private/tmp/lamp-third-schedule-d8e9556dea-baseline.json`
- `/private/tmp/lamp-third-schedule-d8e9556dea-baseline-prepared.json`
- `/private/tmp/lamp-third-schedule-voice-d8-standalone.json`
- `/private/tmp/lamp-third-schedule-voice-d8-standalone-prepared.json`
- `/private/tmp/lamp-third-schedule-voice-d8-preparation-audit.json`

Exact future orchestration, only after explicit Cargo ownership and with permission for local Unix sockets:

```sh
python3 /private/tmp/lamp-third-schedule-d8e9556dea-baseline-capture.py --parent-cargo-released
python3 /private/tmp/lamp-third-schedule-voice-d8-standalone-run.py --parent-cargo-released
```

Run them sequentially and retain each first stdout/stderr stream at its new `capture-first.txt` / `run-first.txt` path. A failure stops dependent work; do not overwrite it or perform automatic retries. The baseline must pass all 72 cases before the implementation runner accepts it. Its immutable preparation snapshot must still match root/source/helpers/archive/fixture paths/profiles/matrix; only then is the completed baseline report hash sealed into the implementation report.

Commands inside the clean baseline:

```sh
cargo build --offline --locked -p lamp-actions -p lamp-voice --example schedule_off_actions --example schedule_off_voice
cargo metadata --offline --locked --format-version 1
```

Commands inside the implementation copy:

```sh
cargo build --offline --locked --workspace
cargo metadata --offline --locked --format-version 1
cargo build --offline --locked -p lamp-actions -p lamp-voice --example schedule_off_actions --example schedule_off_voice
cargo test --offline --locked -p lamp-actions --all-targets -- --test-threads=1
cargo test --offline --locked -p lamp-schedule --all-targets -- --test-threads=1
cargo test --offline --locked -p lamp-voice --lib reminder -- --test-threads=1
cargo test --offline --locked -p lamp-voice --lib tools -- --test-threads=1
```

The broader `tools` filter is intentional and distinct from a `tools::` filtered check. Future counts are not inferred from bc3. No complete Voice/Tasks package gate is scheduled in this runner.

The two helpers are invoked once each per case in each copy. The matrix remains 2 profiles (`ordinary`, `tasks`) × 6 tools values × 6 speak values (absent, `off`, `ON`, `true`, `1`, ` on `). Exact lowercase `on` is outside this default-off matrix. Each side produces 144 raw stdout and 144 raw stderr files; the implementation compares 144 stdout payloads byte-for-byte with the matching new d8 baseline and requires empty stderr. The same new baseline fixture paths are used on both sides; no path/output normalization occurs. Invalid relative socket and ledger paths must remain absent. The helpers remain limited to catalog, environment options, tool declarations, the old fixed route list and ordinary Written serialization; no provider or lamp behavior is measured.

Build/capture uses only inherited HOME, PATH, LANG, LC_ALL, TMPDIR, CARGO_HOME and RUSTUP_HOME, plus its own isolated CARGO_TARGET_DIR and recorded per-case LAMP fixture environment. No provider credentials or runtime environment are forwarded. The implementation checks its complete source/helper set, frozen map file, live main frozen runtime bytes, final baseline report hash and preserved older evidence before/after each stage. Source changes abort the run; there is no relabeling to a later base.

Bounds: baseline helper build 900 seconds; each implementation Cargo stage 1,200 seconds; Cargo metadata 60 seconds; each helper invocation 10 seconds. Implementation stage timeouts terminate their process group with a five-second grace. Local Unix sockets are used by the Actions/Schedule tests, so the eventual source-copy run requires the permitted execution boundary that fixed the bc3 sandbox restriction. No SSH, device, provider, deployment or runtime source change is planned.
