#!/usr/bin/env bash
# #906's third test and its 30 ms-pause twin on the stepped clock, 40 batches
# of 10 (400) each, beside 24 busy loops.
cd "$(dirname "$0")"
pids=(); for i in $(seq 24); do (while :; do :; done) & pids+=($!); done
trap 'kill "${pids[@]}" 2>/dev/null' EXIT
uptime > load-start.txt
for T in pending_inspection_is_revoked_before_worker_data_and_release_is_separate \
         pending_inspection_revoke_survives_a_host_pause_before_the_gated_worker_is_released; do
  mkdir -p after/$T
  for b in $(seq 1 40); do
    runs=()
    for j in $(seq 1 10); do ./after.bin --exact coordinator::motor::tests::$T >after/$T/r$b-$j.txt 2>&1 & runs+=($!); done
    wait "${runs[@]}"
  done
done
uptime > load-end.txt
