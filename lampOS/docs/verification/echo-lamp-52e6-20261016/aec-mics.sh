#!/bin/bash
# mic1 vs mic2: how much of the lamp's own voice the echo canceller removes
# while it tells a story. One spoken turn per microphone, numbers only.
# Run from a Mac in earshot of the lamp:  bash aec-mics.sh [host]
# Needs: ssh to the lamp with passwordless sudo, macOS `say`.
set -euo pipefail
L=${1:-orangepi@lamp-52e6.local}
ASK="Lamp! Hey Lamp, tell me a short story about a cat."
STORY_S=${STORY_S:-40}
DROPIN=/etc/systemd/system/lamp-voice.service.d/fifth-aec.conf

r() { ssh -o BatchMode=yes "$L" "$@"; }

cleanup() {
  echo "restoring lamp-voice and lamp-ears as recorded..."
  r "sudo rm -f $DROPIN; sudo rmdir --ignore-fail-on-non-empty /etc/systemd/system/lamp-voice.service.d 2>/dev/null; sudo systemctl daemon-reload; sudo systemctl restart lamp-voice; sudo systemctl start lamp-ears; systemctl is-active lamp-voice lamp-ears | tr '\n' ' '; echo"
}
trap cleanup EXIT

# Echo removed over duplex_hearing trails of the current voice session:
# per 10 ms block while the lamp spoke, capture (before AEC) minus residual
# (after), from the 3 dB level letters; only blocks within 20 dB of the
# trail's loudest, so the floor does not cap the figure.
measure() {
  r 'sudo sh -c "cat /var/lib/lampOS/voice/sessions/$(systemctl show -p InvocationID --value lamp-voice)/metadata/events-*.jsonl"' |
    grep '"kind":"duplex_hearing"' |
    awk -v L=abcdefghijklmnopqrstuvwxyz '
      function field(k) { return match($0, "\"" k "\":\"[^\"]*\"") ? substr($0, RSTART + length(k) + 4, RLENGTH - length(k) - 5) : "" }
      { c = field("capture_level"); z = field("residual_level"); top = 99
        for (i = 1; i <= length(c); i++) { x = index(L, substr(c, i, 1)); if (x && x < top) top = x }
        for (i = 1; i <= length(c); i++) {
          a = index(L, substr(c, i, 1)); b = index(L, substr(z, i, 1))
          if (a && b && a - top <= 6) print 3 * (b - a), -3 * (a - 1), -3 * (b - 1)
        } }' |
    sort -n |
    awk '{ d[NR] = $1; cap += $2; res += $3; if ($3 <= -75) fl++ }
      END { if (!NR) { print "no blocks while the lamp spoke"; exit }
        printf "blocks=%d echo_removed_db median=%d p25=%d p75=%d capture_dbfs_mean=%.0f residual_dbfs_mean=%.0f residual_at_floor(z)=%d\n",
          NR, d[int((NR + 1) / 2)], d[1 + int(.25 * (NR - 1))], d[1 + int(.75 * (NR - 1))], cap / NR, res / NR, fl }'
}

one() { # $1 = mic name
  r "sudo systemctl restart lamp-voice"
  sleep 12                       # voice ready, provider connected
  say -v Samantha "$ASK"
  sleep "$STORY_S"               # the story plays
  printf '%-5s ' "$1"; measure
}

echo "== mic2 (the unit as recorded)"
r "sudo systemctl start lamp-ears"
one mic2

echo "== mic1 (drop-in $DROPIN; lamp-ears stopped, it holds mic1)"
r "sudo systemctl stop lamp-ears
   sudo mkdir -p /etc/systemd/system/lamp-voice.service.d
   ex=\$(systemctl show -p ExecStart --value lamp-voice | sed -n 's/.*argv\[\]=\([^;]*\);.*/\1/p' | sed 's/--microphone mic2/--microphone mic1/')
   case \"\$ex\" in *'--microphone mic1'*) ;; *) echo 'could not derive ExecStart' >&2; exit 1;; esac
   printf '[Service]\nExecStart=\nExecStart=%s\n' \"\$ex\" | sudo tee $DROPIN >/dev/null
   sudo systemctl daemon-reload"
one mic1
