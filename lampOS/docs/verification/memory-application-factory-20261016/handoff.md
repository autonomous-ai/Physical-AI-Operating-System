# Shared Memory application preparation — #823

Recorded **2026-10-16 01:43:35 +0700** from the retained audit's `date` call. Worktree: `/private/tmp/lampos-tests-memory-application`; parent-supplied base `587a361a3`. Source is stable for the parent's gate.

## Change

The ordinary binary now includes one private `application.rs` and calls `entrypoint(Reporter::start)`. The shared body accepts only the agreed `FnOnce(PathBuf, CountSource) -> io::Result<Reporter>` factory. It calls it at the original optional reporting site, after actual socket bind, only when the existing `bus_root` option is present.

The original main crate documentation and all three parser-test bodies stay at the main binary root. The shared file contains no tests. There are no public API exports, dead-code allowances, Cargo changes, new binary, control adapter, environment selection, clock changes or integration migrations.

Guard installation still precedes evaluation of `std::env::args_os()`. Parser, private-directory validation, socket ownership, optional Reporter selection, Legacy/Days opening/serving, Reporter finish and exit/error handling keep their exact original order. The default factory is a function value; creating/passing it performs no work or clock read.

## Boundary tests and preservation

Three new tests each exercise Legacy and Days:

- No bus root invokes no factory, then the real store obstruction returns an error.
- A regular file occupying the socket path fails actual binding before the factory or store is touched, preserving that file and preexisting readiness bytes.
- The selected factory runs once after an actual socket exists and old readiness is removed, receives the original root/source, and accepts an owned FnOnce capture. Its deliberate construction error is not returned as the store result: the original fallback proceeds to the real store obstruction. The owned Server's Drop removes the socket.

All paths end at actual file-validation errors before the serving loop; these tests create no Reporter, worker thread, sleep, global setting or indefinitely running server. They do not change any existing assertion.

`audit_source.py` reverses the four factory substitutions and reinserts the extracted body. The **complete original main.rs reconstructs byte-for-byte**, including its crate documentation and entire original test module. Its test-module hash and expected names are retained in `source-preservation.json`:

- `tests::the_two_paths_are_required_and_the_days_and_the_offset_have_defaults`
- `tests::health_reporting_requires_one_explicit_absolute_bus_root`
- `tests::a_relative_path_an_unknown_option_or_a_bad_number_is_refused`

The include introduces no module, and Cargo's binary identity is unchanged, so these remain at the same test paths. Compilation/listing is left to the parent; this worker did not claim a compiled-list result.

The other 23 original Memory Rust files, all 12 integration sources and Cargo.toml remain byte-identical. Documentation adds one shared-startup subsection before Optional health reports, separate from the later scheduling subsection in the other worktree. The scheduler tree was not edited.

## Files and checks

| Owned file | SHA-256 |
| --- | --- |
| `lampOS/crates/memory/src/main.rs` | `e2e19a04685a7da56d1965237e0f73ed5dc77c8eaf0c4abd3cfbec406d05df48` |
| `lampOS/crates/memory/src/application.rs` | `cadc7f5486bbda00b152453faa73939325b1a47149744ae0055616d3dc156168` |
| `lampOS/crates/memory/src/application_factory_tests.rs` | `d17afc389989a348f430c1328dfa839fa5da0572a84a180b3c8b51d06d725a5a` |
| `lampOS/docs/memory.md` | `27d9cd6b7609bd270d96a347cb1ca13a1cf79b163c0faeae7df990c8351b5d3a` |

Rustfmt 1.96.0, edition 2024, `skip_children=true`, passed on the three owned Rust files. Full source reconstruction/hash audit passed. **No Cargo, build, clippy, tests, Git, network or devices were used by this worker.** The parent owns runtime gates and compiled identity confirmation. No timing improvement is claimed for this preparation step.
