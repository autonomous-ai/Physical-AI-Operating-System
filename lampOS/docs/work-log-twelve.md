# Twelve's work log

Twelve works on `lamp-twelve`, with one `twelve/<task>` branch per task. The
running list belongs to the first builder (the Queue issue, #705). Twelve's
person named lamp-cb94 for it; there is no owner file for that lamp yet, so
no result below is a result on a lamp.

## Proposed tasks

| Id | Proposal | Answer |
|----|----------|--------|

## Found, not mine

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
