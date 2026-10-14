#!/bin/sh
# Step 2 of make.sh: the generated speech, three processes of two threads
# each. Optionally only some kinds: sh make_speech.sh calls near
here=$(cd "$(dirname "$0")" && pwd)
cd "$here" || exit 1
kinds=${*:-calls near replies talk}
# kind:split:count:voice; the two other voices are for testing only.
for spec in calls:test:1500:libritts near:test:2200:libritts replies:test:1400:libritts \
  calls:train:12000:libritts near:train:9000:libritts replies:train:1500:libritts \
  calls:test:450:cori near:test:450:cori replies:test:300:cori replies:test:450:ljspeech \
  talk:train:2400:libritts talk:test:600:libritts; do
  kind=$(echo "$spec" | cut -d: -f1)
  split=$(echo "$spec" | cut -d: -f2)
  count=$(echo "$spec" | cut -d: -f3)
  voice=$(echo "$spec" | cut -d: -f4)
  case " $kinds " in *" $kind "*) ;; *) continue ;; esac
  for part in 0 1 2; do
    OMP_NUM_THREADS=2 sh run.sh speak.py "$kind" --split "$split" --count "$count" --voice "$voice" --part "$part/3" --threads 2 2>&1 | grep -v "Missing phoneme" &
  done
  wait
done
