# Agent 1 status

Owner: Agent 1 (audio and Gemini). This file on
`lamp-a1-continuation-retain` is the status of `MAIN-056` with the kept
start added; the same file on `lamp-a1-continuation` (`9ec2f8cfe`, left
as it was for your review) is the status without it. Reference recovery (`MAIN-053`, `054`, `058`) is
on `lamp-a1-reference` at `a43b44b80` with its own status there. Duplex
(`MAIN-050`) is frozen on `lamp-a1-duplex` at `e2aa5df1c`.

- Updated: 2026-10-11, 21:00 UTC
- **`MAIN-056`: the lost question now has a fix that does not guess, built,
  opt-in and tested against the in-memory service: the next request's
  start and opening words are kept until the earlier answer begins or
  completes.** It has not met the real service. See
  [The kept start](#the-kept-start-built-on-this-branch).
- The service's documentation gives no confirmation for a response
  interrupted before output. The exact missing guarantee and what the
  person gets without it are [stated below](#the-missing-guarantee).
- No cloud and no device from me. Every service result below is yours.
- Two faults of mine in the probe, both found by you, both fixed:
  [below](#my-faults-here).

## Branches

Three, so that each can be taken or left by itself. You have taken the
second: `e9bdcea28` on `lamp-v2-chat` carries its three files.

| Branch | Head | Base | What |
|---|---|---|---|
| `lamp-a1-continuation-probe` | `7ab7e30b9` | your `235f50297` | Example only. During-output mode, the observation bound, the exit rule |
| `lamp-a1-stop-queued` | `53f303c7e` | your `55d7af98e` | Six commits. `5479f9b2b` two regressions that fail, `9c6c1637a` the queue guard; then, after your review, `2bec36350` three more that fail, `e70324912` the fence, `2fd77a417` its document, `53f303c7e` your wording corrections. Nothing to do with continuation |
| `lamp-a1-continuation-retain` | this commit | `9ec2f8cfe` | The kept start: `c80edfa91` library and tests, `76520cb90` probe setting, `270a91412` the proposed `provider_worker.rs` wiring, switched off, `901622831` document and the Lamp case script |
| `lamp-a1-continuation` | `9ec2f8cfe`, untouched | the probe branch and your `e9bdcea28` merged in | The library: `BarrierWait::AtCommit` and barrier reports, their tests, the probe's settings for them, the document. One separate proposal commit for `provider_worker.rs`, `a25e68a52` |

## What the service did, twice

Your two runs, one attempt each, synthetic cached speech, no microphone or
speaker. Times are since the first start was queued.

| | `continue-before-output` | `continue-during-output` |
|---|---|---|
| First `End` queued | 5.027 s | 4.984 s |
| Service activity notice `End` | 5.602 s | 5.552 s, audio offset 4.334 s |
| First audio of request 1 | none | 6.412 s, 1.428 s after its End |
| Second `Start` queued | 5.330 s, 303 ms after the End | 6.413 s, 0.8 ms after that audio |
| Service activity notice `Start` | 5.691 s | 6.771 s, audio offset 4.334 s |
| `interrupted` for request 1 | none exposed | 6.771 s, 358 ms after the Start and 23 µs after the notice |
| Idle completion for request 1 | none exposed | 6.776 s, 363 ms after the Start |
| Outcome | `BarrierTimeout` at 7.331 s, 27,680 of 58,671 samples uploaded, End never written | Settled. End written 10.615 s, first audio 11.752 s, idle 13.956 s |

Read with care:

- One attempt each. Neither is a rate.
- The first run shows no terminal **exposed** for request 1 in the 2.0 s
  the session lasted after the second Start. It does not show that none
  was sent, or that none would have come later.
- The activity notices are the service's own. In the second run their
  audio offsets equal the uploaded audio exactly (69,337 samples is
  4.334 s) and the `Start` notice arrives with `interrupted`. That is an
  observation about two runs. They are not acknowledgements of this
  client's writes and nothing here treats them as settlement.

## What is built

All in `crates/gemini`, all off by default, policy `Require` throughout.
The document section is "Where a barrier's wait starts, and what a barrier
reports" in `docs/gemini-session-reliability.md`.

1. **`SessionConfig::barrier_wait(BarrierWait::AtCommit)`.** The quiet
   bound counts only while a request's `activityEnd` is withheld, from no
   earlier than that. The absolute bound, `Timeouts::response` from the
   interruption, is kept. What is committed, when, and whose the output is
   are unchanged.
2. **`SessionConfig::barrier_reports(true)`.** `Event::Barrier` when a
   barrier settles or times out, with counts of what arrived for the
   earlier response and was dropped. Counts only.

On its own branch, `lamp-a1-stop-queued`, because it is not opt-in and is
not about continuation:

3. **A stop wins over input queued behind it.** The session loop checks
   for a stop at the top of each turn, then takes whichever of its ready
   work comes up, in a random order. A command already in the queue when
   the stop was recorded could be taken first and written: one block of
   microphone audio, or an `activityEnd`. At most one, and then the stop
   took effect. The image path already refused this; the main loop did
   not. Before the fix both regressions failed in 8 runs of 8, each
   showing a queued audio block on the wire after the stop. With it they
   passed in 25 runs of 25, each run being two tests of 40 trials.
   **This changes default behavior**, in the direction of writing less.
4. **A stop is a fence on every write.** Your review of 3 showed two
   paths it did not cover: an End held behind a barrier was still written
   when the stop was recorded as the releasing completion was read, and a
   write already waiting on the transport completed after the stop. Now
   every write after setup, the image path apart, goes through one
   `Actor::write`: none begins after a stop, one that is waiting is
   abandoned, and a message read after a stop is not acted on. Limits, as
   the document states them: bytes the transport had accepted, a whole
   frame included, cannot be recalled; a poll in progress on another
   thread may finish its one message; nothing below this session's
   transport is controlled or measured. On your gates with the
   expectations inverted: red before, 3 runs of 3; with the fix, the
   seven stop tests green 20 runs of 20. No timeout value changed.

### The regression list you gave

| Case | Test | |
|---|---|---|
| Long successor upload | `a_person_still_speaking_is_not_cut_off_by_the_quiet_bound` | Four times the bound of speech; the wait starts at their End |
| Pause and resume before audio | `a_pause_in_the_next_utterance_is_not_the_wait_and_its_end_starts_it` | Three times the bound of silence inside the utterance |
| Repeated continuation | `a_second_continuation_restarts_the_wait_at_its_own_end_and_one_end_is_written`, `a_second_continuation_that_never_settles_times_out_from_its_own_end` | One `activityEnd` in all, never the superseded request's |
| Old output arriving | `a_terminal_while_the_end_is_withheld_commits_it_and_old_output_is_never_the_new_requests`, `old_output_behind_a_barrier_is_counted_and_never_kept_or_delivered` | 300 frames counted, none delivered |
| No terminal | `a_person_still_speaking…` ends in `TimedOut`; `the_absolute_bound_still_ends_a_barrier_nothing_settles` | End never written |
| Final terminal | `a_terminal_during_the_next_utterance_settles_and_its_end_commits_at_once` | |
| Disconnect | `the_service_closing_under_a_withheld_end_is_that_close_and_no_barrier_outcome` | The close code, no invented outcome |
| Cancellation | `a_cancelled_successor_leaves_nothing_waiting_and_a_later_one_waits_strictly`, `an_end_withheld_and_then_cancelled_is_never_written_and_nothing_waits` | |
| Stop or privacy under queued input | `a_stop_under_a_withheld_end_is_the_stop_and_not_a_timeout`; from the stop branch `a_stop_with_input_queued_behind_it_writes_none_of_that_input`, `a_stop_with_input_queued_behind_a_barrier_writes_none_of_that_input`, and the five `stop_fence` tests | Two defects found and one more shown by your review: see 3 and 4 above |
| Bounded memory and time | Old output is counted in fixed-size counters and dropped; the successor's audio is written through and never held; the absolute bound is unchanged | |
| Defaults unchanged | `the_same_person_is_cut_off_as_before_without_the_setting`, `without_reports_the_event_stream_is_what_it_was` | |

These run against the in-memory service in `crates/gemini`. None of them
is a run on the real service, and none goes through `provider_worker`.

## What the protocol does not give

**A response interrupted before it has produced anything gave no sign
that it had stopped.** With manual activity control this client has two
confirmations and no others: `interrupted` and the idle completion. In
your second run both came promptly for a response that was producing. In
your first, for one that was not, neither was exposed.

If that holds on repetition, then for a continuation before output:

- Waiting longer, or starting the wait later (`AtCommit`), does not make
  it succeed. It makes the failure come after the person has finished,
  with a report saying nothing arrived.
- Any design that commits the second request anyway is a guess about
  whose the next output is. That is `AssumeAfterQuiet`, and it stays out.

What does not guess, and what each costs:

| | How | Cost, and what it needs |
|---|---|---|
| **Hold the interruption until the earlier response produces** | Keep the second `activityStart` and its audio back until request 1's first output, then interrupt a producing response, which your second run shows being confirmed | The audio waits, and it is not fresh to begin with. In the one run that shows it, first output came 1.43 s after the End, so a person who resumes 0.3 s after the cut has their audio held about 1.1 s. `MAX_INPUT_AGE` is 1 s and audio older than that is refused; a request opens with retained pre-roll that keeps its original read time, so it is already aged when the request starts. So this needs audio well over a second old to be uploaded: the 1.1 s of that example plus the age of the pre-roll, which I have not measured. **That is a production bound and I have not touched it.** Also unprobed: an interruption followed at once by a burst of held audio |
| **Leave the connection** | On a continuation before output, open a new session for the second request; ownership is by session and absolute | A reconnect inside the utterance, and the model's context is whatever the handle restores: the cut-off first part may not be in it. Unprobed. Mostly `provider_worker` |
| **Do not commit early** | Keep the speculative early endpoint off | It is off. A person who resumes in the 1.4 s between an ordinary endpoint and the first audio still meets this |

I have built none of the three. The first is the one I would build, and
it cannot be built inside the 1 s bound: with the bound kept it fails on
the opening audio of exactly the requests it is for. Widening it is not
mine to decide.

## The kept start, built on this branch

`SessionConfig::successor_start(SuccessorStart::AfterFirstOutput {
patience })`, off by default. The whole of it, with the rule, the
bounds, the ownership argument and the tests, is the section "Keeping the
next request's start until the earlier answer begins" in
`docs/gemini-session-reliability.md`. In short:

- A request that starts while a committed request's response has produced
  nothing has its `activityStart` and its audio kept in the session, in
  order. The earlier response's first output ends that with an
  interruption of a response that is producing; its idle completion ends
  it with nothing interrupted. Then the kept words follow, and the End
  commits once the earlier response's idle completion is in.
- It changes when bytes are written and never which. No committed input
  is replayed. Nothing is assumed after a quiet time; `AssumeAfterQuiet`
  is not involved.
- Bounds: patience 1 ms to 4 s, set by the caller; 6 s of audio kept;
  kept audio written at most `MAX_INPUT_AGE` plus the patience after it
  was captured, which is the one deliberate widening, for audio this
  library itself held; 30 s from supersession to settlement, unchanged.
- Past patience or the buffer the start is written anyway and the default
  path applies, including its strict timeout. That failure is reported as
  what it is, with how long the start was kept and what ended it.
- A stop, a close or any failure drops what was kept, unsent.
- A request cancelled while kept is never committed. What the session had
  already taken from it is still written in order, as on the default
  path: the session cannot tell a cancellation from a continuation, and
  guessing wrong would lose words.

### The missing guarantee

What the documentation would have to say for strict continuation to be
safe without this, and does not:

> that a generation cancelled by a client's `activityStart` is always
> reported by `interrupted` followed by `turnComplete` with an idle
> `interactionStatus`, including when it had produced no output; or that
> the service acknowledges which client message it has consumed.

Neither is in the Live API reference or guide as read on 2026-10-11.
The kept start avoids needing the first by never interrupting a response
that has produced nothing. It still needs the part that is documented
and was seen once: that a response interrupted while producing is
confirmed.

What the person gets:

| Situation | With the kept start | Without it |
|---|---|---|
| They go on speaking before the first answer has begun | Their words wait up to the patience, the cut-off answer is interrupted the moment it begins and is never played, and the whole thought is answered. If they finish before that, the answer starts later by the time that was left to wait | The session ends in a barrier timeout and the question is lost |
| The earlier answer never begins and never completes within the patience | The default path: most likely the same timeout, later, and the question lost | The same timeout |
| The service does not confirm an interruption that a burst follows | The same timeout. **This is the case no run has tried** | |

The cut-off first part stays in the model's context with whatever the
service kept of its interrupted answer. Whether the model then answers
the whole thought well is model behavior and is not scored anywhere yet.

## For you

| Id | |
|---|---|
| `A1-021` | Done by you: one during-output attempt, confirmed in 363 ms. Thank you |
| `A1-022` | Still open. `barrier-ms=5000` is now accepted and watches 3 s longer than production. The existing `interrupt-before-output` probe already watches for 5 s under `AssumeAfterQuiet` and then commits, so it also shows what arrives **after** a commit nobody confirmed: one answer or two. As observation only, that is the more useful of the two |
| `A1-023` | Answered by your instruction to build it with explicit bounds. The widening is in the document's bounds table, third row, and is opt-in with the setting |
| `A1-025` | **The Lamp run**: `crates/gemini/examples/service_probe/lamp-cases.sh PROBE PROVIDER.json FIRST.wav SECOND.wav OUTDIR`, seven cases, one connection each, cached synthetic speech, credential read in place. What would qualify the setting is in the document under "On the probe, and on a Lamp". More than one attempt, please |
| | The ARM64 release probe for it is built and waiting, outside Git, in my session scratchpad under `m056/probe-artifact/`: `service_probe` SHA-256 `08a80e49adac086a61258d68b9982917c95d5fd882f0901b004d4c2c47555551`, 3,819,280 bytes, from `901622831`; `source-manifest.txt` SHA-256 `8660e6796d5be70de5454fd3897a71d1fcfa8c152165fb0ae47b12c41ce233df`, 412 files; `lamp-cases.sh` and a `README.txt` with the exact commands beside it. Built by `agent1-arm64-verify33`, 20:57:35Z to 20:57:41Z, alone in the VM. Under emulation it refuses an out-of-range `keep-start-ms` and stops at a missing provider file; it has never opened a connection |
| `A1-026` | **The patience to pilot with.** The script uses 2500 ms: first output came 1.43 s after the End in your one run, and a person who resumes at once waits all of that. It is a setting, not a measurement of mine |
| `A1-024` | Before `AtCommit` is turned on anywhere: the during-output probe once more with `barrier-wait=at-commit`, which needs this branch's library |

A longer observation than 5 s, without widening any bound, is its own
example-only branch, `lamp-a1-continuation-hold` `83e09e504`, on
`9ec2f8cfe` and not on this branch: `hold-end-ms=N` with
`barrier-wait=at-commit` keeps the probe's own second End back, so the
connection stays up to the 30 s absolute bound and records whatever
arrives; it always exits non-zero. I doubt it tells more than the 5 s
runs: I would not expect the service to produce for request 1 while
activity 2 is open. That is an expectation, not an observation. It is
separate so that you can leave it.

## My faults here

- **The probe's `barrier-ms` accepted 10 s and the library refuses more
  than 5.** Your `A1-022` run ended in 0.014 s with `InvalidConfiguration`
  and never reached the service. I had tested the parser and not the
  configuration it feeds. Fixed in `cfdd6e742`: the range is the
  library's, and a test builds a configuration from every accepted bound.
- **The probe exited 1 on the new during-output success label.** Fixed in
  `b01c3054a`: one tested function decides, and an observation-only run
  never exits 0.

## Verification

Code on this branch is verified at `901622831`; later commits change this
file only. `lamp-a1-continuation` is verified at `9ba8883d0`, which is
`b0f21f5a9` with your `e9bdcea28` merged. Against `e9bdcea28` this branch
differs in `crates/gemini` (`config.rs`, `lib.rs`, `wire.rs`,
`session.rs`, the `continuation` and `retain` tests, the probe example
and its case script), in the reliability document, in this file, and by
the proposed lines in `provider_worker.rs`.

**Host**, from a source-only export of each commit, one after the other:
`cargo fmt --all -- --check`, `cargo clippy --locked --offline --workspace
--all-targets -- -D warnings`, `cargo test --locked --offline --workspace
--no-fail-fast -- --test-threads=1`, `cargo build --locked --offline
--release -p lamp-live -p lamp-voice-eval`.

| Commit | Format | Clippy | Tests | Release build |
|---|---|---|---|---|
| `9c6c1637a`, stop branch, queue guard | clean | clean | 1,445 passed, 0 failed, 7 ignored | ok |
| `2fd77a417`, stop branch, fence | clean | clean | 1,450 passed, 0 failed, 7 ignored | ok |
| `b0f21f5a9`, this branch before the fence | clean | clean | **1,458 passed, 1 failed**, 7 ignored | ok |
| `9ba8883d0`, `lamp-a1-continuation` on `e9bdcea28` | clean | clean | 1,480 passed, 0 failed, 7 ignored | ok |
| `901622831`, this branch, the kept start | clean | clean | 1,495 passed, 0 failed, 7 ignored | ok |

The `9ba8883d0` row ran 19:58:10Z to 20:12:02Z and the `901622831` row
20:40:31Z to 20:54:32Z, each with nothing else of mine building on the
host. Two earlier starts of it were stopped by me for your recording
windows and are not counted.

**The one failure**, in the pass of `b0f21f5a9`, kept as it happened:
`provider_worker::tests::twenty_second_answer_through_a_five_second_reply_buffer_follows_capacity`
panicked at `provider_worker.rs:4693:71`, which is the test rig's
`self.parent.data.receive::<Wire>().unwrap()`. I did not keep the message.
I first reported it to you as the capacity counter on the next line; that
was wrong. `Channel::receive` fails on a datagram error, a decode error,
or `ReceiveOrder::accept`, which includes the channel's 100 ms age bound.
Another agent's `cargo test -j2 -p lamp-live --lib failure_notice` was
running on the host when the pass began. This branch changes none of
`transport.rs`, `wire.rs` or that rig.

**Two trees compared, with counters.** Your `55d7af98e` and this branch's
`b0f21f5a9`, each exported to scratch and patched there only, nothing
committed: `Channel::receive` records each packet's age and puts it in its
error, and the rig's playback loop records its longest gap between turns
and can be made to stand still once (`std::thread::sleep`) after its
hundredth packet. One run at a time, the trees alternating, two cargo
jobs at most for the builds.

Unforced, six runs each:

| | Passed | Oldest packet, per run | Longest rig gap, per run |
|---|---|---|---|
| `55d7af98e` | 6 of 6 | 20, 69, 21, 21, 20, 21 ms | 50, 69, 51, 51, 53, 50 ms |
| `b0f21f5a9` | 5 of 6 | 20, 39, 21, 23, 21, 23 ms | **153**, 50, 52, 68, 53, 53 ms |

The run that failed is the one whose rig stood still for 153 ms. Its
output: `lamp-live provider: connection lost (Gemini Live BarrierTimeout)`,
then the rig's heartbeat failing on `ConnectionReset` at
`provider_worker.rs:2824`. The channel and the heartbeat's lease are both
bounded at 100 ms.

One deliberate stall, one run each:

| Stall | `55d7af98e` | `b0f21f5a9` |
|---|---|---|
| 150 ms | **Fails**: the same `BarrierTimeout` line, then `ConnectionReset` at the same heartbeat call (`:2806` there) | **Fails**, identically (`:2824`) |
| 60 ms | Passes, longest gap 71 ms | Passes, longest gap 74 ms |

So a rig that stands still for longer than the 100 ms bound fails this
test in your tree exactly as in mine, and one that stays under it passes
in both. That is what the comparison establishes. What it does not: the
message of the first failure is lost, so that it was this is an inference
from the same test failing after the same kind of stall; and how the
stall becomes a `BarrierTimeout` (my reading: the lease lapses, the worker
retires the request, the session interrupts, and the scripted service
never confirms) I read and did not trace. Gaps of 50 to 70 ms occurred in
every unforced run in both trees, so this test runs with 30 to 50 ms to
spare under its 100 ms bound. Whether those gaps are the host or the
test's own work on its one thread I have not determined.

**Emulated Linux/ARM64**, `agent1-arm64-verify32`, 20:39:06Z to 20:40:25Z,
`901622831`, this branch, nothing else running in the VM: the same steps,
all exit 0, `lamp-gemini` release tests 173 passed, 0 failed, 2 ignored,
with 6, 12 and 23. The 15 `retain` tests are among the 173 and ran in
real time under emulation.

`agent1-arm64-verify31`, 19:41:13Z to 19:42:27Z,
`9ba8883d0`, `lamp-a1-continuation` on `e9bdcea28`, nothing else running in the VM:
the same steps as below, all exit 0, `lamp-gemini` release tests 158
passed, 0 failed, 2 ignored, with 6, 12 and 20.

Earlier, `agent1-arm64-verify29`, 18:19:44Z to 18:20:59Z,
`b0f21f5a9`, before the fence was merged, nothing else running in the VM. All steps exit 0: strict
Clippy `-p lamp-gemini -p lamp-live --all-targets`; `lamp-gemini` release
tests single-threaded, 153 passed, 0 failed, 2 ignored, with 6, 12 and 20
in its other targets; release build of `lamp-live` and of the probe
example, both aarch64 ELF. It is an emulated run, not the device.

**The stop regressions**, on `lamp-a1-stop-queued`. Queue guard: red 8
runs of 8 at `5479f9b2b`, green 25 runs of 25 at `9c6c1637a`, each run two
tests of 40 trials. Fence: at `2bec36350` its three tests red 3 runs of 3;
with that commit's `session.rs` and the final tests, four fail and the
no-stop control passes; at `e70324912` the seven stop tests green 20 runs
of 20. The stop branch's tested code is `2fd77a417`; `53f303c7e` after it
changes comments and the document only, and for it I reran format,
Clippy for the crate and the seven stop tests, not the whole suite.
Emulated ARM64 for the fence: `agent1-arm64-verify30`, 18:53:17Z to
18:54:25Z, `2fd77a417`, alone in the VM, same steps as below, 144 passed,
0 failed, 2 ignored with 6, 12 and 10, all exit 0.

**The probe branch**, `7ab7e30b9`: `cargo test -p lamp-gemini
--all-targets` 137 passed, 0 failed, 2 ignored, with 6, 12 and 17; strict
Clippy for the crate; release build of the example.

## Not done

- The kept start has not met the real service. In particular nobody has
  seen whether an interruption followed at once by a burst of kept audio
  is confirmed.
- No test holds audio long enough to reach the age bound, and none goes
  through `provider_worker`, where the setting is wired and switched off
  by a constant; a session option should replace it before a device run
  that is not the probe.
- The retain tests passed 15 runs of 15 on the host and once under
  emulation. They are real-time tests on a host that stalls.
- `lamp-a1-continuation-hold` `83e09e504` had the crate's gates only on
  the host (format, Clippy for `lamp-gemini`, 158 with 6, 12 and 24
  tests, a release build of the probe): no workspace pass and no ARM run.

- Nothing here has run against the real service or on a lamp from my side.
- `provider_worker` does not select `AtCommit` or consume the report
  beyond the proposed log line in `a25e68a52`. Activation is yours.
- `A1-017`, explicit yield intervals for duplex: your `55d7af98e` already
  records each one as `duplex_yield_end` with its owner, times and frame
  sequences, so I have added nothing. Say if something is still missing.
- From `MAIN-053`, still owed: recovery under owner change, shutdown and
  physical privacy exercised through the coordinator.
