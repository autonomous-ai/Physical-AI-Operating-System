# Starting a reply's playout

Scope: `crates/audio/src/playout.rs` and
`crates/audio/tests/playout_replay.rs`. This is the answer to `MAIN-022`: a
start rule for a reply's speech, the recorded schedules it was chosen from,
and what it does not fix. **Nothing calls it yet.** The queue it gates is the
coordinator's, so the wiring is a proposal to the integrator.

**Every "with the gate" figure below is counterfactual.** It comes from
replaying the arrivals each trace retained for a reply through a model of the
hand-over. The traces carry no completion or cancellation time, so the model
takes a reply to be complete at its last retained arrival. These are not
qualified physical turns, and they are not a held-out set: the horizon was
chosen on the same replies. The integrator's label for them is "counterfactual
on stored reply prefixes", and that is what they are.

## What the pilot recorded

First Rust pilot, `51bd815e9`, one question and one answer. Times are from the
reply's first provider audio event.

| Provider event | Audio carried | In hand before it |
|---|---|---|
| 0.0 ms | 1 sample | 0 ms |
| 14.6 ms | 130 ms | 0 ms |
| 171.8 ms | 240 ms | 130 ms |
| 275.1 ms | 160 ms | 370 ms |
| 435.8 ms | 240 ms | 530 ms |
| 674.8 ms | 160 ms | 770 ms |
| 839.9 ms | 160 ms | 930 ms |
| 997.1 ms | 280 ms | 1,090 ms |

The first speech block was accepted at 24.8 ms. 130 ms of audio was in hand
and it was played at once. It ran out at 154.6 ms. The next audio reached the
reply queue at 175.2 ms, 7 µs before the third block of zeros was accepted,
and speech resumed at 183.5 ms. Three blocks, 720 frames, 30 ms of zeros
inside the opening word.

Nothing was slow locally. Enqueue took 1.2 ms to 12.8 ms and the coordinator
hands the speaker one block per 10 ms slot with, in its own words, no
playback jitter buffer. From the fourth event on, every arrival found 130 ms
or more already queued. The service was 42 ms late once, at the start, and
the playout had started 25 ms in.

## Seventeen recorded replies

One reply is not a distribution. The same timing rows exist in every device
trace with provider audio on this machine on 2026-10-11: nine sessions,
seventeen replies. The last three rows are that day's Rust runs. The two
with noise suppression off arrived after this was first published and are
added here. "Needed horizon" is the time from the first audio event to the
arrival that would have been late, however early playout started.

| Run / turn | Audio in first event | Needed horizon | Recorded first write | Recorded zeros at the start | With a 200 ms horizon: first speech, zeros |
|---|---|---|---|---|---|
| `rubc3c4q` / 1 | 1 sample | 169 ms | 177 ms | none | 177 ms, none |
| `rubc3c4q` / 2 | 1 sample, then 160 ms at 6 ms | 176 ms | 13 ms | 10 ms | 63 ms, none |
| `rubc3c4q` / 3 | 60 ms | 164 ms | 12 ms | 110 ms | 162 ms, none |
| `rubc3c4q` / 4 | 1 sample, then 100 ms at 26 ms | 191 ms | 39 ms | 60 ms | 119 ms, none |
| `rubc3c4q` / 5 | 80 ms | 379 ms | 10 ms | 320 ms | 140 ms, 180 ms |
| `fvackatg` / 1 | 180 ms | none | 11 ms | none | 41 ms, none |
| `fvackatg` / 2 | 1 sample, then 130 ms at 7 ms | 140 ms | 14 ms | none | 84 ms, none |
| `5koru8e0` / 1 | 90 ms | 172 ms | 10 ms | 90 ms | 130 ms, none |
| `l3yc4zsy` / 1 | 1 sample | 188 ms | 200 ms | none | 200 ms, none |
| `j5_xqe32` / 1 | 40 ms | 169 ms | 4 ms | 140 ms | 174 ms, none |
| `j5_xqe32` / 2 | 1 sample | 133 ms | 142 ms | none | 142 ms, none |
| `j5_xqe32` / 3 | 1 sample | 180 ms | 192 ms | none | 192 ms, none |
| `g2kp1mcp` / 1 | 40 ms | 170 ms | 13 ms | 130 ms | 173 ms, none |
| `g2kp1mcp` / 2 | 1 sample | 53 ms | 66 ms | none | 56 ms, none |
| first Rust pilot | 1 sample, then 130 ms at 15 ms | 172 ms | 25 ms | 30 ms | 85 ms, none |
| NS off, trial 1 | 1 sample | 391 ms | 399 ms | none | 399 ms, none |
| NS off, trial 2 | 50 ms | 303 ms | 12 ms | 260 ms | 172 ms, 100 ms |

The last two columns are the model in the test suite at each reply's recorded
clock phase, not a device.

What the seventeen show:

- **The service's second burst arrives 130 ms to 190 ms after its first
  audio event in thirteen of seventeen replies**, once at 53 ms and three
  times past 300 ms. Two of those three are among the day's own three Rust
  runs: 172 ms, 391 ms and 303 ms. What the first event carries varies from
  one sample to 180 ms.
- **Today the start is decided by what the first 26 ms bring.** When they
  bring something playable, speech starts 4 ms to 39 ms in and runs dry
  before the next burst: nine of eleven such replies gapped, 1,150 ms of
  zeros in all. When they bring one sample, nothing can start until the
  second burst, and those six replies started 66 ms to 399 ms in and did not
  gap.
- **Every gap in these traces was at the start or was local.** Nine were the
  start. Three more, 10 ms each, were written with 16,080, 29,520 and 60,960
  frames queued in the coordinator, at the moment a provider burst was being
  enqueued. None was a service stall in the middle of a reply. An eighteenth
  reply, recorded later the same day, has one: see
  [what a start cannot fix](#what-a-start-cannot-fix).

## The rule

```rust
// crates/audio/src/playout.rs
pub const START_HORIZON_US: u64 = 200_000;

let mut gate = StartGate::new(START_HORIZON_US);
gate.audio_arrived(at_us);                          // each arrival; the first one counts
gate.open(now_us, buffered_frames, complete)        // before handing a block over
```

A reply's first block may leave the queue when **the time since its first
audio plus the audio in hand reaches the horizon**, or when everything the
reply will carry has arrived. Once open the gate stays open for that reply.
Audio in hand is audio that can be handed over: the last block, which the
coordinator withholds until the reply is complete, does not count. The
integrator's review found that the first version of the proposal counted it
and asked the gate before a block was ready; the model and every figure here
now follow the corrected reading.

In words: do not start until what is in hand would carry playout to 200 ms
after the first audio, because that is how long the service takes to send
more.

- A first event with 180 ms of audio is held 30 ms. One with 40 ms is held
  170 ms. One with a single sample is not held at all beyond today, because
  the second burst covers the horizon the moment it arrives.
- The hold is bounded by the horizon from the first audio. A reply with
  nothing to play until later starts when it has something, as it does now:
  the latest start in the seventeen is 399 ms with the gate and without it.
- A complete reply starts at once, however short.
- A gate that was never told of an arrival starts its horizon at the first
  question asked with audio in hand. A missed call costs one bounded hold,
  never a silent reply.

A fixed delay was the alternative. It needs 160 ms to cover the same replies
and pays it on every reply, including the ones the service already delayed.
On the enqueue rows of the first fifteen traces that is 112 ms later to first
speech on average and 62 ms later to the end, against 63 ms and 15 ms for the
horizon. That comparison was run outside the suite, before the withheld block
was taken out of the count; it has not been rerun.

## What it costs

At each reply's recorded clock phase, horizon 200 ms, all seventeen:

| | Without | With |
|---|---|---|
| Replies with zeros at the start | 9 | 2 |
| Zeros at the start, total | 1,150 ms recorded; 1,150 ms in the model, two replies one block off in opposite directions | 280 ms |
| First speech after first audio, mean added | | 70 ms |
| First speech after first audio, most added | | 170 ms |
| End of the reply, mean added | | 19 ms |
| End of the reply, most added | | 70 ms |

The hold is paid at the start and mostly refunded at the end: for every reply,
the end moves by exactly the hold minus the zeros that no longer fall inside
the reply. A reply that gapped 140 ms after its first 40 ms of speech today
starts 170 ms later with the gate and ends 30 ms later.

| Horizon | Replies that gap at some clock phase |
|---|---|
| none | 11 |
| 180 ms | 3 |
| 190 ms | 2 |
| 200 ms | 2 |

For the first Rust pilot reply alone the smallest horizon that covers every
clock phase is 170 ms. 190 ms covers every reply whose second burst came
within 191 ms, 200 ms does so with a block to spare, and both shorten the gap
of the two that did not.

**A longer horizon covers those two and costs every reply.** On the enqueue
rows of all seventeen, outside the suite:

| Horizon | Replies with zeros at the start | Zeros, total | First speech, mean and most added | End, mean and most added |
|---|---|---|---|---|
| 200 ms | 2 | 280 ms | 71 ms, 170 ms | 19 ms, 70 ms |
| 300 ms | 1 | 80 ms | 117 ms, 260 ms | 53 ms, 140 ms |
| 400 ms | none | none | 145 ms, 330 ms | 77 ms, 170 ms |

At 400 ms nearly every reply starts when its second burst arrives, which is
what the replies that begin with one sample already do. I recommend 200 ms
as the place where the cost turns, and I do not know that it is right for the
configuration the day's runs used: of three replies, two needed more. The
`start_horizon_needed_us` field in the proposal is how each further run says
which.

## What a start cannot fix

- **A late second burst beyond the horizon.** `rubc3c4q` / 5: 80 ms of
  audio, then nothing for 379 ms. The gate starts it at 140 ms and it gaps
  for 180 ms instead of 320 ms. NS off, trial 2: 50 ms, then nothing for
  303 ms; 100 ms instead of 260 ms. Covering both means a 400 ms horizon for
  every reply.
- **A stall later in a reply.** The gate does not close again. A test delays
  the pilot reply's fourth event by 400 ms: one gap of 230 ms with the gate,
  260 ms plus the start gap without, speech back in the first slot after the
  audio, the same end either way. I recommend no rebuffering rule: in these
  traces the service sends 100 ms or more at a time, so there is no dribble
  to smooth, and holding after a stall only lengthens it.

  **One was then recorded.** The coordinated build's first reply
  (`02148ed4d`, 3.74 s of audio) is not among the seventeen and the horizon
  was not chosen with it. Its service ran about 95 ms ahead of real time
  until its fourteenth event, which came 705 ms after the thirteenth: 2,700 ms
  of audio delivered, 2,910 ms gone. The device recorded 80 ms of zeros at
  the start and 140 ms from 2.77 s. The model gives 90 ms and 130 ms without
  the gate; with it, first speech 110 ms later, no gap at the start, 110 ms
  at the stall, and the same end. A 400 ms horizon would leave 40 ms.
  `StartNeed` reports a deficit of 210 ms set by an arrival 2.9 s in. That is
  what a stall looks like in the trace: the arrival that sets the need is
  nowhere near the start.
- **The three 10 ms gaps with audio queued.** They are not arrival gaps and no
  buffer in front of the queue removes them. See below.

How to tell them apart in a trace: `StartNeed` gives each reply the horizon
its own arrivals needed. At or under the configured horizon and it still
gapped at the start, the gate is wrong. Over it, the service stalled. A gap
with the reply queue above one block is neither.

## Interruption

The gate sits in front of the hand-over. Held audio is in the coordinator's
queue and nowhere else: no permit has been issued for it and the device has
none of it. A cancellation during the hold therefore has nothing to flush. In
the replay, cancelling the pilot reply 60 ms in finds no frame handed over
with the gate and 960 without. After the gate opens nothing about
interruption changes: the device still holds 10 ms to 20 ms.

## For the integrator

1. **Wire the gate where a block is taken from the reply queue.** In
   `Reply`: one `StartGate` per reply, `audio_arrived(received_at_us)` in
   `append_provider_audio`, and `open(now_us, self.pcm.len(),
   self.generation_done)` as a further condition in `next_speaker_chunk`. A
   patch with exactly that is on `gemini-voice-playout-proposal`.
2. **Put the need in the trace.** `StartNeed::arrival` beside the gate and
   `deficit_us()` / `horizon_us()` in `turn_finished`, with
   `StartGate::opened()` in `speaker_first_write`. Then every device run says
   what horizon it needed and whether the constant still fits.
3. **A second generation in one turn** keeps the open gate in that patch,
   which is today's behavior for it. Whether to hold again is yours.
4. **The gaps with audio queued are in the hand-over.** One block is
   outstanding at a time, and the next is sent after the receipt for the
   last. Three times in these traces the speaker's slot came with no block
   while over 16,000 frames were queued. I have not established why. Letting
   two blocks be outstanding would cover a late tick, and a block the speaker
   has not written is still refused on cancellation, so it costs interruption
   nothing. That is a coordinator change and a suggestion, not a finding.
5. **Ten of seventeen replies begin with an audio event of one sample.**
   `provider_first_audio` then marks a moment with nothing to play. I have not
   established whether the service sends it or the decode path makes it.

## Tested

Host, `crates/audio/src/playout.rs`, 11 unit tests: the boundary of the rule
to the microsecond, the bound, completion, staying open, reset during a hold,
the unarmed gate, a clock reading before the first audio, and `StartNeed` on
the pilot's events.

`crates/audio/tests/playout_replay.rs`, 11 tests: 10 over the seventeen
recorded schedules and one over the reply with the stall:

| Test | What it pins |
|---|---|
| `without_a_hold_the_model_reproduces_the_gaps_the_devices_recorded` | For all seventeen, first speech within one slot of the recorded first write and zeros within one block of the recorded zeros |
| `the_first_rust_pilot_gap_is_reproduced_from_its_enqueue_rows` | From the 36 enqueue rows: first speech at 24,775 µs, one gap of 720 frames thirteen blocks later |
| `the_horizon_plays_the_first_rust_pilot_reply_whole_at_every_clock_phase` | No gap at 40 clock phases; first speech 82 ms to 92 ms in; at the recorded phase 60 ms later to start and 30 ms later to end |
| `the_smallest_horizon_that_covers_the_first_rust_pilot_reply_is_170_ms` | 30, 30, 20, 10, 0, 0, 0 ms of zeros at horizons 0, 100, 150, 160, 170, 180, 200 ms |
| `the_horizon_removes_every_recorded_startup_gap_but_the_two_the_service_stalled` | Eleven replies gap at some phase without it; with it only `rubc3c4q` / 5, 310 ms down to 180 ms, and NS off trial 2, 270 ms down to 110 ms |
| `a_180_ms_horizon_is_short_by_one_reply` | The table above |
| `the_hold_costs_the_start_what_the_gap_cost_the_middle` | End moves by hold minus zeros removed, for every reply; 1,190 ms and 320 ms in total; no start later than horizon plus enqueue plus a slot unless the reply had nothing to play before then; the latest start is unchanged |
| `a_stall_later_in_the_reply_is_a_gap_under_any_start_and_is_not_held_again` | The 400 ms stall |
| `a_recorded_stall_in_the_middle_of_a_reply_is_shortened_and_not_covered` | The eighteenth reply: both recorded gaps reproduced within one block; with the gate the first is gone, the second is 110 ms, the end is unchanged; the need is 210 ms set 2.9 s in |
| `a_cancellation_during_the_hold_finds_nothing_handed_to_the_speaker` | 0 frames against 960 |
| `each_recorded_reply_reports_the_horizon_it_needed` | The "needed horizon" column |

The schedules in the test are timings and frame counts. No audio, no words.

## Not established

- **Anything on a device.** The replay is a model of the hand-over: one block
  per 10 ms slot, the last block withheld until the reply is complete, zeros
  when nothing is in hand. It reproduces the recorded gaps to within one
  block. It is not the coordinator and not the speaker loop, and it has no
  scheduler in it.
- **That 200 ms is right.** Two of the day's three replies needed more.
  Other models, voices, regions and networks may pace differently again. The
  trace fields in item 2 are how to find out.
- **What a person hears.** A 30 ms hole in a word and a 50 ms later start are
  both below what these tests can judge. The room recording is the
  integrator's.
- **The speaker loop over the scripted device** with this schedule. The loop
  has no test yet; that is still the next piece of the
  [device seam](audio-device-seam.md).
