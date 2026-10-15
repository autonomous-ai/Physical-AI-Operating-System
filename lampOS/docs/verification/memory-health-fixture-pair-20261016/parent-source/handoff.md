# Memory parent fixture helper handoff

Recorded 2026-10-16 02:48:06+0700 from `date`. Source implementation only: no Cargo/build/test execution, Git, network or device operations were performed. Root owns integration and gates. Formatting was applied directly to only the two assigned new files with rustfmt 1.96.0, edition 2024, skip_children=true.

## Exact public API

The helper is `lampOS/crates/memory/tests/support/health_control.rs`. It includes the child owner's exact `src/fixture_protocol.rs` via `#[path = "../../src/fixture_protocol.rs"] pub mod protocol`; it does not duplicate any wire serde structure.

```rust
pub fn start(command: &mut Command, private_root: &Path, log_path: &Path)
    -> io::Result<(Child, Controller)>;
pub struct Controller { /* private fields */ }
pub struct Window { /* private fields */ }
impl Controller {
    pub fn cadence_now(&self) -> Instant;
    pub fn window(&self) -> io::Result<Window>;
    pub fn settle(&mut self, window: &Window) -> io::Result<()>;
    pub fn next_due(&mut self, window: &Window) -> io::Result<Instant>;
    pub fn advance_to(&mut self, target: Instant, window: &Window) -> io::Result<()>;
    pub fn wait_cadence(&mut self, duration: Duration) -> io::Result<()>;
}
```

`Controller` and its windows use one explicit local `Rc<()>` identity. This is intentionally a parent-thread helper, compatible with root's later explicit `Rc<RefCell<Option<Controller>>>` Scratch/Reports/Process integration; there is no process registry or TLS lookup. Window ownership, initial elapsed microseconds, through-time (start + full 8 seconds), and absolute host deadline are private and immutable through its API. Each operation rejects another controller's window, regression, sub-microsecond rounding, overflow or exceeding either bound.

`cadence_now()` simply returns the last committed `Instant`. `next_due()` first settles zero if no reporter boundary has been admitted. `advance_to()` settles zero if needed, then splits a target at every original `REPORT_EVERY` due instant; equality-time calls produce a fresh framing round but require unchanged cycle, attempt, outcome and deadline state. `wait_cadence()` opens one eight-second window for its entire requested duration and delegates to that splitting path; a zero duration settles without advancing.

## Startup and ownership

The parent validates the provided absolute directory is actual mode 0700, opens the existing regular stderr file, binds `health-control.sock` before spawning, and explicitly chmods it to 0600. It never removes an unrelated pre-existing socket to bind successfully. The per-child nonce is ASCII alphanumeric `P<parent PID>C<child PID>`, within the 32-byte schema bound, and belongs to the private socket lifetime.

Only stdin is changed to `Stdio::piped()`. Application argv, environment, stdout and caller-configured regular-file stderr are untouched. One bounded shared Bootstrap frame is written via nonblocking ChildStdin and rustix fd polling under the same absolute startup eight-second deadline, then stdin closes. The listener accepts under that deadline; the shared Channel validates the complete Connected version/nonce before returning. The listener path is then removed. The returned Connected is only control attachment: `checked` remains absent, and real readiness plus an initial positive WaitChecked still belong to the integration caller.

An owned `StartupChild` guard kills and actually waits/reaps on every partial startup error; successful startup returns the actual Child separately to the original Process owner. No receipt is treated as OS exit. The cleanup mirrors the existing Process::Drop behavior. Root/cross-review were explicitly notified that blocking kernel `Child::wait` after kill and local regular-file syscalls are not preempted by the protocol eight-second watchdog; an unconditional hard return bound plus guaranteed actual reap would need a retained child/reaper ownership contract on timeout. No such owner or invented exit receipt was added.

## Receipt and actual-file commit

The shared Checked schema is validated against the exact nonce/round/time/marker of the outstanding Step. Initial progress must be cycle 1, started at zero, due at the original one-second period, and attempts 0 only for NotAttempted or 1 for ReturnedOk/ReturnedErr. A same-time or pre-due round keeps all prior cycle fields unchanged. An exact-due round completes exactly one cycle and adds exactly one attempt if its actual outcome says attempted. The resulting wait is always positive. Checked contains no report/event payload and no child status.

Only after a complete exact marker line is read from the real regular-file stderr does the parent commit time, round and progress. The reader checks original device/inode and rejects replacement/truncation; its line memory is capped at the shared 128-byte marker bound and it discards oversized non-marker lines through their LF. Partial lines and suffixes after a found marker remain tracked correctly. Wrong nonce/round, substrings and missing LF do not match. Actual file visibility uses bounded 1 ms polling capped by the operation's unchanged absolute host deadline, with no new logical time advancement. EINTR is retried only under that same deadline.

Every operation error permanently latches a diagnostic and shuts the Channel. A missing marker after the child has advanced cannot roll the controller back into a reusable state: pure cadence reads hold the prior admitted instant, but every subsequent operation fails. Error reporting in later integration must use a raw log reader, not recursively call settle.

## New boundary tests and evidence limits

`tests/health_control.rs` includes the same helper source to inspect private failure state without a scenario-facing test hook. It defines 23 new test identities and 93 static assertion sites. Tests cover pure reads, initial worker-boundary distinction, equal-time rounds, every due split, exact initial/pre-due/due counters, wrong Connected/Checked identities, marker framing and incomplete lines, file replacement/truncation, missing marker after child advance, fixed logical/host windows, wrong-window ownership, arithmetic/round overflow, closed-peer queued frame handling, malformed/oversized/truncated frames, bounded missing peer, bounded bootstrap writer, and private startup paths.

The mock peers use real Unix sockets and are closed before joined cleanup on assertion unwind. They supply explicit mock receipts and write marker bytes to a real temporary file; those are protocol and parent-scheduling evidence, not actual Reporter/Bus publication evidence. The child owner's separate `actual_marker_follows_prior_bytes_through_the_real_log_relay` test covers the real relay path.

Two tests use actual owned child processes. A shell blocked on its owned stdin is killed/reaped and its actual SIGKILL ExitStatus checked. A specifically selected copy of this test executable reads precisely one Bootstrap and actual stdin EOF, records the full received argv, writes a real stderr marker, and returns a mock checked boundary; its parent retains the actual Child and separately observes/reaps its successful OS exit. Its child-local test mode is confined to that test executable, not a Memory application environment switch. It is not an actual Memory scenario claim. No fake Diagnostic/Event, synthetic ready file, timeout shortening of existing cases, retrying a failed test, changed old assertion, or speedup claim is present.

## Preservation and remaining integration

`source-audit.json` proves both owned paths are new and all 30 unchanged pre-existing files retain their exact initial bytes. The two changes recorded outside our ownership are the child author's then-in-progress shared protocol/schedule files, explicitly listed with before/after hashes. All original Memory scenario and existing test bodies, production application, Bus/Reporter and Cargo were untouched by this task. In particular, `tests/health.rs` stays `31add261e22dda578ca5f5c0b9016a1382f7a1e3d87f6968a1d083ec722e394d`.

Root still owns the explicit controller-slot wiring, five selected real-child migrations, unchanged host-only cases, API warning review under strict clippy, all compilation/runtime gates, exact scenario/assertion proof and paired timing. This handoff is not runtime qualification.

## Stable SHA-256

- `lampOS/crates/memory/tests/support/health_control.rs`: `a06cb3e241ac901f5729eefe89abedf96cf5c8f7eb580bfcc715540ee67f97db`
- `lampOS/crates/memory/tests/health_control.rs`: `e90c17dd703d4d9879732d4b2ead58d725d9e7f712f53fd2e56bd8198c771325`
- Shared child protocol read for compatibility: `21f2d5ca2fc309c8b63989ff6a889f37edd5e3eb8a4fc5de305348d11a0efb0c`
