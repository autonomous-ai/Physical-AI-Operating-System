# The reply-buffer capacity test on a paused clock (Intel Mac, 2026-10-16)

For #823. Not a lamp result: `lamp-voice`'s unit-test binary on the tests
builder's Mac (16 logical CPUs, other builders' work running). `base` is
`origin/lamp-voice` at `42c4dec84` (with #1035); `paused` is this change.
`runs.txt` is unedited: for each run of
`provider_worker::tests::twenty_second_answer_through_a_five_second_reply_buffer_follows_capacity --exact --nocapture`,
its `MEASURED` values and its result. Its last line, `[exited with code 0]`,
was added by the session tool that captured the script's output when the
script ended; nothing was cut after it.

- base, three runs one at a time: 24.04 to 24.10 s.
- paused, five runs one at a time: 2.83 to 3.75 s.
- paused, six at once, two rounds (`load-r*`): 5.57 to 7.12 s.

Every paused run printed the same values: hold 4,000 ms, playback finished
24,004 ms, completion 20,004 ms, starved 0 ms, written in 4,200 ms, 500
packets, peak 5.00 s. Base: 4,000 to 4,001, 24,024 to 24,028, 20,003 to
20,007, 0 to 1, 4,326 to 4,350 ms, 500, 5.00 s.

The gate on the change: `cargo fmt --all --check` clean; `cargo clippy -p
lamp-voice --all-targets -- -D warnings` clean; `cargo build --workspace`
ok; `cargo test -p lamp-voice --no-fail-fast`: lib 1,370 passed, 24 failed,
10 ignored; every integration test binary passed. The 24 are
`coordinator::motor::tests` (19), `process::ring_startup_tests` (4) and
`coordinator::optional_ring::tests` (1); run again one at a time they
passed 24 of 24.
