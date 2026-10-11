# Hearing while speaking

Scope: `MAIN-050`. The duplex turn-taking mode and the capture-clocked echo
reference, both opt-in, on `lamp-a1-duplex`. Code:
`crates/audio/src/{duplex,drift,evidence}.rs`,
`crates/live/src/{turn_taking,reference,audio_workers}.rs`, and one commit
of changes proposed to Main's coordinator, admission, options and wire.

Everything below was measured on recordings, in tests or under emulation.
**Nothing here has run on a lamp.** No default changes.

## Answer

- **The lamp can keep hearing while it speaks, on one of the two lamps
  recorded.** Where a person reaches the microphone about as loudly as the
  lamp's own voice does (`4ace`), a synthetic overlap at the person's
  recorded level stopped the speaker in 31 of 33 held-out trials and
  became an interruption in 24. Where the lamp's own voice arrives 22 dB
  louder than the person (`a0ae`), the canceller removes the person with
  the echo and the mode hears almost none of them: no held-out trial of 75
  stopped the speaker, at any level. There it behaves as the conservative
  baseline does, and its trace says how far apart the two levels are.
- **Its own voice did not become a turn in any replay.** In 176 s of
  recorded own speech across four sessions the lamp stopped for its own
  voice twice, both in one development reply, and never lost a reply to
  it. In the 64 s that were held out it did not stop once. Each such stop
  is a pause of about half a second, after which the reply goes on from
  the sample it stopped at.
- **A long session no longer ends at eleven minutes.** The fixed reference
  margin is used up by the measured difference between the speaker's and
  the microphone's clocks after 66,326 blocks. The capture-clocked
  reference ran four virtual hours at that difference with every counter
  intact, and five minutes under real pacing before the host stalled. It
  still faults at once on real loss.
- **The 200 ms target is met for a loud interruption and missed for an
  ordinary one.** From the start of speech to the decision to stop took a
  median of 240 ms at the person's recorded level and 110 ms at 6 dB
  above it, held-out. About 30 ms more of audio is already with the
  speaker at that moment.
- **Every positive is synthetic.** They are recorded loudspeaker prompts
  added to the microphone in software. No recording of a person speaking
  over a lamp exists yet, and the microphone board's own processing may
  treat real overlap differently. See [what is not established](#not-established).

## Turning it on, and off

```sh
lamp-live directed /absolute/private/provider.json 40 /new/private/run \
  --turn-taking duplex --reference-clock capture
```

| Flag | Values | Default | Effect |
|---|---|---|---|
| `--turn-taking` | `vad-only`, `v1-baseline`, `duplex` | `vad-only` | `duplex` is `v1-baseline` with hearing |
| `--reference-clock` | `fixed`, `capture` | `fixed` | `capture` makes the echo reference follow the microphone's clock |

The two are separate because a long session needs the second under every
turn-taking policy. With neither flag the supervisor's argv to both audio
workers is byte for byte what it was, and a worker's periodic
`reference_clock` report has no new field. Capture frames do carry echo
observations in every mode; this is not byte-identical microphone metadata
or proof of unchanged processing cost.

The fallback is `--turn-taking v1-baseline`. Inside the mode there are further conservative paths,
none of them a setting:

| When | The mode does |
|---|---|
| A microphone block arrives without echo evidence | Masks that block exactly as `v1-baseline` does |
| A candidate's six onset blocks were not all heard with evidence | Decides it by the `v1-baseline` rules |
| A reply has twice stopped for something that turned out to be nothing | Stops yielding for the rest of that reply and counts a person over the lamp as silence, as `v1-baseline` does |
| A prospective fallback prefix outlives the mask's bounded original-read history | Rejects that candidate with `retained_history_unavailable` and `history_loss=expired` or `evicted`; preserves the old reply and drains the rejected utterance to its endpoint |
| Hearing is older than 100 ms | The mask no longer withholds output; the real conversation still faults on its mandatory capture heartbeat, so this is not a healthy microphone recovery |

Original input validation remains mandatory. A new invalid read, unknown
identity, cached timestamp mismatch, privacy/owner change or invalid capture
lineage is not relabelled as normal history aging. The mask exposes a typed
history-loss distinction only for elapsed history or a sequence it actually
evicted. Constant-size retirement bounds do not reconstruct the exact old
interval and cannot authorize its audio: an unverifiable fallback candidate
is rejected. Complete echo-evidence prefixes retain their existing path.
No 700 ms/1.1 s admission limit, 80-read history capacity, detector threshold,
reference clock, 40 ms reference freshness check or opt-in flag changes.

## What the lamps recorded first

Three measurements from bundles already on the Mac set the order of the
work. Lamp `4ace` is `lamp-4ace-varied-12io918n` (102 s, six turns); `a0ae`
is `lamp-rust-v1-ten-evidence-20261011T070623Z` (151 s, ten turns).

| | `4ace` | `a0ae` |
|---|---|---|
| Speaker clock against microphone clock | about +98 ppm | about +66 ppm |
| Echo delay 7 s into the session | 122 ms | 119 ms |
| Echo delay 45 s in, and after | 177 ms, then 167 ms to 183 ms | 179 ms, then 168 ms to 180 ms |
| Canceller removed, median | 52 dB | 52 dB |
| Cancelled signal above -45 dBFS during own speech | 18 % of blocks | 18 % of blocks |
| Detector starts (six blocks at 0.80) on the lamp's own speech | 20 in 56 s | 21 in 43 s |
| A person at the microphone, before cancellation | -18 dBFS | -33 dBFS |
| The lamp's own voice at the microphone, before cancellation | -14 dBFS | -10 dBFS |

The clock figures have a resolution of one block and read as 50 ppm to
150 ppm. The delay growth of 55 ms is in no queue either device reports:
the speaker queue stays at 456 frames and the capture backlog at 16
throughout. Its cause is not known.

So: fed the cancelled signal, the speech detector fires on the lamp's own
voice every two to three seconds, and the leaked voice is as loud as a
person. Neither the detector's score nor a level can carry an interruption
decision. What can is what the render explains.

## What V1 does, and what is kept

Pinned V1 (`d5efe9d7b`, `hal/drivers/voice/voice_service.py`) discards
capture while its own speech plays, discards a short tail after it, and
clears its detector before a fresh question. As the Lamp profile
configures it (`HAL_LIVE_MODE=false`), it does not listen for a person
over itself. `v1-baseline` is this repository's
version of that: a mask over the intervals in which accepted speaker audio
is sounding, plus 100 ms.

Duplex keeps all of it. `PlaybackMask::duplex()` is the same mask, fed the
same speaker receipts, holding the same intervals. Whenever it lacks the
evidence to do better, that mask is what decides, block by block. What it
adds is the evidence.

| V1 safeguard | In duplex |
|---|---|
| Own speech is not a turn | Kept: the lamp's own voice reaches the detector as silence |
| A tail after playback | Kept as the mask's 100 ms, and extended: nothing counts as "in the clear" until the render has been silent for 300 ms |
| Detector state cleared around playback | Kept by the mask for masked blocks; heard blocks never raise the detector on the lamp's voice in the first place |
| A person over the lamp is discarded | **Changed**: heard, when the evidence can separate them |

## The decision

One capture block is 10 ms. For each, the capture worker now sends the
`EchoEvidence` for that very block with the microphone frame. The mask
reads it together with the detector's score (`Hearing::heard`):

| The block is | When |
|---|---|
| `Still` | Not speech-like, and the render has been silent for 300 ms |
| `Clear` | Speech-like, and the render has been silent for 300 ms |
| `Under` | Not speech-like while the lamp is sounding |
| `Own` | Speech-like while the lamp is sounding, and the render accounts for it |
| `Over` | Speech-like while the lamp is sounding, and the render does not account for it |
| `Unknown` | Speech-like while the lamp is sounding, with no alignment to judge by |

Speech-like is a detector score of 0.5 or more. The render accounts for a
block when it explains more than 75 % of the microphone's energy before
cancellation, or when more than 80 % of the cancelled signal has to be
playback by energy accounting. Neither a score nor a level decides
anything alone: a score of 0.99 with the render explaining the block is
the lamp. The 300 ms is the longest the microphone took to fall back to
its floor after a reply stopped, 270 ms over sixteen recorded stops, and
one block.

The turn detector is Agent 2's and is not changed. It is given a different
score: 1.0 for `Over`, 0.0 for `Own` and `Unknown`, and the block's own
score otherwise. Its rule, six consecutive blocks at 0.80, then starts on a
person and not on the lamp.

A run of blocks becomes at most three moments (`Interruption`):

1. **Yield.** Six consecutive `Over` blocks while the lamp has something to
   say. The coordinator hands nothing more to the speaker. It cancels
   nothing and drops nothing: the reply, its owner and its queue stay.
2. **Commit.** Six `Clear` blocks after the yield: the lamp is silent, the
   room has gone quiet of it, and speech-like input remains. This is a
   hearing decision, not addressee evidence or completed input admission.
3. **Resume.** Fifteen consecutive `Still` blocks, or 600 ms without a
   commit. The coordinator hands the speaker the next sample of the reply.

The candidate the detector raised at the yield is held, not decided, while
the yield is open. A hearing commit still requires current privacy,
ownership, capture lineage, deadline and original-read evidence. Only then
does the candidate go through the ordinary path (`commit_admitted_input`):
the 30-block prefix includes the six onset blocks (normally 240 ms before
the first onset block), the old reply is cancelled as
`user_interrupted`, a new turn is admitted, and the retained audio is the
start of the request. On resume it is rejected as policy, inside its own
budget, so the microphone is not left draining.

What that means for a person in the room:

- Someone who starts a new question over the lamp is heard from their
  first word, the lamp falls silent, and their whole sentence reaches the
  model.
- Someone who says "mm-hm" makes the lamp pause for about half a second
  and carry on. Nothing of the reply is lost or repeated.
- The lamp's own voice, when the evidence misjudges it, costs the same
  pause and nothing else.

This is no judgement of who is being addressed. Two people talking to each
other over the lamp for longer than the confirmation takes will interrupt
it, and so will a television. The mode knows that a sound is speech-like
and is not the lamp. It knows nothing else, and its acceptance basis in
the trace, `directed_session_duplex`, says so.

### The limit: how loud the person is against the lamp

Every stage after the canceller sees only what the canceller left. When
the lamp's own voice reaches its microphone far louder than the person,
the canceller takes the person out with the echo, and no evidence
downstream can find them. This is a property of where the speaker and the
microphones sit and how loud the reply is, not of a threshold.

`Margin` measures it: a slow average of the level of blocks heard in the
clear against the level of blocks that were the lamp's own voice, both
before cancellation. It is in every `duplex_yield`, `duplex_commit` and
`duplex_resume` event and in `duplex_summary` as `hearing_margin_db`. On
`4ace` it settled between -3 dB and -5 dB and on `a0ae` near -22 dB. The
results below
are the only calibration it has: most overlaps were heard at -6 dB and
above, few at -12 dB and below.

### What the hold costs the shared code

A candidate that began over the lamp waits for the lamp's sound to die
away: about 3 blocks still queued, 30 of silence, 6 in the clear. The
standard admission budget is 200 ms and 20 blocks, and with it the same
person is turned away for capacity before the lamp is silent
(`the_standard_hold_cannot_keep_a_candidate_through_a_yield`). So in this
mode only:

| Bound | Standard | Duplex |
|---|---|---|
| Decision budget after the trigger | 200 ms | 700 ms |
| Oldest retained block | 600 ms | 1.1 s |
| Blocks held while deciding | 20 | 70 |
| Queue length threshold before appending microphone PCM | 64 entries | 114 entries |
| Total command capacity including the endpoint's one `End` slot | 65 entries | 115 entries |

It is still one candidate, one identity, one deadline that nothing renews,
and one bounded list. The queue bound grows by exactly the 50 blocks the
hold grew by, because the accepted request is that much longer; the
standard bound is unchanged and tested to be. The terminal marker already
had this one-slot allowance; making it explicit does not admit additional
PCM or raise a threshold. A second terminal append at total capacity is
refused without growing the queue.

## Stopping the speaker

Two delays, reported apart.

**Detection**: from the first sample of the person's speech to the yield.
Six blocks is the floor, 60 ms. The evidence looks back 120 ms, so the
person has to fill enough of that window to stop the render explaining
it. Measured in the lab on `4ace`, held-out, 33 trials a level:

| Person against recorded level | Median | 90th percentile | Worst | Within 170 ms |
|---|---|---|---|---|
| +6 dB | 110 ms | 332 ms | 930 ms | 24 of 33 |
| 0 dB | 240 ms | 610 ms | 1,000 ms | 13 of 31 that stopped |
| -6 dB | 345 ms | 850 ms | 1,180 ms | 6 of 22 that stopped |

**Stop**: from the yield to a silent speaker. The coordinator's dispatch
returns without sending in the same pass of its loop in which the yield
was decided (`a_person_over_the_lamp_stops_the_speaker_...`: the block that
established the person is not followed by a chunk). What plays on is what
the speaker already has: at most one chunk of 10 ms in hand, and the
456 frames, 19 ms, that ALSA reported queued throughout both recorded
runs. That is about 30 ms. It is arithmetic on recorded queue depths, not
a measurement of sound stopping.

While the lamp is yielded the speaker is given nothing, so it keeps its
clock with zeros and reports `playback_gap`, as it does when a reply is
starved. The raw accepted-zero receipts, cursors and timestamps remain
unchanged; no gap is automatically excluded from starvation.

Each coordinator yield has a bounded observation slot and an attempted
`duplex_yield_end` record with its original owner, start/end coordinator
timestamps, source-read/frame identities, missing-evidence marker and
outcome. Outcomes distinguish resume, hearing `commit_decision`, evidence
discontinuity, capture-lineage change, stale hearing, reply completion,
ownership change and cancellation/session termination. `commit_decision`
can still be followed by a rejected prefix; only `input_admitted` records
actual admission. A missing evidence block followed by resumed capture
closes the interval instead of leaving the trace open. This bookkeeping
never authorizes input or output, and keeps only one interval independently
of event-buffer flushes.

These intervals concern coordinator withholding, not DAC silence or
acoustic cutoff. Correlation requires matching owners and actual cursor/time
boundaries; missing or dropped metadata leaves attribution unknown. Existing
metadata backpressure and process death can still prevent a terminal record
from being retained. How the speaker worker behaves over half a second of it on
a device is not established; the longest starvation gap recorded so far
is 160 ms.

Against the target of 200 ms from a clearly established onset to stopped
output, taking 170 ms of detection plus 30 ms of stop: met in 24 of 33
trials at 6 dB above the recorded level, 13 of 33 at that level and 6 of
33 at 6 dB below. The onset here is the first block of the prompt as the
detector first heard it in the clear, which is earlier than "clearly
established" by any reading; counted from the yield, the stop is the
30 ms. In the slow trials the decision was waiting for its evidence: six
blocks in a row that the render did not explain had not come sooner.

Request identity through the stop: the held candidate keeps its identity
and deadline from trigger to decision (`the same candidate, not a later
one`), the old reply ends through `cancel_reply`, and the new turn is a new
owner with a new request. Nothing duplex adds can write to the speaker
after that: the old reply's audio left with the old reply. Stale
completions, late bursts and reconnects after an interruption are for the
existing cancellation path to refuse, as after any interruption. Duplex
enters that path and adds no way round it. I added no test of those cases
and changed none.

## The reference clock

The canceller needs one render block for each capture block. The speaker
and the microphones are separate USB devices, and the speaker accepts
about 98 ppm more than one block in that time on `4ace`. `ReferenceAssembly`
counted block against block with a margin that starts at two and may not
leave one to eight. Through the production calls, in virtual time
(`the_fixed_margin_is_used_up_by_the_clock_difference_measured_on_the_lamps`):

| Speaker against microphone | Fixed margin ends in | After |
|---|---|---|
| +98 ppm | `ReferenceBacklog` | 66,326 blocks, eleven minutes |
| -98 ppm | `ReferenceLate` | between 5,000 and 25,000 blocks |

No recorded session is long enough to have shown it.

`--reference-clock capture` puts accepted render frames in a bounded ring
and reads one block out per capture block at a rate within 500 ppm of one
frame per frame (`lamp_audio::drift`). The rate follows how full the ring
is, slowly: a clock difference is slow and steady and is absorbed; loss is
fast and is not.

- **Every accepted sample is used once, in order.** The read is a
  16-tap windowed-sinc interpolation. At exactly one frame per frame the
  output is the input bit for bit. Off rate, a tone stays a tone to better
  than -65 dB up to 3 kHz and -45 dB at 7 kHz.
- **No counter is reset.** `accepted_through`, `analysed_through` and
  `capture_blocks_processed` remain the counts they were. After twenty
  virtual minutes at ±98 ppm, accepted minus analysed equals what waits in
  the ring, to the frame.
- **No silence is made up, and the ring is bounded.** It holds ten blocks
  at most: five kept in hand, the four that can be waiting after the
  longest hold-up a reference survives, and one. Dry or over is a fault.
- **Loss faults exactly as before.** Packets stop: `ReferenceLate` within
  1,002 to 1,005 blocks of the same schedule in either mode. A skipped
  packet is `CursorGap`. A flood is `ReferenceBacklog`. A difference of
  2,000 ppm, which no crystal has, is not followed.
- **Four virtual hours** at +98 ppm and at -66 ppm through the real render
  path: 1,440,000 blocks each, no fault, learned rate 98.0004 ppm and
  -65.9994 ppm (`four_virtual_hours_capture_clocked`, ignored by default;
  29 s in a release build).

The ring keeps about 45 ms of reference in hand where the fixed margin
kept 20 ms at the start. The canceller is told, through the same delay
hint it already takes.

The worker's `reference_clock` report, once a second, gains a `drift`
object in this mode only: the rate in force, the learned difference, and
the frames waiting. A reference fault carries it too.

### Paced by the real clock

`crates/live/examples/reference_pace.rs` runs the production reference
calls and the real canceller in one process, fed by a second process that
sends one accepted-render packet per block of a clock running fast by a
chosen amount, over the same datagram transport. Both are paced by the
host's real clock. There is no audio device, no USB clock and no ALSA: it
is this software under real pacing and scheduling, on a desktop.

**No run reached the length it was asked for.** This Mac stops every
process for 40 ms to 65 ms a few times in five minutes, whether the
process sleeps or spins, and a reference older than 40 ms is a fault by
design in either mode. Runs in a round started together and shared those
stalls.

| Round | Mode | Speaker fast by | Ran | Ended with | Why |
|---|---|---|---|---|---|
| 1, ring of eight blocks | capture | 400 ppm | 20.9 s | `ReferenceBacklog` | The ring: capture loop woken 27 ms late |
| 1 | capture | 98 ppm | 20.9 s | `ReferenceBacklog` | The same, at the same block |
| 1 | fixed | 400 ppm | 126.6 s | `ReferenceBacklog` | The margin, used up by the clock difference |
| 1 | fixed | 98 ppm | 235.2 s | `QueueTiming` | Host: loop woken 65 ms late |
| 2, ring of ten | fixed | 400 ppm | 127.9 s | `ReferenceBacklog` | The margin, used up |
| 2 | capture | 400 ppm | 138.1 s | `ReferenceLate` | Host: no packet for 46 ms |
| 2 | capture | 98 ppm | 18.1 s | `ReferenceLate` | Host: no packet for about 40 ms |
| 2 | fixed | 98 ppm | 258.1 s | `ReferenceLate` | Host: no packet for 50 ms |
| 3, spinning | fixed | 400 ppm | 56.9 s | `ReferenceLate` | Host: no packet for 49 ms |
| 3 | capture | 400 ppm | 56.9 s | `ReferenceLate` | Host: the same block, 52 ms |
| 3 | capture | 98 ppm | 296.9 s | `ReferenceLate` | Host: no packet for 43 ms |

What that establishes:

- **The fixed margin runs out under real pacing as it does in virtual
  time.** At 400 ppm, after 126.6 s and 127.9 s. Six blocks of margin at
  that rate is 150 s; jitter takes the rest. At the measured 98 ppm it
  would be about eleven minutes, and no run on this host lasted that long.
- **The capture-clocked reference held for five minutes at the measured
  difference.** 29,693 blocks, the learned rate 98.8 ppm, the ring between
  963 and 1,886 frames of its 2,400, through capture wake-ups up to 29 ms
  late. It ended when the host kept the speaker process from sending for
  43 ms.
- **At 400 ppm it outlived the fixed margin**, 138.1 s against 127.9 s in
  the same round, with the ring at 2,077 frames at most and the rate at
  473 ppm of its 500 ppm limit, still settling.
- **The first round found a defect.** With a ring of eight blocks and five
  kept in hand, a capture loop woken 27 ms late found more reference
  waiting than there was room for. The fixed margin survived the same
  moment. The ring is ten blocks now, and
  `a_capture_loop_kept_waiting_catches_up_without_a_fault_in_either_mode`
  is that stall in virtual time: it fails at eight. A stall of 35 ms is a
  fault in both modes, because the oldest packet waiting is then 45 ms
  old.
- **Cost**: 76 µs to 132 µs of work per 10 ms block for the reference calls
  and the canceller on silence, and 2.8 MB to 3.1 MB resident, on this
  host.

It does not establish a run of more than five minutes. That needs a
quieter host or a lamp.

## The lab

`crates/live/examples/duplex_lab.rs` replays a recorded session's
microphone and accepted render through the real canceller, detector and
evidence observer in the recorded interleave. With `--duplex` it closes
the loop: a yield stops the recorded render after three more blocks,
the microphone keeps the recorded echo for twelve and then holds a stretch
of the same session's room noise; a resume plays on from where the render
stopped; a commit skips to the end of that reply. How the echo dies away
is that model, not a measurement. The lab runs `Hearing` and
`Interruption` themselves: the mask's fallback after two fruitless yields
and the admission's hold are not in it. Those are in the production-path
tests.

**Negatives** are the recordings as they are. Nobody speaks over the lamp
in any of them, so every yield is a mistake.

**Positives** are synthetic. A prompt from the same session, as the lamp's
own microphone recorded it while the lamp was silent, is added to the
microphone signal during a reply, before cancellation, one overlap per
reply and per trial, at five levels from 6 dB above its recorded level to
18 dB below, at three random positions. They are loudspeaker speech
recorded once through the lamp's microphone and mixed in software.

| Session | Lamp | Role | Replies | Used |
|---|---|---|---|---|
| `lamp-4ace-varied-12io918n` | `4ace` | development and held-out | 7 | even replies to choose the thresholds, odd replies held out |
| `lamp-rust-v1-ten-evidence-20261011T070623Z` | `a0ae` | development and held-out | 10 | the same split |
| `lamp-connected-extended-20261011-01` | `4ace` | development | 10 | all |
| `lamp-connected-plain-20261011-01` | `4ace` | **held-out** | 8 | all; not opened for any decision before the rule was frozen |

The thresholds were chosen on the development halves of the first two
sessions, on negatives and positives together, before the closed loop
existed. The closed loop was then run on development data only. It showed
a weakness described below, and the rule was **not** changed in response:
it was frozen as committed at `feb560767` and evaluated once on the
held-out material. Every trial, failed ones included, is in
`docs/verification/duplex-lab-20261011/`.

### Own voice

The recordings as they are, closed loop. A stop here is the lamp stopping
for itself.

| Session | Part | Own speech | Stops | Replies lost | Pause |
|---|---|---|---|---|---|
| `4ace` six turns | development | 43.6 s | 2 | 0 | 480 ms, 470 ms |
| `4ace` six turns | held-out | 10.8 s | 0 | 0 | |
| `a0ae` ten turns | development | 21.9 s | 0 | 0 | |
| `a0ae` ten turns | held-out | 18.7 s | 0 | 0 | |
| `4ace` extended | development | 46.3 s | 0 | 0 | |
| `4ace` plain | held-out | 34.4 s | 0 | 0 | |

Both stops are in one reply, 220 ms of it apart: the render stopped
explaining the microphone for a stretch, and the lamp paused twice in a
row. Against the detector alone, which started on the lamp's own voice 41
times in 99 s of the first two sessions, that is the difference the
evidence makes. Two stops in 176 s is too few to give a rate.

### A person over the lamp, `4ace`

Synthetic overlaps. "Stopped" is a yield during the overlap; "interrupted"
is a commit. Delays are from the first block of the prompt's speech.

Held-out: odd replies of the six-turn session and all of the plain one.

| Level | Trials | Stopped | Median delay | Interrupted | Median delay | Paused and resumed | Never heard |
|---|---|---|---|---|---|---|---|
| +6 dB | 33 | 33 | 110 ms | 30 | 520 ms | 3 | 0 |
| 0 dB | 33 | 31 | 240 ms | 24 | 580 ms | 7 | 2 |
| -6 dB | 33 | 22 | 345 ms | 17 | 690 ms | 5 | 11 |
| -12 dB | 33 | 14 | 390 ms | 9 | 680 ms | 5 | 19 |
| -18 dB | 33 | 8 | 505 ms | 4 | 725 ms | 4 | 25 |

Development: even replies of the six-turn session and all of the extended
one.

| Level | Trials | Stopped | Median delay | Interrupted | Median delay | Paused and resumed | Never heard |
|---|---|---|---|---|---|---|---|
| +6 dB | 42 | 42 | 140 ms | 38 | 530 ms | 4 | 0 |
| 0 dB | 42 | 40 | 230 ms | 30 | 555 ms | 10 | 2 |
| -6 dB | 42 | 30 | 280 ms | 20 | 565 ms | 10 | 12 |
| -12 dB | 42 | 15 | 440 ms | 10 | 700 ms | 5 | 27 |
| -18 dB | 42 | 8 | 650 ms | 4 | 700 ms | 4 | 34 |

Held-out and development agree. The person's recorded level on this lamp
is 3 dB to 5 dB below the lamp's own voice at the microphone, so the rows
are margins of about +2, -4, -10, -16 and -22 dB.

### A person over the lamp, `a0ae`

| Part | Trials | Stopped | Interrupted |
|---|---|---|---|
| Development, five levels | 75 | 5 | 0 |
| Held-out, five levels | 75 | 0 | 0 |

The person's recorded level here is 22.5 dB below the lamp's own voice,
so the loudest row is a margin of about -16 dB. The result is no better
than the `4ace` rows at that margin and somewhat worse: it is the same
limit, reached by geometry and volume and not by a setting. On this lamp,
at this volume, a person cannot interrupt by voice in this mode. They are
heard once the reply ends.

### What the closed loop showed, and what was left alone

**A pause after the first words ends the yield.** Of the overlaps that
stopped the speaker at the recorded level, 7 of 31 held-out and 10 of 40
in development were resumed and not committed, and none of those was
committed later. In the development cases opened, the person had gone on
speaking for about 300 ms after the stop, and then 15 blocks in a row
scored below 0.5 just as the room became quiet enough to count: a pause
in the prompt, or its end. The lamp then spoke over the rest of it. A
person who says a name, pauses and asks their question would meet this.

**The obvious repair is not safe on this evidence.** Counting speech the
render does not explain, heard after the stop, towards the commitment
would have kept those overlaps. But one of the two stops for the lamp's
own voice was followed by fifteen such blocks, so the same change would
have cancelled a reply for the lamp's own voice. With two examples of one
and a handful of the other, all from replay, choosing between them would
be choosing a gate on echo negatives. It was not done.

**Software noise suppression** was turned on for one comparison
(`duplex_lab --noise-suppression`), on the development material of `4ace`
only, 42 trials a level:

| | Off | On |
|---|---|---|
| Stops for the lamp's own voice in 89.9 s | 2 | 4 |
| Replies lost to it | 0 | 0 |
| +6 dB: stopped, interrupted | 42, 38 | 42, 39 |
| 0 dB: stopped, interrupted | 40, 30 | 40, 37 |
| 0 dB: median delay to the stop | 230 ms | 120 ms |
| -6 dB: stopped, interrupted | 30, 20 | 34, 28 |

In replay it lets the person be heard sooner and more often, and the lamp
stops for itself twice as often. Four stops are still too few for a rate.
The recordings were made with it off and whatever the microphone board
does is in both columns. This is a replay comparison on development data
and it selects nothing.

## Tests

Deterministic, no device, no recording:

```sh
cargo test --locked -p lamp-audio --lib -- drift duplex evidence
cargo test --locked -p lamp-live --lib -- reference turn_taking admission options coordinator::tests::duplex
cargo test --locked --release -p lamp-live --lib -- four_virtual_hours --ignored
```

What the production-path tests in `coordinator/tests/duplex.rs` hold,
through the real hearing, observation, decision, commit and dispatch
functions:

| Test | Holds |
|---|---|
| `the_lamps_own_voice_through_a_whole_reply_...` | 300 blocks of leaked own voice at a score of 0.99: no yield, no candidate, a chunk to the speaker every block |
| `a_person_over_the_lamp_stops_the_speaker_then_interrupts_...` | Yield on the sixth block; nothing dispatched and nothing cancelled while held; commit; one unbroken run of retained audio from before the first word; one `user_interrupted`; a new request |
| `a_brief_acknowledgment_costs_a_pause_and_none_of_the_reply` | A 46-block pause; same owner; every sample of the reply reaches the speaker once, in order |
| `a_yield_that_is_never_settled_is_given_up_at_its_deadline` | Resume at 60 blocks; the candidate ends as policy, not deadline or capacity |
| `the_standard_hold_cannot_keep_a_candidate_through_a_yield` | Why the hold is longer |
| `a_reply_that_twice_yields_to_nothing_masks_like_the_baseline_...` | The third time, no yield and no candidate |
| `without_echo_evidence_duplex_decides_as_the_conservative_baseline_does` | Same masked blocks, candidates and rejections as `v1-baseline` |
| `a_microphone_that_stops_arriving_does_not_hold_the_reply_back` | Dispatch resumes after 100 ms without a block |
| `a_person_who_speaks_once_the_lamp_is_silent_is_an_ordinary_turn` | Six blocks and admitted, as ever |

## Verification

On `bc2f1a7df`, each from a source-only export of that commit.

| Where | Command | Result |
|---|---|---|
| Host, x86_64 macOS | `cargo fmt --all -- --check` | clean |
| Host | `cargo clippy --locked --offline --workspace --all-targets -- -D warnings` | clean |
| Host | `cargo test --locked --offline --workspace --no-fail-fast -- --test-threads=1` | 1,374 passed, 0 failed, 6 ignored |
| Host | `cargo build --locked --offline --release -p lamp-live -p lamp-voice-eval` | built |
| Emulated Linux/ARM64 | `cargo clippy --locked --offline -p lamp-audio -p lamp-live --all-targets --target aarch64-unknown-linux-gnu -- -D warnings` | clean |
| Emulated | `cargo test ... --release -p lamp-audio --lib -- drift duplex evidence --test-threads=1` | 21 passed |
| Emulated | `cargo test ... --release -p lamp-live --lib -- reference turn_taking admission options coordinator::tests::duplex --test-threads=1` | 100 passed, 1 ignored |
| Emulated | `cargo build ... --release -p lamp-live --bins --examples`, then `file` | `lamp-live`, `reference_pace` and `duplex_lab` are aarch64 ELF |

The four-hour test ran on the host in a release build at `b09f69767`;
`reference.rs` and `drift.rs` are the same in `bc2f1a7df`.

The host does not compile `audio_workers.rs` or the worker branch of
`main.rs`: they are Linux-only, and only the emulated build covers them.
The workspace suite ran on the host only. Emulation here is user-mode
emulation of test binaries on an x86_64 machine: a build and a functional
run, not device timing.

## Integration

One commit holds every change to files that are not mine,
`6f30dadfc`: `coordinator.rs`, `admission.rs`, `options.rs`, `wire.rs`,
`main.rs`, the new `coordinator/tests/duplex.rs` and one line of
`coordinator/tests/provider_terminal.rs`. On Linux it builds only together
with `feb560767`, which gives the capture worker the entry point `main.rs`
calls and the field the wire gained. The tested head is `bc2f1a7df`, on
Main's `6d81ab49f`; none of the files in the proposal changed there since
it was written.

| File | Change |
|---|---|
| `options.rs` | `TurnTaking::Duplex`, `ReferenceClock`, `--reference-clock` for the session and for the capture worker only |
| `wire.rs` | `MicrophoneFrame.echo` (optional, defaulted), `ReferenceTiming.drift` (optional, omitted when absent) |
| `admission.rs` | `Hold` in place of three constants; `InputAdmission::with_hold`; `AcceptanceBasis::DirectedSessionDuplex`. `default()` is the standard hold |
| `coordinator.rs` | Hears each block before admission and gives the detector the gated score; dispatch returns while yielded; the hold and queue bound by mode; `duplex_policy`, `duplex_yield`, `duplex_commit`, `duplex_resume`, `duplex_summary` events |
| `main.rs` | Usage text; the capture worker is started with its options |

Decisions that are Main's: whether a 700 ms hold and a 114-entry queue are
acceptable in this mode; whether `duplex` should imply
`--reference-clock capture`; and every default.

Agent 2's `activity.rs` and endpoint code are untouched. The seam with
them is one number per block: the score the detector is given.

## Not established

- **Anything on a lamp.** No device run of either flag. Emulated ARM64 is a
  build and a test run, not device time, and nothing here is acoustic.
- **A person.** Every positive is a loudspeaker prompt mixed in software.
  A voice in the room may differ in level, spectrum and onset. The team
  reports that speech replayed through a loudspeaker sounds muffled on this
  microphone board where a direct voice is clear, so replayed prompts may
  understate or misstate what a person does.
- **The microphone board's own processing during overlap.** Mixing in
  software assumes the path from room to samples treats a person and the
  lamp's echo independently. The board is reported to do its own echo
  filtering, beamforming and noise control. If it does, real overlap may
  reach the canceller differently from the sum of two recordings, in
  either direction. Only a recording of a person over a lamp can say.
- **Noise suppression on.** The recordings were made with software noise
  suppression off, and the one comparison above is a replay of development
  material. Whether it helps or compounds on a lamp is not measured.
- **Where the 55 ms of delay growth comes from**, and whether the
  canceller's re-alignment during it is what produces the leaks.
- **Who is being addressed.** Not attempted.
- **The acknowledgment pause as a person experiences it.** Half a second
  of silence is the cost in samples. Whether it reads as attentive or as a
  stutter is for a room, not a replay.
- **A long session on a device with the capture clock.** Virtual hours and
  a paced process check are software. USB clocks also wander with
  temperature; the controller follows a change of 200 ppm over a minute in
  test, and no device has been asked to.
