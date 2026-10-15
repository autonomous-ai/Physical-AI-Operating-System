#!/usr/bin/env bash
# Lamp production test: capture one full-resolution frame from the USB camera
# and save it on the board's own SD card.
#
# Finds the camera, picks its largest MJPG frame size, lets auto-exposure
# settle, grabs a frame, verifies the JPEG, and appends a line to results.log.
# Exit 0 = PASS, 1 = FAIL, so it can be chained in a bigger test run.
#
# Usage:
#   ./camera_capture_test.sh                 # auto-detect everything
#   OUT_DIR=/data/hwtest ./camera_capture_test.sh
#   CAM_DEV=/dev/video1 ./camera_capture_test.sh
#
# Output:
#   $OUT_DIR/<hostname>_<timestamp>.jpg      the frame
#   $OUT_DIR/latest.jpg                      symlink to the newest frame
#   $OUT_DIR/results.log                     one line per run: PASS/FAIL + details
#
# Needs ffmpeg + v4l2-ctl (both on the stock lamp image). Run as a user in the
# `video` group (orangepi is), no sudo required.

set -uo pipefail

OUT_DIR="${OUT_DIR:-/home/orangepi/hwtest/camera}"
CAM_DEV="${CAM_DEV:-}"
SETTLE_S="${SETTLE_S:-1.5}"     # seconds of frames to discard for auto-exposure
CAPTURE_TIMEOUT="${CAPTURE_TIMEOUT:-60}"

HOST="$(hostname)"
STAMP="$(date +%Y%m%d-%H%M%S)"
LOG="$OUT_DIR/results.log"

log()  { printf '%s\n' "$*"; }
fail() {
    log "FAIL: $*"
    mkdir -p "$OUT_DIR" 2>/dev/null
    printf '%s %s FAIL dev=%s reason="%s"\n' "$STAMP" "$HOST" "${CAM_DEV:-none}" "$*" >> "$LOG" 2>/dev/null
    exit 1
}

# --- 0. prerequisites --------------------------------------------------------
for tool in ffmpeg ffprobe v4l2-ctl; do
    command -v "$tool" >/dev/null 2>&1 || fail "$tool not installed"
done
mkdir -p "$OUT_DIR" || fail "cannot create $OUT_DIR"

# --- 1. find the camera ------------------------------------------------------
if [ -z "$CAM_DEV" ]; then
    for d in /dev/video*; do
        [ -e "$d" ] || continue
        # a capture node advertises pixel formats; metadata nodes do not
        if v4l2-ctl -d "$d" --list-formats 2>/dev/null | grep -qE "MJPG|YUYV"; then
            CAM_DEV="$d"
            break
        fi
    done
fi
[ -n "$CAM_DEV" ] || fail "no USB camera found (no /dev/video* with MJPG/YUYV)"
[ -r "$CAM_DEV" ] || fail "$CAM_DEV not readable - add user to the video group"

CAM_NAME="$(v4l2-ctl -d "$CAM_DEV" --info 2>/dev/null | awk -F': *' '/Card type/ {print $2}')"
log "camera : $CAM_DEV (${CAM_NAME:-unknown})"

# --- 2. largest MJPG frame size ---------------------------------------------
# Walk --list-formats-ext, track which pixel format we are under, and keep the
# biggest WxH by area for MJPG. Fall back to YUYV if the camera has no MJPG.
# Written for mawk (the stock awk on the lamp image): no \s, no gawk-isms.
pick_max() {
    local want="$1"
    v4l2-ctl -d "$CAM_DEV" --list-formats-ext 2>/dev/null | awk -v want="$want" '
        /^[ \t]*\[[0-9]+\]:/ { infmt = (index($0, "\x27" want "\x27") > 0) }
        infmt && /Size: Discrete/ {
            split($3, wh, "x"); a = wh[1] * wh[2]
            if (a > best) { best = a; size = $3 }
        }
        END { print size }'
}
FMT="mjpeg"
SIZE="$(pick_max MJPG)"
if [ -z "$SIZE" ]; then
    FMT="yuyv422"
    SIZE="$(pick_max YUYV)"
fi
[ -n "$SIZE" ] || fail "could not read frame sizes from $CAM_DEV"
log "format : $FMT @ $SIZE (max)"

# --- 3. capture --------------------------------------------------------------
OUT="$OUT_DIR/${HOST}_${STAMP}.jpg"
rm -f "$OUT"
log "capture: settling ${SETTLE_S}s, then one frame -> $OUT"

if ! timeout "$CAPTURE_TIMEOUT" ffmpeg -hide_banner -loglevel error -nostdin \
        -f v4l2 -input_format "$FMT" -video_size "$SIZE" -i "$CAM_DEV" \
        -ss "$SETTLE_S" -frames:v 1 -q:v 2 -y "$OUT"; then
    fail "ffmpeg capture failed on $CAM_DEV"
fi

# --- 4. verify ---------------------------------------------------------------
[ -s "$OUT" ] || fail "output file missing or empty"
DIMS="$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height \
        -of csv=p=0 "$OUT" 2>/dev/null | tr ',' 'x')"
[ -n "$DIMS" ] || fail "output is not a readable image"
[ "$DIMS" = "$SIZE" ] || fail "captured $DIMS but expected $SIZE"
BYTES="$(stat -c %s "$OUT")"
# a black / sensor-fault frame compresses to almost nothing; real scenes do not
[ "$BYTES" -gt 20000 ] || fail "image suspiciously small (${BYTES} bytes) - lens covered or sensor dead?"

ln -sfn "$(basename "$OUT")" "$OUT_DIR/latest.jpg"
printf '%s %s PASS dev=%s cam="%s" fmt=%s size=%s bytes=%s file=%s\n' \
    "$STAMP" "$HOST" "$CAM_DEV" "$CAM_NAME" "$FMT" "$DIMS" "$BYTES" "$(basename "$OUT")" >> "$LOG"

log "result : $DIMS, $BYTES bytes"
log "PASS   : $OUT"
exit 0
