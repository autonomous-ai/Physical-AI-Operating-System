#!/bin/sh
# Step 4 of make.sh: the training and validation inputs, three processes of
# two threads each. Optionally only some: sh make_inputs.sh words talk
here=$(cd "$(dirname "$0")" && pwd)
cd "$here" || exit 1
for what in ${*:-words talk check check-talk}; do
  for part in 0 1 2; do
    OMP_NUM_THREADS=2 sh run.sh make_data.py "$what" --part "$part/3" &
  done
  wait
done
