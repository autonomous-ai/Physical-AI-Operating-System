#!/bin/sh
# Step 4 of make.sh: the training and validation inputs, three processes of
# two threads each. The talk to validate against is training/stop's
# check-talk (an hour of LibriSpeech dev readers kept back), linked here.
here=$(cd "$(dirname "$0")" && pwd)
cd "$here" || exit 1
inputs="$here/../../.cache/name/inputs"
mkdir -p "$inputs"
for part in 0 1 2; do
  ln -sf "$here/../../.cache/stop/inputs/check-talk-$part.npz" "$inputs/check-talk-$part.npz"
done
for what in ${*:-words talk check}; do
  for part in 0 1 2; do
    OMP_NUM_THREADS=2 sh run.sh make_data.py "$what" --part "$part/3" &
  done
  wait
done
