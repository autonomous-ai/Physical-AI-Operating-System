#!/bin/sh
# The recordings with a person for task 39 (#720): whether a face's mouth
# moves, against when the person was asked to speak. Run on the Mac beside
# the lamp, with the person sitting in front of the lamp, about a metre away,
# facing it. Numbers only: nothing here keeps a picture or a sound.
#
#   sh mouths.sh LAMP EYES_SHA OUT_DIR [BUS_CLI]
#
# LAMP is the ssh target (orangepi@lamp-52e6.local), EYES_SHA the folder
# under /opt/lampOS/eyes holding a lamp-eyes built from fifth/mouth-moving,
# BUS_CLI the lamp-bus program on the lamp (~/fifth-lamp-bus). The eyes run
# with the mouths read for the run only; the lamp's own unit is restored at
# the end, whatever happens.
set -eu
LAMP=$1 SHA=$2 OUT=$3 BUS=${4:-'~/fifth-lamp-bus'}
DROP=/etc/systemd/system/lamp-eyes.service.d/mouth-trial.conf
mkdir -p "$OUT"

now() { perl -MTime::HiRes=time -e 'printf "%.3f\n", time'; }

on() {
    ssh "$LAMP" "set -e; E=\$(systemctl show -p ExecStart --value lamp-eyes | sed -n 's/.*argv\[\]=\([^;]*\);.*/\1/p' | sed 's#/opt/lampOS/eyes/[0-9a-f]*/lamp-eyes#/opt/lampOS/eyes/$SHA/lamp-eyes#'); test -x /opt/lampOS/eyes/$SHA/lamp-eyes; sudo mkdir -p \$(dirname $DROP); printf '[Service]\nEnvironment=LAMP_BUS=on LAMP_EYES_MOUTH=on\nExecStart=\nExecStart=%s\n' \"\$E\" | sudo tee $DROP >/dev/null; sudo systemctl daemon-reload; sudo systemctl restart lamp-eyes"
}
off() {
    ssh "$LAMP" "sudo rm -f $DROP; sudo systemctl daemon-reload; sudo systemctl restart lamp-eyes" || true
}
trap off EXIT INT TERM

# The lamp's clock less the Mac's, taken once: marks are written in the
# lamp's clock (seconds since its boot, as the bus's at_us).
offset() {
    before=$(now); lamp=$(ssh "$LAMP" 'cut -d" " -f1 /proc/uptime'); after=$(now)
    echo "$lamp $before $after" | awk '{printf "%.3f\n", $1 - ($2 + $3) / 2}'
}

mark() { echo "$(now) $OFF $1" | awk '{printf "%.3f %s\n", $1 + $2, $3}' >> "$MARKS"; }

# A scene: NAME, then pairs of LABEL SECONDS. A label is shown big and
# written down at the moment it is shown.
scene() {
    name=$1; shift
    MARKS=$OUT/$name.marks; : > "$MARKS"
    total=0; for value in "$@"; do case $value in *[!0-9]*) ;; *) total=$((total + value)) ;; esac; done
    ssh "$LAMP" "$BUS tap humans/faces/mouths --seconds $((total + 3))" > "$OUT/$name.mouths" 2>&1 &
    tap=$!
    sleep 1
    while [ $# -ge 2 ]; do
        label=$1 seconds=$2; shift 2
        mark "$label"
        printf '\n\n        %s   (%s s)\n\n' "$label" "$seconds"
        sleep "$seconds"
    done
    mark end
    wait $tap || true
}

echo "Turning the mouths on for this run."
on
# The camera opens only while the voice permits it, and after a restart of
# lamp-eyes it can fail to open for minutes (DeadlineExceeded): wait for a look.
echo "Waiting for the camera to look (up to 10 minutes)..."
since=$(ssh "$LAMP" 'date +%T')
i=0
until ssh "$LAMP" "journalctl -u lamp-eyes --since $since --no-pager -o cat | grep -q 'looked at [1-9]'"; do
    i=$((i + 1)); [ $i -lt 120 ] || { echo "The camera has not looked; stopping."; exit 1; }
    sleep 5
done
OFF=$(offset)

echo "1. Talking to the lamp. Face it. Speak (anything) when it says SPEAK, be still when it says QUIET."
sleep 3
scene talking QUIET 4 SPEAK 4 QUIET 4 SPEAK 4 QUIET 4 SPEAK 4 QUIET 4

echo "2. Talking over the lamp. The Mac asks it for a story; speak over it when it says SPEAK."
say -v Daniel "Hey Lamp, tell me a long story about a lighthouse keeper and his cat."
sleep 7
scene over QUIET 4 SPEAK 3 QUIET 4 SPEAK 3 QUIET 4 SPEAK 3 QUIET 4

echo "3. Silent, facing the lamp. Look at it and say nothing."
sleep 3
scene silent QUIET 15

echo "4. Chewing and smiling, silent. CHEW: chew (gum or nothing), SMILE: smile and laugh silently."
sleep 3
scene chewing QUIET 3 CHEW 6 QUIET 3 SMILE 6 QUIET 3

if [ "${TWO:-}" = 1 ]; then
    echo "5. Two people. A speaks on A, B on B, both still on QUIET."
    sleep 3
    scene two QUIET 3 A 4 QUIET 3 B 4 QUIET 3 A 4 QUIET 3
fi

# Each scene: the mean score while each label was shown, per face id.
for marks in "$OUT"/*.marks; do
    name=$(basename "$marks" .marks)
    sed -n 's/.*"at_us":\([0-9]*\).*"mouths":\[\(.*\)\]}.*/\1 \2/p' "$OUT/$name.mouths" |
        awk -v marks="$marks" -v name="$name" '
            BEGIN { while ((getline line < marks) > 0) { split(line, f, " "); at[++n] = f[1]; label[n] = f[2] } }
            {
                t = $1 / 1e6; l = ""
                for (i = 1; i <= n; i++) if (t >= at[i] + 0.5) l = label[i]   # half a second to begin
                if (l == "" || l == "end") next
                rest = $0
                while (match(rest, /"id":"[a-z0-9]*","moving":[0-9.e-]*/)) {
                    item = substr(rest, RSTART, RLENGTH); rest = substr(rest, RSTART + RLENGTH)
                    split(item, kv, "\""); id = kv[4]; split(item, mv, ":"); m = mv[3] + 0
                    sum[l] += m; count[l]++
                }
            }
            END { for (l in sum) printf "%s %s mean %.3f over %d scores\n", name, l, sum[l] / count[l], count[l] }'
done | sort | tee "$OUT/summary.txt"
