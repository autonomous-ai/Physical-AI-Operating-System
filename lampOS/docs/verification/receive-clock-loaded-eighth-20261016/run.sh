#!/usr/bin/env bash
# Interleaved 40 batches per side, 10 concurrent runs each (400 per side),
# beside 24 busy loops; same conditions as #914's reproduction.
cd "$(dirname "$0")"  # beside before.bin and after.bin
T=runtime::supervisor::tests::inert_child_stop_timing_samples_are_host_ipc_only
pids=(); for i in $(seq 24); do (while :; do :; done) & pids+=($!); done
trap 'kill "${pids[@]}" 2>/dev/null' EXIT
uptime > load-start.txt
for b in $(seq 1 40); do
  for side in before after; do
    mkdir -p $side
    runs=()
    for j in $(seq 1 10); do ./$side.bin --exact $T >$side/r$b-$j.txt 2>&1 & runs+=($!); done
    wait "${runs[@]}"
  done
done
uptime > load-end.txt
