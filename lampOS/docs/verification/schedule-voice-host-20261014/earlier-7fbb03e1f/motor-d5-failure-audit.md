# d5 motor timeout audit — read only, 2026-10-14

Observed: the serial d5 gate exited 101 after 676.865360202 s: 1,111 passed,
2 failed, 8 ignored in 16 completed harnesses. Voice lib was 918/2/8 in
491.59 s. Both panics are `motor/tests.rs:441`, `owned child timed out: None`.
The raw log is `/private/tmp/lamp-third-schedule-voice-final-gate-tests.txt`;
the sibling `-results.json` and `-source.json` retain commands and 675 hashes.
No test, Cargo job, source edit, process inspection or hardware call ran here.

## Which waits failed

- `failed_reap_after_stopped_is_latched_and_reported_once`: tests.rs:1001 is
  the only `wait` call in that test. Its predicate is `OptionalMotor::is_ready`.
  The failure is before `begin_shutdown` (1002), Stopped (1005–1011), and the
  separate one-second child-reap loop (1027–1035). This is initial readiness,
  not evidence that failed reaping after Stopped is broken.
- `crash_and_stalled_control_never_allow_replacement_or_false_cleanup`:
  tests.rs:643–645 iterates `crash`, then `hang`, and waits for `reaped`.
  The log does not identify the failing mode. Cleanup's later identical
  predicate (464) would already be satisfied after this first wait succeeds;
  restart at650 cannot replace the still-retained child (motor.rs:495).
- The helper at433–451 has a two-second deadline. It asserts before the next
  publish/poll, publishes fresh authority every10ms, and parks1ms. `None`
  means only the OptionalMotor failure field has not yet been populated.

## Common source path and timing boundaries

- Fixture tests.rs:402–429 writes a shell wrapper which execs this exact test
  executable's `coordinator::motor::tests::inert_child`. It does not enter
  normal Voice startup or the Schedule consumer. No actual motor is opened.
- Body `runtime/supervisor.rs:230–236` binds parent sockets then spawns that
  wrapper with stdout/stderr null. Child panics therefore are absent from the
  parent raw log. `started` is recorded just before spawn, at231.
- Child tests.rs:320–325 binds/connects with its own two-second bound, then
  builds the fake backend/worker. Body worker.rs:437–451 only emits Ready after
  receiving usable authority. TestLink sends Ready before mode `crash` exits7
  or `hang` stops servicing control (tests.rs:270–291).
- Supervisor allows3s before Ready (supervisor.rs:18,424–426), then requires
  progress within50ms (19,428–429). It stores nonzero exit via try_wait at561–567.
  OptionalMotor adopts returned errors and stored child failures at670–714.
- Thus slow pre-Ready startup or a parent pause before its next poll can leave
  failure=None when the test's shorter bound fires. Once a hang Ready has been
  processed and polls continue, the50ms progress rule should store a failure.
  These are source inferences, not measured scheduling, startup or crash causes.
  The raw log supplies no elapsed phase, child mode, connection state or exit.

## Relation to the upstream socket correction

HEAD had moved to7fbb03e1f during this audit. Read-only diff against d5 is empty
for motor.rs/tests.rs, Body runtime supervisor/worker/channel, and Voice
process.rs. The relevant changed runtime code is IPC Endpoint::connect:
an owned nonprivate socket now returns WouldBlock instead of PermissionDenied.
Crucially, d5 Body channel.rs:135–153 already waits for BOTH owned peer sockets
to become private before connecting. That handles the ordinary bind/chmod
window. Directory::observe_peers (supervisor.rs:97–112) pins identity without
mistaking temporary permissions for a fault. Voice process::Worker startup
is not this fixture's launch path. A socket failure would normally be latched
by supervisor.poll and OptionalMotor.poll; the observed None does not show it.
There is no demonstrated link to the7f fix and no deterministic defect found.

## Narrow diagnostics Main can run, not run by this audit

Preserve/hash the exact failed d5 test binary before another build overwrites
it. The raw log names `target/debug/deps/lamp_voice-04f83e70e13d855a`; that name
alone cannot prove its bytes still match. Run the preserved binary separately:

```sh
RUST_BACKTRACE=full <preserved-d5-voice-test-binary> --exact coordinator::motor::tests::crash_and_stalled_control_never_allow_replacement_or_false_cleanup --test-threads=1 --nocapture
RUST_BACKTRACE=full <preserved-d5-voice-test-binary> --exact coordinator::motor::tests::failed_reap_after_stopped_is_latched_and_reported_once --test-threads=1 --nocapture
```

Use the already-authorized Unix-socket environment; retain both exits/output.
A parent backtrace locates the wait but cannot recover null child stderr or
the crash/hang mode. If reproduction needs instrumentation, a separate scratch
fixture copy can record mode, spawn/connected/Ready/exit times and both stored
failure fields, capturing child output in bounded files. Keep that result
separate from unchanged-source qualification; do not relax production deadlines.
