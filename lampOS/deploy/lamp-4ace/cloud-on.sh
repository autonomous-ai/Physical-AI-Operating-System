#!/bin/sh
# cloud-on.sh: Sunday's step 7 for lamp-4ace (#705, #1396), after the owner's
# setup from the app. Run from the office Mac; it needs no person at the lamp.
# It puts lamp-cloud on lamp-4ace the way a part is installed
# (/opt/lampOS/cloud/<sha256>/lamp-cloud, `current` pointing at it, the product's
# unit), turns it on with a drop-in (LAMP_CLOUD=on) and starts it. Then it prints
# lamp-cloud's own journal lines (online on the broker, offline, the backend's
# ping) with every address and URL taken out, and `systemctl show` for its
# conditions. It never prints a value from setup's account or the voice's key.
#
#   1. here: the binary is an aarch64 ELF; its sha256
#   2. the lamp: setup is done, and the four files the unit needs are there
#      (only present or missing is said); V1's os-server is not running, since
#      the unit conflicts with it and starting lamp-cloud would stop it
#   3. the lamp: binary, `current`, unit, drop-in; daemon-reload; start
#   4. the lamp, after WAIT seconds: the journal lines and the conditions
#
# Usage: cloud-on.sh [--dry-run] HOST BINARY      put it on and start it
#        cloud-on.sh [--dry-run] --undo HOST      stop it and remove the drop-in
#   HOST    the lamp's ssh name (e.g. orangepi@lamp-4ace.local)
#   BINARY  lamp-cloud for aarch64, built from first/lamp-cloud-path (#1418)
#   --dry-run prints every command and runs nothing: no ssh, no copy.
# The unit is started, not enabled: after a reboot lamp-cloud is off until
# this is run again (or the unit is enabled, the owner's call). --undo does
# not start V1's os-server again if it was stopped by hand to let this run.
# The unit is this checkout's lampOS/deploy/product/lamp-cloud.service (#1418;
# it stops before any copy if the checkout has none). sudo on
# the lamp asks for its password once (ssh -t). Any failed check stops here.
set -u

DRY=0
UNDO=0
while [ $# -gt 0 ]; do
	case "$1" in
	--dry-run) DRY=1 ;;
	--undo) UNDO=1 ;;
	--on-lamp) shift; break ;;
	-*) echo "usage: $0 [--dry-run] HOST BINARY | [--dry-run] --undo HOST" >&2; exit 2 ;;
	*) break ;;
	esac
	shift
done
WAIT=${WAIT:-20}
STAGE=/var/tmp/cloud-on
DROPIN=/etc/systemd/system/lamp-cloud.service.d/cloud-on.conf

abort() { printf '\nSTOPPED: %s\nNothing more is done.\n' "$*" >&2; exit 1; }
# run CMD: print it; run it unless --dry-run; stop on its failure.
run() {
	printf '  $ %s\n' "$*"
	[ "$DRY" = 1 ] && return 0
	sh -c "$*" || abort "this failed: $*"
}

# Address-free journal: lamp-cloud's own lines, every URL, host:port and IP out.
redact() {
	sed -E -e 's#[a-z]+://[^ ]+#<url>#g' \
		-e 's#^(lamp-cloud: online on the broker).*#\1 (address not shown)#' \
		-e 's#([0-9]{1,3}\.){3}[0-9]{1,3}(:[0-9]+)?#<address>#g' \
		-e 's#[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.[a-z]{2,}(:[0-9]+)?#<host>#g'
}

# On the lamp, as root: the steps after the copy (`--on-lamp on|undo`).
on_lamp() {
	what=$1
	if [ "$what" = undo ]; then
		echo "=== Undo: stop lamp-cloud, remove the drop-in"
		# A unit never installed has nothing to stop: not a failure here.
		run "systemctl stop lamp-cloud 2>/dev/null || true"
		run "rm -f '$DROPIN'"
		run "systemctl daemon-reload"
		run "systemctl show lamp-cloud -p ActiveState -p SubState -p UnitFileState"
		return 0
	fi
	echo "=== 2. What the unit needs (present or missing only)"
	for path in /var/lib/lampos-setup/done /etc/lampOS/setup/backend.json \
		/etc/lampOS/voice/credential /var/lib/lampos-update/state.json; do
		if [ "$DRY" = 1 ]; then echo "  checks: $path"
		elif [ -e "$path" ]; then echo "  present: $path"
		else echo "  MISSING: $path"; missing=1; fi
	done
	[ "${missing:-0}" = 1 ] && abort "the unit's conditions are not met: it would not start"
	[ "$DRY" = 1 ] || id lampos >/dev/null 2>&1 || abort "no lampos user"
	if [ "$DRY" = 0 ] && systemctl is-active --quiet os-server; then
		abort "V1's os-server is running; lamp-cloud's unit conflicts with it (one broker client id), and starting it would stop os-server. Stop os-server first, if that is meant."
	fi
	echo "=== 3. Install and start"
	sha=$(if [ "$DRY" = 1 ]; then echo '<sha256>'; else sha256sum "$STAGE/lamp-cloud" | cut -d' ' -f1; fi)
	[ -n "$sha" ] || abort "no binary at $STAGE/lamp-cloud on the lamp"
	run "install -d -m 0755 /opt/lampOS/cloud/$sha"
	run "install -m 0755 '$STAGE/lamp-cloud' /opt/lampOS/cloud/$sha/lamp-cloud"
	run "ln -sfn /opt/lampOS/cloud/$sha /opt/lampOS/cloud/current"
	run "install -m 0644 '$STAGE/lamp-cloud.service' /etc/systemd/system/lamp-cloud.service"
	run "install -d -m 0755 '$(dirname "$DROPIN")'"
	run "printf '[Service]\nEnvironment=LAMP_CLOUD=on\n' > '$DROPIN'"
	run "systemctl daemon-reload"
	started=$(date '+%Y-%m-%d %H:%M:%S')
	run "systemctl start lamp-cloud"
	echo "=== 4. After ${WAIT} s: lamp-cloud's lines (addresses taken out) and its conditions"
	[ "$DRY" = 1 ] || sleep "$WAIT"
	printf '  $ journalctl -u lamp-cloud --since "<start>" -o cat | grep "^lamp-cloud:" | <redact>\n'
	[ "$DRY" = 1 ] || journalctl -u lamp-cloud --since "$started" -o cat --no-pager | grep '^lamp-cloud:' | redact
	run "systemctl show lamp-cloud -p ActiveState -p SubState -p Result -p NRestarts -p ExecMainStatus -p ConditionResult -p ConditionTimestamp"
	echo "  Undo: cloud-on.sh --undo HOST"
}

if [ "${1:-}" = on ] || [ "${1:-}" = undo ]; then
	on_lamp "$1"
	exit
fi

HOST=${1:-}
[ -n "$HOST" ] || abort "no HOST"
SELF=$(cd "$(dirname "$0")" && pwd)/$(basename "$0")
UNIT=$(cd "$(dirname "$0")/../product" && pwd)/lamp-cloud.service
if [ "$UNDO" = 1 ]; then
	echo "=== Undo on $HOST"
	run "scp -q '$SELF' '$HOST:$STAGE.sh'"
	run "ssh -t '$HOST' 'sudo sh $STAGE.sh $( [ "$DRY" = 1 ] && echo --dry-run ) --on-lamp undo'"
	exit
fi
BIN=${2:-}
[ -n "$BIN" ] || abort "no BINARY"
echo "=== 1. The binary"
if [ "$DRY" = 0 ]; then
	[ -f "$BIN" ] || abort "no file at $BIN"
	file "$BIN" | grep -q 'ELF 64-bit.*aarch64' || abort "$BIN is not an aarch64 ELF: $(file -b "$BIN")"
	[ -f "$UNIT" ] || abort "no unit at $UNIT"
	echo "  sha256 $(sha256sum "$BIN" 2>/dev/null || shasum -a 256 "$BIN")"
fi
run "ssh '$HOST' 'mkdir -p $STAGE'"
run "scp -q '$BIN' '$HOST:$STAGE/lamp-cloud'"
run "scp -q '$UNIT' '$HOST:$STAGE/lamp-cloud.service'"
run "scp -q '$SELF' '$HOST:$STAGE.sh'"
run "ssh -t '$HOST' 'sudo sh $STAGE.sh $( [ "$DRY" = 1 ] && echo --dry-run ) --on-lamp on'"
if [ "$DRY" = 1 ]; then
	echo "=== On the lamp it then does:"
	on_lamp on
fi
