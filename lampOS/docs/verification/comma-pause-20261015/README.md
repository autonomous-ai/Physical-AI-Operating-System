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

## The commit on human speech (`heldout/`)

Pipecat's public `smart-turn-data-v3.2-test` (revision `dfc17924`, CC-BY-4.0),
shards 4 and 5, the fresh held-out ones `endpoint-detection.md` used:
downloaded to Fourth's scratch folder and checked against their published
SHA-256 (`4c600774…ff66`, `b4537a3b…9335`), the 1,167 human English and
Spanish clips decoded to 16 kHz mono WAV by macOS's `afconvert`. No audio is
kept here. Each clip is one person; every turn end before its last is a
split, the person going on. The finished clips' final wait is the
detector's own pause at its end. Same replay, lamp-4ace's 600 ms window.

| Commit | Splits (the person went on) | Clips with a split | Wait at a finished end, median (mean) |
|---|---|---|---|
| 400 ms, as now | 527 | 384 | 400 ms (468) |
| 450 ms | 474 | 350 | 450 ms (504), +50 |
| 500 ms | 424 | 320 | 500 ms (539), +100 |
| 550 ms | 369 | 285 | 550 ms (571), +150 |
| none, the 600 ms window only | 330 | 258 | 600 ms (606), +200 |

So the 400 ms commit adds 197 splits to what the window alone makes, and
550 ms takes 158 of them away (80%) for 150 ms more at every confident end.
Most of the rest are the window's own: at 600 ms it ends pauses that the
1,200 ms baseline above keeps (13 splits there), nearly all of them in the
long recordings (`liva_1`, 311 of the 330). Kept-start continues those.

`heldout/replay.jsonl` holds every turn and verdict by clip id;
`heldout/score.txt` the table.

## Holding on a rise (`prosody.txt`)

The first builder's idea: hold longer when the last 300 ms before the quiet
rose in pitch or level, as at a comma. Measured from the same audio, 10 ms
frames, a pause being 200 ms under −40 dBFS after speech, the level's and
the pitch's slope (autocorrelation, voiced frames) over the 300 ms before.

| Before | Lines or clips | Level, dB per 100 ms (median; share rising) | Pitch, semitones per 100 ms (median; share rising) |
|---|---|---|---|
| a pause inside a rendered line (`say`) | 107 pauses | −4.75; 5% | −3.57; 12% |
| a rendered line's end | 111 | −4.33; 14% | −0.66; 47% |
| a pause a person went on after (held-out) | 1,089 pauses | −2.08; 35% | +0.18; 55% |
| an unfinished clip's end | 569 | −0.72; 37% | +0.09; 53% |
| a finished clip's end | 598 | −0.25; 43% | −0.03; 49% |

It does not tell a pause from an end, in `say`'s voices or in people's, so
it was not built.

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

Two switches, both off unless set, as the first builder decided on #782:
`LAMP_CUT_KEEPS_NAME=on` ([attention.md](../../attention.md), "A cut keeps
its name"), a request that continues one the lamp ended early keeps what
the busy-room rule knew of the first part; and `--endpoint-commit-ms MS`
([endpoint-pilot.md](../../endpoint-pilot.md), "Cut at a comma"), how long a
pause must last before a confident verdict ends the turn. The tests and the
gate are in the pull request.

## The gate (`gate/`, `arm64/`)

On commit `0b3ebf2a4f97564345402f676d5ec08175e3dac1` (on `lamp-voice` at
`663a1e5fe`; its code is the pull request's second commit, `7c82eccff`),
Fourth's Mac, 10:29 to 10:54 UTC; the commands in `gate/commands.txt`, each
stage's exit code in `gate/stages.txt`, the tree clean before and after:

| Stage | Result |
|---|---|
| `rustfmt --edition 2024 --check` on the changed Rust files | passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | passed |
| `cargo test --locked -p lamp-voice -p lamp-voice-eval -p lamp-memory --no-fail-fast -- --test-threads=1` | 1,625 passed, **1 failed**, 13 ignored, 62 suites |
| `cargo build --workspace --locked` | passed |

The one failure is
`coordinator::tests::endpoint_session::a_worker_still_loading_leaves_the_baseline_window`:
"capture discontinuity", the session's capture blocks more than 50 ms apart
on a loaded Mac, the voice's timed process tests that #739 is about, in a
path this change does not touch (a worker that is still loading; no commit
chosen). Run alone on the branch it then failed once and passed 14 times
(`gate/loading-test-alone/`, the last ten); on `lamp-voice` itself, 5 of 5
passed. It passed in the ARM64 run below.

Before the rebase onto `663a1e5fe`, which merged #792's attention code
into the same lines, the same gate had passed with no failure on
`4b677c852` (1,604 passed, 13 ignored) and on the first commit alone,
`a6bbc80ac` (1,601, 13): `gate/before-rebase-*`. The rebase kept both
sides of three places both had added to, and made a request that
continues one ended early keep #753's `addressed` too.

ARM64: the pull request's run 38720597961 on `0b3ebf2a4`, GitHub's native
Linux ARM64 runners, four shards: 1,183, 1,062, 1,155 and 1,129 tests run,
all passed, the seven new tests among them; `arm64/run-38720597961.txt` and
the SHA-256 of each binary in `arm64/sha256.txt` (`lamp-voice`
`3fd76910362748519818ec8e67f59605745ec23cc6926d3958784e7e4868e50e`). A
GitHub runner is not a lamp.

## Not shown

No replay of what lamp-4ace's microphone heard (the first builder is
copying its post-AEC captures to this Mac, to stay local); no lamp run of
either switch. The held-out clips are people's voices replayed digitally,
not through a loudspeaker and the lamp's microphone. Whether 550 ms is worth
its 150 ms is for the first builder's run at 400 and 550 tonight.
