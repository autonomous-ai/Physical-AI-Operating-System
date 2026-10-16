#!/usr/bin/env bash
# Lamp production test: record from every USB microphone at once and check each.
#
# Mics are found by USB vendor:product ID (from /proc/asound/card*/usbid), not
# by ALSA card number - card numbers change with enumeration order, IDs do not.
#
# Default mode is a CLAP test: the script counts down, the operator claps (or
# says something) near the lamp, and every mic must catch it. That is the one
# check that proves a capsule responds - ambient level alone cannot tell a
# quiet-but-working mic from a dead one (on lamp-0c89 the Jieli mic idles at
# -55 dB RMS in a quiet room, the same as a dead input).
#
# Per mic:
#   present     the USB device enumerated as an ALSA capture card
#   records     a WAV of the expected length came back
#   responds    [STIMULUS=1] clap-window peak beats that mic's own 1 s baseline
#               peak by STIM_MARGIN_DB, and is at least STIM_PEAK_DB absolute.
#               Baseline-relative because the two lamp mics differ by ~25 dB
#               in sensitivity - one fixed threshold can't fit both.
#   not silent  RMS above SILENCE_DB  (nothing at all -> near -inf)
#   not stuck   |DC offset| below DC_MAX (capsule stuck at a rail)
#   not clipped no run of repeated samples AT full scale (flat factor > 0
#               with peak above CLIP_PEAK_DB; flat factor alone also fires
#               on quiet signals, so it is not enough by itself)
#   tone        [TONE=1] a tone on SPK_DEV dominates the capture (full-band
#               RMS minus band-pass RMS < TONE_MAX_GAP_DB). Needs a real
#               speaker on SPK_DEV; on lamp-0c89 neither the C-Media nor the
#               ES8389 output reached the mics, so this is off by default.
#
# Exit 0 = every mic PASS, 1 = any FAIL. One results.log line per mic per run.
#
# Usage:
#   ./mic_test.sh                       # clap test (operator present)
#   STIMULUS=0 ./mic_test.sh            # unattended: ambient checks only
#   REC_SECONDS=5 STIM_MARGIN_DB=15 ./mic_test.sh
#   TONE=1 SPK_DEV=plughw:2,0 ./mic_test.sh
#   PLAYBACK=0 ./mic_test.sh            # don't play the recordings back
#   PLAY_DEV=device_onboard ./mic_test.sh
#   MICS="mic1=0d8c:0014 mic2=4c4a:4155" ./mic_test.sh
#
# Needs arecord + ffmpeg (both on the stock lamp image). Run as a user in the
# `audio` group (orangepi is), no sudo required.

set -uo pipefail

OUT_DIR="${OUT_DIR:-/home/orangepi/hwtest/mic}"
REC_SECONDS="${REC_SECONDS:-3}"
RATE="${RATE:-48000}"
STIMULUS="${STIMULUS:-1}"
STIM_MARGIN_DB="${STIM_MARGIN_DB:-12}" # clap peak must beat baseline peak by this
STIM_PEAK_DB="${STIM_PEAK_DB:--30}"    # ...and reach at least this, absolute
BASELINE_SECONDS="${BASELINE_SECONDS:-1}"
COUNTDOWN="${COUNTDOWN:-3}"
CLIP_PEAK_DB="${CLIP_PEAK_DB:--0.5}"   # flat samples only count as clipping above this
SILENCE_DB="${SILENCE_DB:--70}"       # below this nothing is coming in at all
DC_MAX="${DC_MAX:-0.05}"              # |DC offset|, full scale = 1
TONE="${TONE:-0}"
SPK_DEV="${SPK_DEV:-}"
TONE_HZ="${TONE_HZ:-1000}"
TONE_MAX_GAP_DB="${TONE_MAX_GAP_DB:-6}"
PLAYBACK="${PLAYBACK:-1}"              # play each recording after the test
PLAY_DEV="${PLAY_DEV:-device_speaker}" # ALSA device for playback (/etc/asound.conf)
# label=usbid pairs. The lamp ships a C-Media adapter and a Jieli composite
# device (which calls itself "device_micro2" in ALSA).
MICS="${MICS:-mic1=0d8c:0014 mic2=4c4a:4155}"

HOST="$(hostname)"
STAMP="$(date +%Y%m%d-%H%M%S)"
LOG="$OUT_DIR/results.log"
OVERALL=0

log() { printf '%s\n' "$*"; }

for tool in arecord ffmpeg; do
    command -v "$tool" >/dev/null 2>&1 || { log "FAIL: $tool not installed"; exit 1; }
done
mkdir -p "$OUT_DIR" || { log "FAIL: cannot create $OUT_DIR"; exit 1; }

# usbid -> ALSA card number, or empty
card_for_usbid() {
    local want="$1" f
    for f in /proc/asound/card*/usbid; do
        [ -r "$f" ] || continue
        if [ "$(cat "$f")" = "$want" ]; then
            basename "$(dirname "$f")" | sed 's/^card//'
            return
        fi
    done
}

# One value from ffmpeg astats' "Overall" block; optional filter in front.
astat() {   # astat <wav> <field> [filter]
    local wav="$1" field="$2" pre="${3:-}"
    local af="astats=reset=0"
    [ -n "$pre" ] && af="$pre,$af"
    ffmpeg -hide_banner -nostats -i "$wav" -af "$af" -f null - 2>&1 |
        awk -v f="$field" '/Overall/ {o=1} o && index($0, f) {print $NF; exit}'
}

# float compare: fgt a b -> true if a > b   (awk, because bash has no floats)
fgt() { awk -v a="$1" -v b="$2" 'BEGIN{exit !(a > b)}'; }
fabs() { awk -v a="$1" 'BEGIN{if (a < 0) a = -a; print a}'; }

fail_mic() {   # fail_mic <label> <usbid> <reason>
    log "  FAIL  $1: $3"
    printf '%s %s %s FAIL usbid=%s reason="%s"\n' "$STAMP" "$HOST" "$1" "$2" "$3" >> "$LOG"
    OVERALL=1
}

log "mic test on $HOST  (${REC_SECONDS}s @ ${RATE} Hz, stimulus=$STIMULUS tone=$TONE)"

# --- 1. resolve every mic first, so a missing one fails before we record ---
declare -a LABELS USBIDS CARDS NAMES WAVS
for entry in $MICS; do
    label="${entry%%=*}"; usbid="${entry#*=}"
    card="$(card_for_usbid "$usbid")"
    if [ -z "$card" ]; then
        fail_mic "$label" "$usbid" "not enumerated (no ALSA card with usbid $usbid)"
        continue
    fi
    LABELS+=("$label"); USBIDS+=("$usbid"); CARDS+=("$card")
    NAMES+=("$(cat "/proc/asound/card$card/id" 2>/dev/null)")
    WAVS+=("$OUT_DIR/${HOST}_${STAMP}_${label}.wav")
    log "  $label: card $card ($(cat "/proc/asound/card$card/id"), usb $usbid)"
done
if [ "${#CARDS[@]}" -eq 0 ]; then
    log "FAIL: no mics found"; exit 1
fi

# --- 2. optional tone ---------------------------------------------------------
TONE_PID=""
if [ "$TONE" = "1" ]; then
    [ -n "$SPK_DEV" ] || { log "FAIL: TONE=1 needs SPK_DEV (e.g. plughw:2,0)"; exit 1; }
    ffmpeg -hide_banner -loglevel error -nostdin \
        -f lavfi -i "sine=frequency=${TONE_HZ}:duration=$((REC_SECONDS + COUNTDOWN + 3))" \
        -f alsa "$SPK_DEV" &
    TONE_PID=$!
    log "  tone  ${TONE_HZ} Hz on $SPK_DEV (pid $TONE_PID)"
fi

# record every mic at once into <prefix>_<label>.wav; sets RCS[] per mic
record_all() {   # record_all <seconds> <suffix>
    local secs="$1" suffix="$2" i
    declare -ga RCS=(); local -a pids=()
    for i in "${!CARDS[@]}"; do
        timeout $((secs + 10)) arecord -q -D "plughw:${CARDS[$i]},0" \
            -f S16_LE -r "$RATE" -c 1 -d "$secs" \
            "$OUT_DIR/${HOST}_${STAMP}_${LABELS[$i]}${suffix}.wav" \
            2>"$OUT_DIR/${HOST}_${STAMP}_${LABELS[$i]}${suffix}.err" &
        pids+=("$!")
    done
    for i in "${!pids[@]}"; do wait "${pids[$i]}"; RCS+=("$?"); done
}

# --- 3a. baseline: what does each mic see with nobody doing anything -------
declare -a BASE_PEAK
if [ "$STIMULUS" = "1" ]; then
    log "  baseline ${BASELINE_SECONDS}s (stay quiet)..."
    record_all "$BASELINE_SECONDS" "_baseline"
    for i in "${!CARDS[@]}"; do
        b="$OUT_DIR/${HOST}_${STAMP}_${LABELS[$i]}_baseline.wav"
        if [ "${RCS[$i]}" -eq 0 ] && [ -s "$b" ]; then
            BASE_PEAK+=("$(astat "$b" "Peak level dB")")
        else
            BASE_PEAK+=("")
        fi
        rm -f "$b" "$b.err" "${b%.wav}.err"
    done
fi

# --- 3b. countdown + simultaneous recording ----------------------------------
if [ "$STIMULUS" = "1" ]; then
    log ""
    log "  >>> CLAP or SPEAK near the lamp when recording starts <<<"
    for ((i = COUNTDOWN; i > 0; i--)); do printf '  %d...\r' "$i"; sleep 1; done
    log "  RECORDING ${REC_SECONDS}s - make noise now!        "
else
    sleep 0.5
fi

record_all "$REC_SECONDS" ""
[ "$STIMULUS" = "1" ] && log "  done."
[ -n "$TONE_PID" ] && { kill "$TONE_PID" 2>/dev/null; wait "$TONE_PID" 2>/dev/null; }

# --- 4. analyse each ------------------------------------------------------------
want=$(( RATE * REC_SECONDS ))
for i in "${!CARDS[@]}"; do
    label="${LABELS[$i]}"; usbid="${USBIDS[$i]}"; card="${CARDS[$i]}"
    name="${NAMES[$i]}"; wav="${WAVS[$i]}"

    errf="${wav%.wav}.err"
    if [ "${RCS[$i]}" -ne 0 ]; then
        fail_mic "$label" "$usbid" "arecord failed: $(tr '\n' ' ' < "$errf")"
        rm -f "$errf"; continue
    fi
    rm -f "$errf"
    [ -s "$wav" ] || { fail_mic "$label" "$usbid" "empty recording"; continue; }

    nsamp="$(astat "$wav" "Number of samples")"
    rms="$(astat "$wav" "RMS level dB")"
    peak="$(astat "$wav" "Peak level dB")"
    flat="$(astat "$wav" "Flat factor")"
    dc="$(astat "$wav" "DC offset")"

    if [ -z "$nsamp" ] || [ "$nsamp" -lt $(( want * 9 / 10 )) ]; then
        fail_mic "$label" "$usbid" "short recording: ${nsamp:-0}/${want} samples"
        continue
    fi

    verdict="PASS"; reason=""
    if [ "$rms" = "-inf" ] || fgt "$SILENCE_DB" "$rms"; then
        verdict="FAIL"; reason="silent: RMS ${rms} dB < ${SILENCE_DB} dB"
    elif fgt "$(fabs "$dc")" "$DC_MAX"; then
        verdict="FAIL"; reason="stuck: DC offset ${dc} > ${DC_MAX}"
    elif fgt "$flat" 0 && fgt "$peak" "$CLIP_PEAK_DB"; then
        verdict="FAIL"; reason="clipping: flat factor ${flat} at peak ${peak} dB"
    elif [ "$STIMULUS" = "1" ]; then
        base="${BASE_PEAK[$i]:-}"
        if [ -z "$base" ]; then
            verdict="FAIL"; reason="baseline recording failed"
        else
            need="$(awk -v b="$base" -v m="$STIM_MARGIN_DB" -v f="$STIM_PEAK_DB" \
                     'BEGIN{n = b + m; if (n < f) n = f; printf "%.1f", n}')"
            if fgt "$need" "$peak"; then
                verdict="FAIL"
                reason="no response to clap: peak ${peak} dB, baseline ${base} dB, needed >= ${need} dB"
            fi
        fi
    fi

    tone_note=""
    if [ "$verdict" = "PASS" ] && [ "$TONE" = "1" ]; then
        inband="$(astat "$wav" "RMS level dB" "bandpass=f=${TONE_HZ}:width_type=q:w=8")"
        gap="$(awk -v a="$rms" -v b="$inband" 'BEGIN{printf "%.1f", a - b}')"
        tone_note=" tone_gap=${gap}dB"
        if fgt "$gap" "$TONE_MAX_GAP_DB"; then
            verdict="FAIL"; reason="tone not heard: full ${rms} dB vs ${TONE_HZ}Hz band ${inband} dB (gap ${gap} > ${TONE_MAX_GAP_DB})"
        fi
    fi

    if [ "$verdict" = "PASS" ]; then
        base_note=""
        [ "$STIMULUS" = "1" ] && base_note=" (baseline ${BASE_PEAK[$i]} dB)"
        log "  PASS  ${label}: peak ${peak} dB${base_note}, rms ${rms} dB, dc ${dc}${tone_note}"
        printf '%s %s %s PASS usbid=%s card=%s name="%s" peak=%s rms=%s dc=%s flat=%s%s file=%s\n' \
            "$STAMP" "$HOST" "$label" "$usbid" "$card" "$name" "$peak" "$rms" "$dc" "$flat" \
            "$tone_note" "$(basename "$wav")" >> "$LOG"
    else
        fail_mic "$label" "$usbid" "$reason"
    fi
done

# --- 5. play each recording back, one mic after another ----------------------
if [ "$PLAYBACK" = "1" ]; then
    for i in "${!CARDS[@]}"; do
        wav="${WAVS[$i]}"
        [ -s "$wav" ] || continue
        log "  playing ${LABELS[$i]} on $PLAY_DEV..."
        aplay -q -D "$PLAY_DEV" "$wav" || log "  (playback failed on $PLAY_DEV)"
        sleep 0.5
    done
fi

if [ "$OVERALL" -eq 0 ]; then
    log "PASS: all mics ok"
else
    log "FAIL: see $LOG"
fi
exit "$OVERALL"
