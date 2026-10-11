#!/bin/sh
# Usage: run-sim.sh REF 'COMMANDS'
# Runs COMMANDS in lampOS of git REF (exact committed source, no working tree)
# inside the privileged dev container on the lampos VM, with the simulated
# hardware and $LAMPOS_SIM (default ~/.lampos-sim) mounted at /sim.
set -e
REF=$1; shift
SIM=${LAMPOS_SIM:-$HOME/.lampos-sim}
cd "$(git rev-parse --show-toplevel)"
git archive "$REF" lampOS | docker --context colima-lampos run --rm -i --privileged \
  -v lampos-cargo:/usr/local/cargo/registry -v lampos-target-sim:/target \
  -v "$SIM":/sim -e CARGO_TARGET_DIR=/target lampos-dev:1.96 \
  sh -c 'D=/src/sim && mkdir -p $D && tar -x -C $D && cd $D/lampOS && cp crates/voice-eval/sim-harness/asound.conf /etc/asound.conf && '"$*"
