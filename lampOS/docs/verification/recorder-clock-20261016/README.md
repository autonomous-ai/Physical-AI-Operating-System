# Recorder clock and wait preparation (#823)

Recorded 2026-10-16 00:42:15 +0700, from `date`. Full-gate base: `b60062b861f8c2350675c87eb3802a7ceac4c86a`.

The explicit `room_recorder_with_clock_and_wait` entry point supplies one pure nanosecond clock and a wait adapter after the actual ready-file write. The default entry point keeps the original monotonic clock, one full host sleep, parsing, private directory and receipt, missing audio/metadata, and configured exit code. The optional declaration takes its original pre-write sample; an explicit adapter can sample freshly after the write. Declared coverage never certifies actual child exit.

Nine new boundary tests pass. [Source preservation](source-preservation.json) reconstructs the original recorder body byte for byte with only its clock and wait substitutions reversed. All other original fake-lamp source and all 23 integration sources are unchanged; all 14 serve identities, including the three recorder cases, retain their original assertions and host waits.

Formatting, strict evaluator all-target clippy, **301 full evaluator tests, zero failures or ignores**, and workspace build passed. The [exact commands and date boundaries](gates.json) and raw logs are adjacent. Shared-target workspace outputs were cleared before compilation to avoid stale local crate artifacts from another worktree. Registry dependencies were retained.

This is a clock preparation only: no new CLI selection, recorder wire role, scenario migration or timing improvement is claimed. Native ARM64 remains pending. No retries, profile changes or device access.

## Rebase qualification

Rebased onto `6b08a70d56e37b061e7a9a6b013072b13639a0b7`. The [source proof](rebase-20261016.json) preserves the complete recorder region and all 24 existing/new test source files. Reversing only the recorder seam restores the complete integration-branch fake-lamp source byte for byte. The new documentation section is placed before the existing backend-clock section; upstream text/order is retained.

Fresh formatting, strict evaluator all-target clippy, **103 library tests (zero failures or ignores)** and workspace build pass. [Exact commands and dates](rebased-20261016-gates.json) and unedited logs are adjacent. These cover the merged finite-clock helpers plus this recorder preparation. The original 301-test full gate remains original-source evidence; it was not rerun or relabelled as a fresh full gate. All original integration test sources and the recorder implementation are unchanged across the rebase.
