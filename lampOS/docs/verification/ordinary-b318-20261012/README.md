# Real Lamp ordinary conversation, 12 October 2026

Latest runtime `b3186e7e…73f4`, frozen source manifest `049a86c2…4b10d`,
played seven cached iMac questions once. Six received completed replies; the
seventh was cancelled by an unplanned detected input before any reply audio.
The eighth question was withheld. **The run remains failed/incomplete.**
[Exact evidence](results.json) retains the failed outcomes as well as timings.

## Latency

For six completed replies, detected endpoint to first ALSA-accepted speaker
write was **1.424 s median**, range **1.266–1.505 s**. The configured **1.2 s**
low-VAD silence wait precedes these values. Adding it gives an estimated total
**2.624 s median**, range **2.466–2.705 s**. This is not an acoustic first-word
measurement; the valid room recording remains unscored.

The provider's first audio arrived 1.184–1.427 s after endpoint. Delivery from
that observation to the first speaker write took 46–185 ms. These intervals
are on the same Lamp clock. Each of the six replies has provider completion,
final-sample retirement, and zero reported playback gaps. Acoustic continuity
and semantic correctness are not certified by those receipts.

## Failure and integrity

Candidate 8 displaced pending turn 7 **1.249 s after its endpoint**. No provider
audio had arrived for turn 7. The internal `user_interrupted` label does not
establish that a person spoke or intended to interrupt; the physical source
remains unknown. The controller stopped after this failed requested outcome.

An independent audit verifies **7,425 unique contiguous events** and the
complete **180-second room WAV**, with zero reported drops, clipping, xruns or
timestamp discontinuities. Its observed callback coverage contains the entire
target window. The original helper's secondary missing/duplicate-event error
is contradicted by the archived segments; the primary cancellation remains real.
No raw transcript or recording is published. The runtime stopped and stationary
muted restoration, original mixers and hardware-owner release verified.

Audio, cached prompts, delivery receipts and the full failed result are retained
under `.cache/physical-audio/20261012/ordinary-b318-wpezmg8i`. They can be replayed
without speech regeneration. No V1 benchmark was run. The final natural-voice
latency target and full V2 acceptance remain unmet.
