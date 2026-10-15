#!/usr/bin/env python3
"""mpr121_tune.py - factory (bench) calibration of the lamp's MPR121 touch strip.

Runs ON THE LAMP as root (lamp-remote-test --tune copies it and starts it). Writes the
per-unit factory layer that HAL merges over the device package and keeps across updates:

    /var/lib/hal/mpr121.factory.json   {"touch_thresholds": [12], "release_thresholds": [12], ...}

Steps: hands off (idle noise per pad) -> suggested thresholds -> slide over all 12 pads
(each must register, weak pads flagged) -> accept / adjust / redo -> save -> 60 s hands-off
soak (any touch = phantom = FAIL, offers redo) -> report in ~/hwtest/mpr121/.

Backends:
  direct (default) - hal.service is stopped, so touches during calibration trigger nothing on
           the lamp; the chip is driven over I2C with HAL's own register setup, and hal is
           started again at the end (if it was running). The saved file is used by any HAL with
           tuning layers.
  api    - --backend api: hal keeps running and the touch API measures what HAL sees.

Keys at the save prompt (the hub button sends the same keys):
  y / Enter / 2 clicks   accept and save        n / 1 click   redo from the idle measurement
  c                      check touches again    t 8 | t 3 10  touch all / pad 3   r 4 | r 3 5  release
"""

import argparse
import ctypes
import json
import os
import pwd
import select
import subprocess
import sys
import termios
import time
import tty
import urllib.error
import urllib.request

PADS = 12
HAL = "http://127.0.0.1:5001"
FACTORY_PATH = "/var/lib/hal/mpr121.factory.json"
USER_PATH = "/var/lib/hal/mpr121.user.json"
MIN_TOUCH = 6            # same rule as HAL POST /touch/calibrate
TOUCHED_NOISE = 30       # idle noise this high = someone touched the strip
WEAK_MARGIN = 4          # a real touch should beat its threshold by at least this
# Right after a chip reset the filtered data reads ~0 for a moment (delta ~700); a finger
# gives tens of counts, so frames with a delta above this are reset transients and skipped.
MAX_REAL_DELTA = 200


def valid(delta):
    return all(-MAX_REAL_DELTA < d < MAX_REAL_DELTA for d in delta)
FFI = {6: 0, 10: 1, 18: 2, 34: 3}
SFI = {4: 0, 6: 1, 10: 2, 18: 3}
ESI = {1: 0, 2: 1, 4: 2, 8: 3, 16: 4, 32: 5, 64: 6, 128: 7}


def say(msg=""):
    print(msg, flush=True)


def suggest(noise):
    touch = [min(255, max(MIN_TOUCH, 2 * n + 2)) for n in noise]
    return touch, [max(2, round(t / 2)) for t in touch]


# ---------------------------------------------------------------- keyboard (raw single keys)
class Keys:
    def __init__(self):
        self.fd = sys.stdin.fileno() if sys.stdin.isatty() else None
        self.old = termios.tcgetattr(self.fd) if self.fd is not None else None

    def __enter__(self):
        if self.fd is not None:
            tty.setcbreak(self.fd)
        return self

    def __exit__(self, *exc):
        self.restore()

    def restore(self):
        if self.fd is not None:
            termios.tcsetattr(self.fd, termios.TCSADRAIN, self.old)

    def key(self, timeout):
        # os.read on the fd, never sys.stdin.read: that buffers the rest of a typed line
        # inside Python, where select() and the next read cannot see it.
        if self.fd is None:
            time.sleep(timeout)
            return None
        ready, _, _ = select.select([self.fd], [], [], timeout)
        return os.read(self.fd, 1).decode(errors="replace") if ready else None

    def line(self, first):
        """Echo `first`, then read the rest of a line key by key (backspace works)."""
        text = first
        print(first, end="", flush=True)
        while True:
            k = self.key(3600)
            if k is None:
                continue
            if k in ("\n", "\r"):
                print(flush=True)
                return text
            if k in ("\x7f", "\b"):
                if len(text) > 0:
                    text = text[:-1]
                    print("\b \b", end="", flush=True)
                continue
            text += k
            print(k, end="", flush=True)


def flush_input():
    try:
        termios.tcflush(sys.stdin.fileno(), termios.TCIFLUSH)
    except (termios.error, ValueError, OSError):
        pass


# ---------------------------------------------------------------- HAL API backend
class ApiBackend:
    name = "api"

    def __init__(self, config):
        self.config = config
        self.order = config.get("swipe_axis") or list(range(PADS))

    @staticmethod
    def probe():
        try:
            with urllib.request.urlopen(f"{HAL}/touch/config", timeout=3) as r:
                return json.load(r)
        except (urllib.error.URLError, OSError, ValueError):
            return None

    def _post(self, path, body=None, timeout=10):
        req = urllib.request.Request(f"{HAL}{path}", method="POST",
                                     data=json.dumps(body or {}).encode(),
                                     headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            detail = e.read().decode(errors="replace")
            try:
                detail = json.loads(detail).get("detail", detail)
            except ValueError:
                pass
            raise RuntimeError(f"HAL {path}: {e.code} {detail}") from e

    def noise(self, seconds):
        try:
            return self._post("/touch/calibrate", {"seconds": seconds, "save": False}, timeout=seconds + 15)["noise"]
        except RuntimeError as e:
            if " 409 " in str(e):               # touched during the measurement
                return None
            raise

    def apply(self, touch, release):
        # The candidate goes into the factory layer right away so HAL measures with it;
        # the previous file is restored by restore_backup() if the operator aborts.
        write_json(FACTORY_PATH, {"touch_thresholds": touch, "release_thresholds": release,
                                  "source": "bench-candidate"})
        if os.path.exists(USER_PATH):            # a user layer would override the candidate
            os.remove(USER_PATH)
        self._post("/touch/reload")

    def frames(self, seconds):
        """Yield (delta, touched) about 10 times a second for `seconds`."""
        end = time.monotonic() + seconds
        with urllib.request.urlopen(f"{HAL}/touch/stream", timeout=5) as r:
            for raw in r:
                if time.monotonic() > end:
                    return
                line = raw.decode(errors="replace").strip()
                if not line.startswith("data:"):
                    continue
                frame = json.loads(line[5:])
                if frame.get("ready"):
                    yield frame["delta"], frame["touched"]

    def saved(self):
        self._post("/touch/reload")

    def finish(self):
        pass


# ---------------------------------------------------------------- direct I2C backend
class _Msg(ctypes.Structure):
    _fields_ = [("addr", ctypes.c_uint16), ("flags", ctypes.c_uint16),
                ("len", ctypes.c_uint16), ("buf", ctypes.POINTER(ctypes.c_uint8))]


class _Data(ctypes.Structure):
    _fields_ = [("msgs", ctypes.POINTER(_Msg)), ("nmsgs", ctypes.c_uint32)]


class I2C:
    """Repeated-start I2C (I2C_RDWR) like HAL's transport; stdlib only."""

    def __init__(self, bus, addr):
        self.addr = addr
        self.libc = ctypes.CDLL("libc.so.6", use_errno=True)
        self.libc.ioctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_void_p]
        self.fd = os.open(f"/dev/i2c-{bus}", os.O_RDWR)

    def _xfer(self, msgs):
        arr = (_Msg * len(msgs))(*msgs)
        if self.libc.ioctl(self.fd, 0x0707, ctypes.byref(_Data(arr, len(msgs)))) < 0:
            err = ctypes.get_errno()
            raise OSError(err, os.strerror(err))

    def read(self, reg, length):
        ptr, buf = (ctypes.c_uint8 * 1)(reg), (ctypes.c_uint8 * length)()
        self._xfer([_Msg(self.addr, 0, 1, ptr), _Msg(self.addr, 1, length, buf)])
        return bytes(buf)

    def write(self, reg, value):
        buf = (ctypes.c_uint8 * 2)(reg, value)
        self._xfer([_Msg(self.addr, 0, 2, buf)])

    def close(self):
        os.close(self.fd)


def package_entry():
    """The lamp's MPR121 board entry from the device package (HAL's register inputs)."""
    env = {}
    try:
        for line in open("/opt/hal/.env"):
            if "=" in line and not line.lstrip().startswith("#"):
                k, v = line.strip().split("=", 1)
                env[k] = v.strip().strip('"')
    except OSError:
        pass
    path = os.path.join(env.get("DEVICES_DIR", "/opt/devices"), env.get("DEVICE_TYPE", "lamp"), "mpr121.json")
    boards = json.load(open(path))["boards"]
    entry = boards.get("orangepi_sun60") or next(v for v in boards.values() if v and v.get("enabled", True))
    return entry, path


class DirectBackend:
    name = "direct"

    def __init__(self):
        self.entry, self.path = package_entry()
        self.order = self.entry.get("swipe_axis") or list(range(PADS))
        self.hal_was_active = subprocess.run(["systemctl", "is-active", "--quiet", "hal"]).returncode == 0
        if self.hal_was_active:
            say("  stopping hal.service (it owns the I2C chip) - started again at the end")
            subprocess.run(["systemctl", "stop", "hal"], check=False)
        self.bus = I2C(self.entry.get("bus", 0), self.entry.get("address", 0x5A))
        t, r = self.entry.get("touch_threshold", 6), self.entry.get("release_threshold", 4)
        self.apply([t] * PADS, [r] * PADS)

    def apply(self, touch, release):
        """HAL's _initialize() register sequence with per-pad thresholds."""
        e, w = self.entry, self.bus.write
        ffi = FFI[e.get("ffi", 6)]
        w(0x80, 0x63)
        time.sleep(0.001)
        w(0x5E, 0x00)
        for i in range(PADS):
            w(0x41 + 2 * i, touch[i])
            w(0x42 + 2 * i, release[i])
        for reg, val in ((0x2B, 1), (0x2C, 1), (0x2D, 14), (0x2E, 0),
                         (0x2F, 1), (0x30, e.get("nhdf", 1)), (0x31, e.get("nclf", 0xFF)), (0x32, e.get("fdlf", 2)),
                         (0x33, e.get("nhdt", 0)), (0x34, e.get("nclt", 0)), (0x35, e.get("fdlt", 0)),
                         (0x5B, e.get("chip_debounce", 0) << 4 | e.get("chip_debounce", 0)), (0x5C, ffi << 6 | 0x10),
                         (0x5D, 0x20 | SFI[e.get("sfi", 10)] << 3 | ESI[e.get("esi_ms", 1)])):
            w(reg, val)
        if e.get("autoconfig", True):
            for reg, val in ((0x7D, 200), (0x7F, 180), (0x7E, 130), (0x7B, ffi << 6 | 0x0B)):
                w(reg, val)
        w(0x5E, 0x8F)
        self.settle()

    def settle(self, limit=6, timeout=20):
        """After a reset the chip loads only the 5 high bits of each reading as its baseline
        (ECR CL=10), so the baseline starts up to ~31 counts off and tracking pulls it in over
        seconds. Measuring before that reads the offset as noise; wait until it is gone."""
        time.sleep(0.3)
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            delta, _ = self._sample()
            off = max(abs(d) for d in delta)
            if valid(delta) and off <= limit:
                break
            print(f"\r\033[K  settling baseline after chip reset: max offset {off:>3}", end="", flush=True)
            time.sleep(0.2)
        print("\r\033[K", end="", flush=True)

    def _sample(self):
        status = int.from_bytes(self.bus.read(0x00, 2), "little")
        raw = self.bus.read(0x04, 24)
        base = self.bus.read(0x1E, 12)
        delta = [(base[i] << 2) - (int.from_bytes(raw[2 * i:2 * i + 2], "little") & 0x3FF) for i in range(PADS)]
        return delta, [bool(status & (1 << i)) for i in range(PADS)]

    def noise(self, seconds):
        peak, end = [0] * PADS, time.monotonic() + seconds
        while time.monotonic() < end:
            delta, _ = self._sample()
            if valid(delta):
                # only baseline - filtered > 0 can become a (phantom) touch
                peak = [max(p, d) for p, d in zip(peak, delta)]
            time.sleep(0.05)
        return None if max(peak) >= TOUCHED_NOISE else peak

    def frames(self, seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            yield self._sample()
            time.sleep(0.05)

    def saved(self):
        pass

    def finish(self):
        try:
            self.bus.close()
        except OSError:
            pass
        if self.hal_was_active:
            say("  starting hal.service again")
            subprocess.run(["systemctl", "start", "hal"], check=False)


# ---------------------------------------------------------------- files
def write_json(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = f"{path}.tmp"
    with open(tmp, "w") as f:
        json.dump(data, f, indent=1)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


class Backup:
    """Keeps the factory/user layers as they were, to put back if the run is aborted."""

    def __init__(self):
        self.saved = {p: (open(p).read() if os.path.exists(p) else None) for p in (FACTORY_PATH, USER_PATH)}

    def restore(self):
        for path, text in self.saved.items():
            if text is None:
                if os.path.exists(path):
                    os.remove(path)
            else:
                with open(path, "w") as f:
                    f.write(text)


# ---------------------------------------------------------------- steps
def table(order, rows):
    say("  pad     " + " ".join(f"{p:>3}" for p in order))
    for label, values in rows:
        if values is not None:
            say(f"  {label:<7} " + " ".join(f"{values[p]:>3}" for p in order))


def measure_noise(backend, seconds, keys):
    while True:
        say(f"\n[1/3] HANDS OFF the touch strip - measuring idle noise for {seconds} s ...")
        flush_input()
        noise = backend.noise(seconds)
        if noise is not None:
            return noise
        say("  the strip was touched during the measurement (noise >= 30) - again in 3 s")
        time.sleep(3)


def touch_check(backend, touch, keys, timeout, passes=3):
    say(f"\n[2/3] SWIPE along the whole strip {passes} times (either way).")
    say(f"  digit = times each pad registered; ends at {passes} each, Enter or {timeout} s")
    hits, best, was = [0] * PADS, [0] * PADS, [False] * PADS
    order = backend.order
    flush_input()
    done_at = None
    for delta, touched in backend.frames(timeout):
        if not valid(delta):
            continue
        for i in range(PADS):
            best[i] = max(best[i], delta[i])
            if touched[i] and not was[i]:
                hits[i] += 1                     # one registration per press of this pad
            was[i] = touched[i]
        bar = "".join("#" if touched[p] else str(min(hits[p], 9)) for p in order)
        swipes = min(hits)
        # short enough for a half-width LCD pane: a wrapped line cannot be redrawn with \r
        print(f"\r\033[K  [{bar}] swipes {min(swipes, passes)}/{passes}", end="", flush=True)
        if swipes >= passes and done_at is None:
            done_at = time.monotonic()
        if done_at and time.monotonic() - done_at > 0.8:
            break
        k = keys.key(0)
        if k in ("\n", "\r", "s"):
            break
    print()
    missing = [p for p in order if hits[p] == 0]
    few = [p for p in order if 0 < hits[p] < passes]
    weak = [p for p in order if hits[p] and best[p] < touch[p] + WEAK_MARGIN]
    if few:
        say(f"  registered fewer than {passes} times: pads {few} (counts {[hits[p] for p in few]})")
    return best, missing, weak


def soak(backend, seconds, keys):
    say(f"\n[3/3] HANDS OFF again - {seconds} s phantom-touch check (HAL thresholds in effect) ...")
    flush_input()
    phantom = {}
    start = time.monotonic()
    for delta, touched in backend.frames(seconds):
        if not valid(delta):
            continue
        for i in range(PADS):
            if touched[i]:
                phantom[i] = phantom.get(i, 0) + 1
        left = seconds - (time.monotonic() - start)
        print(f"\r  {max(0, left):4.0f} s left   phantom touches: {sum(phantom.values())} ", end="", flush=True)
    print()
    return phantom


def prompt(keys, question):
    """Single-key answer; t/r start an adjustment line. The hub button looks for '[Y/n]'."""
    flush_input()
    print(f"{question} [Y/n] ", end="", flush=True)
    while True:
        k = keys.key(3600)
        if k is None:
            continue
        if k in ("y", "Y", "\n", "\r"):
            print("-> Y")
            return "y"
        if k in ("n", "N"):
            print("-> N")
            return "n"
        if k in ("c", "C"):
            print("-> check again")
            return "c"
        if k in ("t", "r", "T", "R"):
            print()
            return keys.line(k.lower())


def adjust(cmd, touch, release):
    """'t 8' / 't 3 10' / 'r 4' / 'r 3 5' -> new lists, or an error string."""
    parts = cmd.split()
    try:
        nums = [int(x) for x in parts[1:]]
    except ValueError:
        return "numbers please, e.g. t 8  or  t 3 10"
    target = touch if parts[0] == "t" else release
    if len(nums) == 1:
        target[:] = [nums[0]] * PADS
        if parts[0] == "t":
            release[:] = [max(2, round(v / 2)) for v in touch]
    elif len(nums) == 2 and 0 <= nums[0] < PADS:
        target[nums[0]] = nums[1]
        if parts[0] == "t":
            release[nums[0]] = max(2, round(nums[1] / 2))
    else:
        return "use  t 8  (all pads)  or  t 3 10  (pad 3)"
    for i in range(PADS):
        if not 0 <= release[i] < touch[i] <= 255:
            return f"pad {i}: need 0 <= release < touch <= 255"
    return None


def report(name, backend, result, out_dir):
    ts = time.strftime("%Y%m%d-%H%M%S")
    os.makedirs(out_dir, exist_ok=True)
    base = os.path.join(out_dir, f"{name}-mpr121-{ts}")
    order = backend.order
    lines = [f"# MPR121 factory calibration - {name}", "",
             f"- date: {time.strftime('%Y-%m-%d %H:%M:%S')} (lamp clock)",
             f"- backend: {backend.name}", f"- colour: {result.get('color') or '?'}", f"- result: **{result['result']}**",
             f"- saved: `{FACTORY_PATH}`" if result.get("saved") else "- saved: no", "",
             "| pad | " + " | ".join(str(p) for p in order) + " |",
             "|---|" + "---|" * PADS]
    for label in ("noise", "touch", "release", "best"):
        values = result.get(label)
        if values:
            lines.append(f"| {label} | " + " | ".join(str(values[p]) for p in order) + " |")
    for label in ("missing", "weak", "phantom"):
        if result.get(label):
            lines.append(f"\n- {label}: {result[label]}")
    with open(base + ".md", "w") as f:
        f.write("\n".join(lines) + "\n")
    with open(base + ".json", "w") as f:
        json.dump({"name": name, "backend": backend.name, **result}, f, indent=1)
    owner = os.environ.get("SUDO_USER")
    if owner:
        pw = pwd.getpwnam(owner)
        for path in (out_dir, base + ".md", base + ".json"):
            os.chown(path, pw.pw_uid, pw.pw_gid)
    return base + ".md"


WATCH_DONE = "/tmp/mpr121-watch.done"
WATCH_STOP = "/tmp/mpr121-watch.stop"


def led(path, body):
    """Best-effort HAL LED call (the ring is the operator's status light during watches)."""
    try:
        req = urllib.request.Request(f"{HAL}{path}", method="POST", data=json.dumps(body).encode(),
                                     headers={"Content-Type": "application/json"})
        urllib.request.urlopen(req, timeout=3).read()
        return True
    except (urllib.error.URLError, OSError):
        return False


def watch(name, seconds, out_dir, poweroff=False):
    """Hands-off phantom-touch watch with HAL running on the saved calibration."""
    for path in (WATCH_DONE, WATCH_STOP):
        if os.path.exists(path):
            os.remove(path)
    tty = sys.stdout.isatty()
    say(f"=== {name}: phantom-touch watch, {seconds // 60} min, hal running - DO NOT touch the lamp ===")
    say(f"  started {time.strftime('%Y-%m-%d %H:%M:%S')} (lamp clock)")
    # hal was just restarted by the calibration (or is disabled): start it and wait for the API
    if subprocess.run(["systemctl", "is-active", "--quiet", "hal"]).returncode != 0:
        say("  starting hal.service")
        subprocess.run(["systemctl", "start", "hal"], check=False)
    config, wait_end = None, time.monotonic() + 150
    while config is None and time.monotonic() < wait_end:
        config = ApiBackend.probe()
        if config is None:
            try:                                  # HAL up but without the touch API: stop waiting
                urllib.request.urlopen(f"{HAL}/device", timeout=2)
                break
            except (urllib.error.URLError, OSError):
                print(f"\r  waiting for hal ... {int(wait_end - time.monotonic())} s ", end="", flush=True)
                time.sleep(2)
    print()
    backend = ApiBackend(config) if config else None
    if backend is None:
        say("  HAL touch API not answering - watching the chip directly (hal stopped)")
        backend = DirectBackend()
        layer = {}
        try:
            layer = json.load(open(FACTORY_PATH))
        except (OSError, ValueError):
            pass
        if layer.get("touch_thresholds"):
            backend.apply(layer["touch_thresholds"], layer["release_thresholds"])
    order = backend.order
    touch = config["touch"] if config else (layer.get("touch_thresholds") or [0] * PADS)
    say("  touch   " + " ".join(f"{touch[p]:>3}" for p in order) + "   (pads " + ",".join(map(str, order)) + ")")
    events, peak, was = [], [0] * PADS, [False] * PADS
    start = time.monotonic()
    last_print = 0
    try:
        for delta, touched in backend.frames(seconds):
            if os.path.exists(WATCH_STOP):
                say("\n  stopped early")
                break
            if not valid(delta):
                continue
            now = time.monotonic()
            for i in range(PADS):
                peak[i] = max(peak[i], delta[i])
                if touched[i] and not was[i]:
                    events.append({"t": round(now - start, 1), "clock": time.strftime("%H:%M:%S"),
                                   "pad": i, "delta": delta[i]})
                    print(f"{chr(13) + chr(27) + '[K' if tty else ''}  {time.strftime('%H:%M:%S')}  PHANTOM touch pad {i:>2}  delta {delta[i]:>3}  "
                          f"(touch {touch[i] or '?'})", flush=True)
                was[i] = touched[i]
            # a terminal gets a live status line; a log file one line per minute
            if now - last_print >= (1 if tty else 60):
                last_print = now
                left = max(0, seconds - (now - start))
                status = (f" {int(left // 60):02d}:{int(left % 60):02d} left ph {len(events):>2} peak "
                          + " ".join(f"{peak[p]:>2}" for p in order))
                if tty:
                    print(f"\r\033[K{status}", end="", flush=True)
                else:
                    print(f"{time.strftime('%H:%M:%S')}{status}", flush=True)
    finally:
        backend.finish()
    print()
    elapsed = round(time.monotonic() - start)
    result = "PASS" if not events else "FAIL"
    say(f"  watched {elapsed // 60} min {elapsed % 60} s: {len(events)} phantom touch(es) -> {result}")
    if events:
        by_pad = {}
        for e in events:
            by_pad[e["pad"]] = by_pad.get(e["pad"], 0) + 1
        say(f"  per pad: {by_pad}   peak idle delta per pad: {dict(zip(order, [peak[p] for p in order]))}")
    os.makedirs(out_dir, exist_ok=True)
    path = os.path.join(out_dir, f"{name}-watch-{time.strftime('%Y%m%d-%H%M%S')}.json")
    with open(path, "w") as f:
        json.dump({"name": name, "result": result, "seconds": elapsed, "events": events,
                   "peak": peak, "touch": touch}, f, indent=1)
    owner = os.environ.get("SUDO_USER")
    if owner:
        pw = pwd.getpwnam(owner)
        os.chown(path, pw.pw_uid, pw.pw_gid)
    with open(WATCH_DONE, "w") as f:
        f.write(result + "\n")
    say(f"WATCH  : {name} {result} ({len(events)} phantom in {elapsed} s)")
    if poweroff and result == "PASS":
        # green for 5 s, then off: a dark lamp at the bench = passed, unplug it
        led("/led/solid", {"color": [0, 80, 0]})
        say("  PASS -> powering off in 5 s (dark lamp = passed, unplug it)")
        time.sleep(5)
        subprocess.run(["sync"], check=False)
        subprocess.run(["systemctl", "poweroff"], check=False)
    elif result != "PASS":
        # stays on, red: keep it and fetch its log
        if not led("/led/status", {"state": "error"}):
            led("/led/solid", {"color": [80, 0, 0]})
        say("  FAIL -> lamp stays on with a red ring; fetch the log (--watch-fetch)")
    return 0 if not events else 1


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--name", default=os.uname().nodename, help="lamp id for the report (hotspot name)")
    ap.add_argument("--seconds", type=int, default=10, help="idle noise measurement")
    ap.add_argument("--touch-timeout", type=int, default=90)
    ap.add_argument("--passes", type=int, default=3, help="swipes: every pad must register this many times")
    ap.add_argument("--soak", type=int, default=60, help="hands-off phantom check after saving")
    ap.add_argument("--backend", choices=("auto", "api", "direct"), default="direct",
                    help="direct = stop hal while calibrating (default); auto = api when HAL has it")
    ap.add_argument("--out", default=None, help="report dir (default ~SUDO_USER/hwtest/mpr121)")
    ap.add_argument("--color", default="", help="lamp colour/material, kept in the report and the factory file")
    ap.add_argument("--poweroff-on-pass", action="store_true", help="watch: power the lamp off when it passes")
    ap.add_argument("--watch", type=int, default=0, metavar="SECONDS",
                    help="only watch for phantom touches this long, hal running (after calibration)")
    args = ap.parse_args()
    if os.geteuid() != 0:
        sys.exit("run as root (sudo)")
    owner_home = pwd.getpwnam(os.environ.get("SUDO_USER", "root")).pw_dir
    out_dir = args.out or os.path.join(owner_home, "hwtest", "mpr121")
    if args.watch:
        sys.exit(watch(args.name, args.watch, out_dir, args.poweroff_on_pass))

    say(f"=== MPR121 factory calibration: {args.name} ===")
    config = ApiBackend.probe() if args.backend in ("auto", "api") else None
    if config is None and args.backend == "api":
        sys.exit("HAL touch API not answering on :5001")
    backup = Backup()
    backend = ApiBackend(config) if config is not None else DirectBackend()
    say(f"  backend: {backend.name}" + (" (hal stopped while calibrating)" if backend.name == "direct" else ""))
    result = {"result": "FAIL", "saved": False}
    try:
        with Keys() as keys:
            while True:
                noise = measure_noise(backend, args.seconds, keys)
                touch, release = suggest(noise)
                say("  idle noise per pad and suggested thresholds:")
                table(backend.order, [("noise", noise), ("touch", touch), ("release", release)])
                backend.apply(touch, release)
                best, missing, weak = touch_check(backend, touch, keys, args.touch_timeout, args.passes)
                answer = None
                while True:
                    table(backend.order, [("touch", touch), ("release", release), ("best", best)])
                    if missing:
                        say(f"  NOT registered: pads {missing}")
                    if weak:
                        say(f"  weak (best < touch + {WEAK_MARGIN}): pads {weak}")
                    say("  y = save, n = redo, c = check touches again, t 8 / t 3 10 / r 4 = adjust")
                    answer = prompt(keys, "Save this as the factory calibration?")
                    if answer in ("y", "n"):
                        break
                    if answer == "c":
                        best, missing, weak = touch_check(backend, touch, keys, args.touch_timeout, args.passes)
                        continue
                    error = adjust(answer, touch, release)
                    if error:
                        say(f"  {error}")
                    else:
                        backend.apply(touch, release)
                        weak = [p for p in backend.order if p not in missing and best[p] < touch[p] + WEAK_MARGIN]
                if answer == "n":
                    continue
                write_json(FACTORY_PATH, {
                    "touch_thresholds": touch, "release_thresholds": release,
                    "calibrated": {"touch": touch, "release": release}, "noise": noise,
                    "source": "bench", "updated_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
                    "note": f"{args.name} color={args.color or '?'} best={best}",
                })
                if os.path.exists(USER_PATH):
                    os.remove(USER_PATH)             # a fresh unit starts from the factory tuning
                backend.saved()
                say(f"  saved {FACTORY_PATH}")
                phantom = soak(backend, args.soak, keys)
                result = {"result": "PASS" if not phantom and not missing else "FAIL", "saved": True, "color": args.color,
                          "noise": noise, "touch": touch, "release": release, "best": best,
                          "missing": missing, "weak": weak, "phantom": phantom}
                if phantom:
                    say(f"  PHANTOM touches while nobody touched: { {p: n for p, n in phantom.items()} }")
                    if prompt(keys, "Redo the calibration?") == "y":
                        continue
                break
    except (KeyboardInterrupt, Exception) as exc:
        if not isinstance(exc, KeyboardInterrupt):
            say(f"\n  ERROR: {exc}")
        say("\n  stopped - previous calibration files put back")
        backup.restore()
        try:
            backend.saved()
        except Exception:
            pass
        result = {"result": "ABORTED", "saved": False}
    finally:
        backend.finish()
    path = report(args.name, backend, result, out_dir)
    say(f"\nreport : {path}")
    say(f"RESULT : {args.name} MPR121 {result['result']}")
    sys.exit(0 if result["result"] == "PASS" else 1)


if __name__ == "__main__":
    main()
