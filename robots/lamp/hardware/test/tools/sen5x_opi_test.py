#!/usr/bin/env python3
"""Sensirion SEN5x (SEN50/SEN54/SEN55) test for OrangePi 4 Pro.

Linux port of the Sensirion reference driver
(https://github.com/Sensirion/raspberry-pi-i2c-sen5x), pure stdlib: talks to
/dev/i2c-* through the I2C_RDWR ioctl, so no smbus/i2c-tools needed.

Wiring - the SEN5x cable, on the 40-pin header:
    pin 1 VDD  red     -> header pin 2 or 4  (5V +/-10%, NOT 3.3V)
    pin 2 GND  black   -> header pin 6       (GND)
    pin 3 SDA  YELLOW  -> header pin 3       (SDA.0)
    pin 4 SCL  GREEN   -> header pin 5       (SCL.0)
    pin 5 SEL  blue    -> header pin 9       (GND - selects I2C mode)
    pin 6 NC           -> leave open

Watch the two signal colours: per the SEN5x datasheet green is SCL and yellow
is SDA. The Sensirion raspberry-pi-i2c-sen5x README lists them the other way
round; following that table crosses SDA/SCL and the sensor never ACKs.

Two datasheet requirements that cause silent failures:
  * SEL must be pulled to GND BEFORE or as the sensor powers up - grounding it
    afterwards does nothing until the sensor is power-cycled.
  * Standard mode only: 100 kbit/s max, and it does not use clock stretching.
    On a 400 kHz bus the sensor simply never answers.

Usage:
    sudo ./sen5x_opi_test.py probe          # is it there? identify it
    sudo ./sen5x_opi_test.py read           # start measuring, stream values
    sudo ./sen5x_opi_test.py read --once    # one sample, then stop
    sudo ./sen5x_opi_test.py status         # device status register
    sudo ./sen5x_opi_test.py reset
"""

import argparse
import ctypes
import glob
import os
import sys
import time

# --- Linux i2c-dev ioctl plumbing -------------------------------------------

I2C_RDWR = 0x0707
I2C_M_RD = 0x0001


class I2CMsg(ctypes.Structure):
    _fields_ = [
        ("addr", ctypes.c_uint16),
        ("flags", ctypes.c_uint16),
        ("len", ctypes.c_uint16),
        ("buf", ctypes.POINTER(ctypes.c_uint8)),
    ]


class I2CRdwrIoctlData(ctypes.Structure):
    _fields_ = [
        ("msgs", ctypes.POINTER(I2CMsg)),
        ("nmsgs", ctypes.c_uint32),
    ]


libc = ctypes.CDLL("libc.so.6", use_errno=True)
libc.ioctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_void_p]

SEN5X_ADDR = 0x69

# command, execution time in seconds (from the SEN5x datasheet)
CMD_START_MEASUREMENT = (0x0021, 0.050)
CMD_START_MEASUREMENT_RHT_ONLY = (0x0037, 0.050)
CMD_STOP_MEASUREMENT = (0x0104, 0.200)
CMD_READ_DATA_READY = (0x0202, 0.020)
CMD_READ_MEASURED_VALUES = (0x03C4, 0.020)
CMD_READ_PRODUCT_NAME = (0xD014, 0.020)
CMD_READ_SERIAL_NUMBER = (0xD033, 0.020)
CMD_READ_FIRMWARE_VERSION = (0xD100, 0.020)
CMD_READ_DEVICE_STATUS = (0xD206, 0.020)
CMD_DEVICE_RESET = (0xD304, 0.100)


def crc8(data):
    """Sensirion CRC-8: poly 0x31, init 0xFF, MSB first, no final xor."""
    crc = 0xFF
    for byte in data:
        crc ^= byte
        for _ in range(8):
            if crc & 0x80:
                crc = ((crc << 1) ^ 0x31) & 0xFF
            else:
                crc = (crc << 1) & 0xFF
    return crc


class SEN5x:
    def __init__(self, bus_num, addr=SEN5X_ADDR):
        self.path = "/dev/i2c-%d" % bus_num
        self.bus_num = bus_num
        self.addr = addr
        self.fd = os.open(self.path, os.O_RDWR)

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None

    def _rdwr(self, msgs):
        arr = (I2CMsg * len(msgs))(*msgs)
        data = I2CRdwrIoctlData(msgs=arr, nmsgs=len(msgs))
        if libc.ioctl(self.fd, I2C_RDWR, ctypes.byref(data)) < 0:
            err = ctypes.get_errno()
            raise OSError(err, os.strerror(err))

    def send_command(self, cmd_tuple, args_words=()):
        """Write a 16-bit command (plus optional CRC'd argument words)."""
        cmd, exec_time = cmd_tuple
        payload = [(cmd >> 8) & 0xFF, cmd & 0xFF]
        for word in args_words:
            hi, lo = (word >> 8) & 0xFF, word & 0xFF
            payload += [hi, lo, crc8([hi, lo])]
        buf = (ctypes.c_uint8 * len(payload))(*payload)
        self._rdwr([I2CMsg(addr=self.addr, flags=0, len=len(payload), buf=buf)])
        time.sleep(exec_time)

    def read_words(self, count):
        """Read `count` 16-bit words, validating the CRC after each one.

        The SEN5x needs the command write and the data read to be separate
        transactions with the execution delay between them, so this is a plain
        read - not a repeated start.
        """
        nbytes = count * 3
        rbuf = (ctypes.c_uint8 * nbytes)()
        self._rdwr([I2CMsg(addr=self.addr, flags=I2C_M_RD, len=nbytes, buf=rbuf)])
        raw = bytes(rbuf)
        words = []
        for i in range(count):
            hi, lo, crc = raw[i * 3], raw[i * 3 + 1], raw[i * 3 + 2]
            if crc8([hi, lo]) != crc:
                raise IOError("CRC mismatch in word %d (got 0x%02X)" % (i, crc))
            words.append((hi << 8) | lo)
        return words

    def read_string(self, cmd_tuple, nwords=16):
        self.send_command(cmd_tuple)
        words = self.read_words(nwords)
        raw = bytearray()
        for w in words:
            raw += bytes([(w >> 8) & 0xFF, w & 0xFF])
        return raw.split(b"\x00", 1)[0].decode("ascii", "replace")

    # -- high level --------------------------------------------------------

    def product_name(self):
        return self.read_string(CMD_READ_PRODUCT_NAME)

    def serial_number(self):
        return self.read_string(CMD_READ_SERIAL_NUMBER)

    def firmware_version(self):
        self.send_command(CMD_READ_FIRMWARE_VERSION)
        word = self.read_words(1)[0]
        return "%d.%d" % ((word >> 8) & 0xFF, word & 0xFF)

    def device_status(self):
        self.send_command(CMD_READ_DEVICE_STATUS)
        hi, lo = self.read_words(2)
        return (hi << 16) | lo

    def start(self, pm=True):
        self.send_command(CMD_START_MEASUREMENT if pm
                          else CMD_START_MEASUREMENT_RHT_ONLY)

    def stop(self):
        self.send_command(CMD_STOP_MEASUREMENT)

    def reset(self):
        self.send_command(CMD_DEVICE_RESET)

    def data_ready(self):
        self.send_command(CMD_READ_DATA_READY)
        return (self.read_words(1)[0] & 0xFF) != 0

    def measured_values(self):
        self.send_command(CMD_READ_MEASURED_VALUES)
        w = self.read_words(8)

        def u(v, scale):
            return None if v == 0xFFFF else v / scale

        def s(v, scale):
            # Datasheet: "If any value is unknown, 0xFFFF is returned" - this
            # applies to the int16 fields too, so 0xFFFF means unknown here
            # rather than -1.
            if v == 0xFFFF:
                return None
            if v > 0x7FFF:
                v -= 0x10000
            return v / scale

        return {
            "pm1p0": u(w[0], 10.0),
            "pm2p5": u(w[1], 10.0),
            "pm4p0": u(w[2], 10.0),
            "pm10": u(w[3], 10.0),
            "humidity": s(w[4], 100.0),
            "temperature": s(w[5], 200.0),
            "voc_index": s(w[6], 10.0),
            "nox_index": s(w[7], 10.0),
        }


# --- status register bits (SEN5x datasheet) ----------------------------------

STATUS_BITS = [
    (21, "SPEED - fan speed out of range"),
    (5, "GAS - gas sensor error"),
    (4, "RHT - RH/T communication error"),
    (3, "LASER - laser failure"),
    (2, "FAN - fan failure, blocked or broken"),
]


def describe_status(value):
    if value == 0:
        return ["no faults"]
    out = []
    for bit, text in STATUS_BITS:
        if value & (1 << bit):
            out.append("bit %d: %s" % (bit, text))
    return out or ["unknown flags set (0x%08X)" % value]


# --- helpers -----------------------------------------------------------------

def list_buses():
    buses = []
    for path in glob.glob("/dev/i2c-*"):
        try:
            buses.append(int(path.rsplit("-", 1)[1]))
        except ValueError:
            pass
    return sorted(buses)


def probe_bus(bus_num, addr=SEN5X_ADDR):
    """Probe by writing a real command - a bare read can be NAKed by a SEN5x."""
    try:
        dev = SEN5x(bus_num, addr)
    except OSError:
        return False
    try:
        dev.send_command(CMD_READ_PRODUCT_NAME)
        dev.read_words(16)
        return True
    except (OSError, IOError):
        return False
    finally:
        dev.close()


def find_sensor(addr=SEN5X_ADDR, bus=None):
    buses = [bus] if bus is not None else list_buses()
    print("Probing 0x%02X on bus(es): %s"
          % (addr, ", ".join(str(b) for b in buses)), flush=True)
    for bus_num in buses:
        if probe_bus(bus_num, addr):
            return bus_num
    return None


def open_sensor(args):
    bus = find_sensor(args.addr, args.bus)
    if bus is None:
        print("\nNo SEN5x responding at 0x%02X on any bus." % args.addr,
              file=sys.stderr)
        print(
            "Check, in this order:\n"
            "  1. SDA/SCL not crossed: YELLOW (SDA) -> header pin 3,\n"
            "     GREEN (SCL) -> header pin 5. The Sensirion Pi README lists\n"
            "     these colours the other way round - the datasheet is right.\n"
            "  2. SEL (blue) tied to GND - required for I2C mode, and it must\n"
            "     be grounded before the sensor powers up, so power-cycle the\n"
            "     sensor after fixing it.\n"
            "  3. VDD (red) on 5V (header pin 2 or 4), NOT 3.3V - measured at\n"
            "     the sensor connector, not at the header.\n"
            "  4. Bus speed <= 100 kHz (this board: the i2c0-100k overlay).\n"
            "  5. /dev/i2c-0 exists (needs the i2c0 overlay enabled).",
            file=sys.stderr)
        sys.exit(1)
    print("Found SEN5x on /dev/i2c-%d\n" % bus)
    return SEN5x(bus, args.addr)


def cmd_probe(args):
    dev = open_sensor(args)
    try:
        print("product name     : %s" % dev.product_name())
        print("serial number    : %s" % dev.serial_number())
        print("firmware version : %s" % dev.firmware_version())
        status = dev.device_status()
        print("device status    : 0x%08X" % status)
        for line in describe_status(status):
            print("                   %s" % line)
    finally:
        dev.close()
    return 0


def cmd_status(args):
    dev = open_sensor(args)
    try:
        status = dev.device_status()
        print("device status: 0x%08X" % status)
        for line in describe_status(status):
            print("  %s" % line)
    finally:
        dev.close()
    return 0


def cmd_reset(args):
    dev = open_sensor(args)
    try:
        dev.reset()
        print("device reset issued")
    finally:
        dev.close()
    return 0


def cmd_read(args):
    dev = open_sensor(args)
    try:
        print("product: %s  firmware: %s" %
              (dev.product_name(), dev.firmware_version()))
        dev.start(pm=not args.no_pm)
        print("measurement started%s" % ("" if not args.no_pm else " (RH/T/gas only)"))
        print("PM values need ~30s to settle; VOC/NOx index starts at 100 "
              "and needs minutes.\nCtrl-C to stop.\n")
        print("%-8s %7s %7s %7s %7s %7s %7s %6s %6s" %
              ("time", "PM1.0", "PM2.5", "PM4.0", "PM10", "RH%", "T°C",
               "VOC", "NOx"))

        deadline = time.time() + args.timeout
        while True:
            while not dev.data_ready():
                if time.time() > deadline:
                    print("timed out waiting for data-ready", file=sys.stderr)
                    return 1
                time.sleep(0.2)
            v = dev.measured_values()

            def f(x, width=7, dp=1):
                return ("%*.*f" % (width, dp, x)) if x is not None else "%*s" % (width, "-")

            print("%-8s %s %s %s %s %s %s %s %s" % (
                time.strftime("%H:%M:%S"),
                f(v["pm1p0"]), f(v["pm2p5"]), f(v["pm4p0"]), f(v["pm10"]),
                f(v["humidity"]), f(v["temperature"]),
                f(v["voc_index"], 6, 0), f(v["nox_index"], 6, 0)), flush=True)

            if args.once:
                break
            deadline = time.time() + args.timeout
            time.sleep(args.interval)
    except KeyboardInterrupt:
        print("\nstopping")
    finally:
        try:
            dev.stop()
        except OSError:
            pass
        dev.close()
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=["probe", "read", "status", "reset"])
    ap.add_argument("--bus", type=int, default=None,
                    help="I2C bus number (default: search every bus)")
    ap.add_argument("--addr", type=lambda s: int(s, 0), default=SEN5X_ADDR,
                    help="I2C address (default: 0x69)")
    ap.add_argument("--interval", type=float, default=1.0,
                    help="seconds between samples (default: 1.0)")
    ap.add_argument("--timeout", type=float, default=15.0,
                    help="seconds to wait for data-ready (default: 15)")
    ap.add_argument("--once", action="store_true", help="read a single sample")
    ap.add_argument("--no-pm", action="store_true",
                    help="start without the PM fan (RH/T/VOC/NOx only)")
    args = ap.parse_args()

    try:
        if args.command == "probe":
            return cmd_probe(args)
        if args.command == "read":
            return cmd_read(args)
        if args.command == "status":
            return cmd_status(args)
        return cmd_reset(args)
    except PermissionError:
        print("Permission denied on /dev/i2c-* - run with sudo.", file=sys.stderr)
        return 1
    except (OSError, IOError) as exc:
        print("I2C error: %s" % exc, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
