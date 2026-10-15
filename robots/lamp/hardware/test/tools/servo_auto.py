#!/usr/bin/env python3
"""Each servo 1..5 in turn: 1800 -> 2200 -> 1800, 1 s pause after each move."""
import time
from servo_test import Bus, JOINTS

bus = Bus("/dev/ttyACM0", 1_000_000)
try:
    for sid in JOINTS:
        if sid == 1 :
            continue
        print(f"== servo {sid} ({JOINTS[sid]}) ==")
        for pos in (1800, 2200):
            bus.move_slow(sid, pos, 1.0, 30, 0, True)
            time.sleep(1.0)
    print("done.")
except KeyboardInterrupt:
    print("\n  stopped.")
finally:
    bus.close()
