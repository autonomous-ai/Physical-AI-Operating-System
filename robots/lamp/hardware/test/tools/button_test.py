#!/usr/bin/env python3
"""
Simple GPIO button test — OrangePi 4 Pro (sun60iw2) lamp.

Mirrors os/hal/drivers/gpio_button.py:
  - lgpio alert callback on gpiochip1 line 9
  - SET_PULL_UP, BOTH_EDGES
  - active-LOW: press = LOW (falling edge), release = HIGH (rising edge)
  - per-edge 200ms debounce (press/release tracked separately), tick in ns

Prints each press with its hold duration; summary at the end.
Run:   python3 button_test.py                 # 20s window
       python3 button_test.py --seconds 60
Needs: lgpio (pip install lgpio) + RW on /dev/gpiochip1.
"""
import argparse, time
import lgpio

CHIP = 1
LINE = 9
DEBOUNCE_NS = 200_000_000   # 200ms — matches board_profile().button.debounce_ns


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seconds", type=int, default=20)
    args = ap.parse_args()

    st = {"presses": 0, "press_mono": 0.0, "last_press_tick": 0, "last_release_tick": 0}

    h = lgpio.gpiochip_open(CHIP)
    lgpio.gpio_claim_alert(h, LINE, lgpio.BOTH_EDGES, lgpio.SET_PULL_UP)

    def on_edge(chip, gpio, level, tick):
        # active-low: level 0 = pressed (falling), 1 = released (rising)
        if level == 0:
            if tick - st["last_press_tick"] < DEBOUNCE_NS:   # debounce bounces
                return
            st["last_press_tick"] = tick
            st["press_mono"] = time.monotonic()
            st["presses"] += 1
            print(f"  PRESSED   (#{st['presses']})", flush=True)
        elif level == 1:
            if tick - st["last_release_tick"] < DEBOUNCE_NS:
                return
            st["last_release_tick"] = tick
            held = time.monotonic() - st["press_mono"]
            print(f"  RELEASED  (held {held:.2f}s)", flush=True)

    cb = lgpio.callback(h, LINE, lgpio.BOTH_EDGES, on_edge)

    print(f"Button test — press the button a few times for {args.seconds}s ...", flush=True)
    try:
        time.sleep(args.seconds)
    except KeyboardInterrupt:
        pass
    finally:
        cb.cancel()
        lgpio.gpiochip_close(h)

    print("\n=== summary ===")
    print(f"  button (gpiochip{CHIP} line {LINE}): {st['presses']} presses  "
          f"[{'OK' if st['presses'] else 'no press detected'}]")


if __name__ == "__main__":
    main()
