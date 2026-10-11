# Agent 1 status

Owner: Agent 1 (Gemini and audio), branch `gemini-voice`.

- Protocol acknowledged: `direct-and-git-v2` (`MAIN-001`), read from
  `origin/lamp-v2-chat` at `666693a84`.
- Updated: 2026-10-11
- Tested code commit: `d15c3571b` on `gemini-voice`. The commits after it are
  documentation and this file.
- Base commit: `2535fb565`, with `b20df467d` and `666693a84` merged on top.
  Those two merges bring only coordination and documentation files.
- State: checkpoints 1 to 13 published. `MAIN-022`, `MAIN-024`, `MAIN-028`,
  `MAIN-031`, `MAIN-032` and `MAIN-033` are answered below. Environmental
  acquisition stays deferred.
- **Waiting on you:** nothing. `A1-009` is with your review, and your own
  adaptation of the gate supersedes my proposal branch.
- **Not building:** the continuation start (`A1-010`), as you decided.
- Direct messages: I read the ones that arrive in my session and answer here
  and in my own session output. I do not type into another session's
  terminal.
- Harness team: the team's own introduction reached my session after
  checkpoint 8, with a working route. I read `MAIN-028`, `MAIN-031`,
  `MAIN-032` and `MAIN-033` there and replied to the two that were
  questions. My reply to `MAIN-031` was recorded late: the question had
  expired while I was working. Git is still where my results are.

## Checkpoint 13: `MAIN-033`, the replies that interrupted themselves

[The analysis](../docs/self-echo-admission.md). No code. Host only: your two
certified bundles replayed with `lamp-audio replay-aec`, their capture
metadata, and your `SpeechProbability` in a scratch test that is not in the
tree. No device, no cloud, no upload, no provider transcript used as
evidence.

**Self-echo or room speech.** All four admissions that cancelled a reply are
explained by the lamp's own voice: render active in 20 to 50 of the last 50
blocks, the microphone explained 89 % to 99 % by the accepted render, and
the detector firing as the canceller's output rose. For trial 1 that is
established from the target's signals alone: at most -35 dB of that
microphone was anything but playback, and the prompt arrives at about
-24 dB. For trial 2's three it is the sufficient explanation and I cannot
exclude a prompt-level talker underneath from the target's signals, because
the echo there is at -5 dB to -9 dB and clipped. Your room audit decides
those.

**The reference timing is right.** One playback epoch and one DSP epoch per
run, no stale packets, the reference analysed 0.1 ms to 4.2 ms before each
capture block was read and 20 ms of audio ahead of it throughout, the
canceller's alignment steady at 104 ms and 112 ms. I found no fault in what
I own.

**Why the canceller leaves it.**

| | |
|---|---|
| The lamp's voice at its own microphone | -3.5 dB to -13 dB, samples at full scale in every reply |
| The prompt at the same microphone | about -22 dB to -26 dB |
| The echo against the reference the canceller is given | 9.6 dB and 9.9 dB louder at the median, up to 13 dB |
| Left by the best fixed linear filter, fitted afterwards per stretch | -13 dB to -33 dB |
| Left by the device in the admitted windows | -12.6 dB to -63.8 dB |
| Detector score on a cancelled signal at -50 dB | 0.87 to 0.99 |

After the best linear cancellation the lamp's own voice is as loud as the
person it is listening for. The rest is suppression, which mutes or lets
through by guessing, and the detector fires at any level.

**Tried offline, none proposed.** The replay scored with your detector
reproduces three of the four device admissions at the same block, so a
correction can be tried without a device. Transparent mode `Hmm`: no change,
block for block. Noise suppression on, the reference 6, 12 and 18 dB louder,
the capture 12 dB quieter going in: each still leaves two to four of seven
replies with six detector blocks at 0.80 or more, and moves which ones. The
louder reference was my best guess, from the canceller's source: it assumes
a path gain of one where this path has three. It did not hold up. None of
this is in the tree.

**Proposed.**

1. No change to the canceller or the reference.
2. The level is the lever: +10 dB to +20 dB of own voice over the prompt at
   the microphone, with clipping. It can be read from any bundle before a
   conversation is tried. While it holds, nothing measured at the microphone
   can see the person, and my evidence cannot either.
3. Speech probability alone cannot admit a turn while the lamp is speaking.
   The four false admissions had render active; the two correct ones had
   none. `render_active_blocks` is not an echo threshold. What admission does
   with it is yours.
4. The offline check is yours to adopt if you want it: the scratch test is
   twenty lines in your crate. Say so and I send it as a proposal.

**Limits.** Two trials, one lamp, one room, volume 100. Nothing at another
volume. No recording with a person speaking over the lamp. The replay
differs from the device in 1.7 % to 2.5 % of samples with noise suppression
off and does not reproduce trial 2's third admission. I have not established
what makes the path non-linear.

## Checkpoint 12: your two reviews, `MAIN-031` and `MAIN-032`

| Commit | Files | Content |
|---|---|---|
| `eaf7ce62f` | `crates/gemini/examples/service_probe.rs`, `crates/gemini/Cargo.toml`, `crates/gemini/src/{lib,session}.rs`, `session/tests.rs`, new `session/tests/drops.rs` | The probe scores what settles the barrier apart from what does not, and output apart from markers; `InputSender::drop_ledger()`; 14 tests |
| `7014e568c` | `crates/audio/src/playout.rs`, `crates/audio/tests/playout_replay.rs` | The withheld last block is no longer audio in hand in the replay; the replay is labelled counterfactual |
| `d15c3571b` | the same two files | The coordinated build's reply, with its stall in the middle, as a regression of its own |

**`MAIN-032`, 1: the source attribution. You are right and I was wrong.**
`51bd815e9` builds the `Supervisor` and selects `Reporting::Legacy`; in that
mode a lost turn is returned as an error and the process exits. I had written
"the worker had no supervisor" from the text on standard error without
reading `provider_worker.rs` at that commit. Corrected in
[the handoff](../docs/gemini-unconfirmed-interruption.md) and in checkpoint
10 below.

**`MAIN-032`, 2: the probe. Both errors were real.**

- The Boolean was true for an interruption notice or any completion, and
  `Require` releases only on a completion that is idle. It is gone. The probe
  now reports `interruption_seen`, `completion_without_idle_seen`,
  `idle_completion_settled` and `probe_assumed_after_silence` separately,
  each only for the cut-off request and only before the probe stopped
  waiting.
- "Output after the answer" was any dropped event. A discard event does not
  say whether the message carried output: with no request in progress a stray
  completion and stray audio are both `UnownedOutput`. So the connection now
  counts what it drops by what it carried, and the probe reads the counts.
  `after_answer` is present only if request 2 reached its own idle
  completion; it reports `output_messages` and `marker_only_messages`
  apart. Without that completion the probe does not listen on and scores
  nothing.
- Raw observations are all still printed: `before_second_start`, `events`,
  `after_answer_events`.

One accessor is new, and additive: `InputSender::drop_ledger() -> DropLedger`
with `retired_output`, `unowned_output` and `unowned_markers`. No `Event` or
`Discard` variant changed, since your worker matches both. It changes nothing
that is delivered. `Require` is untouched and there is no continuation.

Regressions, all in the normal gate (`crates/gemini/Cargo.toml` now builds
the example's tests):

| Where | Tests | What they pin |
|---|---|---|
| `service_probe.rs`, scoring | 8 | An interruption notice alone, a non-idle completion, an idle completion after the probe had assumed, and the second request's own events each leave `idle_completion_settled` false; an idle completion in time sets it; nothing after the answer is scored without a completed second request; markers are not output |
| `service_probe.rs`, the whole trial against a scripted service, paused clock | 5 | A silent service; a notice with a non-idle completion, then a late empty completion after the answer (0 output, 1 marker); a service that settles; one that answers twice (5 output, 2 markers); an answer that never completes (`after_answer` absent) |
| `session/tests/drops.rs` | 1 | Output of a retired request, a stray completion and stray audio are counted where they belong; a delivered answer counts nowhere |

A trap I fell into while writing them: a completion with no interaction
status decodes as idle. A non-idle one has to say `IN_PROGRESS`.

The probe has still never spoken to the service.

**`MAIN-031`: the playout replay.**

- **Relabelled.** [The handoff](../docs/playout-start.md) and the test file
  now say at the top that every "with the gate" figure is counterfactual: a
  replay of the arrivals each trace retained, with completion assumed at the
  last retained arrival because the traces carry no completion or
  cancellation time. Not qualified physical turns, not a held-out set. The
  horizon was chosen on the same replies.
- **The withheld block.** My model made your reviewer's mistake too: it
  counted the last 240 frames as audio in hand. It no longer does, and it
  asks the gate only when a block can be handed over. `StartGate::open` says
  so in its documentation. Every figure moved by one block:

  | Seventeen replies, counterfactual | Before | Corrected |
  |---|---|---|
  | Replies with zeros at the start, with the gate | 2 | 2 |
  | Their zeros | 300 ms | 280 ms |
  | First speech, mean and most added | 64 ms, 160 ms | 70 ms, 170 ms |
  | End of reply, mean and most added | 14 ms, 60 ms | 19 ms, 70 ms |
  | First Rust pilot reply: first speech, end | +50 ms, +20 ms | +60 ms, +30 ms |
  | Smallest horizon that covers that reply | 180 ms | 170 ms |
  | Your NS-off trial 2 | 260 ms to 110 ms | 260 ms to 100 ms |

- **My proposal branch is superseded.** `b4782eda1` asks the gate before the
  withheld block is checked, as your reviewer found. I have not changed it:
  you have your own corrected adaptation and asked me to keep my changes
  apart from your candidate.
- I read `/private/tmp/lamp-nscheck-room-review-20261011/report.md`. Its
  trial 2, 260 ms at reply offset 40 ms to 300 ms, is the reply already in
  the replay as `ns-off/2`.
- **The first stall in the middle of a reply.** Your coordinated build's
  reply (`lamp-rust-coordinated-evidence-20261011T030937Z`) is the first of
  eighteen in which the service itself fell behind: about 95 ms ahead of
  real time for thirteen events, then 705 ms to the fourteenth, with 2,700 ms
  delivered and 2,910 ms gone. Its 140 ms of zeros at 2.77 s had only the
  withheld block queued. That is the unavoidable kind `MAIN-022` asked me to
  tell apart. My model agrees with what you measured for your adaptation:
  110 ms later to first speech, the opening gap gone, the later gap still
  there at 110 ms instead of 130 ms in the model, the same end. I kept the
  reply out of the seventeen: the horizon was not chosen with it, and it is
  a regression of its own. In the trace it shows as a need set by an arrival
  2.9 s in, nowhere near the start.

Host, source-only export of `7014e568c`:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 939 passed, 0 failed, 4 ignored |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |

The 939 are the 925 of checkpoint 11 and 14 new: 13 in the probe, 1 for the
drop ledger.

Host, source-only export of `d15c3571b`, which adds one test and one
comment: `cargo fmt --all -- --check` and the workspace Clippy pass;
`cargo test --locked --offline -p lamp-audio -- --test-threads=1`,
114 passed, 0 failed.

Linux/ARM64, emulated:

| Command (each with `--locked --offline --target aarch64-unknown-linux-gnu`) | Commit | Result |
|---|---|---|
| `cargo clippy --workspace --all-targets -- -D warnings` | `7014e568c` | pass |
| `cargo test --release -p lamp-gemini -- --test-threads=1` | `7014e568c` | 171 passed, 0 failed, 2 ignored |
| `cargo test --release -p lamp-audio -- --test-threads=1` | `7014e568c` | 133 passed, 0 failed |
| `cargo build --release -p lamp-live -p lamp-voice-eval` | `7014e568c` | pass |
| `cargo clippy -p lamp-audio --all-targets -- -D warnings` | `d15c3571b` | pass |
| `cargo test --release -p lamp-audio -- --test-threads=1` | `d15c3571b` | 134 passed, 0 failed |

Not run: the whole workspace for `d15c3571b`; the live crate's tests on
ARM64 for either commit, which they do not touch; any device; any cloud.

## Checkpoint 11: the echo evidence on a real recording, and two more replies

[Echo evidence](../docs/echo-evidence.md), revised.
[Playout start](../docs/playout-start.md), extended.

| Commit | Files | Content |
|---|---|---|
| `64235a78a` | `crates/audio/src/evidence.rs`, `crates/audio/tests/evidence.rs` | A sixteen-copy fit decides a match; rival alignments; `EchoMatch::residual_playback_floor`; one regression over the path the device recorded |
| `9e95cbf1e` | `crates/audio/tests/playout_replay.rs`, `docs/playout-start.md` | The two NS-off replies of `MAIN-028` added to the playout replay: seventeen replies |

**`A1-011`, the echo evidence.** On the first pilot's certified recording,
replayed offline, the observer as published reported a match in 0 of 4,001
blocks. It now reports one in 100, starting 120 ms after the echo became
loud.

| First pilot recording, the device's own signals | |
|---|---|
| `capture_explained` while the capture was above -40 dB, 85 blocks | 91 / 94 / 99 at the tenth, fiftieth and ninetieth percentile; none below 50 |
| Match claimed during the spoken question or after the reply | none |
| `residual_explained` while the cancelled signal was above -30 dB, 30 blocks | median 26, with nobody speaking |
| `residual_playback_floor`, new, on those blocks | 27 / 78 / 93; 50 or more in 22 of 30 |

Three things were wrong, all found on that recording and none on my scripted
scenes:

1. The path from the speaker to the microphone beside it is spread over
   about 3 ms. One delayed copy explains 30 % of a capture that was all
   playback; the search wanted 50 % from one copy. Sixteen copies explain a
   median of 91 %.
2. The voice repeats every 5.25 ms, and an alignment that far off explains a
   steady vowel at 97 % to 99 % and then fails. The observer now fits rivals
   within 10 ms and takes one that does better.
3. **`residual_explained` is not what I said it was.** I wrote that a low
   value means "something the render does not explain: a person, another
   device, or noise". On the device it is low with nobody speaking, because
   what a canceller leaves is what a linear fit could not remove. A low value
   means nothing. I have corrected the handoff and the list under `A1-003`.
   The new floor is the field to read: the cancelled signal cannot hold more
   of anything else than the capture did.

The device's canceller left the cancelled signal as loud as -11 dB during
that reply, a median of 9.6 dB under the capture in the blocks where it was
above -40 dB. That is your recording and your canceller configuration, and I
am only reporting what the replay shows.

`EchoMatch` gained one field. Nothing consumed the type. Cost: 15.3 µs per
block on the host in a release build, from 9.0. Emulated ARM64: 88.6 µs, from
60.0. Neither is the board.

**Limits.** One reply, one lamp, one room, one voice, and no moment with a
person speaking over playback. Every double-talk figure is still scripted.
The floor is zero, and says nothing, whenever the unexplained part of the
capture could cover the cancelled signal: a quiet person under loud playback
is not something this sees. It is not ready to gate admission, and I am not
asking for `A1-003` yet.

**Playout, two more replies.** Your second NS-off reply is the same defect
with a slower service: 50 ms of audio, then nothing for 303 ms. With the
200 ms horizon it starts at 162 ms and gaps for 110 ms instead of 260 ms.
The first NS-off reply began with one sample and nothing for 391 ms, so it
could not start early and did not gap. Of seventeen replies the second burst
came within 191 ms in fourteen and past 300 ms in three, and two of those
three are among the day's own three runs.

| Horizon, seventeen replies, enqueue rows, outside the suite | Replies with zeros at the start | Zeros, total | First speech, mean and most added | End, mean and most added |
|---|---|---|---|---|
| none | 9 | 1,150 ms | | |
| 200 ms | 2 | 300 ms | 65 ms, 160 ms | 14 ms, 60 ms |
| 300 ms | 2 | 100 ms | 112 ms, 250 ms | 50 ms, 130 ms |
| 400 ms | none | none | 140 ms, 320 ms | 72 ms, 170 ms |

200 ms is still where the cost turns. I no longer say it covers everything
but one stall, and I do not know it is right for the configuration the day's
runs used. The proposal's `start_horizon_needed_us` on every reply is what
will say. The proposal branch is unchanged at `b4782eda1`.

**For your endpoint diagnosis, from the NS-on bundle of `MAIN-024`.** Host
only, your detector untouched. I replayed the certified capture with
`lamp-audio replay-aec` and scored the outputs with
`lamp_live::activity::SpeechProbability` in a scratch test that is not in the
tree.

| | |
|---|---|
| Replayed cancelled signal against the device's own, NS on | 2 of 54,240 samples differ |
| Offline score of the device's cancelled signal against its recorded `vad_score` | within 0.005 on every block |
| Longest run under 0.35, NS on, device and replay | blocks 53 to 120: 68 blocks, so the endpoint at 60 |
| The same capture replayed with NS off | blocks 79 to 119: 41 blocks, no endpoint |
| What the capture holds in blocks 53 to 81 | the end of the first phrase, at -22 dB falling to -58 dB |
| Blocks 82 to 116 | room silence at -74 dB, about 350 ms |

So the pause inside the question is 350 ms, and the other 250 ms of the 600
are speech the detector did not score as speech. NS took 3 dB to 6 dB off
those blocks. With NS off five of them reach 0.35: blocks 62 to 64 at 0.45 to
0.65, block 68 at 0.38 and block 78 at 0.36. The last two are barely over.
Without them the run would be 55 blocks, 50 ms short of an endpoint. On this
one utterance NS off avoids the split, and not by much.

Host, source-only export of `64235a78a`:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 925 passed, 0 failed, 4 ignored |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |

Host, source-only export of `9e95cbf1e`, which changes one test file:
`cargo fmt --all -- --check` and the workspace Clippy pass;
`cargo test --locked --offline -p lamp-audio -- --test-threads=1`,
113 passed, 0 failed.

Linux/ARM64, emulated:

| Command (each with `--locked --offline --target aarch64-unknown-linux-gnu`) | Commit | Result |
|---|---|---|
| `cargo clippy --workspace --all-targets -- -D warnings` | `64235a78a` | pass |
| `cargo test --release -p lamp-audio -- --test-threads=1` | `64235a78a` | 133 passed, 0 failed |
| `cargo build --release -p lamp-live -p lamp-voice-eval` | `64235a78a` | pass |
| `cargo test --release -p lamp-live -- --test-threads=1` | `64235a78a` | 302 passed, 0 failed, 2 ignored |
| `cargo clippy -p lamp-audio --all-targets -- -D warnings` | `9e95cbf1e` | pass |
| `cargo test --release -p lamp-audio -- --test-threads=1` | `9e95cbf1e` | 133 passed, 0 failed |

Offline, on the host, outside the suite: the two replays of device bundles
and the scoring described above. Their numbers are in the handoffs. The
recordings are not in Git.

Not run: the whole workspace for `9e95cbf1e`; the Gemini crate on ARM64 for
either commit, which they do not touch; any device; any cloud.

## Checkpoint 10: `MAIN-024`, the interruption the service never confirmed

[Analysis and proposal](../docs/gemini-unconfirmed-interruption.md).

| Commit | Files | Content |
|---|---|---|
| `95cd4b263` | new `crates/gemini/src/recovery/tests/barrier.rs`, `recovery/tests.rs`, `testing.rs`, `examples/service_probe.rs` | The device's timeline through the supervisor; a `try_send` test helper; `interrupt-before-output [GAP_MS]` now reports whether the service confirmed and whether a second answer followed |

No production code changed. No interface changed.

**What happened, in transport terms.** Request 1 was committed and
unanswered when request 2 started 245 ms later. Its answer became the
barrier. `activityStart` went out as the interruption request, the second
part's audio went out behind it, and request 2's `activityEnd` was withheld.
The service sent one input transcript and nothing else. Two seconds after the
interruption request the connection was given up. That is `Require` working
as written; I had listed it as a known gap in checkpoint 1.

**The safe recovery is a new connection, and nothing less.** To move on the
same connection the withheld `activityEnd` has to be sent, and then the
service owes one answer or two, and nothing says which. Dropping the first
does not help: the transport still cannot know whether another is owed, so
the next request inherits the doubt. Only a new socket sets the count to
zero. The supervisor already does this, and the regression pins it: one
`TurnLost` for request 2 at `AwaitingResponse`, nothing for request 1, a
resumed connection, and an answer the old connection sends afterwards
reaching nobody. **Corrected (`MAIN-032`):** I wrote here that `51bd815e9`
had no supervisor. It builds one and selects `Reporting::Legacy`, where a
lost turn ends the process.

**It will not stay rare.** It takes an endpoint and then more speech before
the first audio of the answer, about a second later. A person who pauses
600 ms inside a sentence does that. One sample says the service stays silent.

**Proposed** (the handoff has each in full):

1. Keep `Require`. Treat `TurnLost` after `BarrierTimeout` as one lost
   question, answered with the turn-lost cue.
2. Do not submit a continuation as a new question. `A1-010`. **Rejected in
   `MAIN-028`.**
3. Run `service_probe ... interrupt-before-output 241` once you have the
   cloud. **Its scoring was wrong as published here and is corrected in
   checkpoint 12.**

I am not proposing `AssumeAfterQuiet`. It would have answered this run after
its own two seconds of silence, on the assumption `Require` exists to avoid.

**Limits.** One device run. I ran nothing on the cloud or a device. The probe
change compiles and its argument handling was checked offline against a
missing provider file; it has never spoken to the service.

Host, source-only export of `95cd4b263`:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 924 passed, 0 failed, 4 ignored |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |

The two new tests are in the 924.

Linux/ARM64, emulated, `95cd4b263`:

| Command (each with `--locked --offline --target aarch64-unknown-linux-gnu`) | Result |
|---|---|
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --release -p lamp-gemini -- --test-threads=1` | 157 passed, 0 failed, 2 ignored |
| `cargo test --release -p lamp-audio -- --test-threads=1` | 132 passed, 0 failed |
| `cargo build --release -p lamp-live -p lamp-voice-eval` | pass |

Not run on ARM64 for this commit: the live crate's tests, which it does not
touch.

## Checkpoint 9: `MAIN-022`, the gap in the first reply

[Recommendation, data and tests](../docs/playout-start.md).

| Commit | Files | Content |
|---|---|---|
| `fd8f6845a` | new `crates/audio/src/playout.rs`, new `crates/audio/tests/playout_replay.rs`, `crates/audio/src/lib.rs` | `StartGate`, `StartNeed`, and fifteen recorded arrival schedules replayed through them |
| `b4782eda1` | `gemini-voice-playout-proposal`: `crates/live/src/coordinator.rs` | **Proposal, your file.** The gate on the first speaker chunk, and what each reply needed in the trace |

**The gap.** The reply's first audio event carried one sample, the second
130 ms at 14.6 ms, the third came at 171.8 ms. Speech started at 24.8 ms, ran
out at 154.6 ms and resumed at 183.5 ms. The service was 42 ms late once, at
the start. Nothing local was slow.

**It is the usual case, not this reply's.** The same rows exist in six
earlier device sessions on this machine. Of fifteen replies, eight have zeros
at the start, 890 ms in all, 10 ms to 320 ms each. The service's second burst
comes 130 ms to 190 ms after its first audio event in thirteen of fifteen.
When the first 26 ms bring something playable, playout starts on it and runs
dry. When they bring one sample, playout cannot start until the second burst,
and those replies do not gap.

**Recommendation.** Hold a reply's first block until the time since its first
audio plus the audio in hand reaches 200 ms, or the reply is complete. Then
never hold again.

| Fifteen recorded replies, in the model | Without | With |
|---|---|---|
| Replies with zeros at the start | 8 | 1 |
| Zeros at the start | 890 ms | 190 ms |
| First speech, mean added | | 62 ms |
| First speech, most added | | 160 ms |
| End of reply, mean added | | 15 ms |
| Latest start after first audio | 200 ms | 200 ms |

The hold is paid at the start and refunded at the end: each reply ends later
by the hold minus the zeros that no longer fall inside it. A fixed delay
needs 160 ms on every reply to cover the same ones.

**Interruption.** Held audio is in the coordinator's queue. No permit has
been issued for it and the device has none, so cancelling during the hold has
nothing to flush.

**What a start cannot fix.** One reply's second burst came 379 ms after
80 ms of audio; the gate shortens that gap from 320 ms to 190 ms. A stall
later in a reply is a gap under any start, and I recommend no rebuffering
rule: the service sends 100 ms or more at a time in these traces, so holding
after a stall only lengthens it. None of the fifteen has a stall in the
middle.

**Not arrival gaps at all.** Three more 10 ms gaps in the earlier sessions
were written with 16,080, 29,520 and 60,960 frames queued in the coordinator,
each while a provider burst was being enqueued. One block is outstanding at a
time. I have not established the cause. Two outstanding would cover a late
tick and cost interruption nothing, since an unwritten block is still refused
on cancellation. That is a suggestion about your file.

**Also seen.** Nine of fifteen replies begin with an audio event of one
sample. I have not established whether the service sends it or the decode
path makes it.

**Limits.** The replay is a model of the hand-over: one block per 10 ms
slot, the last block withheld until the reply is complete, zeros when nothing
is in hand. It reproduces every recorded start gap to within one block. It is
not the coordinator, not the speaker loop, and no device has run the gate.
200 ms is the smallest horizon that covers these fifteen replies but the one
stall, with 9 ms to spare; it comes from one network path and one model
configuration. The schedules in the test are timings and frame counts from
your traces, no audio and no words; say if you would rather they were not in
the repository.

Host, source-only export of `fd8f6845a`:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 922 passed, 0 failed, 4 ignored |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |

The 922 are the 901 of checkpoint 8 and 21 new: 11 in `playout.rs`, 10 in
`playout_replay.rs`.

Host, source-only export of the proposal `b4782eda1`: the same four commands
pass, 923 passed, 0 failed, 4 ignored. The one more is
`a_reply_starts_when_its_audio_in_hand_covers_the_start_horizon` in
`coordinator.rs`.

Linux/ARM64, emulated, `fd8f6845a`:

| Command (each with `--locked --offline --target aarch64-unknown-linux-gnu`) | Result |
|---|---|
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test --release -p lamp-audio -- --test-threads=1` | 132 passed, 0 failed |
| `cargo build --release -p lamp-live -p lamp-voice-eval` | pass |

Not run: the proposal on ARM64; the Gemini and live crates on ARM64 for this
commit, which it does not touch; any device.

## Checkpoint 8: the speaker boundary behind a device trait

The speaker half of `A1-007`. [Handoff](../docs/audio-device-seam.md).

| Commit | Files | Content |
|---|---|---|
| `6d7c39303` | `crates/audio/src/{pcm,device,alsa_device,testing,lib}.rs`, new `crates/audio/tests/speaker_scripted.rs`, `crates/live/src/audio_workers.rs` | `PlaybackDevice`, `SpeakerOn<D>`, `AlsaPlayback`, a scripted speaker, `speaker_on::<D>` |
| `aef2e51a2` | `docs/audio-device-seam.md` | Handoff |

Behavior on the board is meant to be unchanged: `AlsaPlayback` makes the
calls the boundary made before, in the same order, and `Speaker` and
`speaker` keep their names and signatures. The guard, the ownership ledger,
the clock mode and every discard decision are untouched above the trait.

For Agent 2, the pieces a process-level harness needs now all exist:

```rust
lamp_audio::testing::{CaptureControl, ScriptedCapture, PlaybackControl, ScriptedPlayback};
lamp_live::audio_workers::{capture_on, speaker_on};   // generic over the device
```

`PlaybackControl::accepted()` is every sample the scripted speaker accepted,
in order: feed that back into `CaptureControl::push` at your echo gain and
delay and you have the room model. Neither device has a clock; you advance
them. `audio_workers` is compiled on Linux only, so this runs in a Linux
container unless Main opens that gate in `crates/live/src/lib.rs`.

Host, source-only export of `aef2e51a2`:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 901 passed, 0 failed, 4 ignored |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |

Linux/ARM64, emulated, on the same code before it was committed (the tree at
`ae4f05db4` plus this seam), covering checkpoints 7 and 8:

| Command (each with `--locked --offline --target aarch64-unknown-linux-gnu`) | Result |
|---|---|
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo build --release -p lamp-live -p lamp-voice-eval` | pass |
| `cargo test --release -p lamp-audio -- --test-threads=1` | 111 passed, 0 failed |
| `cargo test --release -p lamp-gemini -- --test-threads=1` | 155 passed, 0 failed, 2 ignored |
| `cargo test --release -p lamp-live -- --test-threads=1` | 302 passed, 0 failed, 2 ignored |

The 111 include the ALSA `null` suites through the trait (7 capture, 7
speaker) and the 6 scripted speaker tests. The real-time worker test that
failed once in the previous emulated run passed here.

Not run: the whole workspace on ARM64 (voice evaluator and the other
crates), the long ignored tests, any device. Not established: anything about
a real codec. The two worker loops still have no test of their own.

## Checkpoint 7: `MAIN-019` and the probe's file open

| Commit | Files | Content |
|---|---|---|
| `bb882e024` | `crates/gemini/src/session.rs`, `session/tests/images.rs` | Image completion against revocation, decided under the `Images` lock |
| `a77bdb0fd` | `crates/gemini/examples/service_probe.rs` | The picture's open is bounded at one second |

**`MAIN-019`.** Reproduced as you described and fixed as you asked. When the
image write returns, the connection takes the `Images` lock, clears the
in-flight mark and reads the latched abandonment in the same critical
section. A latched abandonment turns the completed write into
`InputCancelled`: the connection ends, the held input is dropped, the ledger
counts the image as revoked and not sent. A revocation that takes the lock
after that finds nothing in flight and returns `Nothing`. Your transport
wrapper is in the suite as
`a_revocation_that_lands_as_the_image_write_completes_still_abandons_the_connection`.
The image itself can be complete on the wire in that case; that cannot be
undone, and the handoff says so.

**Probe.** `lamp-gemini` has no dependency that names the platform's
nonblocking-open flag, and I did not add one for a probe. The open runs on a
helper thread with a one-second limit, then the descriptor is checked
(regular file, same device and inode as the name) and at most 64 KiB plus one
byte is read. Run offline against a pipe, a 70,000-byte file, a link, a
missing file and a valid file: the first three are refused, the fourth is not
found, the fifth passes and stops at the missing provider file.

Host, source-only export of `a77bdb0fd`, focused:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline -p lamp-gemini -- --test-threads=1` | 155 passed, 0 failed, 2 ignored |
| `cargo test --locked --offline -p lamp-live --lib provider_worker -- --test-threads=1` | 44 passed, 0 failed, 2 ignored |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |

Not run for these two commits: the whole-workspace suite, Linux/ARM64, the
long ignored tests. Image activation stays off on my side too: nothing calls
`try_image`.

## Checkpoint 6: the microphone boundary behind a device trait

The capture half of `A1-007`. [Handoff](../docs/audio-device-seam.md).

| Commit | Files | Content |
|---|---|---|
| `1b656d803` | `crates/audio/Cargo.toml`, `crates/audio/src/{capture,lib}.rs`, new `crates/audio/src/{device,alsa_device,testing}.rs`, `crates/live/src/audio_workers.rs` | `CaptureDevice`, `MicrophoneOn<D>`, `AlsaCapture`, a scripted device, `capture_on::<D>` |
| `fc1be2f0d` | new `docs/audio-device-seam.md` | Handoff |

Behavior on the board is meant to be unchanged: `AlsaCapture` makes the calls
the boundary made before, in the same order, and `Microphone`, `capture` and
`capture_with_noise_suppression` keep their names and signatures. What
changed for callers: `CaptureError` carries `device_error: Option<DeviceFault>`
(call name and `errno`) where it carried `alsa_error`; nothing outside the
audio crate read that field. `crates/audio/Cargo.toml` now lists `lamp-ipc`
for every unix target instead of Linux only; `Cargo.lock` is unchanged.

New on the host: 12 capture unit tests over `ScriptedCapture`, eight of them
covering behavior that had no test off Linux: assembly across short reads, an
xrun and restart as a new epoch, a device that vanishes or stops running, a
read that blocks past its 10 ms budget, a stale partial block, open and
configure failures, a privacy change, a failed discard.

For Agent 2: `lamp_audio::testing::{CaptureControl, ScriptedCapture}` and
`lamp_live::audio_workers::capture_on::<D>` are what a process-level harness
needs for the microphone side. The scripted device has no clock and no rate;
a room model goes on top. The speaker side does not exist yet.

Host, source-only export of `fc1be2f0d`:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 894 passed, 0 failed, 4 ignored |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |

Linux/ARM64, emulated, on the same code (the tree at `0bf54e63f` plus the
seam, before it was committed), covering checkpoints 3 to 6:

| Command (each with `--locked --offline --target aarch64-unknown-linux-gnu`) | Result |
|---|---|
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo build --release -p lamp-live -p lamp-voice-eval` | pass |
| `cargo test --release -p lamp-audio -- --test-threads=1` | 105 passed, 0 failed |
| `cargo test --release -p lamp-gemini -- --test-threads=1` | 154 passed, 0 failed, 2 ignored |
| `cargo test --release -p lamp-live --lib provider_worker -- --test-threads=1` | First run: 43 passed, 1 failed. Second run: 44 passed |

The one failure was `interrupting_a_capacity_paced_answer_is_not_delayed_by_its_queued_audio`,
with the test harness's own 100 ms message-freshness check tripping
("stale, reordered or wrong-boot runtime message"). Repeated alone six times
it passed six times (first packet of the new answer after 7 to 10 ms). It is
the same real-time test that fails in unoptimized emulated builds; this is
the first time it has failed in an optimized one. I count it as a flake of
the harness under emulation and have not hidden it.

An earlier emulated run of this tree reported an unresolved import. That was
my own staging: a file edited while the previous container run was compiling
kept its older time stamp, and cargo linked a stale `lamp-audio`. Staged with
fresh times, the same source builds. No code changed between the two runs.

Not run: the whole-workspace suite on ARM64; the long ignored tests; any
device. The capture loop itself still has no test; see the handoff.

## Checkpoint 5: corrections from Main's review

`MAIN-012`, the rest of `MAIN-008` and the probe points of `MAIN-011`, read
from the request files and from your review of `e73091d22`. Thank you for the
regression; it was right.

| Commit | Files | Content |
|---|---|---|
| `ee5387da7` | `crates/gemini/src/{lib,session,recovery}.rs`, `session/tests/images.rs`, `recovery/tests/{pressure,checked}.rs`, one argument in `examples/service_probe.rs` | Image write stops on shutdown and on camera revocation; `revoke_images`; `try_start_for_session` |
| `d938a3101` | `crates/gemini/examples/service_probe.rs` | Probe corrections |

**`MAIN-012`, shutdown.** While an image is being written the connection now
watches for a stop as well. A shutdown abandons the write at once; nothing
held behind it is sent; and every held command is preceded by the same check.
Your regression is in the suite as
`shutdown_during_a_blocked_image_write_sends_nothing_that_was_held_behind_it`
and passes.

**`MAIN-012`, camera-only revocation.** API changes, both in my own new
surface:

```rust
input.try_image(request, frame, grant, captured_at, jpeg)   // one more argument
input.revoke_images(through_grant) -> ImageRevocation       // Nothing | DroppedUnsent | AbandonedInFlight
// the same two on Supervisor; a revocation is remembered across reconnects
```

`grant` is your number for the camera permission the frame was captured
under, increasing with each new permission. `revoke_images` is synchronous
and takes the lock an offer takes, so the two cannot pass each other. A
waiting image is dropped and the request is untouched. An image already being
written cannot be undone: its write is abandoned and its connection ends as
`Error::InputCancelled`, which is now reconnectable (it was defined and never
produced before). The request then speaking is reported lost once. Later
offers under a revoked grant are refused as `ImageRefusal::Revoked`, on every
later connection as well. `Fault::from(Error::InputCancelled)` in your worker
is still `Local`.

**`MAIN-008`, as you bind it.**

```rust
pub fn try_start_for_session(&self, expected: SessionId, request: RequestId)
    -> Result<(), StartRefusal>;
pub enum StartRefusal { NotConnected, SessionMismatch { live: SessionId }, Refused(Error) }
```

`try_start_on` and `WrongSession` from checkpoint 4 are renamed; nothing else
about them changed. The comparison is made in the same synchronous call that
obtains the ready connection, before `Start` is queued and before any earlier
request is retired. New regression for your case "existing B request/state is
not changed": `a_refused_start_leaves_the_request_on_the_live_connection_untouched`.

**`MAIN-008`, second finding: confirmed.** `TurnLost { InputOpen }`, or
`AwaitingResponse` when the `End` was accepted too, can arrive with no
`InputStarted` before it. The stage is recorded when my call accepts your
input; `InputStarted` is emitted only after `activityStart` is written. It
was not covered; it is now:
`a_request_lost_before_its_start_was_written_is_reported_without_input_started`,
which fails the `activityStart` write on a connection with no room and
asserts both stages and the absence of `InputStarted`.

**`MAIN-011`, probes.** Your suspicion about `interrupt-before-output` is
correct, and the sample settles nothing. `try_start` retires earlier requests
synchronously, so the second start overtook the first `End` inside the
client: request 1 was never committed, no `activityEnd` was sent for it, and
the service received both recordings as one question. `d938a3101`:

- `interrupt-before-output` waits 300 ms after the first `End`, records what
  the service sends meanwhile, and prints
  `first_request_committed_before_second_start` (from the transport's own
  `waiting_for_barrier` on the second start), `first_answer_began_before_second_start`,
  and a `trial` label: `interrupt_before_output`, `interrupt_during_output` or
  `not_an_interruption`. Only the first is the case in question; expect to
  need several runs to get one, since a fast service answers inside 300 ms.
- `resume` records `resumed_setup_ms` when setup completes, and times the
  resumed answer from its own question.
- `image` opens the picture once, requires a regular file not reached through
  a link, reads at most 64 KiB plus one byte, and labels its time origin as
  the local queueing of `End`.

Your `handles` result (none after setup, one after an answer, current) is
recorded in the handoff as reported by you. `BarrierPolicy` stays `Require`.

Host, source-only export of `d938a3101`:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 882 passed, 0 failed, 4 ignored |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |

`lamp-gemini` alone: 154 passed (136 unit, 6 setup, 12 codec), 2 ignored.

Not run: Linux/ARM64 for `d938a3101` (the tree at `5c9eff593` passed its
emulated ARM64 lint, release build, 148 Gemini tests and 44 worker tests);
the long ignored tests; any cloud call.

## Checkpoint 4: two requests from Main

`MAIN-008` and `MAIN-007`, read from `main.md` at `51bd815e9`.
[Handoff](../docs/gemini-session-reliability.md#after-the-third-round-two-requests-from-the-integrator).

| Commit | Files | Content |
|---|---|---|
| `e7b8d24b2` | `crates/gemini/src/recovery.rs`, `lib.rs`, `recovery/tests.rs`, new `recovery/tests/checked.rs` | `MAIN-008`: `Supervisor::try_start_on(session, request)`, `StartRefusal` |
| `f085868f3` | `crates/gemini/src/lib.rs`, `recovery/tests/pressure.rs` | `MAIN-007`: `Error::Backpressure` recovers as a lost connection does |

**`MAIN-008`, checked start.**

```rust
pub fn try_start_on(&self, session: SessionId, request: RequestId)
    -> Result<(), StartRefusal>;
pub enum StartRefusal {
    NotConnected,
    WrongSession { live: SessionId },   // a connection is ready; not the one named
    Refused(Error),                     // the named one refused, as try_start would
}
```

The check is made before anything is queued. A refusal sends nothing,
supersedes no earlier answer and spends no request number. `session` is the
one in `LinkUpdate::session` of the `Ready` the coordinator selected.
`try_start` is unchanged; audio and `End` were already bound to the exact
session and request that started. Your second regression (a legitimate lost
request before `Started`) is on your side and I have not touched it.

**`MAIN-007`, playback backpressure.** `Error::Backpressure::recovery()` is
now `Reconnect`. The stuck connection is abandoned with what it still held;
what had reached the 16-event queue is delivered as an unbroken beginning; the
request is reported lost exactly once; a replacement is dialed with the handle
inside the existing attempt, budget and recovery-rate bounds; nothing is
replayed. No resumable context, authentication and quota remain terminal.
Input that overruns the 32-command queue is the same error and recovers the
same way, with the request lost at `InputOpen`. `Fault::from(Error::Backpressure)`
in your worker is still `Local`, and under `Legacy` the worker still ends on
the lost request, so nothing changes in production until you enable it.

One thing to know when you write the coordinator side: the delivery bound is
only reached by an answer long enough that the service's own deadlines come
later (generation complete, more than about 15 s of audio unplayed). A short
answer whose speaker stops is ended by the stall or completion bound instead,
which were already recoverable. Both arrive as one lost request.

Regressions: `start_chosen_for_a_replaced_connection_is_refused_before_anything_is_sent`,
`checked_start_never_reaches_a_connection_other_than_the_one_named`,
`a_stopped_speaker_past_the_delivery_bound_costs_the_answer_not_the_session`,
`a_stopped_speaker_still_ends_a_session_that_cannot_resume_its_context`,
`input_that_overruns_its_queue_loses_that_request_and_recovers_the_link`.

Host, source-only export of `f085868f3`:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 876 passed, 0 failed, 4 ignored |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |
| `cargo test --locked --offline -p lamp-gemini --lib soak -- --ignored --test-threads=1` | 2 passed: both prolonged conversations, including the adversarial one through both interfaces |

Not run: Linux/ARM64 for checkpoints 3 and 4; the real-time 60 s, 180 s and
stopped-playback tests (the stopped-playback ones are on the proposal branch
and assert the `Legacy` outcome, which is unchanged); any cloud call.

## Checkpoint 3: visual context on the Gemini connection

Item 4 of my list. [Handoff](../docs/gemini-visual-input.md).

| Commit | Files | Content |
|---|---|---|
| `2db38361b` | `crates/gemini/src/{lib,wire,session,recovery,testing}.rs`, `session/tests.rs`, new `session/tests/images.rs`, `recovery/tests/pressure.rs` | `InputSender::try_image`, `Supervisor::try_image`, `image_ledger` |
| `2b5878534` | `crates/gemini/examples/service_probe.rs` | `service_probe ... image PICTURE.jpg` |
| `5085223af` | new `docs/gemini-visual-input.md` | Handoff |

Behavior: a JPEG of at most 64 KiB, at most 2 per request, at most 2 s old,
is accepted only for the request that is speaking now. It waits in one slot
beside the audio queue (newest wins), is sent as `realtimeInput.video` inside
that request's activity after audio already queued, and is dropped unsent if
the request ends or is cancelled first. Audio offered during the write is
held in order. Nothing is carried to a replacement connection. A write that
cannot complete in 400 ms fails the connection as `WriteTimeout`.

`Event`, `Error`, `Timeouts`, `ProviderInput` and `ProviderOutput` are
unchanged: `provider_worker.rs` matches the first two exhaustively and
compiles as it is. Outcomes are read from `image_ledger()` instead of a new
event. Nothing calls `try_image` yet.

Host, source-only export of `5085223af`, same target directory as the
checkpoint 2 run:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 871 passed, 0 failed, 4 ignored |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |
| `cargo test --locked --offline -p lamp-gemini --lib image -- --nocapture --test-threads=1` | 7 passed |

Not run: Linux/ARM64 for this checkpoint; the four ignored long tests; any
cloud call. Not established: that the service uses a frame sent inside a
manual activity, and what an 87 KiB message costs on the real link. The image
probe in `A1-004` answers both.

## Checkpoint 2: echo evidence and failure notices

Items 2 (the evidence part) and 5 of my list in the
[completion plan](../docs/rust-completion-plan.md).

### Commits and files

| Commit | Files | Content |
|---|---|---|
| `6baa55356` | new `crates/audio/src/evidence.rs`, `crates/audio/src/lib.rs`, new `crates/audio/tests/evidence.rs` | `EchoObserver`, and `EchoProcessor::evidence()` |
| `0b3cd0c3a` | `crates/audio/src/replay.rs` | `lamp-audio replay-aec` writes the evidence on every `capture_pair` line |
| `09c11f22b` | new `docs/echo-evidence.md` | Handoff |
| `b9360bc71` | new `crates/audio/src/notice.rs`, `crates/audio/src/lib.rs`, `crates/audio/src/main.rs`, new `crates/audio/tests/notice.rs` | Failure-notice loader and `lamp-audio notices DIR` |
| `0f688994d` | new `docs/failure-notices.md` | Handoff and provisioning contract |

No shared file, wire type, manifest or lock file is touched.

### Behavior

- **Echo evidence.** For every capture block `EchoProcessor::evidence()` now
  reports the capture level before and after cancellation, how much of the
  last 500 ms carried non-silent accepted speaker PCM and, once capture and
  render are aligned, the percentage of the capture's energy and of the
  cancelled signal's energy that the render explains. Measurements only; no
  alignment, no claim. Nothing consumes it yet: `A1-003`.
  [Handoff](../docs/echo-evidence.md).
- **Cost added to the capture path:** one observer update per render block
  and per capture block. 9.0 µs per capture block on the host in a release
  build. Not measured on ARM or the device yet.
- **Failure notices.** `lamp_audio::notice::Notices::load(dir, approval)`
  reads a provisioned directory once, validates each recording against its
  manifest entry (transcript, voice, revision, approval, hash, exact length,
  mono 24 kHz PCM16) and afterwards answers whether a notice is available or
  why not. It never synthesizes, downloads or converts, and it plays nothing.
  A test asset is refused by a production caller.
  [Contract](../docs/failure-notices.md).

### Checks

Host: macOS x86_64, Rust 1.96.0, source-only exports outside the repository.

On `09c11f22b`, fresh target directory:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 857 passed, 0 failed, 4 ignored |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |
| `cargo test --locked --offline --release -p lamp-audio --test evidence -- --nocapture --test-threads=1` | 8 passed; 9.0 µs per block |

On `0f688994d`, which adds only the notice module and its document:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | pass |
| `cargo test --locked --offline -p lamp-audio -- --test-threads=1` | 73 passed, 0 failed |
| `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | pass |

The whole-workspace test run was not repeated for the two notice commits. The
four ignored tests (two prolonged conversations, the 60 s and 180 s answers)
were not rerun for this checkpoint; nothing they exercise changed.

Evidence on scripted scenes through the real canceller, host:

| Scene | Result |
|---|---|
| Playback alone | Matched 166 ms after the first echo; 94 percent explained; none of 500 blocks below 50 |
| A person alone | Never matched |
| A person over playback at -6, 0, +6 dB relative to the echo | 76, 48, 19 percent, against 75, 47, 19 predicted |
| Half the echo left in the cancelled signal (scripted residual) | 94 percent with nobody speaking; 47 with a person as loud as the leak; 1 with no leak |
| Microphone clock 300 ppm fast for 20 s | Followed; 91 percent |

Linux/ARM64, for checkpoints 1 and 2 together: `0f688994d`, source-only
export mounted read-only, cross-compiled in a Debian bookworm container (Rust
1.96.0, `aarch64-linux-gnu-gcc` 12.2), test binaries run under `qemu-aarch64`
user-mode emulation. **Not the device**: no device timing, ALSA hardware or
audio.

| Command (each with `--locked --offline --target aarch64-unknown-linux-gnu`) | Result |
|---|---|
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo build --release -p lamp-live -p lamp-voice-eval` | pass; `lamp-live` is ELF 64-bit ARM aarch64 |
| `cargo build --release -p lamp-gemini --example service_probe` | pass; ELF 64-bit ARM aarch64 |
| `cargo test --release -p lamp-audio -- --nocapture --test-threads=1` | 99 passed, 0 failed |
| `cargo test --release -p lamp-gemini -- --test-threads=1` | 136 passed, 0 failed, 2 ignored |
| `cargo test --release -p lamp-live --lib provider_worker -- --test-threads=1` | 44 passed, 0 failed, 2 ignored |
| `cargo test --release -p lamp-gemini --lib prolonged_adversarial -- --ignored --nocapture --test-threads=1` | pass; every tally identical to the host run, both interfaces |

The audio count is higher than the host's 73 because the Linux-only capture
and speaker suites run there. The evidence scenes give the same numbers as on
the host. The observer costs 60.0 µs per capture block under emulation, which
says nothing about the board. The `MEASURED` lines carry a fixed "host"
suffix in the test source; in this run they come from the emulated ARM64
binaries. Not run on ARM64: the whole-workspace suite, the debug profile, the
60 s, 180 s and stopped-playback real-time tests.

### Not run, and not established

- Cloud and device: nothing.
- **No threshold.** No recording with its render reference is on this
  machine, so nothing says at which share a turn should be refused. The
  scenes are a delayed, inverted copy with one reflection, not a room.
- The recorded false interruption is not reproduced: the canceller removes
  10 dB of this synthetic echo before the first match. A scripted residual
  stands in for an unconverged canceller.
- No notice is recorded, worded, approved or provisioned, and nothing plays
  one.

## Checkpoint 1: recovery under a reader that does not read

Published at `d0c7c53f1`; code `155bab4e2`.
[Handoff](../docs/gemini-session-reliability.md#third-round).

| Commit | Branch | Content |
|---|---|---|
| `67154e0b6` | `gemini-voice` | Fix in `Supervisor::maintain`: a refused replacement attempt is retried while a link update is unread. Five regressions |
| `155bab4e2` | `gemini-voice` | Adversarial conversations through `next()` and through `poll_link()` + `next_output()` |
| `548fda500` | `gemini-voice-worker-proposal` | Seven ignored tests in `provider_worker.rs` measuring stopped playback: `A1-001` |

Host, source-only export of `548fda500`: formatting and strict Clippy pass;
849 passed, 0 failed, 11 ignored; release build passes; the two prolonged
conversations pass in 553 s; the 60 s and 180 s answers pass (70.03 s and
200.05 s with their holds); the seven stopped-playback trials pass.

| Playback stopped for, every holder full | Result |
|---|---|
| 25, 27, 28 s | Complete: 90 of 90 s delivered |
| 29, 30, 31, 33 s | `Backpressure`; 58.64 of 90 s delivered; the worker ended 33.7 s after playback resumed |

Known gaps: a consumer that reads no link update for longer than the outage
budget still loses the link if a replacement dies on arrival; a stopped
speaker past 28 s ends the session and the failure shows only after playback
resumes (`A1-002`); under `Require` a person who continues before the answer
starts ends the session after 2 s if the service never confirms.

## Requests

### A1-001 — Main: take or reject the stopped-playback tests

`548fda500` on `origin/gemini-voice-worker-proposal` adds seven ignored tests
to the `mod tests` of `provider_worker.rs` and nothing else. Cherry-pick it or
tell me what to change; I will not touch that file on `gemini-voice`.

### A1-002 — closed by `MAIN-007`

Decided: the answer, not the session. Done in `f085868f3`; see checkpoint 4.

### A1-003 — Main: one optional field for echo evidence

Now concrete. Please add to `MicrophoneFrame` in `wire.rs`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub echo: Option<lamp_audio::evidence::EchoEvidence>,
```

and `echo: None` in the test helper at `coordinator.rs:2150`. I then set it in
the capture worker from `echo.evidence()` right after `echo.capture(...)`:
one line in `audio_workers.rs`, which is mine. The value is five small
integers plus an optional five, and describes the frame it travels with, so
that frame's freshness and lineage rules cover it. Until the field exists the
capture worker computes the evidence and drops it. If you would rather I send
the wire change as a proposal branch, say so.

**On hold since checkpoint 11.** The evidence is not ready to gate
admission, and the list that stood here was wrong in its second line.
Corrected, with no threshold implied:

- `echo` present with `residual_playback_floor` high: the cancelled signal
  the speech detector fired on is mostly Lamp's own voice.
- `residual_playback_floor` low or zero, or `residual_explained` low: not
  determined. **Never evidence of a person.** I had written that a low
  `residual_explained` meant something the render does not explain was
  there. On the device it is low with nobody speaking.
- `echo` absent with `render_active_blocks > 0`: undetermined. Not "no echo".
- `echo` absent with `render_active_blocks == 0`: Lamp is not speaking.

### A1-004 — Main: service probes you can run on a board

The tool is `crates/gemini/examples/service_probe.rs`. It opens no microphone
or speaker, prints one JSON line of event kinds and millisecond timings, and
prints no transcript, audio, handle or credential.

```sh
cargo build --locked --offline --release --target aarch64-unknown-linux-gnu \
    -p lamp-gemini --example service_probe
# target/aarch64-unknown-linux-gnu/release/examples/service_probe
service_probe /absolute/private/provider.json QUESTION.wav PROBE
```

- `provider.json`: the runtime's own fields (`endpoint`, `model`, `voice`,
  `credential_file`, optional `authentication`, `thinking_level`,
  `language_code`). It and the credential file must be regular files with mode
  0600; anything else is refused before any connection.
- `QUESTION.wav`: 16-bit PCM, any rate and channel count, holding 0.5 to 30 s
  of speech after silence is trimmed. `service_probe x QUESTION.wav fixture`
  is offline and prints what would be spoken.
- Bounds per probe: at most three spoken questions, at most two connections,
  and a fixed observation window.

| `PROBE` | Longest run | What to read in the output | What it decides |
|---|---|---|---|
| `interrupt-before-output` | about 45 s plus twice the question | Events are timed from the second `activityStart`. Does `interrupted` or `turn_complete_idle` for request 1 appear, and how many ms before `first_audio` for request 2? Or `no_confirmation_within_barrier` (the probe waits 5 s where production waits 2)? Any `first_audio` for request 1, or a `dropped:` line? | Whether `Require` can survive a person who continues before the answer starts |
| `handles` | about 45 s | `handle_after_setup`, `handle_after_one_answer`, `handle_current` | Whether recovery is available as configured today (`Resumption::Retain`) |
| `handles-requested` | about 45 s | The same three fields, or a setup error | Whether to switch to `Resumption::Request` |
| `resume` | about 90 s | `handle_issued`; `resumed_setup` is `accepted` or an error category; `resumed_setup_ms`; `first_audio` on the resumed session | Whether a reconnect can keep the conversation at all |
| `idle-close` | 300 s | `go_away` and `connection_ended:...` with their ms, or `observation_window_ended` | The idle-close time and code assumed from V1 (86 to 198 s, 1008) |

`image` takes a fourth argument and is the one probe that prints the model's
words, because nothing else shows whether the picture was used:

```sh
service_probe /absolute/private/provider.json QUESTION.wav image PICTURE.jpg
```

`PICTURE.jpg` is a regular JPEG of at most 64 KiB. Longest run about 45 s.
Read `sent` (1) and `dropped` (0), `image_written_in_ms`, `image_waited_ms`,
the event timings from `activityEnd`, and `answer`. Use a picture and a
question fit to appear in a log, such as one coloured object and "What colour
is the thing I am holding?". It decides whether a frame inside a manual
activity reaches the model, and whether 64 KiB and 400 ms hold on this link.

Please record the output lines verbatim with the endpoint and model named,
and nothing from the provider file.

### A1-005 — Main: wiring for failure notices

`Notices::load` wants a directory from configuration and
`Approval::Production`. `capability()` is serializable for a status report.
Everything about speaking one is yours:
[for the integrator](../docs/failure-notices.md#for-the-integrator). Tell me
if the three kinds (`provider_unavailable`, `turn_lost`, `service_refused`)
are not the ones your outage handling distinguishes.

### A1-006 — Agent 2 and Main: the frame handed to the provider must be small

Answer to section 3 of `docs/agent2-interfaces.md`, read at `0239ac27c`. I
agree with the provider as the `context` consumer, the grant and age checks
where the frame is submitted, and a control naming the owner. I disagree with
"original JPEG bytes". A camera frame is 1280x720 MJPG up to 2 MiB. One
WebSocket carries audio and images in order, each client message is bounded
at 256 KiB, and every write has a 200 ms deadline, so an image delays the
audio behind it by its own transmission time and a large one fails the
connection. I intend to accept, per request, at most 2 JPEG images of at most
64 KiB each, no older than 2 s at submission, through their own single slot
(newest wins) so they can never fill the audio queue. The reduction has to
happen before the provider: the perception worker already decodes frames. Not
implemented yet; object now if 64 KiB cannot hold a useful frame.

### A1-007 — Agent 2 and Main: the audio device seam

Answer to section 4 of `docs/agent2-interfaces.md`. I accept the `PcmPort`
request. Proposed shape, not yet implemented; object before I build on it:

```rust
// crates/audio/src/device.rs, compiled on every unix target
pub struct DeviceFault { pub errno: i32, pub call: &'static str }
pub enum DeviceState { Setup, Prepared, Running, XRun, Draining, Suspended, Disconnected, Other }
pub struct CaptureStatus { pub state: DeviceState, pub monotonic_us: Option<u64>,
                           pub available: i64, pub delayed: i64 }

pub trait CaptureDevice: Sized {
    fn open(name: &str, config: CaptureConfig) -> Result<(Self, CaptureFormat), CaptureOpenError>;
    fn start(&mut self) -> Result<(), DeviceFault>;
    /// At most one nonblocking read. A would-block is a fault with EAGAIN.
    fn read(&mut self, into: &mut [i16]) -> Result<usize, DeviceFault>;
    fn status(&mut self) -> Result<CaptureStatus, DeviceFault>;
    fn discard(&mut self) -> Result<(), DeviceFault>;
}
pub trait PlaybackDevice: Sized {
    fn open(name: &str, clock_mode: bool) -> Result<(Self, PcmFormat), PlaybackOpenError>;
    fn avail_delay(&mut self) -> Result<(i64, i64), DeviceFault>;
    fn start(&mut self) -> Result<(), DeviceFault>;
    fn write(&mut self, samples: &[i16]) -> Result<usize, DeviceFault>;
    fn drain(&mut self) -> Result<(), DeviceFault>;
    fn prepare(&mut self) -> Result<(), DeviceFault>;
    fn discard(&mut self) -> Result<(), DeviceFault>;
    fn state(&self) -> DeviceState;
}
```

- `MicrophoneOn<D: CaptureDevice>` and `SpeakerOn<D: PlaybackDevice>` hold
  everything that is there today: guards, ownership ledger, epochs, timing
  and deadlines. `Microphone` and `Speaker` stay as the names of the ALSA
  ones, so no existing call site changes.
- The ALSA implementations move behind the traits, Linux only, calling
  exactly what is called now. `CaptureError` carries a `DeviceFault` where it
  carries an `alsa::Error` today.
- The capture and speaker loops in `audio_workers.rs` become generic over the
  device, with the ALSA entry points unchanged.
- A scripted device for tests lives in `lamp_audio::testing`: a capture side
  fed PCM at a set rate with injectable faults and stalls, and a playback
  side that accepts and drains at a set rate and reports what it accepted.
  Agent 2's room model can sit on top of it or replace it.
- Main: `crates/live/src/lib.rs` decides whether the audio workers compile
  off Linux. If they stay Linux only, their loop tests run in a Linux
  container; say if you would rather open them to every unix target.

Agent 2 acknowledged this at `6699284d4` and will review the shape.

### A1-008 — Main: how may the sensor worker reach I2C?

Needed before I start environmental acquisition (item 6, last in my order).
The contract in `docs/hardware-contract.md` is plain I2C: open `/dev/i2c-N`,
select the address with one `ioctl`, then write and read. The workspace has
no I2C code, denies `unsafe`, and `Cargo.lock` has no I2C crate. Three ways:

1. Add `i2cdev` (rust-embedded, safe API) to `crates/environment`. This is
   what `crates/ring` does for SPI with `spidev`. It changes `Cargo.lock`,
   which is shared, and needs the crate in the offline registry.
2. One audited `#[allow(unsafe_code)]` wrapper around that single `ioctl`,
   using `rustix`, which is already locked. No new package.
3. Something you already have in the motor or control work that I should
   reuse.

I prefer 1 for the same reason the ring uses a maintained crate. Tell me
which, and whether the environmental worker's role name and wire events are
yours to register, as I assume.

Answered in `MAIN-024`: a maintained safe crate is preferred in principle,
no shared dependency change is authorized now, registration is yours, and
acquisition stays deferred behind audio. Closed until you reopen it.

### A1-009 — Main: wire the start gate, or reject it

Checkpoint 9. `lamp_audio::playout::{StartGate, StartNeed, START_HORIZON_US}`
exist and nothing calls them. The patch on `gemini-voice-playout-proposal`
(`b4782eda1`, one file, yours) does three things in `coordinator.rs`:

1. `Reply` gets a `StartGate` and a `StartNeed`; `append_provider_audio`
   tells both of each arrival.
2. `dispatch_speaker_chunk` returns early while `Reply::may_start` is false.
   `next_speaker_chunk` is unchanged.
3. `speaker_first_write` records `start_held_us`, `start_buffered_frames` and
   `start_by_completion`; `turn_finished` records `start_deficit_us` and
   `start_horizon_needed_us`.

The existing test fixtures build their `Reply` with a zero horizon, so their
timing is what it was. One test is added. A second generation inside one turn
keeps the open gate. The fixture provider is gated like any other.

What I need from the device run, whichever way you decide: the two
`turn_finished` fields on every reply. They say what horizon each reply
needed, with or without the gate, and whether 200 ms is the right number.

### A1-010 — Main: may I add a continuation start to the transport?

Checkpoint 10, proposal 2. An interface, so I am asking first.

```rust
supervisor.try_continue_for_session(expected, request) -> Result<(), StartRefusal>
```

For input that goes on with a question which has ended and has produced no
output. No barrier, no withheld `activityEnd`, `BarrierAssumed` reported at
once. Refused, typed, if the earlier request has produced anything; then it
is an interruption and takes the barrier. Whether more speech is the same
question is admission's decision and yours. Without it the device failure of
`MAIN-024` is one lost question and one reconnect every time a person pauses
and goes on before the answer starts, if the service is always as silent as
it was once.

It rests on one property of the service that the probe measures: that an
unanswered generation cut off by `activityStart` does not answer later. If it
does, the owner is still right and the answer is the truncated question's.

**Rejected in `MAIN-028`, and closed.** Your reasons, as I read them: a late
answer to the truncated question could be delivered in place of the complete
question's, and a common owner does not make that correct; no output yet
does not prove the same intent. `Require` and one lost question stay. I am
not building it, not waiving input freshness and not replaying input. The
probe and the regressions stay.

### A1-011 — notice: the echo evidence is silent on the first real recording

`A1-003` should wait for this. I replayed the first Rust pilot's certified
recording offline with `lamp-audio replay-aec`. Host, no device.

| | |
|---|---|
| Capture blocks | 4,001 |
| Blocks with the render active | 157 |
| Blocks in which the observer reported a match | **0** |
| Capture level before the reply, median | -77 dBFS |
| Capture level during the reply | -7.5 to -17 dBFS |

The lamp's own voice is plainly in the microphone and the observer says
"not determined" throughout. Measured outside the observer, at its own
analysis rate: one delayed copy of the render, at 104 ms, explains 30 % of
the capture over the reply, and 50 % or more in 27 of 106 windows. Three taps
explain 50 % or more in 95 of 106 windows, median 64 %. Twenty-one taps
explain 72 %.

The observer searches with one tap and needs 50 % to adopt an alignment; the
three-tap fit is only used afterwards. My synthetic rooms were a direct path
and one reflection, which one tap explains. The real path is spread over
about five milliseconds. The module keeps its promise that absent is never
"no echo", but as built it would be absent on the device. I am fixing the
search next and will test it against this recording, which stays outside Git.

Fixed in checkpoint 11. Kept inactive for admission, as you said.

## Notes for peers

- The podman machine `lampos-arm64` on this Mac is shared. I start it for my
  ARM64 runs and, since `MAIN-024`, leave it running. My containers are named
  `agent1-*` (one earlier run today was `lampos-arm64-verify13`) and use the
  volumes `lampos-arm64-target`, `-cargo` and `-git`.
- Agent 2, read at `6699284d4`: `A1-006` agreed (reduced context frames of
  at most 64 KiB and 640 pixels from the perception worker, with the original
  frame identity and grant); `A1-007` acknowledged.
- Main, read at `51bd815e9`: `A1-002` decided (`MAIN-007`); `A1-003`
  accepted, with the wire migration yours; visual context bounds accepted
  provisionally. `A1-001`, `A1-004` and `A1-005` are open. You built
  `service_probe` from `64806f862`; the `image` probe needs `2b5878534` or
  later.

## Answers

- `MAIN-001`, read at `666693a84`: acknowledged. I fetch the three branches
  at startup, on a checkpoint notice, before a shared-interface change and
  after publishing.
- `MAIN-002`, read at `666693a84`: checkpoints 1 and 2 are published above.
  Order of remaining work: audio device seam with stall and restart tests,
  then environmental acquisition last.
- `MAIN-007`: done, checkpoint 4.
- `MAIN-008`: done, checkpoints 4 and 5, under the name you bind.
- `MAIN-010`: read from your file. Acknowledged here; see "Harness team" at
  the top for why not there.
- `MAIN-011`: probe corrections in `d938a3101`; your results recorded as
  yours.
- `MAIN-012`: fixed in `ee5387da7`.
- `MAIN-017`, read at `dccc0fa32`: acknowledged. I am not bypassing the team
  guard. The probe's remaining open race is closed in `a77bdb0fd`.
- `MAIN-019`: fixed in `bb882e024`.
- `MAIN-022`: checkpoint 9. A recommendation, a gate, fifteen recorded
  schedules as a regression, and a patch for your file on a separate branch.
- `MAIN-024`: checkpoint 10. Sensor acquisition deferred, no dependency
  change, registration yours: understood. No device, cloud or credential
  action taken. The shared VM is left running.
- `MAIN-028`, read from the team channel: `A1-009` under your review and
  kept separate; `A1-010` rejected and closed; `A1-011` fixed in checkpoint
  11 and inactive for admission. Your second NS-off reply is in the playout
  replay.
- `MAIN-031` and `MAIN-032`, read from the team channel: checkpoint 12.
- `MAIN-033`, read from the team channel: checkpoint 13.
- Device ownership: yours. I take no device action.
