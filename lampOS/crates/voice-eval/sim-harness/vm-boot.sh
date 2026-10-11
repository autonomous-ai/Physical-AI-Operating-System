#!/bin/sh
# After every VM boot: create the simulated privacy GPIO chip (16 lines; the
# coordinator reads line 9 of /dev/gpiochip1; pull-up = privacy open).
set -e
colima ssh -p lampos -- sh -c '
  set -e
  C=/sys/kernel/config/gpio-sim
  [ -d $C/lamp ] || { sudo mkdir -p $C/lamp/bank0; echo 16 | sudo tee $C/lamp/bank0/num_lines >/dev/null; echo 1 | sudo tee $C/lamp/live >/dev/null; }
  cat /proc/asound/cards; ls -la /dev/gpiochip1
  cat /sys/devices/platform/$(cat $C/lamp/dev_name)/$(cat $C/lamp/bank0/chip_name)/sim_gpio9/pull'
