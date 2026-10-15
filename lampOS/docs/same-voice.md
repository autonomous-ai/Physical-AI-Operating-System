# The same voice (#829)

Whether two things said to the lamp are the same voice. The question
comes from two people at the lamp: when B follows up on A's question
("And what about tomorrow?"), or A and B go on talking to each other after
the lamp answered, the busy-room rule now takes anyone who speaks within
45 s of an answer as the conversation going on
([attention.md](attention.md)). A face turned to the lamp cannot stand in
for it. On lamp-4ace on 15 October, 6 of the 9 answers let out because a
conversation was going on had nobody facing, all in one live exchange
([verification](verification/two-people-20261015/README.md)).

## What exists

`crates/same-voice` (`lamp-same-voice`), built by the fourth builder on 15
October 2026. Nothing on the lamp runs it yet: it is a library and a
command to measure with.

- **The model:** CAM++ from 3D-Speaker (Apache-2.0), as sherpa-onnx exports
  it (`3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx`, 29,596,978
  bytes, SHA-256
  `357a834f702b80161e5b981182c038e18553c1f2ca752ed6cec2052365d4129b`, from
  the `speaker-recongition-models` release of k2-fsa/sherpa-onnx). It has
  7.2 M weights and reports 0.73% EER on VoxCeleb1-O. It is not in the
  repository.
- **The front end:** 80 Kaldi filter banks a frame, computed in Rust as
  `torchaudio.compliance.kaldi.fbank` computes them with no dither, with
  the utterance's mean taken off each band, as 3D-Speaker does.
- **The run:** tract 0.23.8, pinned as in `lamp-name`, with the number of
  frames left open, so a clip of any length runs.
- `lamp-same-voice pairs MODEL.onnx LIST.tsv` prints the cosine of every
  pair of clips and how long each took.

## Measured (fourth builder's Intel Mac, 2026-10-15)

47 played lines from lamp-4ace's runs of 14 October, as the second
microphone gave them to the voice after the canceller: Daniel 28, Karen 9,
Samantha 10, all synthetic (`say`) from the Mac's loudspeaker. Clips with
the lamp sounding through more than half of either clip are counted apart
([verification](verification/same-voice-20261015/README.md)).

| Pairs | Same voice | Different | Equal error |
|---|---|---|---|
| The lamp quiet in both | 75 | 96 | 6.5% (cosine 0.29) |
| … the shorter clip 1.5 to 3 s | 16 | 29 | 0 errors |
| … the shorter clip under 1.5 s | 38 | 67 | 7.7% |
| The lamp sounding over one or both | 195 | 349 | 47.1% |

With the lamp quiet, two 6 s lines of the same voice scored a median 0.95.
With the lamp speaking over one of them, its own voice is what the model
hears: no better than chance. So the same voice can tell a follow-up after
the lamp has finished from another person's, and cannot tell who speaks
over it.

A clip took 75 ms for 1.5 s and 280 ms for 6 s on one thread of the Mac,
and loading took 1.8 s. The lamp's A733 has not been timed.

## Not shown

- **People.** Every voice was synthetic and played from one loudspeaker.
  Real voices at 0.5, 1 and 2 m, in Vietnamese and English, are needed
  before any threshold is chosen (asked for on #829).
- **The lamp.** No timing or memory on the Orange Pi.
- **A reference against PyTorch.** The filter banks were checked against
  their definition (a tone lights its band; the mean is off), not against
  torchaudio's numbers. The same voice scoring 0.95 says they are close.
- **Many voices.** Three voices, 47 clips.
