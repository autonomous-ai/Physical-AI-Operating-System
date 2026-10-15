# The policy program test, whole lines only, under load: eighth, 2026-10-16

#783's test, `crates/policy/tests/program.rs`,
`the_program_judges_what_the_voice_hears_and_prints_nothing_of_it`: before,
on unchanged `lamp-voice` ([before-head](before-head)), and after, with the
change. [after-head](after-head) names the commit before the change, since
the change was still uncommitted when the script read it; what the after
runs ran (that tree plus the change's `program.rs`, blob `ad28dfab`, as
committed in `2228d173e` and `c5a6f2fa8`, and both binaries' SHA-256) is in
[after-source.txt](after-source.txt). A 20-core Linux x86_64
laptop, Rust 1.96.0, 02:41 to 02:54 +07. [qual.sh.txt](qual.sh.txt) is the
script. The load was release builds of the workspace at `-j 20`; the
1-minute load average is in [loadavg.txt](loadavg.txt) (21 samples,
median 16.37, at most 20.74).

1. The test alone, before and after taking turns, 60 runs each
   ([summary.txt](summary.txt)): **before 60 of 60 passed, after 60 of 60
   passed.**
2. Ten full serial runs of `lamp-policy` on the after head, `--no-fail-fast
   --test-threads=1` ([full-1.txt](full-1.txt) to
   [full-10.txt](full-10.txt)): **10 of 10 passed**, every suite.
3. The test alone, 300 runs each side, 10 at a time
   ([parallel-summary.txt](parallel-summary.txt), load about 10): before 0
   of 300 failed, after 0 of 300 failed.

**What this does and does not show.** The failure #783 names (ARM64 runners,
2026-10-15) was not reproduced here, before or after, so these runs show
that the change breaks nothing. They do not show that it mends anything.
The change closes the boundary the tests builder found on #783 (19:00): a
last line still being written could match its wanted start before the rest
of it arrived, and the program was then killed. Its evidence is the reading,
not a failure seen.

**Edited, only this:** paths under the person's home directory are shown as
`~`, `<scratch>` or `<worktree>`, since they name a person. The 120 single
runs and the 600 parallel runs are counted in the summaries; their own
outputs are not kept.
