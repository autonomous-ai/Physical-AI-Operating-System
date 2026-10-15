#!/bin/zsh
# Replays each 14 Oct duplex run under the three suppressor settings into
# lampOS/.cache/828-d14-<run>-<mode>, scores it as the voice hears it, and
# keeps only the report, the AEC-only output and the hearing table.
cd "${0:A:h}/../../../.." || exit 9  # lampOS/
D=~/lamp-voice-data/sessions/duplex-20261014
for run in cut-a cut-b cut-c cut-d cut-q both-a both-noagc; do
  for src in $D/$run/$run-*; do [[ -d $src ]] && break; done
  for mode in standard near-end transparent; do
    out=.cache/828-d14-$run-$mode
    [[ -f $out/segment-001-hearing.tsv ]] && { echo "$run $mode kept"; continue; }
    if [[ ! -f $out/report.json ]]; then
      rm -rf $out
      s=$(date +%s)
      ./target/release/lamp-audio replay-aec $src $out --suppressor $mode > /dev/null 2> $out.err || { echo "$run $mode replay failed"; continue; }
      echo "$run $mode replayed $(( $(date +%s) - s ))s"
    fi
    ./target/release/examples/hearing_replay $out 2>> $out.err || { echo "$run $mode hearing failed"; continue; }
    rm -f $out/operations.jsonl $out/segment-001-aec_ns.wav $out/segment-001-pre_aec.wav $out/segment-001-recorded_post_aec.wav
  done
done
echo all-done
