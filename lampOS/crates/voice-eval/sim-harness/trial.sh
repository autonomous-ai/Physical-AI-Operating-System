# One simulated trial: room (snd-aloop + gpio-sim) + unmodified lamp-live
# directed-fixture, one cached stimulus. Digital loopback, not acoustic.
# Env: BIN, ECHO_GAIN, ECHO_DELAY. Needs /sim/ask.wav and /sim/reply.wav
# (make-fixtures.py). Prints one line: VERDICT detail.
set +e
PULL=/sys/devices/platform/gpio-sim.0/gpiochip1/sim_gpio9/pull
echo pull-up > $PULL
mkdir -m 700 -p /tmp/roomctl; rm -f /tmp/roomctl/room.sock; rm -rf /tmp/room1 /tmp/out
$BIN/lamp-voice-eval sim-room --control /tmp/roomctl/room.sock --out /tmp/room1 --privacy-pull $PULL --max-seconds 60 --echo-gain ${ECHO_GAIN:-0} --echo-delay-ms ${ECHO_DELAY:-0} 2>/tmp/room.err &
ROOM=$!
for i in $(seq 1 50); do grep -q room_start /tmp/room1/room-events.jsonl 2>/dev/null && break; sleep 0.1; done
if ! grep -q room_start /tmp/room1/room-events.jsonl 2>/dev/null; then
  kill $ROOM 2>/dev/null; wait $ROOM; echo "INVALID room did not start: $(tail -1 /tmp/room.err)"; exit 0
fi
( $BIN/lamp-live directed-fixture /sim/reply.wav d9afb80e886e898c7230abc443c60dc4d229b1598f74571e021bc54714132421 10 /tmp/out --diagnostics > /tmp/live.out 2>/tmp/live.err; echo $? > /tmp/live.exit ) &
LIVE=$!
for i in $(seq 1 100); do grep -q listening_ready /tmp/live.out 2>/dev/null && break; sleep 0.1; done
$BIN/lamp-voice-eval sim-room-send --control /tmp/roomctl/room.sock '{"cmd":"play","id":1,"wav":"/sim/ask.wav","reply":null}' 2>/tmp/send.err
SEND=$?
wait $LIVE
$BIN/lamp-voice-eval sim-room-send --control /tmp/roomctl/room.sock '{"cmd":"stop"}' 2>>/tmp/send.err
wait $ROOM; ROOMEXIT=$?
ERR=$(grep '"kind":"run_end"' /tmp/out/events.jsonl 2>/dev/null | grep -o '"error":"[^"]*"' | head -1)
PLAYED=$(grep -c '"event":"finished"' /tmp/room1/room-events.jsonl 2>/dev/null)
TURN=$(grep '"kind":"turn_finished"' /tmp/out/events.jsonl 2>/dev/null | grep -o '"outcome":"[^"]*"' | head -1)
XRUNS=$(grep -o '"xruns":[0-9]*' /tmp/room1/room-events.jsonl | tail -1)
if [ $SEND -ne 0 ] || [ $ROOMEXIT -ne 0 ]; then V=INVALID
elif grep -q '"status":"failed"' /tmp/out/events.jsonl 2>/dev/null || [ "$(cat /tmp/live.exit)" != 0 ]; then V=FAILED
elif [ "$PLAYED" -ge 1 ] && [ -n "$TURN" ]; then V=ok
else V=NO_TURN; fi
echo "$V live_exit=$(cat /tmp/live.exit) room_exit=$ROOMEXIT played=$PLAYED $TURN $XRUNS $ERR"
