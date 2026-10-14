# Task 33: buffered replay output and retained timeout

The fresh source-only copy of `1ce4f987303013631fefd3b0f1b1fe64767144f6`
built the workspace on Third's Intel i7-10700K development Mac, x86_64 macOS
26.6.2 / Darwin 25.6, Rust 1.96.0. All 33 local manifests resolved inside
the copy. Its serial Controls/Msgs tests stopped with **320 passed, one
failed, zero ignored in 17 completed harnesses**. Later harnesses were not
reached. The complete raw output and source-copy metadata are retained here.

The failed check was
`output_is_private_jsonl_and_existing_evidence_is_never_overwritten`: its
child exceeded the unchanged five-second deadline while writing `--output`.
The retained file contained 406 complete lines and part of the next one,
214,122 bytes, with empty stdout/stderr. `incomplete-output.jsonl` is that
unaltered partial file; `failure-files.json` identifies every retained file.
The input/configuration hashes identify the checked-in authored fixtures.
No lamp, sensor or live bus was accessed.

Inspection found direct unbuffered file writes in the new touch replay path.
The file is now wrapped in an 8 KiB `BufWriter`; the replay's existing final
flush must succeed before the success summary. Creation remains private and
exclusive. The existing test now also requires file bytes and summary to
equal stdout bytes and summary. Its deadline, permission assertions and
overwrite rejection were kept. The legacy path is unchanged.

A subsequent unbuffered invocation completed in 1.854922 seconds; one buffered
invocation completed in 0.178619 seconds with exactly the same 905,228 bytes
and 1,050 lines. Both commands and output identity are retained. These are
single observations under different concurrent host workloads, not a controlled
benchmark, a guaranteed speedup, or lamp latency. They do not establish why
the original process exceeded five seconds.

The focused CLI/replay rerun passed **26 checks, zero failed/ignored** across
two harnesses with the original deadline. Formatting, strict all-target
Clippy for the twenty dependent packages, and strict ARM64 Linux cross-target
Clippy for Controls/Msgs also passed. Cross-target Clippy executed no ARM64
binary. `buffered-source.json` identifies the corrected source bytes;
`focused-results.json`, `gate-preflight.json` and `linux-clippy.json` record
commands, durations, source checks and machine. Full final tests, a fresh
source-only copy and exact-head native CI are recorded separately on
`lamp-third`; these preflight checks do not replace them.

The earlier full host run was interrupted after 1,324 passing, zero failing
and six inherited ignored checks in 94 completed harnesses when the standalone
failure required a source fix. The corrected run was then interrupted before
any test harness finished so it could restart after fetching upstream runtime
changes in Eyes, Policy, Records and Voice. Their raw partial streams and
reasons are retained in `interrupted-*.txt` and `interrupted-gates.json`.
Neither is a complete passing gate. The obsolete native run **38571800855**
on `1ce4f9873` was cancelled for the same source correction; its pre-cancellation
identity is retained. Only the final task head's completed green native run
can make the task ready.
