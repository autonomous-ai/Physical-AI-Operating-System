#!/usr/bin/env bash
# Tests for lamp-lock.sh with no lamp: a stand-in for ssh runs the lamp's side
# here, with HOME a temporary directory. Run: lampOS/tools/lamp-lock-test.sh
set -uo pipefail

LOCK=$(cd "$(dirname "$0")" && pwd)/lamp-lock.sh
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/lamp"

# The stand-in: drops ssh's options and the host, runs the rest locally.
# FAKE_DOWN set, or FAKE_DOWN_FILE present, makes it fail as ssh does when
# the lamp cannot be reached.
cat >"$T/fake-ssh" <<'EOF'
#!/usr/bin/env bash
[[ -n ${FAKE_DOWN:-} || -e ${FAKE_DOWN_FILE:-/nonexistent} ]] && { echo "ssh: connect: no route" >&2; exit 255; }
# FAKE_HANG_FILE present: this one call hangs (the next one does not).
[[ -e ${FAKE_HANG_FILE:-/nonexistent} ]] && rm -f "$FAKE_HANG_FILE" && exec sleep 60
while [[ $# -gt 0 && $1 != *@lamp-*.local ]]; do shift; done
shift
HOME=$FAKE_HOME exec bash -c "$*"
EOF
chmod +x "$T/fake-ssh"
export LAMP_SSH="$T/fake-ssh" FAKE_HOME="$T/lamp" LAMP_LOCK_POLL=1
export FAKE_DOWN_FILE="$T/down" FAKE_HANG_FILE="$T/hang"

pass=0 fail=0
check() { # name expected-rc actual-rc
  if [[ $2 == "$3" ]]; then pass=$((pass + 1)); else fail=$((fail + 1)); echo "FAIL: $1 (expected exit $2, got $3)"; fi
}
has() { # name file pattern
  if grep -qF -- "$3" "$2"; then pass=$((pass + 1)); else fail=$((fail + 1)); echo "FAIL: $1 (no '$3' in $2)"; fi
}
L() { "$LOCK" lamp-test "$@" >"$T/out" 2>&1; }
reset() { rm -f "$T/lamp/lamp.lock" "$T/lamp/lamp.lock.log"; }
age() { # make the lock look renewed $1 seconds ago
  local e=$(($(date +%s) - $1))
  sed -i.bak "s/^renewed_epoch: .*/renewed_epoch: $e/" "$T/lamp/lamp.lock"
}

# Free, taken, held, released.
L status; check "free status" 0 $?
L try alice s1 "first test"; check "try takes a free lock" 0 $?
has "purpose kept" "$T/lamp/lamp.lock" "for: first test"
L status; check "held status" 1 $?
has "status shows the renewal" "$T/out" "last renewed"
L try bob s2 "second"; check "try refuses a held lock" 1 $?
L acquire alice s1 "again"; check "acquire when already ours" 0 $?
L release bob s2; check "release refuses another's lock" 1 $?
L release alice s1; check "release our own" 0 $?
L release alice s1; check "release when not held" 0 $?
has "release when not held says so" "$T/out" "was not held"

# A purpose with quotes and shell characters arrives unchanged.
L try alice s1 "it's \$HOME; \`date\` \"q\""; check "odd purpose" 0 $?
has "odd purpose kept" "$T/lamp/lamp.lock" "for: it's \$HOME; \`date\` \"q\""
L release alice s1; check "release after odd purpose" 0 $?

# Holder and session compare as whole strings: a dot is not a wildcard.
reset
L try builder sessionX1 "x"; check "take as sessionX1" 0 $?
L release builder session.1; check "session.1 cannot release sessionX1" 1 $?
L try builder session.1 "y"; check "session.1 is not sessionX1" 1 $?
L release builder sessionX; check "a prefix is not the session" 1 $?
L release builder sessionX1; check "sessionX1 releases" 0 $?

# The lamp cannot be reached: never free, never released.
reset
L try alice s1 "z"; check "take before the outage" 0 $?
FAKE_DOWN=1 L status; check "status, transport down" 3 $?
FAKE_DOWN=1 L release alice s1; check "release, transport down" 3 $?
FAKE_DOWN=1 L renew alice s1; check "renew, transport down" 3 $?
FAKE_DOWN=1 L try bob s2 "w"; check "try, transport down" 3 $?
[[ -e $T/lamp/lamp.lock ]]; check "lock kept through the outage" 0 $?

# The lease: renewal keeps it; 10 minutes without one lets the next take it.
age 590
L renew alice s1; check "renew our own" 0 $?
L try bob s2 "w"; check "renewed lock still held" 1 $?
L renew bob s2; check "renew refuses another's" 1 $?
age 601
L status; check "expired lock shows held" 1 $?
has "status says expired" "$T/out" "expired"
L try bob s2 "after expiry"; check "take an expired lock" 0 $?
has "the take says what it replaced" "$T/out" "replaced an expired lock"
has "the log keeps the old lock" "$T/lamp/lamp.lock.log" "holder: alice"
has "the log records the takeover once done" "$T/lamp/lamp.lock.log" "taken by bob (s2)"
L renew alice s1; check "the old holder's renew fails" 1 $?
L release bob s2; check "release after takeover" 0 $?

# A lock in the older format, with no renewal stamp, is dated by its file:
# held while fresh, expired once 10 minutes old.
reset
printf 'holder: ten\nsession: s9\nmachine: m\nsince: now\nfor: old format\n' >"$T/lamp/lamp.lock"
L try bob s2 "x"; check "a fresh lock with no stamp is held" 1 $?
L status; check "status of a fresh lock with no stamp" 1 $?
grep -q expired "$T/out"; check "a fresh lock with no stamp is not expired" 1 $?
touch -d '-11 minutes' "$T/lamp/lamp.lock"
L try bob s2 "x"; check "an old lock with no stamp has expired" 0 $?
L release bob s2; check "release after the old-format takeover" 0 $?
printf 'holder: ten\nsession: s9\nrenewed_epoch: garbage\n' >"$T/lamp/lamp.lock"
L try bob s2 "x"; check "a fresh lock with a bad stamp is held" 1 $?

# run: releases what it took, also on failure.
reset
L run alice s1 "cmd" -- true; check "run" 0 $?
[[ ! -e $T/lamp/lamp.lock ]]; check "run released" 0 $?
L run alice s1 "cmd" -- false; check "run passes the exit code" 1 $?
[[ ! -e $T/lamp/lamp.lock ]]; check "run released after failure" 0 $?

# A run inside a run, or under an explicit acquire, leaves the lock to its taker.
L run alice s1 "outer" -- "$LOCK" lamp-test run alice s1 "inner" -- true >"$T/out" 2>&1
check "nested run" 0 $?
[[ ! -e $T/lamp/lamp.lock ]]; check "outer run released" 0 $?
L acquire alice s1 "explicit"; check "explicit acquire" 0 $?
L run alice s1 "inside" -- true; check "run under explicit acquire" 0 $?
[[ -e $T/lamp/lamp.lock ]]; check "explicit lock kept after the run" 0 $?
L release alice s1; check "explicit release" 0 $?
"$LOCK" lamp-test run alice s1 "outer" -- bash -c "\"$LOCK\" lamp-test run alice s1 inner -- true; \"$LOCK\" lamp-test try bob s2 rival" >"$T/out" 2>&1
check "a rival cannot take the lock after an inner run" 1 $?

# run renews while its command runs, and stops the command if the lock is lost.
reset
LAMP_LOCK_RENEW=1 L run alice s1 "long" -- bash -c "sed -i.bak 's/^renewed_epoch: .*/renewed_epoch: 1/' '$T/lamp/lamp.lock'; sleep 3; grep '^renewed_epoch:' '$T/lamp/lamp.lock'"
check "run renews" 0 $?
grep -q '^renewed_epoch: 1$' "$T/out"; check "renewed_epoch moved on" 1 $?
reset
start=$(date +%s)
LAMP_LOCK_RENEW=1 L run alice s1 "lost" -- bash -c "rm '$T/lamp/lamp.lock'; sleep 30"
rc=$?
[[ $rc -ne 0 ]]; check "a lost lock fails the run" 0 $?
(($(date +%s) - start < 10)); check "a lost lock stops the command" 0 $?
has "a lost lock is said" "$T/out" "lock lost"

# A lost lock stops every process the command started, not only the first.
reset
LAMP_LOCK_RENEW=1 L run alice s1 "worker" -- bash -c "sleep 30 & echo \$! >'$T/worker'; rm '$T/lamp/lamp.lock'; wait"
[[ $? -ne 0 ]]; check "a lost lock fails a run with a worker" 0 $?
! kill -0 "$(cat "$T/worker")" 2>/dev/null; check "a lost lock stops the command's worker" 0 $?

# Processes the command left running are stopped before the lock is released.
reset
L run alice s1 "leftover" -- bash -c "sleep 30 & echo \$! >'$T/leftover'"
check "a run whose command left a process" 0 $?
! kill -0 "$(cat "$T/leftover")" 2>/dev/null; check "the leftover process is stopped" 0 $?
[[ ! -e $T/lamp/lamp.lock ]]; check "and the lock released" 0 $?

# An outage longer than the lease: the command stops before the lease can
# have expired, and no one else holds the lamp while it still runs.
reset; rm -f "$T/down" "$T/hang"
start=$(date +%s)
LAMP_LOCK_RENEW=1 LAMP_LOCK_STOP=4 L run alice s1 "outage" -- bash -c "touch '$T/down'; sleep 30"
rc=$?
end=$(date +%s); rm -f "$T/down"
[[ $rc -ne 0 ]]; check "an outage past the bound fails the run" 0 $?
((end - start >= 4 && end - start < 12)); check "an outage stops the command at its bound" 0 $?
has "the outage is said" "$T/out" "no renewal confirmed for 4s"

# A short outage: renewal recovers and the command finishes.
reset
LAMP_LOCK_RENEW=1 LAMP_LOCK_STOP=6 L run alice s1 "blip" -- bash -c "touch '$T/down'; sleep 2; rm '$T/down'; sleep 4"
check "a short outage recovers" 0 $?
[[ ! -e $T/lamp/lamp.lock ]]; check "released after the short outage" 0 $?

# A renewal call that hangs counts against the bound too.
reset
start=$(date +%s)
LAMP_LOCK_RENEW=1 LAMP_LOCK_STOP=4 L run alice s1 "hang" -- bash -c "touch '$T/hang'; sleep 30"
rc=$?
end=$(date +%s); rm -f "$T/hang"
[[ $rc -ne 0 ]]; check "a hung renewal fails the run" 0 $?
((end - start >= 4 && end - start < 12)); check "a hung renewal stops the command at its bound" 0 $?

# A command that ignores TERM is still stopped, by KILL, once the lease is lost.
reset
start=$(date +%s)
LAMP_LOCK_RENEW=1 L run alice s1 "stubborn" -- bash -c "trap '' TERM; echo \$\$ >'$T/stubborn'; rm '$T/lamp/lamp.lock'; while :; do sleep 1; done"
rc=$?
end=$(date +%s)
[[ $rc -ne 0 ]]; check "a TERM-ignoring command fails the run" 0 $?
! kill -0 "$(cat "$T/stubborn")" 2>/dev/null; check "a TERM-ignoring command is killed" 0 $?
((end - start < 20)); check "within the 10 s grace after the loss" 0 $?

# A command that ends while a renewal call hangs: the call is stopped too.
reset
LAMP_LOCK_RENEW=1 L run alice s1 "ends mid-renewal" -- bash -c "touch '$T/hang'; sleep 3"
check "a command ending during a hung renewal" 0 $?
! pgrep -f "^sleep 60$" >/dev/null; check "the hung renewal call is stopped with run" 0 $?
[[ ! -e $T/lamp/lamp.lock ]]; check "and the lock released" 0 $?
rm -f "$T/hang"

# The lamp cannot write or remove its lock: never "acquired" or "released".
reset
touch "$T/lamp/.lamp.lock.mutex"
chmod 555 "$T/lamp"
L try alice s1 "read-only"; check "take on a read-only lamp" 3 $?
has "the take says it could not write" "$T/out" "could not write"
[[ ! -e $T/lamp/lamp.lock ]]; check "no lock after a failed take" 0 $?
L run alice s1 "read-only" -- touch "$T/ran"; check "run on a read-only lamp" 3 $?
[[ ! -e $T/ran ]]; check "run's command does not start without the lock" 0 $?
chmod 755 "$T/lamp"
L try alice s1 "x"; check "take before the directory is locked" 0 $?
before=$(grep '^renewed_epoch:' "$T/lamp/lamp.lock")
age 100
chmod 555 "$T/lamp"
L renew alice s1; check "renew on a read-only lamp" 3 $?
L release alice s1; check "release on a read-only lamp" 3 $?
[[ -e $T/lamp/lamp.lock ]]; check "the lock stays after a failed release" 0 $?
chmod 755 "$T/lamp"
L release alice s1; check "release once writable" 0 $?

# A takeover whose log append is denied: nothing changes, nothing is logged.
reset
L try alice s1 "x"; check "take before a denied log" 0 $?
age 700
before=$(cat "$T/lamp/lamp.lock")
: >"$T/lamp/lamp.lock.log"; chmod 444 "$T/lamp/lamp.lock.log"
L try bob s2 "x"; check "takeover with the log denied" 3 $?
has "it says nothing changed" "$T/out" "nothing changed"
[[ $(cat "$T/lamp/lamp.lock") == "$before" ]]; check "the old lock is unchanged" 0 $?
[[ ! -s $T/lamp/lamp.lock.log ]]; check "and nothing was logged" 0 $?
chmod 644 "$T/lamp/lamp.lock.log"

# A takeover logged as an attempt whose replacement then fails: the lock is
# unchanged, and the log says the attempt failed, never that it was taken.
reset
L try alice s1 "x"; check "take before a failed replacement" 0 $?
age 700
before=$(cat "$T/lamp/lamp.lock")
: >"$T/lamp/lamp.lock.log"
chmod 555 "$T/lamp" # the log can be appended to; no new lock file can be made
L try bob s2 "x"; check "takeover whose replacement fails" 3 $?
chmod 755 "$T/lamp"
has "it says the lock is unchanged" "$T/out" "the lock is unchanged"
! grep -q "nothing changed" "$T/out"; check "it does not say nothing changed" 0 $?
[[ $(cat "$T/lamp/lamp.lock") == "$before" ]]; check "the old lock is unchanged after the failure" 0 $?
has "the log records the attempt" "$T/lamp/lamp.lock.log" "bob (s2) is taking it from:"
has "and its failure" "$T/lamp/lamp.lock.log" "bob (s2) could not replace it"
! grep -q "taken by bob" "$T/lamp/lamp.lock.log"; check "the log never says taken" 0 $?
L run bob s2 "x" -- touch "$T/ran"; check "run after a failed replacement" 0 $?
[[ -e $T/ran ]]; check "once writable, the next take succeeds" 0 $?
rm -f "$T/ran"

# The command ends after a renewal call has forked, before its PID was kept
# (held there by LAMP_LOCK_TEST_FORKED), while that call hangs: run still
# stops the call and releases, without waiting on it.
reset
rm -f "$T/forked"
start=$(date +%s)
LAMP_LOCK_RENEW=1 LAMP_LOCK_TEST_FORKED="$T/forked" L run alice s1 "fork window" -- \
  bash -c "touch '$T/hang'; while [ ! -e '$T/forked' ]; do sleep 0.1; done"
check "a command ending in the fork window" 0 $?
end=$(date +%s)
[[ -e $T/forked ]]; check "the window was reached" 0 $?
((end - start < 20)); check "run ends from the fork window within its bound" 0 $?
! pgrep -f "^sleep 60$" >/dev/null; check "the unkept renewal call is stopped" 0 $?
[[ ! -e $T/lamp/lamp.lock ]]; check "and the lock released from the fork window" 0 $?
rm -f "$T/hang" "$T/forked"

# A run from a terminal (here a pseudo-terminal by script(1)) whose command
# reads its input: it reads /dev/null and ends, instead of being stopped by
# the terminal while run keeps the lock renewed.
reset
if command -v script >/dev/null; then
  start=$(date +%s)
  timeout 30 script -qec "$(printf '%q ' "$LOCK" lamp-test run alice s1 tty -- bash -c 'read -r x; echo "read:[$x]"')" /dev/null >"$T/out" 2>&1
  rc=$?
  check "a run from a terminal whose command reads input" 0 $rc
  (($(date +%s) - start < 20)); check "it ends, not stopped by the terminal" 0 $?
  has "the command read an empty input" "$T/out" "read:[]"
  [[ ! -e $T/lamp/lamp.lock ]]; check "and the lock released after the terminal run" 0 $?
else
  echo "SKIP: no script(1); the terminal case is not tested here"
fi

# The bound cannot be set past the lease's last minute.
reset
grep -q 'STOP <= 540' "$LOCK"; check "the bound is capped at 540 s" 0 $?

sleep 1
! pgrep -f "^sleep 60$" >/dev/null; check "no hung call is left running" 0 $?

echo "lamp-lock: $pass passed, $fail failed"
[[ $fail -eq 0 ]]
