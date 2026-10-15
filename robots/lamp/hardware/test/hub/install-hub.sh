#!/usr/bin/env bash
# Set up an Orange Pi 4 Pro as the lamp test hub (done on opidev1 / lamp-0c89).
# Run ON THE HUB from a checkout of this repo:   bash hub/install-hub.sh
#
#  - copies the lamp-side test files to ~ (lamp-remote-test pushes them to each lamp from there)
#  - installs /usr/local/bin/lamp-remote-test
#  - installs sshpass (log in to lamps) and chafa (camera preview)
#  - gives end0 a fixed 192.168.1.10/24 when no DHCP answers (direct cable to the Mac)
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"

echo "== lamp test files -> $HOME"
cp "$REPO"/lamp/{lamp_fulltest.py,lamp-test,servo_test.py,mpr121_opi_test.py,mpr121_tune.py,camera_capture_test.sh} "$HOME/"
if [ -d "$REPO/lamp/lamp_voice" ]; then          # spoken step prompts, made by mac/make_voice.sh (not in git)
    rm -rf "$HOME/lamp_voice" && cp -r "$REPO/lamp/lamp_voice" "$HOME/"
else
    echo "   (no lamp/lamp_voice/ - run mac/make_voice.sh on a Mac and copy it here for spoken prompts)"
fi
chmod +x "$HOME"/{lamp_fulltest.py,lamp-test,camera_capture_test.sh}

echo "== /usr/local/bin/lamp-remote-test"
sudo install -m 755 "$REPO/hub/lamp-remote-test" /usr/local/bin/lamp-remote-test
sudo install -m 755 "$REPO/hub/lamp-mpr121-stats" /usr/local/bin/lamp-mpr121-stats

echo "== dashboard: tmux session 'lamprun' (live log / runs / MPR121 stats panes, starts at boot)"
sudo install -m 755 "$REPO/hub/lamp-hub-tmux" /usr/local/bin/lamp-hub-tmux
sudo install -m 644 "$REPO/hub/lamp-dash.service" /etc/systemd/system/lamp-dash.service
sudo systemctl daemon-reload && sudo systemctl enable --now lamp-dash.service

echo "== LCD/HDMI: kmscon on tty1 shows the dash session (no fbdev console on this kernel)"
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -q --no-install-recommends kmscon
# the kmscon package makes itself the autovt login console on tty1 - that would cover the dashboard
sudo systemctl disable kmsconvt@.service || true
sudo systemctl mask kmsconvt@tty1.service
sudo install -m 644 "$REPO/hub/lamp-lcd.service" /etc/systemd/system/lamp-lcd.service
sudo systemctl daemon-reload && sudo systemctl enable --now lamp-lcd.service

echo "== push button on header pin 40 (to GND pin 39): start tests / answer questions"
sudo install -m 755 "$REPO/hub/lamp-button" /usr/local/bin/lamp-button
sudo install -m 644 "$REPO/hub/lamp-button.service" /etc/systemd/system/lamp-button.service
sudo systemctl daemon-reload && sudo systemctl enable --now lamp-button.service

echo "== packages"
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -q sshpass chafa

echo "== end0 fallback address (Mac cable)"
if ! grep -q "profile cable_static" /etc/dhcpcd.conf; then
    sudo cp -n /etc/dhcpcd.conf /etc/dhcpcd.conf.bak
    sudo tee -a /etc/dhcpcd.conf < "$REPO/hub/dhcpcd-end0.conf" >/dev/null
    sudo dhcpcd -n end0 || true
fi
ip -4 -br addr show end0 || true

echo "done. From the Mac:  lamp-test --scan   |   live view:  lamp-test dash"
