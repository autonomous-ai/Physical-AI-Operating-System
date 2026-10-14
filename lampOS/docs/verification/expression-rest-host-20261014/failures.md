# Expression rest: development failures

These checks ran on Third's Intel i7-10700K development Mac, not the lamp.
The final gate and source hashes are in `results.json`; earlier successes
below do not substitute for that gate. Text logs preserve diagnostics and
test outcomes; trailing terminal whitespace and blank terminal lines are removed before committing.

| Check | Observed failure and correction | Retained output |
|---|---|---|
| Missed hold, first observed after resume while still Listening | A valid higher audio generation cleared the queue but still entered the legacy backward-state retirement rule. The focused regression failed 0/1. Exempt the validated new generation from that rule; terminal and expired turns remain retired. | `engine-missed-hold-failure.txt` |
| Activity arriving late from a held interval | Resuming Listening in the same generation cleared activity history without advancing its observation boundary. Delayed true/false activity from the hold was accepted and could reconstruct a pause. The focused regression failed 0/1. Advance `voice_since` on every resume, even with no state change. | `engine-held-activity-failure.txt` |
| Replay/runtime adaptation | One of 18 tests compared JSON objects and treated omitted optional `first_audio_us` as different from explicit `null`. Compare parsed typed inputs, retaining all identity, generation and timing assertions. No production parser change was needed. | `replay-initial.txt` |
| Strict Clippy | Simplify a negated activity-admission expression into named conditions. Replace a complex benchmark tuple with a named `Scenario` and a constant assertion with the observed injected timestamp difference. No warning suppression was added. | `clippy-initial.txt`, `clippy-engine-initial.txt`, `clippy-replay-initial.txt` |
| First broad gate attempt | Format and Clippy passed on the preceding source. The final activity regression changed two files after the standalone source copy, so Third stopped only that gate's process group during its build and restarted the complete gate on frozen source. The interrupted build is not a test failure or a completed build. | `superseded-STOP-REASON.json`, `superseded-lamp-third-expression-rest-gate-results.json`, corresponding `superseded-*.txt` |
| Staged whitespace check | `git diff --cached --check` found blank terminal lines in seven captured test logs. Remove only terminal whitespace and blank terminal lines; runtime source and test outcomes are unchanged. | `log-whitespace-check.txt` |
| Machine inventory | The default sandbox refused the two read-only CPU/memory `sysctl` calls with `Operation not permitted`. Their recorded failures remain alongside the successful host-permission recheck. | `host.json` |

The first engine run passed 33 inherited and 25 new tests. Review added more
boundary cases; the final focused engine run passed 33 inherited and 32 new
tests. Replay/runtime/program checks passed 52 tests, and the six catalogue
and six additive message-contract tests passed. Their retained focused logs
are supplementary to the final dependent-crate and standalone gates.

`cost-exploratory-debug.json` is an earlier debug-profile first-plus-100-warm
execution, retained as exploratory evidence. It ran during development and
is not a comparable before/after baseline for the final release benchmark.

## Not shown

No real speaker queue, motor, lamp recording, optical response or acoustic
latency was tested. Fixes here govern planner evidence and requested output;
First must implement the corresponding producer fence and qualify consumers
on the lamp.
