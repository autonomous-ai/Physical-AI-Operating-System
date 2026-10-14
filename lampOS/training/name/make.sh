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
# Every step runs at the lowest priority with at most six threads.
set -e
here=$(cd "$(dirname "$0")" && pwd)
cache="$here/../../.cache/name"
cd "$here"

# Shared with training/stop: its downloads, unpacking, speech and clip sets.
STOP_ONLY_CLIPS=1 sh ../stop/make.sh

d="$cache/downloads"
mkdir -p "$d/mswc" "$d/fleurs" "$d/edacc" "$cache/piper"
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
(cd "$d" && shasum -a 256 -c "$here/downloads.sha256")

# 2. Generated calls, near misses and replies.
sh make_speech.sh
# 3. The name's own clip sets.
sh run.sh clips.py mswc libri fleurs edacc piper
# 4. Inputs.
sh make_inputs.sh
# 5. Train, three threads each as they were made: the kept model, and the same
#    without a bare "Lamp" to compare with.
options="--inputs $cache/inputs --shape time --hidden 64 --steps 12000 --heard -99 --against 3 --hard 0"
OMP_NUM_THREADS=3 sh run.sh ../stop/train.py $options --out "$here/../../models/name/name.onnx" &
OMP_NUM_THREADS=3 sh run.sh ../stop/train.py $options --leave-out-way 3 --out "$cache/runs/without-bare.onnx" &
wait
# 6. Measure both.
for model in "$here/../../models/name/name.onnx" "$cache/runs/without-bare.onnx"; do
  OMP_NUM_THREADS=3 sh run.sh evaluate.py "$model" --only calls,near &
  OMP_NUM_THREADS=3 sh run.sh evaluate.py "$model" --only talk,ami &
  wait
done
sh run.sh tables.py "$cache/runs/name-calls-near.eval.json" "$cache/runs/name-talk-ami.eval.json"
sh run.sh tables.py "$cache/runs/without-bare-calls-near.eval.json" "$cache/runs/without-bare-talk-ami.eval.json"
