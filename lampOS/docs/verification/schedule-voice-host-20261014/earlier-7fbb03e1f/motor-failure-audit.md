# Task 31 gate: inherited motor fixture failure preflight

Read-only inspection on 2026-10-14 while the full gate was still running.
No Cargo/test run, source edit, device access or process action was performed.
The inspected log `/private/tmp/lamp-third-schedule-voice-gate-tests.txt`
reported one failed test at line 427:
`coordinator::motor::tests::frozen_config_and_executable_are_exact_and_source_changes_are_not_adopted`.
The assertion/backtrace was not yet present in the inspected output. This note
does not assign a cause.

## Exact source comparison

The following read-only command exited **0 with no diff** from repository root:

```text
git diff --exit-code 3773c1fb -- lampOS/crates/voice/src/coordinator/motor.rs lampOS/crates/voice/src/coordinator/motor/tests.rs lampOS/crates/voice/src/process.rs lampOS/crates/body/src/runtime/supervisor.rs lampOS/crates/body/src/runtime/worker.rs lampOS/crates/body/src/runtime/channel.rs lampOS/crates/body/src/runtime/config.rs lampOS/crates/ipc/src/lib.rs
```

Thus the fixture, preparation, session-directory, supervisor/worker/channel
and clock files inspected here match upstream `3773c1fb` exactly. This does
not imply the newly linked Voice test executable is byte-identical to upstream.

## What the named test actually does

`voice/src/coordinator/motor/tests.rs:780` calls `fixture("normal")`, waits for
`OptionalMotor::is_ready`, reads frozen config/executable bytes and compares
both SHA-256 digests, overwrites only the original `input.json` with `{}`, and
checks the frozen config remains unchanged. Then `cleanup` (`:458`) sends
shutdown, waits for reap, drops the motor and recursively removes this test's
private directory. The source executable is not changed by this particular
test despite the broad title; no motion offer is issued.

`fixture_with_controller` (`:381`) creates a random private SessionDirectory,
writes mode-0600 JSON and a mode-0700 shell wrapper, then calls
`PreparedMotor::load`. The wrapper exports child-only fixture variables and
executes **the current Voice unit-test binary** with exactly
`--exact coordinator::motor::tests::inert_child --nocapture` (`:403`).
It does not launch the ordinary Voice main/service, provider or reminder
consumer. `inert_child` (`:307`) opens Unix channels and uses the real Body
worker over an in-memory protocol emulator; no serial/GPIO/audio/camera device
is opened. Supervisor redirects its stdin/stdout/stderr to null
(`body/src/runtime/supervisor.rs:232`), so a child panic does not automatically
appear in the parent test output.

Preparation (`voice/src/coordinator/motor.rs:118,214`) nofollow-opens owned
regular source files, checks permissions/link counts/length, copies and hashes
bounded bytes into new private files, detects source metadata change and
syncs output. Configuration bound is 16 KiB; executable bound is 64 MiB.
`FrozenFile::drop` (`:190`) removes only its recorded inode. The fixture's
executable copy is the small wrapper, not the full unit-test binary.

## Real-clock and process boundaries

| Boundary | Current source |
|---|---|
| Qualification validity | Config construction starts it approximately 1 ms in the past and ends it 10 seconds later; `tests.rs:54`. Preparation and spawn both check it. |
| Test wait | Every `wait` call has its own 2-second real `Instant` deadline; refreshes authority about every 10 ms, polls, parks 1 ms (`tests.rs:433`). Timeout reports `motor.failure()`. |
| Child channel connection | 2-second child-side loop, 1 ms parks (`tests.rs:321`). |
| Supervisor startup | 3 seconds (`body/runtime/supervisor.rs:18,425`); test's 2-second readiness wait can expire first. |
| Fresh authority/channel | Authority maximum lease 250 ms (`turn-owner/src/types.rs:6`); Body channel packet maximum age 100 ms (`body/runtime/channel.rs:12`). |
| Worker loop/progress | Tick 2 ms, progress 10 ms, initial authority allowance 250 ms (`body/runtime/worker.rs:17`); fixture maximum lifetime 3 seconds (`tests.rs:78`). |
| Supervisor progress | 50 ms since last accepted observed progress (`supervisor.rs:19,428`). Enforcement happens before receiving queued events as well as after polling (`:539`). |
| Shutdown | One Stop send; inhibit acknowledgement deadline 20 ms, teardown 250 ms after inhibit receipt, reap deadline 250 ms after kill (`supervisor.rs:20,372,399`). Parent test still waits up to 2 seconds for reap. |

These are configured deadlines, not measurements. A readiness timeout, stored
channel/progress failure, digest assertion, filesystem error and cleanup
timeout have different implications; the pending assertion is needed to tell
which branch failed. The fixture's explicit mode is `normal`, so its deliberate
crash/hang/gated failure branches are not selected.

## Task 31 interaction and limits

No direct call from this test enters the new Schedule/Reminder code. Session
directories use `/tmp/lampOS-<random 128-bit boot>` (`voice/src/process.rs:52`);
the new tests do not use the same fixed motor directory or mutate the fixture's
environment/clock. Inspection found no Task 31 production change to the files
above. The shared unit-test binary, process launch, Unix IPC, filesystem and
host scheduler are real shared resources, so indirect load/startup effects
remain possible, not established. Source equality and a serial test harness
alone do not rule out those effects or an inherited defect.

During unwinding `OptionalMotor::drop` preserves frozen inputs if reap was not
observed (`motor.rs:1023`); Supervisor drop kills only its owned child and tries
one nonblocking reap (`supervisor.rs:587`), while SessionDirectory drop merely
removes an empty directory. A failed wait can therefore intentionally leave
evidence/cleanup unresolved. Do not erase leftovers or count forced teardown
as confirmed normal cleanup to make the gate pass.

Next evidence belongs to the parent: retain the final assertion/backtrace,
then run the exact single test after the full gate has ended. If the failure
is readiness-related, preserve `motor.failure()` and distinguish 2-second test
wait from 3-second supervisor startup, 100-ms IPC age and 50-ms progress before
considering any narrowly scoped fixture or production change. No deadline
change is recommended from the single FAILED line alone.
