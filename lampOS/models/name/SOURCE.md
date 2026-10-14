# name.onnx

A name model for `lamp-name` that hears "Hey Lamp", "Lamp ơi" and a bare
"Lamp" at the start of a sentence ([docs/bare-name.md](../../docs/bare-name.md)).
Ours: trained for lampOS on 2026-10-14. It has the shape of the name model
`lamp-name` already loads, and can be installed in its place.

| | |
|---|---|
| File | `name.onnx`, 124,135 bytes |
| SHA-256 | `f376e964711a7460d248f8b48b2c0f07b03f73c5adf270a67536e68b04202030` |
| Input | `input`, float32 `[1, 16, 96]`: the newest sixteen openWakeWord embeddings, exactly as `crates/name/src/hear.rs` makes them |
| Output | `output`, float32 `[1, 1]`: a score from 0 to 1 that the lamp was just called |
| Graph | ONNX opset 13, IR 8: Transpose, Conv (96 to 64, three embeddings wide), Relu, Conv (64 to 64), Relu, ReduceMax over time, Gemm (64 to 1), Sigmoid. 30,913 weights |
| Runs under | tract-onnx 0.23.8 (default features off), through `hear.rs` itself: the same scores as onnxruntime 1.31 to within 6e-6 over 2,127 steps, on macOS and in a Linux ARM64 container |
| Front end it needs | openWakeWord v0.5.1 `melspectrogram.onnx` (SHA-256 `ba2b0e0f8b7b875369a2c89cb13360ff53bac436f2895cced9f479fa65eb176f`) and `embedding_model.onnx` (`70d164290c1d095d1d4ee149bc5e00543250a7316b59f31d056cff7bd3075c1f`), as now |

## What it was trained on, and under which licences

| Source | Used for | Licence |
|---|---|---|
| Speech generated with Piper's `en_US-libritts-high` (678 of its 904 voices; 113 kept for testing and 113 for validation) | "Hey Lamp", "Lamp ơi" and "Lamp" calls; near misses ("Lan", "Liam", "lamb", "land", "plan", "lab", "Sam", "ma'am", "Lâm", other names, rhymes, "lamp" inside a sentence); lamp-like replies by the female voices. A third read as espeak's Vietnamese voice reads them | Voice trained from scratch on LibriTTS train-clean-360, CC BY 4.0; voices repository MIT; made with `piper-tts` 1.8.0 |
| MSWC English (MLCommons, from Common Voice), via Hugging Face, not gated | "lamp" (202 clips to train on), and "hey", "lamb", "land", "plan", "plant", "lab", "lap", "Sam", "Liam", "Lan", "lam", "Lamar", "madam", "camp", "damp", "ramp", "stamp", "clamp", "lamps"; and about 8,900 other words | CC BY 4.0 |
| Speech Commands v0.02 | Its 34 words other than "stop" | CC BY 4.0 |
| LibriSpeech dev-clean, dev-other, train-clean-100 | Talk; "lamp" said inside sentences | CC BY 4.0 |
| Google FLEURS Vietnamese (vi_vn), its train split | Vietnamese talk | CC BY 4.0 |
| Generated talk read with a Vietnamese accent (LibriTTS voice, espeak Vietnamese phonemes) | Talk in the accent of the accented calls | As the LibriTTS voice above |
| wav2vec 2.0 base 960 h (torchaudio) | Only to find where the MSWC words are | MIT |

Testing only, not trained on: Piper `en_GB-cori-medium` and
`en_US-ljspeech-medium` (trained from scratch on public-domain recordings),
the held-out voices and speakers of the sets above, FLEURS Vietnamese test,
LibriSpeech test-clean and test-other, AMI (CC BY 4.0), and EdAcc's
Vietnamese speakers of English (CC BY-SA 4.0).

Not used: L2-ARCTIC and the Speech Accent Archive (non-commercial
licences), Common Voice itself (an account is needed), and every
Vietnamese Piper voice (fine-tuned from the research-only Lessac voice,
or trained on non-commercial or unknown data). Each downloaded file's
SHA-256 is in [`training/name/downloads.sha256`](../../training/name/downloads.sha256)
and [`training/stop/downloads.sha256`](../../training/stop/downloads.sha256).

## The command that made it

From `lampOS/training/name/`, on an Apple M2 Max laptop (macOS 26.6),
with training/stop's environment (`uv.lock` there):

    sh make.sh

which, after its downloads and training/stop's shared sets, does:

    sh make_speech.sh
    sh run.sh clips.py mswc libri fleurs edacc piper
    sh make_inputs.sh
    OMP_NUM_THREADS=3 sh run.sh ../stop/train.py --inputs ../../.cache/name/inputs \
      --shape time --hidden 64 --steps 12000 --heard -99 --against 3 --hard 0 \
      --out ../../models/name/name.onnx

It ran once, with seed 0. Made again it will not be the same file to the
bit: Piper draws its own noise, and the number of threads changes the
order of sums.
