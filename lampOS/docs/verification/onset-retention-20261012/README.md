# Where the opening of the coffee request went, 2026-10-12

An audit of onset retention in the native serve run on lamp `4ace`
(`docs/verification/final-serve-pilot-20261012`), by Agent 1. The cached
stimulus said "Actually, Lamp, let's talk about coffee instead. Why does
it smell so good?" and the service's transcript holds only "Why does it
smell so good?".

**The opening was never admitted.** The request that the coordinator
admitted begins 2.3 s after the stimulus's speech began. The provider was
handed nothing earlier, so nothing in the provider, the kept start or
continuation lost it. The lamp yielded 2.6 s after the person began, and
a request reaches back 290 ms before the block that makes the lamp yield,
by design.

**Why the lamp yielded that late is not established for the device.** The
run recorded no per-block hearing and no microphone audio. An offline lab
with the same clip over recorded `4ace` replies reproduces the device's
timing in 2 of the 39 trials that can be judged and points at the
canceller: while the lamp is sounding, its output carries the added
speech about 17 dB lower than when the lamp is silent, and the detector
does not score it as speech.

**No correction is justified by this evidence.** One native diagnostic
would settle it; it is at the end. Two controls are added to the
coordinator's tests.

**That diagnostic has now run, 2026-10-12 03:50 UTC: the interruption is
lost at the microphone input, before any software.** See
[The device recording](#the-device-recording). While the lamp speaks,
the microphone signal handed to the software holds the interrupting
speech at least 20 dB weaker than the same loudspeaker delivered five
seconds earlier with the lamp silent. The microphone's hardware
`Auto Gain Control` is on, and its quiet floor drops 7 to 10 dB at the
lamp's first sound and needs more than 30 s to come back. The canceller,
the detector and the hearing rule then did what their input allowed.

Nothing here is a listening test. No audio is in this directory.

## The native run, turn by turn

From the native trace, the independent audit's step timing and the cached
clips (`native-timeline.json`, made by `native_timeline.py`).

| Step | Displaced an owner | Trigger after the clip's speech onset | Request reaches back before the trigger | Request starts, against the speech onset |
|---|---|---|---|---|
| sky | no | 233.8 ms | 288.8 ms | 55.0 ms before |
| why | no | 231.3 ms | 288.6 ms | 57.2 ms before |
| sunset | no | 329.9 ms | 290.5 ms | 39.3 ms after |
| **coffee** | **yes, a reply being spoken** | **2,606.7 ms** | 290.8 ms | **2,315.9 ms after** |
| more | no | 275.0 ms | 290.5 ms | 15.5 ms before |
| correct | yes, a request not yet answered | 295.2 ms | 290.6 ms | 4.6 ms after |
| change | no | 299.1 ms | 288.1 ms | 11.0 ms after |
| math | no | 309.6 ms | 289.0 ms | 20.6 ms after |

- The clip's start is its first source copy, mapped to the lamp clock.
  No output-device latency or acoustic path is added, so the speech
  reached the microphone later than this by an unknown amount. The local
  endpoint, which waits 1,200 ms of silence, came 1,112 to 1,426 ms after
  the clip's last speech: the unknown is of the order of 0.2 s at most.
  With it, the seven other requests begin before their speech does.
- The coffee clip's speech, by its own energy: 0 to 590 ms, 770 to
  1,140 ms, 1,370 to 3,070 ms and 3,280 to 4,560 ms. Four stretches for
  "Actually," "Lamp," "let's talk about coffee instead." and "Why does it
  smell so good?"; which words fall where is my reading of the pauses,
  not a transcription.
- The admitted coffee request begins at 2,316 ms of the clip, inside the
  third stretch. The last sentence, 3,280 ms on, is wholly inside it, and
  that sentence is what the service transcribed.

## Stage by stage

| Stage | What the evidence shows |
|---|---|
| Detector and hearing | One `duplex_yield` in the run, at 2,607 ms of the clip, hearing margin −2.2 dB. Reply 3 had been speaking for 3.8 s and still had 112,321 frames queued, 4.7 s, so the lamp was sounding throughout. In the other seven turns the trigger came 231 to 330 ms after the speech onset. **Why six consecutive blocks were first heard as a person only then is not in the trace** |
| Candidate buffer | In all eight turns the candidate's first block is 288 to 291 ms before its trigger block: the detector's pre-roll, `PRE_ROLL_BLOCKS` = 30 with the trigger as its last block (`crates/live/src/activity.rs`). Nothing older exists to admit |
| Coordinator and admission | `input_admitted.prefix_first_host_read_us` equals the candidate's `first_read_at_us` in all eight: nothing was trimmed at admission. For coffee the commit followed the yield by 478 ms, once the lamp's sound had gone, and the admission followed the commit by 0.3 ms |
| Provider | `provider_input_started` 3.8 ms after admission, `waiting_for_barrier: true`: reply 3 had produced output, so this was the ordinary interruption and nothing was kept back. No `provider_input_discarded` in the run. **The trace does not count the samples the provider wrote for a request**, so "it wrote all it was given" is supported only by the service's transcript of the last sentence |
| Continuation, turn 5 to 6 | The run used `--provider-continuation kept-start`. Turn 5 had ended and had no answer yet when turn 6 displaced it. Turn 6's request starts 4.6 ms after its clip's speech onset, before the latency above, and the answer was to the corrected question ("12 times 3 is 36."). A native positive for the kept start, one attempt |

## The lab: the same clip over recorded replies

`coffee_lab.py`, `summarise.py`, results in `lab-pooled.json`.

- The cached coffee clip (SHA-256 `38e12e61…0ffc`), clean, added in
  software to the microphone recording before cancellation, 1.2 s after
  the start of each of 25 recorded replies in three earlier `4ace`
  sessions (`plain` 8, `six` 7, `ext` 10; inputs as in
  `docs/verification/duplex-lab-20261011`), at the level that session's
  own prompts arrived at the microphone and 6 dB below: 50 trials.
- `crates/live/examples/duplex_lab.rs` at `a75a8ddad`, release, SHA-256
  `1bb4dc7d…5b98`: the production canceller, detector, echo observer and
  duplex decision, closed loop.
- Negative control, the recorded replies with nothing added: no yield in
  `plain` and `ext`; 2 yields and no commit in `six`.
- **39 of the 50 trials count.** In those the lamp sounded at every block
  from the clip's onset to the first yield, as on the device. In the
  other 11 the recorded reply paused or ended first; a person is then
  admitted by the ordinary path, which this lab does not model, so those
  trials say nothing and are pooled apart in the data.

| The 39 trials | |
|---|---|
| Committed within the clip plus one second | 36 |
| Whole request retained (yield within the pre-roll, then commit) | **6** |
| Opening not retained, the 36 committed | median **675 ms**; quartiles 100 and 1,540 ms; worst 3,150 ms; at least 1 s in 17; at least 2 s in 3 |
| First yield | median 400 ms after the onset; within the 290 ms pre-roll in 14; latest 2,700 ms |
| First yield given up at a pause in the person's speech | 14. The commit came after one such yield in 5 and after two in 6 |
| **Yield at 2,500 to 2,650 ms of the clip, then commit** | **2**: yields at 2,580 and 2,610 ms, commits at 2,970 and 3,000 ms, opening not retained 2,290 and 2,320 ms. The device: 2,607, 3,087 and 2,316 ms |
| Blocks before the first yield, 2,989 | nothing speech-like (detector score under 0.5) **83.5%**; a person over the lamp 10.2%; the lamp's own voice 6.0%; no alignment 0.4% |
| The added speech in the canceller's output before the first yield, 25 trials | **−20.8 dB** against what was added (−38.2 to −10.3); 12.1 dB below everything else in that signal |
| The same measure, same clip, lamp silent, 28 trials of the 50 | −3.5 dB (−4.0 to −2.4); 5.2 dB below everything else |

Read with these limits:

- A software mix of a clean clip is not a voice in a room through the
  microphone board. The canceller starts cold on this host. The replies
  are not the one the lamp was speaking on 2026-10-12.
- The lab applies the yield rule without the mask's rule that a reply
  stops yielding after two fruitless yields. The 6 commits that followed
  two given-up yields would not happen in production: the reply would
  play on, and more would be lost.
- The level measure is one least-squares gain of the added speech in the
  cancelled signal, at the canceller's delay of 144 samples. It is a
  linear measure of a non-linear canceller: compare its two rows with
  each other, not with zero.
- It is one stimulus. Its first two words are followed by pauses of 180
  and 230 ms, which is where the early yields were given up.

**Corrected on 2026-10-12, 03:15 UTC.** The first version of this section
pooled all 50 trials: whole request in 6 of 50, median 1,240 ms lost, the
device's timing in 4 of 50. It counted 11 trials in which the reply had
paused or ended before any yield, where the lab's yield is not what would
decide. Two of the four "like the device" were such trials. My fault,
found while validating the analysis for the coming device bundle. The
first figures are kept in `lab-pooled.json` as
`all_trials_as_first_published`.

## The device recording

The integrator's finite diagnostic on `4ace`, runtime `b318`, with
`--diagnostics`: `/private/tmp/lamp-duplex-diagnostic-executed-fhld3dr3`.
`sunset` then `coffee`, same cached clips, duplex, capture-clocked
reference, kept start, noise suppression off. Analysis in
`native_bundle.py`, output `native-bundle.json`; every figure below is
in that file.

**What happened.** The lamp never stopped for the coffee request.

- The coffee clip began 0.45 s after the reply's first write, at frame
  680. Its place in the recording is the sunset clip's measured position
  plus the difference of the two players' source-start times, 4.73193 s.
- One yield, at frame 1113: 4.33 s after the clip arrived and 0.23 s
  before its speech ended. Nothing was heard in the clear after it; the
  lamp resumed 500 ms later and the candidate was rejected. The reply
  played to its end. Nothing reached admission or the provider.
- **This is not the pilot's scene.** Here the reply was the session's
  first playback and the clip began 0.45 s into it. In the pilot the
  clip came 1.2 s into the third reply. The plan says so
  (`scene_preconditions_not_recreated`).

**The clip was emitted.** In the room recording, at the iMac's own
microphone, the sunset clip is found 62.7 ms after its source start and
the coffee clip 67.2 ms after its own, matched-filter peaks 0.552 and
0.51, levels −23.4 and −26.1 dBFS.

**At the lamp's microphone, before the software canceller.**

| | Sunset question, lamp silent | Coffee clip, lamp speaking |
|---|---|---|
| Microphone level | −15.5 dBFS | −14.3 dBFS |
| Matched filter with the clip | 0.622, 6.6 times chance | **0.033, chance is 0.029** |
| Share of the microphone signal coherent with the clip | −0.3 dB | **−26.6 dB (32 ms windows), −21.5 dB (128 ms)** |
| Level expected for the clip from the sunset's loudspeaker-to-microphone gain | | −20.1 dBFS |

Calibration, in the same recording: the clean clip added in software to
a stretch where the lamp speaks alone gives 0.184 and −15 dB at
−30 dBFS, 0.083 and about −20 dB at −40 dBFS, and 0.045 and −25.2 /
−21.2 dB with nothing added. The real overlap reads as nothing added.
So the clip's coherent part is below about −40 dBFS there: at least
20 dB under what the same loudspeaker delivered with the lamp silent.

**The microphone's gain moves.**

- `expected-mixers.json` of the run, the state every rollback restores:
  `'Auto Gain Control'` is `on` and `'Mic Capture Volume'` is 147, its
  maximum, on `device_micro2`.
- Quiet floor before any playback: −74 to −77 dBFS. After the lamp's
  first sound: −82 to −86 dBFS, for the twelve quiet seconds after the
  reply too. Thirty-three seconds after the second reply: −78 dBFS.
- 89 samples clip in the first second of the first reply.
- With the lamp's render silent at the end of the clip, the microphone
  is about 5 dB below the clip's own level (−6.5 to +0.8 dB over seven
  50 ms steps). In the clear it was 5.3 dB above. About 10 dB lower.
- The device's own hearing margin at the yield: −22.1 dB, the person
  against the lamp's own voice at the microphone. In the pilot's trace it
  read −2.2 dB.

**Downstream of that.**

| Stage | On the device |
|---|---|
| Canceller | Reduction 9.8 to 21.7 dB in each second of the clip; 36.6 to 58.9 dB in the seconds after it, lamp alone. It had 0.45 s of the lamp's sound before the clip began |
| Detector, the device's own recorded score | Under 0.5 in 402 of the clip's 457 blocks, and in 339 of its 394 speech blocks, 86%. Longest run at 0.8 or over: 7 blocks. On the sunset question: 0.5 or over in 155 of 188 |
| Hearing, by replay through the production code | Nothing speech-like 391, the lamp's own 37, a person 25, no alignment 4. The only run of six, nine long, is at the clip's end, where the device yielded. Where blocks scored as speech, the render explained a median 80% of the microphone, against the rule's 75% |

The replay runs this host's canceller from cold with the device's render
cursor rounded down to whole blocks, because the lab has no
capture-clocked reference; the rounding is under 0.7 ms at the clip. Its
score agrees with the device's own at 0.825 correlation and it yields
where the device did.

**What this corrects in the lab above.** The lab added clips "at the
level the session's own prompts arrived at the microphone". Those
prompts were recorded with the lamp silent, at the microphone's high
gain, and laid over replies recorded at its reduced gain. A person over
the lamp was therefore at least 10 dB too loud in every lab trial here
and in `docs/verification/duplex-lab-20261011`. The finding I recorded
then, that on `4ace` a person is about as loud as the lamp's own voice at
the microphone, came from the same two gains and is withdrawn.

**Not established.**

- How much of the 20 dB is the gain control and how much is the lamp's
  loudspeaker being next to its microphone. The floor and the
  lamp-silent speech say about 10 dB for the first.
- Why the pilot's margin read −2.2 dB. No audio exists for that run.
- Whether the canceller, once it has converged on a reply or two, leaves
  enough of a person to be scored as speech.

**The smallest next native comparison.** The same two clips, with two
changes and nothing else:

1. The microphone's `Auto Gain Control` mixer switch off, and its capture
   volume fixed where the lamp's own voice stays below full scale.
2. The pilot's precondition: two complete replies before the overlap, so
   the canceller has heard the lamp.

Recorded with `--diagnostics` as this run was. It shows whether the
floor then holds still, how far below the lamp's voice a person really
is at the microphone, what the converged canceller leaves of them, and
whether the lamp yields sooner. It is a mixer setting and a longer
scene, not a threshold: a gain control ahead of an echo canceller
changes the very path the canceller has learned, whatever this one
negative says. No software correction is proposed until it has run.

## What is missing, exactly

1. **How each block was heard on the device between 0 and 2,607 ms of
   the clip**: nothing speech-like, the lamp's own, or a person in runs
   shorter than six. The trace has the decision and not the blocks.
2. **The microphone before and after cancellation for that interval.**
   The run recorded none. The room recording is invalid and is not the
   lamp's microphone in any case.
3. **When the clip's sound reached the microphone.** Bounded above, not
   measured.
4. **Samples written to the service per request.** Not in the trace.
5. Whether the device's canceller, converged and behind the microphone
   board's own processing, removes a person's speech during the lamp's
   as the cold one in the lab does.

## Why no correction is proposed

| Candidate | Why not now |
|---|---|
| Reach further back than the pre-roll at a duplex commit | In the lab the opening is not in the cancelled signal to reach back to: 17 dB lower than with the lamp silent, and below the residue. It would send the service the lamp's own residue. It also needs audio older than the transport's one second, which stays as it is |
| Yield sooner | A change to what is heard as a person while the lamp sounds. The evidence for it is one native negative and a replay; neither chooses a gate |
| Not give a yield up at a short pause | The same. The weakness was known and left alone on purpose in `docs/duplex-listening.md` |
| Anything in the provider or the kept start | The opening never reached them |

## Controls added

`crates/live/src/coordinator/tests/duplex.rs`, through the real hearing,
detector, retention and admission; scripted blocks and evidence, no
audio. They describe today's behavior and change none of it.

| Test | Shows |
|---|---|
| `a_request_begun_while_the_lamp_is_silent_starts_before_its_first_word` | Positive: the request reaches back the pre-roll from the trigger and starts before the first word, in one unbroken run |
| `words_not_heard_as_speech_before_the_lamp_yields_are_not_in_the_request` | Negative, the native loss: two seconds spoken over the lamp and not heard as speech, then six blocks that are. The request starts the pre-roll before the block that made the lamp yield, and 1.76 s of what was said is not in it |

That an admitted request then reaches the service whole and in order is
already `provider_pair::duplex_through_the_worker`.

## The one next native diagnostic

Repeat only the two steps `sunset` then `coffee`, with the same cached
clips and the same `duplex` settings, in a run that records the
microphone bundle (`--diagnostics`: `capture-audio/pre_aec.pcm16le`,
`post_aec.pcm16le`, `events.jsonl`, and `render-audio/render_accepted.pcm16le`),
as the `lamp-connected-*` runs of 2026-10-11 did. `docs/voice-eval-serve.md`
says the serve runner refuses raw audio diagnostics, so this is not that
runner.

From that bundle, with no further device time, the lab's replay gives how
every block of the overlap was heard on the device, what the canceller
left of the person's speech, and when the clip reached the microphone:
items 1, 2, 3 and 5. It is also the first recording of speech over the
lamp through the real acoustic path, which any later correction would
have to be judged on.

If raw audio may not be recorded there, the audio-free equivalent is a
bounded per-block hearing history written to the trace at each yield. It
is not built.

## Reproduce

```sh
python3.12 -I native_timeline.py AUDIT.json METADATA_DIR .
python3.12 -I coffee_lab.py DUPLEX_LAB LAB_ROOT SESSION CLIP.wav CLIP_SHA256 OUT 0,-6 120
python3.12 -I summarise.py OUT plain six ext
```

`AUDIT.json` is the independent audit of the run (SHA-256
`21d8bea8…0c38`), `METADATA_DIR` the native trace segments, `LAB_ROOT` the
prepared lab directories of the earlier duplex work. The recordings and
the cached clips are private and stay on the office Mac.
