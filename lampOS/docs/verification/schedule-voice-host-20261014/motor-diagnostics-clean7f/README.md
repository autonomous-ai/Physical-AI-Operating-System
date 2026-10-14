# Clean-7f motor diagnostics on the Third development host

Three exact diagnostic cases passed, then one full motor-module invocation
passed 28 tests. No earlier timeout was reproduced; its cause remains unproven.
The original Task 31 run (27 passed / 1 failed) and clean-7f run (25 passed /
3 failed) are retained alongside these diagnostics. This bundle does not
qualify Task 31 or a subsequent integration revision.

The tested base was `7fbb03e1f2e61ad6dfaf184cd768bad6524c2d76`, in an isolated
source copy containing no Task 31 changes. Only the motor test file was
instrumented. No production code, existing deadline, or assertion was changed.
All measurements are on the development Mac described in [machine.json](machine.json),
using Rust 1.96.0; none is a result on a lamp.

## Read this bundle

- [Report and interpretation](report.md), [per-fixture analysis](analysis.json),
  and [compact command/source/binary metadata](run-summary.json).
- [Three exact-case results](exact-cases/results.json), with the original
  compile log, individual raw logs and metadata in `exact-cases/`.
- [Single 28-test module result](motor-module/results.json) and
  [raw log](motor-module/tests.txt).
- All 88 original saved wait records are under `diagnostics/`: ten for the
  exact cases and 78 for the module. Raw evidence files are copied verbatim.
- [Original failed Task 31 metadata](original-failures/task31-7f.json) and
  [log](original-failures/task31-7f.txt); [original failed clean-7f metadata](original-failures/clean-7f.json)
  and [log](original-failures/clean-7f.txt). The [binary references](original-failures/binary-references.json)
  explicitly say those original hashes were captured after the failed runs,
  not at execution time.
- [Diagnostic-only Rust diff](source/instrumentation.diff),
  [clean source map](source/source-clean.json), and
  [instrumented source map](source/source-instrumented.json). The other source
  snapshots prove all 1,436 copied files stayed unchanged across the commands.

The diff is preserved evidence, not a production patch proposal. Its paths
are rooted at `lampOS/`; application to a separate clean diagnostic copy
would occur at that copy's parent root. The [pre-execution plan](source/pre-execution-plan.md)
and [prepared metadata](source/prepared.json) are historical records from
before execution; their original `started: false` fields do not override the
completed run results.

Absolute scratch paths in raw records and the original report are provenance.
[path-map.json](path-map.json) maps every copied original text artifact to
its relative location here. No external scratch file is needed to read the
run evidence. Full application source is identified by commit and source maps,
not embedded as a second source checkout.

No binary, archive, Python source, bytecode, credentials, or provider output
is included. [Outside-repository metadata](retained-outside-repository.json)
keeps only the paths/hashes of excluded runners and binaries. Do not infer
authorization for another diagnostic execution from this record.

`files.json` gives byte counts and SHA-256 values for every payload file;
the external retention manifest additionally hashes `files.json` itself and
names the exact proposed repository destinations. This bundle was assembled
without running Cargo, tests, or editing the main worktree.

## What was not shown

Instrumentation changes execution timing; only selected parent samples are
retained, and missing best-effort child markers are not proof of an absent
phase. Passing diagnostics neither erase original failures nor establish a
loader/OS cause. No physical motor safety, acoustic behavior, reminder
behavior, later rebase, or complete Task 31 gate was demonstrated here.
