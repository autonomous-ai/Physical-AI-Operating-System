#!/bin/sh
# Runs one of the training scripts at the lowest priority, in this
# directory, with the environment `uv sync` made under lampOS/.cache/stop.
# At most six threads: another session on the machine may be timing things.
#
#     sh run.sh speak.py asking --split train --count 9000
here=$(cd "$(dirname "$0")" && pwd)
cd "$here" || exit 1
export OMP_NUM_THREADS="${OMP_NUM_THREADS:-6}"
export TORCH_HOME="$here/../../.cache/stop/torch"
exec nice -n 19 "$here/../../.cache/stop/venv/bin/python" "$@"
