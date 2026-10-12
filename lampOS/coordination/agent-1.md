# Agent 1 status

Owner: Agent 1 (audio and Gemini). This file on `lamp-a1-capture-order`
is the status of `MAIN-056` after your two reviews: of
`lamp-a1-continuation-discard` `1fab3e22d`, and of
`lamp-a1-continuation-pilot` `aa1558be3`. The branch is the pilot branch
at `c0ad8a7e3`, left as it was, plus three commits for the ordering
blocker you found. See [The pilot branch](#the-pilot-branch). All of it
is on your `2021c1bb9`; you have integrated it through `4d40fff3d`.
`lamp-a1-continuation-provider` (`8ab02d2c6`, which you integrated),
`lamp-a1-continuation-retain` (`55ccc4a1a`) and `lamp-a1-continuation`
(`9ec2f8cfe`) are left exactly as they were. Reference recovery (`MAIN-053`, `054`, `058`) is
on `lamp-a1-reference` at `a43b44b80` with its own status there. Duplex
(`MAIN-050`) is frozen on `lamp-a1-duplex` at `e2aa5df1c`.

- Updated: 2026-10-12, 02:01 UTC
- **Newest: the ARM64 gate and build of your final pilot source passed every step, four binaries exported.** See [ARM64 gate and build of your final pilot source](#arm64-gate-and-build-of-your-final-pilot-source).
- **Newest: one ARM64 gate and build of your combined pilot source, with one test failing.** See [ARM64 gate and build of your combined pilot source](#arm64-gate-and-build-of-your-combined-pilot-source).
- **New: `lamp-a1-capture-order`.** Your review of `aa1558be3` found
  that a capture time which really goes backwards could be carried
  forward by a later clock gap and accepted. Your regression is in, as
  you wrote it, failing first. The worker now refuses a shared time
  below its predecessor's in the same request before anything is carried
  over. `provider_worker.rs` only.
- **New: the pilot branch.** Two defects your review reproduced are
  fixed, each with its regression committed failing first. The count of
  sent samples by request is bounded at 64 entries with a terminal, named
  failure before one more, as you decided. A duplex admission is now
  taken through the real worker and session in a test, and that test
  found capture instants that could seem to go backwards across the
  worker's two clocks and end the provider; fixed in the worker alone,
  regression first.
- **New on this branch: a kept request that is abandoned is told apart
  from one that is continued, end to end, and what is still unsent of it
  is dropped.** No new message from the coordinator: the authority it
  already publishes says which it is. What had already been handed to the
  socket is reported as sent and never as discarded. See
  [Continued or abandoned](#continued-or-abandoned).
- Your terminal showed a usage limit from about 22:15Z until you were
  back. Notes I pasted in that time may not have been read; everything
  in them is in this file or the document.
- **`MAIN-056`: one explicit option, `--provider-continuation kept-start`,
  off by default, carries the kept start, the at-commit wait and barrier
  reports from `lamp-live serve` to the session.** Verified through the
  provider worker's own loop against a scripted service. See
  [The candidate](#the-candidate).
- **Your three real-service runs, one attempt each:** the at-commit wait
  alone still timed out, with nothing at all from the service in 6.18 s;
  with the start kept the same case was confirmed and answered, burst
  included. See [the kept start](#the-kept-start-built-on-this-branch).
- **A defect of mine that your review found is fixed here:** a request
  retired while its kept start was being written still had its End
  committed. `04dcb8f6c`.
- The service's documentation gives no confirmation for a response
  interrupted before output. The exact missing guarantee and what the
  person gets without it are [stated below](#the-missing-guarantee).
- No cloud and no device from me. Every service result below is yours.
- Two faults of mine in the probe, both found by you, both fixed:
  [below](#my-faults-here).

## Branches

Separate, so that each can be taken or left by itself. You have taken
the stop fence (`e9bdcea28` on `lamp-v2-chat` carries its three files)
and the provider candidate (your `2021c1bb9`).

| Branch | Head | Base | What |
|---|---|---|---|
| `lamp-a1-capture-order` | this commit | `lamp-a1-continuation-pilot` `c0ad8a7e3` | Three commits: `241e64dfd` your regression, failing; `da9891f7e` the refusal on the shared clock and its controls, `provider_worker.rs` only; and this status |
| `lamp-a1-continuation-pilot` | `c0ad8a7e3`, untouched | `lamp-a1-continuation-discard` `1fab3e22d`, on your `2021c1bb9` | The corrections after your review, the bound, the capture-instant fix and the pilot's tests. Commit by commit under [The pilot branch](#the-pilot-branch) |
| `lamp-a1-continuation-discard` | `1fab3e22d`, untouched | your `2021c1bb9` | The continue-or-abandon contract as you reviewed it |
| `lamp-a1-continuation-probe` | `7ab7e30b9` | your `235f50297` | Example only. During-output mode, the observation bound, the exit rule |
| `lamp-a1-stop-queued` | `53f303c7e` | your `55d7af98e` | Six commits. `5479f9b2b` two regressions that fail, `9c6c1637a` the queue guard; then, after your review, `2bec36350` three more that fail, `e70324912` the fence, `2fd77a417` its document, `53f303c7e` your wording corrections. Nothing to do with continuation |
| `lamp-a1-continuation-provider` | this commit | `55ccc4a1a` | `bdc1708de` two regressions that fail, `04dcb8f6c` the retirement fix with your regression, `66960b3f1` the provider worker's continuation choice and nine tests, `20889d416` **the shared commit**: `options.rs`, `main.rs`, `coordinator.rs`, `74d996eb1` the document, `6717a4f86` **a second small shared commit** after your review: `fixture_provider.rs`, `main.rs`, `options.rs`, `b19cadd09` two worker cases under the coordinated contract |
| `lamp-a1-continuation-retain` | `55ccc4a1a`, untouched | `9ec2f8cfe` | The kept start as you probed it: `c80edfa91` library and tests, `76520cb90` probe setting, `270a91412` wiring switched off by a constant, `901622831` document and the Lamp case script |
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

## Continued or abandoned

The whole contract, with its table of what the coordinator does and what
the worker then sees, is the document section "Continued or abandoned:
what becomes of kept input". In short:

- `commit_admitted_input` cancels as `user_interrupted` and admits the
  new input at once, so the worker sees a newer owner: **continued**, and
  the kept words stay, in order, as before.
- Every other `cancel_reply` leaves no owner: **abandoned**. The worker
  tells the transport `discard` where it told it `retire`, and whatever of
  that request, and of the kept words it had continued, has not yet been
  handed to the socket is dropped and never written.
- The outcome is typed and reaches the coordinator's trace as
  `provider_input_discarded`: how many samples were dropped unsent, and
  how many had already been sent in an activity with no End. The second
  number is never called discarded: those samples are heard with the next
  request, and saying so is the point of the number.
- A healthy connection is not abandoned and no retry rule changes.
  Replacing the connection when `sent_samples` is not zero is proposed in
  the document as a separate decision and is not built.
- Default off: with `--provider-continuation off` nothing is kept,
  `discard` is `retire`, and no new output is ever produced.

Commits, on `8ab02d2c6` and then merged onto your `2021c1bb9`:

| | |
|---|---|
| `4167c6a84` | Library: `InputSender::discard`, `Supervisor::discard`, the session applying it, `Event::InputDiscarded`, six session tests, the probe's row for it. Also the worker's log line for the event, so that the tree builds at this commit |
| `f932d1155` | **Shared proposal**: `ProviderOutput::InputDiscarded` (in `provider_worker.rs`), the coordinator's two arms for it (`coordinator.rs`: no state change, one trace record), the fixture's arm (`fixture_provider.rs`) |
| `929d6e23a` | Worker: retirement with no owner is `discard`, with a newer owner `retire`; two tests through its own loop; the earlier "limitation" test rewritten for the new behavior |
| `ffd5a1e64` | **Shared proposal, tests only**: `coordinator/tests/provider_pair.rs`, seven end-to-end tests through `cancel_reply`, authority and `flush_provider_input` with the Coordinated worker |
| `66cb8bc44` | Document, with the compatibility and migration note |
| `f8e600ef9` | Merge of your `2021c1bb9`. Six of the seven conflicts were files you had taken from me unchanged; the document was merged three ways with your edits kept. `lamp-cases.sh` is removed, as in yours |

Against your `2021c1bb9` this branch changes eleven files under
`crates` and `docs`, about a thousand added lines, most of them tests.

The integration must take `f932d1155` together with the option: a
coordinator without the arm would not decode the new output.

## The candidate

`lamp-live serve CONFIG OUTPUT --provider-continuation kept-start`. Off
by default; `off` leaves the provider worker's command line and behavior
exactly as they were. On, it selects together, for the worker's lifetime:
the kept start with a patience of 2,500 ms, the at-commit wait, and
barrier reports on the worker's standard error. `Require`, every timeout,
both queue bounds, capture times and the 1 s age bound for audio as it is
handed in are unchanged. Nothing committed is replayed.

Two things your read-only review of `74d996eb1` found, both corrected:
the option was also accepted and advertised for `directed-fixture`, whose
worker takes exactly its WAV and digest, so a kept start would have been
misread there and the fixture would not have started (`6717a4f86`: the
fixture refuses it, with a test); and all nine worker cases ran under the
legacy contract while the command line uses the coordinated one
(`b19cadd09`: the successful kept question, and Stop and mute while kept,
now run under both).

Integration is three small places, in `20889d416` and `6717a4f86`, and the document
section "In the provider worker" lists them: the option in `options.rs`,
the worker's argument and the `run_start` field in `coordinator.rs`, and
the worker taking the flag in `main.rs`.

Through the worker's own intake, hand-off and session against the
scripted service, input at 10 ms a block with its own capture times
(`provider_worker::tests::kept_start`, ten tests): a resumed question is
kept and exactly it is delivered; two resumed utterances are one question
with one End; a first answer that completes with nothing; Stop and mute
while kept; Stop while being written out; a quota close while kept, with
nothing written and nothing replayed to a new connection; no first
answer, then patience and the strict timeout with no End; the cancelled
case below; and, without the option, the old behavior.

**The retirement defect, mine, found by your review.** After the earlier
response settled, the kept start and audio were written and then the
withheld End, and those writes are awaited: a retirement recorded during
one of them was not read again, so a cancelled or superseded request had
its End committed. Now retirement is read immediately before an End is
handed to the socket. Red on `bdc1708de` and on your regression with the
same message as your evidence; green at `04dcb8f6c`. The limit, which you
accepted: once the End frame is inside the WebSocket layer it goes out
with the next flush, and a retirement recorded after that finds a
committed request, handled as one. A healthy connection is not abandoned
to take a frame back.

**The cancelled kept request, shown and not hidden.** It is never
committed. But the audio the session had taken from it is still written
when the first answer begins, and the next request continues that
activity, so the service hears it as the start of the next question. The
test `a_cancelled_kept_question_is_not_committed_but_its_audio_joins_the_next_one`
shows exactly that on the wire. On the default path the same audio
reaches the service too, only sooner. The session cannot tell a
cancellation from a continuation, and I did not make it guess. The
contract that would settle it is proposed in the document, a provider
input `void { request }` from the coordinator, and is not built.

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
| The service does not confirm an interruption that a burst follows | The same timeout. Tried once on a Lamp: it was confirmed, 465 ms after the start went out with 10,400 kept samples behind it | |

The cut-off first part stays in the model's context with whatever the
service kept of its interrupted answer. Whether the model then answers
the whole thought well is model behavior and is not scored anywhere yet.

## The pilot branch

`lamp-a1-continuation-pilot`. Only with `--provider-continuation
kept-start`; the default keeps nothing and remembers nothing. Detail and
the test list are in the document under "After review".

| Commit | What |
|---|---|
| `b1bbc139b` | Shared test, proposed: a mute while a question is kept, through the coordinator pair |
| `c4488603f` | Your regression, as you wrote it, failing: a block accepted between the two publications of a discard is written (320 samples) |
| `52bd28564` | Library fix: retirement is published before abandonment; a block at or below the applied abandonment is skipped at drain time; sent samples are counted by request, so a late abandonment reports its own and no other's |
| `7ce437ff2` | Shared test, proposed, failing: an abandonment read in one worker turn together with a new owner and that owner's continuation never reaches the transport |
| `0a70ab3cb` | Worker fix: abandonment progress is kept apart from retirement progress; `discard` is sent before `retire` |
| `4d40fff3d` | Library: the count by request is bounded. `MAX_OPEN_ACTIVITY_REQUESTS` = 64; a block that would need a 65th entry is refused before it is handed to the socket and the session ends with `Error::OpenActivityOverrun`, `Recovery::Fatal` as you decided, `Fault::Local` to the coordinator; an abandonment removes the entries it reports; without the kept start nothing is counted. One arm and one assertion in `provider_worker.rs`. **This is the commit you reviewed and are integrating** |
| `41e504fa1` | Session test: an interruption that arrives after the kept question was committed is dropped and answers nothing |
| `688f81b22` | Worker regression, failing: a delay between the two clock readings for one block hands the next block over as captured before it ("49.888835ms before the earlier one") |
| `53e58fe93` | Worker fix, `provider_worker.rs` only: a block is handed its predecessor's instant, never an earlier one, wherever the shared times are in order. I also wrote that times which really go backwards were passed on and still refused by the session. **That was wrong**; see `da9891f7e` |
| `aa1558be3` | Shared tests, proposed (`provider_pair.rs`, and visibility only in `duplex.rs`): a pause for breath, an interruption of an answer that has begun, the bound reached by 65 continuations, and a duplex admission through the real worker |
| `241e64dfd` | On `lamp-a1-capture-order`. Your regression exactly as in `genuine-clock-disorder-red.patch` (SHA-256 `8acc4a97…f879e`), failing on my tree as on yours: "accepted and mapped forward by Some(50.002566ms)" |
| `da9891f7e` | Worker fix, `provider_worker.rs` only. In `submit`, after `require_open` and before the age is computed, anything is mapped, remembered or submitted: `read_completed_at_us` below the last one handed over for the same request returns the error "capture time went backwards". An equal time is in order. A newer request begins its own order. The clamp stays for times that are in order. Age bounds and defaults unchanged, no replay. That error ends the worker's intake, like "invalid or stale capture time" beside it; if you would rather it be `Error::InputSequence` through `refused`, it is one line |

**How the fourth defect showed.** The duplex test ended the provider in 5
of 171 runs before the fix, with `Failure { error: InputSequence, reason:
NotRecoverable, attempts: 0 }` and contiguous sequence numbers. After the
fix, 400 of 400. The five failing outputs are kept. With real 10 ms
blocks it needs a 10 ms delay inside a window a few microseconds wide; I
have not seen it outside this harness and have no rate for a device.

**Limits, named.**

- Audio of an abandoned request that was already handed to the socket
  stays in the open activity and is heard with the next request.
  Reported as `sent_samples`, not prevented: preventing it would need the
  connection replaced, and keeping the conversation across that would
  need replay or a guess.
- The overrun is terminal. Through the supervisor it is one `TurnLost`
  for the request in flight and a `NotRecoverable` end with no dial. Its
  Coordinated path in the worker is exercised by the pair test only.
- The duplex test scripts the capture blocks and their echo evidence. It
  is not a microphone or a speaker, and its blocks are microseconds
  apart.
- `kept_start::with_no_first_answer_the_patience_ends_in_the_strict_timeout_and_no_end`
  failed once in 8 repeats on the host while other work ran (output
  kept): the worker reported request 2 abandoned and dropped its 3,200
  kept samples. That test never cancels, so the only way I can see for
  its authority to lose the owner is the rig's own 100 ms input lease
  lapsing between two heartbeats, which the controller answers by
  revoking the turn. That is an inference: I have no timing record of
  that run. It passed 6 of 6 before, and under emulation.

**Verification of this branch.**

| Commit | What ran | Result |
|---|---|---|
| `0a70ab3cb` | Full pass, 2 jobs, every step's output kept, 23:36:42Z to 23:57:48Z. Its `cargo` was suspended from 23:48:38Z to 23:56:11Z for a recording on the lamp; the test binary then running finished by itself and no other started in that time | format clean; workspace strict Clippy clean; 1,575 passed, 0 failed, 7 ignored single-threaded; release build ok; all exit 0 |
| `0a70ab3cb` | Emulated ARM64, `agent1-arm64-verify37`, 23:34:14Z to 23:36:36Z, alone in the VM | strict Clippy for `lamp-gemini` and `lamp-live`; `lamp-gemini` release tests 188 passed, 0 failed, 2 ignored, with 6, 12 and 23; `lamp-live` release tests of `kept_start`, `continuation_or_discard`, options and fixture 41 passed, 0 failed; release build |
| `4d40fff3d` | Focused, from a source export of the exact commit, 00:00:12Z to 00:02:51Z | format clean; strict Clippy `-p lamp-gemini -p lamp-live --all-targets` clean; `lamp-gemini` 236 passed, 0 failed, 2 ignored; `lamp-live` provider worker, pair, options and fixture 109 passed, 0 failed, 2 ignored |
| `4d40fff3d` | Red control: the same seven bound tests with only the check of the bound removed | 3 of 7 fail (the session's overrun, the supervisor's, the unit test of the bound); with it, 7 pass |
| `4d40fff3d` | Emulated ARM64, `agent1-arm64-verify38`, 00:14:41Z to 00:16:45Z, alone in the VM | strict Clippy for both crates; `lamp-gemini` release tests 195 passed, 0 failed, 2 ignored, with 6, 12 and 23; `lamp-live` release tests as above 41 passed, 0 failed; release build of `lamp-live`, ELF aarch64, SHA-256 `03c0907be469e4bea126e62ab877ce7b72d061a7363b1a4bbe7837e01d743efd` |
| `688f81b22` | The worker regression alone, before the fix | fails: "the later block was handed over as captured 49.888835ms before the earlier one" |
| before `53e58fe93` | The duplex test through the worker, 171 runs in batches of 1, 8, 40 and 122, the last with a temporary diagnostic line in the worker | 5 ended the provider with `InputSequence`; outputs kept |
| `53e58fe93` and the tests of `aa1558be3` | The duplex test through the worker, 400 runs | 400 passed |
| the same | Pair, duplex, worker `kept_start` and clock tests, 25 tests, 8 runs | 7 runs of 25; in the other `kept_start::with_no_first_answer_the_patience_ends_in_the_strict_timeout_and_no_end` failed once (output kept; see the status file) |
| `aa1558be3` | Focused, from a source export of the exact commit, 00:30:11Z to 00:32:58Z | format clean; strict Clippy for both crates clean; `lamp-gemini` 237 passed, 0 failed, 2 ignored; `lamp-live` provider worker, pair, duplex coordinator tests, options and fixture 135 passed, 0 failed, 2 ignored |
| `aa1558be3` | Emulated ARM64, `agent1-arm64-verify39`, 00:38:14Z to 00:40:19Z, alone in the VM | strict Clippy for both crates; `lamp-gemini` release tests 196 passed, 0 failed, 2 ignored, with 6, 12 and 23; `lamp-live` release tests of `kept_start`, `continuation_or_discard`, `duplex_through_the_worker`, the duplex coordinator tests, the clock regression, options and fixture 67 passed, 0 failed; release build of `lamp-live`, ELF aarch64, SHA-256 `951c66ce58f5aca4f3c4592e7185fffc47641f8ea476a6766aea9b762ad06da8` |
| `241e64dfd` | The integrator's regression alone, before the fix | fails: "a genuinely regressing shared capture timestamp was accepted and mapped forward by Some(50.002566ms)" |
| `da9891f7e` | The same regression, my delay control and the new positive controls, after the fix | 3 passed |
| `da9891f7e` | Focused, from a source export of the exact commit, 00:58:46Z to 01:01:25Z | format clean; strict Clippy `-p lamp-gemini -p lamp-live --all-targets` clean; `lamp-gemini` 237 passed, 0 failed, 2 ignored; `lamp-live` provider worker, pair, duplex coordinator tests, options and fixture 137 passed, 0 failed, 2 ignored |

Not run on `aa1558be3` or after it: the whole workspace. No ARM64 run
after `aa1558be3`; you said you will run one combined ARM gate after
integration. Not run at all: anything against the real service or on a
lamp.

## ARM64 gate and build of your combined pilot source

At your request, one bounded job, 2026-10-12. Not a device, not the real
service: cross-compiled on amd64, tests under `qemu-user`.

- Source: `/private/tmp/lamp-pilot-combined-haj6os7l/source/lampOS`,
  mounted read-only and not edited. Manifest SHA-256
  `160777d723aa6cd5e2703c5317d16e0d8e427b1162917f481992924003ff09f8`, 463
  files; exact set and digests before and after the job.
- Job: `agent1-arm64-pilotgate40`, 01:29:13Z to 01:32:37Z, alone in the
  VM, `--timeout 880`, `CARGO_BUILD_JOBS=2`, target volume reused.
- Artifact directory, read-only and immutable:
  `/private/tmp/lamp-a1-pilot-arm-20261012T0129Z-K2UwxH` (`README.md`,
  `receipt.json`, `job.log`, `logs/`, `receipts/`, `bin/`).

| Step | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| strict Clippy `-p lamp-gemini -p lamp-live -p lamp-motor -p lamp-voice-eval --all-targets` | exit 0 |
| release build of `lamp-live` and `lamp-voice-eval` | exit 0, exported at once |
| `lamp-live` release tests, `provider_worker::tests::kept_start` | 11 passed, 0 failed, 0 ignored |
| `continuation_or_discard` | 12 passed, 0 failed, 0 ignored |
| **`coordinator::motor`** | **15 passed, 1 failed**, 0 ignored |
| `startup_cues` | 6 passed, 0 failed, 0 ignored |
| `service_cue_tests` | 3 passed, 0 failed, 0 ignored |
| `lamp-motor --all-targets` | 169 passed, 0 failed, 0 ignored, in 12 result lines |

**The failure, kept.**
`coordinator::motor::tests::motor_crash_preserves_actual_voice_owner_and_next_audio_dispatch`
panicked at `crates/live/src/coordinator/motor/tests.rs:697:5`, left 0,
right 1: the trace held no `motor_degraded` event where the test expects
exactly one. Every assertion before it in that test passed. One run, not
repeated; I have not looked for the cause and changed no source.

| Binary | Bytes | SHA-256 |
|---|---|---|
| `bin/lamp-live` | 9,319,672 | `227749311a33c18555f3974fc3b0793ec2075dc4f771764315039a7f85e609de` |
| `bin/lamp-voice-eval` | 4,321,368 | `f2fc44cac54d66b2cce07b549b618ab05b10e82dfaba52e5950191e41e826ae9` |

Both are ELF 64-bit aarch64 PIE with interpreter
`/lib/ld-linux-aarch64.so.1`. They were built and exported before the
test steps. Whether to use them with that test failing is yours to
decide.

## ARM64 gate and build of your final pilot source

At your request, one bounded job, 2026-10-12. Not a device, not the real
service: cross-compiled on amd64, tests under `qemu-user`. **Every
required step passed.** The failed gate above stays as it is, in its own
directory.

- Source: `/private/tmp/lamp-pilot-final-_nuluwe1/source/lampOS`, mounted
  read-only and not edited. Manifest SHA-256
  `049a86c27ae4595af7d52821f8b024586f1ec6bcc773c1de699431edc594b10d`, 468
  files; exact set and digests before and after the job.
- Job: `agent1-arm64-pilotfinal41`, 01:56:34Z to 01:59:30Z, alone in the
  VM, `--timeout 770`, `CARGO_BUILD_JOBS=2`, target volume reused for
  third-party dependencies only.
- **454 of the 468 source files carry modification times older than my
  previous build in that volume**, made from the earlier source at the
  same path. Cargo judges freshness by time, so the job first removed the
  workspace crates' own outputs (`cargo clean -p` for all twelve, both
  profiles) and compiled them from this source. The release build log
  shows each crate these targets depend on compiled once.
- Artifact directory, read-only and immutable:
  `/private/tmp/lamp-a1-pilot-final-arm-20261012T0156Z-25UYTh`
  (`README.md`, `receipt.json`, `job.log`, `logs/`, `receipts/`, `bin/`).

| Step | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| strict Clippy `-p lamp-live -p lamp-motor -p lamp-voice-eval --all-targets` | exit 0 |
| release build, bins of `lamp-live`, `lamp-voice-eval`, `lamp-motor`; example `head_qualification_operator` | exit 0, exported at once |
| `lamp-live` release tests, `coordinator::motor` | 28 passed, 0 failed, 0 ignored, including the test that failed before |
| `head_qualification` | 3 passed, 0 failed, 0 ignored |
| `coordinator::camera` | 7 passed, 0 failed, 0 ignored |
| `options` | 20 passed, 0 failed, 0 ignored |
| `startup_cues` | 6 passed, 0 failed, 0 ignored |
| `service_cue_tests` | 3 passed, 0 failed, 0 ignored |
| `lamp-motor --all-targets` | 169 passed, 0 failed, 0 ignored, in 12 result lines |

| Binary | Bytes | SHA-256 |
|---|---|---|
| `bin/lamp-live` | 9,506,184 | `b3186e7e40d838898d8575ba63732037e2082cb3463ca30adb24447550e673f4` |
| `bin/lamp-voice-eval` | 4,321,984 | `e1dc042a0aa55f769f6c3c5f15f1d9477cffdc310bd04d25b38b0bfc1c13e54f` |
| `bin/lamp-motor-worker` | 1,203,816 | `d0003697752b18e815bb8797d730197f2d85c6376d028bc511322f772e18165f` |
| `bin/head_qualification_operator` | 773,912 | `f7e782fc8ed5e8668824c44de79caed2608d32b0452c2521d5a802711714141a` |

All four are ELF 64-bit aarch64 PIE with interpreter
`/lib/ld-linux-aarch64.so.1`. Provider and Gemini selectors were not
repeated, as you said. Doctests do not run for a cross target.

## For you

| Id | |
|---|---|
| `A1-021` | Done by you: one during-output attempt, confirmed in 363 ms. Thank you |
| `A1-022` | Still open. `barrier-ms=5000` is now accepted and watches 3 s longer than production. The existing `interrupt-before-output` probe already watches for 5 s under `AssumeAfterQuiet` and then commits, so it also shows what arrives **after** a commit nobody confirmed: one answer or two. As observation only, that is the more useful of the two |
| `A1-023` | Answered by your instruction to build it with explicit bounds. The widening is in the document's bounds table, third row, and is opt-in with the setting |
| `A1-025` | Done by you, thank you: three cases, one attempt each, in the document under "Three real-service runs". More attempts, and the other four cases of the script, are still worth having before this is called qualified. The request as it stood: **The Lamp run**: `crates/gemini/examples/service_probe/lamp-cases.sh PROBE PROVIDER.json FIRST.wav SECOND.wav OUTDIR`, seven cases, one connection each, cached synthetic speech, credential read in place. What would qualify the setting is in the document under "On the probe, and on a Lamp". More than one attempt, please |
| | The ARM64 release probe for it is built and waiting, outside Git, in my session scratchpad under `m056/probe-artifact/`: `service_probe` SHA-256 `08a80e49adac086a61258d68b9982917c95d5fd882f0901b004d4c2c47555551`, 3,819,280 bytes, from `901622831`; `source-manifest.txt` SHA-256 `8660e6796d5be70de5454fd3897a71d1fcfa8c152165fb0ae47b12c41ce233df`, 412 files; `lamp-cases.sh` and a `README.txt` with the exact commands beside it. Built by `agent1-arm64-verify33`, 20:57:35Z to 20:57:41Z, alone in the VM. Under emulation it refuses an out-of-range `keep-start-ms` and stops at a missing provider file; it has never opened a connection |
| `A1-027` | **The `void` message.** Whether the coordinator will say when an admitted input is abandoned and not continued. Without it a cancelled kept request costs what a cancelled request already costs on the default path |
| `A1-026` | **The patience to pilot with.** In your kept run the start was kept 717 ms. The option uses 2500 ms: first output came 1.43 s after the End in your one run, and a person who resumes at once waits all of that. It is a setting, not a measurement of mine |
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

- **`InputSender::discard` published the abandonment before the
  retirement**, so a block accepted in between outlived the purge and
  was written. Your review found and reproduced it. Fixed in `52bd28564`.
- **An abandonment read together with a newer owner was lost in the
  worker.** I derived abandonment from retirement progress. Your review
  named the hypothesis; my end-to-end regression confirmed it. Fixed in
  `0a70ab3cb`.
- **My fix for the per-request count had no bound, and counted under the
  default too.** Your question found it. Fixed in `4d40fff3d`.
- **Two assertions in my new bound tests guessed what a call returns
  after the failure** (`OpenActivityOverrun`; it is `Closed` at the
  session and `NotConnected` at the supervisor, as after any end). Wrong
  in the test, corrected before the commit.
- **My first version of the duplex test left a gap of more than 50 ms in
  its scripted capture**, which the detector rightly refused. Corrected
  before the commit.
- **I claimed that a capture time which really goes backwards was still
  refused by the session, and tested it only where nothing delayed the
  clock readings.** My control regressed by 99 ms with no gap, so it was
  carried backwards and looked refused. Your regression, one microsecond
  back with a 50 ms gap, was accepted. I had fixed one direction of the
  same error and assumed the other. Fixed in `da9891f7e`.

- **The first worker test of a cancelled kept request asserted the
  limitation as if it were the contract.** It is rewritten here for what
  the contract now is; the old behavior is described in the document's
  migration note, not hidden.
- **A retired request could be committed after its kept start was
  written.** Found by your review, reproduced by you and by me, fixed in
  `04dcb8f6c`. I had not thought about retirement arriving during the
  awaited writes.
- **The probe's `barrier-ms` accepted 10 s and the library refuses more
  than 5.** Your `A1-022` run ended in 0.014 s with `InvalidConfiguration`
  and never reached the service. I had tested the parser and not the
  configuration it feeds. Fixed in `cfdd6e742`: the range is the
  library's, and a test builds a configuration from every accepted bound.
- **The probe exited 1 on the new during-output success label.** Fixed in
  `b01c3054a`: one tested function decides, and an observation-only run
  never exits 0.

## Verification

**This branch**, `f8e600ef9`, on your `2021c1bb9`:

| | |
|---|---|
| Host, full pass, 2 jobs, every step's output kept | 22:14:48Z to 22:30:12Z: format clean, workspace strict Clippy clean, 1,569 passed, 0 failed, 7 ignored single-threaded, release build ok, all exit 0 |
| Emulated ARM64, `agent1-arm64-verify36`, 22:12:34Z to 22:14:29Z, alone in the VM | strict Clippy `-p lamp-gemini -p lamp-live --all-targets`; `lamp-gemini` release tests 184 passed, 0 failed, 2 ignored, with 6, 12 and 23; `lamp-live` release tests of `kept_start`, `continuation_or_discard`, `options::` and `fixture_provider`, 39 passed, 0 failed; release build. All exit 0 |
| Host, focused, before the merge | the seven pair tests and the eleven worker tests green 6 runs of 6; the session's `retain`, `stop_fence` and `continuation` tests, 45, green 6 runs of 6 |
| Host, focused, after the merge | `lamp-gemini` 184 passed, 0 failed, 2 ignored; `lamp-live` provider, pair, options and fixture tests 107 passed, 0 failed, 2 ignored; format and strict Clippy for both crates clean |

Everything below is from the earlier branches and unchanged.

The full host pass of this branch is of `74d996eb1`. The two commits
after it, `6717a4f86` and `b19cadd09`, change the fixture's option check,
a usage string and tests; for those I reran the focused checks below and
one ARM run, not the whole workspace, as you said. Later commits change
this file and the document only. `lamp-a1-continuation-retain` is verified at `901622831`. `lamp-a1-continuation` is verified at `9ba8883d0`, which is
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
| `901622831`, `lamp-a1-continuation-retain`, the kept start | clean | clean | 1,495 passed, 0 failed, 7 ignored | ok |
| `74d996eb1`, this branch, the candidate | clean | clean | 1,510 passed, 0 failed, 7 ignored | ok |

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

**Emulated Linux/ARM64**, `agent1-arm64-verify35`, 21:47:09Z to 21:49:05Z,
`b19cadd09`, the corrected head, nothing else running in the VM, all
steps exit 0: the same steps as below with the fixture tests added,
`lamp-gemini` 178 passed, 0 failed, 2 ignored with 6, 12 and 23, and
`lamp-live` `kept_start`, `options::` and `fixture_provider` 31 passed,
0 failed.

Host, focused, on `b19cadd09`: format clean; strict Clippy `-p lamp-live
--all-targets` clean; `options::` and `fixture_provider` 21 passed, 0
failed; the ten worker tests green 6 runs of 6; all of `provider_worker`
73 passed, 0 failed, 2 ignored.

The `74d996eb1` host pass ran from 21:25Z; its last stage, the release
build, was held for your recording and ran at 21:43Z.

`agent1-arm64-verify34`, 21:23:08Z to 21:25:09Z,
`74d996eb1`, this branch, nothing else running in the VM, all steps exit
0: strict Clippy `-p lamp-gemini -p lamp-live --all-targets`;
`lamp-gemini` release tests 178 passed, 0 failed, 2 ignored, with 6, 12
and 23; `lamp-live` release tests of `provider_worker::tests::kept_start`
and `options::`, 27 passed, 0 failed; release build of `lamp-live`.

Host, focused, before the full pass: the nine worker tests green 8 runs
of 8; `provider_worker` and `options::` 89 passed, 0 failed, 2 ignored;
the session's `retain` and `stop_fence` tests, 25, green 10 runs of 10.

`agent1-arm64-verify32`, 20:39:06Z to 20:40:25Z,
`901622831`, `lamp-a1-continuation-retain`, nothing else running in the VM: the same steps,
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

- No real-service run abandons a kept request. Everything about the
  discard is in-memory service and scripted transport.
- Audio of an abandoned request that had already been sent is still
  heard with the next request. Reported, not prevented.
- Mute while kept is now tested through the coordinator pair as well
  (`b1bbc139b`).
- The session loop's own record of `provider_input_discarded` is two arms
  I added in `coordinator.rs`; the pair tests assert the typed output and
  not that trace line, because the pair harness has its own receive loop.

- The kept start has had one real-service attempt per case, yours. That
  is not a rate. Nothing has been through a microphone, an endpointer or
  a speaker with it.
- The `void` contract for a cancelled kept request is proposed and not
  built; the coordinator would have to send it.
- The barrier report is a line on the worker's standard error, not a
  typed event the coordinator records. The patience is a constant.
- No test holds audio long enough to reach the age bound.
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
