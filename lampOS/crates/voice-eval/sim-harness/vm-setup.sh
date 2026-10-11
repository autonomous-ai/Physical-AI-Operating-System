#!/bin/sh
# One-time host setup of the Colima VM "lampos" with kernel-simulated Lamp
# hardware: ALSA snd-aloop (audio) and gpio-sim (privacy line). The VM's own
# virtio sound device is blacklisted so nothing can reach host speakers.
# Verified state: Ubuntu 24.04.1, kernel 6.8.0-146-generic with
# linux-modules-extra-6.8.0-146-generic, modules snd_aloop and gpio_sim.
set -e
colima start lampos --cpu 6 --memory 12 --disk 60 --vm-type vz --mount-type virtiofs
colima ssh -p lampos -- sh -c '
  set -e
  sudo DEBIAN_FRONTEND=noninteractive apt-get update -qq
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq linux-image-generic
  printf "snd-aloop\ngpio-sim\n" | sudo tee /etc/modules-load.d/lampos-sim.conf >/dev/null
  echo "blacklist virtio_snd" | sudo tee /etc/modprobe.d/lampos-no-host-audio.conf >/dev/null'
colima restart lampos
colima ssh -p lampos -- sh -c '
  set -e
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq linux-modules-extra-$(uname -r)
  sudo modprobe -r virtio_snd 2>/dev/null || true
  sudo modprobe snd-aloop
  sudo modprobe gpio-sim'
docker --context colima-lampos build -t lampos-dev:1.96 "$(dirname "$0")"
"$(dirname "$0")/vm-boot.sh"
