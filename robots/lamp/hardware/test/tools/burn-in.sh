#!/usr/bin/env bash
# burn-in.sh — hardware acceptance test, run on the device before it ships.
#
# Catches the failures that look like software bugs for days: bad DRAM, a dying
# card, a thermal or power problem. Written after a unit shipped with ~50 MB of
# stuck-at-0 DRAM and spent a day masquerading as a kernel bug, a filesystem
# bug, and an agent crash in turn.
#
#   sudo ./burn-in.sh              full run  (~20-30 min, factory bench)
#   sudo ./burn-in.sh --quick      short run (~4-6 min, first boot / spot check)
#   sudo ./burn-in.sh --ram 4096 --passes 2
#
# Exit: 0 = PASS, 1 = FAIL, 2 = could not run (not root, missing tool).
# Results land in /var/lib/autonomous/burn-in.{log,json} for the fleet report.
#
# Coverage honesty: memtester can only test memory it is allowed to allocate,
# so the kernel's own pages are never covered. This is a strong screen, not a
# proof. A unit that fails here is definitely bad; one that passes is probably
# fine.

set -uo pipefail

VERSION="1.0"
RESULT_DIR="/var/lib/autonomous"
LOG="${RESULT_DIR}/burn-in.log"
JSON="${RESULT_DIR}/burn-in.json"

# Cap how many mismatch lines reach the screen and the log. A properly broken
# DIMM emits them by the million — enough to fill a RAM-backed /var/log and
# take the device down mid-test, which would destroy the evidence we came for.
MAX_FAIL_LINES=20

QUICK=0
RAM_MB=""
PASSES=""
SOAK_S=""
SKIP_STORAGE=0
SKIP_THERMAL=0
# Bad memory ends the run by default; see the fail-fast block after the RAM test.
FAIL_FAST=1
# Trip point for the soak. The SoC throttles well below this; we only want to
# catch a heatsink that was never seated or a fan that never spins.
TEMP_LIMIT_C=85

# ── Presentation ─────────────────────────────────────────────────────────────
if [ -t 1 ]; then
  C_RST=$'\033[0m'; C_DIM=$'\033[90m'; C_RED=$'\033[31m'
  C_GRN=$'\033[32m'; C_YEL=$'\033[33m'; C_BLD=$'\033[1m'
else
  C_RST=""; C_DIM=""; C_RED=""; C_GRN=""; C_YEL=""; C_BLD=""
fi

ts()   { date '+%H:%M:%S'; }
# Every line is timestamped and tee'd, so the operator watches it live and the
# same stream survives in the log for whoever reads it later.
say()  { printf '%s[%s]%s %s\n' "$C_DIM" "$(ts)" "$C_RST" "$*" | tee -a "$LOG"; }
head2(){ printf '\n%s══ %s %s\n' "$C_BLD" "$*" "$C_RST" | tee -a "$LOG"; }
ok()   { printf '%s[%s]%s   %sPASS%s  %s\n' "$C_DIM" "$(ts)" "$C_RST" "$C_GRN" "$C_RST" "$*" | tee -a "$LOG"; }
bad()  { printf '%s[%s]%s   %sFAIL%s  %s\n' "$C_DIM" "$(ts)" "$C_RST" "$C_RED" "$C_RST" "$*" | tee -a "$LOG"; }
warn() { printf '%s[%s]%s   %sWARN%s  %s\n' "$C_DIM" "$(ts)" "$C_RST" "$C_YEL" "$C_RST" "$*" | tee -a "$LOG"; }
fmt_status() {
  case "$1" in
    PASS) printf '%sPASS%s' "$C_GRN" "$C_RST" ;;
    FAIL) printf '%sFAIL%s' "$C_RED" "$C_RST" ;;
    *)    printf '%sSKIP%s' "$C_YEL" "$C_RST" ;;
  esac
}
die()  { printf '%sburn-in: %s%s\n' "$C_RED" "$*" "$C_RST" >&2; exit 2; }

usage() {
  sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
}

while [ $# -gt 0 ]; do
  case "$1" in
    --quick)        QUICK=1 ;;
    --ram)          RAM_MB="${2:-}"; shift ;;
    --passes)       PASSES="${2:-}"; shift ;;
    --soak)         SOAK_S="${2:-}"; shift ;;
    --skip-storage) SKIP_STORAGE=1 ;;
    --skip-thermal) SKIP_THERMAL=1 ;;
    --no-fail-fast) FAIL_FAST=0 ;;
    -h|--help)      usage ;;
    *)              die "unknown option: $1 (try --help)" ;;
  esac
  shift
done

[ "$(id -u)" -eq 0 ] || die "must run as root (memtester needs to lock memory)"
command -v memtester >/dev/null 2>&1 || die "memtester not installed"

mkdir -p "$RESULT_DIR" || die "cannot create $RESULT_DIR"
: > "$LOG"

# ── Sizing ───────────────────────────────────────────────────────────────────
# Default to most of what is actually free. Leaving ~1 GB keeps the running
# services alive: an OOM kill mid-test reads as a hardware fault and isn't one.
AVAIL_MB=$(awk '/MemAvailable/ {printf "%d", $2/1024}' /proc/meminfo)
if [ -z "$RAM_MB" ]; then
  if [ "$QUICK" -eq 1 ]; then
    RAM_MB=512
  else
    RAM_MB=$(( AVAIL_MB - 1024 ))
    [ "$RAM_MB" -lt 512 ] && RAM_MB=512
  fi
fi
[ -z "$PASSES" ] && { [ "$QUICK" -eq 1 ] && PASSES=1 || PASSES=1; }
[ -z "$SOAK_S" ] && { [ "$QUICK" -eq 1 ] && SOAK_S=30 || SOAK_S=120; }

# ── Header ───────────────────────────────────────────────────────────────────
DEVICE_ID=$(python3 -c "import json;print(json.load(open('/root/config/config.json')).get('device_id',''))" 2>/dev/null || true)
DEVICE_TYPE=$(grep -ho 'DEVICE_TYPE=[^ "]*' /etc/systemd/system/os-server.service /opt/hal/.env 2>/dev/null | head -1 | cut -d= -f2)
ROOT_DEV=$(findmnt -no SOURCE / 2>/dev/null)

head2 "AUTONOMOUS BURN-IN v${VERSION}"
say "host        $(hostname)"
say "device      ${DEVICE_TYPE:-unknown}  id=${DEVICE_ID:-unknown}"
say "kernel      $(uname -r)  $(uname -m)"
say "started     $(date '+%Y-%m-%d %H:%M:%S %Z')"
say "plan        RAM ${RAM_MB} MB x${PASSES} pass · soak ${SOAK_S}s · root ${ROOT_DEV}"
[ "$QUICK" -eq 1 ] && warn "quick mode — a screen, not an acceptance test"

FAIL_RAM=0; FAIL_STORAGE=0; FAIL_THERMAL=0
RAM_ERRORS=0; MAX_TEMP=0
STATUS_STORAGE="PASS"; STATUS_THERMAL="PASS"

# write_result snapshots the verdict to disk. Called as soon as memory is known
# and again at the end, so an unstable board still leaves a usable record.
write_result() {
  local verdict
  verdict=$([ $((FAIL_RAM + FAIL_STORAGE + FAIL_THERMAL)) -eq 0 ] && echo PASS || echo FAIL)
  cat > "$JSON" <<EOF
{
  "version": "${VERSION}",
  "verdict": "${verdict}",
  "quick": $([ "$QUICK" -eq 1 ] && echo true || echo false),
  "finished_at": "$(date -Is)",
  "device_id": "${DEVICE_ID}",
  "device_type": "${DEVICE_TYPE}",
  "kernel": "$(uname -r)",
  "memory": { "status": "$([ $FAIL_RAM -eq 0 ] && echo PASS || echo FAIL)", "tested_mb": ${RAM_MB}, "passes": ${PASSES}, "mismatches": ${RAM_ERRORS} },
  "storage": { "status": "${STATUS_STORAGE}" },
  "thermal": { "status": "${STATUS_THERMAL}", "peak_c": ${MAX_TEMP} }
}
EOF
  sync 2>/dev/null || true
}

# ── 1. Memory ────────────────────────────────────────────────────────────────
# The headline test. Everything else here is quick by comparison.
head2 "1/3  MEMORY"
say "memtester ${RAM_MB}M ${PASSES} — each line below appears as that subtest finishes"
say "this is the slow one; expect roughly 8 min per GB per pass"

FAILCOUNT_FILE=$(mktemp)
echo 0 > "$FAILCOUNT_FILE"

# Heartbeat: memtester emits a whole subtest per line and some take minutes, so
# without this the operator cannot tell a slow test from a hung one.
( while true; do sleep 60; printf '%s[%s]%s   %s…still running%s\n' \
    "$C_DIM" "$(ts)" "$C_RST" "$C_DIM" "$C_RST"; done ) &
HEARTBEAT=$!
trap 'kill "$HEARTBEAT" 2>/dev/null; rm -f "$FAILCOUNT_FILE"' EXIT INT TERM

set -o pipefail
# memtester animates progress in place: a \|/- spinner for some subtests and a
# "setting N/testing N" counter for others, both drawn with backspaces. Dropping
# the backspaces alone would unroll every frame onto one multi-megabyte line, so
# the spinner glyphs and the counter frames are collapsed away too, leaving one
# tidy line per subtest. FAILURE lines are untouched — they are the payload.
stdbuf -oL memtester "${RAM_MB}M" "$PASSES" 2>&1 \
  | stdbuf -oL tr -d '\010' \
  | stdbuf -oL sed -E 's@[\|/\\-]{3,}@@g; s/( *(sett|test)ing +[0-9]+)+/ /g; s/ {2,}/ /g; s/ +$//' \
  | stdbuf -oL awk -v max="$MAX_FAIL_LINES" -v cf="$FAILCOUNT_FILE" '
      /FAILURE/ {
        n++
        if (n <= max) { print "      " $0; fflush() }
        else if (n == max + 1) { print "      … further mismatches suppressed (still counting)"; fflush() }
        next
      }
      /^[[:space:]]*$/ { next }
      { print; fflush() }
      END { print n+0 > cf }
    ' | tee -a "$LOG"
MEMTESTER_RC=${PIPESTATUS[0]}
kill "$HEARTBEAT" 2>/dev/null
RAM_ERRORS=$(cat "$FAILCOUNT_FILE" 2>/dev/null || echo 0)
[ -z "$RAM_ERRORS" ] && RAM_ERRORS=0

if [ "$MEMTESTER_RC" -ne 0 ] || [ "$RAM_ERRORS" -gt 0 ]; then
  FAIL_RAM=1
  bad "memory: ${RAM_ERRORS} mismatches (memtester exit ${MEMTESTER_RC})"
  bad "this board has bad DRAM — it cannot be fixed in software. Reject the unit."
else
  ok "memory: ${RAM_MB} MB x${PASSES} clean"
fi

# Persist the verdict the moment it is known. A board that just failed a memory
# test can hang or panic at any point after, and the result has to survive that.
write_result

# Stop here on bad memory. The unit is already rejected, so later stages add no
# information — and they routinely take the machine down mid-test, which costs
# the log we came for. --no-fail-fast when you deliberately want the full sweep.
if [ "$FAIL_RAM" -eq 1 ] && [ "$FAIL_FAST" -eq 1 ]; then
  warn "skipping storage and thermal — verdict already decided by memory"
  SKIP_STORAGE=1
  SKIP_THERMAL=1
fi

# ── 2. Storage ───────────────────────────────────────────────────────────────
head2 "2/3  STORAGE"
if [ "$SKIP_STORAGE" -eq 1 ]; then
  warn "not run"; STATUS_STORAGE="SKIP"
else
  FS_STATE=$(dumpe2fs -h "$ROOT_DEV" 2>/dev/null | awk -F': *' '/Filesystem state/{print $2}')
  LIFETIME=$(dumpe2fs -h "$ROOT_DEV" 2>/dev/null | awk -F': *' '/Lifetime writes/{print $2}')
  say "filesystem  state=${FS_STATE:-unknown}  lifetime writes=${LIFETIME:-unknown}"

  case "$FS_STATE" in
    clean) ok "filesystem clean" ;;
    "")    warn "could not read filesystem state" ;;
    *)     STATUS_STORAGE="FAIL"; FAIL_STORAGE=1; bad "filesystem state '${FS_STATE}' — run fsck before shipping" ;;
  esac

  # Write, flush, drop caches, read back. Dropping the cache is the point: a
  # read that comes from RAM proves nothing about the card.
  SCRATCH="${RESULT_DIR}/.burnin-scratch"
  say "write/verify 256 MB to ${ROOT_DEV} (cache dropped between)"
  if dd if=/dev/urandom of="$SCRATCH" bs=1M count=256 conv=fsync status=none 2>/dev/null; then
    W_SUM=$(md5sum "$SCRATCH" | cut -d' ' -f1)
    sync; echo 3 > /proc/sys/vm/drop_caches 2>/dev/null || true
    R_SUM=$(md5sum "$SCRATCH" | cut -d' ' -f1)
    rm -f "$SCRATCH"
    if [ "$W_SUM" = "$R_SUM" ]; then
      ok "storage read-back matches (${W_SUM:0:12}…)"
    else
      FAIL_STORAGE=1
      bad "storage read-back MISMATCH — card is corrupting data"
    fi
  else
    STATUS_STORAGE="FAIL"; FAIL_STORAGE=1; bad "could not write scratch file — disk full or read-only?"
  fi

  DISK_PCT=$(df --output=pcent / | tail -1 | tr -dc '0-9')
  [ "${DISK_PCT:-0}" -gt 90 ] && { warn "root filesystem ${DISK_PCT}% full"; } || say "root usage ${DISK_PCT}%"
fi

# ── 3. Thermal + load ────────────────────────────────────────────────────────
head2 "3/3  THERMAL SOAK"
if [ "$SKIP_THERMAL" -eq 1 ]; then
  warn "not run"; STATUS_THERMAL="SKIP"
else
  ZONE=$(ls /sys/class/thermal/thermal_zone*/temp 2>/dev/null | head -1)
  if [ -z "$ZONE" ]; then
    warn "no thermal zone exposed — cannot measure"; STATUS_THERMAL="SKIP"
  else
    CORES=$(nproc)
    say "loading ${CORES} cores for ${SOAK_S}s, sampling temperature every 10s"
    if command -v stress >/dev/null 2>&1; then
      stress --cpu "$CORES" --timeout "${SOAK_S}s" >/dev/null 2>&1 &
      LOADPID=$!
    else
      warn "stress not installed — measuring idle temperature only"
      LOADPID=""
    fi
    ELAPSED=0
    while [ "$ELAPSED" -lt "$SOAK_S" ]; do
      sleep 10; ELAPSED=$((ELAPSED + 10))
      T=$(awk '{printf "%d", $1/1000}' "$ZONE" 2>/dev/null || echo 0)
      [ "$T" -gt "$MAX_TEMP" ] && MAX_TEMP=$T
      say "  ${ELAPSED}s  ${T}°C"
    done
    [ -n "$LOADPID" ] && wait "$LOADPID" 2>/dev/null

    if [ "$MAX_TEMP" -ge "$TEMP_LIMIT_C" ]; then
      STATUS_THERMAL="FAIL"; FAIL_THERMAL=1
      bad "peak ${MAX_TEMP}°C ≥ ${TEMP_LIMIT_C}°C limit — check heatsink seating"
    elif [ "$MAX_TEMP" -eq 0 ]; then
      warn "temperature never read"
    else
      ok "peak ${MAX_TEMP}°C under load"
    fi
  fi
fi

# ── Verdict ──────────────────────────────────────────────────────────────────
TOTAL_FAIL=$((FAIL_RAM + FAIL_STORAGE + FAIL_THERMAL))
head2 "RESULT"
printf '  %-10s %s\n' "memory"  "$([ $FAIL_RAM     -eq 0 ] && echo "${C_GRN}PASS${C_RST}" || echo "${C_RED}FAIL${C_RST}  ${RAM_ERRORS} mismatches")" | tee -a "$LOG"
printf '  %-10s %s\n' "storage" "$(fmt_status "$STATUS_STORAGE")" | tee -a "$LOG"
printf '  %-10s %s\n' "thermal" "$(fmt_status "$STATUS_THERMAL")$([ "$STATUS_THERMAL" = "FAIL" ] && echo "  peak ${MAX_TEMP}°C")" | tee -a "$LOG"

write_result

echo | tee -a "$LOG"
if [ "$TOTAL_FAIL" -eq 0 ]; then
  printf '%s  ██  BURN-IN PASS  ██%s\n' "$C_GRN$C_BLD" "$C_RST" | tee -a "$LOG"
  [ "$QUICK" -eq 1 ] && say "quick mode: a full run is still required before shipping"
  say "log ${LOG} · result ${JSON}"
  exit 0
else
  printf '%s  ██  BURN-IN FAIL  ██%s\n' "$C_RED$C_BLD" "$C_RST" | tee -a "$LOG"
  [ "$FAIL_RAM" -eq 1 ] && say "DO NOT SHIP. Bad DRAM surfaces later as kernel panics, filesystem"
  [ "$FAIL_RAM" -eq 1 ] && say "corruption and random process crashes that look like software bugs."
  say "log ${LOG} · result ${JSON}"
  exit 1
fi
