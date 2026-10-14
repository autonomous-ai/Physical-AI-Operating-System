#!/bin/sh
# Step 2 of make.sh: the generated speech, three processes of two threads
# each. Optionally only some kinds: sh make_speech.sh calls near
# and only some voices: VOICES=kokoro sh make_speech.sh
here=$(cd "$(dirname "$0")" && pwd)
cd "$here" || exit 1
kinds=${*:-calls near replies talk}
voices=${VOICES:-libritts cori ljspeech kokoro}
# kind:split:count:voice. Piper's LibriTTS voices are the ones trained on;
# Cori, LJ Speech and Kokoro's test voices are for testing only. Kokoro's
# training sets (blends of its other voices) are made only with
# NAME_KOKORO=on, for the inputs that tried them (make_data.py).
specs="calls:test:1500:libritts near:test:2200:libritts replies:test:1400:libritts \
  calls:train:12000:libritts near:train:9000:libritts replies:train:1500:libritts \
  calls:test:450:cori near:test:450:cori replies:test:300:cori replies:test:450:ljspeech \
  talk:train:2400:libritts talk:test:600:libritts \
  calls:test:600:kokoro near:test:600:kokoro replies:test:300:kokoro talk:test:200:kokoro"
if [ "$NAME_KOKORO" = on ]; then
  specs="$specs calls:train:6000:kokoro near:train:4000:kokoro replies:train:600:kokoro talk:train:1000:kokoro"
fi
for spec in $specs; do
  kind=$(echo "$spec" | cut -d: -f1)
  split=$(echo "$spec" | cut -d: -f2)
  count=$(echo "$spec" | cut -d: -f3)
  voice=$(echo "$spec" | cut -d: -f4)
  case " $kinds " in *" $kind "*) ;; *) continue ;; esac
  case " $voices " in *" $voice "*) ;; *) continue ;; esac
  for part in 0 1 2; do
    OMP_NUM_THREADS=2 sh run.sh speak.py "$kind" --split "$split" --count "$count" --voice "$voice" --part "$part/3" --threads 2 2>&1 | grep -v "Missing phoneme" &
  done
  wait
done
