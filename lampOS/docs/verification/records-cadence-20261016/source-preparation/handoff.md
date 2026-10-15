# Records application/cadence preparation — #823

Source frozen at **2026-10-16 04:32:42 +0700** from `date`, in `/private/tmp/lampos-tests-records-report-clock`, parent-created branch `tests/823-records-report-clock`, base `48b3737d5e83efa00278172cb602d3525d368a43`. Fourth notification was supplied by root; this worker made no GitHub call.

## What changed

Six owned files only. `src/main.rs` includes the private `application.rs` and supplies the ordinary command/reporting wrappers and `SystemSchedule`. The complete old application body is shared; the five original main tests stay in their original root `tests` module, byte-identical. There is no fixture executable, protocol, Cargo change, environment switch or integration migration.

The private shared functions are:

```rust
entrypoint(impl FnOnce(Command) -> io::Result<u8>) -> ExitCode
command_with_reporting(
    Command,
    impl FnOnce(PathBuf, PathBuf, Option<u64>) -> io::Result<()>,
) -> io::Result<u8>
reporting_with(
    PathBuf,
    PathBuf,
    Option<u64>,
    &mut impl ReportSchedule,
    impl FnMut(&Path) -> io::Result<Reporter>,
) -> io::Result<()>
```

`ReportSchedule` has pure `now() -> Instant` and fallible `wait(ReportCycle, Duration)`. Its binary-local cycle context carries `started`, the original real `host_started`/`host_seconds`, and the **actual `io::Result<()>` of the combined construction/publication closure**. It intentionally does not invent a publish-attempt count: construction may have failed before `publish`, and `Ok` is not subscriber delivery. The context is supplied only after real work/error handling, the original host lifetime check and the original residual calculation. Zero residual is not parked evidence. No control implementation consumes these observations in this slice.

The system schedule calls `Instant::now` at precisely the two original cadence sample positions and `thread::sleep(remaining)` exactly once, including zero. It does not propagate the already-handled publication error from the context. The only new default-path result is the wait's unconditional `Ok(())`. Real `began.elapsed()`, the conditional error throttle/read, SystemTime, configuration/receipt/cache logic, publication and all cleanup/check commands retain their original source and order.

`Reporter::new_with_cadence_clock` uses the merged Bus API. `Reporter::new` supplies `Instant::now`. The clock is installed before the original declared presence refresh, but Records still separately samples real `started` afterward and still calculates its header timestamp with the exact original `started.elapsed()` body. The publisher factory is called only at the original construction/reconstruction site. A later explicit fixture must supply the same domain to both schedule and Bus cadence; this slice does not couple them implicitly.

## New boundary sources

Nine application tests cover host SystemSchedule/zero wait; initial missing/invalid config before any cadence read/factory/wait; FnOnce report-command selection and original arguments; three actual publication cycles with exactly two loop reads apiece, a 750 ms first residual and two full one-second residuals; zero residual and separate 30-second host lifetime despite an explicit 3600-second modeled work interval; construction failure followed by the existing next-cycle recreation; real config-file bus-root replacement and factory selection; an actual publication error after successful construction and filesystem obstruction; and a held construction operation that cannot reach the wait or emit a Bus Event before release.

Three Reporter tests cover no callback on invalid root/failed real declaration; actual declaration-before-clock and separate real header timing with a future cadence epoch; and real late-subscriber delivery at 200 ms plus actual presence-file recreation at one second, with frozen/pre-boundary exclusion. These operate on real disposable files/Unix datagrams. The held-factory case uses bounded channels and a scoped owned thread with four-second receive guards; no test retry or synchronization sleep is added. The only new test call into system sleep uses the original zero duration.

The new tests have **not been executed by this worker**. Their names and count, 12, are source inventory only. Host filesystem operations and scoped thread joining still cannot promise a universal kernel-level return bound.

## Exact preservation proof

`audit_source.py` passes and writes `source-preservation.json`, `owned.diff`, exact before/after copies and the historical compiled identity excerpt.

- Reversing only named constructor/factory/cadence substitutions, removing the new API block and restoring the extraction-boundary blank line reconstructs **the complete original main.rs byte for byte**. Default wrappers are separately checked against exact expected source, not omitted from the audit.
- Reversing the explicit constructor and removing the new test module reconstructs **the complete original report.rs byte for byte**, including its old test module and untouched `publish` body.
- Removing the named documentation subsection reconstructs original docs exactly.
- All other 15 captured original files are unchanged, including every Records integration source, Cargo, and both Bus source dependencies.
- The selected original realtime function remains entirely byte-identical: SHA-256 `8176cad80df7587ef6a81e1373711027bdcb5b693561785ab6bef642c0e7e7e3`, all 16 assertion sites, original waits, child process and cleanup. This preparation does not accelerate it.

The five original `lamp-records::bin/lamp-records tests::...` identities are retained in `old-main-compiled-identities.txt` from actual raw native run 38780759392. `historical-compiled-source-proof.json` proves that historical main/report/Cargo sources match this pinned base. The include does not create a module, and the old tests stay at the same root paths. A **fresh compiled list still belongs to root's gate**, not to this source proof.

## Frozen SHA-256

| File under the worktree | SHA-256 |
| --- | --- |
| `lampOS/crates/records/src/main.rs` | `33c9354f0d8de852e265002c3a56a8efa7b799bdb2fe5504ff5c326a2460e60f` |
| `lampOS/crates/records/src/application.rs` | `85af4d56754ff6e8b87180401bce78ebdfcbf5d7444d3b32a2dbcfc39ffc0320` |
| `lampOS/crates/records/src/report.rs` | `0a3667e74d854c6b963e659dcb478d5eb8bf5b8faa68be3b6ed49c00d865ead2` |
| `lampOS/crates/records/src/application_schedule_tests.rs` | `46dab0c3551a72a62d7903a0fe6d322e036031fc680d3cba6535e0d03caeec75` |
| `lampOS/crates/records/src/reporter_clock_tests.rs` | `f9fb5a5ccf3b7f967c51289d8a83cb26dded33116c633bcc9f12ef261de350c1` |
| `lampOS/docs/records.md` | `5bf9effbd4b91af3f12bea9da7e4ea6892cb5f0183d172b94df57882b27bf8e8` |

## Validation and next gate

Direct pinned Rust 1.96 rustfmt, edition 2024, `skip_children=true`, passed `--check` on all five owned Rust files; exact command/result are in `format-result.json`, with its empty successful raw `rustfmt.log`. Source reconstruction, unchanged hashes and historical identity provenance checks passed. No Cargo, build, clippy, runtime test, commit, push, network or device operation was run here.

Root should use its reserved target for ordinary workspace formatting, strict `cargo clippy -p lamp-records --all-targets --offline --locked -- -D warnings`, fresh `cargo nextest list -p lamp-records --message-format json --offline --locked` identity comparison, all Records tests without retries (serial where required), and the normal workspace build/native gate. Preserve the original ignored timing and Linux-only privileged cases; their platform-specific compiled shape is unchanged. No new performance or runtime result is claimed before those gates.
