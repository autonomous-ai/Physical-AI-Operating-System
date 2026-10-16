#!/usr/bin/env python3
"""UART7 loopback test. Jumper OPi pin 8 (TXD.7) directly to pin 10 (RXD.7).

Proves the OPi half of the link -- pinmux, clock, baud, driver -- before any
ESP32 is involved. If this fails, no amount of ESP32 debugging will help; if it
passes, any later failure is in the ESP32 or the wiring, not the Orange Pi.

    python3 uart_loopback.py            # 460800, the rate rgb_uart.py uses
"""
import sys
import time

import serial

DEV = sys.argv[1] if len(sys.argv) > 1 else "/dev/ttyS7"
BAUD = int(sys.argv[2]) if len(sys.argv) > 2 else 460800

payload = bytes(range(256)) * 4  # 1024 bytes, every byte value, several times

p = serial.Serial(DEV, BAUD, timeout=1.0)
p.reset_input_buffer()
p.write(payload)
p.flush()
time.sleep(0.2)
got = p.read(len(payload))
p.close()

print(f"{DEV} @ {BAUD}: sent {len(payload)}, got {len(got)}")
if got == payload:
    print("PASS - loopback byte-exact. OPi UART7 TX+RX are good.")
    sys.exit(0)
if not got:
    print("FAIL - nothing came back. Jumper pin 8 <-> pin 10 not fitted, or uart7 not muxed.")
    sys.exit(1)
bad = sum(1 for a, b in zip(payload, got) if a != b)
print(f"FAIL - {len(got)}/{len(payload)} bytes returned, {bad} mismatched -> baud/signal problem.")
sys.exit(1)
