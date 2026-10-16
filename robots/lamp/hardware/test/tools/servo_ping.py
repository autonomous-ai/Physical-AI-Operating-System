#!/usr/bin/env python3
"""Read-only health check of the Feetech STS3215 bus. Nothing moves.

Reads present position / torque / voltage / temperature from each servo. Torque is
never enabled and no goal is ever written, so this is safe to run at any time.

This scservo_sdk build has no sms_sts module -- PacketHandler(0) (SCS_END,
little-endian) is the STS-compatible path.
"""
from scservo_sdk import PortHandler, PacketHandler

PORT = "/dev/ttyACM0"
BAUD = 1000000
NAMES = {1: "base_yaw", 2: "base_pitch", 3: "elbow_pitch", 4: "wrist_roll", 5: "wrist_pitch"}

# STS3215 control table
ADDR_TORQUE_ENABLE = 40
ADDR_PRESENT_POS = 56
ADDR_PRESENT_VOLT = 62
ADDR_PRESENT_TEMP = 63

ph = PortHandler(PORT)
if not ph.openPort():
    raise SystemExit("FAIL: could not open " + PORT)
if not ph.setBaudRate(BAUD):
    raise SystemExit("FAIL: could not set baud " + str(BAUD))

pk = PacketHandler(0)

print("id  name          pos   deg      torque  volt   temp")
print("-" * 52)
ok = 0
for i in range(1, 6):
    name = NAMES.get(i, "?")
    pos, comm, err = pk.read2ByteTxRx(ph, i, ADDR_PRESENT_POS)
    if comm != 0:
        print("{:<3} {:<13} -- NO RESPONSE".format(i, name))
        continue
    tq, _, _ = pk.read1ByteTxRx(ph, i, ADDR_TORQUE_ENABLE)
    v, _, _ = pk.read1ByteTxRx(ph, i, ADDR_PRESENT_VOLT)
    t, _, _ = pk.read1ByteTxRx(ph, i, ADDR_PRESENT_TEMP)
    deg = (pos - 2048) * 360.0 / 4096.0
    print("{:<3} {:<13} {:<5} {:+7.1f}  {:<7} {:.1f}V  {:>3}C".format(
        i, name, pos, deg, "on" if tq else "off", v / 10.0, t))
    ok += 1

ph.closePort()
print()
print("{}/5 servos responding".format(ok))
