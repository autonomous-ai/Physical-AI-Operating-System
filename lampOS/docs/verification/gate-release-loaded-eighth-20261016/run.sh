#!/usr/bin/env bash
# #906: both original identities, before (host clock, #996 head af97a4a4c) and
# after (stepped), interleaved batch by batch; then the two 30 ms-pause twins
# on the fix. 40 batches of 10 (400) each, beside 24 busy loops.
cd "$(dirname "$0")"
pids=(); for i in $(seq 24); do (while :; do :; done) & pids+=($!); done
trap 'kill "${pids[@]}" 2>/dev/null' EXIT
batch() { # side test b
  mkdir -p $1/$2; local runs=()
  for j in $(seq 1 10); do ./$1.bin --exact coordinator::motor::tests::$2 >$1/$2/r$3-$j.txt 2>&1 & runs+=($!); done
  wait "${runs[@]}"
}
uptime > load-start.txt
for T in late_started_callback_cannot_restore_an_offer_after_owner_replacement \
         same_owner_input_end_cancels_queued_gesture_before_inert_bus_write; do
  for b in $(seq 1 40); do batch before $T $b; batch after $T $b; done
done
for T in late_started_callback_survives_a_host_pause_before_the_gated_worker_is_released \
         same_owner_input_end_survives_a_host_pause_before_the_gated_worker_is_released; do
  for b in $(seq 1 40); do batch after $T $b; done
done
uptime > load-end.txt
