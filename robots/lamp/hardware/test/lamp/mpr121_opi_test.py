#!/usr/bin/env python3
"""MPR121 capacitive touch test for OrangePi (12 electrodes, I2C).

Linux/OrangePi counterpart of src/main.cpp (which targets an ESP32-C3).
Pure stdlib - talks to /dev/i2c-* through the I2C_RDWR ioctl, so it needs
no smbus/smbus2/i2c-tools package.

Wiring on the 40-pin header:
    pin 1  -> VCC (3.3V, NOT 5V)
    pin 3  -> SDA
    pin 5  -> SCL
    pin 6  -> GND

Usage:
    sudo ./mpr121_opi_test.py scan          # list every I2C device on every bus
    sudo ./mpr121_opi_test.py calibrate     # measure idle noise, recommend thresholds
    sudo ./mpr121_opi_test.py test          # stream touch/release events
    sudo ./mpr121_opi_test.py raw           # live per-electrode delta from baseline
    sudo ./mpr121_opi_test.py test --touch 20 --release 10 --debounce 3 --ignore 0,1,2

Phantom touches: run `calibrate` with hands off the pads. It reports how far
each electrode wanders at idle and prints thresholds that clear that with
margin. Then re-run `test` with those values and confirm real touches still
register. Unconnected electrodes read a flat 0 and should be --ignore'd.
"""

import argparse
import ctypes
import glob
import os
import sys
import time

# --- Linux i2c-dev ioctl plumbing -------------------------------------------

I2C_SLAVE = 0x0703
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


class I2CBus:
    """Minimal I2C master using repeated-start transfers."""

    def __init__(self, bus):
        self.bus = bus
        self.path = "/dev/i2c-%d" % bus
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

    def read_regs(self, addr, reg, length):
        """Write the register pointer, then read with a repeated start."""
        wbuf = (ctypes.c_uint8 * 1)(reg)
        rbuf = (ctypes.c_uint8 * length)()
        self._rdwr([
            I2CMsg(addr=addr, flags=0, len=1, buf=wbuf),
            I2CMsg(addr=addr, flags=I2C_M_RD, len=length, buf=rbuf),
        ])
        return bytes(rbuf)

    def write_reg(self, addr, reg, value):
        wbuf = (ctypes.c_uint8 * 2)(reg, value)
        self._rdwr([I2CMsg(addr=addr, flags=0, len=2, buf=wbuf)])

    def probe(self, addr):
        """True if a device ACKs at this address."""
        try:
            rbuf = (ctypes.c_uint8 * 1)()
            self._rdwr([I2CMsg(addr=addr, flags=I2C_M_RD, len=1, buf=rbuf)])
            return True
        except OSError:
            return False


# --- MPR121 ------------------------------------------------------------------

TOUCHSTATUS_L = 0x00
OORSTATUS_L = 0x02      # out-of-range: autoconfig failed for that electrode
FILTDATA_0L = 0x04
BASELINE_0 = 0x1E
MHDR = 0x2B
TOUCHTH_0 = 0x41
RELEASETH_0 = 0x42
DEBOUNCE = 0x5B
CONFIG1 = 0x5C
CONFIG2 = 0x5D
ECR = 0x5E
AUTOCONFIG0 = 0x7B
UPLIMIT = 0x7D
LOWLIMIT = 0x7E
TARGETLIMIT = 0x7F
SOFTRESET = 0x80

NUM_ELECTRODES = 12


class MPR121:
    def __init__(self, bus, addr=0x5A):
        self.bus = bus
        self.addr = addr

    def _w(self, reg, val):
        self.bus.write_reg(self.addr, reg, val)

    def begin(self, touch_th=12, release_th=6, autoconfig=True,
              debounce=2, sfi=2, esi=0):
        """Mirror Adafruit_MPR121::begin() + setAutoconfig(), with the three
        knobs that decide how noise-tolerant the chip is exposed:

        debounce  0-7  consecutive samples a change must persist before the
                       touch/release bit flips (reg 0x5B, same value applied
                       to both). 0 = a single noisy sample fires an event.
        sfi       0-3  second-level filter: 4 / 6 / 10 / 18 samples averaged
                       per reading (CONFIG2 bits 4:3). More = less noise.
        esi       0-7  sample interval 1ms << esi (CONFIG2 bits 2:0). Larger
                       = slower response but even more averaging.
        """
        self._w(SOFTRESET, 0x63)
        time.sleep(0.001)

        # All config writes must happen in stop mode (ECR = 0).
        self._w(ECR, 0x00)

        cfg2 = self.bus.read_regs(self.addr, CONFIG2, 1)[0]
        if cfg2 != 0x24:
            raise RuntimeError(
                "CONFIG2 reads 0x%02X, expected 0x24 - not an MPR121 at 0x%02X"
                % (cfg2, self.addr)
            )

        self.set_thresholds(touch_th, release_th)

        # Baseline filter tuning (Adafruit defaults).
        for reg, val in (
            (MHDR, 0x01), (0x2C, 0x01), (0x2D, 0x0E), (0x2E, 0x00),
            (0x2F, 0x01), (0x30, 0x05), (0x31, 0x01), (0x32, 0x00),
            (0x33, 0x00), (0x34, 0x00), (0x35, 0x00),
        ):
            self._w(reg, val)

        debounce = max(0, min(7, debounce))
        self._w(DEBOUNCE, (debounce << 4) | debounce)
        self._w(CONFIG1, 0x10)  # 16uA charge current
        # CDT=001 (0.5us charge time) | SFI | ESI
        self._w(CONFIG2, 0x20 | ((sfi & 3) << 3) | (esi & 7))

        if autoconfig:
            # Limits for a 3.3V supply: up = ((3.3-0.7)/3.3)*256
            self._w(UPLIMIT, 200)
            self._w(TARGETLIMIT, 180)
            self._w(LOWLIMIT, 130)
            self._w(AUTOCONFIG0, 0x0B)

        # Start: baseline tracking enabled, all 12 electrodes on.
        self._w(ECR, 0x8F)

    def set_thresholds(self, touch, release):
        for i in range(NUM_ELECTRODES):
            self._w(TOUCHTH_0 + 2 * i, touch)
            self._w(RELEASETH_0 + 2 * i, release)

    def touched(self):
        data = self.bus.read_regs(self.addr, TOUCHSTATUS_L, 2)
        return ((data[1] << 8) | data[0]) & 0x0FFF

    def out_of_range(self):
        data = self.bus.read_regs(self.addr, OORSTATUS_L, 2)
        return ((data[1] << 8) | data[0]) & 0x0FFF

    def all_filtered(self):
        """All 12 filtered values in one transaction (24 bytes from 0x04)."""
        data = self.bus.read_regs(self.addr, FILTDATA_0L, 24)
        return [data[2 * i] | (data[2 * i + 1] << 8) for i in range(NUM_ELECTRODES)]

    def all_baseline(self):
        data = self.bus.read_regs(self.addr, BASELINE_0, NUM_ELECTRODES)
        return [b << 2 for b in data]

    def filtered_data(self, i):
        data = self.bus.read_regs(self.addr, FILTDATA_0L + 2 * i, 2)
        return (data[1] << 8) | data[0]

    def baseline_data(self, i):
        return self.bus.read_regs(self.addr, BASELINE_0 + i, 1)[0] << 2


# --- helpers -----------------------------------------------------------------

def bus_name(bus_num):
    try:
        with open("/sys/class/i2c-adapter/i2c-%d/name" % bus_num) as fh:
            return fh.read().strip()
    except OSError:
        return ""


def list_buses(include_hdmi=False):
    """All /dev/i2c-* bus numbers, sorted.

    The HDMI DDC adapter is skipped unless asked for: it is never where a
    sensor lives, and with no monitor attached every probe on it has to wait
    out the controller timeout, which makes a full scan look hung.
    """
    buses = []
    for path in glob.glob("/dev/i2c-*"):
        try:
            n = int(path.rsplit("-", 1)[1])
        except ValueError:
            continue
        if not include_hdmi and "HDMI" in bus_name(n).upper():
            continue
        buses.append(n)
    return sorted(buses)


def scan_bus(bus_num):
    """Return the list of addresses that ACK on this bus."""
    try:
        bus = I2CBus(bus_num)
    except OSError as exc:
        return None, str(exc)
    found = []
    try:
        for addr in range(0x08, 0x78):
            if bus.probe(addr):
                found.append(addr)
    finally:
        bus.close()
    return found, None


def find_mpr121(preferred_addr=None):
    """Locate an MPR121 across every bus. Returns (bus_num, addr) or (None, None).

    Probes only the four MPR121 addresses per bus rather than the whole 0x08-0x77
    range - a full scan is ~780 ioctls, each of which waits out a bus timeout on
    an idle bus, which takes long enough to look like a hang.
    """
    candidates = [preferred_addr] if preferred_addr else [0x5A, 0x5B, 0x5C, 0x5D]
    buses = list_buses()
    print("Searching for MPR121 on %d bus(es): %s"
          % (len(buses), ", ".join(str(b) for b in buses)), flush=True)
    for bus_num in buses:
        try:
            bus = I2CBus(bus_num)
        except OSError:
            continue
        try:
            for addr in candidates:
                if bus.probe(addr):
                    return bus_num, addr
        finally:
            bus.close()
    return None, None


def cmd_scan(args):
    buses = list_buses(include_hdmi=args.all)
    if not buses:
        print("No /dev/i2c-* devices found.")
        return 1
    print("Scanning %d bus(es): %s%s\n" % (
        len(buses), ", ".join(str(b) for b in buses),
        "" if args.all else "  (HDMI DDC bus skipped; --all to include)"))
    any_found = False
    for bus_num in buses:
        print("/dev/i2c-%-2d  scanning..." % bus_num, end="\r", flush=True)
        found, err = scan_bus(bus_num)
        if err:
            print("/dev/i2c-%-2d  open failed: %s" % (bus_num, err))
            continue
        if found:
            any_found = True
            labels = []
            for a in found:
                tag = " <-- MPR121" if 0x5A <= a <= 0x5D else ""
                labels.append("0x%02x%s" % (a, tag))
            print("/dev/i2c-%-2d  %s" % (bus_num, ", ".join(labels)))
        else:
            print("/dev/i2c-%-2d  -" % bus_num)
    if not any_found:
        print("\nNothing responded on any bus. Check:")
        print("  * VCC on pin 1 (3.3V) and GND on pin 6")
        print("  * SDA on pin 3, SCL on pin 5 (not swapped)")
        print("  * pull-ups present (most MPR121 breakouts have them)")
    return 0


def resolve_target(args):
    if args.bus is not None:
        return args.bus, args.addr or 0x5A
    bus_num, addr = find_mpr121(args.addr)
    if bus_num is None:
        print("\nNo MPR121 found on any bus.", file=sys.stderr)
        if not os.path.exists("/dev/i2c-0"):
            print(
                "/dev/i2c-0 does not exist, so header pins 3 (SDA) and 5 (SCL) have\n"
                "no I2C controller behind them. Enable TWI0 by adding the i2c0 overlay\n"
                "to /boot/orangepiEnv.txt and rebooting:\n"
                "    overlays=<existing entries> i2c0\n"
                "Then re-run this script.", file=sys.stderr)
        else:
            print("Run '%s scan' to see what is on each bus."
                  % os.path.basename(sys.argv[0]), file=sys.stderr)
        sys.exit(1)
    print("Auto-detected MPR121 on /dev/i2c-%d at 0x%02x" % (bus_num, addr))
    return bus_num, addr


def parse_electrodes(spec):
    if not spec:
        return set()
    out = set()
    for tok in spec.replace(",", " ").split():
        n = int(tok)
        if not 0 <= n < NUM_ELECTRODES:
            raise SystemExit("electrode %d out of range 0-%d" % (n, NUM_ELECTRODES - 1))
        out.add(n)
    return out


def open_and_begin(args):
    bus_num, addr = resolve_target(args)
    bus = I2CBus(bus_num)
    cap = MPR121(bus, addr)
    cap.begin(touch_th=args.touch, release_th=args.release,
              autoconfig=not args.no_autoconfig,
              debounce=args.debounce, sfi=args.sfi, esi=args.esi)
    ignore = parse_electrodes(args.ignore)
    print("MPR121 ready: touch=%d release=%d debounce=%d sfi=%d esi=%d%s"
          % (args.touch, args.release, args.debounce, args.sfi, args.esi,
             ("  ignoring %s" % sorted(ignore)) if ignore else ""))
    # Give autoconfig + the baseline filter a moment, then report any electrode
    # the chip itself gave up on.
    time.sleep(0.1)
    oor = cap.out_of_range()
    if oor:
        bad = [i for i in range(NUM_ELECTRODES) if oor & (1 << i)]
        print("WARNING: autoconfig failed (out of range) on electrodes %s -\n"
              "         they are unconnected, shorted, or outside the charge\n"
              "         limits. Expect junk from them; add them to --ignore."
              % bad)
    return bus, cap, ignore


def cmd_test(args):
    bus, cap, ignore = open_and_begin(args)
    mask = 0x0FFF
    for i in ignore:
        mask &= ~(1 << i)
    print("Touch the pads; Ctrl-C to stop.\n")

    last = 0
    try:
        while True:
            curr = cap.touched() & mask
            if curr != last and args.verbose:
                filt = cap.all_filtered()
                base = cap.all_baseline()
            for i in range(NUM_ELECTRODES):
                bit = 1 << i
                if (curr & bit) and not (last & bit):
                    extra = ("  (delta %d)" % (base[i] - filt[i])) if args.verbose else ""
                    print("%2d touched%s" % (i, extra), flush=True)
                elif not (curr & bit) and (last & bit):
                    print("%2d released" % i, flush=True)
            last = curr
            time.sleep(0.01)
    except KeyboardInterrupt:
        print("\nstopped")
    finally:
        bus.close()
    return 0


def cmd_raw(args):
    bus, cap, ignore = open_and_begin(args)
    print("Per-electrode  baseline-filtered delta  (touch = delta rises above the\n"
          "touch threshold; * = chip reports touched). Ctrl-C to stop.\n")
    try:
        while True:
            touched = cap.touched()
            filt = cap.all_filtered()
            base = cap.all_baseline()
            cells = []
            for i in range(NUM_ELECTRODES):
                if i in ignore:
                    cells.append("%2d:  --- " % i)
                    continue
                mark = "*" if touched & (1 << i) else " "
                cells.append("%2d:%5d%s" % (i, base[i] - filt[i], mark))
            print(" ".join(cells), flush=True)
            time.sleep(0.25)
    except KeyboardInterrupt:
        print("\nstopped")
    finally:
        bus.close()
    return 0


def cmd_calibrate(args):
    """Measure the idle noise per electrode and recommend thresholds.

    The chip flags a touch when (baseline - filtered) exceeds the touch
    threshold. So the question is simply: with nobody touching, how far does
    that delta wander? Thresholds must sit well clear of that.
    """
    bus, cap, ignore = open_and_begin(args)
    secs = args.seconds
    print("\nRecording %d s of idle noise. DO NOT TOUCH the pads.\n" % secs)
    # let the baseline filter settle before we start judging it
    time.sleep(1.0)

    samples = [[] for _ in range(NUM_ELECTRODES)]
    events = [0] * NUM_ELECTRODES
    last = 0
    t_end = time.time() + secs
    n = 0
    try:
        while time.time() < t_end:
            filt = cap.all_filtered()
            base = cap.all_baseline()
            curr = cap.touched()
            for i in range(NUM_ELECTRODES):
                samples[i].append(base[i] - filt[i])
                if (curr & (1 << i)) and not (last & (1 << i)):
                    events[i] += 1
            last = curr
            n += 1
            if n % 20 == 0:
                print("  %3d s left, %d samples" % (t_end - time.time(), n),
                      end="\r", flush=True)
            time.sleep(0.02)
    except KeyboardInterrupt:
        print("\ninterrupted - report is on partial data")
    finally:
        bus.close()

    print("\n%-4s %6s %6s %6s %6s %7s  %s" %
          ("ele", "min", "max", "mean", "p2p", "spurious", "verdict"))
    print("-" * 60)
    worst_noise = 0
    dead, noisy = [], []
    for i in range(NUM_ELECTRODES):
        d = samples[i]
        if not d:
            continue
        lo, hi = min(d), max(d)
        mean = sum(d) / len(d)
        p2p = hi - lo
        if i in ignore:
            verdict = "ignored"
        elif all(v == 0 for v in d):
            # delta stuck at exactly 0 the whole time = no signal at all
            verdict = "DEAD - no electrode connected"
            dead.append(i)
        elif events[i]:
            verdict = "NOISY - fired %d phantom touch(es)" % events[i]
            noisy.append(i)
            worst_noise = max(worst_noise, hi)
        else:
            verdict = "ok"
            worst_noise = max(worst_noise, hi)
        print("%-4d %6d %6d %6.1f %6d %7d  %s" % (i, lo, hi, mean, p2p, events[i], verdict))

    # Recommendation: touch clear of the worst idle excursion by a margin,
    # release at roughly half so a held touch does not chatter.
    rec_touch = max(6, int(worst_noise * 2.5) + 3)
    rec_release = max(3, rec_touch // 2)
    print()
    print("worst idle excursion on a live electrode: %d counts" % worst_noise)
    print("current thresholds: touch=%d release=%d" % (args.touch, args.release))
    print("recommended:        touch=%d release=%d debounce=%d"
          % (rec_touch, rec_release, max(args.debounce, 2)))
    if dead:
        print("dead electrodes (add to --ignore): %s" % ",".join(str(i) for i in dead))
    cmd = "%s test --touch %d --release %d --debounce %d" % (
        os.path.basename(sys.argv[0]), rec_touch, rec_release, max(args.debounce, 2))
    if dead or ignore:
        cmd += " --ignore %s" % ",".join(str(i) for i in sorted(set(dead) | ignore))
    print("\nnext:  sudo ./%s" % cmd)
    print("then touch a pad and check it still registers; if a real touch\n"
          "produces a delta near the threshold, lower --touch a little.")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["scan", "test", "raw", "calibrate"])
    parser.add_argument("--bus", type=int, default=None,
                        help="I2C bus number (default: auto-detect)")
    parser.add_argument("--addr", type=lambda s: int(s, 0), default=None,
                        help="I2C address (default: probe 0x5a-0x5d)")
    parser.add_argument("--touch", type=int, default=12,
                        help="touch threshold in counts (default 12; the ESP32 "
                             "sample's 2 sits inside the noise floor)")
    parser.add_argument("--release", type=int, default=6,
                        help="release threshold (default 6)")
    parser.add_argument("--debounce", type=int, default=2,
                        help="consecutive samples before a state change counts, "
                             "0-7 (default 2)")
    parser.add_argument("--sfi", type=int, default=2, choices=[0, 1, 2, 3],
                        help="samples averaged per reading: 0=4 1=6 2=10 3=18 "
                             "(default 2)")
    parser.add_argument("--esi", type=int, default=0, choices=range(0, 8),
                        help="sample interval 1ms<<esi (default 0 = 1ms)")
    parser.add_argument("--ignore", default="",
                        help="electrodes to mask out, e.g. 0,1,2")
    parser.add_argument("--seconds", type=int, default=10,
                        help="calibrate: idle recording length (default 10)")
    parser.add_argument("--verbose", action="store_true",
                        help="test: print the delta that triggered each touch")
    parser.add_argument("--no-autoconfig", action="store_true",
                        help="skip MPR121 autoconfiguration")
    parser.add_argument("--all", action="store_true",
                        help="scan: include the HDMI DDC bus (slow)")
    args = parser.parse_args()

    try:
        if args.command == "scan":
            return cmd_scan(args)
        if args.command == "test":
            return cmd_test(args)
        if args.command == "calibrate":
            return cmd_calibrate(args)
        return cmd_raw(args)
    except PermissionError:
        print("Permission denied on /dev/i2c-* - run with sudo.", file=sys.stderr)
        return 1
    except OSError as exc:
        print("I2C error: %s" % exc, file=sys.stderr)
        return 1
    except RuntimeError as exc:
        print("Error: %s" % exc, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
