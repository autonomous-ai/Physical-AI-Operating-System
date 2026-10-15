# Short stop words under the lamp's voice: #828 step 1, 2026-10-15

Fourth's evidence for step 1 of [#828](https://github.com/autonomous-ai/lampOS/issues/828):
whether a different setting of the echo canceller's suppressor, or the
stop spotter on what the canceller leaves, keeps a short "stop" said over
the lamp. Run on Fourth's Intel Mac (macOS 26.6.2, i7-10700K, Rust 1.96.0).
Nothing was run on a lamp.

## The recordings

The first builder's copies of lamp-4ace's runs of the morning of 14 October
(`~/lamp-voice-data/sessions/duplex-20261014/`, local only, out of Git):
the voice's audio diagnostics (mic2 before and after the canceller, what
the speaker accepted, each block's timing), and `plays.tsv`, when each
synthetic line was played from the Mac's loudspeaker, on the lamp's clock.
The words are the task's own lines; nothing of a person's voice is here.

- cut-a, cut-b, cut-d, both-a, both-noagc: a story asked for, then a short
  line played over it; 11 stop words and 3 other lines (`yields.tsv`).
- cut-q: the same lines played to a quiet lamp.
- cut-c: its recording was never finished (`result.json` empty), and
  `replay-aec` refuses it.

## What was run

The scripts named here are in Fourth's scratch folder, outside the
repository (lampOS is Rust only): this says what each read and counted, so
that it can be written again. The Rust parts are in the repository:
`lamp-audio replay-aec --suppressor`, `crates/voice/examples/hearing_replay.rs`
and `lamp-name check`.

1. `replay-all.sh`: `lamp-audio replay-aec RUN OUT --suppressor S` for
   `standard`, `near-end` and `transparent`, into `lampOS/.cache/`, then
   `crates/voice/examples/hearing_replay.rs` on each, which scores every
   replayed block with the voice's `SpeechProbability` and judges it with
   `Hearing::EVALUATED` against the replayed canceller's own echo evidence
   (the `capture_pair` records of `operations.jsonl`).
2. `hearing.py` (`yields.tsv`): per played line, the longest run of `Over`
   blocks from 0.1 s before it to 0.4 s after it, and plain blocks;
   7 `Over` in a row is a yield (`Confirmation::EVALUATED`). False yields:
   runs of 7 `Over` while the lamp speaks 1.5 s or more from any played
   line.
3. `why.py` (`why.txt`): of the speech-like blocks (0.5 or more) in each
   word's window, how many were `Over`, `Own` or `Unknown`, and the median
   share of the raw microphone the speaker explains.
4. `separate.py` (`separate.txt`): would any `explained_percent` bar keep
   the words' blocks without the lamp's own.
5. `levels.py` (`levels.txt`): each setting's output level in each word's
   window. `mics.py` (`mics.txt`): each word's level against the second
   before it, on mic2 before the canceller and on mic1 (both-a,
   both-noagc).
6. `slices.py` and `stop_table.py` (`stop-spotter*.txt`): each replayed
   output cut into one file per played line (2 s before it to 0.8 s after)
   and 3 s pieces of the lamp speaking to nobody; `lamp-name check` with
   `stop.onnx` as its model and openWakeWord v0.5.1's front end (downloaded;
   SHA-256 as pinned in `models/name/SOURCE.md`), hearings at 0.9.

`replays.tsv`: each replay's level against what lamp-4ace recorded. The
standard replay of cut-a differs from the recording in 26% of samples at a
level within 0.7% (1,604.8 against 1,593.9 RMS): the host's arithmetic is
not the lamp's, so the settings are compared with each other.

## What it showed

| | standard | near-end | transparent |
|---|---|---|---|
| Speech-like blocks in the 11 stop words' windows | 178 | 287 | 217 |
| …heard as a person over the lamp (`Over`) | 59 | 82 | 70 |
| …taken for the lamp's own voice (`Own`) | 71 | 156 | 99 |
| Stop words that would make it yield | 3 of 11 | 4 of 11 | 3 of 11 |
| False yields in 2.5 min of the lamp speaking to nobody | 2 | 3 | 3 |
| `stop.onnx` at 0.9, stop words over the lamp | 0 of 11 | 1 of 11 | 0 of 11 |
| `stop.onnx` at 0.9, stop words with the lamp quiet | 4 of 9 | 4 of 9 | 4 of 9 |
| `stop.onnx` false hearings, 29 × 3 s of the lamp to nobody | 1 | 1 | 0 |

Half of a stop word's speech-like blocks lie under the lamp's own voice
(the speaker explains 97% or more of the microphone), where nothing here
tells them from its leak; no suppressor setting changes that.

## Not shown

- The lamp: every figure is a host replay of recorded audio.
- A person: every word was played from a loudspeaker.
- Many words: 11 stop words over the lamp, in five runs of one morning.
- The duplex loop: yields are counted open-loop on the recording; after a
  real yield the lamp's voice stops, which these recordings show only where
  the lamp did yield.
