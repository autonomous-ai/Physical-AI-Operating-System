# A pause at a comma ends the person's turn: #782, 2026-10-15

Fourth's evidence for task 50 ([#782](https://github.com/autonomous-ai/lampOS/issues/782)).
Run on Fourth's Intel Mac (macOS 26.6.2, i7-10700K, Rust 1.96.0). Nothing
was run on a lamp. The scenes were played to lamp-4ace by the first builder
with synthetic voices (`say`, rate 185); their records are the first
builder's, on the Mac the two builders share
(`~/lamp-voice-data/scenarios/20261015T*.txt`), copied to Fourth's scratch
folder and read there. Nothing kept here holds a word of what was said:
counts, times, scores and hashed line ids only.

## How often (`counts.json`, `cuts.json`)

- 44 scenes, 158 ends of speech: 84 `model_complete` (400 ms of quiet at
  p 0.9 or more, 75 of them at exactly 400 ms), 39 `silence_fallback`
  (600 ms), 35 `model_uncertain` (600 ms).
- The plain count the issue asks for (`counts.json`): a `model_complete`
  end followed within 2 s by `user_interrupted` and an `input_admitted`,
  13 of 84. That also counts the next person's line in two-people scenes.
- A cut (`cuts.json`): an end whose quiet began more than 0.25 s before the
  played line's end, the line's length taken from the line rendered again
  by `say` with its voice and rate, to a file, never to the loudspeaker.
  Each cut line was rendered again up to each comma or full stop, to put
  the cut at a mark when its quiet began within 0.25 s of the mark's end.

| End | Ends | Cut a line | Rest admitted within 2 s | At a comma | At a full stop |
|---|---|---|---|---|---|
| `model_complete` | 84 | 10 | 9 | 9 | 1 |
| `silence_fallback` | 39 | 8 | 8 | 2 | 6 |
| `model_uncertain` | 35 | 0 | — | — | — |

The six full-stop cuts by the window are the two-breath scenes. The two
comma cuts by the window are "Hey Lamp," at 0.66 to 0.74 s into the line.
Comma pauses as lamp-4ace heard them (the rest's admission less the 60 ms
admission takes): 400, 460, 460, 460, 470, 490, 500, 550 and 560 ms for the
nine `model_complete` cuts; 600 and 630 ms for the two "Hey Lamp," ones.
`say` adds 0.33 to 0.45 s for one comma, 0.63 to 0.72 s for two (rendered
with and without it, Samantha and Daniel).

## The replay (`replay/`)

`crates/voice-eval/examples/turn_end_replay.rs`: each line as `say` renders
it (16 kHz mono, 600 ms lead-in, 2.5 s tail at −63 dBFS) through the voice's
own `TurnDetector` with lamp-4ace's 600 ms window, and the pinned Smart Turn
model (`smart-turn-v3.2-gpu.onnx`, SHA-256 `ab8dc64b…dc539be`) in-process,
each verdict handed over 130 ms later on the replay's own clock.

- `clean.jsonl`, `score-replay-1.txt`: seven profiles,
  `NAME:COMMIT_MS:P:LEAST_SPEECH_MS`. The current one (`400:0.9:800`) cuts
  1 of 97 one-sentence lines, and 1 of the 9 lamp-4ace cut.
- `floor-28-speech-19.jsonl`, `floor-28-speech-23.jsonl`: the same with
  each line scaled to a median speech level of −19 or −23 dBFS (lamp-4ace
  measured −15 to −23 at 1 m) and the −28 dB floor with its 400 ms hold.
  Still 1 of the 9.
- `clean-window-verdicts.jsonl`: every verdict the model gave. At the
  commas lamp-4ace cut, the clean audio gets 0.92 to 0.99, as lamp-4ace's
  0.91 to 0.99: the verdict is the same, the pause is not.

So the way from the loudspeaker to the voice microphone lengthens a comma's
quiet by 50 to 150 ms, past the 400 ms commit. A replay of lamp-4ace's own
capture of these scenes would measure that way; it was not available here.

## What others do

Read in their official documents and code on 15 October 2026: Pipecat
Smart Turn v3 asks after 0.2 s of quiet and ends on a finished verdict
(p > 0.5, fixed), else after 3 s; LiveKit's audio detector ends at 0.3 s
when p ≥ 0.36 (English), else 2.5 s, and its bench puts Smart Turn v3.2 at
35% cut off at 300 ms and 15% at 600 ms; AssemblyAI ends at 400 ms when
confident, at most 1,280 ms (0.7 / 800 / 3,600 ms conservative); Deepgram
Flux times out at 5 s; OpenAI's semantic VAD at 2, 4 or 8 s. People's
pauses inside their own talk have a median of 580 to 640 ms (Heldner and
Edlund 2010, Table 7). No early end under about 600 ms leaves them whole.

## What was built

`LAMP_CUT_KEEPS_NAME=on`, off unless set ([attention.md](../../attention.md),
"A cut keeps its name"): a request that continues one the lamp ended early
keeps what the busy-room rule knew of the first part. The tests and the
gate are in the pull request.

## Not shown

No replay of what lamp-4ace's microphone heard; no person's voice; no
lamp run of the switch. Whether a commit of 500 to 550 ms is worth its
latency is the first builder's choice, and is not built.
