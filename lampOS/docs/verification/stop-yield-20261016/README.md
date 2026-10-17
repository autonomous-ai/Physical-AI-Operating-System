# Why a short stop word makes no yield: #828, 2026-10-16

Fourth's evidence for `LAMP_STOP_WORDS=heard-always`
([#828](https://github.com/autonomous-ai/lampOS/issues/828),
[stop-word.md](../../stop-word.md#a-stop-word-heard-over-the-lamps-voice-828-16-october)).
Read and run on Fourth's Intel Mac (macOS 26.6.2, i7-10700K, Rust 1.96.0).
Nothing was run on a lamp. The words are the synthetic played lines.

## The recordings

The first builder's two recordings of 16 October at 01:51 on lamp-4ace, on
the Mac the builders share (`~/lamp-voice-data/names/cutab-night0151-now/`
and `…-6db/`, out of Git), as in
[first-words-20261016](../first-words-20261016/README.md). A story was
asked for, then a short line was played over it from the Mac's speaker.
The lamp's speaker was at step 9 ("now") and at step 3 ("-6 dB").
`plays.tsv` says when each line was played, on the lamp's clock.

## What was run

1. `lamp-audio replay-aec RUN OUT` (the standard suppressor, as on the
   lamp) on each recording.
2. `crates/voice/examples/hearing_replay.rs` on each replay: every block
   scored by the voice's detector and judged by `Hearing::EVALUATED`, with
   the replayed canceller's own echo evidence (`segment-001-hearing.tsv`).
3. Counts, in Fourth's scratch folder (lampOS is Rust only):
   - for each played cut-in, its blocks from its start to 300 ms after
     its end: how many were judged each way, and the longest run judged
     "over" (the duplex rule yields on 7, `Confirmation::EVALUATED`);
   - the residual's level in the second before each line and over it;
   - a rule of N blocks in a row, speech-like, while the lamp sounds,
     with the residual T dB over the median of its previous 100 blocks:
     the cut-ins it catches, and how often it fires with only the lamp
     sounding (outside every played line and 300 ms after it);
   - capture minus residual (what the canceller takes off) over the
     speech-like blocks with the lamp sounding, inside the cut-ins and
     with the lamp alone.

## What it showed

Nine cut-ins were said while the lamp was sounding; three came after it
had stopped, were heard in the clear, and need no yield.

| Run | Line | Speech-like blocks | Judged the lamp's own | Longest run "over" |
|---|---|---|---|---|
| now | "Stop." | 0 | 0 | 0 |
| now | "Wait." | 70 | 50 | 20 (yields) |
| now | "Lamp, stop talking." | 28 | 28 | 0 |
| now | "Okay, stop." | 51 | 47 | 4 |
| -6 dB | "Stop." (Samantha) | 25 | 25 | 0 |
| -6 dB | "Lamp, stop." | 13 | 13 | 0 |
| -6 dB | "Stop." (Daniel) | 27 | 27 | 0 |
| -6 dB | "Lamp, stop talking." | 50 | 22 | 16 (yields) |
| -6 dB | "Okay, stop." | 1 | 1 | 0 |

- In the speech-like blocks over the seven that made no yield, the lamp's
  own sound explained a median 96 to 99% of the microphone (the bar for
  "the lamp's own" is 75%). The other blocks scored under 0.5 after the
  canceller.
- Over most cut-ins the residual's top tenth rose 47 to 74 dB above its
  median in the second before.

The residual rule, both runs together (143 s of the lamp sounding alone):

| T | Run | Cut-ins caught | Firings with the lamp alone |
|---|---|---|---|
| 15 dB | 7 | 6 of 9 | 27 |
| 20 dB | 5 | 7 of 9 | 39 |
| 20 dB | 7 | 6 of 9 | 22 |
| 20 dB | 10 | 5 of 9 | 12 |
| 30 dB | 7 | 2 of 9 | 17 |
| 40 dB | 7 | 1 of 9 | 14 |

The firings with the lamp alone came throughout its replies, 41 to 1,549
blocks after it began sounding, not only while the canceller settled.

Capture minus residual, speech-like blocks with the lamp sounding (244 in
the cut-ins, 833 with the lamp alone):

| | p10 | p25 | p50 | p75 | p90 |
|---|---|---|---|---|---|
| Cut-ins | 1.5 dB | 7.1 dB | 13.7 dB | 31.7 dB | 39.0 dB |
| The lamp alone | 2.6 dB | 7.2 dB | 17.4 dB | 37.8 dB | 47.9 dB |

## What it means

On these recordings, no sign per block tells a short stop word said under
the lamp's voice from its own voice let through, so the duplex rule
cannot yield for it, and words heard only in a yield never hear it. Words
heard over the whole reply can.

## Not shown

A person's own voice at the lamp, the device's canceller in place of the
host's replay of it, other speaker levels and rooms, and what the relay's
speech-to-text makes of the canceller's output while the lamp speaks.
