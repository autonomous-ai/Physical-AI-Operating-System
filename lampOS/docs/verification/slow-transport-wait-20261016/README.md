# Explicit full-delay transport wait preparation

Recorded 2026-10-16 01:58:12 +0700, from `date`, for #823. Kind: off unless an explicit library entrypoint is used. No CLI, global clock switch, existing scenario migration or timing-improvement claim.

`slow_transport_with_wait` parses the original arguments and calls one fallible `FnOnce(Duration)` with the full u64 millisecond duration, then executes the original command with the original arguments and inherited streams. The ordinary entrypoint supplies the original one `thread::sleep`; neither entrypoint adds a clock sample. A wait error returns unchanged before exec.

Six new boundary tests cover malformed shape/numbers before waiting; zero, 400 ms, beyond-u32 and full-u64 durations; consuming typed errors; actual exec failure; and an owned real child replaced by exec while preserving its PID, literal argv, one queued input line and exit 37. Tests own bounded cleanup/reaping and introduce no retries or sleeps. All 23 original integration Rust files remain byte-identical to the integration base. The new test file is unchanged across rebase. Reversing only the adapter and test-module addition restores the entire base `fake_lamp.rs`; removing only the documentation section restores the entire base document.

## Validation

- Full original gate on `587a361a3`: formatting; strict evaluator all-target clippy; **302 passed (100 library + 202 integration), zero failed/ignored/filtered**, across all 25 evaluator suite summaries; workspace build.
- Rebase on `2bf2eaf76f2b273215c5a3fe3e9a6e018f3d6617`, including merged finite coordination and recorder preparation: exact source/docs reversal and all 23 integration files verified against this base, then fresh formatting, strict evaluator clippy, **165 library tests / zero failed or ignored**, and workspace build.
- The prior full gate is retained as prior, not described as a fresh full gate on the rebase. Native ARM64 qualification is pending when this PR opens; the normal profile must execute all six new cases before it is ready.

`root-gates.json` and `integration-gates.json` retain exact commands, target/workspace and start/finish times from `date`; their raw logs are unedited. The earlier worker `audit.json` records source-authoring time (`tests_executed: false`); the separate root gate records above establish execution. Scratch Python only orchestrated commands and exact byte/hash comparisons; no Python is committed under lampOS.

The eventual delayed-relay migration must additionally prove a fresh actual relay registration after terminal wait acknowledgement. Delay completion alone is not proof of exec, readiness or child exit. Existing assertions, wait values, runtime argv and real process cleanup are unchanged here. No lamp/device run was made.
