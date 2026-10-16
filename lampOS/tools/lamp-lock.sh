#!/usr/bin/env bash
# One lock per lamp, kept on the lamp itself, so builders on any machine take
# turns at it. See lampOS/docs/joining.md, "Sharing a lamp".
#
#   lamp-lock.sh <lamp> status                     # exit 0 free, 1 held, 3 unknown
#   lamp-lock.sh <lamp> try     <holder> <session> <for...>   # exit 1 at once if held
#   lamp-lock.sh <lamp> acquire <holder> <session> <for...>   # wait while held
#   lamp-lock.sh <lamp> renew   <holder> <session>            # every 5 minutes
#   lamp-lock.sh <lamp> release <holder> <session>            # only our own lock
#   lamp-lock.sh <lamp> run     <holder> <session> <for> -- <command...>
#                     # acquire, run the command here renewing the lock, release
#
# The lock is ~orangepi/lamp.lock on <lamp>.local. Every step on it (look,
# take, renew, release) runs on the lamp in one ssh call under flock(1) on
# ~/.lamp.lock.mutex, so no two steps interleave. It is a lease: a lock not
# renewed for 10 minutes, by the lamp's own clock, has expired, and the next
# take replaces it and appends what it replaced to ~/lamp.lock.log. A lock is
# ours when its holder and session lines equal ours exactly.
#
# Exit codes: 0 done, 1 held by another (or our lock lost), 2 usage,
# 3 the lamp could not be asked (ssh failed, or flock timed out).
#
# Reaching the lamp:
#   LAMP_SSH        the ssh command (default: ssh -o BatchMode=yes, a 15 s
#                   connect timeout and a 15 s keepalive)
#   LAMP_SSH_OPTS   extra options, e.g. "-J <host>" for a jump host
#   LAMP_USER       the lamp's user (default: orangepi)
#   LAMP_LOCK_POLL  seconds between tries while waiting (default: 30)
#   LAMP_LOCK_RENEW seconds between renewals during run (default: 300)
#   LAMP_LOCK_STOP  seconds after the start of the last confirmed take or
#                   renewal at which run stops its command (default and
#                   largest: 540, a minute inside the lamp's 600 s lease)
set -euo pipefail

usage() { sed -n '4,11p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
[[ $# -ge 2 ]] || usage
LAMP=$1 CMD=$2; shift 2
[[ "$LAMP" =~ ^lamp-[0-9a-z]+$ ]] || { echo "not a lamp name: $LAMP" >&2; exit 2; }
SSH=${LAMP_SSH:-ssh -o BatchMode=yes -o ConnectTimeout=15 -o ServerAliveInterval=5 -o ServerAliveCountMax=3}
POLL=${LAMP_LOCK_POLL:-30}
RENEW=${LAMP_LOCK_RENEW:-300}
STOP=${LAMP_LOCK_STOP:-540}
((STOP <= 540)) || STOP=540
# Holder and session go into a remote command line, so they stay plain.
plain() { [[ "$1" =~ ^[A-Za-z0-9._-]+$ ]] || { echo "holder and session: letters, digits, . _ - only" >&2; exit 2; }; }

# The part that runs on the lamp, on stdin to `bash -s`. Arguments: the step,
# then holder, session, machine and the purpose in base64. Its exit codes are
# its answers; anything else (ssh's 255, a missing flock) is a failure to ask.
#   10 held by another   11 not held   12 already ours   0 done
#   5 the lamp could not write or remove its lock (nothing was changed)
#   6 an expired lock could not be replaced: it is unchanged, and the log
#     records the attempt and its failure
# shellcheck disable=SC2016 # expanded on the lamp
REMOTE='
set -u
step=$1 holder=${2-} session=${3-} machine=${4-} purpose=${5-}
L=$HOME/lamp.lock LOG=$HOME/lamp.lock.log EXPIRE=600
exec 9>"$HOME/.lamp.lock.mutex"
flock -w 20 9 || exit 4
now=$(date +%s)
field() { sed -n "s/^$1: //p" "$L" | head -n 1; }
held=no ours=no expired=no
if [ -e "$L" ]; then
  held=yes
  [ "$(field holder)" = "$holder" ] && [ "$(field session)" = "$session" ] && ours=yes
  renewed=$(field renewed_epoch)
  # A lock with no stamp (an older lamp-lock.sh wrote none) dates from the
  # file itself, which every take and renew rewrites; never from epoch 0.
  case $renewed in ""|*[!0-9]*) renewed=$(stat -c %Y "$L") ;; esac
  age=$((now - renewed))
  [ "$age" -ge "$EXPIRE" ] && expired=yes
fi
write() { # since since_epoch
  printf "holder: %s\nsession: %s\nmachine: %s\nfor: %s\nsince: %s\nsince_epoch: %s\nrenewed: %s\nrenewed_epoch: %s\n" \
    "$holder" "$session" "$machine" "$purpose" "$1" "$2" "$(date "+%Y-%m-%d %H:%M:%S %z")" "$now" >"$L.new" &&
    mv -f "$L.new" "$L" || { rm -f "$L.new"; return 1; }
}
case $step in
status)
  [ $held = yes ] || exit 0
  cat "$L"
  echo "last renewed ${age}s ago$([ $expired = yes ] && echo ", expired")"
  exit 10 ;;
take)
  if [ "$purpose" = _ ]; then purpose=; else purpose=$(printf %s "$purpose" | base64 -d); fi
  if [ $held = yes ] && [ $expired = no ]; then
    [ $ours = yes ] && exit 12
    cat "$L"; echo "last renewed ${age}s ago"; exit 10
  fi
  if [ $held = no ]; then
    write "$(date "+%Y-%m-%d %H:%M:%S %z")" "$now" || exit 5
    exit 0
  fi
  # A takeover is logged as an attempt first, and as done only once the
  # lock has been replaced.
  old=$(cat "$L")
  { echo "--- $(date "+%Y-%m-%d %H:%M:%S %z") expired after ${age}s; $holder ($session) is taking it from:"; printf "%s\n" "$old"; } >>"$LOG" || exit 5
  if ! write "$(date "+%Y-%m-%d %H:%M:%S %z")" "$now"; then
    echo "--- $(date "+%Y-%m-%d %H:%M:%S %z") $holder ($session) could not replace it; the lock above is unchanged" >>"$LOG"
    exit 6
  fi
  echo "--- $(date "+%Y-%m-%d %H:%M:%S %z") taken by $holder ($session)" >>"$LOG" ||
    echo "warning: the takeover is done, but the log could not record it" >&2
  echo "replaced an expired lock (${age}s without renewal):"; printf "%s\n" "$old"
  exit 0 ;;
renew)
  [ $held = yes ] || exit 11
  [ $ours = yes ] || exit 10
  purpose=$(field for) machine=$(field machine)
  write "$(field since)" "$(field since_epoch)" || exit 5
  exit 0 ;;
release)
  [ $held = yes ] || exit 11
  [ $ours = yes ] || { cat "$L"; exit 10; }
  rm -f "$L" && [ ! -e "$L" ] || exit 5
  exit 0 ;;
esac
exit 2
'

# ask <step> [holder session purpose]: runs REMOTE on the lamp; its output on
# stdout, its exit code returned.
ask() {
  local step=$1 holder=${2:-} session=${3:-} purpose=${4:-}
  local b64 rc=0
  b64=$(printf %s "$purpose" | base64 | tr -d '\n')
  # shellcheck disable=SC2086 # options are meant to split
  printf %s "$REMOTE" | $SSH ${LAMP_SSH_OPTS:-} "${LAMP_USER:-orangepi}@$LAMP.local" \
    bash -s -- "$step" "${holder:-_}" "${session:-_}" "$(hostname -s)" "${b64:-_}" || rc=$?
  return "$rc"
}

unreachable() {
  if [[ $1 == 5 ]]; then echo "$LAMP could not write or remove its lock file; nothing changed" >&2
  elif [[ $1 == 6 ]]; then echo "$LAMP could not replace its expired lock; the lock is unchanged, and its log records the failed attempt" >&2
  else echo "cannot ask $LAMP about its lock (exit $1)" >&2; fi
  return 3
}

status() {
  local out rc=0
  out=$(ask status) || rc=$?
  case $rc in
    0) echo "$LAMP is free" ;;
    10) echo "$LAMP is held:"; printf '%s\n' "$out" | sed 's/^/  /'; return 1 ;;
    *) unreachable "$rc" ;;
  esac
}

# acquire holder session purpose once; returns 0 taken, 12 already ours,
# 1 held (once), 3 unreachable.
acquire() {
  local holder=$1 session=$2 purpose=$3 once=$4 out rc
  plain "$holder"; plain "$session"
  while :; do
    ATTEMPT=$(date +%s) # run's lease counts from before the take that held
    rc=0; out=$(ask take "$holder" "$session" "$purpose") || rc=$?
    case $rc in
      0) [[ -z $out ]] || printf '%s\n' "$out"; echo "$LAMP acquired by $holder"; return 0 ;;
      12) echo "$LAMP already held by us"; return 12 ;;
      10)
        if [[ $once == yes ]]; then echo "$LAMP is held:"; printf '%s\n' "$out" | sed 's/^/  /'; return 1; fi
        echo "$(date '+%H:%M:%S') waiting for $LAMP:"; printf '%s\n' "$out" | sed 's/^/  /'
        sleep "$POLL" ;;
      *) unreachable "$rc"; return 3 ;;
    esac
  done
}

renew() {
  local rc=0
  plain "$1"; plain "$2"
  ask renew "$1" "$2" >/dev/null || rc=$?
  case $rc in
    0) return 0 ;;
    10|11) echo "$LAMP: our lock is gone (taken or released)" >&2; return 1 ;;
    *) unreachable "$rc" ;;
  esac
}

release() {
  local out rc=0
  plain "$1"; plain "$2"
  out=$(ask release "$1" "$2") || rc=$?
  case $rc in
    0) echo "$LAMP released" ;;
    11) echo "$LAMP was not held" ;;
    10) echo "refusing: $LAMP is not held by $1 (session $2):" >&2; printf '%s\n' "$out" | sed 's/^/  /' >&2; return 1 ;;
    *) unreachable "$rc" ;;
  esac
}

case "$CMD" in
  status) [[ $# -eq 0 ]] || usage; status ;;
  try|acquire)
    [[ $# -ge 3 ]] || usage
    h=$1 s=$2; shift 2
    rc=0; acquire "$h" "$s" "$*" "$([[ $CMD == try ]] && echo yes || echo no)" || rc=$?
    [[ $rc -eq 12 ]] && rc=0
    exit "$rc" ;;
  renew) [[ $# -eq 2 ]] || usage; renew "$1" "$2" ;;
  release) [[ $# -eq 2 ]] || usage; release "$1" "$2" ;;
  run)
    [[ $# -ge 5 && "$4" == -- ]] || usage
    h=$1 s=$2 purpose=$3; shift 4
    rc=0; acquire "$h" "$s" "$purpose" no || rc=$?
    case $rc in
      0) ;;
      # Held already by this holder and session (a run inside a run, or an
      # explicit acquire): whoever took it renews and releases it, not us.
      12) exec "$@" ;;
      *) exit "$rc" ;;
    esac
    taken=$ATTEMPT
    # The command runs in a process group of its own (job control), so
    # stopping it stops every process it started, not only the first.
    # From a terminal, a background group that reads it is stopped by
    # SIGTTIN and would hold the lamp while renewing: the command reads
    # /dev/null there. Interactive work on the lamp takes acquire/release.
    set -m
    if [[ -t 0 ]]; then "$@" </dev/null & else "$@" & fi
    cmd=$!
    set +m
    # stop_group: TERM the command's group, wait up to 10 s, then KILL.
    stop_group() {
      kill -TERM -- "-$cmd" 2>/dev/null || return 0
      local i
      for i in 1 2 3 4 5 6 7 8 9 10; do
        kill -0 -- "-$cmd" 2>/dev/null || return 0
        sleep 1
      done
      kill -KILL -- "-$cmd" 2>/dev/null || true
    }
    # kill_tree: stop a renewal call and every process under it (its ssh).
    kill_tree() {
      local child
      kill -STOP "$1" 2>/dev/null || return 0
      for child in $(pgrep -P "$1"); do kill_tree "$child"; done
      kill -KILL "$1" 2>/dev/null || true
    }
    # Renew while the command runs. The command is stopped when the lock is
    # lost, or when no renewal has been confirmed for STOP seconds (counted
    # from the start of the last confirmed call, so a slow or hung call cannot
    # carry the work past the lamp's lease).
    (
      call=
      # This subshell's own PID. Bash 3.2 (macOS) has no BASHPID, and $$ is
      # run's shell, so it is read from a child that replaces $( )'s fork.
      self=${BASHPID:-$(exec sh -c 'echo "$PPID"')}
      # Stopped by run's exit: stop every child, with what it started. Bash runs
      # this between commands, so a renewal call forked just before its PID
      # was kept in $call is already a child here, and is stopped too.
      trap 'for c in $(pgrep -P "$self"); do kill_tree "$c"; done; wait || true; exit 0' TERM
      confirmed=$taken next=$((taken + RENEW))
      # The command is stopped from here, TERM then KILL, even while run's
      # own shell is still waiting for it.
      lost() { echo "$LAMP: $1, stopping the command" >&2; stop_group; exit 1; }
      while sleep 1; do
        now=$(date +%s)
        ((now < confirmed + STOP)) || lost "no renewal confirmed for ${STOP}s"
        ((now >= next)) || continue
        renew "$h" "$s" &
        # lamp-lock-test.sh only: hold after the fork, before its PID is kept.
        [[ -z ${LAMP_LOCK_TEST_FORKED:-} ]] || { : >"$LAMP_LOCK_TEST_FORKED"; while :; do sleep 0.1; done; }
        call=$!
        while kill -0 "$call" 2>/dev/null; do
          if (($(date +%s) >= confirmed + STOP)); then
            kill_tree "$call"
            wait "$call" 2>/dev/null || true
            call=
            lost "no renewal confirmed for ${STOP}s"
          fi
          sleep 1
        done
        rc=0; wait "$call" || rc=$?
        call=
        case $rc in
          0) confirmed=$now next=$((now + RENEW)) ;;
          3) next=$((now + (RENEW < 30 ? RENEW : 30))) ;; # cannot ask: try again soon
          *) lost "lock lost" ;;
        esac
      done
    ) &
    renewer=$!
    # stop_renewer: TERM the renewer, give it 15 s to stop its calls, then
    # stop it and everything under it, so run's exit never waits unbounded.
    stop_renewer() {
      kill -TERM "$renewer" 2>/dev/null || return 0
      local i state
      for i in $(seq 15); do
        state=$(ps -o stat= -p "$renewer" 2>/dev/null) || break
        [[ $state == Z* ]] && break
        sleep 1
      done
      kill_tree "$renewer"
    }
    # (A renewer killed before its own trap is set makes wait return 143;
    # under set -e that must not end the trap before the release.)
    trap 'stop_renewer; wait "$renewer" 2>/dev/null || true; stop_group; release "$h" "$s" >&2 || true' EXIT
    trap 'exit 130' INT TERM
    rc=0; wait "$cmd" || rc=$?
    exit "$rc" ;;
  *) usage ;;
esac
