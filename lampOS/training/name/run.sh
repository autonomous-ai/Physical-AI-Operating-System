#!/bin/sh
# Runs one of the name's training scripts at the lowest priority, in this
# directory, with the environment of training/stop (its pyproject.toml and
# uv.lock, made by `uv sync` under lampOS/.cache/stop). At most six threads.
#
#     sh run.sh speak.py calls --split train --count 9000
here=$(cd "$(dirname "$0")" && pwd)
cd "$here" || exit 1
export OMP_NUM_THREADS="${OMP_NUM_THREADS:-6}"
export TORCH_HOME="$here/../../.cache/stop/torch"
exec nice -n 19 "$here/../../.cache/stop/venv/bin/python" "$@"
