#!/usr/bin/env bash
# Set up the Mac side: launcher in ~/.local/bin, USB-LAN adapter on the cable subnet,
# SSH key login to the hub.   Run:  bash mac/install-mac.sh
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
HUB_IP=192.168.1.10
MAC_IP=192.168.1.233
PORT="USB 10/100/1000 LAN"            # networksetup -listallhardwareports

mkdir -p "$HOME/.local/bin"
install -m 755 "$REPO/mac/lamp-test" "$HOME/.local/bin/lamp-test"; ln -sf lamp-test "$HOME/.local/bin/lamp-test-remote"
echo "launcher: ~/.local/bin/lamp-test (make sure ~/.local/bin is on PATH)"

if networksetup -listallhardwareports | grep -q "$PORT"; then
    sudo networksetup -setmanual "$PORT" "$MAC_IP" 255.255.255.0
    echo "$PORT -> $MAC_IP/24 (no router)"
else
    echo "WARN: '$PORT' not found - plug in the USB ethernet adapter, then rerun"
fi

if ping -c1 -t2 "$HUB_IP" >/dev/null 2>&1; then
    sshpass -p orangepi ssh-copy-id -o StrictHostKeyChecking=accept-new -i "$HOME/.ssh/id_rsa.pub" "orangepi@$HUB_IP"
    echo "key login to orangepi@$HUB_IP installed"
else
    echo "WARN: hub $HUB_IP not answering on the cable yet - rerun once it is up"
fi
