#!/usr/bin/env python3
"""
WS2812 effects — OrangePi 4 Pro lamp strip on SPI3 (/dev/spidev3.0, pin 19).

Drives the strip DIRECTLY from SPI3 MOSI through a 74HCT1G125GW buffer (3.3V ->
5V), so no ESP32 bridge is involved. Same WS2812 encoding as the HAL's _StripSPI:
bit0=0xC0, bit1=0xF8, GRB order, 6.4MHz, primer + reset.

    python3 rgb_fade.py                    # original 7-colour rainbow fade (unchanged)
    python3 rgb_fade.py --list             # show every effect
    python3 rgb_fade.py rainbow            # spectrum rotating around the ring
    python3 rgb_fade.py comet --color 0,128,255
    python3 rgb_fade.py fire
    python3 rgb_fade.py off                 # blank and exit

Needs: spidev + RW on /dev/spidev3.0.
"""
import argparse
import colorsys
import math
import random
import time

import spidev

BIT0, BIT1, PRIMER, RESET = 0xC0, 0xF8, 10, 250
SPI_HZ = 6_400_000

RINGS = [32, 24, 12]
TOTAL_LEDS = sum(RINGS)

# Original 7-colour rainbow used by the default "fade" mode. Untouched.
COLORS = [
    (255,   0,   0),   # red
    (255,  90,   0),   # orange
    (255, 200,   0),   # yellow
    (0,   255,   0),   # green
    (0,     0, 255),   # blue
    (75,    0, 130),   # indigo
    (148,   0, 211),   # violet
]


# ---------------------------------------------------------------- encoding

def _enc_byte(b):
    return [BIT1 if b & (1 << (7 - i)) else BIT0 for i in range(8)]

# One encoded byte per bit is 8 SPI bytes; precompute all 256 so per-frame
# encoding is a dict lookup instead of a bit loop (matters at 50fps).
_ENC = [_enc_byte(v) for v in range(256)]


def show_frame(spi, pixels, br):
    """Push a full per-pixel frame. pixels = list of (r,g,b) at full 0-255 scale;
    brightness is applied here so effects never scale twice."""
    buf = [0x00] * PRIMER
    for r, g, b in pixels:
        buf += _ENC[int(g * br) & 0xFF]      # WS2812 wants GRB
        buf += _ENC[int(r * br) & 0xFF]
        buf += _ENC[int(b * br) & 0xFF]
    buf += [0x00] * RESET
    spi.writebytes2(buf)


# ---------------------------------------------------------------- helpers

def hsv(h, s=1.0, v=1.0):
    r, g, b = colorsys.hsv_to_rgb(h % 1.0, s, max(0.0, min(1.0, v)))
    return (int(r * 255), int(g * 255), int(b * 255))


def scale(rgb, k):
    k = max(0.0, min(1.0, k))
    return (int(rgb[0] * k), int(rgb[1] * k), int(rgb[2] * k))


# ---------------------------------------------------------------- effects
# (frame, count, args, state) -> list of `count` (r,g,b) at full scale.

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
    """Flame simulation: cool, drift up, spark at the base."""
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
    """One white pixel walking -- the honest signal-integrity test."""
    px = [(0, 0, 0)] * n
    px[int(f * 0.5 * a.speed) % n] = (255, 255, 255)
    return px


def fx_police(f, n, a, st):
    """Red/blue strobe, each colour owning half the ring."""
    on = (f // 6) % 2
    half = n // 2
    return [((255, 0, 0) if on else (0, 0, 0)) if i < half else
            ((0, 0, 0) if on else (0, 0, 255)) for i in range(n)]


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
}


# ---------------------------------------------------------------- fade (original)

def run_fade(spi, args):
    """The original 7-colour crossfade. Kept byte-for-byte in behaviour so
    `python3 rgb_fade.py` with no args is unchanged."""
    br = args.brightness
    steps = max(1, int(args.fade * args.fps))
    t_end = (time.monotonic() + args.seconds) if args.seconds > 0 else None
    n = len(COLORS)
    print(f"fading {n} colors on /dev/spidev{args.bus}.{args.device} "
          f"({args.count} px, brightness {br}) -- Ctrl-C to stop", flush=True)
    i = 0
    while True:
        c0, c1 = COLORS[i % n], COLORS[(i + 1) % n]
        for s in range(steps):
            t = s / steps
            col = tuple(int(c0[k] + (c1[k] - c0[k]) * t) for k in range(3))
            show_frame(spi, [col] * args.count, br)
            time.sleep(1.0 / args.fps)
            if t_end and time.monotonic() >= t_end:
                return
        i += 1


def run_effect(spi, args):
    fn = EFFECTS[args.mode]
    st = {}
    t_end = (time.monotonic() + args.seconds) if args.seconds > 0 else None
    print(f"effect '{args.mode}' on /dev/spidev{args.bus}.{args.device} "
          f"({args.count} px, brightness {args.brightness}) -- Ctrl-C to stop", flush=True)
    f = 0
    while True:
        show_frame(spi, fn(f, args.count, args, st), args.brightness)
        f += 1
        time.sleep(1.0 / args.fps)
        if t_end and time.monotonic() >= t_end:
            return


def main():
    ap = argparse.ArgumentParser(
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="modes: fade (default), off, " + ", ".join(EFFECTS),
    )
    ap.add_argument("mode", nargs="?", default="fade",
                    choices=["fade", "off"] + list(EFFECTS))
    ap.add_argument("--list", action="store_true", help="describe every effect and exit")
    ap.add_argument("--bus", type=int, default=3)
    ap.add_argument("--device", type=int, default=0)
    ap.add_argument("--count", type=int, default=TOTAL_LEDS)
    ap.add_argument("--brightness", type=float, default=0.4)
    ap.add_argument("--fps", type=int, default=50)
    ap.add_argument("--speed", type=float, default=1.0, help="animation rate (effects)")
    ap.add_argument("--color", default="0,128,255", help="R,G,B for single-colour effects")
    ap.add_argument("--fade", type=float, default=1.2, help="seconds per colour (fade mode)")
    ap.add_argument("--seconds", type=float, default=0, help="0 = loop until Ctrl-C")
    args = ap.parse_args()

    if args.list:
        w = max(len(k) for k in list(EFFECTS) + ["fade", "off"])
        print("mode".ljust(w), " description")
        print("fade".ljust(w), "  original 7-colour rainbow crossfade (default)")
        for k, fn in EFFECTS.items():
            print(k.ljust(w), "", (fn.__doc__ or "").strip().split("\n")[0])
        print("off".ljust(w), "  blank the strip and exit")
        print("\ncolour applies to: comet, scanner, breathe, theater, wipe, solid")
        return

    args.rgb = tuple(int(x) for x in args.color.split(","))

    spi = spidev.SpiDev()
    spi.open(args.bus, args.device)
    spi.max_speed_hz = SPI_HZ
    spi.mode = 0

    try:
        if args.mode == "off":
            pass
        elif args.mode == "fade":
            run_fade(spi, args)
        else:
            run_effect(spi, args)
    except KeyboardInterrupt:
        pass
    finally:
        show_frame(spi, [(0, 0, 0)] * args.count, 1.0)   # blank
        spi.close()
        print("stopped, LED off", flush=True)


if __name__ == "__main__":
    main()
