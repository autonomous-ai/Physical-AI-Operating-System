# Relay first full ARM64 shard: dependency download failure

Recorded 2026-10-16 07:52:53 +0700, from shell `date`. PR #942 head `917a19c5c36bdecb965f404385df386c16b22af4`, full run [38831310695](https://github.com/autonomous-ai/lampOS/actions/runs/38831310695), shard 3 job `116750485039`.

The build failed before any test ran: crates.io `se/rd/serde` download failed with curl 55 / SSL_ERROR_SYSCALL. The build step lasted 00:46:17–00:46:18 UTC in GitHub's raw timestamps, and the test step is skipped. [Unedited completed-job log](shard3.log), SHA-256 `e67bafd7f0e1706d411f8c868dd7c0dbe71dcd68a2c7b204b97884146f13a5c6`, and [run/job snapshot](run-during-first-attempt.json) preserve the actual first failure. The other jobs were still running when captured; this is not the full run's terminal result.

No test or successful shard has been rerun. Completion of the other jobs and any explicitly identified unexecuted infrastructure follow-up must remain separate evidence. Normal PR run 38831311751 is also in progress. Neither run is yet qualified. DevOps owns the workflow/cache/runner response to this dependency-download failure; Tests changes none of those settings.
