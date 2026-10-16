#!/usr/bin/env python3
"""Button / switch test for OrangePi 4 Pro (sun60i-a733).

Tests momentary buttons on physical pins 35 and 37, and a static (latching)
switch on physical pin 11.

Pin map for this board, taken from `gpio readall`:
    physical 11 -> GPIO 361 (PL9)    <- static switch
    physical 35 -> GPIO  99 (PD3)    <- button
    physical 37 -> GPIO 100 (PD4)    <- button

Pins 11 and 37 boot muxed to an alternate function (ALT14), not GPIO, so the
mode reset this script performs is required before they read correctly.

Wiring assumed (the common case): one leg of each button/switch to the pin,
the other leg to GND, with the internal pull-up enabled. That makes a pin read
0 when pressed/closed and 1 when open - i.e. active-low, the default here.
If your hardware pulls the pin HIGH instead, pass --active-high --pull down.

Usage:
    sudo ./gpio_buttons_opi_test.py              # watch all three, print events
    sudo ./gpio_buttons_opi_test.py --monitor    # live state table instead
    sudo ./gpio_buttons_opi_test.py --active-high --pull down
    sudo ./gpio_buttons_opi_test.py --buttons 35,37 --switch 11
"""

import argparse
import os
import subprocess
import sys
import time

# physical pin -> kernel GPIO number (OrangePi 4 Pro, sun60i-a733)
PIN_TO_GPIO = {
    3: 35, 5: 34, 7: 356, 8: 358, 10: 359,
    11: 361, 12: 360, 13: 364, 15: 329, 16: 357, 18: 354,
    19: 130, 21: 131, 22: 119, 23: 129, 24: 140, 26: 132,
    27: 37, 28: 36, 29: 96, 31: 97, 32: 101, 33: 98,
    35: 99, 36: 102, 37: 100, 38: 103, 40: 365,
}

SYSFS = "/sys/class/gpio"


class Pin:
    """A GPIO input read through sysfs, configured via wiringOP."""

    def __init__(self, phys, label, pull="up", active_low=True):
        if phys not in PIN_TO_GPIO:
            raise ValueError("physical pin %d is not a GPIO on this board" % phys)
        self.phys = phys
        self.gpio = PIN_TO_GPIO[phys]
        self.label = label
        self.active_low = active_low
        self.pull = pull
        self._fh = None
        self._exported_by_us = False
        # debounce state
        self.stable = None
        self._pending = None
        self._pending_since = 0.0

    # -- setup -------------------------------------------------------------

    def configure(self):
        self._set_mode_and_pull()
        self._export()
        self._open()

    def _set_mode_and_pull(self):
        """Use wiringOP to set the pin mux to input and apply the bias.

        sysfs cannot configure pull-up/pull-down, and on this board several
        header pins come up muxed to an alternate function (pin 11 boots as
        ALT14), so this step is what actually turns them into usable inputs.
        """
        if not _have_gpio_tool():
            print("warning: wiringOP 'gpio' not found - cannot set mode/pull; "
                  "relying on the pin's current configuration", file=sys.stderr)
            return
        for args in (["mode", str(self.phys), "in"],
                     ["mode", str(self.phys), self.pull]):
            if self.pull == "none" and args[-1] == "none":
                continue
            try:
                subprocess.run(["gpio", "-1"] + args, check=True,
                               capture_output=True, timeout=5)
            except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as exc:
                print("warning: gpio -1 %s failed: %s" % (" ".join(args), exc),
                      file=sys.stderr)

    def _export(self):
        path = os.path.join(SYSFS, "gpio%d" % self.gpio)
        if not os.path.exists(path):
            with open(os.path.join(SYSFS, "export"), "w") as fh:
                fh.write(str(self.gpio))
            self._exported_by_us = True
            # udev needs a moment to chmod the new node
            for _ in range(50):
                if os.path.exists(os.path.join(path, "value")):
                    break
                time.sleep(0.01)
        with open(os.path.join(path, "direction"), "w") as fh:
            fh.write("in")

    def _open(self):
        self._fh = open(os.path.join(SYSFS, "gpio%d" % self.gpio, "value"), "rb")

    def close(self):
        if self._fh:
            self._fh.close()
            self._fh = None
        if self._exported_by_us:
            try:
                with open(os.path.join(SYSFS, "unexport"), "w") as fh:
                    fh.write(str(self.gpio))
            except OSError:
                pass

    # -- reading -----------------------------------------------------------

    def raw(self):
        self._fh.seek(0)
        return 1 if self._fh.read().strip() == b"1" else 0

    def active(self):
        """True when the button is pressed / the switch is closed."""
        v = self.raw()
        return v == 0 if self.active_low else v == 1

    def poll(self, debounce_s):
        """Return the debounced state, or None if it has not changed."""
        now = time.monotonic()
        cur = self.active()
        if self.stable is None:
            self.stable = cur
            return cur
        if cur != self.stable:
            if self._pending != cur:
                self._pending = cur
                self._pending_since = now
            elif now - self._pending_since >= debounce_s:
                self.stable = cur
                self._pending = None
                return cur
        else:
            self._pending = None
        return None


def _have_gpio_tool():
    for d in os.environ.get("PATH", "").split(os.pathsep):
        if os.access(os.path.join(d, "gpio"), os.X_OK):
            return True
    return False


def parse_pins(spec):
    if not spec:
        return []
    return [int(p) for p in spec.replace(",", " ").split()]


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--buttons", default="35,37",
                    help="physical pins with momentary buttons (default: 35,37)")
    ap.add_argument("--switch", default="11",
                    help="physical pins with static/latching switches (default: 11)")
    ap.add_argument("--pull", choices=["up", "down", "none"], default="up",
                    help="internal bias (default: up, for switches wired to GND)")
    ap.add_argument("--active-high", action="store_true",
                    help="pin reads HIGH when pressed (default: active-low)")
    ap.add_argument("--debounce", type=float, default=0.03,
                    help="debounce time in seconds (default: 0.03)")
    ap.add_argument("--interval", type=float, default=0.005,
                    help="poll interval in seconds (default: 0.005)")
    ap.add_argument("--monitor", action="store_true",
                    help="show a live state table instead of printing events")
    args = ap.parse_args()

    active_low = not args.active_high
    pins = []
    for p in parse_pins(args.buttons):
        pins.append(Pin(p, "button", args.pull, active_low))
    for p in parse_pins(args.switch):
        pins.append(Pin(p, "switch", args.pull, active_low))
    if not pins:
        print("no pins requested", file=sys.stderr)
        return 1

    try:
        for pin in pins:
            pin.configure()
    except PermissionError:
        print("Permission denied on /sys/class/gpio - run with sudo.", file=sys.stderr)
        return 1
    except OSError as exc:
        print("GPIO setup failed: %s" % exc, file=sys.stderr)
        return 1

    print("pin  gpio  role    initial", flush=True)
    for pin in pins:
        # Seed the debounce state so the first poll does not report the
        # starting position as a fresh event.
        pin.stable = pin.active()
        state = "CLOSED/PRESSED" if pin.stable else "OPEN/RELEASED"
        print("%-4d %-5d %-7s %s" % (pin.phys, pin.gpio, pin.label, state),
              flush=True)
    print("\nbias=%s, active-%s, debounce=%dms" %
          (args.pull, "low" if active_low else "high", args.debounce * 1000))
    print("Press the buttons / flip the switch. Ctrl-C to stop.\n", flush=True)

    try:
        if args.monitor:
            _run_monitor(pins, args)
        else:
            _run_events(pins, args)
    except KeyboardInterrupt:
        print("\nstopped")
    finally:
        for pin in pins:
            pin.close()
    return 0


def _run_events(pins, args):
    while True:
        for pin in pins:
            changed = pin.poll(args.debounce)
            if changed is None:
                continue
            ts = time.strftime("%H:%M:%S")
            if pin.label == "switch":
                word = "ON (closed)" if changed else "OFF (open)"
                print("%s  pin %-2d switch -> %s" % (ts, pin.phys, word), flush=True)
            else:
                word = "pressed" if changed else "released"
                print("%s  pin %-2d %s" % (ts, pin.phys, word), flush=True)
        time.sleep(args.interval)


def _run_monitor(pins, args):
    header = "  ".join("p%-2d" % p.phys for p in pins)
    print(header)
    while True:
        for pin in pins:
            pin.poll(args.debounce)
        row = "  ".join(" %s " % ("1" if p.stable else "0") for p in pins)
        print("\r" + row, end="", flush=True)
        time.sleep(0.1)


if __name__ == "__main__":
    sys.exit(main())
