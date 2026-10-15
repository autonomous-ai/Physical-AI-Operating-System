#!/usr/bin/env python3
"""
Simple standalone WS2812 (NeoPixel) RGB test for the OrangePi 4 Pro (sun60iw2).

Mirrors the production driver in autonomous-os:
    os/hal/drivers/rgb/rgb_service.py  ->  _StripSPI

Wiring (per board/boards.json "orangepi_sun60"):
    LED DIN  <-  SPI3 MOSI  =  physical pin 19   (/dev/spidev3.0)
    LED GND  <-  board GND
    LED 5V   <-  5V rail

Usage:
    python3 rgb_test.py                    # 64 LEDs, /dev/spidev3.0, 30% brightness
    python3 rgb_test.py --count 60         # match your strip's LED count
    python3 rgb_test.py --bus 3 --device 0 # pick a different spidev
    python3 rgb_test.py --brightness 1.0   # full brightness (draws a lot of current!)

Requires:  sudo apt install python3-spidev   (or: pip install spidev)
           SPI3 enabled in the device tree so /dev/spidev3.0 exists.
"""
import argparse
import time
import spidev

# WS2812 bit timing encoded as SPI bytes at 6.4 MHz (copied from _StripSPI)
BIT0 = 0xC0           # ~312 ns high, ~937 ns low  -> logical 0
BIT1 = 0xF8           # ~781 ns high, ~468 ns low  -> logical 1
PRIMER_BYTES = 10     # MOSI-low before first bit so pixel 0 can't latch a stray HIGH
RESET_BYTES = 250     # ~312 us reset latch (>= WS2812B-V5 280 us threshold)
SPI_HZ = 6_400_000


class Strip:
    def __init__(self, count, bus=3, device=0, brightness=0.3):
        self.count = count
        self.brightness = brightness              # 0.0-1.0
        self.pixels = [(0, 0, 0)] * count
        self.spi = spidev.SpiDev()
        self.spi.open(bus, device)                # /dev/spidev<bus>.<device>
        self.spi.max_speed_hz = SPI_HZ
        self.spi.mode = 0b00

    def _encode_byte(self, b):
        return [BIT1 if b & (1 << (7 - i)) else BIT0 for i in range(8)]

    def _encode_pixel(self, r, g, b):
        br = self.brightness
        # WS2812 expects GRB order
        return (self._encode_byte(int(g * br))
                + self._encode_byte(int(r * br))
                + self._encode_byte(int(b * br)))

    def fill(self, color):
        self.pixels = [color] * self.count

    def show(self):
        buf = [0x00] * PRIMER_BYTES
        for r, g, b in self.pixels:
            buf += self._encode_pixel(r, g, b)
        buf += [0x00] * RESET_BYTES
        self.spi.writebytes2(buf)

    def clear(self):
        self.fill((0, 0, 0))
        self.show()

    def close(self):
        self.clear()
        self.spi.close()


def main():
    ap = argparse.ArgumentParser(description="WS2812 RGB test (pin 19 / SPI3)")
    ap.add_argument("--count", type=int, default=64)
    ap.add_argument("--bus", type=int, default=3)        # SPI3 -> pin 19 MOSI on sun60iw2
    ap.add_argument("--device", type=int, default=0)
    ap.add_argument("--brightness", type=float, default=0.3)
    args = ap.parse_args()

    strip = Strip(args.count, args.bus, args.device, args.brightness)
    print(f"WS2812 test on /dev/spidev{args.bus}.{args.device} — "
          f"{args.count} LEDs @ brightness {args.brightness}")

    try:
        for name, c in (("RED",   (255, 0, 0)),
                        ("GREEN", (0, 255, 0)),
                        ("BLUE",  (0, 0, 255)),
                        ("WHITE", (255, 255, 255))):
            print(f"  -> {name}")
            strip.fill(c)
            strip.show()
            time.sleep(1.0)

        # single-pixel chase to prove per-LED addressing + GRB order
        print("  -> chase")
        for i in range(args.count):
            strip.fill((0, 0, 0))
            strip.pixels[i] = (0, 40, 60)
            strip.show()
            time.sleep(0.03)
    finally:
        print("  -> off")
        strip.close()


if __name__ == "__main__":
    main()
