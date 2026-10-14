#!/bin/sh
# Makes lampOS/models/name/name.onnx from nothing. See docs/bare-name.md.
#
#     sh make.sh
#
# It first makes what it shares with training/stop (the environment, the
# front-end models, Piper's LibriTTS voice, LibriSpeech, MSWC's split lists,
# Speech Commands, MSWC other words, generated talk, AMI and an hour of
# validation talk: training/stop/make.sh run up to its clip sets), then its own:
#   MSWC English shards  data/opus/en/{train/12,14,18,33,39,40,41,42,54,56,60,65;
#                        dev and test /1,2,4,5,6,7,8}.tar.gz, from
#                        https://huggingface.co/datasets/MLCommons/ml_spoken_words (not gated)
#   Google FLEURS vi_vn  train.tsv, test.tsv, audio/train.tar.gz, audio/test.tar.gz, from
#                        https://huggingface.co/datasets/google/fleurs (not gated)
#   Piper voices         en_GB-cori-medium, en_US-ljspeech-medium (testing only), from
#                        https://huggingface.co/rhasspy/piper-voices
#   EdAcc test split     data/test-*.parquet (testing only), from
#                        https://huggingface.co/datasets/edinburghcstr/edacc (not gated)
#   Kokoro-82M v1.0      onnx/model.onnx, tokenizer.json, config.json, voices/*.bin (testing
#                        only), from https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX
#                        at revision 1939ad2a8e416c0acfeecc08a694d14ef25f2231
# On a Mac it also measures the model in the Mac's own `say` voices (played.py).
# Every step runs at the lowest priority with at most six threads.
set -e
here=$(cd "$(dirname "$0")" && pwd)
cache="$here/../../.cache/name"
cd "$here"

# Shared with training/stop: its downloads, unpacking, speech and clip sets.
STOP_ONLY_CLIPS=1 sh ../stop/make.sh

d="$cache/downloads"
mkdir -p "$d/mswc" "$d/fleurs" "$d/edacc" "$cache/piper" "$cache/kokoro/onnx" "$cache/kokoro/voices"
get() { [ -s "$1" ] || curl -sSL --fail --retry 5 -C - -o "$1" "$2"; }
m=https://huggingface.co/datasets/MLCommons/ml_spoken_words/resolve/main/data/opus/en
for pair in train:12 train:14 train:18 train:33 train:39 train:40 train:41 train:42 train:54 train:56 train:60 train:65 \
  dev:1 dev:2 dev:4 dev:5 dev:6 dev:7 dev:8 test:1 test:2 test:4 test:5 test:6 test:7 test:8; do
  split=${pair%%:*}; shard=${pair##*:}
  get "$d/mswc/${split}_$shard.tar.gz" "$m/$split/audio/$shard.tar.gz"
done
f=https://huggingface.co/datasets/google/fleurs/resolve/main/data/vi_vn
for x in train.tsv test.tsv audio/train.tar.gz audio/test.tar.gz; do
  get "$d/fleurs/$(echo "$x" | tr '/' '_')" "$f/$x"
done
v=https://huggingface.co/rhasspy/piper-voices/resolve/main/en
for x in en_GB/cori/medium/en_GB-cori-medium en_US/ljspeech/medium/en_US-ljspeech-medium; do
  get "$cache/piper/$(basename "$x").onnx" "$v/$x.onnx"
  get "$cache/piper/$(basename "$x").onnx.json" "$v/$x.onnx.json"
done
e=https://huggingface.co/datasets/edinburghcstr/edacc/resolve/main/data
for x in test-00000-of-00010-f0aceb1ca4406ff1 test-00001-of-00010-856b016d9d438ff3 test-00002-of-00010-2b021baedb4deb8a \
  test-00003-of-00010-4de275e704375a02 test-00004-of-00010-806407c9bc68112a test-00005-of-00010-9c97c4c4c8d01f82 \
  test-00006-of-00010-cc4648d0f66f65a4 test-00007-of-00010-ea5ed4464ecff3c9 test-00008-of-00010-d1aa19b51ad423da \
  test-00009-of-00010-648ce5002d124496; do
  get "$d/edacc/$x.parquet" "$e/$x.parquet"
done
k=https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/1939ad2a8e416c0acfeecc08a694d14ef25f2231
for x in $(cut -c67- "$here/kokoro.sha256"); do
  get "$cache/kokoro/$x" "$k/$x"
done
(cd "$d" && shasum -a 256 -c "$here/downloads.sha256")
(cd "$cache/kokoro" && shasum -a 256 -c "$here/kokoro.sha256")

# 2. Generated calls, near misses, replies and talk; Kokoro's test sets.
sh make_speech.sh
# 3. The name's own clip sets.
sh run.sh clips.py mswc libri fleurs edacc piper kokoro
# 4. Inputs: task 5's, made the same way (no switch of make_data.py set).
sh make_inputs.sh
# 5. Train three times, two at a time, three threads each: the "flat" shape,
#    near misses a tenth of the other words of each batch, seeds 0, 1 and 2.
#    The model is their mean (combine.py).
near=piper-near-train,mswc-near-train,libri-lamp-train,piper-replies-train
options="--inputs $cache/inputs --shape flat --hidden 64 --steps 6000 --heard -99 --against 3 --hard 0 --near-sets $near --near-share 0.1"
mkdir -p "$cache/runs"
OMP_NUM_THREADS=3 sh run.sh ../stop/train.py $options --seed 0 --out "$cache/runs/name-0.onnx" &
OMP_NUM_THREADS=3 sh run.sh ../stop/train.py $options --seed 1 --out "$cache/runs/name-1.onnx" &
wait
OMP_NUM_THREADS=3 sh run.sh ../stop/train.py $options --seed 2 --out "$cache/runs/name-2.onnx"
sh run.sh combine.py mean "$here/../../models/name/name.onnx" "$cache/runs/name-0.onnx" "$cache/runs/name-1.onnx" "$cache/runs/name-2.onnx"
# 6. Measure it, and print the tables.
model="$here/../../models/name/name.onnx"
OMP_NUM_THREADS=3 sh run.sh evaluate.py "$model" --only calls,near &
OMP_NUM_THREADS=3 sh run.sh evaluate.py "$model" --only talk,ami,edacc &
wait
sh run.sh tables.py "$cache/runs/name-calls-near.eval.json" "$cache/runs/name-talk-ami-edacc.eval.json"
if [ "$(uname)" = Darwin ]; then
  sh run.sh played.py make
  sh run.sh played.py score "$model"
  sh run.sh played.py verdict "$cache/runs/played-name.json"
fi
