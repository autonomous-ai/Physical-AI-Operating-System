# Memory and Tasks integration qualification

Ready for lamp-voice: no

Task 31 was rebased from `6573ce6c1b594bf9c8cf7836bbd0e162c09e4cd1` to
`1337e7e1d8b88e102ec9fd12f544841569856f75`. The [rebase record](rebase.json)
preserves all 699 task paths: 696 unchanged, with only the lockfile, Voice
thread and tool dispatcher incorporating upstream changes. The resulting
tree matched the preceding read-only merge preview. The [incoming review](review.md)
found no concrete Task 31 behavior conflict and identified the new Memory
reverse dependency and a missing combined Days/reminder test.

The added test
`remember::tests::kept_out_reminder_is_not_journalled_by_days_but_next_exchange_is`
uses an isolated process with exact `LAMP_MEMORY_DAYS=on`, the actual Voice
queue and a bounded local socket peer. A kept-out reminder is followed by an
ordinary exchange and a FIFO end barrier. Only the ordinary exchange may
reach `append_linked`, with the snapshot's provenance; the end submission
contains only that exchange's returned ID. There is no timed absence check,
production deadline change, live provider or lamp operation.

The [frozen source map](source-frozen.json) identifies 700 Rust, manifest and
toolchain paths, SHA-256
`89adaafcd44a51bbd84233abc575dfdc55987499fcc3f1d427825d4829b885fd`.
It records the rebased pre-amend HEAD `f959aaa5e8b43b9ddea8d0031d0034272dcaa7af`
and the test/document changes still dirty at inventory time; its exact file
hashes identify the source actually executed.

On Third's Intel i7-10700K x86_64 Mac, macOS 26.6.2 / Darwin 25.6.0,
64 GiB RAM, Rust/Cargo 1.96.0, all ten [focused stages](focused.json) passed:
formatting; strict all-target Clippy for Actions, Schedule, Voice, Voice Eval
and Memory; seven Voice test filters; and the workspace build. The filters
total **136 test executions / 0 failed / 2 ignored** across seven harnesses:
reminder 75/0/2, tools 18/0/0, thread 8/0/0, remember 24/0/0, agent tasks
5/0/0, task news 5/0/0, and the exact cleared-worker Days environment test
1/0/0. The new Days test is included in both reminder and remember filters;
these are execution counts, not unique test counts. All 700 source hashes
stayed unchanged. Exact commands, command durations and original raw output
are retained by [the index](retention.json).

The initial runner invocation was made before the source inventory finished
and failed to open its map. It started no Cargo command and created no result
report. [That prelaunch error](prelaunch-error.json) is retained separately;
the executed stages above began only after the inventory completed. Nothing
was retried after a test failure.

The complete affected-package gate, independent fresh-source comparison and
exact-head native ARM64 qualification are pending at this checkpoint. Their
terminal records go to [Third's qualification log](https://github.com/autonomous-ai/lampOS/blob/lamp-third/lampOS/docs/verification/schedule-voice-qualification-20261015/README.md)
without changing this frozen runtime merely to add later results. Earlier
failed gates remain failed; First's #709 exception is not a passing gate.

## Not shown

No lamp, real provider, production Memory extractor or physical audio was
used. Command durations measure launch through exit/log closure on this Mac,
not lamp interaction latency. No native or complete-host pass for this source
is claimed before its terminal record exists.
