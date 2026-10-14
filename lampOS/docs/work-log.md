# The second builder's log

The second builder's own record, on `lamp-second`, which only it writes
to. The tasks and their briefs are in `work-split.md` on `lamp-voice`.
Dates are the lamp's (Asia/Ho_Chi_Minh). The second builder works on an
Apple M2 Max laptop (macOS) and a Linux ARM64 virtual machine on that
laptop (Colima, 6 virtual cores, Debian bookworm, Rust 1.96.0). Neither is
the lamp, and nothing here was run on a lamp.

## Taken

| # | Task | Branch | Taken | State |
|---|---|---|---|---|
| 1 | The end-of-turn model in a tenth of a second | `second/turn-end-fast` | 2026-10-14 | being built |
| 2 | Try to break last night's changes to the voice | `second/voice-review` | 2026-10-14 | on its own branch, 2026-10-14 (`1d737f8e5`) |
| 3 | A listener for "stop" under the lamp's own voice | `second/stop-word` | 2026-10-14 | being built |

## Handed back

Newest first.

### Task 2: try to break last night's changes to the voice (2026-10-14)

On its own branch only, as the task says: `second/voice-review`, one
commit `1d737f8e5` on `e93a828d1`. It adds tests and
`docs/voice-review-2026-10-14.md`; the one line outside a test file is
`mod review_2026_10_14;` at the end of the worker's test module in
`crates/voice/src/provider_worker.rs`. Nothing was fixed.

**Faults, each shown by a test that fails today, most serious first:**

1. **Speaks when it should not.** The note sent while someone is speaking
   ("your own ears caught your name, Lamp, just now") stays with a request
   that is then given up (judged a sound, or the microphone closed): no
   `activityEnd` is written, so the next request, by anyone and however
   much later, reaches the model with the note in front of it. Fails both
   with `kept-start` and without.
   `crates/gemini/tests/review_2026_10_14.rs`:
   `review_a_note_for_a_request_given_up_is_not_carried_into_the_next_request`.
2. **Speaks when it should not.** A name heard after a request's sound has
   ended, but before its first output, lets that request's held answer
   out (a busy room: "Hey man…" just after someone else's line).
   `crates/voice/src/provider_worker/tests/review_2026_10_14.rs`:
   `review_a_name_heard_after_a_requests_sound_ended_does_not_let_out_its_answer`.
3. **Speaks when it should not.** The second look's own written turn
   counts as one of a new session's two protected requests, so the next
   line of other people's talk is not held.
   `crates/voice/tests/review_2026_10_14.rs`:
   `review_the_lamps_own_second_look_does_not_use_up_a_new_sessions_held_requests`.
4. **Silent to a caller, narrow.** Before a request begins, the second
   look keeps only the newest hearing, so a later faint one (0.91)
   replaces a plain one (1.0).
   `review_a_plain_hearing_just_before_a_request_is_not_lost_to_a_fainter_one_after_it`
   (same file).
5. **No effect on behaviour.** `crates/name/src/said.rs` can publish a
   hearing scored above 1, which `NameHeard::check` refuses.
   `crates/name/tests/review_2026_10_14.rs`:
   `review_a_hearing_is_never_said_with_a_score_over_one`.

**Seen by reading, not shown by a test** (the second look is switched on
only from the environment, so no test can turn it on without a seam that
was not added): an answer that opens with a spoken "..." ends as
`nothing_said` and never reaches `second_look.finished`, so that call gets
no second look; the second look counts a hearing until the turn is over and
can bring a held answer back out; it can go to a renewed session that
never heard the call; `lamp-name` dates a hearing 0.8 s early after a gap;
the worker ignores a failure to subscribe to `name/heard` without a word.

**Found sound, with six passing tests:** `try_note` cannot end the link;
the second look is taken once and does not loop; the note goes out once
per request; a name counts only for its own request. No path was found
that answers twice or loses the link.

**Tested on** an Apple M2 Max laptop (macOS, Rust 1.96.0), from `lampOS/`:
`cargo fmt --all --check` clean; `cargo clippy -p lamp-voice -p
lamp-gemini -p lamp-name --all-targets -- -D warnings` clean; `cargo build
--workspace` builds; `cargo test -p lamp-gemini` and `-p lamp-name` fail
only on their review test; `cargo test -p lamp-voice`: library 726 passed
and 5 failed, `tests/review_2026_10_14` 3 passed and 2 failed. Of the
library's 5, one is the review test; the other 4 are older
`coordinator::motor` tests that time out after 2 s when the whole library
runs at once on this laptop (21 of them in another full run, and the same
21 on `origin/lamp-voice` without the review line); one at a time, all 46
in those groups pass. Nothing was run on the lamp.

**For the first builder:** the five failing tests are the work list, in
that order; each passes once its fault is mended. Most likely to matter on
the lamp: the first, because a "Hey Lamp" said alone and judged a sound is
an everyday event there.

## Questions

None yet.
