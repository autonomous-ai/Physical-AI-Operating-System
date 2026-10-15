# Said to the lamp from close to it: #753, 2026-10-15

Fourth's evidence for task 49 ([#753](https://github.com/autonomous-ai/lampOS/issues/753)).
Everything here was run on Fourth's Intel Mac (macOS 26.6.2, i7-10700K,
64 GiB, Rust 1.96.0) or on GitHub's native Linux ARM64 runner. Nothing was
run on a lamp. Every level, face and request in the tests is synthetic.

## What changed

`LAMP_ADDRESSED=on` in the voice's environment, the provider worker's
`--addressed`, right after the attention flags, and only with
`LAMP_ATTENTION` set and `LAMP_BUS=on` (the bus's root reaches the worker
only then; without it every sign is unknown and nothing is published). For each request the worker judges two signs over the
request's own sound: near (the room microphone's highest rise over its
quiet, 18 dB or more) and a close face turned to the lamp (of the eyes'
looks while the person spoke or in the 1.5 s before, at least two, 0.4 or
more saw a face turned to the lamp at least 0.15 of the picture's height).
It publishes `attention/addressed` once per request, lets out in a busy room
an answer the rule would hold when both signs hold, and writes the
judgement in `first_output`. Exact rules: [attention.md](../../attention.md),
"Said to the lamp from close to it". Why these lines, from V1 and
lamp-4ace: [reuse-decisions.md](../../reuse-decisions.md).

## Tests that need no lamp

In `crates/msgs/src/attention.rs`, `crates/voice/src/addressed.rs`,
`crates/voice/src/attention.rs` and `crates/voice/src/first_output.rs`:

- The message: its JSON is numbers only, an unknown sign is `null`, and
  `check()` refuses what does not add up (13 cases).
- The judgement: close and facing is addressed; far, small or turned away
  is not; one glance in four looks is not facing, two in five is, one look
  says nothing; nothing in the window is unknown, not no; looks up to 1.5 s
  before the speech count, earlier ones do not; a request still being said
  is judged up to now; the looks kept are bounded (200, 30 s) and in order.
- The rule: being addressed turns a busy room's hold into a play, never a
  play's reason, in both the plain and busy forms, and a new request
  forgets it; the lamp's own turns are not judged.
- The record: a line without the switch has no `addressed`; with it, the
  judgement is beside the seven `baseline` fields, which are untouched.
- The switch: `on` only; taken in its place among the worker's flags in the
  coordinator's order, alone too, where the worker leaves it off.
- A real bus in a temporary directory: faces published there are read with
  the eyes' own times, a look read twice is kept once, an expired look is
  not kept, and a subscriber gets the judgement exactly once per request.

Not exercised: the provider worker's loop with a live bus. Its bus root is
set once per process and no worker test sets it, so the first run of that
path is the first builder's minute on lamp-4ace.

## Timing (`timing.txt`)

A temporary test, not committed, on the Mac at load average 10 to 14,
2,000 times each. Release: judging a request with 300 rises and 200 looks
all in its window, 1.5 µs (p99 1.6 µs, at most 7.2 µs); reading the eyes'
newest look from a real bus, 21 µs (p99 24 µs, at most 77 µs). Debug, for
comparison: 25 µs and 48 µs. Not a lamp measurement.

## The gate on the commit offered

Commit `d44fdbd765568ddf98040faad1a98a8c4553dbb4`, on `lamp-voice` at
`a125eb3da`, from `lampOS/`, on Fourth's Mac, 06:32 to 07:16 UTC. The
commands are in `gate/commands.txt`, each stage's exit code in
`gate/stages.txt`, and its whole output beside it. The tree was clean
before and after.

| Stage | Result |
|---|---|
| `rustfmt --edition 2024 --check` on the changed files (not the two `lib.rs`, below) | passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | passed |
| `cargo test --workspace --locked --no-fail-fast -- --test-threads=1` | 4,433 passed, **3 failed**, 33 ignored, 287 suites |
| `cargo build --workspace --locked` | passed |

`lamp-msgs` changed, so every crate that reads the bus depends on it: the
whole workspace's tests ran.

**The three failures, none of them this change:**

1. `coordinator::tests::early_end::without_the_option_every_message_and_record_is_what_it_was`
   (`lamp-voice`): "privacy worker did not stop within shared 300 ms
   deadline; forced termination required; reaped with signal: 9", the
   voice's timed process tests that #739 is about, in the very words the
   first builder reported there for another `early_end` test. The provider
   worker this change touches is not the worker that was late.
2. `a_part_that_is_expected_and_silent_or_has_gone_quiet_ends_show_with_3`
   (`lamp-health`): its first line was "the disk is filling up: 5% of it is
   free", this Mac's own disk, where the test expects none. `lamp-health`
   warns under 10% free; the Mac had 22 GiB free of 466 just after
   the run.
3. `wrong_arguments_print_the_usage_and_end_with_2_for_serve_and_64_for_show`
   (`lamp-health`): its stderr was empty, the log relay's loss of a part's
   last lines that Fourth's log reported on 15 Oct for `lamp-memory`.

All three passed in the ARM64 run below.

`cargo fmt --all --check` fails on `lamp-voice` as it is, at
`crates/voice/src/agent_tasks.rs:396`, which this change does not touch.
`rustfmt` on a crate's `lib.rs` follows its modules into that file, so the
first run stopped there (`gate/run1-fmt-followed-lib-mods/`; its command
list still had the two `lib.rs`), and the two `lib.rs` (one line each)
are left out of the check. The second run was stopped because a test was
corrected in the checkout while it ran (`gate/run2-stopped-source-edited/`);
the third, because `lamp-voice` had moved under four of the changed files
and the branch was rebased (`gate/run3-stopped-for-rebase/`). No result of
those is counted.

ARM64: [run 38693004585](https://github.com/autonomous-ai/lampOS/actions/runs/38693004585)
on this commit, GitHub's native Linux ARM64 runner in Debian 12: success,
4,483 passed, 0 failed, 35 ignored across 287 suites, every new test
among those passed. Its whole log is `arm64/run-38693004585.txt` and the
SHA-256 of each binary in `arm64/sha256.txt`; `lamp-voice` is
`99c6c5d5f7cc4f6823d1fa62af04ebc604e42801724df6b1a80436039fc835cb`.

## On the lamp

Not run by Fourth. For the first builder:

1. The minute with the switch absent: the voice as now; no
   `attention/addressed` on the bus (`lamp-bus hz attention/addressed
   --seconds 60` sees nothing).
2. With `LAMP_ADDRESSED=on` beside the `LAMP_ATTENTION` and `LAMP_BUS=on` it has, a few
   requests from the desk: one `attention/addressed` each
   (`lamp-bus tap attention/addressed`), `rise_db` near the 19.5 to
   26.5 dB measured for `busy-near`, `looks` above 1, and with
   `LAMP_FIRST_OUTPUT` the line's `addressed` beside its decision.
3. The takes asked for on #753, for the lines.

Most likely wrong there:

1. lamp-4ace stands sideways to its seat (#763), so no face may be judged
   turned or reach 0.15, and nothing is addressed until it is turned.
2. A short request may get fewer than two looks, which leaves the face sign
   unknown: the eyes look about 4 times a second when active, less when
   idle.
3. Both lines come from other rules: 18 dB from `busy-near` on lamp-4ace,
   the face's from V1's gaze on its own lamps.
