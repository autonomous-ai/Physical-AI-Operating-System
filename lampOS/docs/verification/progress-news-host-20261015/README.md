# Progress news on the development Mac (#732, 2026-10-15)

The second builder's evidence for task 40, `second-P2`: `lamp-tasks serve
--progress` ([tasks.md](../../tasks.md#progress-on-the-bus-serve---progress-732))
and `lamp-policy --progress`
([policy.md](../../policy.md#progress-news-say-this-progress-now-732)). **A
laptop's, not a lamp's.** No lamp, no Harness session, account or real
`ps`, and no decision model was used: listings are made up in Harness
0.4.2's shape, read from its installed CLI's source, and every judge is a
stand-in.

## The machine

Apple M2 Max, macOS 26.6.2 (25G83), aarch64; Rust 1.96.0 pinned by
`rust-toolchain.toml`, release builds. Shared with other sessions: the
load average was 14 to 20 while these ran, so the largest values say more
about the machine than the code.

## What was measured

[`tasks-progress-cost.json`](tasks-progress-cost.json), from

    cargo run --release -p lamp-tasks --example progress_cost -- --samples 200

Listings of 1, 16, 40 and 256 harnesses, each progress line three events of
about 130 characters, printed with one space of indent as the CLI prints:
`parse_ps` on the bytes, `signs_in` on every line, and `Tracker::read` with
16 tasks watched, 200 times each; then 50 whole reads through the adapter
(`Cli::status`), the example standing in for `harness` and printing a
40-harness listing: spawn, capture, parse. Medians: parse 1.5 µs, 22 µs,
55 µs and 342 µs; signs over every line 9.4 µs, 158 µs, 398 µs and 2.6 ms;
tracking 1.0 to 11.7 µs; a whole scripted read 4.2 ms (13.7 ms the first,
18.4 ms the largest).

[`policy-progress-cost.json`](policy-progress-cost.json), from

    cargo run --release -p lamp-policy --example progress_cost -- --samples 30

The progress state's text and the combining of three answers, 1,000 times:
2.4 µs in the middle, 139 µs the first. Then the policy's loop on a bus in
`/tmp` with a stand-in judge that answers at once: a `tasks/status`
published to its decision received, 30 times, 52.7 ms in the middle,
48.1 ms the first, 58.9 ms the largest. Most of that is the loop's own
20 ms waits; Jev's own 0.5 to 1 s is not in it.

## What these do not say

How long a real `harness ps` takes on the lamp (it starts Node and asks the
office machine about every agent there), how often a real agent's `about`
changes, how often the signs fire on real lines and whether rightly, and
what Jev answers to the progress question. Those are the first builder's
hand-back, with the commands in the two documents.

## The gate

From `lampOS/`, on the branch's head, for the three crates touched
(`lamp-msgs`, `lamp-tasks`, `lamp-policy`) and every crate that depends on
`lamp-msgs`:

    cargo fmt --all --check
    cargo clippy -p lamp-msgs -p lamp-tasks -p lamp-policy -p lamp-body -p lamp-air \
      -p lamp-actions -p lamp-ears -p lamp-controls -p lamp-lean -p lamp-eyes \
      -p lamp-expression -p lamp-name -p lamp-keeper -p lamp-mood -p lamp-health \
      -p lamp-memory -p lamp-schedule -p lamp-records -p lamp-surround -p lamp-voice \
      --all-targets -- -D warnings
    cargo test -p <each of them> -- --test-threads=1
    cargo build --workspace

On the head rebased onto `lamp-voice` at `6573ce6c1`: formatting, Clippy
and the workspace build passed. The tests, passed / failed / ignored:

| Crate | Passed | Failed | Ignored |
|---|---:|---:|---:|
| `lamp-msgs` | 79 | 0 | 0 |
| `lamp-tasks` | 115 | 0 | 0 |
| `lamp-policy` | 154 | 0 | 2 |
| `lamp-voice` | 1,042 | 0 | 6 |
| `lamp-body` | 276 | 0 | 0 |
| `lamp-surround` | 260 | 0 | 0 |
| `lamp-expression` | 203 | 0 | 0 |
| `lamp-controls` | 371 | 0 | 0 |
| `lamp-memory` | 135 | 0 | 4 |
| `lamp-schedule` | 129 | 0 | 0 |
| `lamp-records` | 113 | 0 | 1 |
| `lamp-mood` | 95 | 0 | 2 |
| `lamp-lean` | 61 | 0 | 2 |
| `lamp-health` | 42 | 1 | 0 |
| `lamp-eyes` | 23 | 0 | 0 |
| `lamp-actions` | 19 | 0 | 0 |
| `lamp-keeper` | 18 | 0 | 0 |
| `lamp-ears` | 10 | 0 | 0 |
| `lamp-name` | 9 | 0 | 0 |
| `lamp-air` | 5 | 0 | 0 |
| All | 3,159 | 1 | 17 |

The one failure is the Mac's, not this change's, and it failed the same way
run again alone: `lamp-health`'s
`a_part_that_is_expected_and_silent_or_has_gone_quiet_ends_show_with_3`
expects the first journal line to be about the eyes, and got "health warn:
board: the disk is filling up: 5% of it is free". The development Mac's
data volume was 95% full (`df`: 48 GiB free of 926 GiB). Nothing in this
change reaches `lamp-health`. New here: `lamp-msgs` 4 (`tests/tasks.rs`),
`lamp-tasks` 11 (signs, change measure and schedule; `ps` parsing, times and
bounds; the runtime with and without the switch; the watched list and
`get_progress`), `lamp-policy` 10 (state, questions, combining, rule, hour,
counting and the unasked stand-in; the loop with stand-in judges, the hard
rules, the rule on a failed model, and without the switch).
