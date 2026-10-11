# Recovering the echo reference

Scope: `MAIN-053` and `MAIN-054`, branch `lamp-a1-reference`. Code:
`crates/live/src/reference/recovery.rs`, its hooks in `reference.rs`, the
capture worker in `audio_workers.rs`, and one commit of changes proposed to
Main's options, wire, coordinator and `main.rs`.

Opt-in: `--reference-recovery bounded`. Without the flag nothing changes.
**Nothing here has run on a lamp.**

## Answer

- **The native run that ended at 378.6 s ended on clock drift, and only on
  clock drift.** Its own trace gives the speaker 157.3 ppm fast against the
  microphone. The fixed margin is a count of render blocks against capture
  blocks; it started at 2, may not pass 8, and that rate moves it a block a
  minute. No report was late and no packet was missing.
- **Every fixed-margin session on that lamp ends there**, at about six
  minutes, whatever is being said. It is not an idle fault and not a
  scheduling fault.
- **With `--reference-recovery bounded` the fixed margin is re-centred
  instead.** In virtual time through the production calls, at that lamp's
  rate: without the flag `ReferenceBacklog` at 41,322 blocks; with it an
  hour, eight re-centres, every counter intact.
- **The capture-clocked reference (`--reference-clock capture`) does not
  drift and needs no re-centre.** It is the cure where the re-centre is the
  remedy. In replay of four recorded sessions it cancels as well as the
  recorded interleave does. Both are opt-in and neither has run on a lamp.
- **With recovery the capture-clocked reference survives this Mac's
  stalls.** Ten minutes of two paced processes at that lamp's rate ran to
  the end, through a 43 ms hold-up the host made itself. No paced run had
  finished before.
- **Scheduling stalls are a different matter and a smaller one on a lamp
  than on this Mac.** In 453 s of recordings from two lamps no capture read
  was more than 12.6 ms after the last and no reference packet more than
  20.9 ms. What a longer stall does on a lamp is overrun the microphone's
  40 ms buffer or underrun the speaker's, and those are the devices'
  discontinuities, which this work does not recover. See
  [what is not done](#not-done).

## The native run

`/private/tmp/lamp-serve-native-20261011-03`, `4ace`, runtime `7acfabcd6f38`,
`v1-baseline`, fixed reference, read from
`lamp-evidence/runtime/metadata/events-0.jsonl`.

The capture worker reports its reference once a second. Over the 377
reports between the clock start and the fault:

| Seconds in | Capture blocks | Render blocks analysed | Margin |
|---|---|---|---|
| 0 | 1 | 3 | 2 |
| 91.5 | 9,151 | 9,154 | 3 |
| 146.9 | 14,687 | 14,691 | 4 |
| 207.3 | 20,724 | 20,729 | 5 |
| 261.6 | 26,154 | 26,160 | 6 |
| 327.0 | 32,694 | 32,701 | 7 |
| 378.6 | 37,855 | 37,863 | 8, and the next packet is the fault |

Each row is the first report at that margin; between rows it flickers by
one, which is the packets and the capture reads sliding past each other.

| Question | From the trace |
|---|---|
| How fast is the speaker against the microphone? | 157.3 ppm, least squares of render blocks on capture blocks over all 377 reports. This is the drift of the two counts as the software kept them, which is what the margin is made of. It is not a measurement of either oscillator |
| What does that predict? | Six blocks of margin in 382 s. Observed: 378.6 s |
| Was anything late? | Reports 1.0085 s apart at the median, 1.012 s at worst, 100 or 101 capture blocks each. The newest packet was 8 ms old at the fault |
| Was anything lost? | Expected sequence 37,862 with 37,861 packets and the two blocks of prime analysed: complete |
| Was the lamp speaking? | No. The eighth reply had retired |

The earlier estimate for this lamp, from a 102 s recording, was 98 ppm with
a resolution of one block, "read as 50 ppm to 150 ppm". This run is long
enough to measure it: 157 ppm.

Reproduced through `prime`, `started`, `accept` and `capture_step` with the
real canceller (`the_fixed_margin_ends_where_the_native_run_ended`):
`ReferenceBacklog` after 41,322 capture blocks. The model's packets keep
perfect time and the margin has six and a half blocks to cross. The lamp's
packets jitter by up to a block, which brought the bound half a block
nearer: 37,855. Same cause, same fault, 9 % apart.

## What recovery does

Two references, two amounts of recovery.

### The fixed margin: re-centred

Only one thing is recovered, the one that ended the run.

A packet that takes the margin past its bound is taken in, up to four
blocks past, and nothing is decided on it yet. The decision is made at the
next capture block, on that block's own status and no earlier one's: if
the microphone then has no more than 208 frames waiting and the block
before it was read within the last 20 ms, the capture loop is on time and
nothing fell behind. The count drifted. If not, the loop was held up, that
is a backlog, and it is `ReferenceBacklog` as it always was. On drift
(`recentre`):

1. One reseed is taken from the budget, or the worker ends with
   `RecoveryExhausted`.
2. The canceller is reset.
3. It is given the newest accepted blocks again as its lead, in order,
   from a history of eight kept for the purpose: two when the margin was
   rising, six when it was falling.
4. The count the margin is measured against is moved by exactly the
   difference. That move is kept in its own counter. `accepted_through`,
   `analysed_through` and `capture_blocks_processed` do not change.
5. The DSP epoch advances through the existing `CaptureDspReset`, the
   detector's state starts again beside the canceller's, and a
   `reference_status` event reports cause `margin_exhausted`.
6. Echo evidence is withheld until the canceller has processed 300 render
   blocks in which the lamp was sounding: three seconds, V1's own wait
   into playback before it listens.

The same is done from the other side when a capture block finds no render
block ahead of it while the newest packet was accepted within the last
20 ms: a speaker that runs slow. A speaker that is late is not that, and
is `ReferenceLate` as before.

**It is done early when the room allows.** A cold canceller is best had
when nobody needs it. From six blocks ahead, if no capture block has
scored as speech for half a second and the render has been silent for
half a second, the re-centre is made at once, on the same condition that
the capture loop is on time. At the bound it is made regardless. On the
native run's lamp that is a re-centre about every four minutes in a quiet
room and every seven and a half otherwise.

**Everything else in the fixed mode is still the fault it was.** A packet
older than 40 ms, a missing packet, a speaker that stopped, a capture loop
that fell behind by even 20 ms at a margin already high: tested to be
(`the_fixed_margin_recovers_drift_and_nothing_else`,
`audit_fixed_current_capture_stall_must_not_be_labelled_clock_drift`).
The fixed reference renders on arrival and has no way to stop cancelling
and say so.

### The capture-clocked reference: untrusted, reseeded, warmed

It never needs a re-centre: at 157.3 ppm for ten virtual minutes it
reports no status at all. What it can recover is everything else, because
it pairs reference with capture by count in a ring and can leave the
canceller out.

| What happened | What is done |
|---|---|
| The capture loop was held up and every packet is there, in order, with the speaker's own timestamps a block apart | Used. Counted as `late_packets`. No status, no reseed: the pairing by count is as known as it was |
| A packet is missing, or the next starts somewhere else | **Untrusted**, cause `sequence_gap` or `cursor_gap` |
| A capture block has no reference waiting | Untrusted, `absent` |
| By its own timestamps the speaker accepted nothing for more than 40 ms, its whole buffer | Untrusted, `sender_paused`: what it played and what was counted have parted |
| More arrived at once than may wait, or the microphone had more than 40 ms waiting | Untrusted, `overflow` or `capture_backlog` |
| A packet at or before one already seen | Not used. Counted as `ignored_packets` |
| Another privacy generation, a cursor or a clock that runs backwards, a queue observation from the future or from before the last one, a malformed packet | A fault, as always. Never recovered |

**Untrusted** means the canceller is not run. The capture goes on exactly
as the microphone recorded it, in order, every block counted, marked
`untrusted` on its frame, with no echo evidence. No silence is put in the
reference's place and no old reference is passed off as current.

**Reseed.** Once packets that arrived within 40 ms of being accepted and
follow one another without gap or pause fill the ring to its working
level, 50 ms, and the capture loop is on time, the canceller is reset onto
them. The frames it never saw are counted as `skipped_frames` under their
own cursors. The clock rate already learned is kept.

**Warming.** As after a re-centre: cancelled, without evidence, for three
seconds of the lamp's own sound.

**Bounds.** Untrusted for a second at most, then `RecoveryTimedOut`: the
bound is absolute and is checked before any reseed, so fresh reference
arriving at the deadline does not buy another interval. Three reseeds in
a minute, then `RecoveryExhausted`. A speaker that restarts its
own clock is its own reseed and spends none of the budget, but the
canceller warms after it all the same.

## Does the capture-clocked reference cancel as well?

It had never been measured. Four recorded lamp sessions were replayed
through the real canceller twice: with the recorded interleave, and with
the same accepted render put through the capture-clocked ring and read
out one block per capture block (`duplex_lab --capture-clock`).

| Session | Reference | Echo removed, median | Own-speech blocks left above -45 dBFS | Detector starts on own speech |
|---|---|---|---|---|
| `4ace`, six turns | recorded | 48.9 dB | 29.0 % | 20 |
| | capture-clocked | 53.6 dB | 20.2 % | 11 |
| `a0ae`, ten turns | recorded | 48.1 dB | 31.3 % | 18 |
| | capture-clocked | 46.5 dB | 33.8 % | 19 |
| `4ace`, extended | recorded | 55.2 dB | 15.0 % | 12 |
| | capture-clocked | 51.2 dB | 22.9 % | 14 |
| `4ace`, plain | recorded | 51.9 dB | 24.7 % | 12 |
| | capture-clocked | 53.9 dB | 18.0 % | 4 |

Better on two sessions, worse on one, level on one. No systematic loss of
cancellation from the resampled reference. The ring stayed between 960
and 1,490 frames of its 2,400 and no capture block lacked reference.

Limits: a replay, with the canceller starting cold; sessions of 91 s to
152 s, too short for the controller's learned rate to settle; and the
replay does not know how long ago the last packet was accepted, which the
worker does.

## Why the lamp's own voice cannot become a turn through this

Recovery leaves the microphone holding the lamp's voice uncancelled
(untrusted) or poorly cancelled (cold). Five things, each tested, keep it
from being admitted:

1. The worker gives no echo evidence while untrusted or warming
   (`trusted_blocks` in every recovery test).
2. Without evidence the duplex mask is the conservative mask
   (`without_echo_evidence_duplex_decides_as_the_conservative_baseline_does`).
3. The conservative mask admits nothing while accepted speaker audio is
   sounding or within its tail (the existing `v1_baseline_*` tests).
4. A reseed is a new DSP epoch, and a candidate still being decided across
   one is rejected (`a_dsp_reseed_ends_a_candidate_still_being_decided_...`).
   A person already admitted is not cut off.
5. `--reference-recovery bounded` is refused under `vad-only`, which masks
   nothing.

What that chain does not cover is the tail: the conservative mask ends
100 ms after playback, the room rang for up to 270 ms in the recordings,
and a cold canceller cancels that ring less well than a warm one. The
first reply of every session already starts that way.

## What the trace says

| Event | When |
|---|---|
| `run_start.reference_recovery` | `off` or `bounded` |
| `reference_status` | Each change of trust: `trust`, `cause`, the counters as they stood, `untrusted_us` on a reseed |
| `echo_reference_reset` | Each reseed and re-centre, as for any DSP reset |
| `reference_clock.details.recovery` | Once a second with recovery on: `degradations`, `reseeds`, `bypassed_capture_blocks`, `skipped_frames`, `late_packets`, `ignored_packets`, `trust` |
| `reference_fault` with `recovery_exhausted` or `recovery_timed_out` | The bounded end, with the timing and counters at that moment |

Each microphone frame carries `reference`: `trusted`, `untrusted` or
`warming`.

## Tests

Virtual time, production calls, real canceller, no device:

```sh
cargo test --locked --release -p lamp-live --lib -- reference::recovery
cargo test --locked --release -p lamp-live --lib -- carries_an_hour --ignored
cargo test --locked -p lamp-live --lib -- reference_recovery_ a_dsp_reseed
```

| Asked for | Test | Holds |
|---|---|---|
| The native failure | `the_fixed_margin_ends_where_the_native_run_ended` | `ReferenceBacklog` at 41,322 blocks without recovery |
| | `recentred_the_fixed_margin_goes_on_past_where_the_native_run_ended`, and the hour | Two re-centres in sixteen minutes, eight in an hour, 445 s apart, no bypass, no counter rewound |
| | `an_idle_room_is_recentred_before_the_bound_and_a_busy_one_at_it` | 22,250 blocks idle, 41,322 with the lamp or a person speaking |
| | `a_speaker_that_runs_slow_is_recentred_the_other_way` | `ReferenceLate` at 9,537 without; goes on with |
| Capture pauses | `a_capture_loop_held_up_with_nothing_missing_loses_nothing` | 15, 27 and 38 ms: no status, no reseed, every block cancelled |
| | `a_capture_loop_held_up_for_longer_than_a_microphone_holds_reseeds` | 70 ms: untrusted, one reseed, under 25 blocks bypassed in one stretch |
| Sender pauses | `a_speaker_held_up_within_its_queue_changes_nothing` | 15 and 30 ms |
| | `a_speaker_held_up_for_longer_than_its_queue_is_untrusted_until_fresh_reference` | 45, 70 and 100 ms: untrusted, reseed within 250 ms, three seconds of sound to warm |
| Both at once | `stalls_on_both_sides_at_once_are_one_recovery` | 70 ms: one reseed, not two |
| Clock skew | `the_capture_clocked_reference_needs_no_recentre_at_that_lamps_drift`, `the_rate_learned_survives_a_reseed` | No status at 157.3 ppm; the learned rate within 5 ppm across a reseed |
| Actual loss | `a_lost_packet_is_a_gap_that_is_reported_and_reseeded_past` | Expected 3,000, received 3,001; skipped frames counted |
| Duplicates, regression | `a_packet_seen_before_is_counted_and_never_used`, `a_cursor_or_a_clock_that_runs_backwards_is_never_recovered` | Six duplicates ignored; `CursorGap` and `QueueTiming` stay faults |
| Two recoveries, exhaustion | `two_recoveries_are_two_reseeds_and_a_fourth_in_a_minute_is_the_end`, `a_difference_no_clock_has_spends_the_budget_and_says_so` | Two, then `RecoveryExhausted` at the fourth; five over more than a minute |
| Bounded wait | `a_reference_that_does_not_return_ends_the_worker_within_its_bound` | `RecoveryTimedOut` 100 blocks after the ring ran dry |
| Playback ongoing | every test with `sound` | Warm after 299 to 306 sounding blocks; no evidence before |
| Privacy | `privacy_is_never_recovered_from` | `PrivacyMismatch`, no reseed |
| Speaker restart | `a_speaker_that_restarts_its_clock_is_a_cold_canceller_too` | Warming, cause `clock_restart`, budget untouched |

## Paced by the real clock

`crates/live/examples/reference_recovery_pace.rs`: two host processes, a
private datagram socket, the production `accept` and `capture_step` and
the real canceller, each side paced by the host's clock. A script stops
either process for a while, or loses or repeats a packet, at a given
second. There is no audio device: no ALSA, no USB clock and no device
buffer to overrun. Outputs are in
`docs/verification/reference-recovery-20261011/`.

Capture-clocked with recovery, the speaker 157.3 ppm fast:

| Script | Outcome |
|---|---|
| Speaker stops 15 ms | Nothing |
| Speaker stops 30 ms | Its packets 41 ms apart by its own clock: `sender_paused`, reseeded 9 ms later, one block bypassed, trusted again after three seconds of sound |
| Speaker stops 45 ms | `absent`, reseeded 11 ms later, one block bypassed |
| Speaker stops 70 ms, then 100 ms | Two recoveries: untrusted 40 ms and 70 ms, eleven blocks bypassed in all |
| Capture loop stops 15 ms, 27 ms | Nothing |
| Capture loop stops 38 ms | With the host's own lateness the loop was 46 ms behind: `overflow`, reseeded 44 ms later, nine blocks bypassed |
| Capture loop stops 70 ms | `overflow`, one reseed, nine blocks bypassed, five packets counted late |
| A packet lost, a packet repeated | `sequence_gap` at the loss, reseeded 39 ms later, four blocks bypassed; the repeat ignored and counted |
| Four packets lost in twenty seconds | Three reseeds, then `RecoveryExhausted` at the fourth |
| Speaker stops 2.5 s | `absent`, 101 blocks bypassed, then `RecoveryTimedOut` |
| A packet lost as the privacy generation moves on | `PrivacyMismatch`. No reseed |
| A packet lost, recovery off | `SequenceGap`, as always |

In virtual time a capture stop of 38 ms loses nothing. Under real pacing
the host added 8 ms to it and the ring overflowed: recovered, and a
reminder that the margin between "nothing" and "one reseed" is a few
milliseconds wide.

Three runs of ten minutes at 157.3 ppm, started together, with the ARM
gate and then the host test suite running on the same machine:

| Reference | Recovery | Ran | Outcome |
|---|---|---|---|
| Capture-clocked | bounded | **600 s of 600** | One hold-up the host made itself, at 520.45 s: the speaker's packets 43 ms apart, `sender_paused`, reseeded 13 ms later, four blocks bypassed, trusted again three seconds on. No fault. Learned rate 139 ppm and still settling after the reseed |
| Fixed | bounded | 160.4 s | Re-centred early at 139.6 s, the room being idle. Then the host held the capture loop for 31 ms and the packets it found were older than a reference may be: `QueueTiming`, the fault it always was |
| Fixed | off | 160.4 s | `ReferenceBacklog` at the same block: the same 30 ms on a margin already high |

This is the first paced run on this Mac to reach the length it was asked
for. Every earlier one, in `MAIN-050`, ended on a host stall of 40 ms to
65 ms; this one met one and recovered from it in 13 ms. The fixed
reference cannot do that, with recovery or without, and the second row
shows the review's first finding under real pacing: a hold-up at a high
margin was not passed off as drift.

## What an independent review found

Main's review of `a39d04d84`, the first commit pushed, found three
defects with a failing test for each. All three were mine. The tests are
kept as written, reformatted only, as the `audit_*` cases.

| Defect | Was | Now |
|---|---|---|
| A capture loop held up for 20 ms at a margin already high was re-centred as drift | The decision was made when the packet arrived, on the backlog the *previous* capture block had reported | Made at the next capture block on its own status. Held up means `ReferenceBacklog` and no reseed |
| Fresh reference arriving at the one-second deadline reseeded an interval that had expired | The reseed was tried before the deadline was checked | The deadline is checked first: `RecoveryTimedOut` |
| A queue observation from the future was counted as a late packet and its timing taken | Only its age was looked at | From the future, or from before the last one: `QueueTiming` |

The first is the one that mattered for the fix itself: it classified on
stale evidence, which is the thing recovery exists not to do.

## Not done

- **Nothing on a lamp.** The re-centre and the capture-clocked reference
  each need a session longer than six minutes on `4ace` before either is
  believed. The first thing to read is `reference_status`.
- **The devices' own discontinuities.** A lamp's microphone buffer is 640
  frames, 40 ms, and its speaker buffer at most 960, with about 456 queued.
  A capture loop held up for longer overruns the microphone; a speaker
  process held up for longer than its queue underruns the speaker. Both
  end the worker today, in `capture.rs` and `pcm.rs`. Recovering them means
  restarting a device and a clock epoch, in ALSA code I cannot run here.
  Until that exists, a stall of more than about 20 ms on the speaker side
  or 40 ms on the capture side still ends the session on a lamp, with or
  without this flag. No such stall is in any recording so far.
- **Owner changes and shutdown during recovery, through the coordinator.**
  Recovery touches no ownership, cancellation or stop state: it is all
  inside the capture worker's reference, and `Stop` and privacy are read
  first in every pass of its loop as before. That is by construction and
  is not yet a test of its own.
- **Whether the canceller needs the reset at a re-centre at all.** It has
  been given every render block; the bound that was reached is this
  code's count, not the canceller's. Resetting is the known state. Whether
  it would have gone on cancelling at a margin of nine is a question for a
  recording.
- **A candidate caught by a re-centre at the bound.** If someone's first
  200 ms coincide with one, that candidate is rejected and their utterance
  is drained. The early re-centre exists to keep this to rooms that are
  never quiet for half a second in two minutes.
- **`recovery` with `vad-only`.** Refused, by design.
