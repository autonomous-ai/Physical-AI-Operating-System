# ARM64 pipeline baseline, 15 October 2026

Measured before changing `.github/workflows/lampos-arm64.yml` for #788.
The baseline was posted to [the issue](https://github.com/autonomous-ai/lampOS/issues/788#issuecomment-6187684658)
before implementation. Observation times came from `date`; run and step
times came from GitHub Actions metadata and logs.

- [Baseline](baseline.md): exact latest ten runs at the snapshot, latest
  ten terminal runs including cancellations, and ten successful runs.
- [Compilation and execution](compile-execution.md): cache hits/misses,
  Cargo compilation, and actual test execution measured separately.
- [Test identities and crate times](test-manifests.md): parser definitions,
  per-run counts and hashes, plus serial-versus-four-runner comparison.
- [Reference executed identities](38697914780.tests-passed.txt): 4,479
  sorted unique records, SHA-256
  `7539b376757adbaf1932c90b1bec9cdc254da6b6e88b8d813c018ac62bdb0203`.
- [Crate execution seconds](crate-times-all-ten.csv) and
  [crate check counts](crate-test-counts-all-ten.csv): all ten successful
  runs. Execution sums are not elapsed wall time.

The raw inputs are public run logs linked in the baseline. Fetch them
with `gh run view <id> -R autonomous-ai/lampOS --log` and job metadata
with `gh run view <id> -R autonomous-ai/lampOS --json jobs`. The log parser
used outside the repository recognizes nextest PASS/LEAK rows, Cargo
target headers and result groups, doctests, and both custom harnesses.
Every Cargo group's passing count was checked against its own summary;
the serial and sharded manifests matched independently of aggregate
counts. The raw logs and extraction scratch files from this session are
also retained locally under `/private/tmp/pipeline-788-baseline/`.

The reference's executed list equals both serial runs 38696914090 and
38693725124, with zero added or missing records. The newer serial run
38700545585 has 54 added records and none missing. These commits differ,
so this is evidence of emitted identities, not a proof that every test's
source was identical. The new workflow compares original selection,
archive selection and actual execution within the same source revision.

## What was not shown

The snapshot includes pending and cancelled work. Lower bounds and partial
durations are labelled; they are not successful run measurements. Queued
`startedAt` values were not used as evidence that a runner had started.
The historical logs do not expose exact push delivery latency. No claim
of a ten-minute result or three improved pull requests follows from this
baseline. No lamp was accessed.
