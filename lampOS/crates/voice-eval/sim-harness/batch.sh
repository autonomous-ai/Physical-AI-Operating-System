# Usage: run-sim.sh REF 'N=20 OUT=/sim/repro/REF sh crates/voice-eval/sim-harness/batch.sh'
# Interleaves echo gain 0.3 and 0 trials; keeps evidence for every non-ok trial.
set +e
mkdir -p $OUT
cargo build --locked --release -p lamp-live -p lamp-voice-eval 2>&1 | tail -1
export BIN=/target/release
{ uname -r; nproc; cat /proc/loadavg; } > $OUT/host.txt
: > $OUT/summary.txt
for i in $(seq 1 ${N:-20}); do
  for g in 0.3 0; do
    tag=run-$i-gain$g
    p0=$(head -1 /proc/pressure/cpu 2>/dev/null | cut -d' ' -f2); l0=$(cut -d' ' -f1 /proc/loadavg)
    line=$(ECHO_GAIN=$g sh crates/voice-eval/sim-harness/trial.sh 2>&1 | tail -1)
    case "$line" in ok*) ;; *) mkdir -p $OUT/$tag; cp /tmp/out/events.jsonl /tmp/live.err /tmp/room.err /tmp/send.err $OUT/$tag/ 2>/dev/null; cp /tmp/room1/* $OUT/$tag/ 2>/dev/null;; esac
    echo "$tag load1=$l0 cpu_some_$p0 $line" | tee -a $OUT/summary.txt
  done
done
for g in 0.3 0; do
  echo "gain $g: ok=$(grep -c "gain$g [^ ]* [^ ]* ok" $OUT/summary.txt) failed=$(grep -c "gain$g [^ ]* [^ ]* FAILED" $OUT/summary.txt) invalid=$(grep -c "gain$g [^ ]* [^ ]* INVALID" $OUT/summary.txt) no_turn=$(grep -c "gain$g [^ ]* [^ ]* NO_TURN" $OUT/summary.txt)" | tee -a $OUT/summary.txt
done
