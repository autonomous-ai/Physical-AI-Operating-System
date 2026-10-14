# The second builder's log

The second builder's own record, on `lamp-second`, which only it writes
to. The tasks and their briefs are in `work-split.md` on `lamp-voice`.
Dates are the lamp's (Asia/Ho_Chi_Minh). The second builder works on an
Apple M2 Max laptop (macOS) and a Linux ARM64 virtual machine on that
laptop (Colima, 6 virtual cores, Debian bookworm, Rust 1.96.0). Neither is
the lamp, and nothing here was run on a lamp.

## Taken

| # | Task | Branch | Taken | State |
|---|---|---|---|---|
| 1 | The end-of-turn model in a tenth of a second | `second/turn-end-fast` | 2026-10-14 | on `lamp-second`, 2026-10-14; on the lamp 148 ms against 382 |
| 2 | Try to break last night's changes to the voice | `second/voice-review` | 2026-10-14 | on its own branch, 2026-10-14 (`1d737f8e5`); taken into `lamp-voice` with its mends |
| 3 | A listener for "stop" under the lamp's own voice | `second/stop-word` | 2026-10-14 | being built |
| 9 | Faster answers: what exists | `second/faster-answers` | 2026-10-14 | on `lamp-second`, 2026-10-14 (a study, done beside 5 and 10) |
| 10 | A reference that follows the microphone's clock | `second/reference-clock` | 2026-10-14 | on `lamp-second`, 2026-10-14; taken into `lamp-voice` behind `--echo-reference steady` |
| 20 | The replay takes the lamp's own recordings | `second/replay-capture-clocked` | 2026-10-14 | on `lamp-second`, 2026-10-14 |
| 19 | The end sent early, the answer played only when sure | `second/early-end` | 2026-10-14 | on `lamp-second`, 2026-10-14 |

## Handed back

Newest first.

### Task 19: the end sent early, the answer played only when sure (2026-10-14)

**Built** (`crates/voice`; `docs/faster-answers.md` way 2,
`docs/conversation-latency.md`, `docs/live-runtime.md`,
`docs/conversation-thread.md`): `--early-end-ms MS`, off by default;
100 to 1,000 ms in steps of 10, shorter than the end-of-speech wait in
force, and refused without `--provider-continuation kept-start`. For the
lamp: `--early-end-ms 200`.

- At that much quiet the coordinator sends the request's end
  (`activityEnd`) and keeps listening. The authority goes on saying
  "listening" until the 600 ms wait, or the end-of-turn model's verdict,
  confirms; nothing can play before. The answer's sound is held and starts
  at the confirming moment, or at once if that is already past.
- Speech again before confirming (six blocks in a row at 0.8 or more, the
  detector's own start rule) gives the request up exactly as
  `commit_admitted_input` does: `user_interrupted`, then an Interruption
  admit with nothing published between, which kept-start treats as a
  continuation. The blocks heard since the end (at most 30) open the next
  request.
- An utterance that would count as a sound gets no early end, so a lone
  "Lamp," stays one request with its question.
- The model's words reach `voice/said`, and the body its gestures, only once
  confirmed. A "..." answer is ended unheard at the confirming moment; the
  busy-room floor waits for it.
- **Outside the coordinator, for the first builder to look at:**
  `provider_worker.rs` and `thread.rs` take a request given up after its
  end but before confirming out of the conversation thread (`thread::unsay`),
  so its words are not kept as said. It cannot happen with the option off.

**Tests against the scripted service** (real worker and session,
kept-start; "start/end" are activityStart/activityEnd):

| Case | Sent to the service | Given up | Last sound to first block to the speaker |
|---|---|---|---|
| Quiet to the end | 1 start, 145 blocks, 1 end | 0 | 604 ms, at the confirming moment and not before |
| Speech again at 0.3 s (vad-only, and duplex 600) | 2 starts, 235 blocks, 2 ends | 1 | about 1,271 ms, the second answer only; the first never played |
| Answer before confirming | 1 start, 1 end | 0 | 603 ms |
| A "..." answer before confirming | 2 starts, 1 end | 0 | nothing played |
| Answer after confirming (option on / off) | 145 / 185 blocks | 0 | 1,269 / 1,670 ms |
| The link drops between | the first link one request, the next question on a new link | 0 | the first turn `provider_failed`, nothing played; the next 603 ms |
| "Lamp, stop." over an answer | 3 starts, 3 ends | 1 | 602 ms |
| A lone "Lamp," a pause, the question | 1 request | 0 | 1,273 ms |
| Option off | end at 600 ms, no new records, the same worker arguments and `run_start` | 0 | 920 ms |

A transport test shows a name note goes only with the request ended early
and never reaches the next, continued or abandoned. Measured path from the
last sound to the first speaker block: with the injected clock at the
lamp's 1.15 s service time, 1,910 ms off and 1,510 ms on; in a scripted
session with a 900 ms service, the end sent at 604 against 203 ms and the
first block at 1,670 against 1,269 ms.

**Gate.** On the Linux ARM64 VM, from `lampOS/`, on `cd815e905`:
`cargo fmt --all --check` clean; `cargo test --locked -p lamp-voice-eval`
284 passed; `cargo test --locked -p lamp-voice` 955 passed (5 ignored), the
library and every test target but `capture_hold_worker`, which does not
compile on `lamp-voice` itself ("Questions", 1); `cargo build --locked
--workspace` builds. Clippy for `lamp-voice` and `lamp-voice-eval` lists
only the older `lamp-body` warning and that same test; with `-D warnings`
it is clean on the builder's Mac, where that test is not built. On the builder's Mac, the voice library passes test
by test, and in full parallel runs fails only on timing under load (motor
and ring timeouts, ENOBUFS; once one "Lamp, stop." session with
`ConnectionReset`, cause not found, diagnostics added): `origin/lamp-second`
fails the same motor and ring tests there in the same way.

**On the lamp:**

1. Build, deploy, add `--early-end-ms 200` to `ExecStart`.
2. `pauses.sh`: an `early_end` record on each line; "Hey Lamp," with a
   450 ms pause gives one `early_end_given_up` and is answered; a bare
   "Lamp," gives none; 650 and 900 ms pauses behave as before; the first
   sound about 0.4 s sooner.
3. `shorts.sh`: the same words stop the lamp as without the option, and
   the `not_speech` counts are unchanged.
4. `checks.sh`: all pass; give-ups only at pauses of 0.2 to 0.6 s.
5. Watch the journal for `BarrierTimeout`.

**Where it is most likely to be wrong on the lamp:** how the real service
treats a request committed and given up 0.3 s in, and what the model makes
of the rest. And a first piece the service never answers ends the link
(`BarrierTimeout`) unless `provider.json` sets `silent_response:
assume_over` with `no_reply_after_ms`; whether the lamp's file does is not
known here.

**`crates/gemini`:** nothing needed. One possible speed-up: if the real
service confirms an interruption sent before any output, `retire_active`'s
kept start could be released at once, and short continuations would be
answered sooner.

**Not shown:** anything on the lamp or with the real service; an answer
that arrives and completes whole before confirming is still written to the
memory journal (with the lamp's 600 ms wait and no end-of-turn model that
cannot happen).

### Task 20: the replay takes the lamp's own recordings (2026-10-14)

**Built** (`crates/audio/src/replay.rs`; `docs/steady-reference.md`, "The
replay takes the lamp's recordings"):

- `lamp-audio replay-aec` follows a capture-clocked recording through. It
  rebuilds the voice's ring as `ReferenceAssembly` does: the lead and the
  prime at each start or reset; before each capture block, every frame
  accepted by then (`reference.accepted_through`); one block taken; the
  rate for the next from the fill and the frames owed since the last packet
  (`analysed_at_us` less `accepted_at_us`, at most 20 ms). After every
  block its cursor must equal the lamp's (`reference.analysed_through`), so
  the canceller gets the lamp's blocks in the lamp's order.
- It knows the mode from `run_start` in `events.jsonl` (`reference_clock`),
  or, for a session too old to say, from the first capture record's
  cursor; `report.json` says which and how.
- Where it cannot follow, it stops at that capture record, names it and
  why in `limitations`, and keeps and writes out everything before it.
- `events.jsonl` is read a line at a time (up to 4 GiB, which a long serve
  session may need; it was read whole under 32 MiB before): a cut-off last
  line is passed over and reported (`events_cut_off`); a line over 64 KiB
  is counted (`events_long_lines`); any other bad line is an error naming
  its number.
- Without an option the cancellers get the reference the recorded voice
  used (`echo_reference` in `run_start`); `--steady-reference` or the new
  `--as-given-reference` chooses.

**Tested** in `crates/audio/tests/replay.rs` on recordings made in the
tests as the voice makes them: a capture-clocked one of 300 blocks (the
speaker 1,000 ppm fast, every seventh packet late, so the rate moves)
replays whole, `aec_only_mismatched_samples_vs_recorded` 0; the same
without `run_start`, known by its cursor, exact; a reset in the middle,
two segments, both exact; events cut off mid-line, read and reported; a
cursor moved by one frame at the 61st record, stopped there, the record
named, the 60 blocks before kept and exact. The 12 older replay tests pass
unchanged. Gate on the Linux ARM64 VM, on `cb4d24e61`: `cargo fmt --all --check` clean;
`cargo clippy --locked -p lamp-audio --all-targets -- -D warnings` clean;
`cargo test --locked`: `lamp-audio` 178 passed (2 ignored), `lamp-voice-eval`
284 passed, `lamp-voice` 935 passed in its library and every test target
but one; `cargo build --locked --workspace` builds. The one is
`crates/voice/tests/capture_hold_worker.rs`, which does not compile on
`lamp-voice` itself (`1766259f3`): line 161 calls `capture_on` with nine
arguments, and it takes ten since the steady reference was wired
(`--echo-reference`). This change touches nothing in `crates/voice`; see
"Questions".

**On the lamp** (any recording made with the audio diagnostics):

```sh
lamp-audio replay-aec RUN artifacts/replay-as-recorded
lamp-audio replay-aec RUN artifacts/replay-steady --steady-reference
```

The first should end `replay_complete_for_recorded_capture: true` with
`aec_only_mismatched_samples_vs_recorded: 0` in every segment (the lamp
runs without noise suppression). If so, compare the second's
`lamp_sounding_internal_alignment_ms` and `…_changes` with the first's,
and listen to `segment-*-aec_only.wav` at the words played over the lamp.

**Where it is most likely to be wrong on the lamp:** a kind of record the
lamp's recordings have and the made ones do not. The made recordings
follow the recorder's format as `replay.rs` checks it, not the voice's
recorder run against scripted devices (no test in `crates/voice` does that
in a capture-clocked session, and that crate is not this task's).
Candidates: recovery (`--reference-recovery`, off on the lamp but its
reseeds would move the ring), a privacy close and reopen mid-run, or the
fourth recording's events failing for a reason other than a cut-off or
long line (the message now names the line). The first mismatch's
`limitations` line names the record.

**Asked for:** two of the lamp's capture-clocked recordings with played
voices only (for example `cut-a` and the run of words into quiet), as a
`.tar` attached to a draft release of this repository, or wherever is
easiest; say where in "Answers". With them I will make the made
recordings' shape match whatever the lamp's have.

### Task 10: a reference that follows the microphone's clock (2026-10-14)

**Built.** `lamp_audio::steady::SteadyReference`, off unless asked for:
`EchoProcessor::with_reference(noise_suppression, Reference::Steady)`
(`EchoProcessor::new` is unchanged), and `lamp-audio replay-aec RUN OUT
--steady-reference`. A second echo-evidence observer follows where the
echo is from the sound itself; the reference is delayed, through `drift`'s
interpolator and at most 2 ms a block, so that the canceller finds the echo
20 ms after it and keeps finding it there. Everything is in
`docs/steady-reference.md`.

**Why it follows the sound and not a buffer.** Read from the code: in the
capture-clocked mode the lamp runs, a jump in the speaker's queue goes into
the ring too and cannot step the echo; the recorded steps of 5.4 ms back
and forth are most likely the second microphone's own buffering, which the
lamp cannot read; and the delay hint the voice passes leaves the speaker's
queue out in that mode (the queue less the ring's frames is always below
zero and floored), so the hint is the capture delay alone.

**The rig** (`crates/audio/tests/steady_reference.rs`): speech-like sound,
the speaker 98 ppm fast, the microphone 200 ppm slow, the delay stepping as
recorded, a person over the lamp, and the voice's capture-clocked ring as
`ReferenceAssembly` drives it. **The two measures**, before → after, with a
moving echo (M2 Max and the Linux ARM64 VM, identical figures):

| Person against the echo | Echo removed | Person kept over the lamp | Echo within 1 ms of its place |
|---|---|---|---|
| level | 46.5 → 44.6 dB | −5.9 → −3.7 dB | 7% → 76% |
| 15 dB under | 45.3 → 43.3 dB | −8.5 → −7.1 dB | 7% → 81% |
| level, half the echo not linear | 43.9 → 40.4 dB | −5.6 → −3.7 dB | 7% → 71% |

A still echo costs a person 3.8 dB; the moving one 5.9; steadied, 3.7. The
canceller's own estimate goes from 28–84 ms to 0–20 ms. The rig shows the
direction and size, not the lamp's figures: on the rig the canceller copes
with a moving echo better than on the lamp. Cost of the stage: 6 µs a 10 ms
block on the M2 Max, 12 µs on the VM (99th percentile 24 and 52 µs).

**Tested** on the Linux ARM64 VM, from `lampOS/`, on `5d8093a8e` rebased on
`lamp-second`: `cargo fmt --all --check` clean;
`cargo clippy --locked -p lamp-audio --all-targets -- -D warnings` clean;
for `lamp-voice` and `lamp-voice-eval` the only clippy warning is the same
one in `lamp-body` as before (`run` in `crates/body/src/inspection.rs`
never used), in a crate this change does not touch; `cargo test --locked`:
`lamp-audio` 173 passed (2 ignored: the release-only measures and an older
one), `lamp-voice` 937 passed, `lamp-voice-eval` every suite passed, no
failures; `cargo build --locked --workspace` builds. The measures above
come from `cargo test --release -p lamp-audio --test steady_reference --
--include-ignored --nocapture` on both machines.

**On the lamp:** first the replay, twice, over the recordings of
2026-10-14:

```sh
lamp-audio replay-aec RUN artifacts/replay-given
lamp-audio replay-aec RUN artifacts/replay-steady --steady-reference
```

Compare, per segment of `report.json`, `lamp_sounding_internal_alignment_ms`
and `..._changes` (the canceller's estimate: it should hold within a few
milliseconds and change rarely with the option, against 80 to 128 ms as
given), and listen to `segment-*-aec_only.wav` at the words played over the
lamp. `operations.jsonl` has `steady_reference.echo_ms` for every block: it
should step by about 5.4 ms where the recording's echo stepped. If it does,
wire it: where the capture worker makes `EchoProcessor::new(…)`, make it
`EchoProcessor::with_reference(…, Reference::Steady)` behind a voice option.

**Where it is most likely to be wrong on the lamp:** the follower. It
needs a linear fit at one alignment to explain at least half of the
microphone's energy, and on the second microphone little of the echo is a
linear copy; where it explains less, the reference keeps its last delay.
And a match one pitch period of Gemini's voice away may be held for a while
(about a second per step on the rig). `echo_ms` in the replay shows both.

**Not shown:** the lamp, its second microphone's chip, a person, whether
2 dB is heard, the cost on the lamp's processor, the stage inside a running
voice. The causes of the movement are read from the code, not measured.

### Task 9: faster answers, what exists (2026-10-14)

A study, `docs/faster-answers.md`. Nothing built, nothing run on a lamp,
no paid service called. Every outside figure is marked "published, not
measured here" with its source and boundary.

**What it found.** The lamp's speech service (`gemini-3.8-live`) is
published at 1.18 s from a question to its first sound; the lamp sees
1.15 s from the end it sends. So the relay and the road from Ho Chi Minh
City are probably a small part of the wait, and no setting brings this
service near 0.3 to 0.5 s. What the lamp can take off is its own: the
0.6 s it waits to be sure the person has finished, and part of the 0.35 s
it holds the first sound. That gives about 1.5 s. Near 1 s also needs a
faster service, which today means another voice, against the owner's
decision.

**The ways, in short:** (1) end a plainly finished turn sooner: task 1,
take first; (2) send the end at about 0.2 s of quiet and play only once
sure: up to 0.4 s on every answer, code in the voice, behind a switch;
(3) a shorter hold before the first sound: about 0.1 s at 200 ms; (4) the
same service without the relay: unknown, probably small, needs a key;
(5) another Gemini Live model or an Asian endpoint: up to 0.5 s by
published figures, but older, preview or retiring models; (6) another
speech-in, speech-out service: a new voice, no; (7) speech to text, a
text model and a voice: not shown faster from Vietnam; (8) shipped
agents' own figures: a yardstick only (1.0 to 1.7 s median measured from
the user's side).

**For the first builder, in the study's order of trials:** first split
the 1,034 recorded answers into wait, service and hold, and time the road
from the lamp (it changes nothing on the lamp); then the fast end-of-turn
model; then a 200 ms hold; then the end sent sooner.

**Not established:** no round trip from the lamp is recorded anywhere;
where the relay's own server is; where Google serves an AI Studio key;
whether Gemini's text-to-speech "Kore" sounds like the Live voice; the
company's bill through the relay.

### Task 1: the end-of-turn model in a tenth of a second (2026-10-14)

**Built.** `lamp-turn-end --fast`: the same pinned model
(`smart-turn-v3.2-gpu.onnx`, unchanged), made 8-bit in memory as the worker
starts and run by ONNX Runtime 1.30.0 on two threads, the worker kept on
the cores with the largest `cpu_capacity` (cpu6 and cpu7 on the A733).
Without `--fast` the worker is what it was, byte for byte in its answers.
Also `lamp-turn-end bench` (the timing the task asked for),
`lamp-turn-end agree` (fast against exact on labelled clips),
`lamp-turn-end --check-model PATH --fast`, and
`WorkerCommand::smart_turn_fast` in `crates/turn-end-wire` for the voice
to start it with. Nothing in the voice uses it. Everything is in
`docs/endpoint-pilot.md`, "The fast model": what was tried and why each
was or was not taken, the measurements, the agreement tables, the cost of
the native library, and what was not shown.

**What it showed, on the Linux ARM64 VM** (M2 Max host; not the lamp):

| | Today (exact) | Fast |
|---|---|---|
| Verdict, features and model, median of 24 warm runs | 87.0 ms | 29.5–30.3 ms (three runs) |
| Worst of the 24 | 92.8 ms | 31.3–33.2 ms |
| Peak address space | 114 MiB | 243.5 MiB (cap 544 MiB) |
| Processor time per verdict | 88 ms | 59–61 ms, two threads |
| Processor time over 3 s idle | | 0 ms |

Held-out human clips (1,167, shards 4 and 5 of Pipecat's test set), asked
200 ms into a pause as the voice asks: 3 verdicts on the other side of 0.5,
18 in another of the voice's bands (5 newly "finished"), 95.54% correct
against 95.63%. The one multiplication kept in floats was chosen on the
development shards 0 and 1, not on these. By the exact model's measured
ratio (383 ms on lamp-4ace's fast cores against 87 ms here) the fast model
would take about 130 ms on the lamp. That is an estimate, not a
measurement.

**Two findings beside it:**

- tract 0.23.8 built in a Debian bookworm container has no fast 8-bit
  kernel: its build script's assembler probe (`.cpu
  generic+fp+simd+dotprod`) fails on binutils 2.40 and clang 14 to 16, so
  the SDOT kernel is left out without a warning. It does not touch any
  float model the lamp runs today, but anyone who tries 8-bit under tract
  will find it no faster. `.arch armv8.2-a+dotprod` assembles everywhere.
- Asked 100 ms into a pause instead of 200 ms, today's model is no worse
  at what ends a turn early: finished turns judged finished 91.5% against
  90.6%, unfinished ones judged finished 3.3% against 4.0% (held-out clips,
  digital replay). With the fast model that would put a verdict near
  230 ms. That is `ConfirmPolicy` in `crates/voice/src/activity.rs`, yours.

**Tested** on the Linux ARM64 VM, from `lampOS/`, on `1968ba608` rebased on
`lamp-voice` `40e9b1c00`: `cargo fmt --all --check` clean; `cargo clippy
--locked -p lamp-turn-end -p lamp-turn-end-wire --all-targets -- -D
warnings` clean; for `-p lamp-voice` and `-p lamp-voice-eval` clippy stops
on one warning in `lamp-body` (`run` in `crates/body/src/inspection.rs` is
never used), a crate this change does not touch, and lists no other
warning; `cargo test --locked` for `lamp-turn-end` (16 + 7 + 1 + 5, model
tests ignored), `lamp-turn-end-wire` (4 + 12), `lamp-voice-eval` (all
suites) and `lamp-voice` (744 in the library and every integration suite):
no failure; `cargo build --locked --workspace` builds. With the model and
the library present, all 4 tests of `tests/fast.rs` and all 11 of
`tests/parity.rs` pass on the VM, and the 4 of `tests/fast.rs` on macOS.

**On the lamp, as the service user, in the release directory:**

```sh
curl -fsSLO https://github.com/microsoft/onnxruntime/releases/download/v1.30.0/onnxruntime-linux-aarch64-1.30.0.tgz
tar -xzf onnxruntime-linux-aarch64-1.30.0.tgz
cp onnxruntime-linux-aarch64-1.30.0/lib/libonnxruntime.so.1.30.0 .
./lamp-turn-end bench smart-turn-v3.2-gpu.onnx 30                # as it is
./lamp-turn-end bench smart-turn-v3.2-gpu.onnx 30 --cores fast   # held to the fast cores
./lamp-turn-end bench smart-turn-v3.2-gpu.onnx 30 --fast         # the fast model
./lamp-turn-end --check-model smart-turn-v3.2-gpu.onnx --fast
```

It should show `"cores":[6,7]` on the fast runs, the second run's
`total_ms` median near 383 ms, the third's under 150 ms, and the check
exiting 0. To use it: in `crates/voice/src/coordinator/endpoint.rs`
(`EndpointRig::start`), `WorkerCommand::smart_turn_fast` in place of
`WorkerCommand::smart_turn`, for example behind an `--endpoint
smart-turn-fast` beside `smart-turn`.

**Where it is most likely to be wrong on the lamp:** the time. The
130 ms is a ratio carried over from the float model, and an 8-bit
multiplication may lose more on the A76 (half the M2's vector units) than
a float one does. If `bench --fast` is over 150 ms, try `--threads 1`
too: `lamp-name` uses the same two cores every 80 ms, and two threads can
wait on each other. Next most likely: the library not loading on the
lamp's system (it needs glibc 2.28 and `GLIBCXX_3.4.22`; the check says
so at once), and no `cpu_capacity` in the lamp's kernel (the worker then
falls back to the highest frequency; `"cores"` shows what it chose).

**Not shown:** anything on the lamp; speech through its microphone; the
fast model inside a full voice session; whether a 300 or 400 ms end suits
people.

### Task 2: try to break last night's changes to the voice (2026-10-14)

On its own branch only, as the task says: `second/voice-review`, one
commit `1d737f8e5` on `e93a828d1`. It adds tests and
`docs/voice-review-2026-10-14.md`; the one line outside a test file is
`mod review_2026_10_14;` at the end of the worker's test module in
`crates/voice/src/provider_worker.rs`. Nothing was fixed.

**Faults, each shown by a test that fails today, most serious first:**

1. **Speaks when it should not.** The note sent while someone is speaking
   ("your own ears caught your name, Lamp, just now") stays with a request
   that is then given up (judged a sound, or the microphone closed): no
   `activityEnd` is written, so the next request, by anyone and however
   much later, reaches the model with the note in front of it. Fails both
   with `kept-start` and without.
   `crates/gemini/tests/review_2026_10_14.rs`:
   `review_a_note_for_a_request_given_up_is_not_carried_into_the_next_request`.
2. **Speaks when it should not.** A name heard after a request's sound has
   ended, but before its first output, lets that request's held answer
   out (a busy room: "Hey man…" just after someone else's line).
   `crates/voice/src/provider_worker/tests/review_2026_10_14.rs`:
   `review_a_name_heard_after_a_requests_sound_ended_does_not_let_out_its_answer`.
3. **Speaks when it should not.** The second look's own written turn
   counts as one of a new session's two protected requests, so the next
   line of other people's talk is not held.
   `crates/voice/tests/review_2026_10_14.rs`:
   `review_the_lamps_own_second_look_does_not_use_up_a_new_sessions_held_requests`.
4. **Silent to a caller, narrow.** Before a request begins, the second
   look keeps only the newest hearing, so a later faint one (0.91)
   replaces a plain one (1.0).
   `review_a_plain_hearing_just_before_a_request_is_not_lost_to_a_fainter_one_after_it`
   (same file).
5. **No effect on behaviour.** `crates/name/src/said.rs` can publish a
   hearing scored above 1, which `NameHeard::check` refuses.
   `crates/name/tests/review_2026_10_14.rs`:
   `review_a_hearing_is_never_said_with_a_score_over_one`.

**Seen by reading, not shown by a test** (the second look is switched on
only from the environment, so no test can turn it on without a seam that
was not added): an answer that opens with a spoken "..." ends as
`nothing_said` and never reaches `second_look.finished`, so that call gets
no second look; the second look counts a hearing until the turn is over and
can bring a held answer back out; it can go to a renewed session that
never heard the call; `lamp-name` dates a hearing 0.8 s early after a gap;
the worker ignores a failure to subscribe to `name/heard` without a word.

**Found sound, with six passing tests:** `try_note` cannot end the link;
the second look is taken once and does not loop; the note goes out once
per request; a name counts only for its own request. No path was found
that answers twice or loses the link.

**Tested on** an Apple M2 Max laptop (macOS, Rust 1.96.0), from `lampOS/`:
`cargo fmt --all --check` clean; `cargo clippy -p lamp-voice -p
lamp-gemini -p lamp-name --all-targets -- -D warnings` clean; `cargo build
--workspace` builds; `cargo test -p lamp-gemini` and `-p lamp-name` fail
only on their review test; `cargo test -p lamp-voice`: library 726 passed
and 5 failed, `tests/review_2026_10_14` 3 passed and 2 failed. Of the
library's 5, one is the review test; the other 4 are older
`coordinator::motor` tests that time out after 2 s when the whole library
runs at once on this laptop (21 of them in another full run, and the same
21 on `origin/lamp-voice` without the review line); one at a time, all 46
in those groups pass. Nothing was run on the lamp.

**For the first builder:** the five failing tests are the work list, in
that order; each passes once its fault is mended. Most likely to matter on
the lamp: the first, because a "Hey Lamp" said alone and judged a sound is
an everyday event there.

## Questions

1. **2026-10-14, task 20.** `cargo test -p lamp-voice` does not compile on
   `lamp-voice` at `d83834cb2`: `crates/voice/tests/capture_hold_worker.rs`
   line 161 calls `capture_on` with nine arguments; since the steady
   reference was wired it takes ten (the echo reference). Adding
   `lamp_audio::Reference::AsGiven` there is likely all it needs;
   `crates/voice` is yours, so it is left to you. That test's harness runs
   the real capture worker against scripted devices in a capture-clocked
   session; given a diagnostics directory (its sixth argument, `None`
   now) it would make exactly the kind of recording task 20 replays, from
   the voice's own recorder, and a test there could replay it.
2. **2026-10-14, task 20.** Two of the lamp's capture-clocked recordings,
   with played voices only, to check the replay against: where can I
   fetch them? (A `.tar` on a draft release of this repository would do.)
