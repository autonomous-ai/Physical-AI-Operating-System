#!/usr/bin/env python3
"""Read-only inspection of the STS3215 bus. Changes nothing."""
import sys

# 1. Authoritative register addresses from the installed lerobot
ADDRS = {}
for mod, name in [
    ("lerobot.motors.feetech.tables", "STS_SMS_SERIES_CONTROL_TABLE"),
    ("lerobot.common.motors.feetech.tables", "STS_SMS_SERIES_CONTROL_TABLE"),
    ("lerobot.motors.feetech.feetech", "STS_SMS_SERIES_CONTROL_TABLE"),
]:
    try:
        m = __import__(mod, fromlist=[name])
        ADDRS = getattr(m, name)
        print("control table from %s.%s" % (mod, name))
        break
    except Exception:
        continue
if ADDRS:
    for k in ("Min_Angle_Limit", "Max_Angle_Limit", "Lock", "Present_Position",
              "Goal_Position", "Torque_Enable", "Phase", "Mode"):
        if k in ADDRS:
            print("  %-18s addr=%-4s size=%s" % (k, ADDRS[k][0], ADDRS[k][1]))
else:
    print("lerobot control table not importable; falling back to datasheet addrs")

MIN_A = ADDRS.get("Min_Angle_Limit", (9, 2))
MAX_A = ADDRS.get("Max_Angle_Limit", (11, 2))
LOCK = ADDRS.get("Lock", (55, 1))
PRES = ADDRS.get("Present_Position", (56, 2))

from scservo_sdk import PortHandler, PacketHandler

PORT = sys.argv[1] if len(sys.argv) > 1 else "/dev/ttyACM0"
BAUD = int(sys.argv[2]) if len(sys.argv) > 2 else 1000000

ph = PortHandler(PORT)
if not ph.openPort():
    print("failed to open %s" % PORT); sys.exit(1)
if not ph.setBaudRate(BAUD):
    print("failed to set baud %d" % BAUD); sys.exit(1)
pk = PacketHandler(0)          # protocol_end=0 for STS/SMS
print("\nport=%s baud=%d" % (PORT, BAUD))

print("\n=== bus scan (IDs 1..12) ===")
found = []
for sid in range(1, 13):
    model, res, err = pk.ping(ph, sid)
    if res == 0:
        found.append(sid)
        print("  ID %-2d online  model=%s" % (sid, model))
print("  online:", found or "none")

def rd(sid, entry):
    addr, size = entry
    if size == 1:
        v, res, err = pk.read1ByteTxRx(ph, sid, addr)
    else:
        v, res, err = pk.read2ByteTxRx(ph, sid, addr)
    return (v if res == 0 else None), res, err

for sid in (found or []):
    print("\n=== servo %d ===" % sid)
    for label, entry in (("Min_Angle_Limit", MIN_A), ("Max_Angle_Limit", MAX_A),
                         ("Lock", LOCK), ("Present_Position", PRES)):
        v, res, err = rd(sid, entry)
        print("  %-18s addr=%-3d value=%s%s" %
              (label, entry[0], v, "" if res == 0 else "  (comm err %d)" % res))

ph.closePort()
