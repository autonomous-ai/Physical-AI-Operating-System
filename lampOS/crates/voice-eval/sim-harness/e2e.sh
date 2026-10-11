# The real voice-eval physical runner (relay, cue validation, player, scoring)
# against the unmodified lamp-live directed-fixture on simulated hardware.
# Usage: run-sim.sh REF 'OUT=/sim/e2e/NAME SCENARIOS="a b" REPS=1 sh crates/voice-eval/sim-harness/e2e.sh'
# Needs /sim/reply.wav and a stimulus cache at /sim/acoustic with manifest
# /sim/render-1.json (render-stimuli on macOS writes files only).
set +e
cargo build --locked --release -p lamp-live -p lamp-voice-eval 2>&1 | tail -1
BIN=/target/release
PULL=/sys/devices/platform/gpio-sim.0/gpiochip1/sim_gpio9/pull
echo pull-up > $PULL
mkdir -m 700 -p /tmp/roomctl /tmp/lampwork; rm -f /tmp/roomctl/room.sock; rm -rf /tmp/room1
mkdir -p $(dirname $OUT)
$BIN/lamp-voice-eval sim-room --control /tmp/roomctl/room.sock --out /tmp/room1 --privacy-pull $PULL --max-seconds 900 --echo-gain ${ECHO_GAIN:-0} --echo-delay-ms ${ECHO_DELAY:-0} 2>/tmp/room.err &
ROOM=$!
for i in $(seq 1 50); do grep -q room_start /tmp/room1/room-events.jsonl 2>/dev/null && break; sleep 0.1; done
grep -q room_start /tmp/room1/room-events.jsonl || { echo "room did not start"; cat /tmp/room.err; exit 1; }
SC=""; for s in $SCENARIOS; do SC="$SC --scenario $s"; done
$BIN/lamp-voice-eval physical-run --config crates/voice-eval/sim-harness/physical-sim.json --cache /sim/acoustic --manifest /sim/render-1.json --out $OUT $SC --repetitions ${REPS:-1} --execute 2>&1 | tail -40
echo "physical-run exit $?"
$BIN/lamp-voice-eval sim-room-send --control /tmp/roomctl/room.sock '{"cmd":"stop"}'
wait $ROOM; echo "room exit $?"
cp -r /tmp/room1 $OUT/room 2>/dev/null
grep -v '"kind":"command"' /tmp/room1/room-events.jsonl | cut -c1-220 | tail -20
