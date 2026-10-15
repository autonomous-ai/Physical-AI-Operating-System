#!/usr/bin/env python3
"""
TTP223 touch-pad test for the OrangePi 4 Pro (sun60iw2) lamp - 4 pads.

Mirrors os/hal/drivers/ttp223.py:
  - lgpio alert callbacks (NOT polling) on gpiochip0
  - SET_PULL_UP, BOTH_EDGES
  - pads rest HIGH; a touch pulses LOW (TTP223 FastMode)
  - 0.5s settle window to drop the startup edge lgpio emits on claim

Pads, by 40-pin header position (gpiochip0 has base 0, so the lgpio line
number is the kernel GPIO number shown by `gpio readall`):

    header pin  GPIO line  pad
    27          37  (PB5)  T1
    29          96  (PD0)  T2
    31          97  (PD1)  T3
    33          98  (PD2)  T4

Run:   python3 ttp223_test.py                    # 20s window, 4 pads above
       python3 ttp223_test.py --seconds 60
       python3 ttp223_test.py --pins 29,33,37     # other header pins
Needs: lgpio (pip install lgpio) and RW on /dev/gpiochip0.
"""
import argparse, sys, threading, time
import lgpio

CHIP = 0
SETTLE_S = 0.5   # ignore edges right after claim (lgpio reports resting level as an edge)

# physical header pin -> gpiochip0 line, from `gpio readall` on the 4 Pro
PIN_TO_LINE = {
    3: 35, 5: 34, 7: 356, 8: 358, 10: 359, 11: 361, 12: 360, 13: 364,
    15: 329, 16: 357, 18: 354, 19: 130, 21: 131, 22: 119, 23: 129,
    24: 140, 26: 132, 27: 37, 28: 36, 29: 96, 31: 97, 32: 101, 33: 98,
    35: 99, 36: 102, 37: 100, 38: 103, 40: 365,
}
DEFAULT_PINS = [27, 29, 31, 33]


def parse_pins(spec):
    pins = []
    for tok in spec.replace(",", " ").split():
        p = int(tok)
        if p not in PIN_TO_LINE:
            sys.exit(f"header pin {p} is not a GPIO on this board")
        pins.append(p)
    return pins


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seconds", type=int, default=20)
    ap.add_argument("--pins", default=",".join(str(p) for p in DEFAULT_PINS),
                    help="header pins with pads, in T1..Tn order (default: 27,29,31,33)")
    args = ap.parse_args()

    pins = parse_pins(args.pins)
    lines = [PIN_TO_LINE[p] for p in pins]
    names = {PIN_TO_LINE[p]: f"T{i + 1}(pin{p})" for i, p in enumerate(pins)}

    counts = {l: 0 for l in lines}
    lock = threading.Lock()
    ignore_until = time.monotonic() + SETTLE_S

    h = lgpio.gpiochip_open(CHIP)
    for l in lines:
        lgpio.gpio_claim_alert(h, l, lgpio.BOTH_EDGES, lgpio.SET_PULL_UP)

    def on_edge(chip, gpio, level, tick):
        # level: 0 = fell to LOW (touch), 1 = rose to HIGH (release), 2 = watchdog
        if time.monotonic() < ignore_until:
            return
        if level == 0:                       # pad pulled LOW = a touch
            with lock:
                counts[gpio] += 1
                n = counts[gpio]
            print(f"  TOUCH {names[gpio]}  (count={n})", flush=True)

    cbs = [lgpio.callback(h, l, lgpio.BOTH_EDGES, on_edge) for l in lines]

    print("TTP223 test - pads:", flush=True)
    for p, l in zip(pins, lines):
        print(f"  {names[l]:<12} gpiochip{CHIP} line {l}", flush=True)
    print(f"touch each pad within {args.seconds}s ...", flush=True)
    try:
        time.sleep(args.seconds)
    except KeyboardInterrupt:
        pass
    finally:
        for cb in cbs:
            cb.cancel()
        lgpio.gpiochip_close(h)

    print("\n=== summary ===")
    ok = True
    for l in lines:
        hit = counts[l] > 0
        ok = ok and hit
        print(f"  {names[l]:<12} (line {l:>3}): {counts[l]} touches  "
              f"[{'OK' if hit else 'no touch detected'}]")
    print("PASS" if ok else "FAIL: not every pad registered a touch")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
