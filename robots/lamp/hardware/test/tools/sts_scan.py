#!/usr/bin/env python3
"""Find live Feetech STS/SCS servos: raw protocol, echo-aware. Read-only."""
import glob, sys, time
import serial

def cks(b):
    return (~sum(b)) & 0xFF

def pkt(sid, inst, params=()):
    body = [sid, len(params) + 2, inst] + list(params)
    return bytes([0xFF, 0xFF] + body + [cks(body)])

def txrx(ser, req, want, settle=0.012):
    ser.reset_input_buffer()
    ser.write(req); ser.flush()
    time.sleep(settle)
    data = ser.read(ser.in_waiting or 0)
    # half-duplex adapters echo the request back; drop that prefix
    if data.startswith(req):
        data = data[len(req):]
    if len(data) < 6 or data[0] != 0xFF or data[1] != 0xFF:
        return None, data
    body = list(data[2:2 + 3 + want])
    if len(body) < 3 + want:
        return None, data
    if cks(body[:-0] if want == 0 else body) != data[2 + 3 + want] if False else False:
        pass
    return data, data

def ping(ser, sid):
    req = pkt(sid, 0x01)
    ser.reset_input_buffer()
    ser.write(req); ser.flush()
    time.sleep(0.012)
    data = ser.read(ser.in_waiting or 0)
    echo = data.startswith(req)
    if echo:
        data = data[len(req):]
    if len(data) >= 6 and data[0] == 0xFF and data[1] == 0xFF and data[2] == sid:
        return True, echo, data
    return False, echo, data

def read_reg(ser, sid, addr, n):
    req = pkt(sid, 0x02, [addr, n])
    ser.reset_input_buffer()
    ser.write(req); ser.flush()
    time.sleep(0.012)
    data = ser.read(ser.in_waiting or 0)
    if data.startswith(req):
        data = data[len(req):]
    if len(data) >= 6 + n - 1 and data[0] == 0xFF and data[1] == 0xFF and data[2] == sid:
        params = data[5:5 + n]
        if n == 1:
            return params[0]
        if n == 2:
            return params[0] | (params[1] << 8)
    return None

ports = sys.argv[1:] or sorted(set(glob.glob("/dev/ttyACM*") + glob.glob("/dev/ttyUSB*") + glob.glob("/dev/ttyS[0-9]*")))
BAUDS = [1000000, 500000, 250000, 115200]

print("ports to try:", ports)
for port in ports:
    for baud in BAUDS:
        try:
            ser = serial.Serial(port, baud, timeout=0.05)
        except Exception as e:
            print("%-14s %-8d open failed: %s" % (port, baud, e)); break
        found, echoed = [], False
        for sid in range(1, 13):
            ok, echo, raw = ping(ser, sid)
            echoed = echoed or echo
            if ok:
                found.append(sid)
        tag = " (adapter echoes)" if echoed else ""
        if found:
            print("*** %-14s %-8d LIVE IDs: %s%s" % (port, baud, found, tag))
            for sid in found:
                mn = read_reg(ser, sid, 9, 2)
                mx = read_reg(ser, sid, 11, 2)
                pos = read_reg(ser, sid, 56, 2)
                lock = read_reg(ser, sid, 55, 1)
                print("      ID %-2d  min=%s max=%s pos=%s lock=%s" % (sid, mn, mx, pos, lock))
        else:
            print("    %-14s %-8d no response%s" % (port, baud, tag))
        ser.close()
