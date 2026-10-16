#!/usr/bin/env bash
# The fix's final head: each of the three privacy-revoke identities 400 times,
# 10 at a time, beside 24 busy loops.
cd "$(dirname "$0")"
pids=(); for i in $(seq 24); do (while :; do :; done) & pids+=($!); done
trap 'kill "${pids[@]}" 2>/dev/null' EXIT
uptime > final-load-start.txt
for T in pending_segment_privacy_revoke_wins_before_any_position_write \
         privacy_revoke_survives_a_host_pause_before_the_gated_worker_is_released \
         privacy_revoke_survives_a_finished_worker_that_takes_300_ms_to_exit; do
  mkdir -p final/$T
  for b in $(seq 1 40); do
    runs=()
    for j in $(seq 1 10); do ./final.bin --exact coordinator::motor::tests::$T >final/$T/r$b-$j.txt 2>&1 & runs+=($!); done
    wait "${runs[@]}"
  done
done
uptime > final-load-end.txt
