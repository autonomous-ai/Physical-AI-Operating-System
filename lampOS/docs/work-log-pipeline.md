# Pipeline work log

## 2026-10-15 16:50 +0700 — shared ARM64 capacity ready

Ready for lamp-voice: no; native PR checks are running.

- Owner-approved capacity is provisioned: `lampos-arm64-16`, Ubuntu 22.04
  ARM64, 16 cores, 64 GB RAM and 600 GB disk. Pool ID 1 is ready; group
  `lampos-arm64` (ID 3) allows all autonomous-ai repositories at the owner's
  request. All teams share its 40-job ceiling; no static public IPs.
- The capacity change is [draft PR #814](https://github.com/autonomous-ai/lampOS/pull/814),
  head `03bba231e53cfd93c41284e4acf3dc2b03ec6567`. Push began at 16:48:08
  +07 from `date`; [run 38715632523](https://github.com/autonomous-ai/lampOS/actions/runs/38715632523)
  was created at 09:48:17 UTC. The existing four partitions and all test
  commands remain unchanged. actionlint with ShellCheck passed.
- #802's producer and consumers 1/3 passed. The producer selected 4,484
  nextest identities; those two consumers executed 2,163 exact matching
  identities without Cargo compilation. Consumer execution was 300.364s
  and 352.427s. Shards 2/4 were still queued at 16:47:17 +07, so this is
  partial evidence only, not a green workflow or a latency improvement.
- Source-cache commit `1546b7232` is prepared separately on
  `pipeline/788-source-cache`: content, mode, path, configuration and
  toolchain checks guard source timestamp restoration. Three unit tests,
  24 Cargo correctness cases, rustfmt, Clippy, ShellCheck and actionlint
  passed locally. Native performance and a warm shared cache remain open.
- Nightly commit `300e4ae1a` is prepared separately on
  `pipeline/788-nightly`: a tiny default-branch caller invokes the same
  full ARM64 workflow, resolving lamp-voice once and pinning consumers to
  that SHA. Static validation and unchanged-command comparison passed.
  Activation needs the first builder to backport the caller to main after
  review; no other builder's branch was pushed.
- Four exact-duplicate push runs were cancelled, preserving their matching
  active PR runs; details are on #788. No flaky test was retried and no
  assertion changed. No lamp was accessed.

## 2026-10-15 — #788 archive implementation prepared

Ready for lamp-voice: no

- Baseline posted before implementation:
  [timings](https://github.com/autonomous-ai/lampOS/issues/788#issuecomment-6187684658)
  and [crate timings/test identity comparison](https://github.com/autonomous-ai/lampOS/issues/788#issuecomment-6187877480).
- Four-runner reference: 17m12s to last job completion. Its 4,479
  executed check identities match two serial runs exactly. Exact latest
  ten, terminal ten and successful ten samples are preserved separately;
  pending and cancelled work is not presented as successful evidence.
- Prepared on `pipeline/788-arm64-cache`: one ARM64 producer, four
  unchanged serial nextest partitions, refreshable base-branch cache,
  archive/path/source identity checks and selected-versus-executed test
  inventories. Doc/custom tests and release artifacts retained. No
  product code or test assertions changed; no retries.
- Local checks passed: actionlint 1.7.7 with ShellCheck 0.10.0, ShellCheck
  and Bash syntax for both helpers, helper format/failure checks,
  `git diff --check`. A pinned nextest 0.9.148 temporary macOS fixture
  passed three tests with one ignored, after extracting the archive and
  running from metadata without Cargo on PATH. Compiled executable and
  source fixture paths worked; selected/executed identities matched.
- Native ARM64 run requested by [draft PR #802](https://github.com/autonomous-ai/lampOS/pull/802),
  head `dcf8e547a5e2434688ed94d020c4343199d8be56`, after the owner's
  explicit publication approval. Push began at 08:46:18 UTC, from `date`;
  the PR was opened by 15:46:36 +07. It stays draft until its own run is
  green. The earlier automatic commit/push rejection is resolved.
- Next step studied locally: Cargo 1.96 rebuilds unchanged source when
  checkout changes its timestamps. A temporary fixture demonstrates that
  blindly backdating changed inputs can reuse stale code; any follow-up
  must match actual content and canonical paths, and invalidate on
  structural/configuration changes. No timestamp restoration is included
  in this change.
- Remaining: own green ARM64 run; three real pull requests within ten
  minutes; nightly dispatch coordination because main lacks the workflow.

## 2026-10-15 — #788 taken

Ready for lamp-voice: no

- Task: [#788](https://github.com/autonomous-ai/lampOS/issues/788), an ARM64
  result within ten minutes of a pull request push, with the same tests.
- Claimed at 15:12 +07 (from `date +%H:%M`), after reading the role,
  joining rules, Queue and #785. No lamp.
- Base: `origin/lamp-voice` at `af7aed833`; implementation branch:
  `pipeline/788-arm64-cache`. The base branch is already checked out in
  another worktree, so this session did not switch or pull there.
- Measuring the newest ten runs, including their queue waits, and the
  latest completed runs for build and per-runner execution timings before
  changing the workflow. Preserve test identities for comparison.
- Allowed scope: the ARM64 workflow and its scripts and documentation.
  No test assertions changed, no flaky-test retries, no device access,
  no repository settings or paid capacity changes.
- Gate and native ARM64 evidence: not run yet. Three real pull requests
  and before/after test-list equality remain to be shown.

## Proposed tasks

None.

## Found, not mine

None.

## What was not shown

The archive has partial native ARM64 parity evidence from two consumers;
the remaining two are queued. No complete ten-minute result, three improved
pull requests, warm guarded-cache result or active nightly schedule has been
shown yet. No lamp was accessed.
