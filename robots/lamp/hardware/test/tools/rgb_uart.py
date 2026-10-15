#!/usr/bin/env python3
"""Drive the WS2812 ring via the ESP32-C3 bridge over UART7.

The OPi's SPI3 MOSI can't drive the strip reliably, so pixels go to an ESP32-C3
as Adalight frames and the C3 regenerates the WS2812 waveform locally.

    python3 rgb_uart.py --list                 # show every effect
    python3 rgb_uart.py rainbow                # run one
    python3 rgb_uart.py comet --color 0,128,255
    python3 rgb_uart.py off                    # blank and exit

BAUD IS NOT A FREE PARAMETER. The OPi's uart7 (s_uart0, AON domain) runs off the
24 MHz oscillator, so only 1500000/n is reachable: 1500000, 750000, 500000,
375000, 300000, 250000, 125000. Asking for 460800 silently puts 500000 on the
wire (8.5% error) and the link dies with zero bytes AND zero errors -- looks
exactly like an unplugged cable. Leave this at 500000.
"""

import argparse
import colorsys
import math
import random
import signal
import sys
import time

import serial

BAUD_OK = (1500000, 750000, 500000, 375000, 300000, 250000, 125000)


# ---------------------------------------------------------------- helpers

def hsv(h, s=1.0, v=1.0):
    r, g, b = colorsys.hsv_to_rgb(h % 1.0, s, max(0.0, min(1.0, v)))
    return (int(r * 255), int(g * 255), int(b * 255))


def scale(rgb, k):
    k = max(0.0, min(1.0, k))
    return (int(rgb[0] * k), int(rgb[1] * k), int(rgb[2] * k))


# ---------------------------------------------------------------- effects
# Each takes (frame, count, args, state) and returns `count` (r,g,b) tuples at
# full scale; --brightness is applied once at send time. `state` is a per-run
# dict for effects that need memory between frames.

def fx_rainbow(f, n, a, st):
    """Smooth rainbow rotating around the ring."""
    return [hsv(f * 0.004 * a.speed + i / n) for i in range(n)]


def fx_gradient(f, n, a, st):
    """Static rainbow -- one full spectrum, no motion."""
    return [hsv(i / n) for i in range(n)]


def fx_comet(f, n, a, st):
    """A bright head with an exponentially fading tail, looping the ring."""
    head = (f * 0.3 * a.speed) % n
    tail = max(1.0, n / 8.0)
    return [scale(a.rgb, math.exp(-((head - i) % n) / tail)) for i in range(n)]


def fx_scanner(f, n, a, st):
    """Cylon: head sweeps back and forth with a tail."""
    period = 2 * (n - 1)
    p = (f * 0.3 * a.speed) % period
    head = p if p < n - 1 else period - p
    tail = max(1.0, n / 10.0)
    return [scale(a.rgb, math.exp(-abs(head - i) / tail)) for i in range(n)]


def fx_breathe(f, n, a, st):
    """Whole ring pulsing. Squared so the fade looks linear to the eye."""
    b = (math.sin(f * 0.05 * a.speed) + 1.0) / 2.0
    return [scale(a.rgb, b * b)] * n


def fx_twinkle(f, n, a, st):
    """Random sparkles fading out, each its own colour."""
    lv = st.setdefault("lv", [0.0] * n)
    hu = st.setdefault("hu", [0.0] * n)
    for i in range(n):
        lv[i] *= 0.88
    for _ in range(2):
        if random.random() < 0.30 * a.speed:
            j = random.randrange(n)
            lv[j] = 1.0
            hu[j] = random.random()
    return [hsv(hu[i], 1.0, lv[i]) for i in range(n)]


def _heat_color(h):
    """Black -> red -> yellow -> white, the classic FastLED fire ramp."""
    t = h * 191 // 255
    v = (t & 0x3F) * 4
    if t > 128:
        return (255, 255, v)
    if t > 64:
        return (255, v, 0)
    return (v, 0, 0)


def fx_fire(f, n, a, st):
    """Flame simulation: cool, drift, spark."""
    heat = st.setdefault("heat", [0] * n)
    cool = int(100 / n) + 2
    for i in range(n):
        heat[i] = max(0, heat[i] - random.randint(0, cool))
    for i in range(n - 1, 1, -1):
        heat[i] = (heat[i - 1] + heat[i - 2] * 2) // 3
    if random.random() < 0.55 * a.speed:
        j = random.randrange(max(1, n // 8))
        heat[j] = min(255, heat[j] + random.randint(160, 255))
    return [_heat_color(h) for h in heat]


def fx_theater(f, n, a, st):
    """Marquee dashes: every 3rd pixel, stepping around."""
    off = int(f * 0.12 * a.speed) % 3
    return [a.rgb if i % 3 == off else (0, 0, 0) for i in range(n)]


def fx_wipe(f, n, a, st):
    """Fill the ring one pixel at a time, then erase it the same way."""
    period = n * 2
    p = int(f * 0.4 * a.speed) % period
    if p < n:
        return [a.rgb if i <= p else (0, 0, 0) for i in range(n)]
    return [(0, 0, 0) if i <= p - n else a.rgb for i in range(n)]


def fx_solid(f, n, a, st):
    """One colour, held."""
    return [a.rgb] * n


def fx_chase(f, n, a, st):
    """One white pixel walking. The honest signal-integrity test -- stutter and
    stray pixels show up here, where a rainbow would hide them."""
    px = [(0, 0, 0)] * n
    px[int(f * 0.5 * a.speed) % n] = (255, 255, 255)
    return px


def fx_off(f, n, a, st):
    """All dark."""
    return [(0, 0, 0)] * n


def fx_police(f, n, a, st):
    """Red/blue strobe, each colour owning half the ring."""
    on = (f // 6) % 2          # swap twice per ~4 frames at 50fps
    half = n // 2
    red, blue = (255, 0, 0), (0, 0, 255)
    return [(red if on else (0, 0, 0)) if i < half else
            ((0, 0, 0) if on else blue) for i in range(n)]


EFFECTS = {
    "rainbow": fx_rainbow,
    "gradient": fx_gradient,
    "comet": fx_comet,
    "scanner": fx_scanner,
    "breathe": fx_breathe,
    "twinkle": fx_twinkle,
    "fire": fx_fire,
    "theater": fx_theater,
    "wipe": fx_wipe,
    "solid": fx_solid,
    "chase": fx_chase,
    "police": fx_police,
    "off": fx_off,
}


# ---------------------------------------------------------------- transport

def adalight_header(count):
    """'Ada' + hi + lo + chk, where hi<<8|lo == count-1 and chk == hi^lo^0x55."""
    n = count - 1
    hi, lo = (n >> 8) & 0xFF, n & 0xFF
    return b"Ada" + bytes([hi, lo, hi ^ lo ^ 0x55])


def send(port, header, pixels, brightness):
    buf = bytearray(header)
    for r, g, b in pixels:
        buf += bytes((int(r * brightness), int(g * brightness), int(b * brightness)))
    port.write(buf)
    port.flush()


def main():
    ap = argparse.ArgumentParser(
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="effects: " + ", ".join(EFFECTS),
    )
    ap.add_argument("mode", nargs="?", default="rainbow", choices=list(EFFECTS))
    ap.add_argument("--list", action="store_true", help="describe every effect and exit")
    ap.add_argument("--dev", default="/dev/ttyS7")
    ap.add_argument("--baud", type=int, default=500000)
    ap.add_argument("--count", type=int, default=32)
    ap.add_argument("--brightness", type=float, default=0.4, help="0.0-1.0")
    ap.add_argument("--fps", type=float, default=50.0)
    ap.add_argument("--speed", type=float, default=1.0, help="animation rate multiplier")
    ap.add_argument("--color", default="0,128,255", help="R,G,B for single-colour effects")
    args = ap.parse_args()

    if args.list:
        w = max(len(k) for k in EFFECTS)
        print("effect".ljust(w), " description")
        for k, fn in EFFECTS.items():
            print(k.ljust(w), "", (fn.__doc__ or "").strip().split("\n")[0])
        print("\ncolour applies to: comet, scanner, breathe, theater, wipe, solid")
        return

    if args.baud not in BAUD_OK:
        print(f"warning: {args.baud} is not reachable on uart7 (only 1500000/n: "
              f"{', '.join(map(str, BAUD_OK))}). The wire will run at the nearest "
              f"rate and the link will likely see zero bytes.", file=sys.stderr)

    args.rgb = tuple(int(x) for x in args.color.split(","))
    header = adalight_header(args.count)
    port = serial.Serial(args.dev, args.baud, timeout=0)
    state = {}

    def blank(*_):
        send(port, header, [(0, 0, 0)] * args.count, 1.0)
        port.close()
        sys.exit(0)

    signal.signal(signal.SIGINT, blank)
    signal.signal(signal.SIGTERM, blank)

    if args.mode == "off":
        blank()

    fn = EFFECTS[args.mode]
    period = 1.0 / args.fps
    f = 0
    while True:
        send(port, header, fn(f, args.count, args, state), args.brightness)
        f += 1
        time.sleep(period)


if __name__ == "__main__":
    main()
