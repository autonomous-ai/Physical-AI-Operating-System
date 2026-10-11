cargo build --locked --release -p lamp-live -p lamp-voice-eval 2>&1 | tail -1
export BIN=/target/release
python3 crates/voice-eval/sim-harness/jitter.py 75 > /tmp/jitter.txt &
J=$!
for i in 1 2 3 4 5 6; do sh crates/voice-eval/sim-harness/trial.sh | tail -1 | cut -c1-200; done
wait $J; cat /tmp/jitter.txt
