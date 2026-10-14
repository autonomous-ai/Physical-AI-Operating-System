#!/bin/sh
# Records what the body says while a person pushes the lamp, by itself:
# every joint's position, goal and load (body/joint_states) and the body's
# own report with temperatures (diagnostics), each line with its time.
# Marks are added with `record.sh mark "push head down, standing"` (and the
# kitchen scale's reading, if any) from another shell. Numbers only.
#
#   record.sh start [SECONDS]   start recording in the background (1200 s)
#   record.sh mark TEXT         add a timed mark
#   record.sh stop              stop recording
set -eu
BUS=${LAMP_BUS_TOOL:-lamp-bus}
DIR=${RECORD_DIR:-$HOME/fifth-give-way-hand}
case "${1:-}" in
start)
    mkdir -p "$DIR"
    seconds=${2:-1200}
    nohup "$BUS" tap body/joint_states --seconds "$seconds" >"$DIR/joint_states.txt" 2>&1 &
    echo $! >"$DIR/pids"
    nohup "$BUS" tap diagnostics --seconds "$seconds" >"$DIR/diagnostics.txt" 2>&1 &
    echo $! >>"$DIR/pids"
    echo "recording in $DIR for $seconds s"
    ;;
mark)
    shift
    printf '%s %s\n' "$(date +%s.%N)" "$*" >>"$DIR/marks.txt"
    ;;
stop)
    xargs kill <"$DIR/pids" 2>/dev/null || true
    rm -f "$DIR/pids"
    wc -l "$DIR"/*.txt
    ;;
*)
    sed -n '2,12p' "$0"
    exit 2
    ;;
esac
