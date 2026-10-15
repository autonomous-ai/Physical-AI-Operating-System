# A sleeping lamp's name needs a person's speech: #749, 2026-10-15

Fourth's evidence for task 47 ([#749](https://github.com/autonomous-ai/lampOS/issues/749)).
Everything here was run on Fourth's Intel Mac (macOS 26.6.2, i7-10700K,
64 GiB, Rust 1.96.0) or on GitHub's native Linux ARM64 runner. Nothing was
run on a lamp. Every name, speech observation, joint, face and clock in the
replays is synthetic.

## What changed

`alive.sleep.name_requires_speech` in the Expression catalogue, off when
absent. With it `true`, a name heard while the lamp is asleep wakes it only
if the voice reported accepted person speech (`voice/activity`,
`speaking: true`, during a retained Listening turn) within 2 s before or
after the name's onset (the name's publication time less its `ago_ms`).
Otherwise the lamp stays asleep and the journal gets one line,
`lamp-expression: ignored sleeping name (<count>): <reason>`. Exact timing
and limits are in [alive.md](../../alive.md).

Why the voice's speech is a fair sign: the voice opens a turn as soon as its
own speech detector admits a person beginning to speak, and from then on
publishes `voice/activity` every 80 ms (`crates/voice/src/expression_out.rs`,
`commit_admitted_input` in `crates/voice/src/coordinator.rs`). A person
saying "Lamp" to a sleeping lamp therefore gives speech within the window.
On lamp-4ace at 01:45:21 the false name came with no turn and no
`voice/heard` (#714), which this rule would have left asleep.

## Replays (`replay/`)

Eight replays through the `lamp-expression` CLI on the Mac, 3,751 planner
ticks each, of two authored 75-second scenes: a name with no speech
(`unsupported-name`) and a name followed by speech (`name-then-speech`).
Base `e4922a3cb` against the change, with the switch absent, `false` and
`true`.

- Absent and `false`: the complete per-tick body, light and state output is
  byte for byte the base's, in both scenes.
- `true`, name with no speech: identical up to the first tick the rule acts
  on (replay clock 1,065,020,000 µs), then the lamp stays asleep.
- `true`, name then speech: byte for byte the base's, the same wake stretch.
  The first `waking` tick came 13,299 µs of replay clock after the speech
  input (18,300 µs after the name). That is the planner's injected clock,
  not a timing of the Mac, the bus or a lamp.
- The base binary refuses a catalogue that has the new field, even `false`
  (`negative_baseline_schema_probe` in `replay/comparison-1/summary.json`).
  So going back to the old binary needs the old catalogue too.

The first base build reused the changed binary through the shared Cargo
build folder. That pair is kept, marked superseded
(`replay/build-results-superseded.json`, `replay/superseded-cache-collision.json`);
the base was rebuilt and checked by its refusal of the new field. The raw
per-tick outputs were too large to keep. Their SHA-256, sizes and counts are
in `replay/local-raw-output-index.json`, and the inputs, catalogues and
command lines that make them again are in `replay/inputs/`.

The three changed source files of the commit offered
(`alive.rs`, `engine.rs`, `runtime.rs` in `crates/expression/src/`) have the
same SHA-256 as the ones replayed (`replay/source-binding.json`). Since that
base, `lamp-voice` changed one other Expression file: `main.rs` now sends
log lines through `lamp_ipc::log_relay`, which does not change the plan.

## The gate on the commit offered

Commit `10c9da386b4433866f193aa24c7b97037d655bde`, on `lamp-voice` at
`ba098193f`, from `lampOS/`, on Fourth's Mac, 04:28 to 04:57 UTC, with the
shared build folder cleaned first. The commands are in `gate/commands.txt`,
each stage's exit code in `gate/stages.txt`, and its whole output beside it.
The tree was clean before and after (`gate/status-*.txt`).

| Stage | Result |
|---|---|
| `cargo fmt -p lamp-expression --check` | passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | passed |
| `cargo test --locked -p lamp-expression -- --test-threads=1` | 228 passed, 0 failed, 0 ignored, 17 suites |
| `cargo test --locked -p lamp-lean -p lamp-voice -p lamp-memory -p lamp-voice-eval --no-fail-fast -- --test-threads=1` | 1,636 passed, **1 failed**, 15 ignored (the same 15 as before), 67 suites |
| `cargo build --workspace --locked` | passed |

**`cargo fmt --all --check` fails on `lamp-voice` itself**, in
`crates/voice/src/agent_tasks.rs:396`, which came with `5798f73d3` and is
not touched here (`gate/fmt-workspace.txt`). The first run stopped there
(`gate/run1-stopped-at-workspace-fmt/`); the second checks this crate's
format and keeps the workspace check's output.

**The one failure is not this change.** It is
`a_relative_path_or_an_unknown_option_gets_the_usage_line_and_exit_code_two`
in `crates/memory/tests/binary.rs`: the `lamp-memory` binary's stderr
lacked its usage line. That binary links `lamp-bus`, `lamp-msgs` and
`lamp-ipc` only (Expression is a dev-dependency of its tests, through
Voice), so this commit cannot change what it writes. Run again 40 times
on its own (`gate/memory-binary-rerun/`, load average about 7), it failed
5 times: four times without the usage line, once with no stderr at all.
The cause is in `lamp_ipc::log_relay` (`895177f31`, on `lamp-voice` since
09:18): `finish` returns when the pipe is empty and `holding` is 0, but the
relay thread stores `holding` only after its write loop, so while it holds
lines it has read and not yet written, `finish` can let `main` return and
those lines are lost. The same test passed in the ARM64 run below. It is
written under "Found, not mine" in Fourth's log and is not mended here.

ARM64: [run 38681458933](https://github.com/autonomous-ai/lampOS/actions/runs/38681458933)
on this commit, GitHub's native Linux ARM64 runner in Debian 12, built the
release binaries and passed the whole workspace's tests one at a time.
It succeeded: 286 suites, 4,449 passed, 0 failed, 35 ignored, the memory
test above among those passed. Its whole log is `arm64/run-38681458933.txt`
and the SHA-256 of each binary it kept is in `arm64/sha256.txt`;
`lamp-expression` is
`f857ada685d171202f299aefa9d1b097f28492e1a05be1619c747a1fcfaff4a8`. A
GitHub runner is not a lamp.

## On the lamp

Not run by Fourth. [lamp-check.md](lamp-check.md) has the steps for the
first builder: the minute with the switch absent, then the switch on and one
night's sleep, and what to record. Most likely wrong there:

1. The lamp's own servo noise as it bows into sleep may also be admitted by
   the voice's speech detector, and then the false name would still wake it.
   The record to look at is `voice/activity` around the name.
2. A real name said softly, or in a busy room, may not be admitted by the
   voice at all, and then the person must say more than the name, or the
   lamp wakes only when the voice starts to answer (Thinking or Speaking
   wake it as before).
