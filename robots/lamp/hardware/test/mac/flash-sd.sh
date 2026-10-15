#!/usr/bin/env bash
# Flash a lamp image to SD cards on the Mac, one card after another.
#
#   sudo bash mac/flash-sd.sh                      # newest ~/Downloads/golden-*.img.gz
#   sudo bash mac/flash-sd.sh path/to/image.img.gz # .img, .img.gz or .img.xz
#
# Only external, physical, removable disks <= 256 GB are offered; you confirm each card by typing
# its disk number. When macOS says "The disk you attached was not readable" click Ignore.
set -uo pipefail

[ "$(id -u)" -eq 0 ] || exec sudo bash "$0" "$@"

USER_HOME=$(eval echo "~${SUDO_USER:-$USER}")
IMG="${1:-$(ls -t "$USER_HOME"/Downloads/golden-*.img* 2>/dev/null | head -1)}"
[ -f "$IMG" ] || { echo "no image found (give the path as argument)"; exit 1; }
case "$IMG" in
    *.gz) CAT=(gunzip -c) ;;
    *.xz) CAT=(xz -dc) ;;
    *)    CAT=(cat) ;;
esac
echo "image: $IMG ($(du -h "$IMG" | cut -f1))"

list_cards() {
    for d in $(diskutil list external physical | awk '/^\/dev\/disk/{print $1}'); do
        info=$(diskutil info "$d")
        grep -q "Removable Media: *Removable" <<<"$info" || grep -q "Protocol: *\(USB\|Secure Digital\)" <<<"$info" || continue
        bytes=$(awk -F'[()]' '/Disk Size/{split($2,a," "); print a[1]; exit}' <<<"$info")
        [ "${bytes:-0}" -gt 0 ] && [ "$bytes" -le 256000000000 ] || continue
        name=$(awk -F': *' '/Media Name/{print $2; exit}' <<<"$info")
        printf '  %s  %6.1f GB  %s\n' "${d#/dev/disk}" "$(echo "$bytes/1000000000" | bc -l)" "$name"
    done
}

while true; do
    echo
    echo "== insert a card, then press Enter (q = quit)"
    read -r a; [ "$a" = q ] && break
    sleep 2
    cards=$(list_cards)
    if [ -z "$cards" ]; then echo "no SD card found"; continue; fi
    echo "cards found (disk number, size, name):"; echo "$cards"
    read -rp "disk number to ERASE and flash (Enter = cancel): " N
    [ -n "$N" ] || continue
    if ! awk '{print $1}' <<<"$cards" | grep -qx "$N"; then echo "disk$N is not in the list"; continue; fi

    diskutil unmountDisk "/dev/disk$N" || continue
    t0=$(date +%s)
    "${CAT[@]}" "$IMG" | dd of="/dev/rdisk$N" bs=4m status=progress
    rc=("${PIPESTATUS[@]}")
    sync
    if [ "${rc[0]}" -ne 0 ] || [ "${rc[1]}" -ne 0 ]; then
        echo "!! FLASH FAILED (decompress rc=${rc[0]}, dd rc=${rc[1]}) - do not use this card"
        diskutil eject "/dev/disk$N" >/dev/null 2>&1
        continue
    fi
    echo "written in $(( $(date +%s) - t0 )) s"
    diskutil list "/dev/disk$N" | sed -n '3,$p'
    diskutil eject "/dev/disk$N" && echo "OK - disk$N ejected, remove the card"
done
