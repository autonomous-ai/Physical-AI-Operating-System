# Endpoint fixture cleanup

Closes #860 when native qualification passes. This is fixture-only work
following the #783 sweep and #823 full-run evidence. It changes no production
code, deadlines, assertions, workflow or test selection.

Full ARM64 run [38737583302](https://github.com/autonomous-ai/lampOS/actions/runs/38737583302)
reported `LEAK` for the still-loading and unconfirmed-exit endpoint tests.
Both assertion sets passed, but their fixtures could retain inherited stderr
after the test finished. The original result remains in
[the full-run evidence](https://github.com/autonomous-ai/lampOS/blob/15bc18384/lampOS/docs/verification/dispatch-arm64-full-20261015/README.md).

The unconfirmed-exit test keeps its 1,500 ms worker grace and the coordinator's
500 ms teardown deadline. It checks the original Pending/unreaped failure and
refusal to restart, then waits for the existing supervisor's actual reap
receipt. A five-second host guard fails if teardown does not finish. The
still-loading helper now executes its same six-second sleeper directly, so
the supervised child owns its inherited streams. Its four-second scenario,
no-Ready behavior and 1,200 ms silence-fallback assertion remain unchanged.

The guard also closes any endpoint it still owns and waits on unwind. A new
real-child regression reaches Ready/Pending, panics before handoff and inside
the receiving helper, catches the same typed panic, and requires Reaped with
a zero wait, one spawn and one kill. A cleanup failure during another panic
is printed without replacing the original failure.

`source-audit.json` reverses only the named helper/import/test additions,
fixture handoffs/final drops and loading command/comment. It restores the
original source byte for byte: all 18 original identities and 111 assertion
macros remain; one identity and six assertions are added. The implementation
hash is unchanged through the final rebase.

## Local validation

All times are captured with `date`. This is x86_64 macOS evidence, not ARM64
or a lamp result. No failed test was retried.

At base `71a15e75de4345afb6c1937e15ff5a223c7bfb53`:

```sh
cargo fmt --all --check
cargo clippy --locked --offline -p lamp-voice -p lamp-memory -p lamp-voice-eval --all-targets -- -D warnings
cargo test --locked --offline --release -p lamp-voice -p lamp-memory -p lamp-voice-eval --no-fail-fast -- --test-threads=1
cargo build --locked --offline --workspace
```

All stages passed. The complete package/dependent test logs contain **1,767
passed, zero failures, 13 existing ignored measurements**. Every original
endpoint-session test and the new panic regression ran.

The first focused nextest expression selected the two existing cases because
the new regression's name was initially misspelled in the filter. Both passed
without LEAK: loading 4.579 s; unconfirmed exit 1.652 s. The correctly named
new regression was then selected alone and passed in 3.168 s. This is a filter
correction, not a retry of a failing test; both raw invocations remain here.

After rebasing onto `a3dba0c0af457774e5fe7e09af42828c311738b9`, the changed
source and doc hashes are identical. Formatting, the same strict clippy
command and workspace build passed again. The exact three-case nextest
expression passed all three in **9.382 s**, with zero LEAKs or retries:
loading 4.546 s, panic cleanup 3.082 s, unconfirmed exit 1.693 s. Its 1,144
filtered entries are other Voice unit tests. The complete earlier package
gate was retained rather than repeated after unchanged fixture source.

```sh
cargo nextest run --locked --offline --profile default -p lamp-voice --lib --test-threads 1 --retries 0 --no-fail-fast --status-level pass --color never -E 'test(=coordinator::tests::endpoint_session::unconfirmed_endpoint_exit_fails_mute_cleanup_and_never_reopens_service) | test(=coordinator::tests::endpoint_session::a_worker_still_loading_leaves_the_baseline_window) | test(=coordinator::tests::endpoint_session::endpoint_fixture_reaps_on_panic_before_and_after_handoff)'
```

All runs used `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0` and the shared target
`/private/tmp/lampos-tests-capacity/lampOS/target`. Raw logs, exit codes and
timestamps are included. `gate.json` distinguishes the full gate's base from
the rebased checks. Native qualification is pending: the normal PR profile
excludes this module, so a full run must show these cases actually passing
without LEAK before the fix is marked ready.

## Rebase after the owner’s stderr fix

Recorded 2026-10-16 01:37:52 +0700, from `date`. The preceding full native run was blocked by unchanged Tasks stderr loss (#876), recorded on the PR; its new panic-cleanup test was unrun. The owner’s actual relay/producer fix #881 is now merged. This change rebases onto `587a361a373bd856bb790c8cf92b92f4f7211066`, preserving the complete endpoint source and fixture documentation byte for byte. [Source proof](after-relay-source-preservation.json).

Fresh formatting, strict all-target clippy for Voice, Memory and Evaluator, and workspace build pass. All three affected real-child cases pass in serial nextest with no retries or leaks, **9.030 s**: loading 4.425 s, panic cleanup 3.055 s, unconfirmed exit 1.544 s. [Commands, date boundaries and exit codes](after-relay-root-gates.json); raw logs are adjacent. The original full 1,767-test gate remains historical evidence; this is a new integration gate on changed upstream source, not an unchanged-head retry. The new normal and full ARM64 runs are required before ready.
