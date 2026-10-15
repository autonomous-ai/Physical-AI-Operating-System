# Slow transport driver source review

Reviewed the root-owned driver in
`/private/tmp/lampos-tests-slow-transport-progress`; initial snapshot was taken
at `2026-10-16 01:51:12+0700`, and the final source audit at
`2026-10-16 01:57:15+0700`, both from `date`.

No remaining source blocker was found in this slice. This is a source review,
not runtime qualification. One cumulative startup watchdog gap was reported
before the snapshot and fixed by root: repeated ordinary wait calls had been
able to supply a new 30-second budget while the 400-ms logical delay advanced.
`transport_until` now latches and intersects the first startup deadline. New
rounds and later caller deadlines cannot extend it; a shorter preparation
watchdog still wins. It clears only after completed relay/runtime bootstrap
polls (`serve_fixture_driver.rs:579,657`).

## Inspected boundaries

- `controlled_transport` (`serve_fixture_driver.rs:819`) recognizes only the
  explicit same-evaluator wrapper shape. It preserves the original delay
  string and all original relay/fake runtime arguments, checks delay/domain
  overflow before changing config, and delegates the direct case to the
  unchanged `controlled_command`. The old `controlled_command` and
  `controlled_recorder` bodies are byte-identical to the qualified recorder
  source. Configuration remains atomic when validation fails.
- `transport_step` (`serve_fixture_driver.rs:411`) requires the first held
  EPOCH and the complete configured due time. The bounded protocol separately
  enforces immutable start/due, exact role/round, terminal iff due, zero input
  targets and no signal target. This Ack represents only elapsed delay.
- `wait_slow_transport` (`serve_fixture_driver.rs:465`) caps each advance by
  2 ms, the caller's logical bound, full delay due and any earlier recorder
  deadline. The recorder round runs before the transport round at that instant.
  At delay completion the driver holds time and accepts a fresh ServeRelay
  Hello. It does not infer successful exec from the delay receipt. A zero or
  1-ms delay still requires that new Hello before any advance toward the
  original EPOCH+2-ms bootstrap. Relay polls and runtime registration then
  begin at max(full delay due, original bootstrap).
- Recorder terminal progress pauses further clock advancement and startup
  polling until `recorder_exit_observed` is supplied by the existing backend.
  The driver may hold a newly accepted relay while waiting, but sends it no
  Step. That notification remains separate from the Recorder Ack; actual
  backend `Child::try_wait`/cleanup remains in `mac.rs:1536–1590`.
- `bootstrap_at` (`serve_fixture_driver.rs:544`) retains the original relay,
  runtime, player-pump and relay order. Direct bootstrap still uses
  EPOCH+2 ms. After delayed bootstrap, the next ordinary runtime poll remains
  2 ms later; no runtime poll is synthesized for time before runtime startup.
- Every wait error marks the fixture failed and closes all control peers and
  the listener (`serve_fixture_driver.rs:665`). Explicit close also closes the
  player controller before peers. Product process cleanup still belongs to
  the backend, and this module does not kill or manufacture exit statuses.

## Startup Ping and preparation preservation

The driver never reads or writes stdin/stdout. The command adapter drops the
completed delay Participant and returns into the unchanged real `CommandExt::exec`
body. Pending input remains in the actual inherited pipe. The backend's real
first Ping, 250,000-us interval (`mac.rs:772,1759,2110`) and successful-write
frame count are unchanged. Its SessionStart-based exclusion of setup Pings
(`mac.rs:2161`) is unchanged. A 400-ms delayed start therefore still has the
first two setup inputs pending; the new scheduler test verifies their announced
counter is passed intact, but does not claim to perform those pipe writes.

The original `await_ready_fixture` path (`mac.rs:2879`) consumes transport on
both sides of each 2-ms preparation wait and retains its original host deadline.
The controlled prepared player starts its full SlowPrepare only after observing
the real room resource; its 2300-ms delay, original 100-ms cue-freshness bound,
startup cue assertions and actual grant/datagram paths were not edited. All 23
integration source files, including the full slow-transport test body in
`tests/serve_session.rs:648`, remain identical to the snapshot for this work.
The constructor migration and actual child integration run remain root work.

## Evidence and limits

The reviewed root driver SHA-256 before the new test declaration is
`ad3ec60cb40ba8d3828d35378b3a10e502246df7e19c0d7f4decbfe2047fc396`.
The only worker edit to that file appends the new test-module declaration;
removing exactly those bytes recovers the snapshot. `audit_source.py` also
proves the original inline driver tests and the recorder tests unchanged.
`driver-vs-recorder.diff` shows the entire root implementation delta.

No Cargo, test binary, Git, network, device or escalation operation was run.
Socket/exec availability, compilation, actual child readiness, 2300-ms
preparation behavior and the final 100-ms cue assertions still require the
parent's integrated gate. No speedup or OS-exit claim follows from these mocks.
