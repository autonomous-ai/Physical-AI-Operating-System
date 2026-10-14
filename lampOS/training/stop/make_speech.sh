#!/bin/sh
# Step 2 of make.sh: the generated speech, three processes of two threads
# each. Optionally only some kinds: sh make_speech.sh asking near
here=$(cd "$(dirname "$0")" && pwd)
cd "$here" || exit 1
kinds=${*:-asking near listening talk}
for kind_split_count in asking:train:9000 asking:test:1200 near:train:3000 near:test:1400 listening:train:600 listening:test:200 talk:train:1000 talk:test:600; do
  kind=$(echo "$kind_split_count" | cut -d: -f1)
  split=$(echo "$kind_split_count" | cut -d: -f2)
  count=$(echo "$kind_split_count" | cut -d: -f3)
  case " $kinds " in *" $kind "*) ;; *) continue ;; esac
  for part in 0 1 2; do
    OMP_NUM_THREADS=2 sh run.sh speak.py "$kind" --split "$split" --count "$count" --part "$part/3" --threads 2 &
  done
  wait
done
