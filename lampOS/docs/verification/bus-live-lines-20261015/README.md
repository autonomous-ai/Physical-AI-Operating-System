# Bus live recording observation (#846, #783)

Recorded 2026-10-15 19:52:52 +0700, from shell `date`. Source before: `61d1ea6fc795853427bea09ce8c9f45cf50abfd6`; x86_64 macOS, Rust 1.96.0. No lamp or device was used.

## Failure and boundary

[ARM64 run 38728246092, attempt 1](https://github.com/autonomous-ai/lampOS/actions/runs/38728246092/attempts/1), on #795 head `862fec7d0319a7a8bab14ad1ec54b63afd6ec35e`, failed `a_killed_recorder_leaves_whole_lines_and_its_socket_goes_with_the_next_publish` with `half a line` at `crates/bus/tests/cli.rs:152`. It ran 566 of 4,579 selected tests: 565 passed and one failed; 4,013 were unrun after fail-fast, with 117 existing profile skips. [Unedited excerpt](original-arm64-excerpt.log). Later attempts on that run are not evidence of a fix; this work requested no retry.

The strict `lines()` parser was being called inside live observation loops, before the recorder was killed. A file snapshot can end during `write_all`, before its final LF is visible. The new `written_lines()` observation helper reports no completed snapshot until the entire observed byte sequence ends at LF. The four affected polling predicates keep their exact expected counts and the existing 20-second `until` bound. It does not return a completed prefix while a later record is partial.

The original strict parser is unchanged. Every final full-file LF/JSON assertion, exact record count, payload, sequence, size limit, replay and stale-socket assertion remains, as do SIGKILL/reap and publisher order. A complete but malformed record still fails final validation. The change adds no sleep, retry, truncation, ignored test or production behavior.

## Preservation and regression

All **190 original assertions** and **10 original CLI identities** remain. Reversing only the new helper/regression block and four polling predicates restores the complete original file byte-for-byte: [source audit](source-audit.json). One new regression pins absent/empty snapshots, a JSON value missing LF, a complete record followed by a partial second, and final partial or malformed records still being rejected. It uses direct fixture writes, so these boundaries do not depend on scheduler luck.

## Local gate

All checks passed once on the final source, without retries:

- `cargo fmt --all --check`: passed.
- `cargo clippy --locked --offline -p lamp-bus --all-targets -- -D warnings`: passed.
- `cargo test --locked --offline -p lamp-bus --no-fail-fast -- --test-threads=1`: **66 passed**, zero failures or ignored tests, including all 11 CLI tests and two doctests.
- `cargo build --locked --offline --workspace`: passed.

Commands ran from `lampOS/` with `CARGO_BUILD_JOBS=2` and an owned shared build cache at `/private/tmp/lampos-tests-dispatch/lampOS/target`. [Gate boundaries and exit codes](gates.json); raw [fmt](fmt.log), [clippy](clippy.log), [tests](tests.log), [build](build.log). Native ARM64 qualification is recorded on the pull request for its current head. This fixture fix does not establish arbitrary crash-write atomicity or resolve the separate normal-exit relay bug #837; #783's broader qualification remains with the eighth.
