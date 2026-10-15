# Independent serve-relay migration review

Read-only review completed at `2026-10-16 02:25:43+0700` from `date`, against
`/private/tmp/lampos-tests-serve-relay-progress` and its seeded-source inventory.
No blocking correctness defect was identified in the new source. Root owns
compilation, actual child/socket execution, paired measurements and publication;
none was run by this reviewer.

## Preserved original evidence

The complete section containing all six original test bodies is byte-identical,
including all 37 assertion sites, original fault options, 3-second actions,
1-second run, serve policy suffix and full child-versus-spawn argv equality.
Its SHA-256 is `8040e3cbda58f22296ef057df328d8c3017fde6afc162954852ec675daff4ce2`.
The assertion counts remain 11/4/4/7/6/5 in original order. No test is ignored,
retried, filtered or replaced with a simulated exit status.

Independent reconstruction recovered the entire original `fake_lamp.rs`,
`fixture_commands.rs`, `tests/serve_relay.rs`, and docs by reversing only the
new adapter/helper/module/doc substitutions. The live shared controlled body,
not a baseline copy of it, was used for that reconstruction. All 40 other old
test files and all 69 untouched seeded files match their hashes, including the
session, fake serve, protocol, driver, main dispatch and backend.

## Inspected implementation

- `fake_lamp.rs:118–159` derives the parser slice from the one received vector
  after validating its leading absolute control pair. The exported invocation
  adapter cannot take an independent report vector. `fixture_commands.rs:58`
  retains its actual received arguments while extracting the path; main still
  removes only executable/command before handing them over. The existing real
  stdout record writes that vector once, returns its count only after flush,
  and contributes exactly one line to runtime progress. The ordinary fake and
  direct controlled API keep their original parser/report behavior. In
  particular, existing semantic `into_serve` validation still follows the argv
  line; the adapter does not silently reorder old errors or clock samples.
- `tests/common/serve_relay_clock.rs:265–310` clones only execution options,
  inserts the explicit controlled runtime and socket ahead of every unchanged
  fake prefix argument, and calls the unchanged session engine on this test
  thread. The SessionStart argv and actual Command are still created from the
  same options in `session.rs`; no parent-generated child report is substituted.
  The old pilot test therefore continues to inspect an actual spawned child's
  complete received argv and the exact original policy suffix.
- The helper's clock Cell is separate from its RefCell controller. Reads do no
  work or advancement. `before:143–196` accepts the actual runtime, sends a Step
  only for initial/changed time or a new real signal receipt, and uses completed
  flushed stdout counts as reader targets. Repeated same-time relay polls reuse
  the parked runtime's counts while actual runner EOF or child exit is pending.
  They do not poll the runtime merely to infer progress from empty pipes.
- `fire_runner:121–140` sends the original Stop bytes once after the complete
  delay, counts only a successful write, or actually closes the real writer.
  `completed:217–232` holds time for actual EOF propagation and the relay's
  successful SIGTERM-send receipt. A following Step requests observation of
  the existing runtime signal handler; no signal observation is fabricated.
- Boundary clipping (`completed:234–250`) retains 2-ms polls, full 3-second
  Stop/Close, 1+5-second hard deadline, and full 10-second ignored-signal grace.
  Silent mode adds no helper Stop. The ignored runtime continues polling until
  the original session branch kills and waits for its actual process group at
  the grace boundary (`session.rs:1253–1264`). A terminal runtime Ack is neither
  required nor fabricated after that forced kill.
- Runtime logical completion (`before:188–190`) retains final stdout and freezes
  time. Only the session's actual `Child::try_wait` route (`session.rs:1224`) or
  its actual kill/wait route supplies terminal relay progress. The terminal
  callback returns promptly, leaving the old process-group check, late drain,
  metadata validation and SessionEnd emission intact. `run` checks terminal
  progress and actual function return before giving output to the original
  parser. `Control::drop:255` closes peer/writer on return, error or panic;
  session Cleanup still owns actual child/group kill and reap. No helper-owned
  threads are introduced or detached.

## Host-bound distinction to retain in the published evidence

The helper's retained 10-second post-signal budget is a **success condition**,
not a strict 10-second abort-latency guarantee. `host_until` is checked before
and after polls, and terminal success is rejected after expiry. However,
`session.rs:1155` intentionally gives its existing reader-enqueue barrier its
own 30-second host watchdog; that wait can delay reporting an over-budget
failure. The existing 2-second group check and 200-ms late drain also remain
real cleanup work. A frozen clock cannot turn those errors into clean success,
but the code must not be described as aborting every stalled path within 10 s.
Root acknowledged this distinction and will correct docs/handoff wording;
no new session API is recommended for this exceptional failing path.

The original `exited_us - signal < grace` assertion now compares logical
protocol timestamps. It is not a host/device shutdown-latency measurement.
Actual exit/reaping, the retained host success guard and separate host timing
results provide different evidence and must be reported separately.

## New tests and limits

Four invocation tests inspect literal argument slicing/reporting, invalid
prefix/parser/mode errors and failed-flush accounting. Six helper tests exercise
real Stop bytes and EOF, held logical time, terminal-role separation, expired
host budgets, boundary clipping and real endpoint cleanup on unwind. The helper
tests' typed progress values are scheduling inputs, not real child receipts.
The six unchanged integration bodies remain the real argv/signal/kill/reap and
metadata checks; this source review does not assert that they passed.

Reviewed hashes match the author's handoff:

```text
9d18247e2e04944cc09c2cae2074e0c1e759e4a59332255cc3188cb762fc014f  fake_lamp.rs
39f8706b3444b15ce52fbaa4250d4beb3c57d03ab5c411921e2c5082dba1feda  fixture_commands.rs
89f89a010c4a240aa699fd37fb49e0a2c4f2ed8f65ce77332c5c9b575e3d12dc  serve_invocation_tests.rs
836a72136db9573ca12e8b4aeab77ff95e42010cf71fcd652da6318945ec7bf2  tests/serve_relay.rs
60f5ee58ee905b285ea54c5d6c1a2159964e97a2945a79654d283ca14d0ee9cb  tests/common/serve_relay_clock.rs
```

Independent hashes, per-body identities and reconstructed files are under
`/private/tmp/tests-823-serve-relay-independent-evidence/`. No repository edit,
Cargo/compiler/test execution, Git, network, escalation or device operation was
performed. No speedup or native qualification is claimed.
