# Schedule development failures and resolutions

These are Third's development-host results on 2026-10-14, not lamp results.
The raw logs are retained under `development/`; the full dependency gate is
reported separately in `results.json`, with final qualification-client checks
in `supplement-results.json`. A filename containing `final` reflects
the writer's provisional filename, not a claim that its contents passed.

| Evidence | Failure or finding | Resolution |
|---|---|---|
| `storage-first.txt`, `storage-compile.txt` | Offline cache lacked the already-locked Jiff timezone bundle. No test executed. | Fetched locked dependencies with network permission, then ran the gate offline and locked. No dependency version was changed to conceal a failure. |
| `initial-check.txt`, `calendar-first.txt`, `storage-integration-first.txt` | Darwin does not expose the used `connect_unix`, `SocketFlags::NONBLOCK` and `CLOEXEC` API combination. Compilation failed. | Use generic Rustix connect plus explicit nonblocking/CLOEXEC setup. |
| `storage-integration-second.txt` | A test fixture inferred `Vec<i32>`, which cannot be bound as a SQLite BLOB. | Made the fixture bytes `u8`; the later storage suite has 21 integration passes and one unit pass. |
| `msgs-tests.txt`, `msgs-clippy.txt` | An integration test attempted a private notice check method. Compilation failed. | Expose `ScheduleNotice::check` as part of the public structural validation API, also called by the enclosing message check. The focused final suite has 27 passes. |
| `calendar-second.txt` | 36 passed, one failed at Jiff's upper millisecond boundary. | Build a Timestamp from seconds plus fractional nanoseconds; all 37 calendar tests pass. The supported endpoint is written explicitly in the interface. |
| `engine-first.txt` | 34 engine passes and three protocol passes, one protocol failure: Serde's unit `Status` variant accepted an unknown field. | Use an empty struct variant with strict unknown-field handling. The next engine/protocol run had 38 passes. |
| `runtime-first.txt` | Four passed, 14 failed with sandbox Unix socket permission denials. | Repeat local IPC tests with host permission; these were sandbox denials, not an automatic approval-review rejection. |
| `runtime-host.txt` | 12 passed, six failed: five invalid hyphenated test bus-subscriber names, and a real fragmented-read failure on Darwin. | Use valid subscriber names and explicitly make accepted streams blocking before applying cumulative I/O timeouts. |
| `engine-third.txt` | 35 passed, one failed: eight maximum-size item views exceeded the reply bound when request IDs were also maximal. | Page by both item count and serialized bytes; preserve whole items. The regression now follows every cursor and verifies all eight texts/IDs without truncation or duplication. |
| `audit.md` | Source review found synchronous stderr logging could block client replies/shutdown, and a corrupt persisted receipt could replay an incorrect success. | Remove per-request synchronous logging; retain diagnostic counters. Validate each receipt's exact operation response and revision against recoverable state. Added corrupt-receipt and lost-Ack/Snooze restart regressions. |
| `runtime-failures-first.txt` | Two tests failed during fixture bring-up: invalid subscriber spelling and socket-bound CLI startup denied by the sandbox. | Correct the fixture identifier and run the same local tests with host permission. Final two passes cover full unread stderr, bounded shutdown, stale-owner notice withdrawal, and expired queued mutation refusal. |
| `calendar-clippy.txt`, `storage-clippy-first.txt` | Strict Clippy rejected an unnecessarily negated boolean. | Use the direct `is_none_or` form. |
| `calendar-clippy-final.txt`, `storage-clippy-final.txt` | Strict Clippy rejected a redundant binding left after removal of request logging. | Return the match expression directly. These two provisional “final” logs are failures; the later full dependency Clippy gate is the authoritative pass. |
| `format-pre-gate.txt` | Workspace format check required sorting the new Schedule module declarations. | Formatted Schedule and Msgs; full workspace format gate then passed. |
| `gate-attempt1-clippy.txt` | First full dependency Clippy gate rejected `samples + 1 <= MAX_ITEMS` in the new qualification example. No gate tests had started. | Use the equivalent `samples < MAX_ITEMS`; retain the first attempt and restart the entire gate. |
| `qualification-attempt1.json`, `measurement-attempt1.json` | The first 100-warm-run release qualification exited 1 after 26.872 seconds with bare `EINVAL`; no completed timing report was produced. A two-item preflight then passed. | Added stage-specific error context without changing timing boundaries; retained both results as development evidence, not final measurements. |
| `context-diagnosis-qualification.json`, `context-diagnosis-results.json`, `timeout-repro.txt` | The full context run reproduced `EINVAL` on the first pending List page at `IPC set_read_timeout`. A standalone Unix socket pair reproduced rejection after the writer closed, despite 4,096 readable buffered bytes. The same timeout values worked with the writer open. | Correct the qualification client to use nonblocking I/O and readiness polling under its original cumulative deadline. Add buffered-close and deadline/framing regressions. The production owner, storage engine and message types are unchanged. |


| `check-poll-tests-first.txt` | The first qualification-client regression run passed eight tests but blocked in two fixtures that filled a socket before starting a reader. The test child was terminated with SIGTERM. | Stream those larger fixtures concurrently with bounded waits, preserving the byte-limit assertions. The corrected example suite passes all ten tests; the full Schedule supplement includes them. |

The focused core/runtime run after the functional fixes passed 36 engine,
four protocol and 18 runtime tests. The full dependency gate includes these suites again. The subsequent example
correction and its tests have separately recorded final source hashes; no
production source changed between those gates. No new test is ignored and no assertion was weakened
to make a failure disappear.

Preparation also corrected a mistaken source filename (`diagnostics.rs`), a
temporary dependency declaration before resolution, and unsuitable macOS
filesystem probes (`df -T`, `diskutil info` with a directory path). Those are
not runtime test failures. The machine record retains the unsuccessful probes;
`df -h` and the matching mount entry identify the APFS data volume actually used.

The socket finding is consistent with Apple's published
[`sosetoptlock` implementation](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/uipc_socket.c),
which rejects option changes once both socket directions are shut down. Rust
[1.96.0's timeout conversion](https://github.com/rust-lang/rust/blob/1.96.0/library/std/src/sys/net/connection/socket/unix.rs)
uses truncated microseconds; the initially suspected fractional-duration
rounding was not the cause. The executed reproduction is evidence for this
host, not a claim about every Darwin or Linux release.

The standalone socket reproduction was compiled and run from `lampOS/`
with its pinned `rustc 1.96.0 (ac68faa20 2026-05-25)`; both commands exited 0:

```sh
rustc --edition 2024 /private/tmp/lamp-third-schedule-timeout-repro.rs -o /private/tmp/lamp-third-schedule-timeout-repro
/private/tmp/lamp-third-schedule-timeout-repro > /private/tmp/lamp-third-schedule-timeout-repro.txt 2>&1
```

The reproduction source and output are retained under `development/` with
those filename prefixes removed. A final documentation-check script initially
tried to parse a block of independent JSON-line requests as one JSON document
and reported extra data, then wrongly asserted that every value was an object
(the tool catalogue is an array). The check was corrected to decode each JSON
value in order. These were checker errors, not malformed protocol examples.

A whole-diff whitespace check reported trailing whitespace in two captured
Clippy suggestions and final blank lines in raw Cargo outputs. Those logs are
kept byte-for-byte. The implementation and edited prose pass the whitespace
check with the raw verification-output directory excluded; this does not
exclude any runtime test or required gate.

## Not shown

None of these failures or fixes demonstrates Linux/ARM64 execution, actual
power-loss behavior, flash durability, authenticated clock synchronization,
or acoustic delivery. The source audit was an independent inspection; executed
regression evidence is labelled separately. No lamp was accessed.
