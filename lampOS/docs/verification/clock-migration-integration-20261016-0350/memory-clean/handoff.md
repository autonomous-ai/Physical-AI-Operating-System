# Clean Memory health fixture integration handoff

Recorded 2026-10-16 03:32:20+0700 from `date`. Source is frozen in `/private/tmp/lampos-tests-memory-health-fixtures`, branch `tests/823-memory-health-fixtures`, on clean base `48b3737d5e83efa00278172cb602d3525d368a43`. This task performed only the authorized file integration, source audits and a direct pinned rustfmt check. No Cargo/build/tests, Git mutation, network or devices. Neither earlier worktree was edited. Root reports fresh gate 99186 started at 03:31:26 on its separate memory-application target; this handoff does not claim that ongoing gate passed.

The port is complete with exactly 13 changed/new paths. All 33 Memory Rust source files are byte-identical to the qualified tree's `/private/tmp/tests-823-memory-health-migration-evidence/root-final-result.json` manifest. The prior 296 passed / 0 failed / 6 ignored result remains evidence for that older tree. It cannot transfer to this integration by compiled-input identity because inherited dependency source changed.

## Three-stage provenance

1. `base-snapshot.json` records the initially clean base, every original Memory file, absent/new destinations, and the unchanged merged #897 application/main/factory tests. `base/` retains original bytes only for destinations that already existed.
2. `reporter-stage.json` records the exact two Rust blobs from #910 commit `fd3a2764cb6624ac825fa7a044317d9080ff1e3d`. Only its exact 21-line `Reporter scheduling in local fixtures` section was inserted in base `docs/memory.md`; removing that section reconstructs the base documentation. No #910 verification artifacts were copied.
3. `health-stage.json` gives per-file provenance and exact base/final SHA-256 values for the qualified child, protocol, schedule, parent helpers, corrected child tests, original health migration, manifest and fixture documentation. It proves all 33 qualified Memory Rust hashes match, the qualified tree was left intact, all other base tracked files are preserved, and the changed/new set is exactly the 13 authorized paths. Per-file base-to-final diffs are included.

The unchanged merged application anchors are:

- `src/application.rs`: `cadc7f5486bbda00b152453faa73939325b1a47149744ae0055616d3dc156168`
- `src/main.rs`: `e2e19a04685a7da56d1965237e0f73ed5dc77c8eaf0c4abd3cfbec406d05df48`
- `src/application_factory_tests.rs`: `d17afc389989a348f430c1328dfa839fa5da0572a84a180b3c8b51d06d725a5a`

The original base health integration source still has hash `31add261e22dda578ca5f5c0b9016a1382f7a1e3d87f6968a1d083ec722e394d`; the migrated copy is exactly `0d829067a29a9e5105326b1e0b50c09436e6aefe5091849a683be282910f9d91`. Consequently the earlier independent seven-scenario/33-assertion/reversible-source proof applies to the same before/after source bytes. No scenario check, constructor or timing value was edited during this port.

## Manifest and lockfile

Memory `Cargo.toml` is byte-identical to the qualified manifest (`9c76d873ad6b870b46a3e3a7fad8cb3bf80ee0a31ca4b5ccaaa1d72dedac1875`). Removing only the fixture `[[bin]]` block and the fixture rustix comment/dependency reconstructs the new base manifest exactly. The explicit fixture binary retains `test = false`; rustix retains features `fs` and `event`.

The new base lock was preserved. Its sole modification is `"rustix",` in the existing `lamp-memory` dependency array. Parsed package comparison proves no other base package, version, checksum or dependency entry changed. The already resolved rustix remains 1.1.5. Final lock SHA-256 is `40d62fead16a6f138c4d7dbe400a2e4a9855ceece96d19b1e642136431aed4c5`. The older qualified lock was never copied.

## Why fresh gates are required

`compiled-input-equivalence.json` statically follows the selected Bus and Memory test packages' local dependency manifests, including root dev dependencies and all target alternatives. It hashes 459 candidate input files across 15 local packages; the complete hash map and 24 changed candidates are saved. This conservatively includes some files disabled by cfg and is not a Cargo-resolved feature graph or a compiler dependency manifest.

All Memory source/assets and its manifest, all Bus source/assets and its manifest, and every local dependency manifest in that graph are byte-identical to the qualified tree. The following inherited changes prevent broader identity:

- IPC `src/log_relay.rs` extracts `finish_on` and adds a no-op seam around actual read/write boundaries. This is the real stderr relay used by marker visibility, so its source identity matters even though the inherited production paths are intended to preserve behavior. The new IPC `relay_exit` test is enumerated but was not a root test target in the earlier Bus+Memory gate.
- Msgs exports a new `selftest` message module, changing a direct Memory dependency's library source.
- Voice's production attention/coordinator/camera/second/provider-worker source differs. Voice is compiled as a Memory dev dependency. Changed Voice internal test files are listed conservatively; they do not by themselves imply those tests ran in the previous gate.
- The Body `bus_out.rs` change is confined to a `cfg(test)` body. Body was a dependency, not one of the old gate's root test packages; this is a conservative file difference, not by itself a compiled production difference.
- The workspace adds `crates/selftest`. Compared with the qualified lock, the only extra package is `lamp-selftest`; every shared package record is identical. The whole-workspace build therefore has additional inherited inputs, including other source changes outside this 15-package graph.

All these base changes remain intact. Their exact textual differences are in `dependency-diffs/`. Fresh root fmt, strict Clippy, compiled list/scenario checks, full relevant tests and workspace build own qualification on this base. No inherited behavior was changed to manufacture byte identity.

## Validation and publication boundary

The direct pinned command succeeded with exit 0 and empty output:

`/Users/autonomous/.rustup/toolchains/1.96.0-x86_64-apple-darwin/bin/rustfmt --check --edition 2024 --config skip_children=true` on the nine owned Rust files.

`rustfmt.json` records the exact argv, start/finish from `date`, output hash and unchanged file hashes. Repository status still contains exactly the five tracked modifications and eight new files listed in `health-stage.json`; no source changed after root froze it for gates.

This local integration remains unpublished pending #910 merge. Root owns dependency landing, rebase, runtime gates, evidence publication and the eventual separate fixture PR. No speedup or lamp-performance claim is made by this source port.
