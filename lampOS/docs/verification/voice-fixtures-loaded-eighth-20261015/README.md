# The voice's tests on a loaded Mac, eighth, 15 October (#739, part 1)

Ten full serial `lamp-voice` runs of the privacy part of #739 (the fourth's
`ddb13741b`, rebased), beside a build load, and three runs of `lamp-voice`
unchanged for comparison. Taken by the eighth builder on its person's Mac.
**Not a lamp, and not native ARM64**: an Apple M-series Mac (arm64, 15 cores,
48 GB), macOS, Rust 1.96.0. The fourth's runs were on an Intel Mac.

## How

- Each run: from `lampOS/`, `cargo test -p lamp-voice -- --test-threads=1`,
  one after another, nothing between them; the output of each in
  `run-<n>.txt`, the start, end, exit code, load average (1, 5, 15 min) and
  counts in `summary.txt`. The counts in `summary.txt` add every `test result`
  line in the output, so a test that runs itself in a child process counts
  twice when it fails (run 8: one failure, counted as two).
- Load beside them, started before run 1 and kept until after run 10: a
  native release build of the workspace at `-j 8` and a Docker linux/arm64
  release build of the workspace (2 CPUs, 2 GB), each cleaned and repeated
  until stopped. The Docker build ran out of memory at `tract_core` in most
  cycles and started again; it still kept its 2 CPUs busy. The load average,
  sampled every 30 s, is in `loadavg.txt`.
- During runs 4 to 10 the eighth also ran the head-qualification tests alone
  in a loop in another checkout (part 2 of #739), which added to the load.
- Paths in the raw output are shortened to `<lampOS>`, `<scratch>` and `~`;
  nothing else is edited.
- `cargo` stops after the first test binary that fails, so a failed run
  reports the lib suite only.

## What came out

Unchanged `lamp-voice` (`lamp-eighth` at `9778f4680`: `lamp-voice` `08e94288e`
with the eighth's log only), runs in `base/`: 1 of 3 failed. Run 1: `endpoint_session::missing_model_session_ends_on_the_unchanged_silence_fallback`,
`physical privacy closed or lost freshness`, and
`motor::tests::native_head::native_head_six_explicit_actions_prepare_profile_then_new_requests`.
Runs 2 and 3 passed (1,145 tests). During these three the Docker build did
not rebuild after its first cycle (its clean could not remove a mounted
folder); the native build ran throughout (load average 14 to 15).

The privacy part (`eighth/voice-fixtures` at `cea36e12b`: the fourth's
change rebased onto `lamp-voice` `a125eb3da`), ten runs, 13:31 to 14:39 +07.
The pull request has the same change rebased again, without conflict, onto
`686a49c09`; the runs were not repeated on that head.

| Run | Load (1 min) at end | Result |
|---|---|---|
| 1 | 36.72 | failed: `native_head_four_explicit_actions_use_original_owner_and_reap` |
| 2 | 15.39 | passed, 1,181 tests |
| 3 | 11.89 | passed, 1,181 tests |
| 4 | 16.96 | passed, 1,181 tests |
| 5 | 17.79 | failed: `native_head_absence_of_explicit_next_action_never_schedules_return` |
| 6 | 19.45 | passed, 1,181 tests |
| 7 | 16.38 | failed: `native_head_replayed_receipt_and_changed_camera_grant_are_terminal`, `native_head_six_explicit_actions_prepare_profile_then_new_requests` |
| 8 | 29.57 | failed: `kept_out_reminder_is_not_journalled_by_days_but_next_exchange_is` |
| 9 | 21.94 | passed, 1,181 tests |
| 10 | 18.76 | passed, 1,181 tests |

**No privacy-freshness failure in ten runs**: every coordinator test that
failed on 14 October and in run 1 of the unchanged code passed in all ten.
What failed was:

- the head-qualification motor fixtures (`native_head_*`), in runs 1, 5 and 7:
  part 2 of #739, on its own pull request;
- once (run 8), `remember::tests::kept_out_reminder_is_not_journalled_by_days_but_next_exchange_is`
  (on `lamp-voice` since 12:35), `WouldBlock` reading its socket. On macOS an
  accepted stream keeps its listener's nonblocking mode, which Linux does not;
  the commit before this record (`4860bc10f`) sets it blocking, as three
  other voice tests already do.

## What was not shown

- Ten clean full runs: not achieved. Six passed (runs 2, 3, 4, 6, 9 and 10,
  each 1,181 passed, 0 failed, 8 ignored) and four failed, for the two reasons
  above.
- The `remember` change under load: it was made after the ten runs; it passes
  its 28 tests unloaded.
- A lamp, and native ARM64 under this load: the pull request's ARM64 run is
  separate.
- Whether production's 50 ms privacy bound holds on a hot, loaded lamp
  (the issue's "most likely wrong"): host tests cannot say.
