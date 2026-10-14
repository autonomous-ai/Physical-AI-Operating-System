# stop.onnx

The lamp's listener for "stop", "wait" and "hold on" ([docs/stop-word.md](../../docs/stop-word.md)).
Ours: trained for lampOS on 2026-10-14.

| | |
|---|---|
| File | `stop.onnx`, 124,135 bytes |
| SHA-256 | `5fe07569e3d7c5adcdf496770deb306701b057ab8adce1d03ce901db3ce72d41` |
| Input | `input`, float32 `[1, 16, 96]`: the newest sixteen openWakeWord embeddings, exactly as `crates/name/src/hear.rs` makes them for `name.onnx` |
| Output | `output`, float32 `[1, 1]`: a score from 0 to 1 that one of the three was just said |
| Graph | ONNX opset 13, IR 8: Transpose, Conv (96 to 64, three embeddings wide), Relu, Conv (64 to 64), Relu, ReduceMax over time, Gemm (64 to 1), Sigmoid. 30,913 weights |
| Runs under | tract-onnx 0.23.8 (default features off), through `hear.rs` itself: the same scores as onnxruntime 1.31 to within 6e-6, on macOS and in a Linux ARM64 container |
| Front end it needs | openWakeWord v0.5.1 `melspectrogram.onnx` (SHA-256 `ba2b0e0f8b7b875369a2c89cb13360ff53bac436f2895cced9f479fa65eb176f`) and `embedding_model.onnx` (`70d164290c1d095d1d4ee149bc5e00543250a7316b59f31d056cff7bd3075c1f`), as the name already uses. Not in this directory |

## What it was trained on, and under which licences

| Source | Used for | Licence |
|---|---|---|
| Speech Commands v0.02 (Warden 2018) | "stop" from 1,230 speakers; 34 other words; its background noise | CC BY 4.0 |
| Multilingual Spoken Words Corpus, English (MLCommons, from Common Voice), via Hugging Face `MLCommons/ml_spoken_words`, not gated | "stop" (1,488 speakers), "wait" (1,213), near misses ("stopped", "stopping", "stops", "waited", "waiting", "holding"), about 8,900 other words | CC BY 4.0 |
| LibriSpeech dev-clean, dev-other (openslr 12) | The other voice; read "stop", "wait", "hold on" | CC BY 4.0 |
| LibriSpeech train-clean-100, via Hugging Face `openslr/librispeech_asr`, not gated | 25.6 h of talk to train against (no targets) | CC BY 4.0 |
| Speech generated with Piper's `en_US-libritts-high` voice (791 of its 904 speakers), from rhasspy/piper-voices | Phrases with the targets, near misses, short replies, long talk | The voice was trained from scratch on LibriTTS train-clean-360, CC BY 4.0; the voices repository is MIT. Generated with `piper-tts` 1.8.0 |
| wav2vec 2.0 base 960 h (torchaudio `WAV2VEC2_ASR_BASE_960H`) | Only to find where words are in recordings; nothing of it is in the model | MIT |

Not used: macOS voices; Common Voice itself (it needs an account now);
openWakeWord's own negative features (CC BY-NC-SA 4.0); Piper's
`libritts_r-medium` voice (fine-tuned from a voice whose data is licensed
for research only). The test sets (Speech Commands' testing list, the
held-out MSWC and Piper speakers, LibriSpeech test-clean and test-other,
AMI meetings under CC BY 4.0) were not trained on.

Each downloaded file's SHA-256 is in
[`training/stop/downloads.sha256`](../../training/stop/downloads.sha256), and
`make.sh` checks them.

## The command that made it

From `lampOS/training/stop/`, on an Apple M2 Max laptop (macOS 26.6),
Python 3.12 from `uv sync` (`uv.lock`):

    sh make.sh

which, after the downloads, does:

    sh run.sh unpack.py mswc test-clean test-other dev-clean dev-other
    sh make_speech.sh
    sh run.sh clips.py sc mswc words libri more piper
    sh make_inputs.sh
    OMP_NUM_THREADS=3 sh run.sh train.py --out ../../models/stop/stop.onnx

`train.py` with no options is the configuration of this file: `--shape
time --hidden 64 --steps 12000 --heard 3 --against 3 --hard 0 --seed 0`.
It ran once. Made again, it will not be the same file to the bit: Piper
draws its own noise when it speaks, and another number of threads changes
the order of sums. Its measurements are in the document.
