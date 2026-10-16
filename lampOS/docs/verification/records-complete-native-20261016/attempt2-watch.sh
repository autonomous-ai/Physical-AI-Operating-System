#!/bin/bash
set -eu
out=/private/tmp/tests-823-native-20261016
for i in $(seq 1 60); do
  date '+%Y-%m-%d %H:%M:%S %z'
  if gh api repos/autonomous-ai/lampOS/actions/runs/38822752215 > "$out/931-38822752215.attempt2-latest.json"; then
    if python3 - <<'PY'
import json,pathlib
p=pathlib.Path('/private/tmp/tests-823-native-20261016/931-38822752215.attempt2-latest.json');d=json.loads(p.read_text());print(d['run_attempt'],d['status'],d['conclusion'],flush=True)
if d['run_attempt']>=2 and d['status']=='completed':
 q=p.with_name('931-38822752215.attempt2-terminal.json');q.write_bytes(p.read_bytes());raise SystemExit(0)
raise SystemExit(1)
PY
    then exit 0; fi
  fi
  sleep 120
done
exit 124
