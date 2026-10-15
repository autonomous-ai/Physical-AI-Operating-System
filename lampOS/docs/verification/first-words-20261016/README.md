# A yield's first words: #828 step 2, 2026-10-16

Fourth's evidence for `LAMP_FIRST_WORDS=kept`
([#828](https://github.com/autonomous-ai/lampOS/issues/828),
[duplex-listening.md](../../duplex-listening.md#first-words-kept-828)).
Read and run on Fourth's Intel Mac (macOS 26.6.2, i7-10700K, Rust 1.96.0).
Nothing was run on a lamp. The words are the synthetic played lines.

## The recordings

The first builder's two recordings of 16 October at 01:51 on lamp-4ace, on
the Mac the builders share (`~/lamp-voice-data/names/cutab-night0151-now/`
and `…-6db/`, out of Git). A story was asked for, then a line was played
over it. The lamp's speaker was at step 9 ("now") and at step 3 ("-6 dB").
Each holds the voice's audio diagnostics, its `events.jsonl`, and
`plays.tsv`, which says when each line was played, on the lamp's clock.

## What was run

1. `lamp-audio replay-aec RUN OUT` (standard suppressor, as on the lamp) on
   each recording.
2. `crates/voice/examples/hearing_replay.rs` on each replay: every block
   scored by the voice's detector and judged by `Hearing::EVALUATED`.
3. A count, in Fourth's scratch folder (lampOS is Rust only). For each
   `input_candidate` in the run's `events.jsonl`, it finds the played line
   it belongs to, and where the candidate's audio began
   (`first_read_at_us`) against the line's start.
   - It then walks back from the trigger block over the last 160 blocks.
     It takes the earliest block the detector scores 0.35 or more, stops at
     a block without echo evidence, and begins 20 blocks before it, as
     `TurnDetector::begin` does.
   - If no such block comes before the usual 300 ms, the start is
     unchanged.

## What it showed

Milliseconds after the played line began; positive means the line's
opening words were never sent. Only a yield, a candidate that displaced
the lamp's reply, reaches back.

| Line, over the story | Run | Today | With first words kept |
|---|---|---|---|
| "Hey Lamp, … sleepy dragon …" | -6 dB | 1,347 | 57 |
| "Hey Lamp, … pirate …" | -6 dB | 1,303 | 243 |
| "Hey Lamp, … owl …" | -6 dB | 366 | −924 |
| "Hey Lamp, … lighthouse keeper …" | -6 dB | 1,720 | 430 |
| "Hey Lamp, … sleepy dragon …" | now | 2,464 | 1,744 |
| "Wait." | now | 1,175 | −116 |

- Candidates made with the lamp quiet began 181 to 233 ms in, at the
  detector's onset lag, and are unchanged.
- One more line, "lighthouse keeper" in the "now" run, began 1,337 ms in
  without a yield. The switch doesn't reach it.
- The replays had no block without echo evidence. On the lamp, such a
  block stops the walk.
- With gaps of at most 400 ms between speech-like blocks, the walk
  recovered only two of these. The canceller takes out much of the
  person's speech under the lamp's voice, so the gaps are long, and the
  rule takes any gap.

## Not shown

- Nothing was played. A played minute, with the switch, of the cut-ins
  (`cutins.sh`) and the stories asked over a story: are the sent requests'
  first words now heard ("Lamp, stop. What is two plus two?")?
- What the model makes of the lamp's own suppressed voice at the front of
  a request (the owl, 0.9 s).
- A host replay is not the lamp: the standard replay of a 14 October run
  differed from what lamp-4ace recorded in 26% of samples, at a level
  within 0.7% (`stop-word-20261015`).
