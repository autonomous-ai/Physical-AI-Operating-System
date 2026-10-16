# The same voice (#829)

Whether two things said to the lamp are the same voice. The question
comes from two people at the lamp: when B follows up on A's question
("And what about tomorrow?"), or A and B go on talking to each other after
the lamp answered, the busy-room rule now takes anyone who speaks within
45 s of an answer as the conversation going on
([attention.md](attention.md)). A face turned to the lamp cannot stand in
for it. On lamp-4ace on 15 October, 6 of the 9 answers let out because a
conversation was going on had nobody facing, all in one live exchange
([#829, 21:23](https://github.com/autonomous-ai/lampOS/issues/829); its
evidence is with pull request #864, `docs/verification/two-people-20261015/`).

## What exists

`crates/same-voice` (`lamp-same-voice`), built by the fourth builder on 15
October 2026: a library, a command to measure with, and since 16 October
the voice's child (below, "In the voice"). Nothing on the lamp runs it
until its model is put there and the voice is told where.

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

## In the voice (#829, 16 October)

With `LAMP_SPOKEN_TO=on`, a line said within 45 s of the lamp's last words
is taken for the conversation going on, whoever says it. On lamp-4ace on
16 October at 06:07, right after the lamp answered Daniel's "Hey Lamp, how
much is one hundred dollars in euros?", Samantha said "Anyway, if we drop
the trip we save almost half." to him. It rode on his conversation, and
only the model's own silence left it unanswered. With the model's path in
the voice's environment, `LAMP_SAME_VOICE_MODEL=/absolute/path.onnx` (the
worker's `--same-voice-model PATH`, after `--spoken-to`), a line in a voice
plainly not the conversation's is no part of it
(`crates/voice/src/same_voice.rs`):

- **Its own process.** The provider worker starts `lamp-same-voice serve
  MODEL`, installed beside `lamp-voice`, as it starts, so the model loads
  while the link opens. The child reads each line's sound on standard input
  and answers its print on standard output, one line of JSON
  (`lamp_same_voice::serve`). It takes one line at a time: a line asked for
  while it is busy, loading or being started again gets no print. A child
  that ends, or answers something that is not an answer to the line asked,
  is started again after 5 s. Without the child the rule is as without the
  model.
- **Only what was said with the lamp quiet.** The coordinator marks a
  `Start` that was admitted while a reply was in hand (`over_the_lamp`).
  Such a line, and a turn the lamp began itself, is never judged: with the
  lamp over it, the model hears the lamp (47.1% equal error, above).
- **Only a line of 1.5 s or more** (`LEAST_SAMPLES`). Its first 3 s
  (`MOST_SAMPLES`) are asked for as soon as they are heard, while the
  person is still talking; a shorter line is asked for at its end.
- **The conversation's voice** is the last voice the lamp answered that was
  long enough to know. A print that comes after its answer was let out
  still becomes it. A line answered as the conversation going on that is
  too short to know keeps it. One too short to know that began a
  conversation of its own (answered for its name, a face or anything but
  the conversation) leaves no voice known.
- **The rule.** A cosine under 0.25 (`DIFFERENT_BELOW`) between the line's
  print and the conversation's is another voice: no conversation is going
  on for that line, and only the name, a face, or close and turned to the
  lamp, let its answer out. The same voice, or no verdict, is as without
  the model. Each verdict is a journal line with its cosine: "another voice
  than the conversation's (cosine 0.12)", "the conversation's voice
  (cosine 0.91)".

**Its cost**, in the "would a person do this?" sense: B's direct follow-up
to the lamp's answer to A ("And what about tomorrow?", when 1.5 s or
longer) now needs B facing the lamp, or the name. A person at the desk
would usually answer it. That is why it is behind its own model path.

**The bar** is the measurement above, synthetic voices only: with both
clips 1.5 s or longer and the lamp quiet, 0.25 took none of 37 same-voice
pairs for two voices and caught 19 of 29 pairs of two. A minute of two
real people at the lamp is to set it.

Tests: `same_voice::tests` (which lines are judged and when, the
conversation's voice, a late print, a refused one and a busy child, the
flag, the child's answers), `lamp_same_voice::tests::serving_answers_each_request_with_its_print_or_why_not_until_its_input_ends`,
and through the worker's own loop
`with_the_switch_and_a_voice_check_another_voice_is_no_part_of_the_conversation`.
The child with the real model, its timing and memory on the A733, and
real voices are not shown.

## Not shown

- **People.** Every voice was synthetic and played from one loudspeaker.
  Real voices at 0.5, 1 and 2 m, in Vietnamese and English, are needed
  before any threshold is chosen (asked for on #829).
- **The lamp.** No timing or memory on the Orange Pi.
- **A reference against PyTorch.** The filter banks were checked against
  their definition (a tone lights its band; the mean is off), not against
  torchaudio's numbers. The same voice scoring 0.95 says they are close.
- **Many voices.** Three voices, 47 clips.
