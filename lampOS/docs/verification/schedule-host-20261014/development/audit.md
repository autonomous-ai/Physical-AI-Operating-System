# Schedule independent source audit

Reviewed on the Third development host, 2026-10-14, against task 6 in
`lampOS/docs/work-split.md`. This is source inspection of the in-progress
Schedule implementation, not an executed regression result or a lamp claim.
Line references below describe the source read during this audit and may move
when the parent formats or fixes it. No repository files, Git state, services,
or devices were changed. No tests were run.

## Ranked findings

### 1. High: synchronous request logging defeats the client and shutdown bounds

`lampOS/crates/schedule/src/runtime.rs:280` calls `eprintln!` after obtaining
the engine reply but before `write_line` sends it. That write to stderr has no
deadline. If the process's stderr is an undrained/full pipe, the client worker
can remain there indefinitely. The 1,200 ms deadline only bounds socket I/O,
not this intervening write. At shutdown, `runtime.rs:386` unconditionally
joins every client worker, so the store owner's new bounded shutdown does not
resolve this independent hang.

Concrete reproduction to add: spawn the real process with piped stderr and do
not drain that pipe; issue enough valid `status` requests to fill it. Then
assert that another request gets a reply or closes within its declared bound
and that SIGTERM exits within the documented shutdown budget. The current
request path can stop before either condition. Remove the per-request
synchronous log and use the existing counters/diagnostic path, or use a
bounded logger which drops under backpressure. This report does not claim
that a particular lamp supervisor currently stalls stderr.

### 2. Medium: a corrupt stored receipt can replay an incorrect success

`lampOS/crates/schedule/src/engine.rs:136` validates the stored receipt's
guard identity, a few envelope values and a 16 KiB size cap. It does not
validate the response against the request's operation, item or delivery, or
require the response revision to be exactly the guard revision plus one.
`engine.rs:309` subsequently returns this arbitrary `serde_json::Value`
verbatim when the exact request is retried.

Concrete example: create timer 1, persist its successful Set response, then
change only `last_receipt.response.id` to 2 in the serialized snapshot. Keep
the actual items and envelope unchanged. The current `State::check` accepts
this snapshot, but an exact lost-reply retry reports successful creation of
timer 2, which does not exist. Missing acknowledgement fields, a false
`acknowledged` flag, or a mismatched delivery similarly escape the receipt
check. This needs corrupt state; it is not a demonstrated normal-path commit
failure or an authorization boundary against the same Unix user.

Suggested regression: mutate only response ID, delivery, per-operation
success fields, required field presence, or receipt revision in an otherwise
valid persisted snapshot; opening must reject it without overwriting it.
Validate a typed per-operation receipt and the invariants still recoverable
from the last mutation/current state. Normal automatic due promotion may
increase the *current* revision while preserving the original receipt, so
do not require the receipt revision to equal the current state revision.

## Other paths checked

- Pending detection is saved before exposure (`engine.rs:213`), and an
  ambiguous persistence error poisons the live owner (`engine.rs:198`).
  Readiness suppresses the notice after owner failure/staleness
  (`runtime.rs:350`). These paths do not intentionally consume reminders.
- Exact most recent request retries return the saved receipt before the CAS
  check; later unrelated mutations make an older guard conflict. Automatic
  due updates preserve that receipt. I found no normal-path trace which
  creates a second timer from the same guarded retry.
- Ack/Snooze require a pending matching delivery; snooze increments the token
  and preserves the original recurrence anchor. Ack advances one recurrence
  only. Thus long downtime deliberately retains every missed daily/weekly
  occurrence; it is not coalescing. The existing test source explicitly
  covers 26 retained occurrences. Voice's pacing/backlog presentation is
  still First's integration boundary.
- Read/write loops use absolute cumulative deadlines; one request/reply per
  connection, fixed workers and queues, and an 8,192-byte reply fallback are
  present. A reply exceeding the page bound becomes `too_long`, rather than
  an unbounded write. Extend the maximum-page test to 64-byte request IDs and
  maximum valid numeric metadata: its current short `rN` IDs do not cover
  the complete allowed envelope. This is a test suggestion, not a claimed
  observed overflow.
- Add exact lost-Ack and lost-Snooze retry tests after restart and automatic
  promotion of the next delivery. They should replay the old durable receipt
  without consuming or postponing the newer occurrence. Existing source
  chiefly demonstrates lost-Set recovery.
- CLI default-off exits before constructing the clock or starting the
  service (`main.rs:36`). The enabled private socket has an ownership lock
  and inode-checked cleanup. Reminder text remains in private socket/storage
  data; the published due message and routine diagnostic fields omit it.

The previously identified MONOTONIC-versus-BOOTTIME stamp mismatch,
blocking stale-socket connect, unbounded storage-owner join, and thread-spawn
cleanup are parent-owned fixes and are not repeated as new findings here.
The nonblocking socket probe, distinct stamp clock and stop-on-drop guard
were already visible in the later source read.

## Not shown

No audit regression was executed. The store/time owners are running their
own focused checks; their results are not this audit's measurements. This
review does not prove power-loss durability on the lamp filesystem, Linux
socket timing, NTP accuracy, Voice delivery/completion semantics, scheduler
responsiveness under target load, or hard bounds for arbitrary blocked
filesystem/bus operations.
