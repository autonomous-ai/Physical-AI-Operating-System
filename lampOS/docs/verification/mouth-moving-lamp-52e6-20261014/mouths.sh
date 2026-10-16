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
#
# The eyes also say each frame's raw changes (LAMP_EYES_MOUTH_RAW=on): the
# shift each frame was lined up by, the lower face's mean change and its
# share of points past 8, 12 and 16 grey, the eyes' mean change before
# and after the line-up, each region's offset (mean signed change) and the
# share past 12 with it taken out, with each frame's taking time. They stay in the
# .mouths files whole, to set the step, the full share and the head gate
# from (a frame is in several messages: count each taken_us once); the
# run ends by printing each prompt's median of each, and the frames the
# head gate keeps (raw-medians.txt).
set -eu
LAMP=$1 SHA=$2 OUT=$3 BUS=${4:-'~/fifth-lamp-bus'}
DROP=/etc/systemd/system/lamp-eyes.service.d/mouth-trial.conf
mkdir -p "$OUT"

now() { perl -MTime::HiRes=time -e 'printf "%.3f\n", time'; }

on() {
    ssh "$LAMP" "set -e; E=\$(systemctl show -p ExecStart --value lamp-eyes | sed -n 's/.*argv\[\]=\([^;]*\);.*/\1/p' | sed 's#/opt/lampOS/eyes/[0-9a-f]*/lamp-eyes#/opt/lampOS/eyes/$SHA/lamp-eyes#'); test -x /opt/lampOS/eyes/$SHA/lamp-eyes; sudo mkdir -p \$(dirname $DROP); printf '[Service]\nEnvironment=LAMP_BUS=on LAMP_EYES_MOUTH=on LAMP_EYES_MOUTH_RAW=on\nExecStart=\nExecStart=%s\n' \"\$E\" | sudo tee $DROP >/dev/null; sudo systemctl daemon-reload; sudo systemctl restart lamp-eyes"
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
                for (i = 1; i <= n; i++) if (t >= at[i] + 1.2) l = label[i]   # the 1.2 s window to fill
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

# Each scene: the median of each raw field while each prompt was shown,
# each frame once (by taken_us), from 0.5 s into the prompt.
python3 -I - "$OUT" <<'PY' | tee "$OUT/raw-medians.txt"
import glob, json, os, statistics, sys
out = sys.argv[1]
decode = json.JSONDecoder().raw_decode
for marks in sorted(glob.glob(os.path.join(out, "*.marks"))):
    name = os.path.basename(marks)[:-6]
    at = [(float(t), label) for t, label in (line.split() for line in open(marks))]
    frames = {}
    for line in open(os.path.join(out, name + ".mouths")):
        try:
            _, end = decode(line)
            body, _ = decode(line, end + 1)
        except ValueError:
            continue
        for mouth in body.get("mouths", []):
            for raw in mouth.get("raw") or []:
                frames[raw["taken_us"]] = raw
    by = {}
    for taken, raw in frames.items():
        shown = [(t, l) for t, l in at if t <= taken / 1e6]
        if shown and shown[-1][1] != "end" and taken / 1e6 >= shown[-1][0] + 0.5:
            by.setdefault(shown[-1][1], []).append(raw)
    for label, raws in sorted(by.items()):
        keys = sorted(k for k in raws[0] if k != "taken_us")
        cols = " ".join(f"{k} {statistics.median(r[k] for r in raws):.3f}" for k in keys)
        print(f"{name} {label} frames {len(raws)} {cols}")
        # The frames the head gate keeps (HEAD_MOVED, 10), what the score
        # is made of in them, and whether the two regions change together.
        kept = [r["lower_share12_corr"] for r in raws if r["eyes_after"] <= 10 and "lower_share12_corr" in r]
        try:
            together = statistics.correlation([r["eyes_after"] for r in raws], [r["lower_mean"] for r in raws])
        except statistics.StatisticsError:
            together = 0.0
        middle = f"{statistics.median(kept):.3f}" if kept else "-"
        print(f"{name} {label} kept {len(kept)} of {len(raws)} lower_share12_corr {middle} corr_eyes_lower {together:.2f}")
PY
