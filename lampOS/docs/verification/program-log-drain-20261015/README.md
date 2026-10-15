# Program log-drain sweep (#783)

Source before the change: `4ad0ec5c3d0b245662cbc0d28e6940e8618fce03`. Updated 2026-10-15 18:56:18 +0700 (from `date`).

Health and memory tests can receive a successful bus/request result while the child’s asynchronous log relay still holds the lines that the test will inspect. Killing immediately loses those lines. A regular stderr file does not synchronize the relay.

This change waits for complete terminal records before the same SIGKILL/reap, with a five-second failure bound and no retry. All existing assertions remain unchanged. Production logging, service behavior and crash/restart semantics are unchanged. The eighth retains the policy qualification and issue-close decision.

## Scope and findings

The direct scan has 18 candidate files, including the already-fixed policy control. Six adjacent signal/output paths were also inspected. The two findings are fixed here; the others already observe exit or recorded evidence, or do not assert the killed child’s logs. [sweep.json](sweep.json) retains per-path evidence and locations.

| File at audited source | Disposition |
| --- | --- |
| [crates/bus/tests/cli.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/bus/tests/cli.rs#L98) | Two deliberate recorder kills read JSONL afterward, but both first wait for exactly the records later asserted. Running::kill discards the killed child's captured streams; separate finite CLI commands use output/wait_with_output. |
| [crates/controls/tests/program.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/controls/tests/program.rs#L294) | Normal path waits for a real bus diagnostic, sends SIGTERM, waits for successful normal exit, then reads the CLI summary and completed recording. SIGKILL only occurs if the Running guard is still populated while unwinding. |
| [crates/controls/tests/touch_program.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/controls/tests/touch_program.rs#L141) | CLI output is redirected to files, the normal process exit is observed and reaped, and only then are stdout/stderr files read. Kill is the Running fallback; it is not used before the normal assertions. |
| [crates/expression/tests/program.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/expression/tests/program.rs#L19) | The killed live owner is validated through bus state and lock exclusion. Its stderr is only read on an already-exited failure diagnostic. The asserted owner-lock stderr belongs to a second, naturally exited CLI process. |
| [crates/expression/tests/runtime.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/expression/tests/runtime.rs#L604) | The journal child is a re-executed test process writing stderr to a file. Normal try_wait completion is observed before the exact one-line journal assertion. Drop kill is a timeout/panic cleanup or a harmless call after the child is already reaped. |
| [crates/health/tests/program.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/health/tests/program.rs#L36) | Three callers kill before reading the journal they assert. The child uses the asynchronous log relay. The fix observes complete startup/terminal records before SIGKILL, then reaps and drains all remaining bytes. |
| [crates/ipc/tests/probe.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/ipc/tests/probe.rs#L41) | Every success path calls TestChild::output, which observes normal exit and takes the child before wait_with_output. Only timeout/panic cleanup kills. The invalid-control stderr assertion follows natural failure exit. |
| [crates/ipc/tests/sockets.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/ipc/tests/sockets.rs#L320) | The child writes a monotonic timestamp file and naturally exits. Kill is only in the ten-second timeout branch, immediately followed by panic; the success path never reads the file after killing. |
| [crates/memory/tests/binary.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/memory/tests/binary.rs#L85) | Six SIGKILL sites in two crash/restart tests precede log-file assertions, including privacy scans. The fix observes the complete terminal occurrence at the expected record position before SIGKILL; file-backed stderr alone was not a barrier. |
| [crates/memory/tests/health.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/memory/tests/health.rs#L156) | Long-lived memory children are killed only when Process is dropped, after test assertions. Positive log assertions use logged(expected), which reads the redirected file while the child is alive; the exact count checks follow those barriers. Startup-failure cases await natural exit. |
| [crates/mood/tests/program.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/mood/tests/program.rs#L12) | The live owner killed by Drop is checked through MoodState and actuator-topic absence. Its piped stderr is never asserted. Asserted refusal stderr belongs to a separate output() process; finite summaries also use natural output(). |
| [crates/mood/tests/timing.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/mood/tests/timing.rs#L83) | The manually ignored timing test kills its live owner after bus-based latency measurements. Stderr is inherited, stdout null, and no child log content is read or asserted. |
| [crates/policy/tests/judges_program.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/policy/tests/judges_program.rs#L56) | The two-second operator process exits by its own configured lifetime. The harness takes it from Running, waits with output, then asserts judge lines and drained=true. Drop kill is fallback only. |
| [crates/policy/tests/program.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/policy/tests/program.rs#L59) | Fixed control: Running::end starts a stderr reader and waits up to WAIT=5 s for both expected prefixes before kill; it reaps, joins the reader, and drains queued bytes before assertions. Drop kill is fallback cleanup. |
| [crates/records/tests/records.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/records/tests/records.rs#L222) | The live reporter is dropped after all expected typed diagnostics and the persisted receipt have been inspected. Its stderr is inherited, stdout null. Post-kill log assertions are absent. |
| [crates/schedule/tests/runtime.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/schedule/tests/runtime.rs#L456) | This is an intentional crash-recovery test. It waits for a successful durable set receipt before SIGKILL, reaps, restarts, and checks receipt/item recovery. Both child output streams are null. |
| [crates/schedule/tests/runtime_failures.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/schedule/tests/runtime_failures.rs#L114) | The fixture deliberately fills and leaves stderr unread. It tests status service and cooperative SIGTERM completion, then asserts exit and socket cleanup; it never drains or asserts log text. Drop kill is failure cleanup. |
| [crates/tasks/tests/runtime_failures.rs](https://github.com/autonomous-ai/lampOS/blob/4ad0ec5c3d0b245662cbc0d28e6940e8618fce03/lampOS/crates/tasks/tests/runtime_failures.rs#L290) | Both tests keep stderr deliberately full/unread. One sends SIGTERM and asserts clean exit/socket removal; the other triggers storage failure and awaits error exit. No assertion consumes killed-child stderr; Drop kill is fallback. |

## Health: keep the complete journal

A reader drains the stderr pipe while the child lives. Readiness requires LF-terminated records, including the final level change where one is expected; a prefix in a partial read does not count. The host disk may make the initial board Ok, Warn or Error. If it starts Error there is no later level transition; otherwise the final voice error follows the warning. The other two callers wait for startup and the first complete health record; the unchanged assertions still require their exact expected journal contents.

After readiness or failure, the harness kills/reaps, joins the reader and appends every queued byte before checking the unchanged full journal. Missing evidence fails after the bounded cleanup. A focused regression checks incomplete terminal bytes and a missing startup record.

## Memory: retain the entire crash log

The harness retains each existing stderr file path and reads bytes while the child lives. It waits for the final complete operation record at or beyond the expected record position and for the required occurrence. It then sends SIGKILL and reaps exactly as before; all final whole-file reads and privacy checks remain. No extra protocol request or graceful shutdown changes the crash point.

| File | Complete record position | Terminal operation | Occurrence |
| --- | ---: | --- | ---: |
| log1 | 4 | status | 1 |
| log2 | 4 | status | 1 |
| legacy.log | 3 | remember | 1 |
| days1.log | 8 | search | 1 |
| days2.log | 7 | session_open | 2 |
| days3.log | 9 | search | 2 |

The last two repeat an earlier operation; simple substring presence would permit an early kill. The predicate ignores incomplete tails only when deciding readiness, and never filters the evidence file. Regression cases retain a private suffix through its newline and require the later repeated record, not an unrelated line count.

## Validation

- `cargo fmt --all --check`: passed from `lampOS/`.
- `cargo clippy -p lamp-health -p lamp-memory --all-targets -- -D warnings`: passed.
- The initial `cargo test -p lamp-health -p lamp-memory -- --test-threads=1` exited 101 before reaching memory: the host disk had only 4% free and the stale-case barrier expected a stale diagnostic, while the board correctly reported Warn; the separate normal-exit argument test lost its usage tail. [Raw initial output](local-initial.log).
- The stale-case barrier now waits for the complete initial health record, leaving the existing exact journal assertions untouched. The host-dependent test fixture was then run on a dedicated, healthy 256 MiB HFS+ sparse-image volume through `TMPDIR`; no production disk thresholds or assertions were changed.
- `TMPDIR=/private/tmp/tests-783-fixture cargo test -p lamp-health -- --test-threads=1`: 43 passed, one failed before the serve target. All six other program tests passed, including the five-second lease and new newline regression. The unchanged normal-exit argument test again lost stderr. [Raw output](local-health.log). That is an open product bug, [#837](https://github.com/autonomous-ai/lampOS/issues/837), separate from SIGKILL; the [source audit](normal-exit-audit.md) distinguishes the observed failures from the inferred relay interleaving.
- `cargo test -p lamp-memory -- --test-threads=1`: 151 passed, five pre-existing measurement tests ignored, no failures; includes all six binary tests. [Raw output](local-memory.log).
- `cargo test -p lamp-health --test serve -- --test-threads=1` and `cargo test -p lamp-health --doc`, with the same healthy `TMPDIR`: the previously unexecuted serve target passed all 12 tests; no doc tests. [Serve output](local-health-serve.log), [doc output](local-health-doc.log). Total health coverage across the final invocations: 55 passed, one failed.
- `cargo build --workspace`: passed, 2m 35s. [Raw output](local-build.log).
- Native ARM64 check: pending PR. The existing PR profile excludes the five-second health lease test; its local result will be reported separately.
- Exact source comparison retained all 100 original health assertions, all 88 memory assertions and every original test identity; one health and two memory completion-boundary tests were added. [Machine-readable comparison](assertions-and-identities.json). The scratch checker parsed balanced assertion macro invocations, compared their original text and counted test functions against the audited base; it is not a substitute for test execution.

No lamp or device access. No failed test was retried to obtain a pass. The second health invocation followed an actual barrier correction and a documented fixture-environment correction; both failures remain recorded. This is a source/harness fix; loaded policy qualification and three ARM64 policy passes belong to the eighth’s part of #783.

## Boundary of this sweep

A complete terminal record is a barrier for the finite request logger in these tests, not a guarantee that an arbitrary future concurrent writer has drained. Both helpers preserve every captured byte for the existing assertions.

The policy control already waits before kill, but its current `lines().starts_with(...)` predicate can accept an unterminated fragment. That separate boundary is recorded for its owner; this patch does not change the policy qualification under way.
