# The ARM64 pull request check

`.github/workflows/lampos-arm64.yml` runs the whole workspace for pull
requests into `lamp-voice`, pushes to `lamp-voice`, and manual dispatches.
The target is at most ten minutes from a pull request push to its result,
including queueing. The measurements for [#788](https://github.com/autonomous-ai/lampOS/issues/788)
are in [the baseline](verification/arm64-pipeline-20261015/baseline.md).

A newer pull request push cancels its older run. Each `lamp-voice` run has
its own concurrency group so every merge remains eligible to run:
`cancel-in-progress: false` alone still allows GitHub to replace an older
pending run in a shared group.

## Build once, run the same tests

One Ubuntu 22.04 ARM64 job checks out the event revision, builds all release
test binaries with Rust 1.96.0, and packs a nextest archive. It also runs
the existing release build for every workspace binary and example. Four
Ubuntu 22.04 ARM64 jobs consume that archive. Each runs one test at a time,
using the unchanged `hash:N/4` partition and exclusion expression. Tests
run as the ordinary runner user, so permission tests remain meaningful.
Nextest is pinned to 0.9.148, the version used by the measured baseline.

For pull requests, every job tests GitHub's merge of the head with
`lamp-voice`. The producer records that checkout's SHA and absolute
workspace path. Consumers check both, verify the archive's SHA-256, and
extract it into the same workspace. Tests embed `CARGO_BIN_EXE` and
`CARGO_MANIFEST_DIR` paths, so moving the archive to an arbitrary temporary
target directory would break them. After extraction, consumers use the
archive's Cargo and binary metadata; they do not compile again.

The producer retains the original commands for the tests nextest cannot
run:

```sh
cargo test --locked --release --workspace --doc -- --test-threads=1
cargo test --locked --release -p lamp-ipc --test relay_signals
cargo test --locked --release -p lamp-turn-end-wire --test process
```

The four existing check names remain the merge gates. If the producer or
one of these extra test commands fails, all four gates explicitly fail
instead of being skipped. Cancellation also reaches that small failure
gate before checkout or testing. No repository protection settings need
to change for the job split.

Release binaries, the `schedule_check` and `replay_cost` examples, and
`SHA256SUMS` retain their existing `lampos-arm64-<head-sha>` artifact name
and seven-day retention. A merge checkout is tested, while the artifact's
name identifies the pull request head, as before. Install only artifacts
from an entirely successful run; uploading binaries does not certify the
four test jobs.

## Cache scope and freshness

Only the producer restores Cargo registry, Git dependencies and `target`.
The cache key includes the runner platform, Rust version, lockfile and
Cargo configuration, with a source SHA suffix. Restore prefixes allow
Cargo to reuse compatible older output and validate what must rebuild.
The old cache key is also accepted during migration.

Only push builds on `lamp-voice` publish the shared cache. Pull requests
can restore their base branch's cache even though the repository's
default branch is `main`; sibling pull request caches are not shared.
The producer saves after successful compilation, before running tests.
A failing test therefore does not discard a useful build cache. Each new
source revision can refresh it; the former lockfile-only key could never
be updated after its first successful save.

The compiled test archive is transferred within its own workflow run as
`lampos-test-build-<run-id>`, without recompressing its Zstandard data.
It lasts one day. Rerunning failed test jobs can reuse the successful
producer's archive; rerunning the producer replaces only that run's
intermediate artifact. This is not an automatic retry of a failed test.

## Proving coverage

The producer lists the original workspace selection and each of the four
partitions. `ci/test-inventory.sh` canonicalizes the binary ID and test
name. The sorted concatenation of the four partitions must equal the
original list, including multiplicity: missing or duplicated tests fail
the job.

Each consumer lists its tests from the archive and compares that list with
the producer's corresponding partition. After a successful test run, it
compares the actual `PASS`/`LEAK` records with the expected identities.
`LEAK` retains nextest's existing success semantics. An inventory proves
selection; the execution comparison also proves that the selected tests
ran. Ignored tests retain their status in the JSON evidence. There are
no added retries, filters, changed assertions or changed product code.

The original and partition inventories travel with the build archive.
Per-runner JSON, raw execution logs and expected lists are retained for
seven days, including on failure. The executed list and its comparison
are produced after a successful test command. Doctest and custom-harness
logs are separate seven-day artifacts. Shell steps use Bash with
`pipefail`, so recording output with `tee` cannot hide a test failure.

Compare historical runs at identical test source revisions. A count alone
is insufficient: unrelated pull requests can add or rename tests. The
baseline includes exact identities and a serial-versus-sharded comparison.

## Measuring the result

Use `gh run list` and `gh run view <id> --json jobs`, plus
`gh run view <id> --log`. Record the observation time with `date`.

- Queue wait: workflow creation to each job's first actual `Set up job`
  step. A queued job can have a nonzero `startedAt` without running.
- Build and execution: step boundaries, with Cargo's compilation duration
  and nextest's execution duration recorded separately.
- Total: workflow creation to its last job's completion; also retain the
  terminal workflow update. For the build/consumer design, report consumer
  queue delay after the producer finishes separately from time waiting
  for the producer.
- Push-to-result: record the local `date` immediately before the actual
  push, the resulting head SHA, and terminal result. Historical Actions
  records expose workflow creation, not exact push delivery latency.

Read-only polling is bounded. Do not rerun a failing test to obtain a
green result: give a flaky test its own issue and fix its determinism.
Do not increase paid capacity or change repository settings for latency
without the owner's approval.

## References

- [Nextest archives and build metadata](https://nexte.st/docs/ci-features/archiving/)
- [Nextest test-list JSON](https://nexte.st/docs/machine-readable/list/)
- [GitHub cache scope](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching)
- [GitHub concurrency and pending runs](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)
- [GitHub scheduled workflow restrictions](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#schedule)

## What was not shown

The workflow passed actionlint 1.7.7 with ShellCheck 0.10.0 enabled.
Both helper scripts passed ShellCheck and `bash -n`, and
`bash lampOS/ci/test-inventory-check.sh` passed its format and failure
cases locally. A temporary macOS fixture with nextest 0.9.148 passed
three tests and preserved one ignored test after archive extraction and
metadata-only execution, with Cargo absent from `PATH`. The fixture
checked compiled executable paths and source fixture paths; its planned
and executed lists matched. This is not an ARM64 workspace result.

This implementation has not yet had its own native ARM64 run. The
ten-minute target and three real pull request comparisons remain open.
No lamp was accessed. Cold-cache performance and queue time must be
measured; compilation reuse alone does not establish either.

Nightly runs are also still open in #788. GitHub schedules execute only
workflows on the default branch. This workflow currently lives on
`lamp-voice`; adding a cron here alone would not activate it. Enabling
nightly dispatch requires coordination with the owner of `main`, without
changing repository settings or pushing to another builder's branch.
