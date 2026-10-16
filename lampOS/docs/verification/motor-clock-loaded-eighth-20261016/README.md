# The head-qualification clock under load, eighth, 2026-10-16

Part 2 of #739 (`eighth/motor-clock` at `8816b6e66`, in [head](head)),
qualified by the eighth builder on a 20-core Linux x86_64 laptop, Rust
1.96.0, 00:30 to 02:30 +07. This replaces the Mac runs of 2026-10-15 as
part 2's evidence.

**How.** [qual.sh.txt](qual.sh.txt) is the script that took these readings.
Load: release builds of the whole workspace, in a target directory of their
own, at `-j 20`, cleaned and repeated. The 1-minute load average, sampled
every 30 s, is in [loadavg.txt](loadavg.txt): 5 at the start, median 24.6,
at most 60.8. Then:

1. `native_head` alone, 60 runs of the voice lib test binary
   ([native-ramp/](native-ramp/), 00:31 to 00:32): **60 of 60 passed**. The
   load was still rising (5 to 16), so the 60 runs were repeated under full
   load.
2. `native_head` alone, 60 more runs ([native-loaded/](native-loaded/),
   01:42:52 to 01:43:26, load 23.7 to 27.8): **60 of 60 passed.** All 11
   `native_head` tests passed in every run.
3. Full serial runs, `cargo test -p lamp-voice -- --test-threads=1`
   ([full-1.txt](full-1.txt) to [full-8.txt](full-8.txt)). The script ran 8
   of the planned 10 before its 2-hour limit stopped it during run 8. In
   every run, all 11 `native_head` tests passed (88 of 88). The lib failed
   3, 14, 11, 15, 21, 16 and 22 tests in runs 1 to 7, all in other families
   on the host clock: #823's inventory and #893, the tests filed from these
   runs. So the first builder's "full serial run 10 of 10" cannot be met at
   this load until those are on a test clock. cargo stops at the failed lib,
   so the other test binaries were not run.
4. [summary.txt](summary.txt) has one line per run.

The reminders test the seventh gave #739
(`completed_playout_survives_both_real_store_reopens_then_acks_without_respeech`)
passed in all 8 full runs. It also passed 100 runs alone under this load
and 200 runs, 20 at a time, under fsync load; its failure is not reproduced
here.

**Edited, only this:** paths under the person's home directory are shown as
`~`, `<scratch>` or `<worktree>`, and the host name as `<laptop>`, since
they name a person. Nothing else is changed.

Not shown: any run on a lamp (the change is to tests and to the
Supervisor's clock parameter, with `SystemClock` everywhere in production),
and the Mac.
