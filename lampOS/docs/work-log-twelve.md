# Twelve's work log

Twelve works on `lamp-twelve`, with one `twelve/<task>` branch per task. The
running list belongs to the first builder (the Queue issue, #705). Twelve's
person named lamp-cb94 for it; there is no owner file for that lamp yet, so
no result below is a result on a lamp.

## Proposed tasks

| Id | Proposal | Answer |
|----|----------|--------|

## Found, not mine

- 2026-10-16, `reminders::transport::tests::in_process_test_helper_transfers_and_releases_the_real_claim`
  fails at `reminders/transport.rs:588` (the claim is still locked after both
  ends drop) in 2 of 20 runs of `cargo test -p lamp-voice --lib reminder` on
  base `bc68dec87`, on an idle Intel Mac. Not mended.

## 2026-10-16 — joined, and #920 taken

Ready for lamp-voice: no

- Role file `lampOS/builders/twelve.md` made from the template, on
  `lamp-twelve` from `origin/lamp-voice` at `61c7cada7`. No `builder:twelve`
  label exists; twelve does not make one (the owner's yes is needed).
- Twelve's person asked it to take #920 (unlabelled, untaken): the
  lamp-actions bus test reads back a state with a 20 ms life, which a loaded
  runner outlives. Taken with `status:taken` before the first builder has
  taken this role file onto `lamp-voice`, at the person's request; the work
  is on `twelve/920-light-stale-window`.
- lamp-cb94: named by the person, not touched. #920 needs no lamp.

## 2026-10-16 — #920, the light's bus test window

Ready for lamp-voice: yes (#981)

- Kind: off unless set. Test-only, inside `#[cfg(test)]` of `lamp-actions`
  (`light::tests::through_the_bus_a_request_is_kept_and_a_stale_state_is_no_light`):
  the ring's expiring state lives 500 ms, not 20 ms, and the stale read waits
  600 ms. No code a lamp runs changes. Commit `103625ddf` on
  `twelve/920-light-stale-window`.
- Gate on macOS x86_64 (Intel i9), from `lampOS/`: `cargo fmt --all --check`
  clean; clippy `-D warnings` clean for `lamp-actions` and `lamp-voice`;
  `cargo test -p lamp-actions` 34 passed, 0 failed; `cargo build --workspace`
  ok; `cargo test -p lamp-voice` 1234 passed, 40 failed, 8 ignored: the timed
  process tests of #739/#893 on a loaded host (22 in
  `coordinator::motor::tests`), one of them passes run alone; not run again.
- ARM64 run 38847348555, green on `103625ddf`.
- Not shown: the old test never failed here (20/20 with every core busy,
  30/30 with 200 busy processes), so the failure was seen only on the ARM64
  runner. No lamp: none was needed.

## 2026-10-16 — the `builder:twelve` label

Ready for lamp-voice: no

- At its person's request ("đánh dấu builder:twelve luôn đi"), twelve made the
  label `builder:twelve` and put it on #920, and told the first builder on
  #705 (comment 6207819404), which may correct or remove it.

## 2026-10-16 — #920 released

Ready for lamp-voice: no

- Ten, which opened #920, closed it at 11:2x (its person: problems go to the
  first builder first, not into issues). At twelve's person's request twelve
  released it: `status:taken` and `builder:twelve` removed, #981 closed, and
  the branch `twelve/920-light-stale-window` deleted at its person's request
  (the fix was commit `103625ddf`, one test in `crates/actions/src/light.rs`:
  a 500 ms life and a 600 ms wait in place of 20 ms and 60 ms).

## 2026-10-16 — #935 taken

Ready for lamp-voice: no

- The first builder gave twelve #935 on #705 at 11:22 (moved from the
  eighth): the coordinator reminder round trip's `AckConfirmed` is missing
  after one Ack request. Find the cause and make the test deterministic from
  it, as the seventh did for #893 (stepped clock, no retries, no longer
  timeouts). One pull request into `lamp-voice`. Taken at 11:2x; work on
  `twelve/935-reminder-ack` from `origin/lamp-voice`.

## 2026-10-16 — #935, the reminder Ack on a busy disk

Ready for lamp-voice: yes (#1009)

- Kind: off unless set. Test-only: `request_deadline()` in
  `reminders/consumer.rs` is `Instant::now() + REQUEST_BUDGET` as before
  outside a `cfg(test)` scenario scope. Commit `0485ae63f` on
  `twelve/935-reminder-ack`.
- Cause, the fixture: one tick's 250 ms socket budget also spans two
  `fullfsync` commits (Ack plan, Schedule's Ack). Idle Intel Mac ~160 ms of
  250; busy disk 318–340 ms for the plan alone. Production recovers next tick.
- Fix: the two real-Store tests (`consumer/tests.rs:644`, the four
  `reminder_roundtrip` tests) run on the stepped scenario clock. Every
  assertion kept, no retries. Doc: `docs/voice-test-fixtures.md`.
- Shown, Intel Mac, four `F_FULLFSYNC` writers: base 0/10, change 10/10.
  Evidence `docs/verification/935-reminder-ack-mac-20261016/`.
- Gate: fmt clean; clippy `-p lamp-voice` clean; `cargo test -p lamp-voice`
  1,274 passed, 33 failed (the #739/#893 timed tests and the claim test
  above, none #935's), 8 ignored; workspace build ok. ARM64 run 38852786535
  green.
- Not shown: the line-338 failure itself (its boundary, line 336, was). No
  lamp.
