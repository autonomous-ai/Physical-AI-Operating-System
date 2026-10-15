#!/usr/bin/env bash
# Regenerate the spoken step prompts (lamps have no TTS and no internet in hotspot mode).
# Uses macOS `say`; writes 22.05 kHz 16-bit mono WAVs to lamp/lamp_voice/.
#   bash mac/make_voice.sh            # then re-run hub/install-hub.sh on the hub
set -euo pipefail
OUT="$(cd "$(dirname "$0")/.." && pwd)/lamp/lamp_voice"
VOICE="${VOICE:-Samantha}"
mkdir -p "$OUT"
while IFS='|' read -r key text; do
    say -v "$VOICE" -r 175 -o "$OUT/$key.aiff" "$text"
    afconvert -f WAVE -d LEI16@22050 -c 1 "$OUT/$key.aiff" "$OUT/$key.wav"
    rm "$OUT/$key.aiff"
    echo "  $key.wav  - $text"
done <<'EOF'
start|Starting lamp test.
camera|Testing the camera.
led|Testing the lights. Watch the ring.
mic|Testing the microphones. Please speak now.
speaker|Testing the speaker. Listen.
mpr121|Testing the touch panel. Slide your finger over all the pads.
button|Testing the buttons. Press each button.
ttp223|Testing the touch sensors. Touch the pads.
servo|Servo calibration. Rotate every joint through its full range. Then press any two buttons to finish.
servo_done|Calibration saved.
servo_timeout|Calibration was not confirmed.
done_pass|Test complete. All tests passed.
done_fail|Test complete. Some tests failed.
EOF
