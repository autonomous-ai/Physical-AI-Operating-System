# A reference that stays put

Task 10 of the running list ([work-split.md](work-split.md)), by the second
builder, 2026-10-14. Built and measured on a laptop and its Linux virtual
machine; nothing here ran on a lamp.

The echo canceller (WebRTC's AEC3, the `sonora` port) is handed the
speaker's sound and learns where in it the lamp's echo lies in the
microphone. Its linear filter removes the echo only while it stays there.
On lamp-4ace it did not: against the speaker samples handed to the
canceller, the lamp's own voice reached the second microphone 91 ms late at
one moment and 129 ms late 23 s later, in steps of about 5.4 ms, and then
stepped back and forth by one such step every few seconds
([duplex-listening.md](duplex-listening.md), "On lamp-4ace, 2026-10-14").
While the canceller re-learns, its suppressor does the work and takes a
person with the echo.

`lamp_audio::steady::SteadyReference` stands between the reference and the
canceller and holds the echo in one place as the canceller sees it. It is
off unless asked for: `EchoProcessor::with_reference(_, Reference::Steady)`,
and `lamp-audio replay-aec … --steady-reference` over a recording.

## What exists and what we take

- **WebRTC's AEC3** (what the lamp runs) estimates the delay itself and
  re-converges when it changes. It assumes the render and capture streams
  run on one clock; keeping them so is left to the audio layer under it.
  Taken as it is: nothing in the canceller changes.
- **PulseAudio's echo-cancel module** keeps a speaker and a microphone on
  different cards together by measuring how far apart their buffers are and
  resampling to hold that steady (its `adjust_time` and `adjust_threshold`
  settings; read from its documentation, not measured here). PipeWire's
  echo canceller relies on its graph clock and adaptive resampling to the
  same end. Taken: the idea of resampling the reference to hold the echo
  still. Not taken: steering by buffer levels. Here what moves the echo is
  not in any buffer the lamp can read (below).
- **Phones and desktop assistants** take the reference from the same clock
  as the microphone, often in the audio hardware. The lamp's first
  microphone is on the speaker's card and its echo did not move by more
  than 0.1 ms; its second, the one used, is not.
- **The lamp's own capture-clocked reference** (`--reference-clock
  capture`, `crate::drift`) already resamples the reference onto the
  capture clock and keeps its ring at a steady fill. Taken: its
  interpolator, and the place in the voice where the reference meets the
  canceller.
- **The lamp's own echo evidence** (`crate::evidence::EchoObserver`)
  already finds where the echo lies, a quarter of a millisecond at a time,
  in bounded integer work per block, and has been checked on the lamp's
  recordings. Taken: a second one of it, given the reference as it comes,
  is the follower.

## Why the echo moves, as far as the code says

These are readings of the code against what was recorded, not
measurements.

- **Not the speaker's queue, in the mode the lamp runs.** The speaker holds
  at most 960 frames (40 ms) accepted and unplayed, filled a period at a
  time, which is the size of the swing recorded. But with `--reference-clock
  capture` every frame the speaker accepts goes into the ring, so a jump in
  its queue is a jump in the ring too and leaves where the echo sits
  unchanged; the ring's controller then drains the extra over about ten
  seconds (at most 500 ppm). That makes a slow slide, not a step.
- **The steps are on the microphone's side, most likely.** Steps of about
  5.4 ms that go back and forth every few seconds, on the second microphone
  only, a USB device with an asynchronous endpoint and a chip of its own,
  while the first microphone on the speaker's clock held still: that is the
  second microphone's own buffering giving or taking a block. Nothing the
  lamp reads reports it.
- **The delay hint does not carry it either.** In capture-clocked mode the
  hint the voice passes the canceller is the speaker's queue less the
  frames waiting in the ring, floored at zero, plus the capture delay. The
  ring holds about 1,200 frames and the queue at most 960, so that part is
  always zero: the hint is the capture delay alone.

So the steady reference follows the echo from the sound itself, and needs
nothing from the speaker or the microphone's driver.

## What it does

- A second `EchoObserver` (the follower) is given each render block as it
  comes and each microphone block before cancellation, and reports how long
  after the newest render the echo arrives (`EchoObserver::echo_delay`,
  added for this).
- Once it has found the echo, the reference is delayed so that the
  canceller finds the echo 20 ms after it (`HOLD_FRAMES`). From then on the
  delay follows the echo: a move of one analysis sample (0.25 ms, a clock's
  drift) at once, a larger one once it has held for 5 blocks (50 ms), so
  that a passing match a pitch period away moves nothing.
- The delay changes by at most 2 ms a block, read through `drift`'s
  windowed-sinc interpolator. For a block or two after a step the reference
  is stretched or squeezed by up to a fifth; no sample is dropped, repeated
  or made up. Until the echo is first found the reference passes 0.5 ms
  late, the least the interpolator needs.
- Where it holds the echo depends only on where the echo is found now, so a
  first match that was wrong costs time and nothing after.
- The delay hint given to the canceller shrinks by the delay applied. The
  canceller and the echo evidence both see the delayed reference; the
  speaker plays what it is given.
- Bounded: a 16,384-frame ring, no allocation after construction, no clock,
  no I/O. On reset of the canceller it is reset too.

## The rig

`crates/audio/tests/steady_reference.rs`, no lamp in it:

- **The lamp's voice**: speech-like sound made in the test (a voice source
  at 205 Hz through moving formants, consonant noise, syllables, pauses), at
  24 kHz, as the speaker is given it.
- **The speaker** plays it on a clock 98 ppm fast (as measured on
  lamp-4ace) and accepts it in 240-frame packets with two packets queued.
- **The path**: a direct sound and four reflections over 15 ms, and a delay
  that follows the recording of 2026-10-14: 85.6 ms, then eight steps of
  5.4 ms to 128.8 ms between 10 and 37 s, then one step back and forth every
  3.5 s. One scene adds a distortion that makes half of the echo not a
  linear copy.
- **The microphone** at 16 kHz on a clock 200 ppm slow (300 ppm from the
  speaker), a −63 dBFS noise floor, the echo at −14 dBFS.
- **A person** at 118 Hz speaks over the lamp six times for 1.5 s, level
  with the echo or 15 dB under it, and once alone at the end.
- **The voice's side** is the capture-clocked reference as the lamp runs it:
  accepted packets into `drift::Resampler`, one render block read for each
  capture block at the rate `drift::Controller` asks, as
  `ReferenceAssembly::render_for_capture` does.

Measured, from the rig's own ground truth: how much of the echo the
canceller takes off while only the lamp speaks (median of 1 s windows); how
much of the person it leaves while both speak and while the person is alone
(the person's part of the output against the person passed alone through a
canceller that hears no lamp, so the canceller's own filtering counts as
nothing lost); and where the echo sits as the canceller sees it.

## The two measures, before and after

`cargo test --release -p lamp-audio --test steady_reference -- --include-ignored --nocapture`,
on the M2 Max (macOS) and on its Linux ARM64 virtual machine, one run each,
the same figures to the last digit on both (the rig is deterministic); 60 s
scenes, the first 6 s not measured.
Before is the reference as given; after is the steady reference.

| Echo | Person against the echo | Echo removed, before → after | Person kept over the lamp, before → after | Where the canceller sees the echo, before → after | Within 1 ms of where it is held, before → after | The canceller's own estimate, before → after |
| --- | --- | --- | --- | --- | --- | --- |
| Still | level | 45.0 → 44.0 dB | −3.8 → −4.0 dB | 36.7–45.2 → 5.2–20.4 ms | 24% → 95% | 28–40 → 0–16 ms |
| Still | 15 dB under | 45.1 → 41.8 dB | −9.4 → −8.1 dB | 36.7–45.2 → 5.2–20.4 ms | 24% → 95% | 28–40 → 8–16 ms |
| Moving, as recorded | level | 46.5 → 44.6 dB | −5.9 → −3.7 dB | 36.7–87.9 → 11.4–27.7 ms | 7% → 76% | 28–84 → 0–20 ms |
| Moving, as recorded | 15 dB under | 45.3 → 43.3 dB | −8.5 → −7.1 dB | 36.7–87.9 → 11.5–27.7 ms | 7% → 81% | 28–80 → 0–20 ms |
| Moving, half not linear | level | 43.9 → 40.4 dB | −5.6 → −3.7 dB | 36.7–87.9 → 14.0–27.7 ms | 7% → 71% | 28–92 → 0–20 ms |
| Moving, half not linear | 15 dB under | 44.5 → 40.8 dB | −11.0 → −8.7 dB | 36.7–87.9 → 14.0–27.7 ms | 7% → 73% | 28–96 → 0–16 ms |

A person alone was kept at 0.0 dB in every scene, before and after.

What it shows:

- **The moving echo costs a person about 2 dB, and the steady reference
  gives it back.** Over the lamp, at the person's level, 5.9 dB of the
  person is lost as given and 3.7 dB steadied, about what a still echo
  costs (3.8 dB). At 15 dB under, 8.5 against 7.1 dB; with half of the echo
  not linear, 5.6 against 3.7 and 11.0 against 8.7 dB.
- **The echo stays put.** As given it wandered over 51 ms as the canceller
  saw it (the steps and the ring's own controller learning the clock
  difference, which alone moves a still echo by 8.5 ms in a minute). Steadied
  it is within 1 ms of 20 ms for 71 to 81% of the time, never more than one
  step away; the lows of 5 to 14 ms are the seconds before the follower
  first found the echo. The canceller's own estimate stops wandering.
- **It costs some echo removal**: 1 to 3.5 dB less, from over 44 to over
  40 dB, while only the lamp speaks. The canceller's suppressor clears what
  is left there either way.
- **Small, not the cure.** On this rig the canceller copes with a moving
  echo better than on the lamp, where little of the echo is a linear copy
  and the person's level against it is lower. The rig shows the direction
  and the size of the effect, not the lamp's.

The default tests (`cargo test -p lamp-audio --test steady_reference`,
7 s in a debug build) hold the findings without the canceller: as given the
echo moves over more than 40 ms; steadied it is within 1 ms of where it is
held for more than 70% of a 45 s scene (82% measured) and within one step
for more than 97% (100% measured).

**What follows a step costs.** After each 5.4 ms step the echo sits one step
away for up to about a second: the follower has to tell the step from the
voice's pitch period (4.9 ms at 205 Hz), and does so where the voice
changes. Asking a moved alignment to hold for 3 blocks in place of 5 was no
quicker (76% within 1 ms either way) and kept less of the person, so 5 was
kept.

**Its cost:** the steady reference's render and capture calls together, a
10 ms block at a time over the moving scene, in the same test: on the M2 Max
a median of 6 µs (99th percentile 24 µs, worst 35 µs); on the Linux ARM64
virtual machine 12 µs (52 µs, 144 µs). Neither is the lamp.

## On the lamp

Wiring, for the first builder: where the capture worker makes its
`EchoProcessor::new(noise_suppression)`, make it
`EchoProcessor::with_reference(noise_suppression, Reference::Steady)` behind
a voice option (for example `--steady-reference`), off unless set.

Before that, over the recordings of 2026-10-14 (or any recorded with the
audio diagnostics), run the replay twice:

```sh
lamp-audio replay-aec RUN artifacts/replay-given
lamp-audio replay-aec RUN artifacts/replay-steady --steady-reference
```

Each segment of `report.json` now says, over the blocks while the lamp was
sounding (`lamp_sounding_blocks`): `lamp_sounding_echo_removed_db`,
`lamp_sounding_internal_alignment_ms` (the canceller's own estimate, lowest
and highest) and `lamp_sounding_internal_alignment_changes`, and with the
option `steady_delay_ms`. Each `capture_pair` in `operations.jsonl` carries
`steady_reference` (the delay applied, its target, and the echo's delay as
followed). What it should show: the canceller's estimate within a few
milliseconds and changing rarely with the option, against 80 to 128 ms as
given; and in the played-word runs, the words over the lamp louder in
`segment-*-aec_only.wav` with the option than without. The person's words
are in the echo-removed figure too, so compare the files by ear at the
words.

**Where it is most likely to be wrong on the lamp:** the follower. It needs
a linear fit at one alignment to explain at least half of the microphone's
energy, and on the second microphone little of the echo is a linear copy
(1.8 to 14.3 dB of possible linear removal, median under 7). Where it
explains less, the follower keeps its last delay, and the reference stays
as it was; where a match one pitch period of Gemini's voice away explains
more, it may hold that for a while. The replay's `steady_reference.echo_ms`
over time shows which: it should step as the recording's echo stepped, by
about 5.4 ms.

## Taken by the first builder (2026-10-14)

On `lamp-voice` as it was made (`64ebb8b33`); the audio crate's tests
pass on the first builder's Mac (153, two ignored) and clippy is clean.

**The replay could not be used on the lamp's recordings.** `lamp-audio
replay-aec` was run over four of the recordings of 2026-10-14
(`cut-a`, `cut-b`, `cut-c`, `cut-d`: words played over the lamp's voice),
as given and with `--steady-reference`. Three stopped at once with
`insufficient_evidence_for_dsp_replay` and no frame replayed ("stopped at
first unsupported/ambiguous boundary: capture/analysis cursor, phase
count or clock continuity is insufficient"), and the fourth could not
read the recording's events. Those recordings were made as the lamp
runs, with the capture-clocked reference, and the replay does not take
them as they are. So nothing was learned from a replay, in either
direction.

**So it is wired, to be measured live.** The voice has
`--echo-reference steady` (`EchoReference` in
`crates/voice/src/options.rs`), which reaches only the capture worker and
makes its canceller with `Reference::Steady`. Without the flag every
argument and every canceller is what it was. It is not on the lamp's
unit. The measure that counts is the one the lamp fails today: of ten
sentences and seven short words played over its own voice with nobody
in the office (`cutins.sh`), how many stop it, with and without the
flag, the same evening.

## Not shown

The lamp; the lamp's second microphone and its chip, which change the sound
in ways the rig does not copy; a person's voice; whether the 2 dB the rig
gives back is heard by anyone; the cost on the lamp's processor; the
steady reference inside a running voice. The causes of the movement are
read from the code and the recordings' description, not measured.
