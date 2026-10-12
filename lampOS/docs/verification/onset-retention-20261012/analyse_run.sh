#!/bin/bash
# One device diagnostic run of the sunset/coffee pair -> native-bundle JSON.
# usage: analyse_run.sh EXECUTED_DIR PLAN_DIR WORK_DIR OUT.json [MIXERS.json]
# Reads the run; writes only under WORK_DIR and OUT.json.
set -eu
SP="${SP:?set SP to the directory holding m050-handoff, onset and target-retain}"
E="$1"; P="$2"; W="$3"; OUT="$4"; MIX="${5:-$P/expected-mixers.json}"
PY="/usr/local/bin/python3.12 -I"
mkdir -p "$W"
$PY "$SP/m050-handoff/prepare_lab.py" "$E/native/runtime" "$W/lab" | tail -1
cp "$W/lab/schedule.tsv" "$W/lab/schedule.device.tsv"
$PY - "$W/lab" <<'PYEOF'
import sys
L = sys.argv[1]
rows = [l.rstrip('\n').split('\t') for l in open(L + '/schedule.device.tsv') if not l.startswith('#')]
worst = max(int(a) - (int(a) // 240) * 240 for a, _ in rows)
open(L + '/schedule.tsv', 'w').write('# render cursor rounded down to a whole 240-frame block for the lab; delay hint ms\n' + ''.join(f'{(int(a) // 240) * 240}\t{h}\n' for a, h in rows))
print('render cursor rounded; largest rounding', worst, 'frames')
PYEOF
"$SP/target-retain/release/examples/duplex_lab" "$W/lab" "$W/lab/base" > "$W/lab/base.stdout" 2>&1; tail -1 "$W/lab/base.stdout"
read -r NOMINAL BETWEEN COFFEE CSHA SUNSET SSHA <<<"$($PY - "$E/result.json" <<'PYEOF'
import json, sys
r = json.load(open(sys.argv[1])); a, b = r['steps'][0], r['steps'][1]
cue = b['original_trigger']['cue']['event_us']
delay_us = (b['player_metadata']['counters']['source_started_host_ns'] - b['original_trigger']['received_host_clock_monotonic_ns']) // 1000
between = (b['player_metadata']['counters']['source_started_host_ns'] - a['player_metadata']['counters']['source_started_host_ns']) / 1e9
print(cue + delay_us, between, b.get('original_wav_path') or b['wav_path'], b['wav_sha256'], a['wav_path'], a['wav_sha256'])
PYEOF
)"
echo "nominal clip start (lamp us) $NOMINAL; between source starts $BETWEEN s"
$PY "$SP/onset/native_bundle.py" "$E/native/runtime" "$W/lab/base/features.tsv" "$COFFEE" "$CSHA" "$SUNSET" "$SSHA" "$NOMINAL" "$OUT" "$BETWEEN" auto "$MIX" > "$W/analysis.out" 2>&1
echo "analysis exit $?"
