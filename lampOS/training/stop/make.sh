#!/bin/sh
# Makes lampOS/models/stop/stop.onnx from nothing, on a Mac or Linux machine
# with uv, about 12 GB of free disk and a network. Every step runs at the
# lowest priority with at most six threads. See docs/stop-word.md.
#
#     sh make.sh
#
# Downloads go to lampOS/.cache/stop/downloads (git-ignored):
#   openWakeWord v0.5.1  melspectrogram.onnx, embedding_model.onnx (SHA-256 checked in features.py)
#   Speech Commands v0.02  http://download.tensorflow.org/data/speech_commands_v0.02.tar.gz
#   LibriSpeech  test-clean, test-other, dev-clean, dev-other  https://www.openslr.org/resources/12/
#                train-clean-100 as parquet from https://huggingface.co/datasets/openslr/librispeech_asr (not gated)
#   MSWC English  data/opus/en/{train/35,66,78; dev/4,8,10; test/4,8,10}.tar.gz and data/splits/en/splits.tar.gz
#                 from https://huggingface.co/datasets/MLCommons/ml_spoken_words (not gated)
#   Piper voice  en_US-libritts-high (.onnx, .onnx.json) from https://huggingface.co/rhasspy/piper-voices
#   AMI  34 meetings' Mix-Headset.wav and ami_public_manual_1.6.2.zip from https://groups.inf.ed.ac.uk/ami/
set -e
here=$(cd "$(dirname "$0")" && pwd)
cache="$here/../../.cache/stop"
cd "$here"
export UV_PROJECT_ENVIRONMENT="$cache/venv" UV_CACHE_DIR="$cache/uv-cache"
uv sync --python 3.12

d="$cache/downloads"
mkdir -p "$d/mswc" "$d/ami" "$cache/oww" "$cache/piper"
get() { [ -s "$1" ] || curl -sSL --fail --retry 5 -C - -o "$1" "$2"; }
for f in melspectrogram.onnx embedding_model.onnx; do
  get "$cache/oww/$f" "https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/$f"
done
for f in en_US-libritts-high.onnx en_US-libritts-high.onnx.json MODEL_CARD; do
  get "$cache/piper/$f" "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/libritts/high/$f"
done
get "$d/speech_commands_v0.02.tar.gz" http://download.tensorflow.org/data/speech_commands_v0.02.tar.gz
for s in test-clean test-other dev-clean dev-other; do
  get "$d/$s.tar.gz" "https://www.openslr.org/resources/12/$s.tar.gz"
done
mkdir -p "$d/train-clean-100-parquet"
for i in 00 01 02 03 04 05 06 07 08 09 10 11 12 13; do
  get "$d/train-clean-100-parquet/00$i.parquet" \
    "https://huggingface.co/datasets/openslr/librispeech_asr/resolve/main/all/train.clean.100/00$i.parquet"
done
m=https://huggingface.co/datasets/MLCommons/ml_spoken_words/resolve/main/data
get "$d/mswc/splits.tar.gz" "$m/splits/en/splits.tar.gz"
for pair in train:35 train:66 train:78 dev:4 dev:8 dev:10 test:4 test:8 test:10; do
  split=${pair%%:*}; shard=${pair##*:}
  get "$d/mswc/${split}_$shard.tar.gz" "$m/opus/en/$split/audio/$shard.tar.gz"
done
# Ten meetings taken first, then every meeting whose transcript holds "hold
# on" and seven with many "wait" and "stop": 34 meetings, 24 h. Testing only.
for meeting in ES2004a ES2004b ES2004c ES2004d IS1009a IS1009b IS1009c TS3003a TS3003b TS3003c \
  EN2001d EN2005a EN2006b EN2009d ES2005b ES2005c ES2005d ES2007c ES2008d ES2009c ES2012c ES2014d \
  ES2016a ES2016b IS1007c TS3004c TS3012c ES2010b IN1016 IN1014 IS1003d TS3008b TS3010d EN2001a; do
  get "$d/ami/$meeting.Mix-Headset.wav" "https://groups.inf.ed.ac.uk/ami/AMICorpusMirror/amicorpus/$meeting/audio/$meeting.Mix-Headset.wav"
done
get "$d/ami/ami_public_manual_1.6.2.zip" https://groups.inf.ed.ac.uk/ami/AMICorpusAnnotations/ami_public_manual_1.6.2.zip
# The files are the ones this model was made from (SHA-256 recorded on 2026-10-14).
(cd "$d" && shasum -a 256 -c "$here/downloads.sha256")

# 1. Unpack.
mkdir -p "$cache/data/speech_commands" "$cache/data/mswc_splits"
[ -f "$cache/data/speech_commands/testing_list.txt" ] || nice -n 19 tar -xzf "$d/speech_commands_v0.02.tar.gz" -C "$cache/data/speech_commands"
[ -f "$cache/data/mswc_splits/test.csv" ] || tar -xzf "$d/mswc/splits.tar.gz" -C "$cache/data/mswc_splits"
sh run.sh unpack.py mswc test-clean test-other dev-clean dev-other
# 2. Generated speech (speak.py).
sh make_speech.sh
# 3. Real speech, aligned, cut and split by speaker.
sh run.sh clips.py sc mswc words libri more piper
# training/name's make.sh stops here, with the hour of validation talk it shares.
if [ -n "$STOP_ONLY_CLIPS" ]; then
  sh make_inputs.sh check-talk
  exit 0
fi
# 4. Training inputs, three processes of two threads.
sh make_inputs.sh
# 5. Train and write the model.
# Three threads, as it was made; one seed (0). Another thread count may change the last bits.
OMP_NUM_THREADS=3 sh run.sh train.py --out "$here/../../models/stop/stop.onnx"
# 6. Measure it on held-out speakers.
# Two halves side by side, three threads each.
OMP_NUM_THREADS=3 sh run.sh evaluate.py "$here/../../models/stop/stop.onnx" --only catches &
OMP_NUM_THREADS=3 sh run.sh evaluate.py "$here/../../models/stop/stop.onnx" --only talk,ami,near &
wait
sh run.sh tables.py "$cache/runs/stop-catches.eval.json" "$cache/runs/stop-talk-ami-near.eval.json" --point 0.9/1 --stricter 0.98/1
