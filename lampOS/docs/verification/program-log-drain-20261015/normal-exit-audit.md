# #783 normal-exit stderr audit

Observed at 2026-10-15T11:44:27Z (`date -u`). Read-only inspection of `/private/tmp/tests-783-tests.log` and `/private/tmp/lampos-tests-783`; no edits to repository files, builds, retries or API calls.

**Finding:** the relay's normal-exit `finish()` has a source-visible false-empty window. It can return after bytes leave its input pipe but before the relay has forwarded those bytes or published that it holds them. This is a plausible explanation for the missing health usage tail. The failed run has no scheduling trace proving that exact interleaving occurred.

## What the failed run establishes

- `/private/tmp/tests-783-tests.log:183`–`:187`: `wrong_arguments_print_the_usage_and_end_with_2_for_serve_and_64_for_show` fails for `["serve", "--stale-ms", "99"]`; captured stderr shown in the panic contains only `lamp-health: --stale-ms is a whole number from 100 to 600000` and its newline, without the required usage tail.
- The failure is the unchanged `said.ends_with(usage)` assertion at `lampOS/crates/health/tests/program.rs:722`. The status assertion at `:719` and prefix assertion at `:721` ran successfully first, establishing exit code 2 and the expected error prefix. `printed` at `:237` performs strict UTF-8 conversion without trimming/filtering the bytes.
- This case calls `Command::output()` (`program.rs:693`–`:698`). It does not call the test's kill/drain helper, send SIGKILL, or inspect a live incomplete pipe. Waiting in that helper cannot fix this case.
- The health binary installs a relay guard at `lampOS/crates/health/src/main.rs:288`. The refusal arm uses one `eprintln!("lamp-health: {why}\n{USAGE}")` at `:303`, then returns the supplied exit code. The source does not intentionally omit usage for this input.

One formatting call is not a single end-to-end relay delivery. Even if every formatted byte reaches the internal pipe before the call returns, the relay forwards lines separately. The following race does not require assumptions about whether formatting itself uses one kernel write or several.

## Source ordering and a concrete allowed interleaving

All references here are to `lampOS/crates/ipc/src/log_relay.rs`:

1. `install` duplicates the original stderr (`:48`), creates an internal pipe (`:50`), starts a relay thread (`:66`–`:68`) and redirects stderr to that pipe (`:71`). The shared `holding` counter starts at zero (`:54`).
2. On normal main return, `Guard::drop` calls `finish(500 ms)` (`:103`–`:106`). The 500 ms is a maximum; it is not an unconditional drain delay.
3. `finish` queries unread input-pipe bytes, then returns immediately if the pipe is empty and the shared held-byte counter is zero (`:133`–`:135`).
4. The relay reads bytes out of the pipe at `:160` and copies them into its **local** `held` vector at `:164`. It may write one complete line at a time at `:196`–`:208`.
5. Crucially, it publishes `holding.store(held.len(), Relaxed)` only at `:220`, after that output loop. During the read-to-publication interval, the pipe can be empty and the published count can still be the preceding zero while local bytes remain.

A schedule matching the observed shape is therefore possible:

- Main submits the error and usage records.
- Relay consumes them into its local buffer, leaving the internal pipe empty. Its published count is still zero.
- Relay forwards the error line and is descheduled before forwarding usage; the usage bytes remain in its local buffer.
- Main enters `finish`; it sees `FIONREAD == 0` and the stale published `held == 0`, returns immediately, and completes process exit.
- Process exit terminates the relay thread. The parent drains the external stderr pipe to EOF and receives the error line already forwarded, but the usage bytes died inside the child's buffer.

This is a real gap in the completion criterion independent of the observed run. It is not repaired merely by changing `Relaxed` to `Acquire`/`Release` or `SeqCst`: the required nonzero state has not yet been stored. Moving the counter store immediately after the read still leaves a smaller read-before-store gap. Adding a sleep or requiring a few empty samples only makes the race less likely.

A separate conservative issue is `ioctl_fionread(...).unwrap_or(0)` at `:133`: a query error is treated as proof of emptiness. There is no evidence that an ioctl error occurred in this failure.

## Why this is an owner fix

The normal-exit capture already waits for the child and drains its externally delivered bytes. Bytes lost inside the exiting child cannot be recovered by a test-side delay after `output()`, a larger assertion timeout, a retry, or a weaker usage check. Keep the exit-status, exact usage-tail, stdout and UTF-8 assertions unchanged.

The relay owner should replace the two independently observed counters with a completion protocol that covers **queued input, local/in-flight bytes and output completion together**. Two viable designs to review:

- **Relay-owned flush acknowledgement:** `finish` requests a new drain generation and waits within the existing bound. The relay acknowledges that generation only after checking/draining input after the request and settling all local/in-flight output. The acknowledgement must be issued by the thread that owns the buffer, after the relevant output writes; not synthesized by the caller from separate empty snapshots. A wakeup/control mechanism must preserve bounded waits and nonblocking producers. Bytes submitted before the flush request must be included; future concurrent writers need an explicitly defined boundary.
- **Synchronized in-flight state:** serialize the transition from queued pipe bytes to relay-owned bytes with the caller's empty-state observation. For example, an explicit in-flight state protected by the same lock as the finish-side queue check can cover the entire consume/publish transition. The relay must claim ownership before consuming input and clear ownership only after completion/held-state publication. Avoid holding a producer-path lock across blocked output, and preserve the existing bounded shutdown behavior.

These are design directions, not patches or validated implementations. Preserve the existing 64 KiB bound, nonblocking journal behavior, counted loss under backpressure/breakage, signal-mask behavior and at-most-500 ms guard wait. A writable small-output sink should receive every complete submitted record on normal exit. A blocked/broken sink must still not hang termination.

## Deterministic regression direction

The current relay tests (`log_relay.rs:249`, `:311`) exercise saturation and broken output, close the writer, then join the relay; they do not exercise an early `finish()` return while the writer remains open and the relay owns unpublished bytes. `lampOS/crates/ipc/tests/relay_signals.rs:10` verifies signal routing, not stderr flush completeness.

Add an owner-side regression around isolated relay state (avoid replacing process-wide stderr/`OnceLock` state in a concurrent unit test):

1. Supply two newline-terminated records, an error followed by usage, to the relay's input pipe. Keep the writer open, as normal `Guard::drop` does.
2. Use explicit test synchronization to pause the relay after it has consumed the final bytes, preferably after writing the first record but before writing the second. At that barrier, assert the input is empty, only the first record reached the sink, and the relay still owns the usage bytes.
3. Exercise the completion decision/flush protocol while that state is pinned. A test hook at the finish decision or flush-acknowledgement point should prove it cannot declare completion. Do not use a timed sleep or a racy `try_recv` immediately after spawning the finishing thread as proof that it remains blocked.
4. Release the relay, require the pending drain acknowledgement/finish to complete within its existing bound, and assert the sink contains the exact full byte sequence including every usage line and the final LF. No output filtering or assertion relaxation.
5. Cover the other boundary too: pause immediately after input consumption, before any shared ownership publication. That specifically prevents an apparent fix which only moves the held-byte store a few lines earlier.
6. Retain a process integration test which installs the actual guard, writes a multi-line stderr record and returns normally; assert exact captured bytes and status. The controlled in-flight regression provides determinism; a repeated-run stress test is not a substitute.

Also retain bounded blocked/broken-output tests and treat failed queue inspection as uncertainty/error rather than successful emptiness. No new retries should be added to health's argument test.

The other health failure in the same validation log concerns an expected stale diagnostic versus a board-disk warning. It is separate evidence and is not explained by this normal-exit audit.
