# Capacity PR: independent native Schedule failure

PR #868, head `1e38b543c9e0f58688ddd089e508b100f4e8d70c`.
[ARM64 run 38760383411](https://github.com/autonomous-ai/lampOS/actions/runs/38760383411)
failed in unchanged `lamp-schedule::runtime` test
`a_finite_service_stops_and_releases_its_owner`: `Schedule store worker stopped`
at `crates/schedule/tests/runtime.rs:105:36`, after 0.016 s.

The original contiguous failure/summary excerpt is retained unchanged.
The run reports **2,551 passed, one failed, 126 existing profile skips**;
2,145 selected tests were not executed after fail-fast. This is not a green
native gate for the capacity change. Build passed; subsequent checks and
artifact publication did not complete. No retry was requested.

The Schedule owner is notified in [#869](https://github.com/autonomous-ai/lampOS/issues/869).
A shutdown/owner-stop race is a source-based hypothesis, not a proven fix.
No Schedule product code was changed. PR #868 stays draft. Its separate
unchanged local capture-stall failure is #866, retained in the PR evidence.
