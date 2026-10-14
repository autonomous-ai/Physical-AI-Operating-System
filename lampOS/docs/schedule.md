# Schedule: timers and reminders that survive a restart

## What exists and what we take

`lamp-schedule` owns a private, durable list of timers and reminders. It resolves
times, retains due occurrences, and serves guarded local requests. It does not
speak. Actions and Voice have not been wired to it. The executable does nothing
unless started with `--enable`.

The product references were reviewed on 2026-10-14. Apple's
[HomePod guide](https://support.apple.com/en-nz/guide/homepod/apd82db5cb0d/homepod)
and Google's [timer guide](https://support.google.com/googlehome/answer/7028899?hl=en)
establish familiar named duration timers and remaining-time questions. Google's
[voice Tasks guide](https://support.google.com/googlehome/answer/16722329)
provides examples of tomorrow and weekly reminders. Amazon's
[reminder guide](https://digprjsurvey.amazon.com/csad/help/node/GVMW5E8YCNR6AC6E)
explicitly describes snoozing and retrieving missed reminders. We take those
plain user requests, separate cancellation from postponement, and keep missed
work visible. These guides do not establish how those assistants handle power
loss, DST ambiguity, or exactly-once audible delivery. Their pause/resume,
household sharing, and follow-up features are not implemented here.

The V1 review covers `system/schedule/spec.go`, its time and timezone tests, and
[GO-3.19](v1-verdicts.md). We retain strictly future initial calendar selection
and calendar recurrence. We do not copy V1's one-minute polling, five-minute
interval floor, device jitter, 30-minute catch-up cutoff, or global timezone
re-anchoring. The current service retains each missed recurrence until an
explicit receipt advances it. V1's distinction between handing off work and
someone hearing it remains essential.

[Jiff](https://docs.rs/jiff/0.2.38/jiff/) supplies civil calendars and bundled
IANA rules; its
[disambiguation API](https://docs.rs/jiff/0.2.38/jiff/tz/enum.Disambiguation.html)
supplies the explicit gap/fold policies below. `rusqlite` uses bundled SQLite;
transactions use `synchronous=FULL`, DELETE journals, and an additional
directory sync. SQLite's [atomic commit](https://www.sqlite.org/atomiccommit.html)
and [synchronization documentation](https://www.sqlite.org/pragma.html#pragma_synchronous)
explain the storage assumptions. A successful sync still depends on the
filesystem, operating system, and storage honoring it.

Implementation: [time.rs](../crates/schedule/src/time.rs),
[engine.rs](../crates/schedule/src/engine.rs),
[store.rs](../crates/schedule/src/store.rs),
[protocol.rs](../crates/schedule/src/protocol.rs),
[runtime.rs](../crates/schedule/src/runtime.rs), and
[the message contract](../crates/msgs/src/schedule.rs).

## Starting the owner

The disabled command returns an `enabled:false` JSON line and exits without
opening a store, socket, bus publisher, or worker:

```sh
lamp-schedule serve
```

An enabled Linux example is below. These are example paths for a service user,
not an installed unit. First must choose the actual user and provision the
existing parent directories. All paths supplied here are absolute.

```sh
lamp-schedule serve --enable \
  --dir /home/lamp/.local/state/lampOS/schedule \
  --socket /home/lamp/.local/state/lampOS/schedule-socket/call.sock \
  --bus-root /run/user/1000/lamp-bus \
  --zone Asia/Ho_Chi_Minh
```

`--dir`, `--socket`, and `--zone` are required when enabled. `--bus-root` defaults
to `/run/lampos-bus` if omitted; supplying it explicitly
makes the intended bus clear. The store and socket each need an existing parent;
the service can create their final private directory. The socket path is at
most 103 bytes. The store directory must be owned by the effective service user
and exactly mode `0700`; its regular files must be mode `0600`, singly linked,
and owned by that user. The socket parent must be private and owned by the same
user; the socket is mode `0600`. A live socket or another directory owner is
refused. This is a local service, with no HTTP endpoint or network listener.

On Linux, the default boot identity comes from
`/proc/sys/kernel/random/boot_id`. Wall time is considered trusted when
`/run/systemd/timesync/synchronized` is a file, or when the operator explicitly
passes `--trust-clock`. This is evidence compatible with the existing Health
boundary, not proof of NTP authenticity or clock accuracy. Removing that marker
withdraws automatic trust on the next sample. The service never sets the clock.

Other development hosts require `--boot-id`. A host-only example, using fresh
test directories, is:

```sh
lamp-schedule serve --enable \
  --dir /tmp/lamp-schedule-host-state \
  --socket /tmp/lamp-schedule-host-socket/call.sock \
  --bus-root /tmp/lamp-schedule-host-bus \
  --zone Asia/Ho_Chi_Minh \
  --boot-id schedule-host-20261014 --trust-clock --run-for-ms 10000
```

An explicit boot identity must stay the same across process restarts on one
boot and change after a real reboot. It is 1–128 ASCII letters, digits, `_`,
`-`, or `.`. `--trust-clock` is the operator's assertion that wall time is usable;
it does not synchronize it. `--run-for-ms` accepts 1–86,400,000. Duplicate and
unknown options are refused. SIGINT and SIGTERM request shutdown; they do not
delete timers.

## What is committed

`<dir>/schedule.sqlite3` contains one `state` table with a singleton BLOB, under
SQLite `user_version=1`. That BLOB is the engine's JSON `schema:1`: persistent
`store_id`, revision, next item ID, active items, and the most recent mutation's
request and receipt. The store ID is 32 lowercase hexadecimal characters.
Reminder text and the most recent request are private plaintext in this store;
they are not encrypted. File permissions do not defend against another hostile
process with the same UID.

When provisioning, First must register this private data directory in the
personal-data reset manifest and arrange owner shutdown before its removal.
The store contains personal reminder text. No deployment, reset manifest, or
reset-owner code is changed by this task.

A new database is prepared as `.schedule.sqlite3.init`, synced, and published
without replacing an existing database. An interrupted, unpublished empty
initialization can be recovered. An existing zero-byte database, corrupt
database, unexpected schema/version, invalid engine snapshot, unsafe file, or
WAL/SHM sidecar is refused rather than replaced with a blank schedule. A valid
newly initialized schema with no snapshot receives its first engine state.
Checked rollback journals can be recovered by SQLite. Do not remove a database
or its journal to make a startup failure disappear; preserve them for recovery.

There is one storage owner, protected by a lifetime advisory lock. Each change
is prepared on a copy, committed with `synchronous=FULL`, and followed by a
directory sync before a successful response or new due notice is exposed.
`fullfsync` is requested where supported. Database pages are capped at 16 MiB;
the snapshot is 1–1,048,576 bytes. The active list is capped at 128, including
scheduled, pending, and waiting-for-clock items. A full store refuses another
item; it does not evict an old or overdue one. Cancellation or completed
one-off delivery frees capacity. IDs are increasing and are not reused.

A failed write or sync can occur after the transaction reached disk. The owner
stops accepting work after such a failure. Restart and inspect the committed
state before deciding whether to retry; neither a transport timeout nor a sync
error proves that the mutation did not happen. Files changed or replaced while
owned stop further storage work. There is no automatic migration, remote
backup, factory-reset action, or expiry-based deletion in this component.

## Time rules

The owner supplies `unix_ms`, `boot_ms`, `boot_id`, and `wall_trusted` to the pure
calendar engine. The model supplies a duration or a civil-time request. It must
not calculate an epoch timestamp from a conversation's opening time.

| Request | Meaning |
|---|---|
| `{"kind":"after","seconds":480}` | Eight elapsed minutes, from the current owner sample. Durations are 1–31,536,000 whole seconds. |
| `{"kind":"at","time":"09:00"}` | The next strictly future 09:00 in the configured zone. Omitted `day` and `day:"next"` mean the same thing. |
| `{"kind":"at","time":"09:00","day":"today"}` | Today at 09:00; past or exactly current time is refused. |
| `{"kind":"at","time":"09:00","day":"tomorrow"}` | The next local calendar date, not now plus 24 hours. |
| `{"kind":"at","time":"09:00","day":"2028-02-29","zone":"Asia/Ho_Chi_Minh"}` | An exact valid calendar date and explicit IANA zone. |
| `{"kind":"daily","time":"09:00"}` | Every local date at 09:00, initially strictly after now. |
| `{"kind":"weekly","weekday":"monday","time":"09:00"}` | Every Monday at 09:00, initially strictly after now. Weekdays are full lowercase English names. |

Times are exact ASCII `HH:MM` on a 24-hour clock. `9:00`, `09:0`, AM/PM strings,
seconds, unknown fields, and invented relative-day words are refused. Dates are
exact `YYYY-MM-DD`. A zone is a name in the bundled IANA database, such as
`Asia/Ho_Chi_Minh`, `America/New_York`, or `UTC`; a numeric offset is not a zone.
Omitting the zone selects the configured zone for that new request. Changing
the service's default later does not move existing reminders.

Every new timer, reminder, and snooze requires trusted wall time, including an
elapsed timer: it needs a durable wall deadline for a later boot. On the same
boot, an elapsed timer uses its stored boot deadline even if wall time moves or
becomes untrusted. Linux uses `CLOCK_BOOTTIME`, so suspend counts toward elapsed
timers. Development hosts use `CLOCK_MONOTONIC`; suspend behavior on those hosts
is not a Linux qualification. Bus stamps separately use shared
`CLOCK_MONOTONIC` microseconds and the host boot identity, matching other lamp
topics. Do not compare a BOOTTIME deadline directly with a bus stamp.

After reboot, an elapsed timer falls back to its persisted Unix deadline. With
untrusted wall time it waits, rather than guessing how much time passed.
That fallback was calculated when the timer was created; a later wall-clock
correction during the original boot does not rewrite it. The same-boot elapsed
deadline stays correct, but the post-reboot estimate can inherit that earlier
wall-clock error. Qualifying clock trust and reboot recovery is part of the
lamp handoff.
Calendar reminders require trusted wall time even on the original boot. A
pending occurrence stays pending across clock corrections and trust loss; it
does not become future work again. A changed boot identity or backward boot
clock within a running owner is refused and requires a restart.

Jiff 0.2.38 with bundled timezone data is used directly; the calendar module
does not consult `/etc/localtime`, the environment, or a runtime clock. One-off
nonexistent local times are refused. Ambiguous one-off times default to
rejection; `fold:"earlier"` or `fold:"later"` chooses an instant explicitly.
For example, New York's `2026-11-01 01:30` needs that choice. An already-past
gap or wholly past fold can be passed when finding the next day's occurrence;
an upcoming ambiguity is not silently resolved.

Daily and weekly reminders deliberately use Jiff's `Compatible` policy:
shift forward through a gap and choose the earlier instant in a fold. The
requested date and `HH:MM` remain in `repeat`, so a shifted 02:30 occurrence
does not turn the entire series into 03:30. Advancement is from the previous
requested civil occurrence, not from delivery time. A whole-day zone jump can
map two distinct civil dates to the same instant, as in Apia in 2011; both
occurrences are retained, with different delivery tokens. Advancement is
receipt-driven, so equal instants do not create a timer loop.

Supported Unix milliseconds are 0 through **253,402,207,200,999**, inclusive:
1970-01-01T00:00:00Z through 9999-12-30T22:00:00.999Z. Jiff reserves its maximum
legal zone offset near year 9999. This runtime bound is narrower than the
message schema's end-of-year-9999 bound. Arithmetic, boot deadlines, and signed
remaining-time conversion are checked for overflow.

Persisted deadlines are checked against the bundled rules on reopen. A future
software update that changes a rule affecting a stored occurrence can therefore
fail validation. There is no implicit timezone-database migration or silent
re-anchoring; that upgrade case needs explicit recovery design and testing.

## Due, late, snoozed, and completed

At due time the owner commits a pending occurrence. It remains there until
cancelled, snoozed, or acknowledged. With Voice absent, disabled, interrupted,
or busy, nothing consumes it. The oldest pending occurrence is selected by
effective due instant, then item ID.

There is at most one pending cursor per active series. The pending count is
therefore not the total number of missed recurrence dates. One acknowledgement
advances one civil occurrence, and the next scan may make the following date
pending. No historical queue is allocated and no intervening date is skipped.
First can say the original date and let the person cancel the series, rather
than presenting this count as all the reminders missed during an outage.

A previously scheduled item found already overdue on reopening is marked late,
even if only one millisecond overdue. Ordinary polling lateness is reported in
`late_by_ms`; the `late` flag is set once it exceeds 1,000 ms. Where the current
clock cannot establish lateness, it is unknown rather than zero. First's spoken
wording should use the returned late flag and current receipt, for example
“This reminder is late: take the laundry out,” rather than inventing how long
the lamp was offline.

Snooze applies only to the current pending `(id, delivery)`. It increments the
delivery token, clears pending, and adds a new elapsed deadline. The original
recurrence anchor remains unchanged. Acknowledging the snoozed occurrence
advances from that original anchor. If the following recurrence is already
past, it becomes pending on the next scan. Missed days are not dropped or
automatically marked delivered. The final Voice policy must pace a backlog;
Schedule has no permission to burst speech into the room.

Acknowledging a one-off removes it. Acknowledging a repeat advances exactly one
requested occurrence and increments its delivery token. Cancellation removes
the whole item, including its recurrence. There is no separate “delete just
one recurrence,” edit, pause, resume, monthly, yearly, or interval operation.

## Private socket requests

Open a Unix stream connection, send one UTF-8 JSON object followed by a newline,
read one JSON line, and close. There is no authentication token; access comes
from the private socket's filesystem permissions. Unknown operations or fields
are rejected. Read operations need no guard:

```json
{"op":"status"}
{"op":"get","id":1}
{"op":"list","after_id":0,"limit":8,"pending_only":false}
```

Each line above represents a separate connection. `status` returns the current
store identity/revision, item and pending counts, waiting-for-clock count,
default zone, wall trust, capacity, and `last_receipt`. The latter is either
`null` or `{"request_id":"...","op":"set","response":{...}}`; its response
is the original committed result, and may carry an older revision than current
status. It does not include the original reminder text.

`get` returns an `item` with its `id`, current `delivery`, text, creating
`request_id`, state (`scheduled`, `pending`, or `waiting_for_clock`), effective
and original due instants, canonical local time, recurrence, snoozed flag,
remaining milliseconds, and lateness. `list` returns the same item views and
`next_after_id`. Use that cursor until it is null. A page contains at most eight
items and may contain fewer to fit the 8,192-byte reply budget. A list is not a
cross-request snapshot: concurrent changes can change revisions between pages.
Remaining time is clamped to zero once due, and is null when the required clock
comparison is unavailable.

For a manual probe, an existing `nc` client that supports Unix sockets can send
one status line. This is an operator convenience, not a runtime dependency:

```sh
printf '%s\n' '{"op":"status"}' | \
  nc -w 2 -U /home/lamp/.local/state/lampOS/schedule-socket/call.sock
```

Every mutation needs a `Guard` containing the current `store_id`, its current
`revision`, and a caller-generated `request_id`. The ID is 1–64 ASCII letters,
digits, `_`, `-`, `.`, or `:`. The Actions/Voice adapter supplies this guard; the
model never sees or invents it. The following are independent shape examples,
not a sequence to replay with the illustrated revision:

```json
{"op":"set","guard":{"store_id":"0123456789abcdef0123456789abcdef","revision":7,"request_id":"request-42"},"text":"Check the pasta","when":{"kind":"after","seconds":480}}
{"op":"set","guard":{"store_id":"0123456789abcdef0123456789abcdef","revision":7,"request_id":"request-43"},"text":"Water the plant","when":{"kind":"at","time":"09:00","day":"tomorrow"}}
{"op":"cancel","guard":{"store_id":"0123456789abcdef0123456789abcdef","revision":7,"request_id":"request-44"},"id":1}
{"op":"snooze","guard":{"store_id":"0123456789abcdef0123456789abcdef","revision":7,"request_id":"request-45"},"id":1,"delivery":2,"seconds":600}
{"op":"ack","guard":{"store_id":"0123456789abcdef0123456789abcdef","revision":7,"request_id":"playout-46"},"id":1,"delivery":2}
```

Successful mutations return `ok:true`, `store_id`, and the committed revision,
plus the item/delivery and operation result. Creation and snooze also return
the resolved `due_unix_ms` and canonical `local` value. Text must contain
1–300 UTF-8 bytes, contain no control characters, and not be whitespace only.
The byte bound is checked before surrounding whitespace is trimmed. Unicode is
preserved and oversized text is not truncated. Confirm the resolved local time
and recurrence to the person only after a successful durable response.

The most recent mutation's exact request and response survive a lost socket
reply, process restart, and automatic due scans. Repeating that same request
with the same guard and arguments returns its original receipt without another
mutation. This is one retained receipt, not an unlimited request-ID history.
After another mutation replaces it, an old compare-and-set revision conflicts.
An automatic due scan can also advance the revision between a status read and
a new mutation. Read status and the relevant items before choosing further
work; do not turn an uncertain request into a second command by silently
retrying it with a new guard. Use `status.last_receipt.request_id` and the
creating `request_id` in list/get to inspect what is known. A lost older receipt
is not reconstructed from a new request ID.

Refusals have `ok:false`, an `error` code, and a static `message`. Important
codes include `bad_request`, `bad_guard`, `conflict`, `bad_text`, `bad_time`,
`not_found`, `stale_delivery`, `full`, `clock`, and `storage`. The transport adds
`busy`, `unavailable`, `timeout`, `uncertain`, `incomplete`, and `too_long`.
`uncertain` means no receipt arrived in time and the operation may have committed.
A full connection queue can close a connection without a JSON refusal. A closed
connection is not evidence that a started mutation was undone.

## Bounds and responsiveness

| Boundary | Implemented limit |
|---|---|
| Request and response JSON | 8,192 bytes each, excluding the newline |
| Text | 300 UTF-8 bytes, no control characters |
| Active items | 128, with no eviction |
| List page | At most 8 items and within the response byte bound |
| Socket clients | 4 workers plus 4 queued connections |
| Storage-owner request queue | 16 entries |
| Complete request read | 300 ms from connection acceptance, including queued time |
| Owner response wait | 1,100 ms from connection acceptance |
| Entire client read/write budget | 1,200 ms from connection acceptance |
| Idle owner due scan | 50 ms tick; requests also run a due scan |
| Observer considers owner stale | After 1,500 ms without an updated owner report |

Queued work that has expired before the owner starts it is refused. Once a
storage call starts, a socket deadline cannot cancel its transaction or bound
`fsync`. Client readers and the main bus observer run separately from the one
storage worker. A silent or trickling client cannot hold that owner while it
reads a request. An actual disk stall can hold due detection and every queued
request; the observer withdraws `ready` and the actionable notice when its last
owner report becomes stale, provided the observer and bus can still progress.
Shutdown likewise cannot force a blocked filesystem call to return.

The interactive targets, not measured lamp guarantees, are: a complete valid
request to durable response within **250 ms** on healthy storage; due detection
within **50 ms plus storage and queue time**; and the main observer beginning
publication within **5 ms after a committed owner report** becomes visible.
The main loop sleeps 5 ms, but its socket and bus operations and host scheduling
also cost time. No platform-independent deadline is claimed for disk sync,
end-to-end audio, or a person hearing the reminder.

## Bus and diagnostics

`schedule/due` is `lamp_msgs/ScheduleDue` version 1, **Latest**, with a 1-second
period and a 3-second lifespan. The owner also publishes on revision and
readiness changes. Its fields are `stamp`, `store_id`, `revision`, `enabled`,
`ready`, pending count, waiting-for-clock count, and optional `oldest` metadata:
`id`, `delivery`, `due_unix_ms`, `late`, and optional `late_by_ms`. There is no
reminder text in this topic. It is a hint to inspect durable work, not a queue
of speech commands. Multiple due items remain in the private store even though
the topic carries only the oldest one.

The consumer must validate the message shape, current host boot, shared
monotonic age, bus publisher identity, and ordering. A restarted publisher's
sequence belongs to its new bus identity. A renewed heartbeat does not create
another delivery. `ready` means the Schedule owner is progressing; it says
nothing about Voice's privacy state or ability to speak. Disabled or unready
metadata offers no actionable `oldest`. If publishing fails, the committed
pending work remains available through the socket and future publications.

`diagnostics` reports item/pending/waiting counts, requests, wall trust, storage
readiness, publication failures, and refused clients. It does not log reminder
text or model arguments. Failure diagnostics are not an acknowledgement. There
is no bus acknowledgement topic: completion is a guarded private `ack` request.

## Proposed model tools: not wired

The following are adapter proposals in the same name/description/parameter
object form as [Tools](tools.md). They are not installed in `tools.json`, not
declared by Voice, and not executed by Actions in this change. First must keep
their declaration and execution behind explicit switches that default off and
qualify the provider's schema support on the actual integration.

```json
[
  {"name":"set_timer","description":"Set a timer only when asked. Use a label and whole seconds; confirm the returned time only after success.","parameters":{"type":"object","properties":{"text":{"type":"string","maxLength":300},"seconds":{"type":"integer","minimum":1,"maximum":31536000}},"required":["text","seconds"],"additionalProperties":false}},
  {"name":"set_reminder","description":"Set one reminder at a local time. Ask for missing or ambiguous intent; never calculate a Unix timestamp.","parameters":{"type":"object","properties":{"text":{"type":"string","maxLength":300},"time":{"type":"string","pattern":"^([01][0-9]|2[0-3]):[0-5][0-9]$"},"day":{"type":"string"},"zone":{"type":"string"},"fold":{"type":"string","enum":["earlier","later","reject"]}},"required":["text","time"],"additionalProperties":false}},
  {"name":"repeat_reminder","description":"Set a daily or weekly local-time reminder only when asked. A weekly reminder needs a weekday.","parameters":{"type":"object","properties":{"text":{"type":"string","maxLength":300},"time":{"type":"string","pattern":"^([01][0-9]|2[0-3]):[0-5][0-9]$"},"every":{"type":"string","enum":["daily","weekly"]},"weekday":{"type":"string","enum":["monday","tuesday","wednesday","thursday","friday","saturday","sunday"]},"zone":{"type":"string"}},"required":["text","time","every"],"additionalProperties":false}},
  {"name":"list_reminders","description":"Read active timers and reminders, including pending ones, before resolving an ambiguous request about an existing item.","parameters":{"type":"object","properties":{"pending_only":{"type":"boolean"}},"additionalProperties":false}},
  {"name":"cancel_reminder","description":"Cancel the selected timer or whole reminder series only when the person asks. Use an ID from the current list.","parameters":{"type":"object","properties":{"id":{"type":"integer","minimum":1}},"required":["id"],"additionalProperties":false}},
  {"name":"snooze_reminder","description":"Postpone the selected currently due occurrence by whole seconds. This keeps a recurring reminder's original schedule.","parameters":{"type":"object","properties":{"id":{"type":"integer","minimum":1},"seconds":{"type":"integer","minimum":1,"maximum":31536000}},"required":["id","seconds"],"additionalProperties":false}}
]
```

The adapter must enforce 00–23 hours, the service's UTF-8 byte limit, valid
day/zone values, and weekday presence only for weekly requests; the schemas
above do not replace service validation. It maps `set_timer` to `After`,
`set_reminder` to `At`, and `repeat_reminder` to `Daily` or `Weekly`. List output
must remain bounded for the model's separate tool-result budget. The adapter
obtains current guards and the current pending delivery for snooze. Neither
guards, delivery acknowledgements, wall-clock trust, nor boot-clock arithmetic
belong in the model's arguments.

Example model arguments are `set_timer {"text":"Check the pasta","seconds":480}`,
`set_reminder {"text":"Water the plant","time":"09:00","day":"tomorrow"}`,
and `repeat_reminder {"text":"Take the bins out","time":"19:00","every":"weekly","weekday":"thursday"}`.
“Snooze that for ten minutes” can become
`snooze_reminder {"id":1,"seconds":600}` only after the adapter identifies which
pending reminder “that” means. The owner resolves the date and returns the
actual local deadline. If Schedule cannot commit, the lamp must say it cannot
set the reminder just now; it must not give a success confirmation anyway.

## First's Voice integration contract

Exactly one logical consumer must own delivery. There is no claim/lease API to
arbitrate multiple independent speakers. That consumer needs durable local
deduplication keyed by `(store_id, id, delivery)` and a stable request identity
for each acknowledgement. Bus sequence is not a delivery identity.

1. Validate a fresh enabled, ready notice, then call private
   `{"op":"get","id":1}` for the offered ID. Compare the reply's top-level
   `store_id` and its `item.id` and `item.delivery` with the offered tuple, and
   require `item.state:"pending"`. Do not add store or delivery fields to the
   `get` request; that operation accepts only `op` and `id`. A retained bus file
   alone is not authority to speak. Use the private text and current late
   metadata.
2. Let final Voice ownership decide whether privacy, interruption, room policy,
   quiet/busy rules, and current input/output readiness permit speech. Schedule
   cannot grant microphone/camera permits or override the current owner. Do not
   copy greeting/“hello” heuristics into reminder eligibility.
3. Persist the consumer's delivery identity and its own in-flight correlation
   before starting playout. Recheck current work before a delayed start. A
   cancellation or snooze invalidates the old occurrence; integration must also
   cancel any matching queued or active audio and fence late callbacks.
4. Only a qualified receipt that the entire intended reminder audio completed
   playout permits `ack` of that exact delivery. A bus send, a provider accepting
   text, generated audio, queue insertion, or the first sound is insufficient.
   Interruption, cancellation, failure, and unknown completion leave it pending
   or require rereading current state; they are not fabricated completion.
5. Persist the completed-playout evidence, obtain a current mutation guard, and
   send one stable acknowledgement request. Resolve a lost acknowledgement
   reply through exact retry and durable receipt inspection. Do not replay the
   sound merely because its acknowledgement reply was lost, and do not let an
   old acknowledgement consume the next recurrence.

There is an unavoidable unproven boundary between sound finishing and durable
completion evidence. A process or power failure there may cause the reminder
to be spoken again. The implementation does not promise exactly-once speech or
that anyone heard it. First must test that window together with real playout
receipts, interruption, process restart, privacy changes, and the chosen
consumer's persistent deduplication. No Voice, Actions, or device deployment
change is included here.

## Verification and handoff

All evidence below is from Third's development host on **2026-10-14**: Intel
Core i7-10700K at 3.80 GHz, 16 logical CPUs, 64 GiB RAM, macOS 26.6.2 / Darwin
25.6.0 x86_64, pinned Rust/Cargo **1.96.0**. The workspace and qualification
scratch parent `/private/tmp` were on the APFS data volume `/dev/disk1s2`,
mounted at `/System/Volumes/Data`; this was not a tmpfs check. See the
[machine record](verification/schedule-host-20261014/machine.json).

The pre-change workspace build passed. The full required gate passed
`cargo fmt --all --check`, strict all-target Clippy and serial tests for all
**15** touched/dependent packages, and `cargo build --workspace`, with Cargo
`--offline --locked`. Tests reported **2,618 passed, zero failed, nine inherited
ignored**. The package list, exact commands, timings, source hashes and
[ignored cases](verification/schedule-host-20261014/ignored.json) are retained
in [the gate record](verification/schedule-host-20261014/results.json).

The first release qualification exposed a Darwin client issue after that gate:
changing a read timeout after the server closed returned `EINVAL` despite
buffered reply bytes. Both failed full runs and a small socket reproduction
are retained in the [failure ledger](verification/schedule-host-20261014/failures.md).
Only the qualification example and its Cargo test/feature declaration changed
thereafter; production Schedule and Msgs source remained identical. The example
now uses nonblocking reads/writes with readiness polling and its cumulative
deadline, with explicit close/fragment/boundary regressions. The
[final supplement](verification/schedule-host-20261014/supplement-results.json)
passed workspace format, all 15 packages' strict all-target Clippy, Schedule's
**129 tests with none failed or ignored**, and the workspace build.
This task adds **141 tests**, none ignored; failures were corrected rather
than removed from the gate.

The source-only copy at
`/private/tmp/lamp-schedule-standalone-8mwn7xax/lampOS` initially contained
978 files, 28 workspace members and 29 local packages including vendored Tough.
Every local dependency resolved inside that copy; its target directory was
separate from the checkout. It built the whole workspace. After refreshing
the two corrected files, that build passed again and Schedule + Msgs reported
**185 passed, zero failed or ignored**. This uses the host's existing
Cargo cache and pinned toolchain, not dependencies copied from V1. Final source
hashes matched after the supplement and measurement.

One successful release invocation of `schedule_check --scratch-parent
/private/tmp --samples 100` records **510 real host timings**: five series
with a first call and 100 warm calls, plus five single observations. The first
call is the first of that operation after its labelled setup, not a whole
process cold start. Set grows the list from one to 101 items; Ack shrinks it.
List measures one page, not a full scan through every item. Warm median is the
average of the two middle samples; table values are rounded to six decimal
places. No other Third-owned build or test ran
during this measurement; unrelated host load was not controlled. Full samples,
boundaries and binary hashes are in the
[qualification output](verification/schedule-host-20261014/qualification.json)
and [measurement record](verification/schedule-host-20261014/measurement.json).

| Operation, milliseconds | First call | Warm median (100) | Warm maximum |
|---|---:|---:|---:|
| Direct durable Set | 75.44885 | 75.299712 | 94.280184 |
| Direct List, first page (up to 8 items) | 0.194719 | 0.139454 | 0.163894 |
| Direct durable Ack | 76.05017 | 75.226196 | 96.577391 |
| Unix durable Set roundtrip | 64.58232 | 81.878153 | 112.466039 |
| Unix Status roundtrip | 5.208467 | 5.959374 | 14.440621 |

| Single observation, milliseconds | Elapsed |
|---|---:|
| fresh Store + Engine open | 218.521855 |
| existing Store + Engine reopen | 0.671332 |
| one durable overdue transition for 101 timers | 75.2287 |
| authored clock jump to bus subscriber receipt | 136.05589 |
| one Status roundtrip beside a silent client | 7.582396 |

Direct durable calls include validation, snapshot serialization, SQLite commit
and directory sync. Unix Set also includes connection, request serialization,
owner queue and response read/parse; fetching its revision guard is outside
that timed call. The bus observation includes the nominal 50 ms scan, pending
commit, publication and receipt, with subscription setup excluded. It can use
the datagram or retained Latest value. The full labelled boundaries are in the
JSON. The client uses cumulative nonblocking read/write deadlines; an OS stall
in its initial local connect is not qualified. The 250 ms response target is
a reference, not an asserted lamp result.

The binary uses an authored trusted UTC epoch and authored boot clock. Its
one-hour clock jump makes 101 ten-minute timers 50 minutes late. It verifies
restart identity, every retained pending item, exact delivery acknowledgements,
an immediate Ack replay, an empty queue after Ack/reopen, every runtime pending
page, and service responsiveness beside one silent connection. These are
functional checks of authored downtime. They do not measure a real one-hour
outage, sound, model latency, clock synchronization or power-loss survival.
The temporary directory was removed after its owners stopped.

**First: build and run.** In the usual ARM64 build environment:

```sh
cargo build --release --locked -p lamp-schedule --bin lamp-schedule --example schedule_check
```

Copy the example binary through the usual review/deployment process. On the
lamp, choose a short absolute scratch parent on persistent storage. Check that
choice with `findmnt -T "$HOME"`; do not use `/tmp` if it is memory-backed. If
the home directory is on persistent storage, the qualification command is:

```sh
./schedule_check --scratch-parent "$HOME" --samples 100
```

It needs no Rust installation, source checkout or fixture files on the lamp.
It creates only its own private `sc-*` child, starts local storage/socket/bus
owners using authored clocks, and removes that child after they stop. It does
not open audio or motors. Expect exit 0, `ok:true`, 101 Set receipts in each
path, 101 verified runtime pending items, and zero pending items after the
direct Ack/reopen check. Keep the raw output and lamp name; a failed check
must remain a failure, not be silently retried into a timing claim.

Then qualify the actual enabled service with Linux BOOTTIME, the lamp's boot
identity and synchronization marker: suspend, restart/reboot, clock trust loss
and correction, overdue recovery, cancellation and repeating backlog. First
owns Voice wiring and must test its privacy/quiet admission, exact token fetch,
interruption, completed-playout acknowledgement, and consumer crash window.
Register the private Schedule directory with personal-data reset before use.
No production deployment is included here.

**Most likely wrong on the lamp:** clock trust and the stored wall-time
fallback after a reboot, storage sync cost or power-loss recovery on its real
flash, and Voice treating a published due notice as completed speech. The
qualification binary deliberately does not substitute for those checks.

## What has not been shown

Linux/ARM64 execution, Rust 1.91 compatibility, actual Orange Pi storage
durability under power loss, Linux suspend/reboot and
clock-synchronization transitions, ARM64 resource use, first and repeated
durable-request latency, due-to-audible latency, late-reminder wording through
the real model, provider tool declarations, final privacy/quiet admission,
interruption and completed-playout receipts, consumer crash deduplication, and
timezone-database upgrade recovery remain unqualified on the lamp. The service
has not been deployed, and no reminder has been claimed as heard from these
host tests.
