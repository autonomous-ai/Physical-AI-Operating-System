#!/usr/bin/env bash
# Interleaved 40 batches per side, 10 concurrent runs each (400 per side),
# beside 24 busy loops; as #988 did for #914.
cd "$(dirname "$0")"
T=coordinator::motor::tests::pending_segment_privacy_revoke_wins_before_any_position_write
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
