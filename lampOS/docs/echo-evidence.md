# Echo evidence

Scope: `crates/audio/src/evidence.rs`, its use in `EchoProcessor`, and the
offline replay. Nothing here changes admission, the wire or the device.

## Why

Admission accepts a turn on speech probability alone, and the speech detector
listens to the microphone after echo cancellation. Whatever of Lamp's own
voice the canceller has not removed looks to that detector exactly like a
person. One recorded run admitted a second turn 846.5 ms after the first reply
began, with nobody speaking ([input admission](input-admission.md)).

The detector cannot tell the two apart. Something that also sees what the
speaker was given can. This module is that something. It produces
measurements for an admission decision made elsewhere. It decides nothing.

## What it reports

`EchoProcessor::evidence()` returns an `EchoEvidence` for the capture block
just processed:

| Field | Meaning |
|---|---|
| `blocks` | Capture blocks since the canceller was built or reset |
| `render_active_blocks` | Of the last 50 render blocks (500 ms), how many carried non-silent accepted speaker PCM |
| `capture_level` | Microphone level before cancellation |
| `residual_level` | Level after cancellation: what the speech detector and the provider receive |
| `echo` | Present only while non-silent render is matched to the capture |

and inside `echo`:

| Field | Meaning |
|---|---|
| `render_level` | Level of the accepted render over the matched window |
| `capture_explained` | Percent of the capture's energy, before cancellation, that the render explains |
| `residual_explained` | The same for the cancelled signal |
| `alignment_age_blocks` | Blocks since the alignment last explained at least half the capture |

Levels are tenths of a decibel relative to full scale, over the last 120 ms,
about the mean, in the analysis band below roughly 2 kHz. They are not
full-band loudness.

`residual_explained` is the number the speech detector lacks. High means what
remains is still mostly Lamp's voice. Low means it is something the render
does not explain: a person, another device, a noise. Which of those, and
whether Lamp was addressed, this cannot know.

**Absent is not "no echo".** `echo` is absent while the render is silent and
while it is active but not yet matched. A consumer must treat the second case
as undetermined: `render_active_blocks > 0` with `echo` absent.

## How

The observer sees the three streams the canceller sees, in the same order:
PCM the speaker device accepted, the microphone block, and the cancelled
block. Provider audio that was never accepted by the speaker never reaches it.

1. Both streams are reduced to 4 kHz by plain averaging (six render samples or
   four capture samples to one, then each with its predecessor). The two get
   the same frequency response.
2. The last 120 ms of capture is compared with the render at one alignment:
   the squared normalized correlation, about the means, is the share of the
   capture's energy that a scaled copy of the render accounts for. Polarity
   does not matter.
3. The alignment is the difference between the two streams' sample counts, so
   it does not move when render blocks arrive in bursts ahead of capture. It is
   searched from no delay back to 400 ms, 32 alignments per block, and adopted
   when it explains at least half the capture. Once held, its two neighbours
   are tried every block so slow clock drift is followed, and the search
   continues outward from it so an alignment lost during silence is found
   again.
4. The reported shares fit the render at the held alignment and one sample
   either side together, by least squares, because a real delay is not a whole
   number of samples.

A silent capture neither confirms nor unseats an alignment. A person speaking
over playback lowers the share and does not unseat it either; the age field
says how long ago it was last confirmed.

## Bounds

| Bound | Value |
|---|---|
| State | 4,096 render samples, two windows of 480 capture samples, a few counters. No allocation after construction |
| Work per capture block | 32 searched alignments, 3 tracked, 2 three-point fits, each over 480 samples |
| Measured cost, host, release build | 9.0 µs per block, observer alone, one thread |
| Measured cost, host, debug build | 0.46 ms per block |
| Delay searched | 0 to 400 ms from acceptance by the speaker device |
| Time to first match in the scripted scene | 166 ms after the first echo reached the microphone |

No I/O, clock, lock or thread. Integer sums; floating point only for the
final ratios and levels.

## What was tested

Host, scripted scenes, `crates/audio/tests/evidence.rs`. Every signal is a
function of time sampled at 24 kHz for the speaker and 16 kHz for the
microphone. The echo path is a 113.5 ms delayed, inverted copy at half
amplitude with one reflection 4 ms later, plus a noise floor. The scenes run
through the real canceller unless marked. **This is a simplification of a
speaker beside a microphone, not a room, and none of it is acoustic
evidence.**

| Scene | Result |
|---|---|
| Playback alone, 5.5 s | Matched 166 ms after the first echo; `capture_explained` median 94; no block below 50 in 5 s; nothing claimed before playback or after its last echo |
| A person alone | Never matched; `render_active_blocks` 0 throughout; levels -27.5 dB before and -29.5 dB after cancellation |
| A person over playback at -6, 0 and +6 dB relative to the echo | `capture_explained` 76, 48, 19 against 75, 47, 19 predicted from the mixture; `residual_explained` 1 in each: once the canceller has converged, what remains is the person |
| Scripted residual, observer alone: half the echo left in | `residual_explained` 94 with nobody speaking; 47 with a person as loud as the leak; 18 with a person 6 dB above it; 1 with no leak |
| Microphone clock 300 ppm fast for 20 s | Alignment followed; median 91 in the last 4.5 s; no block unmatched or stale |
| Render accepted in bursts 1 to 6 blocks ahead | Within 2 points of steady acceptance |
| Canceller reset | Evidence absent until the next capture, then counts from 1 with no match |

The canceller removed 10 dB from this synthetic echo before the first match,
so the recorded false interruption is not reproduced through it. The scripted
residual stands in for a canceller that has not converged.

## What is not established

- **No threshold.** Nothing here says at which share a turn should be
  refused. That needs recordings of real playback with and without a person
  speaking over it, at the desk, and none with its render reference is
  available off the device.
- **A real room.** Reverberation spreads the echo over many alignments. A
  three-point fit will explain less of it than of a scripted path. One
  recorded session measured a normalized correlation of -0.966 between
  microphone and accepted render at a single offset
  ([AEC alignment](aec-alignment.md)), which suggests the direct path
  dominates on this hardware; that is one window of one run.
- **Loudspeaker distortion.** Distortion products are not explained by the
  render and will read as "something else".
- **Low speech-to-echo ratios.** A quiet person under loud playback moves
  `capture_explained` little. `residual_explained` is the better witness
  there, and only once the canceller is doing its work.
- **Who is speaking and to whom.** Out of scope for any measurement of this
  kind.
- Device cost. 9 µs is a host figure. The emulated and native figures are
  not measured yet.

## Scoring a recording

`lamp-audio replay-aec` now writes, on every `capture_pair` line of
`operations.jsonl`:

```json
"echo_evidence": {"recorded": {...}, "aec_only": {...}, "aec_ns": {...}}
```

`recorded` compares the microphone with the cancelled signal the device
itself produced in that run. The other two belong to the replayed cancellers.
Any existing diagnostic bundle with its accepted render can be scored this
way without new device software, including the recorded false interruption
once its raw files are at hand.

## Integration

Nothing on the wire carries this yet. The request is one optional field,
`MicrophoneFrame.echo: Option<lamp_audio::evidence::EchoEvidence>`, set by the
capture worker from `EchoProcessor::evidence()` after each `capture`. It
describes the frame it travels with, so that frame's freshness and lineage
rules cover it. See request `A1-003` in
[Agent 1's status](../coordination/agent-1.md).

A reset of the canceller resets the observer. The alignment therefore has to
be found again after every DSP epoch change, which takes one search pass once
playback resumes.
