#!/bin/sh
# The continuation cases of the strict probe, for a Lamp or any Linux host
# that already holds the provider file and its credential.
#
#   lamp-cases.sh PROBE PROVIDER.json FIRST.wav SECOND.wav OUTDIR
#
# PROBE is the `service_probe` example built for this machine. PROVIDER.json
# is the runtime's own provider file, used where it is: the credential it
# names is read in place by the probe and is neither copied nor printed.
# FIRST.wav and SECOND.wav are cached synthetic speech, exact mono 16 kHz
# PCM16. No microphone and no speaker are opened and nothing recorded in a
# room is sent.
#
# One connection per case. Each result is one JSON line in OUTDIR; this
# prints only the case, the probe's exit status and its outcome label. The
# JSON holds event kinds, timings, counts and bounded transcripts of the
# synthetic prompts and replies: review it before sharing it.
set -eu
[ "$#" -eq 5 ] || { echo "usage: lamp-cases.sh PROBE PROVIDER.json FIRST.wav SECOND.wav OUTDIR" >&2; exit 2; }
PROBE=$1; PROVIDER=$2; FIRST=$3; SECOND=$4; OUT=$5
for file in "$PROBE" "$PROVIDER" "$FIRST" "$SECOND"; do
    [ -f "$file" ] || { echo "missing: $file" >&2; exit 2; }
done
[ -x "$PROBE" ] || { echo "not executable: $PROBE" >&2; exit 2; }
mkdir -p "$OUT"
[ -z "$(ls -A "$OUT")" ] || { echo "not empty: $OUT" >&2; exit 2; }

run() {
    name=$1; shift
    code=0
    "$PROBE" "$PROVIDER" "$@" > "$OUT/$name.json" 2> "$OUT/$name.stderr" || code=$?
    outcome=$(sed -n 's/.*"outcome":"\([a-z_]*\)".*/\1/p' "$OUT/$name.json" | head -n 1)
    printf '%s exit=%s outcome=%s\n' "$name" "$code" "${outcome:-none}"
    sleep 3
}

# The case that failed, as it failed: the control. A strict timeout is the
# expected outcome here, and a non-zero exit.
run default-before-output-300 "$FIRST" continue-before-output "$SECOND" 300
# The same, watched for five seconds and never a pass: what, if anything,
# the service sends for a response interrupted before output, including the
# `waiting_for_input` and resumption counts in its barrier row.
run observed-before-output-300 "$FIRST" continue-before-output "$SECOND" 300 barrier-ms=5000
# The proposal: the second start and its opening audio kept until the first
# answer begins. Three resumption gaps.
run kept-before-output-0 "$FIRST" continue-before-output "$SECOND" 0 barrier-wait=at-commit keep-start-ms=2500
run kept-before-output-300 "$FIRST" continue-before-output "$SECOND" 300 barrier-wait=at-commit keep-start-ms=2500
run kept-before-output-800 "$FIRST" continue-before-output "$SECOND" 800 barrier-wait=at-commit keep-start-ms=2500
# The same question asked again before the first answer.
run kept-repeat-300 "$FIRST" repeat-before-output 300 barrier-wait=at-commit keep-start-ms=2500
# With the first answer already producing nothing is kept: the setting must
# leave this case as it was.
run kept-during-output "$FIRST" continue-during-output "$SECOND" barrier-wait=at-commit keep-start-ms=2500
echo "done: $OUT"
