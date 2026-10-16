#!/usr/bin/env bash
# The 30 ms host-pause regression with the diagnostic lines, 400 batches of 10 (4,000), on the fix,
# beside 24 busy loops.
cd "$(dirname "$0")"
T=coordinator::motor::tests::privacy_revoke_survives_a_host_pause_before_the_gated_worker_is_released
pids=(); for i in $(seq 24); do (while :; do :; done) & pids+=($!); done
trap 'kill "${pids[@]}" 2>/dev/null' EXIT
uptime > diag-load-start.txt
for b in $(seq 1 400); do
  for side in diag; do
    mkdir -p $side
    runs=()
    for j in $(seq 1 10); do ./$side.bin --exact $T >$side/r$b-$j.txt 2>&1 & runs+=($!); done
    wait "${runs[@]}"
  done
done
uptime > diag-load-end.txt
