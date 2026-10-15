# The same voice, first measurement: #829 step 2, 2026-10-15

Fourth's evidence for [docs/same-voice.md](../../same-voice.md). Run on
Fourth's Intel Mac (macOS 26.6.2, i7-10700K, Rust 1.96.0). Nothing was run
on a lamp. Synthetic voices only.

## The clips (`clips.tsv`)

Every played line of 0.5 s or more from lamp-4ace's runs of the morning of
14 October (`~/lamp-voice-data/sessions/duplex-20261014/`, the first
builder's copies, local only), cut by the lamp's clock out of what the
second microphone gave the voice after the canceller
(`capture-audio/post_aec.pcm16le`): from each line's `plays.tsv` start to
its end, the byte offset found from the capture records' read times. They
were written as 16 kHz WAVs to Fourth's scratch folder, not kept.

`sounding`: the share of the clip's blocks in which the lamp's speaker was
sounding (`render_quiet` under 30 in the standard replay's hearing table of
#828, `docs/verification/stop-word-20261015/`). cut-c has no replay, so its
clips are left out. "Lamp quiet": under 0.1; "over": over 0.5.

## What was run

- `lamp-same-voice pairs campplus_en_voxceleb.onnx list.tsv`
  (`pairs.tsv`, `timing.txt`), with the model of
  [docs/same-voice.md](../../same-voice.md), checked by its SHA-256.
  `pairs.tsv` is from `cd6917f51`, whose cosine sums in f64: one of the
  1,081 values differs by 0.0001 from the first run's, and
  `separation.tsv` is the same from either.
- A Python script in Fourth's scratch folder, outside the repository
  (lampOS is Rust only), for `separation.tsv`. For each pair it took the
  same-or-different from the clips' voices and the condition from both
  clips' `sounding`. For each group it swept the cosine threshold from
  -0.5 to 1.0 in steps of 0.005 and reported where the share of same-voice
  pairs below it and of different-voice pairs at or above it were closest,
  their mean as the equal error.

## What it showed (`separation.tsv`)

| Pairs | Same voice | Different | Equal error |
|---|---|---|---|
| The lamp quiet in both | 75 | 96 | 6.5% (cosine 0.29) |
| … the shorter clip 1.5 to 3 s | 16 | 29 | 0 errors |
| … the shorter clip under 1.5 s | 38 | 67 | 7.7% |
| The lamp sounding over one or both | 195 | 349 | 47.1% |

Two 6 s lines of the same voice with the lamp quiet scored a median 0.95.
A clip took 75 ms for 1.5 s and 280 ms for 6 s; loading took 1.8 s.

## Not shown

Real people, the lamp's own timing, a check of the filter banks against
torchaudio's numbers, more than three voices.
